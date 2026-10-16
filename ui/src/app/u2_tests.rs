//! file journeys replayed through the production panel dispatcher, counting
//! presses as panel input does: every button event counts, an
//! encoder turn (event 5) does not.
use super::overlay::{strip_item, strip_len, DataKind, Layer, Place, Role, SaveField};
use super::panel_tests::Panel;
use super::*;

/// Load; its value is the lit type, as observed on the instrument.
const LOAD: (u8, u8) = (1, LIT);
const LIT: u8 = 255;
const SAVE: (u8, u8) = (2, 1);
const PRESS: (u8, u8) = (7, 0);
const ESC: (u8, u8) = (14, 0);
const SHIFT_PRESS: (u8, u8) = (28, 0);
const SAMPLE: (u8, u8) = (33, 0);
const PERF: (u8, u8) = (35, 0);

/// Replays a journey and counts its presses.
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
        for (action, value) in events {
            let value = if (*action, *value) == LOAD {
                match self.p.app.files_ui.lit {
                    DataKind::Sample => 0,
                    DataKind::Patch => 1,
                    DataKind::Perf => 2,
                }
            } else {
                *value
            };
            self.p.events(&[(*action, value)]);
        }
    }
    /// Turn by `delta` detents in one report; turns are not counted.
    fn turn(&mut self, delta: i32) {
        if delta != 0 {
            self.p.events(&[(5, (delta.clamp(-127, 127) as i8) as u8)]);
        }
    }
    fn app(&self) -> &App {
        &self.p.app
    }
    fn top(&self) -> Option<&Layer> {
        self.p.app.files_ui.layers.last()
    }
    fn browser(&self) -> &overlay::Browser {
        match self.top() {
            Some(Layer::Browser(b)) => b,
            _ => panic!("no browser on top"),
        }
    }
    fn focused_key(&self) -> String {
        let b = self.browser();
        b.rows[b.list.focus].key.clone()
    }
    /// Turn the browser's focus onto the row with `key`.
    fn turn_to_row(&mut self, key: &str) {
        let b = self.browser();
        let target = b
            .rows
            .iter()
            .position(|r| r.key == key)
            .unwrap_or_else(|| panic!("no row {key}"));
        let delta = target as i32 - b.list.focus as i32;
        self.turn(delta);
        assert_eq!(self.focused_key(), key);
    }
    /// Turn the name editor's strip onto `item` (a character or control).
    fn strip_to(&mut self, item: &str) {
        let target = (0..strip_len())
            .find(|i| match strip_item(*i) {
                Ok(word) => word == item,
                Err(ch) => ch.to_string() == item,
            })
            .unwrap();
        let delta = target as i32 - self.p.app.files_ui.strip as i32;
        self.turn(delta);
        assert_eq!(self.p.app.files_ui.strip, target);
    }
    /// Shift+Escape: ESC while the engine reports Shift (observed 2026-10-12).
    fn shift_escape(&mut self) {
        self.p.app.shift = true;
        self.go(&[ESC]);
        self.p.app.shift = false;
    }
    fn screen(&self) -> i32 {
        self.p.app.screen_id()
    }
}

#[test]
fn j1_loads_a_sample_into_the_focused_preset_in_two_presses() {
    let mut j = Journey::new();
    j.p.app.files_ui.lit = DataKind::Sample;
    j.go(&[LOAD]);
    assert_eq!(j.screen(), 1, "the browser runs under screen id 1");
    // The browser opens on the current sample, in the Internal folder.
    assert_eq!(j.focused_key(), "REEDS_04.wav");
    j.turn(1);
    // Turning never loads: the next press acts on the focused row.
    assert_eq!(j.focused_key(), "SLOW_WATER.wav");
    assert_eq!(j.app().patch().sample, "REEDS_04.wav");
    j.go(&[PRESS]);
    assert_eq!(j.presses, 2);
    assert_eq!(j.app().patch().sample, "SLOW_WATER.wav");
    assert!(!j.app().overlay_active());
    assert_eq!(j.app().tab, 0, "loading returns to the main screen");
    assert_eq!(j.app().toast, "Loaded SLOW_WATER.wav into 01");
    assert_eq!(j.screen(), 0);

    // The next preset: Load reopens on the last-loaded file, t, Load.
    j.presses = 0;
    j.go(&[LOAD]);
    assert_eq!(j.focused_key(), "SLOW_WATER.wav");
    j.turn(-2);
    j.go(&[LOAD]);
    assert_eq!(j.presses, 2);
    assert_eq!(j.app().patch().sample, "PAPER_FOLD.wav");
}

#[test]
fn j1_costs_three_presses_when_sample_is_not_lit_and_one_per_folder() {
    let mut j = Journey::new();
    assert_eq!(
        j.app().files_ui.lit,
        DataKind::Patch,
        "Patch until a type button arrives"
    );
    j.go(&[LOAD]);
    assert_eq!(j.browser().kind, DataKind::Patch);
    j.go(&[SAMPLE]);
    assert_eq!(j.browser().kind, DataKind::Sample);
    // A folder level adds one press.
    j.turn_to_row("Field recordings/");
    j.go(&[PRESS]);
    assert!(matches!(&j.browser().place, Place::Folder(p) if p == "/Internal/Field recordings"));
    j.turn_to_row("RIVER.wav");
    j.go(&[PRESS]);
    assert_eq!(j.presses, 4);
    assert_eq!(j.app().patch().sample, "RIVER.wav");
    assert_eq!(j.app().files_ui.lit, DataKind::Sample);
    // Load reopens in that folder; Escape climbs to the folder's row.
    j.go(&[LOAD]);
    assert_eq!(j.focused_key(), "RIVER.wav");
    j.go(&[ESC]);
    assert_eq!(j.focused_key(), "Field recordings/");
    j.go(&[ESC]);
    assert_eq!(j.browser().place, Place::Locations);
    assert_eq!(j.focused_key(), "location0");
    j.go(&[ESC]);
    assert!(!j.app().overlay_active());
}

#[test]
fn j7_saves_the_patch_keeping_its_name_in_two_presses() {
    let mut j = Journey::new();
    j.p.app.patch_mut().dirty = true;
    j.go(&[SAVE]);
    match j.top() {
        Some(Layer::Save(s)) => {
            assert_eq!((s.kind, s.focus), (DataKind::Patch, SaveField::Save));
            assert_eq!(s.name, "Frozen reeds");
        }
        _ => panic!("Save opens the Save sheet"),
    }
    j.go(&[SAVE]);
    assert_eq!(j.presses, 2);
    assert!(!j.app().patch().dirty);
    assert_eq!(j.app().saved[0].name, "Frozen reeds");
    assert!(!j.app().overlay_active());
    assert_eq!(j.app().toast, "Saved Frozen reeds");
}

#[test]
fn j7_saves_the_patch_renamed_in_ten_presses() {
    let mut j = Journey::new();
    j.go(&[SAVE]);
    j.turn(-1);
    match j.top() {
        Some(Layer::Save(s)) => assert_eq!(s.focus, SaveField::Name),
        _ => panic!(),
    }
    j.go(&[PRESS]);
    assert_eq!(j.screen(), 2, "the name editor runs under screen id 2");
    j.strip_to("Clear");
    j.go(&[PRESS]);
    for ch in ["D", "u", "n", "e", "Space", "2"] {
        j.strip_to(ch);
        j.go(&[PRESS]);
    }
    match j.top() {
        Some(Layer::Name(n)) => assert_eq!(n.chars.iter().collect::<String>(), "Dune 2"),
        _ => panic!(),
    }
    // Save in the editor applies the name and writes.
    j.go(&[SAVE]);
    assert_eq!(j.presses, 10);
    assert_eq!(j.app().patch().name, "Dune 2");
    assert_eq!(j.app().saved[0].name, "Dune 2");
    assert!(!j.app().overlay_active());
}

fn fifty_performances(j: &mut Journey) {
    let patches = j.p.app.patches.clone();
    j.p.app.performance_library = (0..50)
        .map(|i| match i {
            11 => ("Field notes".to_string(), patches.clone()),
            i if i % 3 == 2 => ("Empty performance".to_string(), patches.clone()),
            i => (format!("Performance {:02}", i + 1), patches.clone()),
        })
        .collect();
    j.p.app.performance_parts = vec![([0, 1, 2, 3], [true, false, false, false], [80.; 4]); 50];
    j.p.app.performance_configs = vec![j.p.app.snapshots; 50];
    j.p.app.performance_snapshot = vec![0; 50];
}

#[test]
fn j7_recalls_performance_slot_twelve_in_three_presses() {
    let mut j = Journey::new();
    fifty_performances(&mut j);
    j.p.app.files_ui.lit = DataKind::Perf;
    j.go(&[LOAD]);
    assert_eq!(j.browser().place, Place::Slots);
    j.turn_to_row("11");
    j.go(&[LOAD]);
    match j.top() {
        Some(Layer::Confirm(c)) => {
            assert!(!c.on_verb, "a confirmation of lost data opens on Cancel");
            assert_eq!(c.lines[0], "Load slot 12");
        }
        _ => panic!("loading a performance asks first"),
    }
    // The panel verb that started it confirms. One frame names the load.
    j.p.stream.push(&[1, 2]);
    j.presses += 1;
    j.p.app
        .dispatch_panel_events(&mut j.p.painter, j.p.stream.drain());
    j.p.app.render(&mut j.p.painter);
    assert_eq!(j.app().toast, "Loading performance 12");
    assert_eq!(j.app().performance_slot, 0);
    j.p.app.run_pending();
    assert_eq!(j.presses, 3);
    assert_eq!(j.app().performance_slot, 11);
    assert_eq!(j.app().performance, "Field notes");
    assert_eq!(j.app().tab, 0);
    assert_eq!(j.app().toast, "Loaded performance 12 Field notes");
}

#[test]
fn j7_recall_from_another_location_costs_five_presses() {
    let mut j = Journey::new();
    fifty_performances(&mut j);
    j.p.app.files_ui.lit = DataKind::Perf;
    // Last used in the Internal folder.
    j.go(&[LOAD, ESC]);
    j.turn_to_row("location0");
    j.go(&[PRESS]);
    j.shift_escape();
    assert!(!j.app().overlay_active());
    j.presses = 0;
    j.go(&[LOAD]);
    assert!(matches!(&j.browser().place, Place::Folder(p) if p == "/Internal"));
    j.go(&[ESC]);
    j.turn_to_row("slots");
    j.go(&[PRESS]);
    j.turn_to_row("11");
    j.go(&[LOAD, LOAD]);
    assert_eq!(j.presses, 5);
    assert_eq!(j.app().performance_slot, 11);
}

#[test]
fn every_list_row_reaches_its_actions_in_two_presses_and_escape_keeps_the_row() {
    let mut j = Journey::new();
    j.p.app.files = (0..60).map(|i| format!("Sample {i:02}.wav")).collect();
    j.p.app.files.push("Folder/".into());
    j.p.app
        .folders
        .insert("/Internal/Folder".into(), vec!["INSIDE.wav".into()]);
    j.p.app.files_ui.lit = DataKind::Sample;
    j.go(&[LOAD]);
    for key in ["Folder/", "Sample 29.wav", "Sample 59.wav"] {
        j.turn_to_row(key);
        let top = j.browser().list.top;
        let actions = {
            j.go(&[SHIFT_PRESS]);
            match j.top() {
                Some(Layer::Sheet(s)) => {
                    assert_eq!(s.focus, 0, "the primary action is focused");
                    s.items.iter().map(|i| i.label.clone()).collect::<Vec<_>>()
                }
                _ => panic!("Shift+press opens the row's sheet"),
            }
        };
        let expected: &[&str] = if key == "Folder/" {
            &["Open", "Fill presets", "Rename", "Delete"]
        } else {
            &["Load", "Rename", "Delete"]
        };
        assert_eq!(actions, expected);
        j.go(&[ESC]);
        assert_eq!(j.focused_key(), key, "Escape returns to the row");
        assert_eq!(j.browser().list.top, top, "and to its scroll position");
        for (i, action) in actions.iter().enumerate() {
            j.presses = 0;
            j.go(&[SHIFT_PRESS]);
            j.turn(i as i32);
            j.go(&[PRESS]);
            assert_eq!(j.presses, 2, "{action} on {key}");
            match (action.as_str(), j.top()) {
                ("Open", Some(Layer::Browser(b))) => {
                    assert!(matches!(&b.place, Place::Folder(p) if p.ends_with("/Folder")));
                    j.go(&[ESC]);
                }
                ("Fill presets" | "Delete", Some(Layer::Confirm(c))) => {
                    assert!(!c.on_verb);
                    j.go(&[ESC]);
                }
                ("Rename", Some(Layer::Name(n))) => {
                    assert_eq!(
                        n.chars.iter().collect::<String>(),
                        key.trim_end_matches('/')
                    );
                    j.go(&[ESC]);
                }
                ("Load", None) => {
                    assert_eq!(j.app().patch().sample, key);
                    j.go(&[LOAD]);
                    break;
                }
                (action, _) => panic!("{action} on {key} did not open its next step"),
            }
            assert_eq!(j.focused_key(), key);
        }
    }
    // Load also acts on the focused row directly: one press.
    j.turn_to_row("Sample 07.wav");
    j.presses = 0;
    j.go(&[LOAD]);
    assert_eq!(j.presses, 1);
    assert_eq!(j.app().patch().sample, "Sample 07.wav");
}

#[test]
fn a_press_with_shift_held_also_opens_the_sheet() {
    let mut j = Journey::new();
    j.p.app.files_ui.lit = DataKind::Sample;
    j.go(&[LOAD]);
    j.p.app.shift = true;
    j.go(&[PRESS]);
    assert!(matches!(j.top(), Some(Layer::Sheet(_))));
    // In the sheet a Shift-held press is an ordinary press.
    j.go(&[PRESS]);
    j.p.app.shift = false;
    assert!(!j.app().overlay_active());
}

#[test]
fn scrolling_never_loads_and_moves_the_view_only_at_the_edge() {
    let mut j = Journey::new();
    j.p.app.files = (0..60).map(|i| format!("Sample {i:02}.wav")).collect();
    j.p.app.files_ui.lit = DataKind::Sample;
    j.go(&[LOAD]);
    assert_eq!((j.browser().list.focus, j.browser().list.top), (0, 0));
    for step in 1..60 {
        j.turn(1);
        let b = j.browser();
        assert_eq!(b.list.focus, step);
        // Focus moves within the visible rows; the view follows one row at the edge.
        assert_eq!(
            b.list.top,
            step.saturating_sub(components::VISIBLE_ROWS - 1)
        );
    }
    j.turn(-3);
    assert_eq!(
        j.browser().list.top,
        52,
        "turning back inside the view does not scroll"
    );
    assert_eq!(j.app().patch().sample, "REEDS_04.wav");
    assert!(j.app().patch().params == Patch::new(0).params);
}

#[test]
fn every_container_remembers_its_focus() {
    let mut j = Journey::new();
    j.p.app.files_ui.lit = DataKind::Sample;
    j.go(&[LOAD]);
    j.turn_to_row("NIGHT_TRAIN.wav");
    j.turn_to_row("Field recordings/");
    j.go(&[PRESS]);
    j.turn_to_row("RIVER.wav");
    j.go(&[ESC]);
    assert_eq!(j.focused_key(), "Field recordings/");
    j.go(&[PRESS]);
    assert_eq!(j.focused_key(), "RIVER.wav");
    j.go(&[ESC, ESC]);
    j.turn_to_row("location2");
    j.go(&[PRESS]);
    j.turn_to_row("STRINGS.wav");
    j.go(&[ESC]);
    assert_eq!(j.focused_key(), "location2");
    j.go(&[PRESS]);
    assert_eq!(j.focused_key(), "STRINGS.wav");
}

#[test]
fn performance_saves_ask_only_when_they_replace_other_saved_data() {
    let mut j = Journey::new();
    fifty_performances(&mut j);
    j.p.app.files_ui.lit = DataKind::Perf;
    // Back where it came from: no question.
    j.go(&[SAVE, SAVE]);
    assert!(!j.app().overlay_active());
    assert_eq!(j.app().toast, "Saved performance 01 Night studies");
    // Into an empty slot (03): no question.
    j.go(&[SAVE]);
    j.turn(-1);
    j.go(&[PRESS]);
    assert_eq!(
        j.browser().list.focus,
        0,
        "the picker opens on the destination"
    );
    j.turn_to_row("2");
    j.go(&[PRESS]);
    match j.top() {
        Some(Layer::Save(s)) => assert_eq!((s.slot, s.focus), (Some(2), SaveField::Save)),
        _ => panic!(),
    }
    j.go(&[SAVE]);
    assert_eq!(j.app().performance_slot, 2);
    // Over slot 12, another performance: Replace, on Cancel; Save confirms.
    j.go(&[SAVE]);
    j.turn(-1);
    j.go(&[PRESS]);
    j.turn_to_row("11");
    j.go(&[SAVE]);
    j.go(&[SAVE]);
    match j.top() {
        Some(Layer::Confirm(c)) => {
            assert_eq!(
                c.lines,
                [
                    "Replace slot 12",
                    "Field notes",
                    "with the working performance"
                ]
            );
            assert!(!c.on_verb);
        }
        _ => panic!("replacing another slot asks"),
    }
    j.go(&[PRESS]);
    assert!(
        matches!(j.top(), Some(Layer::Save(_))),
        "Cancel returns to the sheet"
    );
    assert_eq!(j.app().performance_slot, 2);
    j.go(&[SAVE, SAVE]);
    assert_eq!(j.app().performance_slot, 11);
    assert!(!j.app().overlay_active());
}

#[test]
fn a_sample_save_over_an_existing_file_asks_and_the_sheet_switches_type() {
    let mut j = Journey::new();
    j.go(&[SAVE, SAMPLE]);
    match j.top() {
        Some(Layer::Save(s)) => {
            assert_eq!((s.kind, s.name.as_str()), (DataKind::Sample, "REEDS_04"));
            assert_eq!(s.folder, "/Internal");
        }
        _ => panic!(),
    }
    j.go(&[SAVE]);
    match j.top() {
        Some(Layer::Confirm(c)) => {
            assert_eq!(c.lines[0], "Replace REEDS_04.wav");
            assert_eq!(c.keycap, "Save");
        }
        _ => panic!("an existing file is not replaced silently"),
    }
    j.go(&[ESC]);
    // A new name writes without asking.
    j.turn(-2);
    j.go(&[PRESS]);
    j.strip_to("2");
    j.go(&[PRESS]);
    j.go(&[SAVE]);
    assert!(!j.app().overlay_active());
    assert!(j.app().files.contains(&"REEDS_042.wav".to_string()));
    assert_eq!(j.app().patch().sample, "REEDS_042.wav");
}

#[test]
fn the_name_editor_cancels_deletes_jumps_groups_and_applies() {
    let mut j = Journey::new();
    j.go(&[SAVE]);
    j.turn(-1);
    j.go(&[PRESS]);
    // Shift+Escape deletes the character before the cursor.
    j.shift_escape();
    j.shift_escape();
    match j.top() {
        Some(Layer::Name(n)) => assert_eq!(n.chars.iter().collect::<String>(), "Frozen ree"),
        _ => panic!(),
    }
    // Escape cancels the edit; the sheet keeps its name.
    j.go(&[ESC]);
    match j.top() {
        Some(Layer::Save(s)) => assert_eq!(s.name, "Frozen reeds"),
        _ => panic!(),
    }
    // With Shift held, a turn jumps to the next group.
    j.go(&[PRESS]);
    j.strip_to("A");
    j.p.app.shift = true;
    j.turn(10);
    assert_eq!(strip_item(j.app().files_ui.strip), Err('a'));
    j.turn(1);
    assert_eq!(strip_item(j.app().files_ui.strip), Err('0'));
    j.turn(-1);
    assert_eq!(strip_item(j.app().files_ui.strip), Err('a'));
    j.p.app.shift = false;
    // Done applies to the sheet without writing.
    j.go(&[PRESS]);
    j.strip_to("Done");
    j.go(&[PRESS]);
    match j.top() {
        Some(Layer::Save(s)) => {
            assert_eq!(
                (s.name.as_str(), s.focus),
                ("Frozen reedsa", SaveField::Save)
            )
        }
        _ => panic!(),
    }
    assert_eq!(j.app().saved[0].name, "Frozen reeds");
}

#[test]
fn a_usb_keyboard_types_at_the_cursor() {
    let mut j = Journey::new();
    j.go(&[SAVE]);
    j.turn(-1);
    j.go(&[PRESS]);
    assert!(j.p.app.name_text(" 2"));
    let events = j.p.app.overlay_key("Left").unwrap();
    assert!(events.is_empty(), "arrow keys move the cursor directly");
    assert!(j.p.app.name_text("x"));
    match j.top() {
        Some(Layer::Name(n)) => assert_eq!(n.chars.iter().collect::<String>(), "Frozen reeds x2"),
        _ => panic!(),
    }
    assert_eq!(j.p.app.overlay_key("Return"), Some(vec![]));
    match j.top() {
        Some(Layer::Save(s)) => assert_eq!(s.name, "Frozen reeds x2"),
        _ => panic!(),
    }
}

#[test]
fn shift_load_fills_presets_only_from_a_sample_folder() {
    let mut j = Journey::new();
    j.p.app.files_ui.lit = DataKind::Sample;
    j.go(&[LOAD]);
    j.go(&[(29, 0)]);
    assert!(
        matches!(j.top(), Some(Layer::Browser(_))),
        "KILL on a file does nothing"
    );
    assert_eq!(j.app().patch().sample, "REEDS_04.wav", "and never clears");
    j.turn_to_row("Field recordings/");
    j.go(&[(29, 0)]);
    match j.top() {
        Some(Layer::Confirm(c)) => {
            assert_eq!(c.lines, ["Fill presets 01-02", "from Field recordings"])
        }
        _ => panic!("KILL on a sample folder asks to fill"),
    }
    j.go(&[LOAD]);
    assert_eq!(j.app().patches[0].sample, "BIRDS_AT_DAWN.wav");
    assert_eq!(j.app().patches[1].sample, "RIVER.wav");
    assert_eq!(j.app().toast, "Filled presets 01-02");
}

#[test]
fn the_patch_browser_lists_patches_and_keeps_the_sample() {
    let mut j = Journey::new();
    j.p.app.files = vec!["Other.wav".into(), "Patch.GRP".into(), "Folder/".into()];
    j.go(&[LOAD]);
    let keys: Vec<_> = j.browser().rows.iter().map(|r| r.key.clone()).collect();
    assert_eq!(keys, ["Folder/", "Patch.GRP"]);
    assert_eq!(j.browser().rows[1].label, "Patch");
    j.turn_to_row("Patch.GRP");
    j.go(&[PRESS]);
    assert_eq!(j.app().patch().name, "Patch");
    assert_eq!(j.app().patch().sample, "REEDS_04.wav");
    assert_eq!(j.app().tab, 0);
}

#[test]
fn all_fifty_slots_are_reachable_and_visible() {
    let mut j = Journey::new();
    fifty_performances(&mut j);
    j.go(&[PERF, LOAD]);
    j.turn(60);
    let b = j.browser();
    assert_eq!((b.list.focus, b.rows.len()), (49, 50));
    assert!(b.list.focus >= b.list.top && b.list.focus < b.list.top + components::VISIBLE_ROWS);
    assert!(matches!(b.rows[2].role, Role::Slot { empty: true, .. }));
    j.go(&[LOAD]);
    j.go(&[ESC]);
    assert_eq!(j.focused_key(), "49", "Cancel keeps the row");
}

#[test]
fn load_and_save_open_from_a_menu_and_escape_returns_there() {
    let mut j = Journey::new();
    j.go(&[(13, 0), PRESS]);
    let (tab, group) = (j.app().tab, j.app().group);
    assert_ne!(tab, 0);
    j.go(&[LOAD]);
    assert!(j.app().overlay_active());
    j.go(&[ESC, ESC]);
    assert!(!j.app().overlay_active());
    assert_eq!((j.app().tab, j.app().group), (tab, group));
    j.go(&[SAVE, ESC]);
    assert!(!j.app().overlay_active());
    assert_eq!((j.app().tab, j.app().group), (tab, group));
    // Shift+Escape goes to the main screen from anywhere.
    j.go(&[SAVE]);
    j.shift_escape();
    assert_eq!(j.app().tab, 0);
    // A preset button: MAIN, then SET_PRESET. The browser keeps the new
    // preset as its destination; a menu closes to the main screen.
    j.go(&[LOAD, (6, 0), (10, 0)]);
    assert!(j.app().overlay_active());
    j.go(&[ESC, ESC, (13, 0), PRESS]);
    assert!(!j.app().overlay_active() && j.app().tab != 0);
    j.go(&[(10, 0)]);
    assert_eq!(j.app().tab, 0);
    // A menu chord leaves the browser for that menu.
    j.go(&[LOAD, (12, 0)]);
    assert!(!j.app().overlay_active());
    assert_eq!(j.app().tab, 4, "the Perf root (U4)");
}

#[test]
fn load_follows_its_value_and_save_follows_the_last_type_button() {
    let mut j = Journey::new();
    // Observed: Load's value is the lit LED.
    j.go(&[(1, 2)]);
    assert_eq!(j.browser().kind, DataKind::Perf);
    assert_eq!(j.app().files_ui.lit, DataKind::Perf);
    j.shift_escape();
    // Unobserved for Save: the value is not trusted; the last type button is.
    j.go(&[SAMPLE, (2, 1)]);
    match j.top() {
        Some(Layer::Save(s)) => assert_eq!(s.kind, DataKind::Sample),
        _ => panic!(),
    }
    // A preset change keeps the sheet open with the new destination.
    j.go(&[(6, 0), (10, 0)]);
    assert!(matches!(j.top(), Some(Layer::Save(_))));
}

#[test]
fn legacy_factory_inside_internal_stays_its_own_read_only_location() {
    use super::overlay::location_containing;
    // A legacy data root: Internal is the root and Factory lies inside it.
    let roots = || {
        [
            (0, "/media/internal".to_string()),
            (1, "/media/STICK".to_string()),
            (2, "/media/internal/factory".to_string()),
        ]
        .into_iter()
    };
    let at = |path: &str| location_containing(path, roots()).map(|(i, _)| i);
    assert_eq!(at("/media/internal/factory"), Some(2));
    assert_eq!(at("/media/internal/factory/default2017/000"), Some(2));
    assert_eq!(at("/media/internal/performance01"), Some(0));
    assert_eq!(at("/media/internal/factoryish"), Some(0));
    assert_eq!(at("/media/internal"), Some(0));
    assert_eq!(at("/media/STICK/perf"), Some(1));
    assert_eq!(at("/tmp"), None);
}

#[test]
fn sample_save_confirms_the_canonical_wav_target_for_import_extensions() {
    for name in [
        "take.ogg",
        "take.flac",
        "take.aiff",
        "take.wave",
        "take.WAV",
    ] {
        let mut j = Journey::new();
        let target = crate::device::storage::sample_file_name(name).unwrap();
        assert_eq!(
            target,
            if name.ends_with("WAV") {
                name.into()
            } else {
                format!("{name}.wav")
            }
        );
        j.p.app.files.push(target.clone());
        if name != target {
            j.p.app.files.push(name.to_string());
        }
        j.go(&[SAMPLE, SAVE]);
        j.turn(-2);
        j.go(&[PRESS]);
        j.strip_to("Clear");
        j.go(&[PRESS]);
        assert!(j.p.app.name_text(name));
        // Save from the name editor applies the name and starts the write.
        j.go(&[SAVE]);
        match j.top() {
            Some(Layer::Confirm(c)) => {
                assert_eq!(c.lines[0], format!("Replace {target}"));
                assert!(!c.on_verb);
            }
            _ => panic!("the actual WAV target needs confirmation"),
        }
        j.go(&[ESC]);
        assert!(matches!(j.top(), Some(Layer::Save(_))));
        assert_eq!(j.app().patch().sample, "REEDS_04.wav");
        j.go(&[SAVE, SAVE]);
        assert!(!j.app().overlay_active());
        assert_eq!(j.app().patch().sample, target);
        assert_eq!(j.app().toast, format!("Saved {target}"));
        assert!(j.app().files.contains(&name.to_string()));
        assert_eq!(j.app().files.iter().filter(|f| **f == target).count(), 1);
    }
}

#[test]
fn confirmed_unmount_returns_the_browser_to_locations_without_usb() {
    let mut j = Journey::new();
    j.go(&[SAMPLE, LOAD, ESC]);
    j.turn_to_row("location1");
    // The demo omits the native USB action. Inject its confirmation while
    // retaining the real browser, then exercise the production panel branch.
    j.p.app
        .files_ui
        .layers
        .push(Layer::Confirm(overlay::Confirm {
            lines: vec!["Unmount USB".into()],
            loss: "Unfinished writes end first".into(),
            verb: "Unmount".into(),
            keycap: "",
            on_verb: false,
            action: overlay::Verb::Unmount,
        }));
    j.turn(1);
    j.go(&[PRESS]);
    assert_eq!(j.browser().place, Place::Locations);
    assert!(!j.browser().rows.iter().any(|r| r.key == "location1"));
    assert_eq!(j.app().directory, "/Internal");
    assert!(!j.app().usb_mounted);
    assert_eq!(j.app().toast, "USB drive unmounted");
}
