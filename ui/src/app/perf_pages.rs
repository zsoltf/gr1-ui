//! The Performances, Presets and Parts pages of the Perf root :
//! their rows, sheets and the verbs they run through the device operations.
use super::perf_root::*;
use super::*;

/// A row of the Performances list.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum SlotRow {
    /// The working performance loaded from USB or Factory, pinned first.
    Imported,
    Slot(usize),
}

const EMPTY_SAMPLES: [&str; 2] = ["empty.wav", "Empty sample.wav"];

impl App {
    // ----- Performances -----

    pub(super) fn imported_working(&self) -> bool {
        self.device.as_ref().is_some_and(|d| d.imported())
    }
    pub(super) fn perf_slot_rows(&self) -> Vec<SlotRow> {
        let mut rows = vec![];
        if self.imported_working() {
            rows.push(SlotRow::Imported);
        }
        rows.extend((0..50).map(SlotRow::Slot));
        rows
    }
    /// The focused slot; None on the pinned imported row.
    pub(super) fn perf_focused_slot(&self) -> Option<usize> {
        match self.perf_slot_rows().get(self.perf.slots.focus) {
            Some(SlotRow::Slot(i)) => Some(*i),
            _ => None,
        }
    }
    pub(super) fn is_working_slot(&self, slot: usize) -> bool {
        slot == self.performance_slot && !self.imported_working()
    }
    /// "Saved 3 Oct 21:40" and the number of presets with a sample.
    pub(super) fn perf_slot_info(&mut self, slot: usize) -> (String, usize) {
        if let Some(info) = self.perf.slot_info.get(&slot) {
            return info.clone();
        }
        let info = match &self.device {
            Some(d) => {
                let path = d.performance_paths[slot].clone();
                let saved = std::fs::metadata(path.join("performance.grm"))
                    .and_then(|m| m.modified())
                    .map(|t| format!("Saved {}", overlay_draw::local_time(t)))
                    .unwrap_or_default();
                let count = (0..32)
                    .filter(|i| {
                        crate::device::storage::patch_file(&path.join(format!("{i:03}")))
                            .and_then(|p| crate::device::storage::Document::read(&p, "gr-1 patch"))
                            .is_ok_and(|doc| {
                                let sample = doc.text("sample");
                                !sample.is_empty() && !EMPTY_SAMPLES.contains(&sample)
                            })
                    })
                    .count();
                (saved, count)
            }
            None => (
                "Saved 3 Oct 21:40".into(),
                self.performance_library
                    .get(slot)
                    .map(|(_, patches)| {
                        patches
                            .iter()
                            .filter(|p| !EMPTY_SAMPLES.contains(&p.sample.as_str()))
                            .count()
                    })
                    .unwrap_or(0),
            ),
        };
        self.perf.slot_info.insert(slot, info.clone());
        info
    }
    pub(super) fn perf_slot_sheet(&mut self) {
        let item = |label: &str, keycap, destructive, act| Item {
            label: label.into(),
            keycap,
            destructive,
            act,
        };
        let Some(slot) = self.perf_focused_slot() else {
            let items = vec![item("Save to a slot", Some("Save"), false, Do::SaveToSlot)];
            self.perf_sheet("Working: from USB or Factory".into(), items);
            return;
        };
        let (name, empty) = self.slot_name(slot);
        let items = if self.is_working_slot(slot) {
            vec![
                item("Save", Some("Save"), false, Do::SaveSlot(slot)),
                item("Rename", None, false, Do::RenameWorking),
                item("Revert to saved", None, true, Do::Revert),
                item("Copy preset to all", None, true, Do::CopyAll),
                item("Copy settings to all", None, true, Do::TemplateAll),
                item("Clear performance", None, true, Do::ClearPerformance),
            ]
        } else if empty {
            vec![
                item("Save working here", Some("Save"), false, Do::SaveSlot(slot)),
                item("Initialize", None, false, Do::InitSlot(slot)),
                item("Initialize and load", None, true, Do::InitLoad(slot)),
            ]
        } else {
            vec![
                item("Load", Some("Load"), false, Do::LoadSlot(slot)),
                item("Save working here", Some("Save"), false, Do::SaveSlot(slot)),
                item("Rename", None, false, Do::RenameSlot(slot)),
                item("Initialize", None, true, Do::InitSlot(slot)),
                item("Initialize and load", None, true, Do::InitLoad(slot)),
            ]
        };
        let title = format!(
            "Slot {:02}  {}",
            slot + 1,
            if empty { "Empty" } else { name.as_str() }
        );
        self.perf_sheet(title, items);
    }

    /// Run a sheet item or a panel verb on the focused row.
    pub(super) fn perf_do(&mut self, act: Do) {
        let unsaved = self.patches.iter().any(|p| p.dirty);
        let working_loss = if unsaved {
            "Unsaved changes to the working performance are lost."
        } else {
            "The working performance is replaced."
        };
        match act {
            Do::LoadSlot(slot) => {
                let (name, empty) = self.slot_name(slot);
                if empty {
                    self.error(&format!("Slot {:02} is empty", slot + 1));
                    return;
                }
                self.perf_ask(
                    vec![
                        format!("Load slot {:02}", slot + 1),
                        name,
                        "as the working performance".into(),
                    ],
                    working_loss,
                    "Load",
                    "Load",
                    Verb::LoadSlot(slot),
                );
            }
            Do::SaveSlot(slot) => {
                if self.imported_working() && slot == self.performance_slot {
                    self.open_save_sheet(overlay::DataKind::Perf);
                    return;
                }
                let (name, empty) = self.slot_name(slot);
                if empty || self.is_working_slot(slot) {
                    self.perf_queue_job(Job::SaveSlot(slot));
                } else {
                    let (_, count) = self.perf_slot_info(slot);
                    self.perf_ask(
                        vec![
                            format!("Replace slot {:02}", slot + 1),
                            name,
                            "with the working performance".into(),
                        ],
                        &format!("Its {count} saved presets are overwritten."),
                        "Replace",
                        "Save",
                        Verb::Job(Job::SaveSlot(slot)),
                    );
                }
            }
            Do::SaveToSlot => self.open_save_sheet(overlay::DataKind::Perf),
            Do::RenameSlot(slot) => self.open_name_editor(overlay::NamePurpose::Slot(slot)),
            Do::RenameWorking => self.open_name_editor(overlay::NamePurpose::Performance),
            Do::InitSlot(slot) => {
                let (name, empty) = self.slot_name(slot);
                if empty {
                    self.perf_queue_job(Job::InitSlot(slot));
                } else {
                    self.perf_ask(
                        vec![format!("Initialize slot {:02}", slot + 1), name],
                        "Its saved presets become empty; the working performance stays.",
                        "Initialize",
                        "",
                        Verb::Job(Job::InitSlot(slot)),
                    );
                }
            }
            Do::InitLoad(slot) => {
                let (name, empty) = self.slot_name(slot);
                let mut lines = vec![format!("Initialize slot {:02}", slot + 1)];
                if !empty {
                    lines.push(name);
                }
                lines.push("and load it".into());
                self.perf_ask(lines, working_loss, "Initialize", "", Verb::InitLoad(slot));
            }
            Do::Revert => self.perf_ask(
                vec![
                    "Revert the working".into(),
                    format!("performance to slot {:02}", self.performance_slot + 1),
                    "as saved".into(),
                ],
                "Unsaved changes are lost.",
                "Revert",
                "",
                Verb::Job(Job::Revert),
            ),
            Do::CopyAll | Do::TemplateAll => {
                let source = self.parts[self.part];
                let all = act == Do::CopyAll;
                self.perf_ask_copy_all(source, all);
            }
            Do::ClearPerformance => self.perf_ask(
                vec!["Clear the working".into(), "performance".into()],
                "All 32 presets become empty; unsaved changes are lost.",
                "Clear",
                "",
                Verb::Job(Job::ClearPerformance),
            ),
            Do::Play(cell) => self.perf_assign(cell, self.part),
            Do::AssignTo(cell) => {
                let items = (0..4)
                    .map(|part| Item {
                        label: format!(
                            "Part {}  {:02} {}{}",
                            part + 1,
                            self.parts[part] + 1,
                            self.patches[self.parts[part]].name,
                            if self.enabled[part] { "" } else { "  (Off)" }
                        ),
                        keycap: None,
                        destructive: false,
                        act: Do::Assign { cell, part },
                    })
                    .collect();
                self.perf_sheet(format!("Assign {:02} to", cell + 1), items);
                if let Some(Layer::Sheet(s)) = self.perf.layers.last_mut() {
                    s.focus = self.part;
                }
            }
            Do::Assign { cell, part } => self.perf_assign(cell, part),
            Do::CopyTo(cell) => self.perf_arm_copy(cell),
            Do::RenamePreset => self.open_name_editor(overlay::NamePurpose::Patch),
            Do::RevertPreset => {
                let slot = self.parts[self.part];
                self.perf_ask(
                    vec![
                        format!("Revert preset {:02}", slot + 1),
                        self.patches[slot].name.clone(),
                    ],
                    "Its unsaved changes are lost.",
                    "Revert",
                    "",
                    Verb::Op("revert_patch"),
                );
            }
            Do::ClearPreset => {
                let slot = self.parts[self.part];
                self.perf_ask(
                    vec![
                        format!("Clear preset {:02}", slot + 1),
                        self.patches[slot].name.clone(),
                    ],
                    "Its sample and settings are reset; unsaved changes are lost.",
                    "Clear",
                    "",
                    Verb::Op("clear_patch"),
                );
            }
            Do::PartOn(part) => self.perf_toggle_part(part),
            Do::AllChannels(part) => {
                let channel = self.get(&format!("midi{}", part + 1));
                for p in 0..4 {
                    let i = self.setting_index(&format!("midi{}", p + 1));
                    self.set(i, channel);
                }
                self.notify(&format!(
                    "All parts on {}",
                    self.perf_field_text(Field::PartChannel(part), channel)
                ));
            }
            Do::Select
            | Do::SelectAll
            | Do::UnselectAll
            | Do::RenameFile
            | Do::Delete
            | Do::NewFolder
            | Do::Open
            | Do::Unmount => self.perf_files_do(act),
        }
    }
    /// A save into an empty or the working slot asks nothing (3.6).
    pub(super) fn perf_queue_job(&mut self, job: Job) {
        self.perf.layers.clear();
        self.perf_queue(job);
    }

    // ----- Presets -----

    pub(super) fn perf_cell_sheet(&mut self) {
        let cell = self.perf.cell;
        let current = self.parts[self.part] == cell;
        let item = |label: String, keycap, destructive, act| Item {
            label,
            keycap,
            destructive,
            act,
        };
        let mut items = vec![];
        if !current {
            items.push(item(
                format!("Play on part {}", self.part + 1),
                Some("Load"),
                false,
                Do::Play(cell),
            ));
        }
        items.push(item(
            "Assign to part".into(),
            None,
            false,
            Do::AssignTo(cell),
        ));
        items.push(item(
            "Copy to".into(),
            Some("Copy"),
            false,
            Do::CopyTo(cell),
        ));
        // The engine edits the focused part's preset; the other cells' edits
        // are left out rather than shown disabled (3.5.7).
        if current {
            items.push(item("Rename".into(), None, false, Do::RenamePreset));
            items.push(item("Revert to saved".into(), None, true, Do::RevertPreset));
            items.push(item("Clear".into(), None, true, Do::ClearPreset));
        }
        let title = format!("{:02}  {}", cell + 1, self.patches[cell].name);
        self.perf_sheet(title, items);
    }
    /// Copy arms with the focused cell, by the lit type .
    pub(super) fn perf_arm_copy(&mut self, cell: usize) {
        let kind = if self.files_ui.lit == overlay::DataKind::Sample {
            "copy_sample"
        } else {
            "copy_patch"
        };
        self.copy_arm = Some(CopyArm { from: cell, kind });
        self.notify(&format!(
            "Copied the {} of {:02}",
            if kind == "copy_sample" {
                "sample"
            } else {
                "patch"
            },
            cell + 1
        ));
    }
    pub(super) fn perf_ask_paste_cell(&mut self, copy: CopyArm) {
        let to = self.perf.cell;
        if to == copy.from {
            self.copy_arm = None;
            return;
        }
        let what = if copy.kind == "copy_sample" {
            "sample"
        } else {
            "patch"
        };
        self.perf_ask(
            vec![
                format!("Copy the {what} of {:02}", copy.from + 1),
                format!("into {:02} {}", to + 1, self.patches[to].name),
            ],
            &if copy.kind == "copy_sample" {
                "Its sample is replaced; its settings stay.".to_string()
            } else {
                "Its sample and settings are replaced.".to_string()
            },
            "Copy",
            "Copy",
            Verb::CopyCell {
                kind: copy.kind,
                from: copy.from,
                to,
            },
        );
    }
    pub(super) fn perf_copy_cell(&mut self, kind: &'static str, from: usize, to: usize) {
        self.copy_arm = None;
        let result = match &mut self.device {
            Some(d) => d.copy_from(kind, from, to),
            None => {
                if self.recording == Some(to) {
                    self.recording = None;
                }
                if kind == "copy_sample" {
                    self.patches[to].sample = self.patches[from].sample.clone();
                } else {
                    self.patches[to] = self.patches[from].clone();
                }
                self.patches[to].dirty = true;
                Ok(())
            }
        };
        match result {
            Ok(()) => {
                self.sync_device_view();
                self.notify(&format!(
                    "Copied {:02} into {:02} {}",
                    from + 1,
                    to + 1,
                    self.patches[to].name
                ));
            }
            Err(e) => self.error(&format!("Copy failed: {e}")),
        }
    }
    /// Copy the preset to all others (manual p.73): the confirmation names
    /// what is overwritten; the panel's Copy confirms.
    pub(super) fn perf_ask_copy_all(&mut self, source: usize, all: bool) {
        self.perf_ask(
            vec![
                if all {
                    format!("Copy {:02} to all presets", source + 1)
                } else {
                    format!("Copy the settings of {:02}", source + 1)
                },
                self.patches[source].name.clone(),
                if all {
                    "into the other 31".into()
                } else {
                    "to the other 31 presets".into()
                },
            ],
            if all {
                "Their samples and settings are overwritten."
            } else {
                "Their settings are overwritten; their samples stay."
            },
            "Copy",
            "Copy",
            Verb::Job(if all { Job::CopyAll } else { Job::TemplateAll }),
        );
    }
    /// Put preset `cell` on `part`. The engine refuses a preset another part
    /// plays (event 30); the interface says so instead of asking it.
    pub(super) fn perf_assign(&mut self, cell: usize, part: usize) {
        if let Some(other) =
            (0..4).find(|p| *p != part && self.enabled[*p] && self.parts[*p] == cell)
        {
            self.error(&format!("Preset {:02} is on part {}", cell + 1, other + 1));
            return;
        }
        if self.parts[part] == cell {
            return;
        }
        let result = match &mut self.device {
            Some(d) => d.multi(|m| m.patchnums[part] = cell as i32),
            None => {
                self.parts[part] = cell;
                Ok(())
            }
        };
        match result {
            Ok(()) => {
                self.live = None;
                self.sync_device_view();
                self.notify(&format!(
                    "Part {} plays {:02} {}",
                    part + 1,
                    cell + 1,
                    self.patches[cell].name
                ));
            }
            Err(e) => self.error(&e),
        }
    }

    // ----- Parts -----

    pub(super) fn perf_toggle_part(&mut self, part: usize) {
        if self.enabled[part] && self.enabled.iter().filter(|e| **e).count() == 1 {
            self.error("One part stays on");
            return;
        }
        if !self.enabled[part] {
            if let Some(other) = (0..4)
                .find(|p| *p != part && self.enabled[*p] && self.parts[*p] == self.parts[part])
            {
                self.error(&format!(
                    "Preset {:02} is on part {}",
                    self.parts[part] + 1,
                    other + 1
                ));
                return;
            }
        }
        self.action(Action::PartEnable(part));
    }
    pub(super) fn perf_part_sheet(&mut self, part: usize) {
        let channel = self.perf_field_text(
            Field::PartChannel(part),
            self.get(&format!("midi{}", part + 1)),
        );
        let items = vec![
            Item {
                label: if self.enabled[part] {
                    "Turn off".into()
                } else {
                    "Turn on".into()
                },
                keycap: None,
                destructive: false,
                act: Do::PartOn(part),
            },
            Item {
                label: format!("Use {channel} for all parts"),
                keycap: None,
                destructive: false,
                act: Do::AllChannels(part),
            },
        ];
        self.perf_sheet(format!("Part {}", part + 1), items);
    }

    /// Initialize a user slot on disk.
    pub(super) fn perf_initialize_slot(&mut self, slot: usize) -> Result<(), String> {
        match &mut self.device {
            Some(d) => d.initialize_slot(slot),
            None => {
                self.perf_demo_slots(slot);
                let empty: Vec<Patch> = (0..32)
                    .map(|i| {
                        let mut p = Patch::new(i);
                        p.name = format!("Empty {:02}", i + 1);
                        p.sample = "Empty sample.wav".into();
                        p
                    })
                    .collect();
                self.performance_library[slot] = ("New".into(), empty);
                self.performance_parts[slot] =
                    ([0, 1, 2, 3], [true, false, false, false], [100.; 4]);
                Ok(())
            }
        }
    }
    /// The desktop demo keeps a library entry for every slot it touches.
    pub(super) fn perf_demo_slots(&mut self, slot: usize) {
        while self.performance_library.len() <= slot {
            self.performance_library
                .push(("Empty performance".into(), self.patches.clone()));
        }
        while self.performance_parts.len() <= slot {
            self.performance_parts
                .push(([0, 1, 2, 3], [true, false, false, false], [80.; 4]));
        }
        while self.performance_configs.len() <= slot {
            self.performance_configs.push(self.snapshots);
        }
        while self.performance_snapshot.len() <= slot {
            self.performance_snapshot.push(0);
        }
    }
    /// Rename a saved performance in its slot (the name editor's result).
    pub(super) fn perf_rename_slot(&mut self, slot: usize, name: &str) -> Result<String, String> {
        match &mut self.device {
            Some(d) => d.rename_slot(slot, name)?,
            None => {
                self.perf_demo_slots(slot);
                self.performance_library[slot].0 = name.into();
            }
        }
        if self.is_working_slot(slot) {
            match &mut self.device {
                Some(d) => d.performance.document.put("name", name),
                None => self.performance = name.into(),
            }
        }
        self.refresh_performance_list();
        self.perf.slot_info.remove(&slot);
        Ok(format!("Renamed slot {:02} to {name}", slot + 1))
    }
}
