//! Replay the real event dispatcher without loading the native engine.
use super::*;
use crate::device::native::PanelStream;

pub(super) struct Panel {
    pub(super) app: App,
    pub(super) painter: Painter,
    pub(super) stream: PanelStream,
}
impl Panel {
    pub(super) fn new() -> Self {
        let mut panel = Self::cold();
        // Ordinary journeys start after the encoder's initial count report.
        panel.events(&[(5, 0)]);
        panel
    }
    pub(super) fn cold() -> Self {
        let mut app = App::new();
        app.panel_mode = true;
        let mut painter = Painter::new().unwrap();
        app.render(&mut painter);
        Self {
            app,
            painter,
            stream: PanelStream::default(),
        }
    }
    pub(super) fn events(&mut self, bytes: &[(u8, u8)]) {
        for (action, value) in bytes {
            self.stream.push(&[*action, *value]);
        }
        self.app
            .dispatch_panel_events(&mut self.painter, self.stream.drain());
        self.app.render(&mut self.painter);
        if self.app.files_ui.pending.is_some() {
            // As the frame loop does: the frame naming the bulk operation is
            // drawn, then the operation runs.
            self.app.run_pending();
            self.app.render(&mut self.painter);
        }
    }
}

#[test]
fn lfo_dest_feedback_survives_menus_and_rebases_on_recall() {
    let mut p = Panel::new();
    p.app.patch_mut().routes = [[0.; 11]; 9];
    p.events(&[(13, 0)]);
    for lfo in 0..2 {
        // The view after a native poll: only the destination changed; Amount
        // is still zero. This substitutes readback, not the hardware button.
        p.app.patch_mut().lfo_destinations[lfo] = Some(6);
        p.events(&[(16, 1)]);
        assert_eq!(p.app.readout_target(), Some(LiveTarget::Route(lfo, 6)));
        let last = p.app.touch.last;
        p.events(&[(16, 1), (9, 0)]);
        assert_eq!(
            p.app.touch.last, last,
            "streamed updates do not re-trigger DEST"
        );
        if let Some(captures) = std::env::var_os("GR1_UI_TEST_CAPTURE_DIR") {
            p.painter
                .png(
                    &std::path::PathBuf::from(captures)
                        .join(format!("lfo{}-dest-menu.png", lfo + 1)),
                )
                .unwrap();
        }
        let (target, at) = p.app.touch.last.unwrap();
        p.app.touch.last = Some((target, at - Duration::from_secs(2)));
        p.app.patch_mut().routes[lfo][6] = -0.4;
        p.events(&[(16, 1)]);
        assert_eq!(p.app.readout_target(), Some(LiveTarget::Route(lfo, 6)));
        assert!(
            p.app.touch.last.unwrap().1 > at,
            "Amount renews the readout"
        );
        let last = p.app.touch.last;
        p.events(&[(16, 1), (9, 0)]);
        assert_eq!(p.app.touch.last, last, "steady Amount does not retrigger");
    }
    p.events(&[(14, 0), (14, 0)]);
    assert_eq!(p.app.tab, 0);
    assert_eq!(p.app.patch().lfo_destinations, [Some(6), Some(6)]);
    // Recall of the same named preset must not be reported as a DEST press.
    for event in [10, 27] {
        p.app.patch_mut().lfo_destinations[0] = Some(event as usize % 11);
        p.app.patch_mut().routes[1][6] += 0.1;
        p.events(&[(event, 0), (16, 1)]);
        assert!(p.app.readout_target().is_none());
    }
    p.app.parts[0] = 2;
    p.events(&[]);
    assert!(
        p.app.readout_target().is_none(),
        "a different preset re-bases too"
    );
}

#[test]
fn window_and_lfo_base_readback_show_the_moved_control() {
    for menu in [false, true] {
        let mut p = Panel::new();
        if menu {
            p.events(&[(13, 0)]);
        }
        for (id, value, event) in [
            ("sides", 75., 9),
            ("tilt", 25., 9),
            ("curve", 65., 9),
            ("lfo1_rate", 2.5, 11),
            ("lfo2_rate", 3.5, 11),
            ("lfo1_wave", 2., 11),
            ("lfo2_wave", 3., 11),
        ] {
            let i = p.app.setting_index(id);
            p.app.set(i, value);
            p.events(&[(event, 0)]);
            assert_eq!(p.app.readout_target(), Some(LiveTarget::Setting(i)), "{id}");
            let last = p.app.touch.last;
            p.events(&[(event, 0)]);
            assert_eq!(p.app.touch.last, last, "steady {id} does not retrigger");
            if let Some(captures) = std::env::var_os("GR1_UI_TEST_CAPTURE_DIR") {
                p.painter
                    .png(
                        &std::path::PathBuf::from(captures)
                            .join(format!("{id}-{}.png", if menu { "menu" } else { "main" })),
                    )
                    .unwrap();
            }
            p.app.set(i, value + 1.);
            p.events(&[(10, 0)]);
            assert!(p.app.readout_target().is_none(), "recall re-bases {id}");
            if menu {
                p.events(&[(13, 0)]);
            }
        }
    }
}

#[test]
fn first_encoder_report_after_restart_does_not_jump_the_menu() {
    // The device's retained MCU count was 65460 (-76), with inversion on.
    // A fresh native engine subtracts zero and emits +76 on the first turn.
    let mut p = Panel::cold();
    p.events(&[(13, 0), (5, 76)]);
    let items = p.app.page_items(patch_root::PLAY);
    assert_eq!(p.app.patch_active(), items[0]);
    p.events(&[(5, 1)]);
    assert_eq!(p.app.patch_active(), items[1]);
    // The same rule in the Perf root.
    let mut p = Panel::cold();
    p.events(&[(12, 0), (5, 76)]);
    assert_eq!(p.app.perf.slots.focus, 0);
    p.events(&[(5, 1)]);
    assert_eq!(p.app.perf.slots.focus, 1);
}

#[test]
fn encoder_synchronization_on_sound_does_not_swallow_later_menu_turns() {
    let mut p = Panel::cold();
    p.events(&[(5, 180), (12, 0), (5, 1)]);
    assert_eq!(p.app.tab, 4, "Shift+Perf opens the Perf root");
    assert_eq!(p.app.perf.slots.focus, 1, "the turn after it moves focus");
}

#[test]
fn panel_batch_can_grab_keep_choose_continue_and_return_to_the_strip() {
    let mut p = Panel::new();
    // A complete open/turn/grab/turn/keep sequence in one native poll batch.
    p.events(&[(13, 0), (5, 2), (7, 0), (5, 1), (7, 0)]);
    assert_eq!(p.app.get("patch_volume"), 81.);
    assert!(p.app.live.is_none());
    // A choice in the same batch: grab, turn, keep; then continue turning.
    p.events(&[(5, 254), (7, 0), (5, 1), (7, 0), (5, 3)]);
    assert_eq!(p.app.get("sample_mode"), 1.);
    let spawn = patch_root::Item::Setting(p.app.setting_index("spawn"));
    assert_eq!(p.app.patch_active(), spawn);
    p.events(&[(14, 0)]);
    assert!(p.app.patch_ui.on_strip);
    p.events(&[(14, 0), (5, 1), (7, 0)]);
    assert_eq!(p.app.tab, 0);
    assert!(
        p.app.live.is_none(),
        "Main Select belongs to the native engine"
    );
}

#[test]
fn panel_reaches_last_menu_group_and_restores_its_scroll_position() {
    let mut p = Panel::new();
    // The Performances list: a long turn reaches the last slot, which stays
    // visible; the strip and back keep the row and the view.
    p.events(&[(12, 0), (5, 127)]);
    let cursor = p.app.perf.slots;
    assert_eq!(cursor.focus, 49);
    assert!(cursor.focus < cursor.top + components::VISIBLE_ROWS);
    p.events(&[(14, 0), (7, 0)]);
    assert_eq!(p.app.perf.slots, cursor);
    p.events(&[(14, 0), (14, 0)]);
    assert_eq!(p.app.tab, 0);
    p.events(&[(12, 0)]);
    assert_eq!(p.app.perf.slots, cursor);
}

#[test]
fn datatype_buttons_do_not_leave_the_active_menu_and_clear_uses_its_payload() {
    let mut p = Panel::new();
    p.events(&[(12, 0), (5, 3), (33, 0), (34, 0), (35, 0)]);
    assert_eq!(p.app.tab, 4);
    let focus = p.app.perf_context();
    p.events(&[(29, 0)]);
    match p.app.perf.layers.last() {
        Some(perf_root::Layer::Ask(a)) => assert_eq!(a.lines[0], "Clear the sample of 01"),
        _ => panic!("Shift+Load asks to clear the lit type's object"),
    }
    p.events(&[(14, 0)]);
    assert_eq!(p.app.perf_context(), focus);
}

#[test]
fn waveform_drag_uses_the_new_full_width_geometry() {
    let mut p = Panel::new();
    p.app.pointer(272, 300, true);
    assert!((p.app.patch().params[0] - 50.).abs() < 0.2);
    p.app.pointer(543, 300, false);
    assert_eq!(p.app.patch().params[0], 100.);
}

#[test]
fn numeric_edit_cancel_preserves_its_origin() {
    // A MIDI channel, on the Parts page: grabbed, turned, Escape restores.
    let mut p = Panel::new();
    p.events(&[(12, 0), (12, 0), (12, 0), (5, 1), (7, 0), (5, 1), (7, 0)]);
    let original = p.app.get("midi1");
    p.events(&[(5, 2)]);
    assert_ne!(p.app.get("midi1"), original);
    p.events(&[(14, 0)]);
    assert_eq!(p.app.get("midi1"), original);
    assert_eq!(p.app.perf.inside, Some(3), "focus stays on the channel");
}

#[test]
fn part_actions_have_short_return_paths() {
    let mut p = Panel::new();
    // The Parts page: part 3's row, Level grabbed, one detent, then Escape
    // restores it and a second Escape leaves the row on the same part.
    p.events(&[(12, 0), (12, 0), (12, 0), (5, 3), (7, 0)]);
    assert_eq!(p.app.perf.inside, Some(2));
    let volume = p.app.volumes[2];
    p.events(&[(7, 0), (5, 255)]);
    assert_eq!(p.app.volumes[2], volume - 1.);
    p.events(&[(14, 0)]);
    assert_eq!(p.app.volumes[2], volume);
    p.events(&[(14, 0)]);
    assert_eq!((p.app.perf.part_row, p.app.perf.inside), (3, None));
}

#[test]
fn main_screen_keeps_engine_select_and_answers_readback_and_notices() {
    let mut p = Panel::new();
    assert_eq!(p.app.tab, 0);
    // Main-screen Select and Escape stay with the engine; no GUI edit starts.
    p.events(&[(5, 3), (7, 0), (14, 0), (6, 0)]);
    assert_eq!(p.app.tab, 0);
    assert!(p.app.live.is_none() && p.app.modal.is_none());
    assert!(p.app.readout_target().is_none());
    // The engine names the moved control (event 21, text with its C NUL):
    // the readout shows it and no notice repeats it.
    let named = |text: &str| {
        let mut bytes = vec![21, text.len() as u8 + 1];
        bytes.extend_from_slice(text.as_bytes());
        bytes.push(0);
        bytes
    };
    p.stream.push(&named("Spray: 00m0.120s"));
    p.events(&[]);
    assert_eq!(p.app.readout_target(), Some(LiveTarget::Param(2, false)));
    assert!(p.app.toast.is_empty());
    // A text the readout cannot map stays a notice, without its NUL.
    p.stream.push(&named("LFO REC ON"));
    p.events(&[]);
    assert_eq!(p.app.toast, "LFO REC ON");
    assert_eq!(p.app.readout_target(), Some(LiveTarget::Param(2, false)));
    // A menu round trip (root, strip, main) leaves the main screen as it was.
    p.events(&[(13, 0), (14, 0), (14, 0)]);
    assert_eq!(p.app.tab, 0);
    assert!(p.app.live.is_none());
}

#[test]
fn panel_trace_logs_every_event_with_the_screen_it_arrived_under() {
    let mut p = Panel::new();
    p.app.panel_trace = true;
    p.stream.push(&[21, 4]);
    p.stream.push(b"Size");
    // One poll: every event was emitted under the screen set before the poll,
    // even after the first one opens a menu.
    p.events(&[(13, 0), (5, 255), (24, 60)]);
    p.events(&[(14, 0)]);
    let lines = &p.app.panel_trace_lines;
    assert_eq!(lines.len(), 5, "{lines:#?}");
    let expect = [
        "event=21 CONTROL_NAME value=4 text=\"Size\" screen=0 ",
        "event=13 PATCH_MENU value=0 screen=0 ",
        "event=5 ENC_ROT value=255 delta=-1 screen=0 ",
        "event=24 NOTEON value=60 screen=0 ",
        "event=14 ESC value=0 screen=4 ",
    ];
    for (line, expected) in lines.iter().zip(expect) {
        assert!(line.starts_with("[gr1-panel] t="), "{line}");
        assert!(line.contains(expected), "{line} lacks {expected}");
        assert!(
            line.contains(" | ctx=") && line.contains(" -> ctx="),
            "{line}"
        );
    }
    assert!(lines[1].ends_with("next_screen=4"), "{}", lines[1]);
    assert!(lines[0].contains("batch=1 seq=1 ") && lines[4].contains("batch=2 seq=5 "));
}

#[test]
fn panel_trace_prints_streaming_events_once_per_run_with_their_counts() {
    let mut p = Panel::new();
    p.app.panel_trace = true;
    p.events(&[
        (9, 0),
        (9, 0),
        (16, 1),
        (9, 0),
        (16, 0),
        (9, 0),
        (7, 0),
        (9, 0),
    ]);
    let lines = &p.app.panel_trace_lines;
    assert_eq!(lines.len(), 5, "{lines:#?}");
    assert!(lines[0].contains("event=9 WIN_NOTIFY"));
    assert!(lines[1].contains("event=16 UPDATE_MOD value=1"));
    assert!(
        lines[2].ends_with("repeated 9 WIN_NOTIFY x3, 16 UPDATE_MOD x1"),
        "{}",
        lines[2]
    );
    assert!(lines[3].contains("seq=7 event=7 ENC_PUSH"), "{}", lines[3]);
    assert!(
        lines[4].contains("seq=8 event=9 WIN_NOTIFY"),
        "{}",
        lines[4]
    );
}

#[test]
fn real_file_load_notification_bursts_do_not_repeat_completed_layouts() {
    use super::overlay::{DataKind, Layer, Place};
    use crate::device::{listing::ListingJob, storage::Document};
    use std::fs;
    let root = std::env::temp_dir().join(format!("gr1-load-burst-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    let _scratch = Scratch(root.clone());
    let mut wav = vec![];
    wav.extend(b"RIFF");
    wav.extend(8036u32.to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(8000u32.to_le_bytes());
    wav.extend(16000u32.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(8000u32.to_le_bytes());
    wav.resize(8044, 0);
    for i in 0..2 {
        fs::write(root.join(format!("sample-{i}.wav")), &wav).unwrap();
        Document::defaults("patch")
            .save(&root.join(format!("patch-{i}.grp")), "gr-1 patch")
            .unwrap();
        crate::device::storage::Performance::default()
            .document()
            .save(&root.join(format!("performance-{i}.grm")), "performance")
            .unwrap();
    }
    for kind in [DataKind::Sample, DataKind::Patch, DataKind::Perf] {
        let mut p = Panel::new();
        p.app.open_browser(kind);
        if let Some(Layer::Browser(b)) = p.app.files_ui.layers.last_mut() {
            b.real_files = true;
        }
        p.app.begin_listing(
            ListingJob::start(root.clone(), vec![root.clone()]),
            super::listing::Target::Browser {
                place: Place::Folder(root.to_string_lossy().into_owned()),
                focus: None,
            },
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while p.app.files_ui.listing.is_some() && Instant::now() < deadline {
            p.app.listing_frame();
            std::thread::yield_now();
        }
        assert!(p.app.files_ui.listing.is_none());
        p.app.render(&mut p.painter);
        assert_eq!(p.app.screen_id(), 1);
        for count in [16, 32, 3926] {
            let before = p.app.render_passes;
            let events = (0..count)
                .map(|_| crate::device::native::PanelEvent {
                    action: 9,
                    value: 0,
                    text: None,
                })
                .collect();
            let started = Instant::now();
            p.app.dispatch_panel_events(&mut p.painter, events);
            let elapsed = started.elapsed();
            let passes = p.app.render_passes - before;
            println!("Load {:?}, actual folder {:?}, {} WIN_NOTIFY events: {} render passes, {:?}, dirty={}, context={}", kind, root, count, passes, elapsed, p.app.navigation_dirty, p.app.focus_context());
            assert_eq!(
                passes, 0,
                "streaming events do not redraw a completed Load layout"
            );
            assert!(!p.app.navigation_dirty);
        }
        let before = match p.app.files_ui.layers.last() {
            Some(Layer::Browser(b)) => b.list.focus,
            _ => panic!(),
        };
        let sample = p.app.patch().sample.clone();
        let passes = p.app.render_passes;
        p.events(&[(5, 1)]);
        assert_eq!(
            p.app.render_passes - passes,
            1,
            "the encoder move draws its new focus once"
        );
        match p.app.files_ui.layers.last() {
            Some(Layer::Browser(b)) => assert_eq!(b.list.focus, before + 1),
            _ => panic!(),
        }
        assert_eq!(p.app.patch().sample, sample, "scrolling still never loads");
    }
}

#[test]
fn u2_layout_completion_keeps_save_name_and_confirmation_transitions() {
    use super::overlay::{DataKind, Layer, SaveField};
    fn settled(p: &mut Panel) {
        assert!(!p.app.navigation_dirty);
        assert_eq!(p.app.rendered_context, p.app.focus_context());
        let before = p.app.render_passes;
        let events = (0..32)
            .map(|_| crate::device::native::PanelEvent {
                action: 9,
                value: 0,
                text: None,
            })
            .collect();
        p.app.dispatch_panel_events(&mut p.painter, events);
        assert_eq!(
            p.app.render_passes, before,
            "notifications do not repeat any completed U2 layout"
        );
    }
    for root in [13, 12, 15] {
        let mut p = Panel::new();
        p.events(&[(root, 0), (33, 0)]);
        if root == 12 {
            // Perf-root Load owns its slot action. Invoke the shared overlay
            // entry directly to check this underlay's rendering lifecycle.
            p.app.open_browser(DataKind::Sample);
            p.app.render(&mut p.painter);
        } else {
            p.events(&[(1, 0)]);
        }
        let sample = p.app.patch().sample.clone();
        settled(&mut p);
        assert!(matches!(
            p.app.files_ui.layers.last(),
            Some(Layer::Browser(_))
        ));
        // Each type button switches the same browser, with one completed draw.
        for kind in [34, 35, 33] {
            p.events(&[(kind, 0)]);
            settled(&mut p);
        }
        p.events(&[(2, 0)]);
        settled(&mut p);
        assert!(
            matches!(p.app.files_ui.layers.last(), Some(Layer::Save(s)) if s.kind == DataKind::Sample && s.focus == SaveField::Save)
        );
        let original = match p.app.files_ui.layers.last() {
            Some(Layer::Save(s)) => s.name.clone(),
            _ => panic!(),
        };
        // Save -> Name; text input draws once, then notifications stay cheap.
        p.events(&[(5, 254), (7, 0)]);
        settled(&mut p);
        assert!(p.app.name_text(" X"));
        let before = p.app.render_passes;
        p.events(&[(9, 0)]);
        assert_eq!(
            p.app.render_passes - before,
            2,
            "one layout before dispatch, one ordinary frame after it"
        );
        settled(&mut p);
        assert!(
            matches!(p.app.files_ui.layers.last(), Some(Layer::Name(n)) if n.chars.iter().collect::<String>().ends_with(" X"))
        );
        p.events(&[(14, 0)]);
        settled(&mut p);
        assert!(matches!(p.app.files_ui.layers.last(), Some(Layer::Save(s)) if s.name == original));
        // The existing sample still asks on Cancel. Turning chooses the verb;
        // Escape discards it and leaves the Save sheet, preserving the sample.
        p.events(&[(2, 0)]);
        settled(&mut p);
        assert!(matches!(p.app.files_ui.layers.last(), Some(Layer::Confirm(c)) if !c.on_verb));
        p.events(&[(5, 1)]);
        settled(&mut p);
        assert!(matches!(p.app.files_ui.layers.last(), Some(Layer::Confirm(c)) if c.on_verb));
        p.events(&[(14, 0)]);
        settled(&mut p);
        assert_eq!(p.app.patch().sample, sample);
        assert!(matches!(p.app.files_ui.layers.last(), Some(Layer::Save(_))));
        p.app.shift = true;
        p.events(&[(14, 0)]);
        p.app.shift = false;
        settled(&mut p);
        assert!(!p.app.overlay_active());
        assert_eq!(p.app.tab, 0);
    }
}
