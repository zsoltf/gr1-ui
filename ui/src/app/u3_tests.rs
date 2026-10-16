//! Patch journeys replayed through the production panel dispatcher, counting
//! presses as panel input does: every button event counts, an
//! encoder turn (event 5) does not. No native engine is loaded; desktop
//! values stand in for engine readback.
use super::panel_tests::Panel;
use super::patch_root::{Item, MATRIX, PAGES, PLAY, TIMING};
use super::*;

const PATCH: (u8, u8) = (13, 0);
const PRESS: (u8, u8) = (7, 0);
const ESC: (u8, u8) = (14, 0);

struct Journey {
    p: Panel,
    presses: usize,
}
impl Journey {
    fn new() -> Self {
        Self {
            p: Panel::new(),
            presses: 0,
        }
    }
    fn go(&mut self, events: &[(u8, u8)]) {
        self.presses += events.iter().filter(|(action, _)| *action != 5).count();
        for event in events {
            self.p.events(&[*event]);
        }
    }
    fn turn(&mut self, delta: i32) {
        if delta != 0 {
            self.p.events(&[(5, (delta.clamp(-127, 127) as i8) as u8)]);
        }
    }
    /// Shift+Escape: ESC while the engine reports Shift (observed 2026-10-12).
    fn shift_escape(&mut self) {
        self.p.app.shift = true;
        self.go(&[ESC]);
        self.p.app.shift = false;
    }
    fn app(&self) -> &App {
        &self.p.app
    }
    fn active(&self) -> Item {
        self.p.app.patch_active()
    }
    fn setting(&self, id: &str) -> Item {
        Item::Setting(self.p.app.setting_index(id))
    }
    fn page(&self) -> &'static str {
        PAGES[self.p.app.patch_ui.page]
    }
    fn screen(&self) -> i32 {
        self.p.app.screen_id()
    }
    /// Turn onto `item` on the page or in the entered container.
    fn turn_to(&mut self, item: Item) {
        let app = &self.p.app;
        let items = match app.patch_ui.entered {
            Some(container) => app.inner_items(container),
            None => app.page_items(app.patch_ui.page),
        };
        let from = items.iter().position(|i| *i == app.patch_active()).unwrap();
        let to = items
            .iter()
            .position(|i| *i == item)
            .unwrap_or_else(|| panic!("{item:?} is not on {}", self.page()));
        self.turn(to as i32 - from as i32);
        assert_eq!(self.active(), item);
    }
}

#[test]
fn j3_changes_scan_mode_to_ping_pong_in_six_presses_from_play() {
    let mut j = Journey::new();
    let mode = j.setting("scan_mode");
    j.go(&[PATCH]);
    assert_eq!(
        (j.page(), j.screen()),
        ("Play", 4),
        "the root runs under id 4"
    );
    j.go(&[ESC]);
    assert!(j.app().patch_ui.on_strip);
    assert_eq!(j.screen(), 4, "the page strip is part of the root");
    j.turn(1);
    assert_eq!(
        j.page(),
        "Timing",
        "turning the strip shows the page at once"
    );
    j.go(&[PRESS]);
    j.turn_to(mode);
    j.go(&[PRESS]);
    assert_eq!(j.app().patch_grabbed(), Some(mode));
    j.turn(2);
    // The option shows in amber; nothing reaches the engine while turning.
    assert_eq!(j.app().get("scan_mode"), 0.);
    assert_eq!(
        j.app().patch_ui.choice,
        Some((j.p.app.setting_index("scan_mode"), 2.))
    );
    j.go(&[PRESS]);
    assert_eq!(j.app().get("scan_mode"), 2., "Select applies Ping pong");
    j.shift_escape();
    assert_eq!(j.presses, 6);
    assert_eq!((j.app().tab, j.screen()), (0, 0));
}

#[test]
fn j3_costs_five_presses_after_play_and_four_when_timing_was_last() {
    let mut j = Journey::new();
    // Last page Play: the same chord again steps to Timing.
    j.go(&[PATCH, PATCH]);
    assert_eq!(j.page(), "Timing");
    let mode = j.setting("scan_mode");
    j.turn_to(mode);
    j.go(&[PRESS]);
    j.turn(2);
    j.go(&[PRESS]);
    j.shift_escape();
    assert_eq!(j.presses, 5);
    assert_eq!(j.app().get("scan_mode"), 2.);
    // Timing was last: Shift+Patch resumes on Mode.
    j.presses = 0;
    j.go(&[PATCH]);
    assert_eq!((j.page(), j.active()), ("Timing", mode));
    j.go(&[PRESS]);
    j.turn(-1);
    j.go(&[PRESS]);
    j.shift_escape();
    assert_eq!(j.presses, 4);
    assert_eq!(j.app().get("scan_mode"), 1., "One shot");
}

#[test]
fn j4_adds_lfo2_to_size_at_minus_thirty_in_seven_presses() {
    let mut j = Journey::new();
    j.go(&[PATCH, ESC]);
    j.turn(MATRIX as i32 - PLAY as i32);
    assert_eq!(j.page(), "Matrix");
    j.go(&[PRESS]);
    j.turn_to(Item::Row(1));
    j.go(&[PRESS]);
    assert_eq!(j.app().patch_ui.entered, Some(Item::Row(1)));
    j.turn_to(Item::Cell(1, 3));
    j.go(&[PRESS]);
    assert_eq!(j.app().patch_grabbed(), Some(Item::Cell(1, 3)));
    // Numbers change live, one percent per detent.
    j.turn(-30);
    assert!((j.app().patch().routes[1][3] + 0.30).abs() < 1e-4);
    j.go(&[PRESS]);
    j.shift_escape();
    assert_eq!(j.presses, 7);
    assert!((j.app().patch().routes[1][3] + 0.30).abs() < 1e-4);
    assert!(j.app().patch().dirty);
}

#[test]
fn j4_costs_five_presses_when_matrix_and_the_row_were_last() {
    let mut j = Journey::new();
    j.go(&[PATCH, ESC]);
    j.turn(MATRIX as i32);
    j.go(&[PRESS]);
    j.turn_to(Item::Row(1));
    j.shift_escape();
    j.presses = 0;
    // Shift+Patch resumes on the LFO 2 row; the row remembers nothing yet.
    j.go(&[PATCH]);
    assert_eq!((j.page(), j.active()), ("Matrix", Item::Row(1)));
    j.go(&[PRESS]);
    j.turn_to(Item::Cell(1, 3));
    j.go(&[PRESS]);
    // The Position slider sets a grabbed value (Position slider in menus).
    j.p.events(&[(22, 89)]);
    let slider = j.app().patch().routes[1][3];
    assert!((slider - (-1. + 2. * 89. / 255.)).abs() < 0.006, "{slider}");
    j.turn(-30 - (slider * 100.).round() as i32);
    j.go(&[PRESS]);
    j.shift_escape();
    assert_eq!(j.presses, 5);
    assert!((j.app().patch().routes[1][3] + 0.30).abs() < 1e-4);
}

#[test]
fn escape_goes_to_the_strip_and_a_press_returns_to_the_same_field() {
    let mut j = Journey::new();
    j.go(&[PATCH, PATCH]);
    let divider = j.setting("scan_div");
    j.turn_to(divider);
    j.go(&[ESC]);
    assert!(j.app().patch_ui.on_strip);
    // Turning the strip shows other pages; the field is remembered per page.
    j.turn(3);
    assert_eq!(j.page(), "Voices");
    j.turn(-3);
    j.go(&[PRESS]);
    assert!(!j.app().patch_ui.on_strip);
    assert_eq!((j.page(), j.active()), ("Timing", divider));
    // From the strip, Escape leaves the root; Shift+Patch resumes there.
    j.go(&[ESC, ESC]);
    assert_eq!(j.app().tab, 0);
    j.go(&[PATCH]);
    assert_eq!((j.page(), j.active()), ("Timing", divider));
    assert!(!j.app().patch_ui.on_strip);
}

#[test]
fn the_same_chord_steps_through_all_nine_pages_and_wraps() {
    let mut j = Journey::new();
    j.go(&[PATCH]);
    let mut seen = vec![j.page()];
    for _ in 0..PAGES.len() {
        j.go(&[PATCH]);
        seen.push(j.page());
    }
    assert_eq!(&seen[..9], &PAGES[..]);
    assert_eq!(seen[9], "Play", "after LFO 2 the chord wraps to Play");
    // From the strip the chord steps too, and the strip keeps focus.
    j.go(&[ESC, PATCH]);
    assert_eq!(j.page(), "Timing");
    assert!(j.app().patch_ui.on_strip);
}

#[test]
fn escape_restores_a_grabbed_choice_and_a_grabbed_number() {
    let mut j = Journey::new();
    j.go(&[PATCH]);
    let mode = j.setting("sample_mode");
    assert_eq!(j.active(), mode);
    j.go(&[PRESS]);
    j.turn(1);
    assert_eq!(
        j.app().get("sample_mode"),
        0.,
        "a choice is not sent while turning"
    );
    j.go(&[ESC]);
    assert_eq!(j.app().get("sample_mode"), 0.);
    assert_eq!(j.app().patch_grabbed(), None);
    assert_eq!(j.active(), mode, "Escape releases and keeps the field");
    assert!(!j.app().patch_ui.on_strip);
    let volume = j.setting("patch_volume");
    j.turn_to(volume);
    j.go(&[PRESS]);
    j.turn(7);
    assert_eq!(j.app().get("patch_volume"), 87., "a number changes live");
    j.go(&[ESC]);
    assert_eq!(j.app().get("patch_volume"), 80.);
    assert!(
        !j.app().patch().dirty,
        "the restore also restores unsaved state"
    );
    // Shift+Escape restores, then goes to the main screen.
    j.go(&[PRESS]);
    j.turn(5);
    j.shift_escape();
    assert_eq!(j.app().get("patch_volume"), 80.);
    assert_eq!(j.app().tab, 0);
}

#[test]
fn matrix_rows_are_entered_and_cells_carry_their_sign() {
    let mut j = Journey::new();
    j.go(&[PATCH, ESC]);
    j.turn(MATRIX as i32);
    j.go(&[PRESS]);
    // Turning moves across rows; a press enters one.
    j.turn_to(Item::Row(4));
    j.go(&[PRESS]);
    // MPE sources cannot reach Tune or the window: Timbre starts at Pos.
    assert_eq!(j.active(), Item::Cell(4, 1));
    j.turn(20);
    assert_eq!(j.active(), Item::Cell(4, 6), "only Pitch reaches Cutoff");
    j.turn(-5);
    assert_eq!(j.active(), Item::Cell(4, 1));
    // Grab, go negative, keep: the sign is in the route.
    j.go(&[PRESS]);
    j.turn(-25);
    j.go(&[PRESS]);
    assert!((j.app().patch().routes[4][1] + 0.25).abs() < 1e-4);
    // Grab, go positive, Escape: the negative value comes back.
    j.go(&[PRESS]);
    j.turn(60);
    assert!((j.app().patch().routes[4][1] - 0.35).abs() < 1e-4);
    j.go(&[ESC]);
    assert!((j.app().patch().routes[4][1] + 0.25).abs() < 1e-4);
    assert_eq!(j.app().patch_ui.entered, Some(Item::Row(4)));
    // Escape leaves the row onto the row; the row remembers its cell.
    j.go(&[ESC]);
    assert_eq!((j.app().patch_ui.entered, j.active()), (None, Item::Row(4)));
    j.go(&[PRESS]);
    assert_eq!(j.active(), Item::Cell(4, 1));
}

#[test]
fn a_preset_press_returns_to_the_main_screen_from_any_state() {
    let mut j = Journey::new();
    // A choice being turned is dropped: MAIN, then SET_PRESET.
    j.go(&[PATCH]);
    j.go(&[PRESS]);
    j.turn(1);
    j.go(&[(6, 0), (10, 0)]);
    assert_eq!((j.app().tab, j.screen()), (0, 0));
    assert_eq!(j.app().get("sample_mode"), 0.);
    assert_eq!(j.app().patch_ui.choice, None);
    // A number keeps what it already sent; nothing is written afterwards.
    j.go(&[PATCH]);
    let volume = j.setting("patch_volume");
    j.turn_to(volume);
    j.go(&[PRESS]);
    j.turn(3);
    j.go(&[(10, 0)]);
    assert_eq!(j.app().tab, 0);
    assert!(j.app().live.is_none());
    assert_eq!(j.app().get("patch_volume"), 83.);
    // From the strip and from inside a matrix row.
    j.go(&[PATCH, ESC, (10, 0)]);
    assert_eq!(j.app().tab, 0);
    j.go(&[PATCH, ESC]);
    j.turn(MATRIX as i32);
    j.go(&[PRESS, PRESS, (6, 0), (10, 0)]);
    assert_eq!(j.app().tab, 0);
    assert_eq!(j.app().patch_ui.entered, None);
}

#[test]
fn a_panel_verb_keeps_the_grab_then_acts() {
    let mut j = Journey::new();
    j.go(&[PATCH, PATCH]);
    let mode = j.setting("scan_mode");
    j.turn_to(mode);
    j.go(&[PRESS]);
    j.turn(1);
    j.go(&[(1, 1)]);
    assert_eq!(j.app().get("scan_mode"), 1., "the shown option was kept");
    assert!(j.app().overlay_active(), "then Load opened the browser");
    j.go(&[ESC, ESC]);
    assert_eq!((j.app().tab, j.page(), j.active()), (1, "Timing", mode));
    assert_eq!(j.screen(), 4);
}

#[test]
fn keyboard_tuning_learns_a_played_note_only_while_grabbed() {
    let mut j = Journey::new();
    j.go(&[PATCH, ESC]);
    j.turn(5);
    assert_eq!(j.page(), "Keys");
    j.go(&[PRESS]);
    let tuning = j.setting("tuning");
    assert_eq!(j.active(), tuning);
    // Focused but not grabbed: playing does not retune the patch.
    j.go(&[(24, 48)]);
    assert_eq!(j.app().get("tuning"), 60.);
    j.go(&[PRESS, (24, 48)]);
    assert_eq!(j.app().get("tuning"), 48.);
    j.go(&[PRESS]);
    assert_eq!(j.app().get("tuning"), 48.);
    assert!(j.app().live.is_none());
}

#[test]
fn toggles_flip_and_the_scale_toggles_keys_at_once() {
    let mut j = Journey::new();
    j.go(&[PATCH, ESC]);
    j.turn(TIMING as i32);
    j.go(&[PRESS]);
    let clock = j.setting("scan_clock");
    j.turn_to(clock);
    j.go(&[PRESS]);
    assert_eq!(j.app().get("scan_clock"), 1.);
    assert_eq!(j.app().patch_grabbed(), None, "a toggle never grabs");
    j.go(&[ESC]);
    assert!(j.app().patch_ui.on_strip, "Escape does not undo a toggle");
    assert_eq!(j.app().get("scan_clock"), 1.);
    j.turn(3);
    assert_eq!(j.page(), "Voices");
    j.go(&[PRESS]);
    j.turn_to(Item::Scale);
    j.go(&[PRESS]);
    j.turn_to(Item::Key(1));
    j.go(&[PRESS]);
    assert!(!j.app().scale_notes()[1]);
    j.go(&[PRESS]);
    assert!(j.app().scale_notes()[1]);
    j.go(&[ESC]);
    assert_eq!((j.app().patch_ui.entered, j.active()), (None, Item::Scale));
    // Shift+press may arrive as UNMOUNT: in the root it neither unmounts
    // nor asks to.
    j.go(&[(28, 0)]);
    assert!(j.app().modal.is_none() && j.app().in_patch_root());
    // AUX shape Record/Stop runs at once.
    j.go(&[ESC]);
    j.turn(3);
    j.go(&[PRESS]);
    j.turn_to(Item::Aux);
    j.go(&[PRESS]);
    assert!(j.app().aux_recording);
    j.go(&[PRESS]);
    assert!(!j.app().aux_recording);
}

#[test]
fn every_patch_page_walks_grabs_restores_and_keeps_focus() {
    for page in 0..PAGES.len() {
        let mut j = Journey::new();
        j.go(&[PATCH, ESC]);
        j.turn(page as i32);
        j.go(&[PRESS]);
        assert_eq!(j.page(), PAGES[page]);
        let items = j.app().page_items(page);
        for (n, item) in items.iter().enumerate() {
            assert_eq!(j.active(), *item, "{} item {n}", PAGES[page]);
            if let Item::Setting(i) = item {
                let id = j.app().catalog[*i].id.clone();
                let before = j.app().get(&id);
                match j.app().catalog[*i].kind {
                    Kind::Number { .. } => {
                        j.go(&[PRESS]);
                        j.turn(1);
                        j.go(&[ESC]);
                        assert_eq!(j.app().get(&id), before, "restore {id}");
                    }
                    Kind::Choice(_) => {
                        j.go(&[PRESS]);
                        j.turn(1);
                        assert_eq!(j.app().get(&id), before, "{id} waits for Select");
                        j.go(&[ESC]);
                        assert_eq!(j.app().get(&id), before);
                    }
                    Kind::Toggle => {
                        j.go(&[PRESS, PRESS]);
                        assert_eq!(j.app().get(&id), before, "{id} flips back");
                    }
                }
                assert_eq!(j.active(), *item, "focus stays on {id}");
            }
            // The page keeps its layout when a grab ends (FX type, Tape mode).
            assert_eq!(j.app().page_items(page), items);
            j.turn(1);
        }
        assert_eq!(
            j.active(),
            *items.last().unwrap(),
            "the walk ends at the last item"
        );
        // Escape to the strip and back to the same field.
        j.go(&[ESC, PRESS]);
        assert_eq!(j.active(), *items.last().unwrap());
    }
}

#[test]
fn every_patch_and_modulation_setting_has_one_place_in_the_root() {
    let app = App::new();
    let mut placed: Vec<usize> = vec![];
    for page in 0..PAGES.len() {
        for item in app.page_items(page) {
            if let Item::Setting(i) = item {
                placed.push(i);
            }
        }
    }
    // Effect parameters appear with their type, Tape slew in Tape mode.
    for i in app.group_settings(3, "Effect") {
        placed.push(i);
    }
    placed.push(app.setting_index("tape_slew"));
    placed.sort();
    placed.dedup();
    let old_pages: Vec<&str> = app
        .catalog
        .iter()
        .filter(|s| !s.system || s.id == "matrix_mode")
        .filter(|s| !matches!(s.page, "Sample" | "System"))
        .map(|s| s.id.as_str())
        .collect();
    for id in old_pages {
        assert!(
            placed.contains(&app.setting_index(id)),
            "{id} has no place in the Patch root"
        );
    }
}

#[test]
fn the_mod_tab_and_main_screen_shortcuts_open_patch_pages() {
    let mut j = Journey::new();
    j.p.app.action(Action::Nav(2));
    assert_eq!((j.app().tab, j.page()), (1, "Matrix"));
    j.p.app.action(Action::Nav(0));
    j.p.app.action(Action::Op("lfo2"));
    assert_eq!((j.app().tab, j.page()), (1, "LFO 2"));
    j.p.app.action(Action::Op("cv2"));
    assert_eq!(j.page(), "Matrix");
    assert_eq!(j.active(), j.setting("cv2_amount"));
}

fn luminance([r, g, b]: [u8; 3]) -> f32 {
    let channel = |c: u8| {
        let c = c as f32 / 255.;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

/// The largest 4-connected area of pixels brighter than 0.20.
pub(super) fn largest_bright_area(p: &Painter) -> usize {
    let bright: Vec<bool> = p
        .pixels
        .chunks_exact(4)
        .map(|px| luminance([px[0], px[1], px[2]]) > 0.20)
        .collect();
    let mut seen = vec![false; bright.len()];
    let mut largest = 0;
    for start in 0..bright.len() {
        if !bright[start] || seen[start] {
            continue;
        }
        let (mut stack, mut size) = (vec![start], 0);
        seen[start] = true;
        while let Some(i) = stack.pop() {
            size += 1;
            let (x, y) = (i % 800, i / 800);
            let mut push = |j: usize| {
                if bright[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                push(i - 1);
            }
            if x < 799 {
                push(i + 1);
            }
            if y > 0 {
                push(i - 800);
            }
            if y < 479 {
                push(i + 800);
            }
        }
        largest = largest.max(size);
    }
    largest
}

#[test]
fn dark_patch_pages_draw_no_large_bright_fills_and_show_the_strip() {
    let mut j = Journey::new();
    j.p.app.appearance.theme = Theme::Dark;
    j.p.app.appearance.brightness = 100;
    j.p.app.playing = true;
    // The simulated LFOs follow the demo clock; pinned, every run draws the
    // same traces.
    j.p.app.demo_clock = Some(100.);
    j.go(&[PATCH]);
    for page in 0..PAGES.len() {
        let mut p = Painter::new().unwrap();
        j.p.app.render(&mut p);
        // Redesign 5.5: no connected area above 0.20 larger than 2% of the
        // screen (7,680 pixels). The largest here is the LFO page's trace:
        // its 2-pixel blue line over 760 pixels and the current-value dot,
        // no fill (the band's wash stays below 0.20). At the default 0.12 Hz
        // it measures 887 to 1,942 pixels over the phase, up to 2,138 for any
        // wave below 2 Hz; the limit allows that line, not a filled block.
        let largest = largest_bright_area(&p);
        assert!(
            largest < 2400,
            "{}: {largest} bright pixels in one area",
            PAGES[page]
        );
        let strip = status::STRIP;
        let waveform = WAVEFORM.rgb(Theme::Dark, 100);
        let columns = (strip.1..strip.1 + strip.3)
            .flat_map(|y| (strip.0..strip.0 + strip.2).map(move |x| (x, y)))
            .filter(|(x, y)| {
                let i = (*y as usize * 800 + *x as usize) * 4;
                p.pixels[i..i + 3] == waveform
            })
            .count();
        assert!(columns > 200, "{}: the live strip is drawn", PAGES[page]);
        j.go(&[PATCH]);
    }
}

#[test]
fn the_strip_fallback_drops_grains_and_the_rate_fallbacks_reuse_pixels() {
    let mut j = Journey::new();
    j.p.app.appearance.theme = Theme::Dark;
    j.p.app.playing = true;
    j.go(&[PATCH]);
    let grain = GRAIN.rgb(Theme::Dark, j.p.app.appearance.brightness);
    let count = |app: &mut App| {
        let mut p = Painter::new().unwrap();
        app.render(&mut p);
        let s = status::STRIP;
        (s.1..s.1 + s.3)
            .flat_map(|y| (s.0..s.0 + s.2).map(move |x| (x, y)))
            .filter(|(x, y)| {
                let i = (*y as usize * 800 + *x as usize) * 4;
                p.pixels[i..i + 3] == grain
            })
            .count()
    };
    assert!(count(&mut j.p.app) > 0, "live: grains in the strip");
    j.p.app.strip_policy = patch_root::StripPolicy::parse("no-grains");
    assert_eq!(count(&mut j.p.app), 0, "fallback: no grains");
    // Frozen while grabbed: the drawn frame is reused, not redrawn.
    j.p.app.strip_policy = patch_root::StripPolicy::parse("freeze-grab");
    let mut p = Painter::new().unwrap();
    j.p.app.render(&mut p);
    assert!(j.p.app.strip_drawn.take().is_some());
    j.go(&[ESC, PRESS]);
    j.turn(2);
    j.go(&[PRESS]);
    assert!(j.p.app.patch_grabbed().is_some());
    j.p.app.strip_drawn = None;
    j.p.app.render(&mut p);
    assert!(j.p.app.strip_drawn.is_none(), "frozen while grabbed");
    j.go(&[ESC]);
    j.p.app.render(&mut p);
    assert!(j.p.app.strip_drawn.is_some());
    // Off: nothing drawn, for the measurement without the strip.
    j.p.app.strip_policy = patch_root::StripPolicy::parse("off");
    j.p.app.strip_drawn = None;
    j.p.app.render(&mut p);
    assert!(j.p.app.strip_drawn.is_none());
}

#[test]
fn the_panel_trace_names_the_patch_page_and_focus() {
    let mut j = Journey::new();
    j.p.app.panel_trace = true;
    j.go(&[PATCH, ESC]);
    let lines = &j.p.app.panel_trace_lines;
    assert!(
        lines[0].contains("-> ctx=patch:Play:Setting("),
        "{}",
        lines[0]
    );
    assert!(lines[0].ends_with("next_screen=4"), "{}", lines[0]);
    assert!(lines[1].contains("-> ctx=patch:Play:strip"), "{}", lines[1]);
}

#[test]
fn a_readout_takes_the_strip_place_only_when_the_chips_leave_no_room() {
    let mut j = Journey::new();
    j.go(&[PATCH]);
    let mut p = Painter::new().unwrap();
    // One chip (the desktop's Demo, as the device's USB would be): the
    // readout fits at the right; the strip stays.
    j.p.app.usb_mounted = false;
    j.p.app.touched(LiveTarget::Param(2, false));
    j.p.app.strip_drawn = None;
    j.p.app.render(&mut p);
    assert!(j.p.app.strip_drawn.is_some());
    // USB, Shift and Rec chips as well: the readout takes the strip's place.
    j.p.app.usb_mounted = true;
    j.p.app.shift = true;
    j.p.app.recording = Some(0);
    j.p.app.strip_drawn = None;
    j.p.app.render(&mut p);
    assert!(j.p.app.strip_drawn.is_none());
    j.p.app.touch.last = None;
    j.p.app.render(&mut p);
    assert!(
        j.p.app.strip_drawn.is_some(),
        "the strip returns after the hold"
    );
}

#[test]
fn the_sample_root_draws_the_shared_strip_and_page_strip() {
    let mut j = Journey::new();
    j.go(&[(15, 0)]);
    assert!(j.app().sample_root_active());
    let mut p = Painter::new().unwrap();
    j.p.app.strip_drawn = None;
    j.p.app.render(&mut p);
    assert!(j.p.app.strip_drawn.is_some());
    assert_eq!(j.app().sample_ui.tabs.len(), 2, "both pages have boxes");
    // While its buffer records, the strip carries the red wiper.
    j.p.app.recording = Some(j.p.app.parts[j.p.app.part]);
    j.p.app.sample_ui.demo_record_started = Some(Instant::now() - Duration::from_secs(2));
    let view = j.p.app.strip_view(status::STRIP.2 as usize, true);
    assert!(view.wiper.is_some());
}
