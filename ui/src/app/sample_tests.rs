//! Sample journeys on the Sample root, replayed through the production panel
//! dispatcher. Presses are counted as panel input does: every
//! button event counts, an encoder turn (event 5) does not. The desktop demo
//! stands in for the engine: recording, the level and the buffer are its
//! simulated state, never the native engine's.
use super::overlay::{DataKind, Layer, SaveField, Verb};
use super::panel_tests::Panel;
use super::sample_root::{Field, Page};
use super::*;

const PRESS: (u8, u8) = (7, 0);
const ESC: (u8, u8) = (14, 0);
const SHIFT_SAMPLE: (u8, u8) = (15, 0);
const SAVE: (u8, u8) = (2, 1);
const COPY: (u8, u8) = (4, 0);
const KILL_SAMPLE: (u8, u8) = (29, 0);

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
    /// Turn by `delta` detents in one report; turns are not counted.
    fn turn(&mut self, delta: i32) {
        if delta != 0 {
            self.p.events(&[(5, (delta.clamp(-127, 127) as i8) as u8)]);
        }
    }
    fn turn_to(&mut self, field: Field) {
        let page = self.app().sample_ui.page;
        let target = page
            .fields()
            .iter()
            .position(|f| *f == field)
            .unwrap_or_else(|| panic!("{field:?} is not on {page:?}"));
        let at = self.app().sample_ui.focus[page.index()] as i32;
        self.turn(target as i32 - at);
        assert_eq!(self.app().sample_field(), field);
    }
    fn app(&self) -> &App {
        &self.p.app
    }
    fn value(&self, id: &str) -> f32 {
        self.p.app.get(id)
    }
    fn grabbed(&self) -> bool {
        self.p.app.sample_ui.grab.is_some()
    }
}

#[test]
fn j6_records_eight_seconds_and_saves_the_sample_in_seven_presses() {
    let mut j = Journey::new();
    assert_eq!(
        j.value("start_mode"),
        0.,
        "the demo starts with Start mode Off"
    );
    assert_eq!(j.value("sample_size"), 8.);
    j.go(&[SHIFT_SAMPLE]);
    assert_eq!(j.app().tab, 3);
    assert_eq!(
        j.app().screen_id(),
        3,
        "the Sample root runs under screen id 3"
    );
    assert_eq!(j.app().sample_ui.page, Page::Record);
    assert_eq!(
        j.app().sample_field(),
        Field::Record,
        "Record/Stop is first"
    );
    // Start mode Off -> Wiper: grab, turn, keep (2 presses).
    j.turn_to(Field::Setting("start_mode"));
    j.go(&[PRESS]);
    j.turn(2);
    assert_eq!(j.value("start_mode"), 0., "a choice applies on Select only");
    j.go(&[PRESS]);
    assert_eq!(j.value("start_mode"), 2.);
    // Record, then Stop on the same field.
    j.turn_to(Field::Record);
    j.go(&[PRESS]);
    assert_eq!(j.app().recording, Some(0));
    assert!(
        j.app().record_fraction().is_some(),
        "the wiper has a position"
    );
    j.go(&[PRESS]);
    assert_eq!(j.app().recording, None);
    // Save sample opens the Save sheet with the Sample type on Save.
    j.turn_to(Field::Save);
    j.go(&[PRESS]);
    match j.app().files_ui.layers.last() {
        Some(Layer::Save(s)) => {
            assert_eq!((s.kind, s.focus), (DataKind::Sample, SaveField::Save));
            assert_eq!(s.folder, "/Internal", "the last sample folder");
            assert_eq!(s.name, "REEDS_04-2", "a recording is a new take");
        }
        _ => panic!("Save sample opens the Save sheet"),
    }
    j.go(&[SAVE]);
    assert_eq!(j.presses, 7);
    assert!(!j.app().overlay_active());
    assert!(j.app().files.contains(&"REEDS_04-2.wav".to_string()));
    assert_eq!(j.app().toast, "Saved REEDS_04-2.wav");
    // Back on the Record page, on the field that opened the sheet.
    assert_eq!(j.app().tab, 3);
    assert_eq!(j.app().sample_field(), Field::Save);
}

#[test]
fn j6_takes_five_presses_when_the_page_resumes_on_record() {
    let mut j = Journey::new();
    j.go(&[SHIFT_SAMPLE, PRESS, PRESS]);
    j.turn_to(Field::Save);
    j.go(&[PRESS, SAVE]);
    assert_eq!(j.presses, 5);
    assert!(j.app().files.contains(&"REEDS_04-2.wav".to_string()));
    // Leaving and re-entering resumes the last page and field (3.5.4).
    j.go(&[ESC, ESC]);
    assert_eq!(j.app().tab, 0);
    j.go(&[SHIFT_SAMPLE]);
    assert_eq!(j.app().sample_field(), Field::Save);
    j.turn_to(Field::Record);
    j.go(&[ESC, ESC, SHIFT_SAMPLE]);
    assert_eq!(j.app().sample_field(), Field::Record);
    // A second take of the same sample does not replace the first.
    j.go(&[PRESS, PRESS]);
    j.turn_to(Field::Save);
    j.go(&[PRESS]);
    match j.app().files_ui.layers.last() {
        Some(Layer::Save(s)) => assert_eq!(s.name, "REEDS_04-3"),
        _ => panic!("Save sample opens the Save sheet"),
    }
}

#[test]
fn record_and_stop_is_the_first_field_and_stays_on_a_part_change() {
    let mut j = Journey::new();
    j.go(&[SHIFT_SAMPLE]);
    assert_eq!(j.app().sample_ui.focus, [0, 0]);
    assert_eq!(Page::Record.fields()[0], Field::Record);
    assert!(j.app().focus_context().starts_with("sample:Record:Record"));
    j.go(&[PRESS]);
    assert_eq!(j.app().recording, Some(0));
    // The focused part changes; Stop still stops the recording buffer.
    j.p.app.part = 1;
    j.go(&[PRESS]);
    assert_eq!(j.app().recording, None);
}

#[test]
fn sample_size_previews_while_grabbed_and_applies_only_on_press() {
    let mut j = Journey::new();
    j.go(&[SHIFT_SAMPLE]);
    j.turn_to(Field::Setting("sample_size"));
    j.go(&[PRESS]);
    assert!(j.grabbed());
    j.turn(40);
    assert_eq!(j.app().sample_ui.grab.unwrap().draft, 12.);
    assert_eq!(j.value("sample_size"), 8., "turning only previews");
    j.go(&[ESC]);
    assert!(!j.grabbed());
    assert_eq!(j.value("sample_size"), 8., "Escape discards the preview");
    assert_eq!(j.app().sample_field(), Field::Setting("sample_size"));
    // Shift: the engine reports 10 per detent; it is not multiplied again.
    j.go(&[PRESS]);
    j.turn(10);
    j.go(&[PRESS]);
    assert_eq!(j.value("sample_size"), 9., "Select applies the new size");
    // Only while stopped: recording refuses the grab and says why.
    j.turn_to(Field::Record);
    j.go(&[PRESS]);
    j.turn_to(Field::Setting("sample_size"));
    j.go(&[PRESS]);
    assert!(!j.grabbed());
    assert_eq!(j.app().toast, "Stop recording to resize the sample");
    assert!(j.app().toast_error);
    j.turn(40);
    assert_eq!(j.value("sample_size"), 9.);
}

#[test]
fn escape_returns_to_the_same_field_from_every_level() {
    let mut j = Journey::new();
    j.go(&[SHIFT_SAMPLE]);
    // A live number: turning sends, Escape restores and stays on the field.
    j.turn_to(Field::Setting("rec_level"));
    j.go(&[PRESS]);
    j.turn(5);
    assert_eq!(j.value("rec_level"), 105., "numbers reach the engine live");
    j.go(&[ESC]);
    assert_eq!(j.value("rec_level"), 100.);
    assert_eq!(j.app().sample_field(), Field::Setting("rec_level"));
    // Page strip and back.
    j.go(&[ESC]);
    assert!(j.app().sample_ui.strip);
    assert!(j.app().focus_context().ends_with(":strip"));
    j.go(&[PRESS]);
    assert!(!j.app().sample_ui.strip);
    assert_eq!(j.app().sample_field(), Field::Setting("rec_level"));
    // The Save sheet and back.
    j.turn_to(Field::Save);
    j.go(&[PRESS, ESC]);
    assert!(!j.app().overlay_active());
    assert_eq!(j.app().sample_field(), Field::Save);
    // A confirmation opens on Cancel and Escape keeps the sample.
    j.turn_to(Field::Clear);
    j.go(&[PRESS]);
    match j.app().files_ui.layers.last() {
        Some(Layer::Confirm(c)) => {
            assert!(!c.on_verb, "a lossy confirmation opens on Cancel");
            assert_eq!(
                c.action,
                Verb::Run {
                    op: "clear_sample",
                    slot: 0
                }
            );
        }
        _ => panic!("Clear sample asks first"),
    }
    j.go(&[ESC]);
    assert_eq!(j.app().patch().sample, "REEDS_04.wav");
    assert_eq!(j.app().sample_field(), Field::Clear);
    // Another page keeps its own field; the strip shows it at once.
    j.go(&[ESC]);
    j.turn(1);
    assert_eq!(j.app().sample_ui.page, Page::Sample);
    j.go(&[PRESS]);
    assert_eq!(j.app().sample_field(), Field::Setting("pitch"));
    j.go(&[ESC]);
    j.turn(-1);
    j.go(&[PRESS]);
    assert_eq!(j.app().sample_field(), Field::Clear);
    // Shift+Escape: the main screen, restoring a grab first.
    j.turn_to(Field::Setting("monitor"));
    j.go(&[PRESS]);
    j.turn(7);
    j.p.app.shift = true;
    j.go(&[ESC]);
    j.p.app.shift = false;
    assert_eq!(j.app().tab, 0);
    assert_eq!(j.value("monitor"), 0.);
}

#[test]
fn the_chord_steps_pages_and_a_preset_press_returns_to_the_main_screen() {
    let mut j = Journey::new();
    j.go(&[SHIFT_SAMPLE]);
    assert_eq!(j.app().sample_ui.page, Page::Record);
    j.go(&[SHIFT_SAMPLE]);
    assert_eq!(j.app().sample_ui.page, Page::Sample);
    assert!(!j.app().sample_ui.strip, "the chord enters the page");
    assert_eq!(j.app().screen_id(), 3);
    j.go(&[SHIFT_SAMPLE]);
    assert_eq!(j.app().sample_ui.page, Page::Record);
    // A grab is dropped without a write: the engine already changed preset.
    j.turn_to(Field::Setting("sample_size"));
    j.go(&[PRESS]);
    j.turn(20);
    j.go(&[(6, 0), (10, 4)]);
    assert_eq!(j.app().tab, 0);
    assert!(!j.grabbed());
    assert_eq!(j.value("sample_size"), 8.);
    // Another root's chord keeps a grabbed value first (3.4).
    j.go(&[SHIFT_SAMPLE]);
    j.turn_to(Field::Setting("start_mode"));
    j.go(&[PRESS]);
    j.turn(1);
    j.go(&[(13, 0)]);
    assert_eq!(j.value("start_mode"), 1.);
    assert_ne!(j.app().tab, 3);
}

#[test]
fn toggles_flip_on_press_and_tuning_learns_only_while_grabbed() {
    let mut j = Journey::new();
    j.go(&[SHIFT_SAMPLE]);
    j.turn_to(Field::Setting("loop_rec"));
    j.go(&[PRESS]);
    assert_eq!(j.value("loop_rec"), 1.);
    j.go(&[PRESS]);
    assert_eq!(j.value("loop_rec"), 0.);
    j.go(&[SHIFT_SAMPLE]);
    j.turn_to(Field::Setting("tuning"));
    let tuning = j.value("tuning");
    j.go(&[(24, 64)]);
    assert_eq!(
        j.value("tuning"),
        tuning,
        "a played note never retunes while moving"
    );
    j.go(&[PRESS, (24, 64)]);
    assert_eq!(j.value("tuning"), 64.);
    j.go(&[PRESS]);
    assert!(!j.grabbed());
    assert_eq!(j.value("tuning"), 64.);
}

#[test]
fn clear_sample_asks_first_and_shift_load_asks_on_this_root() {
    let mut j = Journey::new();
    j.go(&[SHIFT_SAMPLE, KILL_SAMPLE]);
    match j.app().files_ui.layers.last() {
        Some(Layer::Confirm(c)) => assert_eq!(
            c.action,
            Verb::Run {
                op: "clear_sample",
                slot: 0
            }
        ),
        _ => panic!("Shift+Load asks first"),
    }
    j.turn(1);
    j.go(&[PRESS]);
    assert_eq!(j.app().patch().sample, "Empty sample.wav");
    assert_eq!(j.app().toast, "Cleared REEDS_04.wav in 01");
    assert!(!j.app().overlay_active());
    assert_eq!(j.app().tab, 3);
}

#[test]
fn copy_to_preset_arms_the_copy_and_copy_on_another_preset_pastes() {
    let mut j = Journey::new();
    j.go(&[SHIFT_SAMPLE, SHIFT_SAMPLE]);
    j.turn_to(Field::Copy);
    j.go(&[PRESS]);
    assert_eq!(
        j.app().copy_arm.map(|a| (a.from, a.kind)),
        Some((0, "copy_sample"))
    );
    assert_eq!(j.app().toast, "Copied REEDS_04.wav from 01");
    // A preset press returns to the main screen; the engine selects preset
    // 04, which the demo stands in for here.
    j.go(&[(6, 0), (10, 3)]);
    assert_eq!(j.app().tab, 0);
    j.p.app.parts[0] = 3;
    j.go(&[COPY]);
    assert_eq!(j.app().patches[3].sample, "REEDS_04.wav");
    assert_eq!(j.app().copy_arm, None);
    assert_eq!(j.app().toast, "Pasted REEDS_04.wav into 04");
    // Without an arm, Copy keeps its existing behavior.
    j.go(&[COPY]);
    assert!(matches!(j.app().modal, Some(Modal::Slots { .. })));
}

#[test]
fn the_record_and_sample_pages_draw_their_state() {
    let mut j = Journey::new();
    j.go(&[SHIFT_SAMPLE, PRESS]);
    let started = Instant::now() - Duration::from_millis(3210);
    j.p.app.sample_ui.demo_record_started = Some(started);
    j.p.app.render(&mut j.p.painter);
    let drawn_by = started.elapsed().as_secs_f32();
    let pixel = |p: &Painter, x: usize, y: usize| {
        let i = (y * 800 + x) * 4;
        [p.pixels[i], p.pixels[i + 1], p.pixels[i + 2]]
    };
    let red = RED.rgb(j.p.painter.theme, j.p.painter.brightness);
    // The red wiper sits at the recording position within the 8 s buffer.
    let wiper = (16..784)
        .find(|x| pixel(&j.p.painter, *x, 100) == red)
        .expect("a red wiper in the buffer");
    let at = |seconds: f32| 16. + seconds / 8. * 767.;
    assert!(
        (at(3.21) - 1.0..=at(drawn_by) + 1.0).contains(&(wiper as f32)),
        "wiper at {wiper}"
    );
    // The Record button reads Stop beside a red square while recording.
    assert_eq!(pixel(&j.p.painter, 36 + 3, 241 + 3), red);
    j.go(&[PRESS, SHIFT_SAMPLE]);
    assert_eq!(j.app().sample_ui.page, Page::Sample);
    assert_ne!(pixel(&j.p.painter, 36 + 3, 241 + 3), red);
}

#[test]
fn preset_press_cancels_clear_confirmation_and_returns_main() {
    for (kind, events) in [0, 1].into_iter().flat_map(|kind| {
        [vec![(6, 0), (10, 3)], vec![(10, 3)]]
            .into_iter()
            .map(move |events| (kind, events))
    }) {
        let mut j = Journey::new();
        j.go(&[SHIFT_SAMPLE, (29, kind)]);
        j.turn(1);
        let samples: Vec<_> = j.app().patches.iter().map(|p| p.sample.clone()).collect();
        // The engine changes the focused preset before dispatching its reports.
        j.p.app.parts[0] = 3;
        j.go(&events);
        assert!(!j.app().overlay_active());
        assert_eq!(j.app().tab, 0);
        j.go(&[PRESS]);
        assert_eq!(
            samples,
            j.app()
                .patches
                .iter()
                .map(|p| p.sample.clone())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn clear_refuses_a_changed_target_even_without_a_preset_report() {
    let mut j = Journey::new();
    j.go(&[SHIFT_SAMPLE, KILL_SAMPLE]);
    j.turn(1);
    let samples: Vec<_> = j.app().patches.iter().map(|p| p.sample.clone()).collect();
    // A part change in readback must not redirect the pending clear.
    j.p.app.part = 1;
    j.go(&[PRESS]);
    assert_eq!(
        samples,
        j.app()
            .patches
            .iter()
            .map(|p| p.sample.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(j.app().toast, "Clear cancelled: the focused preset changed");
    assert!(j.app().toast_error);
}
