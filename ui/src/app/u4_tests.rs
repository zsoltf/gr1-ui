//! Perf journeys replayed through the production panel dispatcher, counting
//! presses as panel input does: every button event counts, an
//! encoder turn (event 5) does not, and holding Shift is one press. Panel
//! presses the engine owns on the main screen (part enable, part focus, bank
//! and preset buttons) are stood in for by setting the desktop demo state the
//! engine would produce; each such substitution is named where it happens.
use super::panel_tests::Panel;
use super::perf_root::{Layer, FILES, PARTS, PERFORMANCES, PRESETS, SYSTEM};
use super::*;

const LOAD: (u8, u8) = (1, 0);
const SAVE: (u8, u8) = (2, 1);
const COPY: (u8, u8) = (4, 0);
const PRESS: (u8, u8) = (7, 0);
const ESC: (u8, u8) = (14, 0);
const PERF: (u8, u8) = (12, 0);
const PERF_LIT: (u8, u8) = (35, 0);
const SHIFT_PRESS: (u8, u8) = (28, 0);

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
        self.p.events(events);
    }
    /// One report of `delta` detents; turns are not counted.
    fn turn(&mut self, delta: i32) {
        if delta != 0 {
            self.p.events(&[(5, (delta.clamp(-127, 127) as i8) as u8)]);
        }
    }
    /// Shift goes down (one press) and stays held.
    fn hold_shift(&mut self) {
        self.presses += 1;
        self.p.app.shift = true;
        self.p.events(&[]);
    }
    fn release_shift(&mut self) {
        self.p.app.shift = false;
        self.p.events(&[]);
    }
    /// Shift+Escape: ESC while the engine reports Shift (observed 2026-10-12).
    fn shift_escape(&mut self) {
        self.p.app.shift = true;
        self.go(&[ESC]);
        self.p.app.shift = false;
        self.p.events(&[]);
    }
    fn app(&self) -> &App {
        &self.p.app
    }
    fn app_mut(&mut self) -> &mut App {
        &mut self.p.app
    }
    fn sheet(&self) -> Vec<String> {
        match self.app().perf.layers.last() {
            Some(Layer::Sheet(s)) => s.items.iter().map(|i| i.label.clone()).collect(),
            _ => panic!("no sheet open: {}", self.app().perf_context()),
        }
    }
    fn ask(&self) -> (Vec<String>, bool, &'static str) {
        match self.app().perf.layers.last() {
            Some(Layer::Ask(a)) => (a.lines.clone(), a.on_verb, a.keycap),
            _ => panic!("no confirmation open: {}", self.app().perf_context()),
        }
    }
    fn file_key(&self) -> String {
        let f = &self.app().perf.files;
        f.rows[f.list.focus].key.clone()
    }
    fn turn_to_file(&mut self, key: &str) {
        let f = &self.app().perf.files;
        let target = f
            .rows
            .iter()
            .position(|r| r.key == key)
            .unwrap_or_else(|| panic!("no row {key}"));
        let delta = target as i32 - f.list.focus as i32;
        self.turn(delta);
        assert_eq!(self.file_key(), key);
    }
    /// Run frames until the running paste ends; the notices it showed.
    fn finish_paste(&mut self) -> Vec<String> {
        let mut notices = vec![];
        for _ in 0..50 {
            if self.app().perf.files.job.is_none() {
                return notices;
            }
            notices.push(self.app().toast.clone());
            self.p.app.perf_frame();
            self.p.app.render(&mut self.p.painter);
        }
        panic!("the paste did not end");
    }
}

/// A desktop demo library with all 50 slots: 12 is Field notes, 20 holds a
/// saved performance, every third one is empty.
fn fifty_performances(j: &mut Journey) {
    let patches = j.app().patches.clone();
    let app = j.app_mut();
    app.performance_library = (0..50)
        .map(|i| match i {
            0 => ("Night studies".to_string(), patches.clone()),
            11 => ("Field notes".to_string(), patches.clone()),
            19 => ("Drone sketches".to_string(), patches.clone()),
            i if i % 3 == 2 => ("Empty performance".to_string(), patches.clone()),
            i => (format!("Performance {:02}", i + 1), patches.clone()),
        })
        .collect();
    app.performance_parts = vec![([0, 1, 2, 3], [true, false, false, false], [80.; 4]); 50];
    app.performance_configs = vec![app.snapshots; 50];
    app.performance_snapshot = vec![0; 50];
}

#[test]
fn the_perf_root_opens_with_shift_perf_under_screen_five_and_resumes_its_page() {
    let mut j = Journey::new();
    j.go(&[PERF]);
    assert_eq!(j.app().tab, 4);
    assert_eq!(
        j.app().perf.page,
        PERFORMANCES,
        "a first visit lands on the first page"
    );
    assert_eq!(j.app().screen_id(), 5);
    // The same chord steps to the next page, entering it, not the strip.
    j.go(&[PERF]);
    assert_eq!(j.app().perf.page, PRESETS);
    assert!(!j.app().perf.strip);
    // Escape goes to the strip; turning shows the page at once; Select enters.
    j.go(&[ESC]);
    assert!(j.app().perf.strip);
    j.turn(2);
    assert_eq!(j.app().perf.page, FILES);
    j.turn(-1);
    j.go(&[PRESS]);
    assert_eq!(j.app().perf.page, PARTS);
    assert!(!j.app().perf.strip);
    j.turn(3);
    assert_eq!(j.app().perf.part_row, 3);
    // Escape from the page and Select again: the same row keeps focus.
    j.go(&[ESC, PRESS]);
    assert_eq!(j.app().perf.part_row, 3);
    // Escape twice: the main screen; the next Shift+Perf resumes Parts, row 3.
    j.go(&[ESC, ESC]);
    assert_eq!(j.app().tab, 0);
    assert_eq!(j.app().screen_id(), 0);
    j.go(&[PERF]);
    assert_eq!((j.app().perf.page, j.app().perf.part_row), (PARTS, 3));
    // A preset button inside the root returns to the main screen, as stock
    // does; MAIN precedes SET_PRESET.
    j.go(&[(6, 0), (10, 0)]);
    assert_eq!(j.app().tab, 0);
    // Shift+Escape returns to the main screen from a page.
    j.go(&[PERF]);
    j.shift_escape();
    assert_eq!(j.app().tab, 0);
    // The other roots' chords leave the Perf root.
    j.go(&[PERF, (13, 0)]);
    assert_ne!(j.app().tab, 4);
}

#[test]
fn j5_adds_part_two_with_preset_twelve_at_70_on_channel_2_in_ten_presses() {
    let mut j = Journey::new();
    // The last Perf page used was Parts.
    j.go(&[PERF, ESC]);
    j.turn(2);
    j.go(&[PRESS]);
    j.shift_escape();
    j.app_mut().system.insert("midi2".into(), 1.);
    j.presses = 0;
    // Engine-owned, three panel presses: Shift+Preset 2 enables part 2,
    // Select focuses it on the main screen, Bank 2 and Preset 4 choose
    // preset 12. The demo state stands in for the engine's result.
    j.presses += 3;
    j.app_mut().enabled[1] = true;
    j.app_mut().part = 1;
    j.app_mut().parts[1] = 11;
    // Shift+Perf resumes the Parts page.
    j.go(&[PERF]);
    assert_eq!(j.app().perf.page, PARTS);
    j.turn(2 - j.app().perf.part_row as i32);
    assert_eq!(j.app().perf.part_row, 2);
    // Select enters the row on Level: the next press grabs Level.
    j.go(&[PRESS]);
    assert_eq!(j.app().perf.inside, Some(2));
    j.go(&[PRESS]);
    assert_eq!(
        j.app().perf.grab.map(|g| g.field),
        Some(perf_root::Field::PartLevel(1))
    );
    // A number reaches the engine while turning.
    j.turn(-10);
    assert_eq!(j.app().volumes[1], 70.);
    j.go(&[PRESS]);
    assert!(j.app().perf.grab.is_none());
    j.turn(1);
    j.go(&[PRESS]);
    j.turn(1);
    assert_eq!(j.app().get("midi2"), 2.);
    j.go(&[PRESS]);
    j.shift_escape();
    assert_eq!(j.presses, 10);
    assert_eq!(j.app().tab, 0);
    assert_eq!(j.app().volumes[1], 70.);
    assert_eq!(j.app().get("midi2"), 2.);
    assert_eq!(j.app().parts[1], 11);
    assert!(j.app().enabled[1]);
}

#[test]
fn j8_saves_the_working_performance_into_slot_twelve_in_four_presses() {
    let mut j = Journey::new();
    fifty_performances(&mut j);
    // From the main screen through the Save sheet (file): Save (Perf lit);
    // t, Select on Slot; t, Select on slot 12; Save.
    j.go(&[PERF_LIT]);
    j.presses = 0;
    j.go(&[SAVE]);
    j.turn(-1);
    j.go(&[PRESS]);
    let focus = match j.app().files_ui.layers.last() {
        Some(overlay::Layer::Browser(b)) => b.list.focus as i32,
        _ => panic!("the slot picker opens"),
    };
    j.turn(11 - focus);
    j.go(&[PRESS]);
    j.go(&[SAVE]);
    // Slot 12 holds another performance: a second Save confirms (3.6).
    assert_eq!(j.presses, 4);
    assert!(j.app().overlay_active());
    j.go(&[SAVE]);
    assert_eq!(j.app().performance_slot, 11);
    // From the Performances page the panel's Save acts on the focused slot:
    // an empty slot takes it without a question.
    j.go(&[PERF]);
    j.turn(5 - j.app().perf.slots.focus as i32);
    j.presses = 0;
    j.go(&[SAVE]);
    assert_eq!(j.presses, 1);
    assert_eq!(j.app().performance_slot, 5);
    assert_eq!(j.app().toast, "Saved performance 06 Night studies");
    assert_eq!(j.app().tab, 4, "saving from the page stays on the page");
}

#[test]
fn j8_initializes_slot_twenty_in_four_presses() {
    let mut j = Journey::new();
    fifty_performances(&mut j);
    j.go(&[PERF]);
    assert_eq!(j.app().perf.page, PERFORMANCES);
    j.presses = 1;
    j.turn(19);
    j.go(&[PRESS]);
    assert_eq!(
        j.sheet(),
        [
            "Load",
            "Save working here",
            "Rename",
            "Initialize",
            "Initialize and load"
        ]
    );
    j.turn(3);
    j.go(&[PRESS]);
    let (lines, on_verb, _) = j.ask();
    assert_eq!(lines, ["Initialize slot 20", "Drone sketches"]);
    assert!(!on_verb, "a confirmation of lost data opens on Cancel");
    j.turn(1);
    j.go(&[PRESS]);
    assert_eq!(j.presses, 4);
    assert_eq!(j.app().toast, "Initialized slot 20");
    assert_eq!(j.app().performance_library[19].0, "New");
    assert_eq!(j.app().performance_slot, 0, "the working performance stays");
    assert_eq!(j.app().perf.slots.focus, 19, "focus stays on the row");
}

#[test]
fn j8_copies_preset_seven_to_all_presets_in_four_presses() {
    let mut j = Journey::new();
    j.go(&[PERF_LIT]);
    // Preset 07: MAIN and SET_PRESET from one press; the engine selects the
    // preset for the focused part, which the demo state stands in for.
    j.go(&[(6, 0), (10, 0)]);
    j.presses -= 1;
    j.app_mut().parts[0] = 6;
    j.app_mut().patches[6].name = "Frozen reeds".into();
    j.go(&[COPY]);
    let (lines, on_verb, keycap) = j.ask();
    assert_eq!(
        lines,
        [
            "Copy 07 to all presets",
            "Frozen reeds",
            "into the other 31"
        ]
    );
    assert!(!on_verb);
    assert_eq!(keycap, "Copy");
    assert_eq!(j.app().screen_id(), 5);
    // The panel's Copy that started it confirms.
    j.go(&[COPY]);
    assert_eq!(j.presses, 4);
    assert!(j.app().patches.iter().all(|p| p.name == "Frozen reeds"));
    assert_eq!(j.app().toast, "Copied 07 Frozen reeds to all presets");
    assert_eq!(j.app().tab, 0);
    assert!(j.app().perf.layers.is_empty());
    assert_eq!(j.app().screen_id(), 0);
}

fn backup_folders(j: &mut Journey) {
    let app = j.app_mut();
    app.files = vec![
        "Drone samples/".into(),
        "Field samples/".into(),
        "Patches/".into(),
        "performance01/".into(),
        "performance02/".into(),
        "performance03/".into(),
        "REEDS_04.wav".into(),
    ];
    for (folder, contents) in [
        ("Drone samples", vec!["LOW.wav", "HIGH.wav"]),
        ("Field samples", vec!["RAIN.wav"]),
        ("Patches", vec![]),
        ("performance01", vec!["performance.grm", "000/"]),
        ("performance02", vec!["performance.grm"]),
        ("performance03", vec!["performance.grm", "000/"]),
    ] {
        app.folders.insert(
            format!("/Internal/{folder}"),
            contents.into_iter().map(String::from).collect(),
        );
    }
    app.folders.insert(
        "/Internal/performance03/000".into(),
        vec!["SAMPLE.wav".into(), "empty.grp".into()],
    );
    // The last Perf page used was Files.
    app.perf.page = FILES;
}

#[test]
fn j9_backs_up_performance03_and_two_sample_folders_to_usb_in_eight_presses() {
    let mut j = Journey::new();
    backup_folders(&mut j);
    j.go(&[PERF]);
    assert_eq!(j.app().perf.page, FILES);
    assert_eq!(j.app().directory, "/Internal", "Files resumes in Internal");
    j.turn_to_file("performance03/");
    j.hold_shift();
    j.release_shift();
    j.turn_to_file("Drone samples/");
    j.hold_shift();
    // With Shift held one detent arrives as 10: one row, selected.
    j.turn(10);
    assert_eq!(j.file_key(), "Field samples/");
    j.release_shift();
    let selected: Vec<_> = j.app().perf.files.selection.iter().cloned().collect();
    assert_eq!(
        selected,
        ["Drone samples/", "Field samples/", "performance03/"]
    );
    j.go(&[COPY]);
    assert_eq!(j.app().clipboard.len(), 3);
    assert_eq!(j.app().toast, "Copied 3 items");
    j.go(&[ESC]);
    assert!(j.app().perf.files.at_locations);
    assert!(j.app().perf.files.selection.is_empty());
    assert_eq!(
        j.file_key(),
        "location0",
        "Escape keeps the row it came from"
    );
    j.turn_to_file("location1");
    j.go(&[PRESS]);
    assert_eq!(j.app().directory, "/USB");
    j.go(&[COPY]);
    let (lines, on_verb, keycap) = j.ask();
    assert_eq!(lines, ["Copy 3 items", "into USB"]);
    assert!(!on_verb);
    assert_eq!(keycap, "Copy");
    j.go(&[COPY]);
    assert_eq!(j.presses, 8);
    // The copy runs a slice per frame and counts its progress (3.6).
    assert_eq!(j.app().toast, "Copying 1 of 3");
    let notices = j.finish_paste();
    assert!(
        notices.contains(&"Copying 2 of 3".to_string()),
        "{notices:?}"
    );
    assert_eq!(j.app().toast, "Copied 3 items into USB");
    for name in ["Drone samples/", "Field samples/", "performance03/"] {
        assert!(j.app().files.contains(&name.to_string()), "{name}");
    }
    assert_eq!(
        j.app().folders["/USB/performance03/000"],
        ["SAMPLE.wav", "empty.grp"]
    );
    assert!(j.app().clipboard.is_empty());
}

#[test]
fn escape_cancels_a_running_copy_and_keeps_what_finished() {
    let mut j = Journey::new();
    backup_folders(&mut j);
    j.go(&[PERF]);
    j.turn_to_file("Drone samples/");
    j.go(&[SHIFT_PRESS]);
    j.turn(j.sheet().iter().position(|l| l == "Select all").unwrap() as i32);
    j.go(&[PRESS, COPY, ESC]);
    j.turn_to_file("location1");
    j.go(&[PRESS, COPY, COPY]);
    j.p.app.perf_frame();
    j.p.app.perf_frame();
    j.go(&[ESC]);
    assert!(j.app().perf.files.job.is_none());
    assert_eq!(j.app().toast, "Copy cancelled after 2 of 7");
    assert_eq!(
        j.app().directory,
        "/USB",
        "Escape cancelled; it did not climb"
    );
}

#[test]
fn files_selects_a_folder_and_a_range_with_shift_and_toggles_a_file_with_a_press() {
    let mut j = Journey::new();
    backup_folders(&mut j);
    j.go(&[PERF]);
    // Holding Shift selects the focused folder; press would open it.
    j.turn_to_file("Patches/");
    j.hold_shift();
    j.release_shift();
    assert!(j.app().perf.files.selection.contains("Patches/"));
    // A range: hold Shift and turn three rows, one report per detent.
    j.turn_to_file("performance01/");
    j.hold_shift();
    j.turn(10);
    j.turn(10);
    j.turn(10);
    j.release_shift();
    assert_eq!(j.file_key(), "REEDS_04.wav");
    assert_eq!(j.app().perf.files.selection.len(), 5);
    // Without Shift a turn only moves; a press on a file toggles it.
    j.turn(-1);
    assert_eq!(j.app().perf.files.selection.len(), 5);
    j.turn(1);
    j.go(&[PRESS]);
    assert!(!j.app().perf.files.selection.contains("REEDS_04.wav"));
    j.go(&[PRESS]);
    assert!(j.app().perf.files.selection.contains("REEDS_04.wav"));
    // A Shift+press sheet offers Unselect on a selected row.
    j.go(&[SHIFT_PRESS]);
    assert_eq!(j.sheet()[0], "Unselect");
    assert!(j.sheet().contains(&"Delete 5 selected".to_string()));
    j.go(&[ESC]);
    // Entering a folder clears the selection, as stock does; Escape climbs
    // back to the folder's row.
    j.turn_to_file("performance03/");
    j.go(&[PRESS]);
    assert_eq!(j.app().directory, "/Internal/performance03");
    assert!(j.app().perf.files.selection.is_empty());
    j.turn_to_file("performance.grm");
    j.go(&[ESC]);
    assert_eq!(j.file_key(), "performance03/");
    j.go(&[PRESS]);
    assert_eq!(
        j.file_key(),
        "performance.grm",
        "the folder remembers its focus"
    );
}

/// Every row reaches each of its actions in two presses :
/// press opens the sheet on the primary action, turn, press runs it.
fn two_presses_from(j: &mut Journey, label: &str) {
    let actions = j.sheet();
    let title = match j.app().perf.layers.last() {
        Some(Layer::Sheet(s)) => s.title.clone(),
        _ => unreachable!(),
    };
    j.go(&[ESC]);
    for (i, item) in actions.iter().enumerate() {
        let row = j.app().perf_context();
        j.presses = 0;
        j.go(&[PRESS]);
        j.turn(i as i32);
        j.go(&[PRESS]);
        assert_eq!(j.presses, 2, "{item} on {label}");
        let same = matches!(j.app().perf.layers.last(),
            Some(Layer::Sheet(s)) if s.title == title);
        assert!(!same, "{item} on {label} did not run");
        // Close what opened; the row keeps focus.
        for _ in 0..4 {
            if j.app().perf.layers.is_empty()
                && !j.app().overlay_active()
                && j.app().copy_arm.is_none()
            {
                break;
            }
            j.go(&[ESC]);
        }
        if j.app().tab != 4 {
            // A load returned to the main screen, as stock does.
            return;
        }
        assert_eq!(
            j.app().perf_context(),
            row,
            "{item} on {label} kept the row"
        );
    }
}

#[test]
fn slots_presets_and_files_reach_every_action_in_two_presses_and_escape_keeps_the_row() {
    // Slots: the working one, another saved one, an empty one at the end.
    for (slot, expected) in [
        (
            0,
            &[
                "Save",
                "Rename",
                "Revert to saved",
                "Copy preset to all",
                "Copy settings to all",
                "Clear performance",
            ][..],
        ),
        (
            11,
            &[
                "Load",
                "Save working here",
                "Rename",
                "Initialize",
                "Initialize and load",
            ],
        ),
        (
            49,
            &["Save working here", "Initialize", "Initialize and load"],
        ),
    ] {
        let mut j = Journey::new();
        fifty_performances(&mut j);
        j.app_mut().performance_library[49].0 = "Empty performance".into();
        j.go(&[PERF]);
        j.turn(slot as i32);
        let top = j.app().perf.slots.top;
        j.go(&[PRESS]);
        assert_eq!(j.sheet(), expected, "slot {}", slot + 1);
        j.go(&[ESC]);
        assert_eq!(j.app().perf.slots.focus, slot, "Escape returns to the row");
        assert_eq!(j.app().perf.slots.top, top, "and to its scroll position");
        j.go(&[PRESS]);
        two_presses_from(&mut j, &format!("slot {}", slot + 1));
    }
    // Presets: the focused part's own preset and another cell.
    for (cell, first) in [(0, "Assign to part"), (31, "Play on part 1")] {
        let mut j = Journey::new();
        j.go(&[PERF, PERF]);
        j.turn(cell);
        j.go(&[PRESS]);
        assert_eq!(j.sheet()[0], first);
        j.go(&[ESC]);
        assert_eq!(j.app().perf.cell, cell as usize);
        j.go(&[PRESS]);
        two_presses_from(&mut j, &format!("cell {}", cell + 1));
    }
    // Files: the first, a middle and the last of 60 rows.
    for key in ["Folder/", "Sample 29.wav", "Sample 59.wav"] {
        let mut j = Journey::new();
        j.app_mut().files = (0..60).map(|i| format!("Sample {i:02}.wav")).collect();
        j.app_mut().files.push("Folder/".into());
        j.app_mut().perf.page = FILES;
        j.go(&[PERF]);
        j.turn_to_file(key);
        let top = j.app().perf.files.list.top;
        j.go(&[SHIFT_PRESS]);
        assert_eq!(j.app().perf.layers.len(), 1);
        j.go(&[ESC]);
        assert_eq!(j.file_key(), key, "Escape returns to the row");
        assert_eq!(
            j.app().perf.files.list.top,
            top,
            "and to its scroll position"
        );
        j.presses = 0;
        j.go(&[SHIFT_PRESS]);
        let items = j.sheet();
        j.turn(items.iter().position(|l| l == "Rename").unwrap() as i32);
        j.go(&[PRESS]);
        assert_eq!(j.presses, 2, "Rename on {key}");
        assert!(matches!(
            j.app().files_ui.layers.last(),
            Some(overlay::Layer::Name(_))
        ));
        j.go(&[ESC]);
        assert_eq!(j.file_key(), key);
    }
}

#[test]
fn a_renamed_file_keeps_focus_and_a_deleted_one_moves_to_the_trash() {
    let mut j = Journey::new();
    backup_folders(&mut j);
    j.go(&[PERF]);
    j.turn_to_file("REEDS_04.wav");
    j.go(&[SHIFT_PRESS]);
    j.turn(j.sheet().iter().position(|l| l == "Rename").unwrap() as i32);
    j.go(&[PRESS]);
    assert!(j.p.app.name_text("X"));
    j.go(&[SAVE]);
    assert!(!j.app().overlay_active());
    assert_eq!(j.file_key(), "REEDS_04.wavX");
    j.go(&[SHIFT_PRESS]);
    j.turn(j.sheet().iter().position(|l| l == "Delete").unwrap() as i32);
    j.go(&[PRESS]);
    let (lines, on_verb, _) = j.ask();
    assert_eq!(lines, ["Delete REEDS_04.wavX"]);
    assert!(!on_verb);
    j.turn(1);
    j.go(&[PRESS]);
    assert!(!j.app().files.iter().any(|f| f == "REEDS_04.wavX"));
    assert_eq!(j.app().toast, "Moved REEDS_04.wavX to the trash");
}

#[test]
fn preset_copy_arms_with_the_lit_type_and_pastes_after_a_confirmation() {
    let mut j = Journey::new();
    j.go(&[PERF, PERF]);
    assert_eq!(j.app().perf.page, PRESETS);
    // Copy arms with the focused cell (Patch lit): the Copy chip shows it.
    j.turn(1);
    j.go(&[COPY]);
    assert_eq!(
        j.app().copy_arm.map(|a| (a.from, a.kind)),
        Some((1, "copy_patch"))
    );
    j.turn(4);
    j.go(&[COPY]);
    let (lines, _, keycap) = j.ask();
    assert_eq!(lines[0], "Copy the patch of 02");
    assert_eq!(keycap, "Copy");
    j.go(&[COPY]);
    assert_eq!(j.app().patches[5].name, "Glass tide");
    assert!(j.app().patches[5].dirty);
    assert!(j.app().copy_arm.is_none());
    // An arm survives a preset press to the main screen, where Copy pastes
    // into the focused preset: one copy arm for the whole interface.
    j.go(&[(33, 0)]);
    j.turn(-2);
    j.go(&[COPY]);
    assert_eq!(j.app().copy_arm.map(|a| a.kind), Some("copy_sample"));
    j.go(&[(6, 0), (10, 0)]);
    assert_eq!(j.app().tab, 0);
    j.app_mut().parts[0] = 9;
    j.go(&[COPY]);
    assert_eq!(j.app().patches[9].sample, j.app().patches[3].sample);
    assert!(j.app().copy_arm.is_none());
}

#[test]
fn system_choices_apply_on_select_numbers_and_brightness_restore_on_escape() {
    let mut j = Journey::new();
    j.app_mut().perf.page = SYSTEM;
    j.go(&[PERF]);
    let fields = j.app().perf_system_fields();
    assert_eq!(
        fields.len(),
        20,
        "Display 2, Controls 6, MIDI 5, Audio 2, Device 5"
    );
    // Brightness: the screen dims while turning; Escape restores it.
    j.turn(1);
    j.go(&[PRESS]);
    let before = j.app().appearance.brightness;
    j.turn(-2);
    assert_ne!(j.app().appearance.brightness, before);
    j.go(&[ESC]);
    assert_eq!(j.app().appearance.brightness, before);
    // ADSR sliders: a choice shows in amber and applies only on Select.
    let adsr = fields
        .iter()
        .position(|f| *f == perf_root::Field::Setting(j.app().setting_index("adsr_mode")))
        .unwrap();
    j.turn(adsr as i32 - 1);
    j.go(&[PRESS]);
    j.turn(1);
    assert_eq!(
        j.app().get("adsr_mode"),
        0.,
        "nothing applies while turning"
    );
    j.go(&[PRESS]);
    assert_eq!(j.app().get("adsr_mode"), 1.);
    // LED brightness: a number is live and Escape restores it.
    j.turn(3);
    j.go(&[PRESS]);
    let leds = j.app().get("leds");
    j.turn(-5);
    assert_eq!(j.app().get("leds"), leds - 5.);
    j.go(&[ESC]);
    assert_eq!(j.app().get("leds"), leds);
    // A toggle flips on press.
    j.turn(-1);
    let invert = j.app().get("invert");
    j.go(&[PRESS]);
    assert_ne!(j.app().get("invert"), invert);
    // Shut down asks first, on Cancel.
    j.turn(30);
    j.go(&[PRESS]);
    let (lines, on_verb, _) = j.ask();
    assert_eq!(lines, ["Shut down", "the instrument"]);
    assert!(!on_verb);
    j.go(&[ESC]);
    assert!(j.app().perf.layers.is_empty());
}

#[test]
fn the_panels_load_on_a_slot_asks_and_load_confirms() {
    let mut j = Journey::new();
    fifty_performances(&mut j);
    j.go(&[PERF]);
    j.turn(11);
    j.presses = 0;
    j.go(&[LOAD]);
    let (lines, on_verb, keycap) = j.ask();
    assert_eq!(lines[0], "Load slot 12");
    assert!(!on_verb);
    assert_eq!(keycap, "Load");
    j.go(&[LOAD]);
    assert_eq!(j.presses, 2);
    assert_eq!(j.app().performance_slot, 11);
    assert_eq!(j.app().tab, 0, "loading returns to the main screen");
    assert_eq!(j.app().toast, "Loaded performance 12 Field notes");
}

#[test]
fn dark_perf_pages_and_sheets_draw_no_large_bright_fills() {
    let mut j = Journey::new();
    j.app_mut().appearance.theme = crate::appearance::Theme::Dark;
    j.app_mut().appearance.brightness = 100;
    backup_folders(&mut j);
    j.app_mut().perf.page = PERFORMANCES;
    j.go(&[PERF]);
    for page in 0..5 {
        assert_eq!(j.app().perf.page, page);
        // The page, then the focused row's sheet .
        for sheet in [false, true] {
            if sheet {
                j.go(&[SHIFT_PRESS]);
            }
            let mut p = Painter::new().unwrap();
            j.p.app.render(&mut p);
            let largest = super::u3_tests::largest_bright_area(&p);
            assert!(
                largest < 7680 / 4,
                "{} (sheet {sheet}): {largest} bright pixels in one area",
                perf_root::PAGES[page]
            );
        }
        while !j.app().perf.layers.is_empty() {
            j.go(&[ESC]);
        }
        // The same chord steps to the next page, wrapping around.
        j.go(&[PERF]);
    }
    assert_eq!(j.app().perf.page, PERFORMANCES, "the chord wraps around");
}
