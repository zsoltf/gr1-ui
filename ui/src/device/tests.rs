//! Local regression checks. None loads or substitutes for the native engine.
use super::{abi, bindings, native, storage::*};
use std::{fs, path::PathBuf};

#[test]
fn native_stencil_preserves_asymmetry_and_subpixel_amplitude() {
    let raw = [
        0.,
        512.,
        0.,
        768.,
        1.,
        511.99,
        1.,
        512.01,
        2.,
        f32::NAN,
        2.,
        f32::NAN,
    ];
    let points = super::display::stencil_columns(&raw, 1024);
    assert_eq!(points[0], (0.5, 0.75));
    assert!(points[1].0 < 0.5 && points[1].1 > 0.5);
    assert!(
        points[2].0.is_nan(),
        "Unavailable data must remain distinguishable from silence"
    );
}

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("gr1-{}-{name}", std::process::id()));
    fs::create_dir(&path).unwrap();
    path
}

#[test]
fn panel_stream_retains_split_utf8_and_following_events() {
    let mut bytes = vec![5, 255, 21, 3, 0xc3];
    assert_eq!(
        native::frames(&mut bytes),
        vec![native::PanelEvent {
            action: 5,
            value: 255,
            text: None
        }]
    );
    assert_eq!(bytes, [21, 3, 0xc3]);
    bytes.extend([0xa9, b'!', 7, 0]);
    let events = native::frames(&mut bytes);
    assert_eq!(events[0].text.as_deref(), Some("\u{e9}!"));
    assert_eq!(events[1].action, 7);
    assert!(bytes.is_empty());
}

#[test]
fn encoder_sync_survives_split_frames_and_preserves_subsequent_deltas() {
    for initial in [0, 1, 76, 128, 180, 255] {
        let mut stream = native::PanelStream::default();
        stream.push(&[13, 0, 5]);
        let events = stream.drain();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, 13);
        stream.push(&[initial, 5, 1, 7, 0, 5, 255, 5, 10]);
        let events = stream.drain();
        assert_eq!(
            events
                .iter()
                .map(|e| (e.action, e.value))
                .collect::<Vec<_>>(),
            [(5, 1), (7, 0), (5, 255), (5, 10)]
        );
    }
}

#[test]
fn all_visible_settings_have_a_native_or_operation_binding() {
    let defaults = Document::defaults("patch").ipc().unwrap();
    let system = Document::defaults("system");
    for s in crate::catalog::catalog() {
        if let Some(binding) = bindings::patch(&s.id) {
            assert!(binding.read(&defaults).unwrap().is_finite(), "{}", s.id);
            let mut state = defaults;
            assert!(binding.write(&mut state, s.default), "{}", s.id);
        } else if let Some((key, _)) = bindings::system(&s.id) {
            assert!(system.number(key).is_ok(), "{}", s.id);
        } else {
            assert!(
                s.id.starts_with("voice")
                    || ["normalize", "sample_size", "samba", "matrix_mode"]
                        .contains(&s.id.as_str()),
                "Unconnected setting: {}",
                s.id
            );
        }
    }
}

#[test]
fn musical_units_and_effect_ids_match_pinned_native_conversions() {
    let mut p = Document::defaults("patch").ipc().unwrap();
    bindings::patch("pitch").unwrap().write(&mut p, 12.);
    assert_eq!(p.sample_tune, 2.);
    bindings::patch("delay_time").unwrap().write(&mut p, 5000.);
    assert_eq!(p.delay_time, 5.);
    bindings::patch("reverb_time").unwrap().write(&mut p, 100.);
    assert!((p.reverb_time - f32::from_bits(0x3dbe37c6)).abs() < 1e-7);
    assert_eq!(abi::FXTYPE_NONE, 0);
    assert_eq!(abi::FXTYPE_REVERB, 4);
}

#[test]
fn legacy_matrix_is_reordered_by_names_and_unknown_keys_survive() {
    let mut doc = Document::defaults("patch");
    doc.put("matrix_rows", "CV2, LFO1");
    doc.put("matrix_cols", "AMP, TUNE");
    doc.put("mod_matrix", "0.25 -0.5 0.75 1");
    for key in ["lfo1dest", "lfo2dest", "cv1dest", "cv2dest"] {
        doc.put(key, 1);
    }
    doc.put("future_field", "opaque 100%");
    doc.put("grain_size", 0); // Shipped empty factory patches have zero size.
    let state = doc.ipc().unwrap();
    assert_eq!(state.mod_matrix[3 * 11 + 5], 0.25);
    assert_eq!(state.mod_matrix[0], 1.);
    assert_eq!(state.lfo1dest, 0);
    doc.update(&state);
    let parsed = Document::parse(&doc.render("gr-1 patch"), "gr-1 patch").unwrap();
    assert_eq!(parsed.text("future_field"), "opaque 100%");
    assert_eq!(state, parsed.ipc().unwrap());
}

#[test]
fn performance_literal_handles_quotes_booleans_and_nonexecuting_rejection() {
    let root = temp("performance");
    let path = root.join("performance.grm");
    let mut p = Performance::default();
    set_text(&mut p.multis[2].name, "Player's \\ \u{e9}").unwrap();
    p.multis[2].partenables = [1, 1, 0, 0];
    p.current = 2;
    p.document().save(&path, "performance").unwrap();
    let roundtrip = Performance::read(&path).unwrap();
    assert_eq!(roundtrip.multis, p.multis);
    assert_eq!(roundtrip.current, 2);
    let text = fs::read_to_string(&path).unwrap().replace(
        "\"partenables\":[1,1,0,0]",
        "\"partenables\":[True,True,False,False]",
    );
    fs::write(&path, text).unwrap();
    assert_eq!(Performance::read(&path).unwrap().multis, p.multis);
    fs::write(
        &path,
        "[performance]\nmulti_presets = __import__('os').system('false')\n",
    )
    .unwrap();
    assert!(Performance::read(&path).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn interrupted_save_retains_original_and_success_keeps_previous_directory() {
    let root = temp("save");
    let target = root.join("000");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("notes.txt"), "keep me").unwrap();
    let stage = super::staging(&target).unwrap();
    super::preserve_extras(&target, &stage, true).unwrap();
    fs::write(stage.join("new.grp"), "new").unwrap();
    assert_eq!(
        fs::read_to_string(target.join("notes.txt")).unwrap(),
        "keep me"
    );
    assert!(super::staging(&target).is_err());
    super::replace_directory(&stage, &target).unwrap();
    assert!(target.join("new.grp").is_file());
    assert!(target.join("notes.txt").is_file());
    assert!(root.join(".000.gr1-ui-previous/notes.txt").is_file());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lfo_window_is_two_periods_within_two_and_sixty_seconds() {
    use super::display::lfo_window;
    assert_eq!(lfo_window(10.), 2.);
    assert_eq!(lfo_window(0.5), 4.);
    assert_eq!(lfo_window(0.01), 60.);
    assert_eq!(lfo_window(0.), 60.);
    assert_eq!(lfo_window(f32::NAN), 2.);
}

#[test]
fn lfo_trace_keeps_one_bucket_per_column_and_merges_on_a_longer_window() {
    use super::display::{lfo_window, LfoTrace, LFO_COLUMNS};
    let mut trace = LfoTrace::default();
    let sine = |t: f64| (t * std::f64::consts::TAU).sin() as f32;
    // Two seconds of a 1 Hz oscillator observed at 60 Hz.
    for i in 0..120 {
        let t = i as f64 / 60.;
        trace.push(t, sine(t), lfo_window(1.));
    }
    let extremes = |trace: &LfoTrace| {
        trace
            .buckets()
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), b| {
                (lo.min(b.min), hi.max(b.max))
            })
    };
    let before = extremes(&trace);
    assert!(before.0 < -0.99 && before.1 > 0.99);
    // A slower rate widens the window: neighbours merge, history survives.
    trace.push(2., sine(2.), lfo_window(0.25));
    assert!(trace.bucket_seconds() * LFO_COLUMNS as f64 >= 8.);
    assert_eq!(extremes(&trace), (before.0.min(sine(2.)), before.1));
    // A minute of a slow oscillator never exceeds one bucket per column.
    for i in 0..3600 {
        let t = 2. + i as f64 / 60.;
        trace.push(t, sine(t * 0.05), lfo_window(0.05));
    }
    assert!(trace.buckets().len() <= LFO_COLUMNS);
    assert!(trace.bucket_seconds() * LFO_COLUMNS as f64 >= 40.);
    // Returning to a fast rate keeps the newest value and the column bound.
    trace.push(62., 0.5, lfo_window(5.));
    assert!(trace.buckets().len() <= LFO_COLUMNS);
    assert_eq!(trace.buckets().back().unwrap().last, 0.5);
    assert!(trace.bucket_seconds() * LFO_COLUMNS as f64 >= 2.);
}

/// A stock-format performance folder: performance.grm and 32 preset folders,
/// each with one patch and its (empty) sample file.
fn performance_fixture(folder: &std::path::Path, name: &str) {
    let mut perf = Performance::default();
    perf.document.put("name", name);
    perf.document()
        .save(&folder.join("performance.grm"), "performance")
        .unwrap();
    let mut patch = Document::defaults("patch");
    patch.put("sample", "empty.wav");
    for i in 0..32 {
        let preset = folder.join(format!("{i:03}"));
        fs::create_dir_all(&preset).unwrap();
        fs::write(preset.join("empty.wav"), b"").unwrap();
        patch.save(&preset.join("empty.grp"), "gr-1 patch").unwrap();
    }
}

#[test]
fn layout_prefers_user_perf_and_otherwise_detects_the_legacy_root() {
    use super::layout::{Kind, Layout};
    let current = temp("layout-user");
    fs::create_dir_all(current.join("user/perf")).unwrap();
    // A stray legacy folder does not override an existing user/perf.
    fs::create_dir(current.join("performance01")).unwrap();
    let layout = Layout::detect(&current);
    assert_eq!(layout.kind, Kind::User);
    assert_eq!(layout.slot(0), current.join("user/perf/performance01"));
    assert_eq!(layout.slot(49), current.join("user/perf/performance50"));
    assert_eq!(layout.internal(), current.join("user"));
    assert_eq!(layout.factory(), current.join("factory"));
    assert_eq!(
        layout.system_settings(),
        current.join("system_settings.ini")
    );
    assert_eq!(layout.appearance(), current.join("ui-appearance.json"));

    // An official firmware upgrade must not discard the user's UI preferences.
    let previous_appearance = current.join("gr1-ui-settings/appearance.json");
    fs::create_dir(previous_appearance.parent().unwrap()).unwrap();
    fs::write(&previous_appearance, br#"{"theme":"dark","brightness":25}"#).unwrap();
    assert_eq!(layout.appearance(), previous_appearance);
    let current_appearance = current.join("ui-appearance.json");
    fs::write(&current_appearance, br#"{"theme":"light","brightness":75}"#).unwrap();
    assert_eq!(layout.appearance(), current_appearance);

    let legacy = temp("layout-legacy");
    fs::create_dir(legacy.join("performance01")).unwrap();
    let layout = Layout::detect(&legacy);
    assert_eq!(layout.kind, Kind::Legacy);
    let slots = layout.slots();
    assert_eq!(slots.len(), 50);
    assert_eq!(slots[0], legacy.join("performance01"));
    assert_eq!(slots[49], legacy.join("performance50"));
    assert_eq!(layout.internal(), legacy);
    assert_eq!(layout.factory(), legacy.join("factory"));
    // Host files stay in the host's folder, never in or as a performance.
    let host = legacy.join("gr1-ui-settings");
    assert_eq!(layout.system_settings(), host.join("system_settings.ini"));
    assert_eq!(layout.appearance(), host.join("appearance.json"));
    assert!(slots.iter().all(|s| !host.starts_with(s)));

    // Neither layout present: the current layout, whose read names user/perf.
    let empty = temp("layout-empty");
    let layout = Layout::detect(&empty);
    assert_eq!(layout.kind, Kind::User);
    let Err(error) = super::read_performance(&layout.slot(0)) else {
        panic!("an empty root has no slot to read");
    };
    assert!(error.contains("user/perf/performance01"), "{error}");
    for root in [current, legacy, empty] {
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn legacy_root_reads_saves_and_lists_in_place_without_user_perf() {
    use super::layout::{Kind, Layout};
    let root = temp("layout-legacy-save");
    performance_fixture(&root.join("performance01"), "Stock one");
    performance_fixture(&root.join("factory/default2017"), "Factory");
    let layout = Layout::detect(&root);
    assert_eq!(layout.kind, Kind::Legacy);

    // Startup's read of the selected slot.
    let (perf, docs) = super::read_performance(&layout.slot(0)).unwrap();
    assert_eq!(perf.document.text("name"), "Stock one");
    assert_eq!(docs.len(), 32);

    // A save to user slot 2 goes through the same stage/replace path as
    // Device::save_performance, with the layout's slot as its destination.
    let dest = layout.slot(1);
    let stage = super::staging(&dest).unwrap();
    super::preserve_extras(&dest, &stage, false).unwrap();
    performance_fixture(&stage, "Saved two");
    super::replace_directory(&stage, &dest).unwrap();
    assert_eq!(dest, root.join("performance02"));
    let (saved, _) = super::read_performance(&dest).unwrap();
    assert_eq!(saved.document.text("name"), "Saved two");
    // Replacing slot 1 keeps its previous folder beside it, hidden.
    let dest = layout.slot(0);
    let stage = super::staging(&dest).unwrap();
    super::preserve_extras(&dest, &stage, false).unwrap();
    performance_fixture(&stage, "Saved one");
    super::replace_directory(&stage, &dest).unwrap();
    assert!(root
        .join(".performance01.gr1-ui-previous/performance.grm")
        .is_file());

    // Host settings land in the host folder only.
    Document::defaults("system")
        .save(&layout.system_settings(), "system")
        .unwrap();
    assert!(root.join("gr1-ui-settings/system_settings.ini").is_file());
    assert!(!root.join("system_settings.ini").exists());
    assert!(
        !root.join("user").exists(),
        "nothing is written to user/perf"
    );

    // Internal (the root itself) lists the slots with Factory beside them and
    // hides host staging; Factory lists its own content.
    assert_eq!(
        super::list_files(&layout.internal()).unwrap(),
        [
            "factory/",
            "gr1-ui-settings/",
            "performance01/",
            "performance02/"
        ]
    );
    assert_eq!(
        super::list_files(&layout.factory()).unwrap(),
        ["default2017/"]
    );
    let (factory, _) = super::read_performance(&layout.factory().join("default2017")).unwrap();
    assert_eq!(factory.document.text("name"), "Factory");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn sample_replacement_checks_trashes_and_writes_the_same_wav_target() {
    let root = temp("sample-wav-target");
    for name in [
        "take.ogg",
        "take.flac",
        "take.aiff",
        "take.wave",
        "take.WAV",
    ] {
        let directory = root.join(name);
        fs::create_dir(&directory).unwrap();
        let target_name = sample_file_name(name).unwrap();
        let target = directory.join(&target_name);
        fs::write(&target, b"previous WAV").unwrap();
        if name != target_name {
            fs::write(directory.join(name), b"import source").unwrap();
        }
        let result = save_sample_with(&directory, name, false, |_| {
            panic!("an unconfirmed replacement reached the writer")
        });
        assert_eq!(result.unwrap_err(), "File already exists");
        assert_eq!(fs::read(&target).unwrap(), b"previous WAV");
        // The native writer is substituted only at this final callback;
        // canonicalization, collision and recoverable trash are production IO.
        save_sample_with(&directory, name, true, |path| {
            assert_eq!(path, target);
            fs::write(path, b"new WAV fixture").map_err(|e| e.to_string())
        })
        .unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new WAV fixture");
        assert_eq!(
            fs::read(directory.join(".gr1-ui-trash").join(&target_name)).unwrap(),
            b"previous WAV"
        );
        if name != target_name {
            assert_eq!(fs::read(directory.join(name)).unwrap(), b"import source");
        }
        println!(
            "Sample target {name}: collision, recoverable trash and writer all use {target_name}"
        );
    }
    fs::remove_dir_all(root).unwrap();
}
