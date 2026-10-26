//! Renderer checks for the redesigned main screen. They draw desktop state
//! into the real framebuffer; device readback is represented only by the
//! values the view would hold after a sync. No native engine is loaded.
use super::*;

fn app() -> (App, Painter) {
    let mut app = App::new();
    app.panel_mode = true;
    app.appearance.theme = Theme::Dark;
    let mut painter = Painter::new().unwrap();
    app.render(&mut painter);
    (app, painter)
}

fn rgb(p: &Painter, color: Color) -> [u8; 3] {
    color.rgb(p.theme, p.brightness)
}

fn pixel(p: &Painter, x: i32, y: i32) -> [u8; 3] {
    let i = (y as usize * 800 + x as usize) * 4;
    [p.pixels[i], p.pixels[i + 1], p.pixels[i + 2]]
}

fn count(p: &Painter, r: Rect, color: Color) -> usize {
    let want = rgb(p, color);
    (r.1..r.1 + r.3)
        .flat_map(|y| (r.0..r.0 + r.2).map(move |x| (x, y)))
        .filter(|(x, y)| pixel(p, *x, *y) == want)
        .count()
}

const READOUT: Rect = Rect(296, 430, 290, 46);

#[test]
fn both_lfo_shapes_show_large_names_and_distinct_icons_then_expire() {
    for theme in [Theme::Dark, Theme::White] {
        for menu in [false, true] {
            let mut panel = super::panel_tests::Panel::new();
            panel.app.appearance.theme = theme;
            if menu {
                panel.events(&[(13, 0)]);
            }
            let (left, top, value_y, size, icon) = if menu {
                (244, 428, 448, 22, Rect(443, 449, 103, 27))
            } else {
                (296, 430, 446, 28, Rect(485, 447, 97, 27))
            };
            let text_area = Rect(left, top, 174, 46);
            let mut expected = Painter::new().unwrap();
            expected.theme = theme;
            expected.brightness = panel.app.appearance.brightness;
            for lfo in 0..2 {
                let index = panel.app.setting_index(&format!("lfo{}_wave", lfo + 1));
                let mut icons = Vec::new();
                // Start at AUX so Sine is also an actual change in readback.
                panel.app.set(index, 4.);
                panel.events(&[(11, 0)]);
                for (shape, name) in ["Sine", "Random", "Saw", "Square", "AUX"]
                    .iter()
                    .enumerate()
                {
                    panel.app.set(index, shape as f32);
                    panel.events(&[(11, 0)]);
                    assert_eq!(panel.app.readout_target(), Some(LiveTarget::Setting(index)));
                    expected.rect(text_area, BACKGROUND);
                    expected.text_in(
                        Face::Condensed,
                        left,
                        top,
                        16,
                        &format!("LFO {} shape", lfo + 1),
                        INK2,
                    );
                    expected.text_in(Face::Bold, left, value_y, size, name, AMBER);
                    assert!(
                        panel.painter.save(text_area) == expected.save(text_area),
                        "LFO {} {name} large name, menu={menu}",
                        lfo + 1
                    );
                    assert!(
                        count(&panel.painter, icon, AMBER) > 100,
                        "visible shape icon"
                    );
                    let pixels = panel.painter.save(icon);
                    assert!(!icons.contains(&pixels), "{name} has its own icon");
                    icons.push(pixels);
                    if let Some(captures) = std::env::var_os("GR1_UI_TEST_CAPTURE_DIR") {
                        panel
                            .painter
                            .png(&std::path::PathBuf::from(captures).join(format!(
                                "lfo{}-shape-{name}-{}-{}.png",
                                lfo + 1,
                                theme.name(),
                                if menu { "menu" } else { "main" }
                            )))
                            .unwrap();
                    }
                    let last = panel.app.touch.last;
                    panel.events(&[(11, 0)]);
                    assert_eq!(
                        panel.app.touch.last, last,
                        "steady shape does not retrigger"
                    );
                    let (target, at) = last.unwrap();
                    panel.app.touch.last = Some((target, at - Duration::from_secs(2)));
                    panel.events(&[]);
                    assert!(panel.app.readout_target().is_none());
                    assert_eq!(count(&panel.painter, icon, AMBER), 0);
                }
            }
        }
    }
}

#[test]
fn both_lfo_destinations_show_at_zero_immediately_and_remain_after_readout_expires() {
    for theme in [Theme::Dark, Theme::White] {
        let (mut app, mut p) = app();
        app.appearance.theme = theme;
        app.patch_mut().routes = [[0.; 11]; 9];
        let mut expected = Painter::new().unwrap();
        expected.theme = theme;
        expected.brightness = app.appearance.brightness;
        for lfo in 0..2 {
            for (dest, name) in catalog::DESTINATIONS.iter().enumerate() {
                app.patch_mut().lfo_destinations[lfo] = Some((dest + 1) % 11);
                app.render(&mut p);
                app.patch_mut().lfo_destinations[lfo] = Some(dest);
                app.render(&mut p);
                assert_eq!(app.readout_target(), Some(LiveTarget::Route(lfo, dest)));
                let label = Rect(560, 285 + lfo as i32 * 88, 232, 18);
                expected.rect(label, BACKGROUND);
                expected.text_in(
                    Face::Condensed,
                    label.0,
                    label.1,
                    14,
                    &format!("{name} +0%"),
                    AMBER,
                );
                assert_eq!(
                    p.save(label),
                    expected.save(label),
                    "LFO {} {name}",
                    lfo + 1
                );
                let readout_name = Rect(296, 430, 284, 16);
                expected.rect(readout_name, BACKGROUND);
                expected.text_in(
                    Face::Condensed,
                    296,
                    430,
                    16,
                    &format!("LFO {} > {name}", lfo + 1),
                    INK2,
                );
                assert_eq!(p.save(readout_name), expected.save(readout_name));
                assert!(count(&p, READOUT, AMBER) > 100, "zero amount is visible");
                if dest == 7 {
                    if let Some(captures) = std::env::var_os("GR1_UI_TEST_CAPTURE_DIR") {
                        p.png(&std::path::PathBuf::from(captures).join(format!(
                            "lfo{}-dest-zero-{}.png",
                            lfo + 1,
                            theme.name()
                        )))
                        .unwrap();
                    }
                }
                let (target, at) = app.touch.last.unwrap();
                app.touch.last = Some((target, at - Duration::from_secs(2)));
                app.render(&mut p);
                assert!(app.readout_target().is_none());
                expected.rect(label, BACKGROUND);
                expected.text_in(
                    Face::Condensed,
                    label.0,
                    label.1,
                    14,
                    &format!("{name} +0%"),
                    BLUE,
                );
                assert_eq!(
                    p.save(label),
                    expected.save(label),
                    "selection stays visible"
                );
                assert_eq!(
                    app.patch().routes,
                    [[0.; 11]; 9],
                    "display never changes routing"
                );
            }
        }
    }
}

#[test]
fn both_lfo_amounts_reopen_and_refresh_the_readout_without_a_dest_press() {
    for theme in [Theme::Dark, Theme::White] {
        for lfo in 0..2 {
            let (mut app, mut p) = app();
            app.appearance.theme = theme;
            app.patch_mut().routes = [[0.; 11]; 9];
            app.patch_mut().lfo_destinations = [Some(7); 2];
            app.touch = status::Touch::default();
            app.render(&mut p);
            assert!(app.readout_target().is_none());
            // Only Amount changes in the native readback. DEST is unchanged.
            for (amount, text) in [(-0.25, "-25%"), (0., "+0%"), (0.5, "+50%")] {
                app.patch_mut().routes[lfo][7] = amount;
                app.render(&mut p);
                assert_eq!(app.readout_target(), Some(LiveTarget::Route(lfo, 7)));
                let mut expected = Painter::new().unwrap();
                expected.theme = theme;
                expected.brightness = app.appearance.brightness;
                let value_area = Rect(296, 446, 130, 28);
                expected.rect(value_area, BACKGROUND);
                expected.text_in(Face::Bold, 296, 446, 28, text, AMBER);
                let amber = rgb(&p, AMBER);
                let mask = |pixels: Vec<u8>| {
                    pixels
                        .chunks_exact(4)
                        .map(|px| px[..3] == amber)
                        .collect::<Vec<_>>()
                };
                assert!(
                    mask(p.save(value_area)) == mask(expected.save(value_area)),
                    "the large readout shows {text}"
                );
                assert!(count(&p, READOUT, AMBER) > 100, "large amount is visible");
                let last = app.touch.last;
                app.render(&mut p);
                assert_eq!(
                    app.touch.last, last,
                    "unchanged readback does not retrigger"
                );
                if amount == 0.5 {
                    if let Some(captures) = std::env::var_os("GR1_UI_TEST_CAPTURE_DIR") {
                        p.png(&std::path::PathBuf::from(captures).join(format!(
                            "lfo{}-amount-{}.png",
                            lfo + 1,
                            theme.name()
                        )))
                        .unwrap();
                    }
                }
                let (target, at) = app.touch.last.unwrap();
                app.touch.last = Some((target, at - Duration::from_secs(2)));
                app.render(&mut p);
                assert!(app.readout_target().is_none());
                assert_eq!(count(&p, READOUT, AMBER), 0);
            }
            app.patch_mut().routes[lfo][1] = 0.3;
            app.render(&mut p);
            assert!(
                app.readout_target().is_none(),
                "other routes are not Amount"
            );
        }
    }
}

#[test]
fn spotlight_brightens_only_the_spray_range() {
    let (app, p) = app();
    // Demo patch 1: scan 0.42 with spray radius 0.18, loop 0..100.
    assert_eq!(app.main_rows(), vec![0]);
    let inside = Rect(200, 0, 1, 424);
    let outside = Rect(450, 0, 1, 424);
    assert!(
        count(&p, inside, SPOTLIGHT) > 50,
        "spray columns are spotlit"
    );
    assert_eq!(count(&p, inside, WAVEFORM), 0);
    assert!(count(&p, outside, WAVEFORM) > 50, "the rest stays dim");
    assert_eq!(count(&p, outside, SPOTLIGHT), 0);
    assert!(
        count(&p, Rect(0, 414, 544, 10), BLUE) > 30,
        "Position marker"
    );
}

#[test]
fn readout_answers_a_readback_change_then_clears() {
    let (mut app, mut p) = app();
    assert!(app.readout_target().is_none());
    assert_eq!(count(&p, READOUT, AMBER), 0);
    // A Grain Size knob move reaches the view as a changed readback value.
    app.patch_mut().params[1] = 240.;
    app.render(&mut p);
    assert_eq!(app.readout_target(), Some(LiveTarget::Param(1, false)));
    assert!(count(&p, READOUT, AMBER) > 200, "28 px amber value and bar");
    assert!(
        count(&p, Rect(226, 196, 22, 22), AMBER) > 20,
        "grain-size bracket at the playhead"
    );
    let at = app.touch.last.unwrap().1;
    app.touch.last = Some((LiveTarget::Param(1, false), at - Duration::from_secs(2)));
    app.render(&mut p);
    assert!(app.readout_target().is_none());
    assert_eq!(count(&p, READOUT, AMBER), 0);
}

#[test]
fn recall_and_preset_changes_do_not_count_as_touches() {
    let (mut app, mut p) = app();
    for i in 0..5 {
        app.patch_mut().params[i] += 1.;
    }
    app.render(&mut p);
    assert!(
        app.readout_target().is_none(),
        "five values at once is a recall"
    );
    app.parts[0] = 2;
    app.render(&mut p);
    assert!(app.readout_target().is_none(), "another preset re-bases");
    app.patch_mut().params[1] = 400.;
    app.render(&mut p);
    assert_eq!(app.readout_target(), Some(LiveTarget::Param(1, false)));
}

#[test]
fn several_enabled_parts_draw_one_row_each_with_the_focus_bar() {
    let (mut app, mut p) = app();
    app.enabled = [true, true, false, true];
    app.part = 1;
    app.render(&mut p);
    assert_eq!(app.main_rows(), vec![0, 1, 3]);
    // Rows of 138 px with 5 px gaps; the focused second row has the bar.
    assert_eq!(pixel(&p, 1, 210), rgb(&p, BLUE));
    assert_ne!(pixel(&p, 1, 70), rgb(&p, BLUE));
    assert_ne!(pixel(&p, 1, 355), rgb(&p, BLUE));
    assert_eq!(pixel(&p, 300, 140), rgb(&p, BACKGROUND), "gap between rows");
}

#[test]
fn dsp_bar_appears_only_with_metering_on() {
    let (mut app, mut p) = app();
    assert_eq!(count(&p, Rect(0, 425, 300, 3), BLUE), 0);
    let dsp = app.setting_index("dsp");
    app.set(dsp, 1.);
    app.render(&mut p);
    assert!(count(&p, Rect(0, 425, 300, 3), BLUE) > 800);
}

/// Relative luminance per WCAG.
fn luminance([r, g, b]: [u8; 3]) -> f32 {
    let channel = |c: u8| {
        let c = c as f32 / 255.;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

#[test]
fn dark_large_fills_stay_below_the_brightness_rule() {
    // Redesign 5.5: no area larger than 2% of the screen above 0.20 in Dark.
    for color in [
        BACKGROUND,
        WELL,
        RAISED,
        BLUE_WASH,
        WAVEFORM,
        SPOTLIGHT,
        OUTSIDE_LOOP,
        SPRAY_BAND,
        CURVE_FILL,
        WAVEFORM_DIM,
        SPOTLIGHT_DIM,
    ] {
        assert!(luminance(color.rgb(Theme::Dark, 100)) <= 0.2);
    }
}

#[test]
fn modulated_readback_changing_every_frame_never_triggers_the_readout() {
    let (mut app, mut p) = app();
    let cutoff = app.setting_index("cutoff");
    for frame in 0..30 {
        // Untracked changing readback: filter, spray, scan and part levels.
        // Window base controls are operator state; the modulated window is
        // fetched separately through gr_fetch_grainwin, not those settings.
        let v = (frame as f32 * 0.7).sin();
        app.set(cutoff, 4000. + 1000. * v);
        app.patch_mut().params[2] = 20. + 5. * v;
        app.patch_mut().params[4] = v;
        app.volumes[0] = 80. + 5. * v;
        app.render(&mut p);
        assert!(app.readout_target().is_none(), "frame {frame}");
    }
    assert_eq!(count(&p, READOUT, AMBER), 0);
}

#[test]
fn engine_control_names_map_to_their_controls_or_to_nothing() {
    let (app, _) = app();
    let setting = |id: &str| Some(LiveTarget::Setting(app.setting_index(id)));
    let cases = [
        ("Density: 12.50/s", Some(LiveTarget::Param(3, false))),
        ("Spray: 00m0.120s", Some(LiveTarget::Param(2, false))),
        ("Pan spray: 0.300", Some(LiveTarget::Param(5, false))),
        ("Panning: 20% L", Some(LiveTarget::Param(5, true))),
        ("Scan: 0.25x", Some(LiveTarget::Param(4, false))),
        ("Fwdrev bias: 0.500", Some(LiveTarget::Param(4, true))),
        ("Loop start 0.250", Some(LiveTarget::Param(6, false))),
        ("Loop end 0.900", Some(LiveTarget::Param(7, false))),
        ("Attack: 0.300s", setting("attack")),
        ("Release: 2.400s", setting("release")),
        ("Cutoff: 800.00Hz", setting("cutoff")),
        ("Patch vol: 0.800", setting("patch_volume")),
        ("Reducer bits: 0.500", setting("bit_reduction")),
        ("Voice 2 volume 0.80", setting("voice2_volume")),
        ("Voice 3: C#4", setting("voice3_pitch")),
        ("Part [2] volume [-3.0] dB", Some(LiveTarget::PartVolume(1))),
        ("Voice 3 pitch ---", None),
        ("Part 2 volume ---", None),
        ("LFO1 MIDI Clock: 1/2", None),
        ("Scan MIDI clock: 3/2", None),
        ("Tuning: +0.250 st", None),
        ("LFO REC ON", None),
        ("Play mode", None),
        ("Emergency engine stop!", None),
    ];
    for (text, expected) in cases {
        assert_eq!(app.engine_control(text), expected, "{text}");
    }
}

#[test]
fn lfo_trace_thickens_into_the_band_by_readings_per_period() {
    use super::performance::lfo_band_seconds;
    // 12 or more readings per period: the wave alone.
    assert_eq!(lfo_band_seconds(5.), 0.);
    assert_eq!(lfo_band_seconds(0.5), 0.);
    assert_eq!(lfo_band_seconds(0.), 0.);
    // 8 or fewer: one whole period of readings.
    assert!((lfo_band_seconds(7.5) - 1. / 7.5).abs() < 1e-6);
    assert!((lfo_band_seconds(15.) - 1. / 15.).abs() < 1e-6);
    // In between the span grows steadily, from a segment to a period.
    let mut last = 0.;
    for i in 0..=50 {
        let rate = 5.01 + i as f32 * 0.0498;
        let band = lfo_band_seconds(rate);
        assert!(band >= last - 1e-6 && band >= 2. / 60. - 1e-6, "{rate}");
        assert!(band <= 1. / rate + 1e-6);
        last = band;
    }
    assert!((lfo_band_seconds(7.5001) - 1. / 7.5).abs() < 1e-3);
}

#[test]
fn parts_view_does_not_repeat_the_focused_preset_in_the_status_bar() {
    let (mut app, mut p) = app();
    let identity = Rect(16, 428, 260, 26);
    assert!(
        count(&p, identity, INK) > 50,
        "single part: name in the status bar"
    );
    assert!(count(&p, identity, BLUE) > 20, "single part: preset number");
    app.enabled = [true, true, false, false];
    app.part = 1;
    app.render(&mut p);
    assert_eq!(count(&p, identity, INK), 0, "name only in the focused row");
    assert_eq!(
        count(&p, identity, BLUE),
        0,
        "number only in the focused row"
    );
    assert!(
        // Two rows of 209 px: the focused second row starts at y = 214.
        count(&p, Rect(36, 218, 200, 22), INK) > 30,
        "focused row header"
    );
}

#[test]
fn grain_size_readout_at_scan_endpoint_stays_visible_and_expires() {
    let (mut app, mut p) = app();
    // The desktop scan follows Position; this represents a native scan
    // readback that rounds to the last pixel of the 544-pixel waveform.
    app.patch_mut().params[0] = 99.99;
    app.render(&mut p);
    app.patch_mut().params[1] = 240.;
    println!("Rendering Size readout: scan=0.9999, rounded x=543, prior clamp upper=0");
    app.render(&mut p);
    assert_eq!(app.readout_target(), Some(LiveTarget::Param(1, false)));
    assert_eq!(pixel(&p, 543, 205), rgb(&p, AMBER));
    assert_ne!(pixel(&p, 544, 205), rgb(&p, AMBER));
    if let Some(captures) = std::env::var_os("GR1_UI_TEST_CAPTURE_DIR") {
        p.png(&std::path::PathBuf::from(captures).join("grain-size-endpoint.png"))
            .unwrap();
    }
    let at = app.touch.last.unwrap().1;
    app.touch.last = Some((LiveTarget::Param(1, false), at - Duration::from_secs(2)));
    app.render(&mut p);
    assert!(app.readout_target().is_none());
    assert_eq!(count(&p, Rect(542, 198, 2, 16), AMBER), 0);
    assert_eq!(count(&p, READOUT, AMBER), 0);
}

#[test]
fn grain_size_bracket_finite_boundaries_keep_the_waveform_layout() {
    for theme in [Theme::White, Theme::Dark] {
        let (mut app, mut p) = app();
        app.appearance.theme = theme;
        for position in [0., 50., 99.7, 99.9, 99.99, 100.] {
            for size in [0., 1., 240., 9000.] {
                app.patch_mut().params[0] = position;
                app.patch_mut().params[1] = size;
                app.render(&mut p);
                app.touched(LiveTarget::Param(1, false));
                app.render(&mut p);
                assert_eq!(app.main_rows(), [0]);
                assert!(
                    count(&p, Rect(0, 198, 544, 16), AMBER) > 0,
                    "{theme:?}, Position={position}, Size={size}: bracket stays visible"
                );
                assert_eq!(
                    count(&p, Rect(544, 198, 20, 16), AMBER),
                    0,
                    "the bracket stays inside the waveform field"
                );
            }
        }
        app.enabled = [true, true, false, true];
        app.render(&mut p);
        assert_eq!(app.main_rows(), [0, 1, 3]);
        assert_eq!(pixel(&p, 300, 140), rgb(&p, BACKGROUND));
    }
}
