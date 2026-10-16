//! The Perf root of the redesign :
//! Performances, Presets, Parts, Files and System under one page strip,
//! reached with Shift+Perf. This module holds the root's interface state
//! (page, strip focus, per-page focus, the grabbed field, its sheets and
//! confirmations, the armed copy) and its panel grammar. The engine and the
//! files stay the state owners: every result goes through the existing device
//! operations, or the file browser, Save sheet and name editor.
use super::components::ListCursor;
use super::device_ui::encoder_delta;
use super::*;
use crate::device::native::PanelEvent;

pub(super) const PAGES: [&str; 5] = ["Performances", "Presets", "Parts", "Files", "System"];
pub(super) const PERFORMANCES: usize = 0;
pub(super) const PRESETS: usize = 1;
pub(super) const PARTS: usize = 2;
pub(super) const FILES: usize = 3;
pub(super) const SYSTEM: usize = 4;
/// The engine screen id of the whole root, page strip and sheets included.
pub(super) const SCREEN_ID: i32 = 5;
/// Fields inside a part row, in turn order.
pub(super) const PART_FIELDS: usize = 4;
/// Entering a part row lands on Level, the field journey J5 edits first.
const PART_LEVEL: usize = 2;
pub(super) const BRIGHTNESS: [u8; 5] = [10, 25, 50, 75, 100];

/// A field the grab model edits .
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Field {
    Setting(usize),
    Appearance,
    Brightness,
    Multi,
    PartOn(usize),
    PartPreset(usize),
    PartLevel(usize),
    PartChannel(usize),
    Disk,
    Firmware,
    Reset,
    Shutdown,
}

pub(super) enum FieldKind {
    Toggle,
    /// Options cycle in amber while turning and apply on Select.
    Choice(Vec<String>),
    /// Turning sends the value at once; Escape restores it.
    Number {
        min: f32,
        max: f32,
        step: f32,
    },
    Action,
}

/// A grabbed field: the value shown and the value at the moment of the grab.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Grab {
    pub field: Field,
    pub original: f32,
    pub value: f32,
}

/// What a sheet item does.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Do {
    LoadSlot(usize),
    SaveSlot(usize),
    /// The imported working performance: the Save sheet picks a slot.
    SaveToSlot,
    RenameSlot(usize),
    RenameWorking,
    InitSlot(usize),
    InitLoad(usize),
    Revert,
    CopyAll,
    TemplateAll,
    ClearPerformance,
    Play(usize),
    AssignTo(usize),
    Assign {
        cell: usize,
        part: usize,
    },
    CopyTo(usize),
    RenamePreset,
    RevertPreset,
    ClearPreset,
    PartOn(usize),
    AllChannels(usize),
    Select,
    SelectAll,
    UnselectAll,
    RenameFile,
    Delete,
    NewFolder,
    Open,
    Unmount,
}

#[derive(Clone, Debug)]
pub(super) struct Item {
    pub label: String,
    pub keycap: Option<&'static str>,
    pub destructive: bool,
    pub act: Do,
}

pub(super) struct Sheet {
    pub title: String,
    pub items: Vec<Item>,
    pub focus: usize,
}

/// What a confirmation runs.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Verb {
    Job(Job),
    LoadSlot(usize),
    InitLoad(usize),
    CopyCell {
        kind: &'static str,
        from: usize,
        to: usize,
    },
    /// An existing confirm operation: clear_patch, revert_patch, factory...
    Op(&'static str),
    Delete(Vec<String>),
    Paste,
    LoadPerfPath(String),
}

pub(super) struct Ask {
    pub lines: Vec<String>,
    pub loss: String,
    pub verb_label: String,
    /// The panel button that started it, which also confirms.
    pub keycap: &'static str,
    pub on_verb: bool,
    pub verb: Verb,
}

pub(super) enum Layer {
    Sheet(Sheet),
    Ask(Ask),
}

/// Native bulk operations, run after one frame naming them
/// on the file queue.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Job {
    SaveSlot(usize),
    LoadSlot(usize),
    InitSlot(usize),
    InitLoad(usize),
    Revert,
    CopyAll,
    TemplateAll,
    ClearPerformance,
}

/// Stock's copy arm (the Copy chip), one for the whole interface: the
/// source preset and what it copies (`copy_sample` or `copy_patch`). The
/// Sample root arms a sample; the Presets page arms by the lit type; Copy on
/// a cell or on the main screen pastes into the focused preset.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct CopyArm {
    pub from: usize,
    pub kind: &'static str,
}

/// Perf-root switches that depend on instrument behavior not yet observed.
#[derive(Clone, Copy, Debug)]
pub(super) struct PerfPolicy {
    /// Unobserved: the cost of computing every preset's stencil for the
    /// Presets grid. Off: presets no part plays show no mini waveform.
    pub preset_stencils: bool,
    /// Stencils computed per frame while a page shows them.
    pub stencils_per_frame: usize,
    /// Time a paste may take per frame on the interface thread.
    pub copy_slice: Duration,
}
impl Default for PerfPolicy {
    fn default() -> Self {
        Self {
            preset_stencils: true,
            stencils_per_frame: 2,
            copy_slice: Duration::from_millis(4),
        }
    }
}

#[derive(Default)]
pub(super) struct PerfRoot {
    pub page: usize,
    /// Focus is on the page strip.
    pub strip: bool,
    pub layers: Vec<Layer>,
    pub grab: Option<Grab>,
    pub policy: PerfPolicy,
    /// Performances: rows are the pinned imported row (if any) and the slots.
    pub slots: ListCursor,
    /// Presets: the focused cell 0..31.
    pub cell: usize,
    /// Parts: row 0 is the multi selector, rows 1-4 the parts; `inside` is
    /// the field focused inside a part row; each part remembers its field.
    pub part_row: usize,
    pub inside: Option<usize>,
    pub part_fields: [Option<usize>; 4],
    /// System: the focused field and the first visible row.
    pub system: usize,
    pub system_top: usize,
    pub files: super::perf_files::FilesPage,
    /// Saved-performance second lines by slot: (modified, preset count).
    pub slot_info: HashMap<usize, (String, usize)>,
    /// Free bytes, read on entering System and on Disk free.
    pub free: Option<Result<u64, String>>,
    /// Shift as last seen, for selecting in Files when it goes down.
    pub shift_seen: bool,
}

impl App {
    // ----- entry, exit and routing -----

    /// The root or one of its confirmations over the main screen is active.
    pub(super) fn perf_active(&self) -> bool {
        self.tab == 4 || (self.tab == 0 && self.modal.is_none() && !self.perf.layers.is_empty())
    }
    /// Shift+Perf: open the root at its last page and field; again while it is
    /// open, the next page .
    pub(super) fn open_perf_root(&mut self, advance: bool) {
        let was_open = self.tab == 4;
        self.perf_release();
        self.live = None;
        self.modal = None;
        self.panel_sidebar = false;
        self.list_action_origin = None;
        self.page_return = None;
        if was_open && advance {
            self.perf.page = (self.perf.page + 1) % PAGES.len();
            self.perf_enter_page();
        }
        self.perf.strip = false;
        self.tab = 4;
        self.perf_enter_page();
        self.navigation_dirty = true;
        self.device_context();
    }
    /// Transient state ends whenever the root or its confirmation closes.
    pub(super) fn perf_release(&mut self) {
        if matches!(
            self.files_ui.listing.as_ref().map(|l| &l.target),
            Some(super::listing::Target::Files { .. })
        ) {
            self.cancel_listing();
        }
        self.perf.layers.clear();
        self.perf.grab = None;
        self.perf.inside = None;
    }
    pub(super) fn perf_to_main(&mut self) {
        self.perf_release();
        self.perf.strip = false;
        if self.tab == 4 {
            self.tab = 0;
            self.group = 0;
            self.page = 0;
        }
        self.navigation_dirty = true;
        self.device_context();
    }
    /// A page shows: refresh what it reads from disk.
    pub(super) fn perf_enter_page(&mut self) {
        match self.perf.page {
            PERFORMANCES => {
                self.refresh_performance_list();
                self.perf.slot_info.clear();
                let len = self.perf_slot_rows().len();
                self.perf.slots.reveal(len);
            }
            FILES => {
                self.perf_files_relist();
                if !self.perf.files.at_locations && self.files_ui.listing.is_none() {
                    self.refresh_files_listing();
                }
            }
            SYSTEM => self.perf_read_free(),
            _ => {}
        }
    }
    pub(super) fn perf_context(&self) -> String {
        let focus = match self.perf.page {
            PERFORMANCES => format!("slot{}", self.perf.slots.focus),
            PRESETS => format!("cell{}", self.perf.cell),
            PARTS => format!("row{}:{:?}", self.perf.part_row, self.perf.inside),
            FILES => format!("file{}", self.perf.files.list.focus),
            _ => format!("field{}", self.perf.system),
        };
        format!(
            "perf:{}:{}{}{}{}",
            PAGES[self.perf.page],
            if self.perf.strip { "strip" } else { &focus },
            if self.perf.grab.is_some() {
                ":grab"
            } else {
                ""
            },
            match self.perf.layers.last() {
                Some(Layer::Sheet(_)) => ":sheet",
                Some(Layer::Ask(_)) => ":confirm",
                None => "",
            },
            if self.copy_arm.is_some() { ":copy" } else { "" },
        )
    }

    /// Shift held for the Files selection: the engine's explicit-press flag on
    /// the instrument, as stock reads it; the Shift state elsewhere.
    pub(super) fn shift_held(&self) -> bool {
        match &self.device {
            Some(d) => d.shift_explicit,
            None => self.shift,
        }
    }
    /// Holding Shift selects the focused Files row (stock, manual p.74),
    /// whether or not the engine also sends SHIFT (31).
    pub(super) fn perf_shift_edge(&mut self) {
        let held = self.shift_held();
        if held && !self.perf.shift_seen {
            self.perf_shift_select();
        }
        self.perf.shift_seen = held;
    }

    /// Panel events for the Perf root and its confirmations; true when
    /// consumed. Events it leaves go to the file screens and the old handler.
    pub(super) fn perf_event(&mut self, event: &PanelEvent) -> bool {
        if self.tab != 0 && self.tab != 4 {
            self.perf_release();
            return false;
        }
        if !self.perf_active() {
            return false;
        }
        if !matches!(
            event.action,
            8..=11 | 16 | 17 | 20 | 21 | 23 | 25..=27 | 30 | 31 | 33..=35
        ) {
            self.clear_error();
        }
        let policy = self.files_ui.policy;
        let consumed = match event.action {
            5 => {
                self.perf_turn(encoder_delta(event.value));
                true
            }
            7 if policy.shifted_press_opens_sheet && self.shift => {
                self.perf_shift_press();
                true
            }
            7 => {
                self.perf_press();
                true
            }
            28 if policy.unmount_opens_sheet => {
                self.perf_shift_press();
                true
            }
            14 if policy.shifted_escape && self.shift => {
                self.perf_shift_escape();
                true
            }
            14 => {
                self.perf_escape();
                true
            }
            1 => self.perf_load(),
            2 | 19 => self.perf_save(),
            3 | 4 => {
                self.perf_copy();
                true
            }
            // MAIN precedes SET_PRESET on a preset press; the preset button
            // returns to the main screen, as stock does.
            6 => true,
            10 => {
                self.perf_keep();
                self.perf_to_main();
                true
            }
            12 => {
                self.perf_keep();
                self.open_perf_root(true);
                true
            }
            13 | 15 => {
                self.perf_keep();
                self.perf_release();
                false
            }
            22 => {
                self.perf_slider(event.value);
                true
            }
            29 if self.tab == 4 => {
                self.perf_kill(event.value);
                true
            }
            31 => {
                self.perf_shift_select();
                true
            }
            32 if self.tab == 4 => {
                self.perf_ask(
                    vec!["Reset device settings".into()],
                    "Saved patches, performances and samples stay.",
                    "Reset",
                    "",
                    Verb::Op("factory"),
                );
                true
            }
            _ => false,
        };
        if consumed {
            self.navigation_dirty = true;
        }
        consumed
    }

    fn perf_turn(&mut self, delta: i32) {
        if delta == 0 {
            return;
        }
        match self.perf.layers.last_mut() {
            Some(Layer::Sheet(s)) => {
                let last = s.items.len().saturating_sub(1) as i32;
                s.focus = (s.focus as i32 + delta).clamp(0, last) as usize;
                return;
            }
            Some(Layer::Ask(a)) => {
                a.on_verb = delta > 0;
                return;
            }
            None => {}
        }
        if self.perf.grab.is_some() {
            self.perf_grab_turn(delta);
            return;
        }
        if self.perf.strip {
            let page = (self.perf.page as i32 + delta).clamp(0, PAGES.len() as i32 - 1) as usize;
            if page != self.perf.page {
                self.perf.page = page;
                self.perf_enter_page();
            }
            return;
        }
        match self.perf.page {
            PERFORMANCES => {
                let len = self.perf_slot_rows().len();
                self.perf.slots.turn(delta, len);
            }
            PRESETS => self.perf.cell = (self.perf.cell as i32 + delta).clamp(0, 31) as usize,
            PARTS => match self.perf.inside {
                Some(field) => {
                    let next = (field as i32 + delta).clamp(0, PART_FIELDS as i32 - 1) as usize;
                    self.perf.inside = Some(next);
                    self.perf.part_fields[self.perf.part_row - 1] = Some(next);
                }
                None => {
                    self.perf.part_row = (self.perf.part_row as i32 + delta).clamp(0, 4) as usize
                }
            },
            FILES => self.perf_files_turn(delta),
            _ => {
                let last = self.perf_system_fields().len() as i32 - 1;
                self.perf.system = (self.perf.system as i32 + delta).clamp(0, last) as usize;
            }
        }
    }
    fn perf_press(&mut self) {
        match self.perf.layers.last() {
            Some(Layer::Sheet(s)) => {
                if let Some(act) = s.items.get(s.focus).map(|i| i.act.clone()) {
                    self.perf.layers.pop();
                    self.perf_do(act);
                }
                return;
            }
            Some(Layer::Ask(a)) => {
                if a.on_verb {
                    self.perf_confirm();
                } else {
                    self.perf.layers.pop();
                }
                return;
            }
            None => {}
        }
        if self.perf.grab.is_some() {
            self.perf_keep();
            return;
        }
        if self.perf.strip {
            self.perf.strip = false;
            return;
        }
        match self.perf.page {
            PERFORMANCES => self.perf_slot_sheet(),
            PRESETS => match self.copy_arm {
                Some(copy) => self.perf_ask_paste_cell(copy),
                None => self.perf_cell_sheet(),
            },
            PARTS => match (self.perf.part_row, self.perf.inside) {
                (0, _) => self.perf_field_press(Field::Multi),
                (row, None) => {
                    let field = self.perf.part_fields[row - 1].unwrap_or(PART_LEVEL);
                    self.perf.inside = Some(field);
                }
                (row, Some(field)) => self.perf_field_press(part_field(row - 1, field)),
            },
            FILES => self.perf_files_press(),
            _ => {
                if let Some(field) = self.perf_system_fields().get(self.perf.system).copied() {
                    self.perf_field_press(field);
                }
            }
        }
    }
    /// Shift+press: the focused row's action sheet .
    fn perf_shift_press(&mut self) {
        if !self.perf.layers.is_empty() || self.perf.grab.is_some() || self.perf.strip {
            self.perf_press();
            return;
        }
        match self.perf.page {
            PARTS if self.perf.part_row > 0 && self.perf.inside.is_none() => {
                self.perf_part_sheet(self.perf.part_row - 1)
            }
            FILES => self.perf_files_sheet(),
            PRESETS if self.copy_arm.is_some() => self.perf_press(),
            PERFORMANCES | PRESETS => self.perf_press(),
            _ => self.perf_press(),
        }
    }
    fn perf_escape(&mut self) {
        if self.perf_files_cancel_job() {
            return;
        }
        if self.perf.layers.pop().is_some() {
            return;
        }
        if self.perf.grab.is_some() {
            self.perf_restore();
            return;
        }
        if self.tab != 4 {
            return;
        }
        if self.perf.strip {
            self.perf_to_main();
            return;
        }
        match self.perf.page {
            PRESETS if self.copy_arm.is_some() => self.copy_arm = None,
            PARTS if self.perf.inside.is_some() => self.perf.inside = None,
            FILES => {
                if !self.perf_files_escape() {
                    self.perf.strip = true;
                }
            }
            _ => self.perf.strip = true,
        }
    }
    /// Shift+Escape: restore a grabbed value, then the main screen.
    fn perf_shift_escape(&mut self) {
        if self.perf.grab.is_some() {
            self.perf_restore();
        }
        self.perf_to_main();
    }
    /// The panel's Load : on a slot or preset it loads it, on a
    /// loadable file it loads it; elsewhere the browser opens.
    fn perf_load(&mut self) -> bool {
        match self.perf.layers.last() {
            Some(Layer::Sheet(s)) => {
                if let Some(item) = s.items.iter().find(|i| i.keycap == Some("Load")) {
                    let act = item.act.clone();
                    self.perf.layers.pop();
                    self.perf_do(act);
                }
                return true;
            }
            Some(Layer::Ask(a)) => {
                if a.keycap == "Load" {
                    self.perf_confirm();
                }
                return true;
            }
            None => {}
        }
        if self.tab != 4 {
            return false;
        }
        self.perf_keep();
        if self.perf.strip {
            return false;
        }
        match self.perf.page {
            PERFORMANCES => {
                match self.perf_focused_slot() {
                    Some(slot) => self.perf_do(Do::LoadSlot(slot)),
                    None => self.error("This performance is already loaded"),
                }
                true
            }
            PRESETS => {
                self.perf_do(Do::Play(self.perf.cell));
                true
            }
            FILES => self.perf_files_load(),
            _ => false,
        }
    }
    /// The panel's Save: on a slot it saves the working performance there;
    /// elsewhere the Save sheet opens for the lit type.
    fn perf_save(&mut self) -> bool {
        match self.perf.layers.last() {
            Some(Layer::Sheet(s)) => {
                if let Some(item) = s.items.iter().find(|i| i.keycap == Some("Save")) {
                    let act = item.act.clone();
                    self.perf.layers.pop();
                    self.perf_do(act);
                }
                return true;
            }
            Some(Layer::Ask(a)) => {
                if a.keycap == "Save" {
                    self.perf_confirm();
                }
                return true;
            }
            None => {}
        }
        if self.tab != 4 {
            return false;
        }
        self.perf_keep();
        if self.perf.page == PERFORMANCES && !self.perf.strip {
            match self.perf_focused_slot() {
                Some(slot) => self.perf_do(Do::SaveSlot(slot)),
                None => self.perf_do(Do::SaveToSlot),
            }
            return true;
        }
        false
    }
    /// The panel's Copy (and PASTE): confirms a copy question; on Presets it
    /// arms and pastes; in Files it registers the selection and pastes.
    fn perf_copy(&mut self) {
        match self.perf.layers.last() {
            Some(Layer::Ask(a)) if a.keycap == "Copy" => {
                self.perf_confirm();
                return;
            }
            Some(_) => return,
            None => {}
        }
        if self.tab != 4 || self.perf.strip {
            return;
        }
        self.perf_keep();
        match self.perf.page {
            PRESETS => match self.copy_arm {
                Some(copy) if copy.from != self.perf.cell => self.perf_ask_paste_cell(copy),
                Some(_) => self.copy_arm = None,
                None => self.perf_arm_copy(self.perf.cell),
            },
            FILES => self.perf_files_copy(),
            _ => {}
        }
    }
    /// Shift+Load (KILL) clears the lit type's object, as on the main screen.
    fn perf_kill(&mut self, value: u8) {
        let (op, what, loss): (&'static str, String, &str) = match value {
            0 => (
                "clear_sample",
                format!("Clear the sample of {:02}", self.parts[self.part] + 1),
                "The preset keeps its settings; its unsaved sample is lost.",
            ),
            2 => (
                "init_perf",
                "Clear the performance".into(),
                "All 32 presets become empty; unsaved changes are lost.",
            ),
            _ => (
                "clear_patch",
                format!("Clear preset {:02}", self.parts[self.part] + 1),
                "Its sample and settings are reset; unsaved changes are lost.",
            ),
        };
        self.perf_ask(vec![what], loss, "Clear", "", Verb::Op(op));
    }
    /// The Position slider sets a grabbed value when Position slider in menus
    /// is On (manual p.67).
    fn perf_slider(&mut self, value: u8) {
        if self.get("menu_slider") == 0. {
            return;
        }
        let Some(grab) = self.perf.grab else {
            return;
        };
        let (min, max, step) = match self.perf_field_kind(grab.field) {
            FieldKind::Number { min, max, step } => (min, max, step),
            FieldKind::Choice(options) => (0., options.len().saturating_sub(1) as f32, 1.),
            _ => return,
        };
        let v = ((min + (max - min) * value as f32 / 255.) / step).round() * step;
        self.perf_grab_set(v.clamp(min, max));
    }

    // ----- sheets and confirmations -----

    pub(super) fn perf_sheet(&mut self, title: String, items: Vec<Item>) {
        if items.is_empty() {
            return;
        }
        self.perf.layers.push(Layer::Sheet(Sheet {
            title,
            items,
            focus: 0,
        }));
    }
    /// A confirmation opens on Cancel .
    pub(super) fn perf_ask(
        &mut self,
        lines: Vec<String>,
        loss: &str,
        verb_label: &str,
        keycap: &'static str,
        verb: Verb,
    ) {
        self.perf.layers.push(Layer::Ask(Ask {
            lines,
            loss: loss.into(),
            verb_label: verb_label.into(),
            keycap,
            on_verb: false,
            verb,
        }));
    }
    fn perf_confirm(&mut self) {
        let Some(Layer::Ask(ask)) = self.perf.layers.pop() else {
            return;
        };
        self.perf.layers.clear();
        match ask.verb {
            Verb::Job(job) => self.perf_queue(job),
            Verb::LoadSlot(slot) => self.perf_queue(Job::LoadSlot(slot)),
            Verb::InitLoad(slot) => self.perf_queue(Job::InitLoad(slot)),
            Verb::CopyCell { kind, from, to } => self.perf_copy_cell(kind, from, to),
            Verb::Op(op) => {
                let label = ask.lines.first().cloned().unwrap_or_default();
                match self.perf_run_op(op) {
                    Ok(()) => {
                        if op == "unmount" {
                            self.perf_files_unmounted();
                        }
                        self.notify(&match op {
                            "factory" => "Device settings reset".to_string(),
                            "unmount" => "USB drive unmounted".to_string(),
                            "revert_patch" => "Reverted to the saved preset".to_string(),
                            "shutdown" => "Shutting down".to_string(),
                            _ => label.replacen("Clear", "Cleared", 1),
                        });
                    }
                    Err(e) => self.error(&e),
                }
            }
            Verb::Delete(paths) => self.perf_files_delete(&paths),
            Verb::Paste => self.perf_files_paste(),
            Verb::LoadPerfPath(path) => {
                self.queue(
                    "Loading performance".into(),
                    overlay::Verb::LoadPerfPath(path),
                    None,
                );
            }
        }
    }
    pub(super) fn perf_queue(&mut self, job: Job) {
        let label = match &job {
            Job::SaveSlot(i) => format!("Saving performance {:02}", i + 1),
            Job::LoadSlot(i) => format!("Loading performance {:02}", i + 1),
            Job::InitSlot(i) => format!("Initializing slot {:02}", i + 1),
            Job::InitLoad(i) => format!("Initializing performance {:02}", i + 1),
            Job::Revert => "Reverting the working performance".into(),
            Job::CopyAll | Job::TemplateAll => "Copying to all presets".into(),
            Job::ClearPerformance => "Clearing the working performance".into(),
        };
        self.queue(label, overlay::Verb::Perf(job), None);
    }
    /// Run a queued bulk operation; the frame naming it is on screen.
    pub(super) fn run_perf_job(&mut self, job: Job) {
        let source = self.parts[self.part];
        let name = self.patches[source].name.clone();
        let mut to_main = false;
        if self.device.is_none() {
            if let Job::SaveSlot(slot)
            | Job::LoadSlot(slot)
            | Job::InitSlot(slot)
            | Job::InitLoad(slot) = job
            {
                self.perf_demo_slots(slot);
            }
        }
        let result: Result<String, String> = match job {
            Job::SaveSlot(slot) => {
                self.selected_performance = slot;
                self.perf_run_op("save_perf_as")
                    .map(|_| format!("Saved performance {:02} {}", slot + 1, self.performance))
            }
            Job::LoadSlot(slot) => {
                to_main = true;
                self.selected_performance = slot;
                self.perf_run_op("load_perf")
                    .map(|_| format!("Loaded performance {:02} {}", slot + 1, self.performance))
            }
            Job::InitSlot(slot) => self
                .perf_initialize_slot(slot)
                .map(|_| format!("Initialized slot {:02}", slot + 1)),
            Job::InitLoad(slot) => {
                to_main = true;
                self.perf_initialize_slot(slot).and_then(|_| {
                    self.selected_performance = slot;
                    self.perf_run_op("load_perf")
                        .map(|_| format!("Loaded the initialized performance {:02}", slot + 1))
                })
            }
            Job::Revert => {
                self.selected_performance = self.performance_slot;
                self.perf_run_op("load_perf")
                    .map(|_| format!("Reverted to the saved {}", self.performance))
            }
            Job::CopyAll => self
                .perf_run_op("copy_all")
                .map(|_| format!("Copied {:02} {name} to all presets", source + 1)),
            Job::TemplateAll => self.perf_run_op("template_all").map(|_| {
                format!(
                    "Copied the settings of {:02} {name} to all presets",
                    source + 1
                )
            }),
            Job::ClearPerformance => self
                .perf_run_op("init_perf")
                .map(|_| "Cleared the working performance".into()),
        };
        self.refresh_performance_list();
        self.perf.slot_info.clear();
        self.sync_device_view();
        match result {
            Ok(text) => {
                if to_main {
                    self.perf_to_main();
                }
                self.notify(&text);
            }
            Err(e) => self.error(&e),
        }
    }
    /// An existing confirm operation: the device operation on the instrument,
    /// the demo state's on the desktop.
    pub(super) fn perf_run_op(&mut self, op: &'static str) -> Result<(), String> {
        match self.run_device_op(op) {
            Some(result) => {
                self.sync_device_view();
                result
            }
            None => {
                self.confirm(op);
                Ok(())
            }
        }
    }

    // ----- the grab model  -----

    pub(super) fn perf_field_kind(&self, field: Field) -> FieldKind {
        match field {
            Field::Setting(i) => match &self.catalog[i].kind {
                Kind::Toggle => FieldKind::Toggle,
                Kind::Choice(options) => {
                    FieldKind::Choice(options.iter().map(|o| o.to_string()).collect())
                }
                Kind::Number { min, max, step, .. } => FieldKind::Number {
                    min: *min,
                    max: *max,
                    step: *step,
                },
            },
            Field::Appearance => FieldKind::Choice(vec!["Light".into(), "Dark".into()]),
            Field::Brightness => {
                FieldKind::Choice(BRIGHTNESS.iter().map(|b| format!("{b}%")).collect())
            }
            Field::Multi => FieldKind::Choice((1..=4).map(|i| i.to_string()).collect()),
            Field::PartOn(_) => FieldKind::Toggle,
            Field::PartPreset(_) => FieldKind::Choice(
                (0..32)
                    .map(|i| format!("{:02} {}", i + 1, self.patches[i].name))
                    .collect(),
            ),
            Field::PartLevel(_) => FieldKind::Number {
                min: 0.,
                max: 100.,
                step: 1.,
            },
            Field::PartChannel(_) => FieldKind::Number {
                min: 0.,
                max: 16.,
                step: 1.,
            },
            Field::Disk | Field::Firmware | Field::Reset | Field::Shutdown => FieldKind::Action,
        }
    }
    pub(super) fn perf_field_value(&self, field: Field) -> f32 {
        match field {
            Field::Setting(i) => self.value(&self.catalog[i]),
            Field::Appearance => match self.appearance.theme {
                crate::appearance::Theme::White => 0.,
                crate::appearance::Theme::Dark => 1.,
            },
            Field::Brightness => BRIGHTNESS
                .iter()
                .position(|b| *b >= self.appearance.brightness)
                .unwrap_or(BRIGHTNESS.len() - 1) as f32,
            Field::Multi => self.snapshot as f32,
            Field::PartOn(p) => f32::from(self.enabled[p]),
            Field::PartPreset(p) => self.parts[p] as f32,
            Field::PartLevel(p) => self.volumes[p],
            Field::PartChannel(p) => self.get(&format!("midi{}", p + 1)),
            _ => 0.,
        }
    }
    /// The value as the field shows it.
    pub(super) fn perf_field_text(&self, field: Field, value: f32) -> String {
        match (field, self.perf_field_kind(field)) {
            (Field::PartChannel(_), _) if value < 0.5 => "Off".into(),
            (Field::PartChannel(_), _) => format!("Ch {value:.0}"),
            (Field::PartLevel(_), _) => format!("{value:.0}%"),
            (Field::Setting(i), _) => Self::val_text(&self.catalog[i], value),
            (_, FieldKind::Toggle) => if value > 0. { "On" } else { "Off" }.into(),
            (_, FieldKind::Choice(options)) => options
                .get(value.round().max(0.) as usize)
                .cloned()
                .unwrap_or_default(),
            (Field::Disk, _) => match &self.perf.free {
                Some(Ok(bytes)) => format!("{:.1} GB free", *bytes as f64 / 1e9),
                Some(Err(_)) => "Unknown".into(),
                None => String::new(),
            },
            _ => String::new(),
        }
    }
    pub(super) fn perf_field_press(&mut self, field: Field) {
        match self.perf_field_kind(field) {
            FieldKind::Toggle => {
                let value = self.perf_field_value(field);
                self.perf_field_apply(field, if value > 0. { 0. } else { 1. });
            }
            FieldKind::Action => self.perf_field_action(field),
            _ => {
                let value = self.perf_field_value(field);
                self.perf.grab = Some(Grab {
                    field,
                    original: value,
                    value,
                });
            }
        }
    }
    fn perf_grab_turn(&mut self, delta: i32) {
        let Some(grab) = self.perf.grab else {
            return;
        };
        let value = match self.perf_field_kind(grab.field) {
            FieldKind::Number { min, max, step } => {
                (grab.value + delta as f32 * step).clamp(min, max)
            }
            FieldKind::Choice(options) => {
                (grab.value + delta as f32).clamp(0., options.len().saturating_sub(1) as f32)
            }
            _ => return,
        };
        self.perf_grab_set(value);
    }
    /// Numbers reach the engine at once; choices only show in amber until
    /// Select, except Brightness, whose screen dims while turning (4.8).
    fn perf_grab_set(&mut self, value: f32) {
        let Some(grab) = self.perf.grab.as_mut() else {
            return;
        };
        grab.value = value;
        let field = grab.field;
        match (field, self.perf_field_kind(field)) {
            (Field::Brightness, _) => {
                self.appearance.brightness = BRIGHTNESS[value as usize];
            }
            (_, FieldKind::Number { .. }) => self.perf_field_apply(field, value),
            _ => {}
        }
    }
    /// Select keeps the grabbed value; a choice applies now.
    pub(super) fn perf_keep(&mut self) {
        let Some(grab) = self.perf.grab.take() else {
            return;
        };
        if grab.value != grab.original || grab.field == Field::Brightness {
            self.perf_field_apply(grab.field, grab.value);
        }
    }
    /// Escape restores the value from the moment of the grab.
    fn perf_restore(&mut self) {
        let Some(grab) = self.perf.grab.take() else {
            return;
        };
        match (grab.field, self.perf_field_kind(grab.field)) {
            (Field::Brightness, _) => {
                self.appearance.brightness = BRIGHTNESS[grab.original as usize];
            }
            (_, FieldKind::Number { .. }) if grab.value != grab.original => {
                self.perf_field_apply(grab.field, grab.original)
            }
            _ => {}
        }
    }
    /// Write a field through the existing state paths.
    fn perf_field_apply(&mut self, field: Field, value: f32) {
        match field {
            Field::Setting(i) => self.set(i, value),
            Field::Appearance => {
                let theme = if value > 0.5 {
                    crate::appearance::Theme::Dark
                } else {
                    crate::appearance::Theme::White
                };
                self.action(Action::Theme(theme));
            }
            Field::Brightness => {
                self.action(Action::Brightness(BRIGHTNESS[value as usize]));
            }
            Field::Multi => self.action(Action::Snapshot(value as usize)),
            Field::PartOn(p) => {
                if (value > 0.) != self.enabled[p] {
                    self.perf_toggle_part(p);
                }
            }
            Field::PartPreset(p) => self.perf_assign(value as usize, p),
            Field::PartLevel(p) => {
                if !self.device_live(LiveTarget::PartVolume(p), value) {
                    self.volumes[p] = value;
                }
            }
            Field::PartChannel(p) => {
                let i = self.setting_index(&format!("midi{}", p + 1));
                self.set(i, value);
            }
            _ => {}
        }
    }
    fn perf_field_action(&mut self, field: Field) {
        match field {
            Field::Disk => {
                self.perf_read_free();
                let text = self.perf_field_text(Field::Disk, 0.);
                self.notify(&text);
            }
            Field::Firmware => self.notify(match &self.device {
                Some(_) => "Engine firmware 3.4.4 (official); interface built from this repository",
                None => "Firmware 3.4.4 target; desktop demo engine",
            }),
            Field::Reset => self.perf_ask(
                vec!["Reset device settings".into()],
                "Saved patches, performances and samples stay.",
                "Reset",
                "",
                Verb::Op("factory"),
            ),
            Field::Shutdown => self.perf_ask(
                vec!["Shut down".into(), "the instrument".into()],
                "Unsaved changes are lost.",
                "Shut down",
                "",
                Verb::Op("shutdown"),
            ),
            _ => {}
        }
    }
    pub(super) fn perf_read_free(&mut self) {
        self.perf.free = Some(match &self.device {
            Some(d) => d.free_bytes(),
            None => Ok(2_400_000_000),
        });
    }

    /// The System page's fields in order, with their section names.
    pub(super) fn perf_system_rows(&self) -> Vec<(&'static str, Vec<Field>)> {
        let section = |group: &str| -> Vec<Field> {
            self.catalog
                .iter()
                .enumerate()
                .filter(|(_, s)| s.page == "System" && s.group == group)
                .map(|(i, _)| Field::Setting(i))
                .collect()
        };
        let mut device = section("Device");
        device.extend([Field::Disk, Field::Firmware, Field::Reset, Field::Shutdown]);
        vec![
            ("Display", vec![Field::Appearance, Field::Brightness]),
            ("Controls", section("Controls")),
            ("MIDI", section("MIDI")),
            ("Audio", section("Audio")),
            ("Device", device),
        ]
    }
    pub(super) fn perf_system_fields(&self) -> Vec<Field> {
        self.perf_system_rows()
            .into_iter()
            .flat_map(|(_, fields)| fields)
            .collect()
    }

    /// Desktop keys while the root or its confirmation is active: they stand
    /// in for the panel and go through the panel dispatcher.
    pub(super) fn perf_key(&mut self, key: &str) -> Option<Vec<PanelEvent>> {
        if !self.perf_active() {
            return None;
        }
        let event = |action, value| PanelEvent {
            action,
            value,
            text: None,
        };
        Some(vec![match key {
            "Up" | "Left" => event(5, 255),
            "Down" | "Right" => event(5, 1),
            "Return" | "Keypad Enter" => event(7, 0),
            "Escape" => event(14, 0),
            "F3" => event(12, 0),
            "F7" => event(4, 0),
            _ => return None,
        }])
    }
}

/// The field at `index` inside part row `part`.
pub(super) fn part_field(part: usize, index: usize) -> Field {
    match index {
        0 => Field::PartOn(part),
        1 => Field::PartPreset(part),
        2 => Field::PartLevel(part),
        _ => Field::PartChannel(part),
    }
}
