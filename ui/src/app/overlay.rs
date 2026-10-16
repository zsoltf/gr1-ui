//! file screens : the Load browser, the Save sheet, the
//! name editor, action sheets and confirmations. They stack above the main
//! screen or a menu page; the top layer receives the panel. Focus belongs to
//! row identity and every container remembers its focus. Nothing is written
//! until a load, a Save or a confirmed verb; the engine and the files stay the
//! state owners, reached through the existing device operations.
use super::components::ListCursor;
use super::device_ui::encoder_delta;
use super::*;
use crate::device::native::PanelEvent;

/// The panel's data type, lit by the Sample, Patch and Perf buttons.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub(super) enum DataKind {
    Sample,
    #[default]
    Patch,
    Perf,
}
impl Browser {
    pub(super) fn row_path(&self, row: &Row) -> String {
        match &self.place {
            Place::Folder(path) => join(path, &row.key),
            _ => row.key.clone(),
        }
    }
}

impl DataKind {
    pub(super) fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Sample,
            2 => Self::Perf,
            _ => Self::Patch,
        }
    }
    pub(super) fn chip(self) -> &'static str {
        match self {
            Self::Sample => "Sample",
            Self::Patch => "Patch",
            Self::Perf => "Perf",
        }
    }
    pub(super) fn word(self) -> &'static str {
        match self {
            Self::Sample => "sample",
            Self::Patch => "patch",
            Self::Perf => "performance",
        }
    }
    fn next(self) -> Self {
        match self {
            Self::Sample => Self::Patch,
            Self::Patch => Self::Perf,
            Self::Perf => Self::Sample,
        }
    }
}

/// Panel behavior that depends on the pre-file instrument checks .
/// Each switch is set to what the instrument showed on 2026-10-12, or to the
/// design's fallback where the check was not observed.
#[derive(Clone, Copy, Debug)]
pub(super) struct PanelPolicy {
    /// Observed: LOAD's value is the lit type (0 Sample, 1 Patch, 2 Perf).
    pub load_value_names_type: bool,
    /// Unobserved: whether SAVE's value is the lit type. Fallback: the last
    /// type-button event (33-35, observed on every screen), Patch until one
    /// arrives; the sheet's type chips stay switchable.
    pub save_value_names_type: bool,
    /// Unobserved: how Shift+Load arrives in the browser. Fallback: KILL (29)
    /// on a focused sample folder asks to fill presets, and does nothing on
    /// anything else; it never clears in the browser. Filling also stays on
    /// Load and in the folder's sheet.
    pub kill_fills_in_browser: bool,
    /// Unobserved: Shift+press in file contexts. Fallback: UNMOUNT (28) opens
    /// the focused row's sheet, where Unmount is an explicit item.
    pub unmount_opens_sheet: bool,
    /// Shift+press arriving as ENC_PUSH while the engine reports Shift also
    /// opens the focused row's sheet.
    pub shifted_press_opens_sheet: bool,
    /// Observed: with Shift held the engine reports 10 per detent itself. The
    /// interface never multiplies again; in the name editor one Shift-held
    /// report jumps one character group, whatever its size.
    pub shift_turn_jumps_group: bool,
    /// Observed: Shift+Escape arrives as ESC (14) with the Shift flag; MAIN
    /// (6) is not sent for it.
    pub shifted_escape: bool,
    /// Unobserved: SDL text from a USB keyboard. When it arrives it types at
    /// the name editor's cursor; without it naming is strip-only.
    pub keyboard_text: bool,
}
impl Default for PanelPolicy {
    fn default() -> Self {
        Self {
            load_value_names_type: true,
            save_value_names_type: false,
            kill_fills_in_browser: true,
            unmount_opens_sheet: true,
            shifted_press_opens_sheet: true,
            shift_turn_jumps_group: true,
            shifted_escape: true,
            keyboard_text: true,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(super) enum Place {
    /// Internal, the USB stick, Factory; for Perf also User slots.
    Locations,
    /// The 50 internal performance slots.
    Slots,
    Folder(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Purpose {
    Load,
    /// Choose the Save sheet's sample folder.
    Folder,
    /// Choose the Save sheet's performance slot.
    Slot,
}

#[derive(Clone, PartialEq, Debug)]
pub(super) enum Role {
    Location(Place),
    Folder,
    /// A loadable file, or a folder holding one patch or one performance.
    Item,
    Slot {
        index: usize,
        empty: bool,
    },
}

#[derive(Clone, Debug)]
pub(super) struct Row {
    /// Identity within its container: the file or folder name, or the slot.
    pub key: String,
    pub label: String,
    pub role: Role,
    /// Right column: item count, length, "Patch", "Perf".
    pub right: String,
    pub current: bool,
}

pub(super) struct Browser {
    pub kind: DataKind,
    pub purpose: Purpose,
    pub place: Place,
    pub rows: Vec<Row>,
    pub list: ListCursor,
    /// Second line of the focused row, read once per focus.
    pub details: Option<(String, String)>,
    pub error: Option<String>,
    /// Metadata comes from the filesystem in device mode, not the demo names.
    pub real_files: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum SaveField {
    Type,
    Name,
    Into,
    Save,
    Cancel,
}

pub(super) struct SaveSheet {
    pub kind: DataKind,
    pub name: String,
    /// Sample destination folder.
    pub folder: String,
    /// The folder's names, read when it is chosen rather than every frame.
    pub names: Vec<String>,
    /// Performance destination slot; None until chosen for an imported one.
    pub slot: Option<usize>,
    pub focus: SaveField,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum NamePurpose {
    /// The Save sheet below receives the name; Save also writes.
    Sheet,
    Rename(String),
    /// A new folder in the Files page's directory.
    NewFolder,
    Patch,
    Performance,
    /// A saved performance in a user slot (the Perf root's Rename).
    Slot(usize),
}

pub(super) struct NameEditor {
    pub title: String,
    pub chars: Vec<char>,
    pub cursor: usize,
    pub limit: usize,
    pub purpose: NamePurpose,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Act {
    Open,
    Load,
    Fill,
    Rename,
    Delete,
    SaveHere(usize),
    Unmount,
}

#[derive(Clone, Debug)]
pub(super) struct SheetItem {
    pub label: String,
    pub keycap: Option<&'static str>,
    pub destructive: bool,
    pub act: Act,
}

pub(super) struct ActionSheet {
    pub title: String,
    pub items: Vec<SheetItem>,
    pub focus: usize,
    /// The row it acts on, by identity.
    pub path: String,
    pub row: Row,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Verb {
    LoadSlot(usize),
    LoadPerfPath(String),
    Fill(String),
    Delete(String),
    SavePerf(usize),
    ReplaceSample,
    Unmount,
    /// An existing confirmation operation (`App::confirm`), such as
    /// clear_sample from the Sample root.
    Run {
        op: &'static str,
        slot: usize,
    },
    /// A Perf root bulk operation (Perf).
    Perf(super::perf_root::Job),
}

pub(super) struct Confirm {
    pub lines: Vec<String>,
    pub loss: String,
    pub verb: String,
    /// The panel button that started it, which also confirms.
    pub keycap: &'static str,
    pub on_verb: bool,
    pub action: Verb,
}

pub(super) enum Layer {
    Browser(Browser),
    Save(SaveSheet),
    Name(NameEditor),
    Sheet(ActionSheet),
    Confirm(Confirm),
}

/// Native bulk operations run after one frame naming them .
pub(super) struct Pending {
    pub label: String,
    pub verb: Verb,
    pub name: Option<String>,
}

pub(super) struct FilesUi {
    pub layers: Vec<Layer>,
    pub lit: DataKind,
    pub policy: PanelPolicy,
    pub pending: Option<Pending>,
    /// Last place per type and purpose.
    places: HashMap<(DataKind, bool), Place>,
    /// Focused row key and first visible row per container.
    memory: HashMap<String, (String, usize)>,
    /// The name editor's strip position, kept between uses.
    pub strip: usize,
    /// Sample headers of the current listing, by path.
    pub info: HashMap<String, Option<super::overlay_draw::SampleInfo>>,
    /// The focused sample to preview once focus rests.
    pub preview_due: Option<(Instant, String)>,
    pub listing: Option<super::listing::Listing>,
    pub folders: HashMap<String, Vec<String>>,
}

impl Default for FilesUi {
    fn default() -> Self {
        Self {
            layers: vec![],
            lit: DataKind::default(),
            policy: PanelPolicy::default(),
            pending: None,
            places: HashMap::new(),
            memory: HashMap::new(),
            // The strip starts on A, after its controls.
            strip: STRIP_CONTROLS.len(),
            info: HashMap::new(),
            preview_due: None,
            listing: None,
            folders: HashMap::new(),
        }
    }
}

/// The name editor's strip : controls, then character groups.
pub(super) const STRIP_CONTROLS: [&str; 6] = ["Done", "Delete", "<", ">", "Space", "Clear"];
pub(super) const STRIP_GROUPS: [&str; 4] = [
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
    "abcdefghijklmnopqrstuvwxyz",
    "0123456789",
    "-_.()+&#",
];
/// The location among `(index, root)` holding `path`. The deepest root wins:
/// on a legacy data root Factory lies inside Internal and stays read-only.
pub(super) fn location_containing(
    path: &str,
    roots: impl Iterator<Item = (usize, String)>,
) -> Option<(usize, String)> {
    roots
        .filter(|(_, root)| path == root || path.starts_with(&format!("{root}/")))
        .max_by_key(|(_, root)| root.len())
}
pub(super) fn strip_len() -> usize {
    STRIP_CONTROLS.len() + STRIP_GROUPS.iter().map(|g| g.len()).sum::<usize>()
}
/// The strip item at `index`: a control's label or a character.
pub(super) fn strip_item(index: usize) -> Result<&'static str, char> {
    if index < STRIP_CONTROLS.len() {
        return Ok(STRIP_CONTROLS[index]);
    }
    let mut i = index - STRIP_CONTROLS.len();
    for group in STRIP_GROUPS {
        if i < group.len() {
            return Err(group.as_bytes()[i] as char);
        }
        i -= group.len();
    }
    Ok("Done")
}
/// Start index of each strip group, the controls first.
pub(super) fn strip_group_starts() -> Vec<usize> {
    let mut starts = vec![0];
    let mut at = STRIP_CONTROLS.len();
    for group in STRIP_GROUPS {
        starts.push(at);
        at += group.len();
    }
    starts
}

const AUDIO: [&str; 8] = ["wav", "wave", "aif", "aiff", "flac", "ogg", "avr", "xi"];
const PATCH_NAME_LIMIT: usize = 16;
const PREVIEW_REST: Duration = Duration::from_millis(250);
const PERF_NAME_LIMIT: usize = 32;
const FILE_NAME_LIMIT: usize = 255;

fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}
pub(super) fn is_audio(name: &str) -> bool {
    !name.ends_with('/') && AUDIO.contains(&extension(name).as_str())
}
fn join(folder: &str, name: &str) -> String {
    format!(
        "{}/{}",
        folder.trim_end_matches('/'),
        name.trim_end_matches('/')
    )
}
fn parent(path: &str) -> Option<(String, String)> {
    path.rsplit_once('/')
        .map(|(p, n)| (p.to_string(), n.to_string()))
        .filter(|(p, _)| !p.is_empty())
}

impl App {
    // ----- entry and routing -----

    pub(super) fn overlay_active(&self) -> bool {
        !self.files_ui.layers.is_empty()
    }
    pub(super) fn overlay_screen_id(&self) -> Option<i32> {
        match self.files_ui.layers.last()? {
            Layer::Name(_) => Some(2),
            _ => Some(1),
        }
    }
    /// The type Load or Save acts on: its event value once the instrument
    /// shows it names the lit LED, else the last type button.
    fn event_type(&mut self, value: u8, named: bool) -> DataKind {
        if named && value <= 2 {
            self.files_ui.lit = DataKind::from_index(value as usize);
        }
        self.files_ui.lit
    }
    /// Panel events for the file screens. True when consumed.
    pub(super) fn overlay_event(&mut self, event: &PanelEvent) -> bool {
        if (33..=35).contains(&event.action) {
            self.files_ui.lit = DataKind::from_index((event.action - 33) as usize);
        }
        if !self.overlay_active() {
            return match event.action {
                1 => {
                    let kind =
                        self.event_type(event.value, self.files_ui.policy.load_value_names_type);
                    self.open_browser(kind);
                    true
                }
                2 => {
                    let kind =
                        self.event_type(event.value, self.files_ui.policy.save_value_names_type);
                    self.open_save_sheet(kind);
                    true
                }
                _ => false,
            };
        }
        if !matches!(event.action, 8..=11 | 16 | 17 | 20 | 21 | 23 | 25..=27 | 30 | 31) {
            // An error notice stays until the next input.
            self.clear_error();
        }
        let policy = self.files_ui.policy;
        match event.action {
            5 => {
                let delta = encoder_delta(event.value);
                let jump = policy.shift_turn_jumps_group && self.shift;
                self.overlay_turn(delta, jump);
            }
            7 if policy.shifted_press_opens_sheet && self.shift && self.on_load_row() => {
                self.overlay_sheet()
            }
            7 => self.overlay_press(),
            28 if policy.unmount_opens_sheet => self.overlay_sheet(),
            28 => {
                self.close_overlay();
                return false;
            }
            14 if policy.shifted_escape && self.shift => self.overlay_shift_escape(),
            14 => self.overlay_escape(),
            // MAIN precedes SET_PRESET when a preset button is pressed. In the
            // browser and the Save sheet the new preset is the destination
            // , so both stay open and SET_PRESET goes on to the
            // engine readback.
            6 | 10 => {
                if matches!(
                    self.files_ui.layers.last(),
                    Some(Layer::Confirm(Confirm {
                        action: Verb::Run { .. },
                        ..
                    }))
                ) {
                    // A Sample-root clear is a menu operation, not a file
                    // destination picker. A preset press cancels it.
                    self.finish_to_main();
                }
            }
            1 => self.overlay_load(),
            29 => {
                if policy.kill_fills_in_browser {
                    self.overlay_kill()
                }
            }
            2 | 19 => self.overlay_save(),
            33..=35 => self.overlay_type(DataKind::from_index((event.action - 33) as usize)),
            // Copy, paste and the Position slider have no meaning here. NOTEON
            // feeds keyboard-tuning learn, a grabbed field of Patch's Keyboard
            // page; like today's dispatcher, file ignores it.
            3 | 4 | 22 | 24 => {}
            12 | 13 | 15 | 18 | 32 => {
                // Another screen: the overlay closes and the old handler opens it.
                self.close_overlay();
                return false;
            }
            _ => return false,
        }
        self.navigation_dirty = true;
        true
    }
    fn on_load_row(&self) -> bool {
        matches!(self.files_ui.layers.last(), Some(Layer::Browser(b)) if b.purpose == Purpose::Load)
    }
    pub(super) fn close_overlay(&mut self) {
        if matches!(
            self.files_ui.listing.as_ref().map(|l| &l.target),
            Some(super::listing::Target::Browser { .. })
        ) {
            self.cancel_listing();
        }
        while let Some(layer) = self.files_ui.layers.pop() {
            if let Layer::Browser(b) = &layer {
                self.remember_browser(b);
            }
        }
        self.files_ui.preview_due = None;
        if let Some(d) = &mut self.device {
            d.stop_preview();
        }
        self.navigation_dirty = true;
    }
    /// After a load: the main screen, as stock does.
    fn finish_to_main(&mut self) {
        self.close_overlay();
        self.perf_release();
        self.modal = None;
        self.live = None;
        self.panel_sidebar = false;
        self.list_action_origin = None;
        self.tab = 0;
        self.group = 0;
        self.page = 0;
    }
    pub(super) fn clear_error(&mut self) {
        if self.toast_error {
            self.toast.clear();
            self.toast_error = false;
        }
    }
    pub(super) fn error(&mut self, text: &str) {
        self.notify(text);
        self.toast_error = true;
    }

    fn overlay_turn(&mut self, delta: i32, jump: bool) {
        if delta == 0 {
            return;
        }
        match self.files_ui.layers.last_mut() {
            Some(Layer::Browser(b)) => {
                b.list.turn(delta, b.rows.len());
                b.details = None;
                self.preview_focus();
            }
            Some(Layer::Save(s)) => {
                let fields = s.fields();
                let at = fields.iter().position(|f| *f == s.focus).unwrap_or(0) as i32;
                let next = (at + delta).clamp(0, fields.len() as i32 - 1);
                s.focus = fields[next as usize];
            }
            Some(Layer::Name(_)) => {
                let len = strip_len() as i32;
                let strip = self.files_ui.strip as i32;
                self.files_ui.strip = if jump {
                    let starts = strip_group_starts();
                    let group = starts.iter().rposition(|s| *s as i32 <= strip).unwrap_or(0) as i32;
                    let target = (group + delta.signum()).rem_euclid(starts.len() as i32);
                    starts[target as usize]
                } else {
                    (strip + delta).clamp(0, len - 1) as usize
                };
            }
            Some(Layer::Sheet(s)) => {
                let last = s.items.len().saturating_sub(1) as i32;
                s.focus = (s.focus as i32 + delta).clamp(0, last) as usize;
            }
            Some(Layer::Confirm(c)) => c.on_verb = delta > 0,
            None => {}
        }
    }
    fn overlay_press(&mut self) {
        match self.files_ui.layers.last() {
            Some(Layer::Browser(b)) => {
                if let Some(row) = b.rows.get(b.list.focus).cloned() {
                    let purpose = b.purpose;
                    match (&row.role, purpose) {
                        (Role::Slot { index, .. }, Purpose::Slot) => self.pick_slot(*index),
                        (Role::Item, Purpose::Load) | (Role::Slot { .. }, Purpose::Load) => {
                            self.load_row(&row)
                        }
                        _ => self.enter_row(&row),
                    }
                }
            }
            Some(Layer::Save(s)) => match s.focus {
                SaveField::Type => {
                    let kind = s.kind.next();
                    self.overlay_type(kind);
                }
                SaveField::Name => self.open_name_editor(NamePurpose::Sheet),
                SaveField::Into => self.open_destination(),
                SaveField::Save => self.write_save(false),
                SaveField::Cancel => {
                    self.files_ui.layers.pop();
                }
            },
            Some(Layer::Name(_)) => self.press_strip(),
            Some(Layer::Sheet(s)) => {
                let act = s.items.get(s.focus).map(|i| i.act.clone());
                if let Some(act) = act {
                    self.run_act(act);
                }
            }
            Some(Layer::Confirm(c)) => {
                if c.on_verb {
                    self.run_confirm();
                } else {
                    self.files_ui.layers.pop();
                }
            }
            None => {}
        }
    }
    fn overlay_escape(&mut self) {
        let browser = match self.files_ui.layers.last() {
            Some(Layer::Browser(b)) => Some((b.place.clone(), b.purpose)),
            Some(_) => None,
            None => return,
        };
        let Some((place, purpose)) = browser else {
            self.pop_layer();
            return;
        };
        // Escape climbs one level: folder, location root, locations, close.
        let up = match place {
            Place::Folder(path) => match self.location_of(&path) {
                Some((i, root)) if root == path => Some((Place::Locations, format!("location{i}"))),
                _ => parent(&path).map(|(p, name)| (Place::Folder(p), format!("{name}/"))),
            },
            Place::Slots if purpose == Purpose::Load => Some((Place::Locations, "slots".into())),
            _ => None,
        };
        match up {
            Some((place, from)) => self.browse_to(place, Some(from)),
            None => self.pop_layer(),
        }
    }
    fn pop_layer(&mut self) {
        if let Some(Layer::Browser(b)) = self.files_ui.layers.pop() {
            self.remember_browser(&b);
            if let Some(d) = &mut self.device {
                d.stop_preview();
            }
        }
    }
    fn overlay_shift_escape(&mut self) {
        if let Some(Layer::Name(n)) = self.files_ui.layers.last_mut() {
            if n.cursor > 0 {
                n.cursor -= 1;
                n.chars.remove(n.cursor);
            }
            return;
        }
        self.finish_to_main();
    }
    fn overlay_load(&mut self) {
        match self.files_ui.layers.last() {
            Some(Layer::Browser(b)) => {
                let Some(row) = b.rows.get(b.list.focus).cloned() else {
                    return;
                };
                match (b.purpose, &row.role) {
                    (Purpose::Load, Role::Item | Role::Slot { .. }) => self.load_row(&row),
                    (Purpose::Load, Role::Folder) if b.kind == DataKind::Sample => {
                        let path = self.row_path(&row);
                        self.ask_fill(path);
                    }
                    (Purpose::Load, _) => self.enter_row(&row),
                    _ => {}
                }
            }
            Some(Layer::Sheet(s)) => {
                if let Some(i) = s.items.iter().position(|i| i.keycap == Some("Load")) {
                    let act = s.items[i].act.clone();
                    self.run_act(act);
                }
            }
            Some(Layer::Confirm(c)) if c.keycap == "Load" => self.run_confirm(),
            _ => {}
        }
    }
    fn overlay_kill(&mut self) {
        if let Some(Layer::Browser(b)) = self.files_ui.layers.last() {
            if let (Purpose::Load, DataKind::Sample, Some(row)) =
                (b.purpose, b.kind, b.rows.get(b.list.focus).cloned())
            {
                if row.role == Role::Folder {
                    let path = self.row_path(&row);
                    self.ask_fill(path);
                }
            }
        }
    }
    fn overlay_save(&mut self) {
        match self.files_ui.layers.last() {
            Some(Layer::Browser(b)) => match b.purpose {
                Purpose::Load => {
                    let kind = b.kind;
                    self.open_save_sheet(kind);
                }
                Purpose::Folder => {
                    if let Place::Folder(path) = &b.place {
                        let path = path.clone();
                        self.pop_layer();
                        let names = self.list_folder(&path).unwrap_or_default();
                        if let Some(Layer::Save(s)) = self.files_ui.layers.last_mut() {
                            s.folder = path;
                            s.names = names;
                            s.focus = SaveField::Save;
                        }
                    }
                }
                Purpose::Slot => {
                    if let Some(Role::Slot { index, .. }) =
                        b.rows.get(b.list.focus).map(|r| &r.role)
                    {
                        let index = *index;
                        self.pick_slot(index);
                    }
                }
            },
            Some(Layer::Save(_)) => self.write_save(false),
            Some(Layer::Name(_)) => self.apply_name(true),
            Some(Layer::Sheet(s)) => {
                if let Some(i) = s.items.iter().position(|i| i.keycap == Some("Save")) {
                    let act = s.items[i].act.clone();
                    self.run_act(act);
                }
            }
            Some(Layer::Confirm(c)) if c.keycap == "Save" => self.run_confirm(),
            _ => {}
        }
    }
    fn overlay_type(&mut self, kind: DataKind) {
        self.files_ui.lit = kind;
        match self.files_ui.layers.last_mut() {
            Some(Layer::Browser(b)) if b.purpose == Purpose::Load && b.kind != kind => {
                let b = self.files_ui.layers.pop();
                if let Some(Layer::Browser(b)) = b {
                    self.remember_browser(&b);
                }
                self.open_browser(kind);
            }
            Some(Layer::Save(s)) if s.kind != kind => {
                let focus = s.focus;
                self.files_ui.layers.pop();
                self.open_save_sheet(kind);
                if let Some(Layer::Save(s)) = self.files_ui.layers.last_mut() {
                    if s.fields().contains(&focus) {
                        s.focus = focus;
                    }
                }
            }
            _ => {}
        }
    }
    /// Shift+press: the focused row's action sheet.
    fn overlay_sheet(&mut self) {
        let Some(Layer::Browser(b)) = self.files_ui.layers.last() else {
            return;
        };
        if b.purpose != Purpose::Load {
            return;
        }
        let Some(row) = b.rows.get(b.list.focus).cloned() else {
            return;
        };
        let kind = b.kind;
        let item = |label: &str, keycap, destructive, act| SheetItem {
            label: label.into(),
            keycap,
            destructive,
            act,
        };
        let writable = !self.read_only(&self.row_path(&row));
        let mut items = vec![];
        match &row.role {
            Role::Item => {
                items.push(item("Load", Some("Load"), false, Act::Load));
                if writable {
                    items.push(item("Rename", None, false, Act::Rename));
                    items.push(item("Delete", None, true, Act::Delete));
                }
            }
            Role::Folder => {
                items.push(item("Open", None, false, Act::Open));
                if kind == DataKind::Sample {
                    items.push(item("Fill presets", Some("Load"), false, Act::Fill));
                }
                if writable {
                    items.push(item("Rename", None, false, Act::Rename));
                    items.push(item("Delete", None, true, Act::Delete));
                }
            }
            Role::Slot { index, empty } => {
                if !empty {
                    items.push(item("Load", Some("Load"), false, Act::Load));
                }
                items.push(item(
                    "Save working here",
                    Some("Save"),
                    false,
                    Act::SaveHere(*index),
                ));
            }
            Role::Location(_) => {
                items.push(item("Open", None, false, Act::Open));
                if row.key == "location1" && self.device.is_some() {
                    items.push(item("Unmount", None, false, Act::Unmount));
                }
            }
        }
        let title = match &row.role {
            Role::Slot { index, .. } => format!("Slot {:02}  {}", index + 1, row.label),
            _ => row.label.clone(),
        };
        let path = self.row_path(&row);
        self.files_ui.layers.push(Layer::Sheet(ActionSheet {
            title,
            items,
            focus: 0,
            path,
            row,
        }));
    }

    // ----- the browser -----

    pub(super) fn open_browser(&mut self, kind: DataKind) {
        self.open_browser_for(kind, Purpose::Load);
    }
    fn open_browser_for(&mut self, kind: DataKind, purpose: Purpose) {
        self.live = None;
        self.modal = None;
        self.panel_sidebar = false;
        let place = match purpose {
            Purpose::Slot => Place::Slots,
            _ => self
                .files_ui
                .places
                .get(&(kind, purpose == Purpose::Folder))
                .cloned()
                .filter(|p| self.place_exists(p))
                .unwrap_or_else(|| match kind {
                    DataKind::Perf if purpose == Purpose::Load => Place::Slots,
                    _ => self
                        .location_root(0)
                        .map(Place::Folder)
                        .unwrap_or(Place::Locations),
                }),
        };
        let b = Browser {
            kind,
            purpose,
            place: Place::Locations,
            rows: vec![],
            list: ListCursor::default(),
            details: None,
            error: None,
            real_files: self.device.is_some(),
        };
        self.files_ui.layers.push(Layer::Browser(b));
        self.show_browser_place(Place::Locations, None);
        self.browse_to(place, None);
        self.navigation_dirty = true;
    }
    /// Show `place` in the top browser, focusing `focus` (a row key) or the
    /// container's remembered row.
    fn browse_to(&mut self, place: Place, focus: Option<String>) {
        if let (Place::Folder(path), Some(d)) = (&place, &self.device) {
            let job = d.start_listing(path.into());
            self.begin_listing(job, super::listing::Target::Browser { place, focus });
            return;
        }
        self.show_browser_place(place, focus);
    }
    pub(super) fn show_browser_place(&mut self, place: Place, focus: Option<String>) {
        let Some(Layer::Browser(b)) = self.files_ui.layers.pop() else {
            return;
        };
        self.remember_browser(&b);
        let mut b = b;
        let (rows, error) = match self.list_place(b.kind, b.purpose, &place) {
            Ok(rows) => (rows, None),
            Err(e) => (vec![], Some(e)),
        };
        let key = self.container_key(b.kind, b.purpose, &place);
        let memory = self.files_ui.memory.get(&key).cloned();
        let wanted = focus.or_else(|| memory.as_ref().map(|m| m.0.clone()));
        b.list.focus = wanted
            .and_then(|k| rows.iter().position(|r| r.key == k))
            .or_else(|| {
                // The current item is where a new visit starts.
                rows.iter().position(|r| r.current)
            })
            .unwrap_or(0);
        b.list.top = memory.map_or(0, |m| m.1);
        b.place = place;
        b.rows = rows;
        self.files_ui.info.clear();
        b.error = error;
        b.details = None;
        b.list.reveal(b.rows.len());
        self.files_ui.layers.push(Layer::Browser(b));
        self.preview_focus();
    }
    fn remember_browser(&mut self, b: &Browser) {
        if let Some(row) = b.rows.get(b.list.focus) {
            let key = self.container_key(b.kind, b.purpose, &b.place);
            self.files_ui
                .memory
                .insert(key, (row.key.clone(), b.list.top));
        }
        if b.purpose != Purpose::Slot {
            self.files_ui
                .places
                .insert((b.kind, b.purpose == Purpose::Folder), b.place.clone());
        }
    }
    fn container_key(&self, kind: DataKind, purpose: Purpose, place: &Place) -> String {
        match place {
            Place::Locations => format!("locations:{kind:?}"),
            Place::Slots if purpose == Purpose::Slot => "slot-pick".into(),
            Place::Slots => "slots".into(),
            Place::Folder(path) => format!("{kind:?}:{}:{path}", purpose == Purpose::Folder),
        }
    }
    fn enter_row(&mut self, row: &Row) {
        let place = match &row.role {
            Role::Location(place) => place.clone(),
            Role::Folder | Role::Item => Place::Folder(self.row_path(row)),
            Role::Slot { .. } => return,
        };
        self.browse_to(place, None);
    }
    pub(super) fn row_path(&self, row: &Row) -> String {
        match self.files_ui.layers.iter().rev().find_map(|l| match l {
            Layer::Browser(b) => Some(&b.place),
            _ => None,
        }) {
            Some(Place::Folder(path)) => join(path, &row.key),
            _ => row.key.clone(),
        }
    }
    /// Play the focused sample when sound previews are on (manual p.68).
    fn preview_focus(&mut self) {
        let Some(Layer::Browser(b)) = self.files_ui.layers.last() else {
            return;
        };
        if b.kind != DataKind::Sample || b.purpose != Purpose::Load {
            return;
        }
        let path = b
            .rows
            .get(b.list.focus)
            .filter(|r| r.role == Role::Item)
            .map(|r| self.row_path(r));
        // A preview loads the file natively; it waits until focus rests so
        // turning through a folder does not load every sample on the way.
        self.files_ui.preview_due = path.map(|p| (Instant::now() + PREVIEW_REST, p));
        if let Some(d) = &mut self.device {
            d.stop_preview();
        }
    }
    /// Start a due preview (device mode; sound previews on, manual p.68).
    pub(super) fn run_due_preview(&mut self) {
        let due = match &self.files_ui.preview_due {
            Some((at, _)) => *at <= Instant::now(),
            None => false,
        };
        if !due {
            return;
        }
        if let (Some((_, path)), Some(d)) = (self.files_ui.preview_due.take(), &mut self.device) {
            if self
                .files_ui
                .layers
                .iter()
                .any(|l| matches!(l, Layer::Browser(_)))
            {
                if let Err(e) = d.preview(Path::new(&path)) {
                    eprintln!("GR-1: {e}");
                }
            }
        }
    }

    /// The location roots, as (label, path or place).
    pub(super) fn location_root(&self, index: usize) -> Option<String> {
        match &self.device {
            Some(d) => d
                .location(index)
                .ok()
                .map(|p| p.to_string_lossy().into_owned()),
            None => match index {
                0 => Some("/Internal".into()),
                1 => self.usb_mounted.then(|| "/USB".into()),
                2 => Some("/Factory".into()),
                _ => None,
            },
        }
    }
    pub(super) fn location_label(&self, index: usize) -> String {
        match index {
            0 => "Internal".into(),
            2 => "Factory".into(),
            _ => self
                .location_root(1)
                .and_then(|p| p.rsplit('/').next().map(str::to_string))
                .filter(|n| !n.is_empty() && self.device.is_some())
                .unwrap_or_else(|| "USB".into()),
        }
    }
    /// The location holding `path`, and that location's root.
    pub(super) fn location_of(&self, path: &str) -> Option<(usize, String)> {
        location_containing(
            path,
            (0..3).filter_map(|i| self.location_root(i).map(|root| (i, root))),
        )
    }
    /// "Internal > Samples > Field" for a folder.
    pub(super) fn place_title(&self, place: &Place) -> String {
        match place {
            Place::Locations => "Locations".into(),
            Place::Slots => "User slots".into(),
            Place::Folder(path) => match self.location_of(path) {
                Some((i, root)) => std::iter::once(self.location_label(i))
                    .chain(
                        path[root.len()..]
                            .split('/')
                            .filter(|s| !s.is_empty())
                            .map(str::to_string),
                    )
                    .collect::<Vec<_>>()
                    .join("  \u{203a}  "),
                None => path.clone(),
            },
        }
    }
    fn place_exists(&self, place: &Place) -> bool {
        match place {
            Place::Folder(path) => self.location_of(path).is_some(),
            _ => true,
        }
    }
    fn read_only(&self, path: &str) -> bool {
        self.location_of(path).is_some_and(|(i, _)| i == 2)
    }
    /// Names in a folder: folders end in '/', folders first, then by name.
    pub(super) fn list_folder(&self, path: &str) -> Result<Vec<String>, String> {
        if let Some(names) = self.files_ui.folders.get(path) {
            return Ok(names.clone());
        }
        let mut names = match &self.device {
            Some(_) => return Err("Folder listing is not ready".into()),
            None => {
                if path == self.directory {
                    self.files.clone()
                } else if let Some(files) = self.folders.get(path) {
                    files.clone()
                } else {
                    match path {
                        "/USB" => vec!["FOREST.wav".into(), "TAPE_CHORD.wav".into()],
                        "/Factory" => vec!["CHOIR.wav".into(), "STRINGS.wav".into()],
                        _ if self.location_of(path).is_some() => vec![],
                        _ => return Err(format!("{path}: no such folder")),
                    }
                }
            }
        };
        names.sort_by_key(|s| (!s.ends_with('/'), s.to_lowercase()));
        Ok(names)
    }
    /// What a folder holds, for Patch and Perf annotations.
    fn folder_holds(&self, path: &str) -> Option<DataKind> {
        let names = self.list_folder(path).ok()?;
        if names
            .iter()
            .any(|n| n.eq_ignore_ascii_case("performance.grm"))
        {
            Some(DataKind::Perf)
        } else if names.iter().filter(|n| extension(n) == "grp").count() == 1 {
            Some(DataKind::Patch)
        } else {
            None
        }
    }
    fn list_place(
        &self,
        kind: DataKind,
        purpose: Purpose,
        place: &Place,
    ) -> Result<Vec<Row>, String> {
        let mut rows = vec![];
        match place {
            Place::Locations => {
                if kind == DataKind::Perf && purpose == Purpose::Load {
                    rows.push(Row {
                        key: "slots".into(),
                        label: "User slots".into(),
                        role: Role::Location(Place::Slots),
                        right: "50".into(),
                        current: false,
                    });
                }
                for i in 0..3 {
                    if let Some(root) = self.location_root(i) {
                        if purpose == Purpose::Folder && i == 2 {
                            continue;
                        }
                        rows.push(Row {
                            key: format!("location{i}"),
                            label: self.location_label(i),
                            role: Role::Location(Place::Folder(root)),
                            right: String::new(),
                            current: false,
                        });
                    }
                }
            }
            Place::Slots => {
                for index in 0..50 {
                    let (label, empty) = self.slot_name(index);
                    rows.push(Row {
                        key: format!("{index}"),
                        label,
                        role: Role::Slot { index, empty },
                        right: String::new(),
                        current: index == self.performance_slot && !self.imported_performance(),
                    });
                }
            }
            Place::Folder(path) => {
                let current_sample = self.patch().sample.clone();
                let current_name = self.patch().name.clone();
                for name in self.list_folder(path)? {
                    let full = join(path, &name);
                    if let Some(folder) = name.strip_suffix('/') {
                        let holds = match kind {
                            DataKind::Sample => None,
                            _ => self.folder_holds(&full),
                        };
                        if purpose == Purpose::Folder && holds.is_some() {
                            continue;
                        }
                        let (role, right) = match holds {
                            Some(k) if k == kind => (Role::Item, k.chip().to_string()),
                            Some(_) => continue,
                            None => (
                                Role::Folder,
                                self.list_folder(&full)
                                    .map(|n| n.len().to_string())
                                    .unwrap_or_default(),
                            ),
                        };
                        rows.push(Row {
                            key: name.clone(),
                            label: folder.into(),
                            current: role == Role::Item
                                && kind == DataKind::Patch
                                && folder == current_name,
                            role,
                            right,
                        });
                        continue;
                    }
                    if purpose == Purpose::Folder {
                        continue;
                    }
                    let wanted = match kind {
                        DataKind::Sample => is_audio(&name),
                        DataKind::Patch => extension(&name) == "grp",
                        DataKind::Perf => extension(&name) == "grm",
                    };
                    if !wanted {
                        continue;
                    }
                    let label = match kind {
                        DataKind::Sample => name.clone(),
                        _ => name
                            .rsplit_once('.')
                            .map_or(name.as_str(), |(s, _)| s)
                            .into(),
                    };
                    rows.push(Row {
                        current: match kind {
                            DataKind::Sample => name == current_sample,
                            DataKind::Patch => {
                                name.rsplit_once('.').map(|(s, _)| s) == Some(&current_name)
                            }
                            DataKind::Perf => false,
                        },
                        right: String::new(),
                        label,
                        key: name,
                        role: Role::Item,
                    });
                }
            }
        }
        Ok(rows)
    }
    pub(super) fn slot_name(&self, index: usize) -> (String, bool) {
        match &self.device {
            Some(d) => match d.performance_paths.get(index) {
                Some(path) if path.join("performance.grm").exists() => (
                    self.performance_library
                        .get(index)
                        .map(|p| p.0.clone())
                        .unwrap_or_default(),
                    false,
                ),
                _ => ("Empty".into(), true),
            },
            None => match self.performance_library.get(index) {
                Some((name, _)) if name != "Empty performance" => (name.clone(), false),
                _ => ("Empty".into(), true),
            },
        }
    }
    fn imported_performance(&self) -> bool {
        self.device.as_ref().is_some_and(|d| d.imported())
    }

    // ----- loading -----

    fn load_row(&mut self, row: &Row) {
        let Some(Layer::Browser(b)) = self.files_ui.layers.last() else {
            return;
        };
        let kind = b.kind;
        let path = self.row_path(row);
        match (&row.role, kind) {
            (Role::Slot { empty: true, .. }, _) => self.error("This slot is empty"),
            (Role::Slot { index, .. }, _) => {
                let verb = Verb::LoadSlot(*index);
                self.ask_load_perf(verb, &format!("slot {:02}", index + 1), Some(&row.label));
            }
            (_, DataKind::Perf) => {
                let verb = Verb::LoadPerfPath(path);
                self.ask_load_perf(verb, &row.label, None);
            }
            (_, DataKind::Sample) => self.load_sample_path(&path, &row.label),
            (_, DataKind::Patch) => self.load_patch_path(&path, &row.label),
        }
    }
    fn destination(&self) -> (usize, String) {
        let slot = self.parts[self.part];
        (slot, self.patches[slot].name.clone())
    }
    fn remember_focus_now(&mut self) {
        if let Some(Layer::Browser(b)) = self
            .files_ui
            .layers
            .iter()
            .rev()
            .find(|l| matches!(l, Layer::Browser(_)))
        {
            let snapshot = b.snapshot();
            self.remember_browser(&snapshot);
        }
    }
    pub(super) fn load_sample_path(&mut self, path: &str, name: &str) {
        self.remember_focus_now();
        let (slot, _) = self.destination();
        let result = match &mut self.device {
            Some(d) => {
                let slot = d.slot();
                d.load_sample(Path::new(path), slot)
            }
            None => {
                if self.recording == Some(slot) {
                    self.recording = None;
                }
                self.patch_mut().sample = name.into();
                self.patch_mut().dirty = true;
                Ok(())
            }
        };
        match result {
            Ok(()) => {
                self.sync_device_view();
                self.finish_to_main();
                self.notify(&format!("Loaded {name} into {:02}", slot + 1));
            }
            Err(e) => self.error(&format!("Load failed: {e}")),
        }
    }
    pub(super) fn load_patch_path(&mut self, path: &str, name: &str) {
        self.remember_focus_now();
        let (slot, _) = self.destination();
        let result = match &mut self.device {
            Some(d) => {
                let slot = d.slot();
                d.load_patch(Path::new(path), slot)
            }
            None => {
                let stem = name.rsplit_once('.').map_or(name, |(s, _)| s).to_string();
                self.patch_mut().name = stem;
                self.patch_mut().dirty = true;
                Ok(())
            }
        };
        match result {
            Ok(()) => {
                self.sync_device_view();
                let name = self.patch().name.clone();
                self.finish_to_main();
                self.notify(&format!("Loaded {name} into {:02}", slot + 1));
            }
            Err(e) => self.error(&format!("Load failed: {e}")),
        }
    }
    fn ask_load_perf(&mut self, verb: Verb, what: &str, name: Option<&str>) {
        let mut lines = vec![format!("Load {what}")];
        lines.extend(name.map(str::to_string));
        lines.push("as the working performance".into());
        self.files_ui.layers.push(Layer::Confirm(Confirm {
            lines,
            loss: if self.patches.iter().any(|p| p.dirty) {
                "Unsaved changes to the working performance are lost.".into()
            } else {
                "The working performance is replaced.".into()
            },
            verb: "Load".into(),
            keycap: "Load",
            on_verb: false,
            action: verb,
        }));
    }
    fn ask_fill(&mut self, path: String) {
        let count = self
            .list_folder(&path)
            .map(|n| n.iter().filter(|n| is_audio(n)).count().min(32))
            .unwrap_or(0);
        if count == 0 {
            self.error("This folder has no samples");
            return;
        }
        let name = path.rsplit('/').next().unwrap_or_default().to_string();
        self.files_ui.layers.push(Layer::Confirm(Confirm {
            lines: vec![
                format!("Fill presets 01-{count:02}"),
                format!("from {name}"),
            ],
            loss: "Their samples are replaced; unsaved changes are lost.".into(),
            verb: "Fill".into(),
            keycap: "Load",
            on_verb: false,
            action: Verb::Fill(path),
        }));
    }
    fn run_confirm(&mut self) {
        let Some(Layer::Confirm(c)) = self.files_ui.layers.pop() else {
            return;
        };
        match c.action {
            Verb::Delete(path) => self.delete_path(&path),
            Verb::ReplaceSample => self.write_save(true),
            Verb::SavePerf(slot) => self.queue_save_perf(slot),
            Verb::Unmount => match self.perf_run_op("unmount") {
                Ok(()) => {
                    self.browse_to(Place::Locations, None);
                    self.notify("USB drive unmounted");
                }
                Err(e) => self.error(&e),
            },
            Verb::Run { op, slot } => self.run_clear(op, slot),
            verb => {
                let label = match &verb {
                    Verb::LoadSlot(i) => format!("Loading performance {:02}", i + 1),
                    Verb::LoadPerfPath(_) => "Loading performance".into(),
                    _ => "Filling presets".into(),
                };
                self.remember_focus_now();
                self.queue(label, verb, None);
            }
        }
    }
    pub(super) fn queue(&mut self, label: String, verb: Verb, name: Option<String>) {
        self.notify(&label);
        self.toast_error = false;
        self.files_ui.pending = Some(Pending { label, verb, name });
    }
    /// Run a queued native bulk operation. The caller has presented one
    /// frame naming it; the screen does not update until it ends.
    pub(super) fn run_pending(&mut self) {
        let Some(pending) = self.files_ui.pending.take() else {
            return;
        };
        if let Verb::Perf(job) = &pending.verb {
            let job = job.clone();
            self.run_perf_job(job);
            return;
        }
        let result: Result<String, String> = match pending.verb {
            Verb::LoadSlot(index) => {
                let loaded = match &mut self.device {
                    Some(d) => d.load_performance(index),
                    None => {
                        self.selected_performance = index;
                        self.confirm("load_perf");
                        Ok(())
                    }
                };
                self.refresh_performance_list();
                self.sync_device_view();
                loaded.map(|_| format!("Loaded performance {:02} {}", index + 1, self.performance))
            }
            Verb::LoadPerfPath(path) => {
                let name = path.rsplit('/').next().unwrap_or_default().to_string();
                match &mut self.device {
                    Some(d) => d
                        .load_performance_path(Path::new(&path))
                        .map(|_| format!("Loaded {name}; save it to a user slot to keep it")),
                    None => Err("Performance files load on the instrument".into()),
                }
            }
            Verb::Fill(path) => self.fill_presets(&path),
            Verb::SavePerf(slot) => {
                let saved = match &mut self.device {
                    Some(d) => {
                        if let Some(name) = &pending.name {
                            d.performance.document.put("name", name.as_str());
                        }
                        d.save_performance(slot)
                    }
                    None => {
                        if let Some(name) = &pending.name {
                            self.performance = name.clone();
                        }
                        self.selected_performance = slot;
                        self.confirm("save_perf_as");
                        Ok(())
                    }
                };
                self.refresh_performance_list();
                self.sync_device_view();
                saved.map(|_| format!("Saved performance {:02} {}", slot + 1, self.performance))
            }
            _ => Ok(String::new()),
        };
        self.sync_device_view();
        match result {
            Ok(text) => {
                self.finish_to_main();
                self.notify(&text);
            }
            Err(e) => {
                let _ = pending.label;
                self.error(&e)
            }
        }
    }
    fn fill_presets(&mut self, path: &str) -> Result<String, String> {
        let names: Vec<String> = self
            .list_folder(path)?
            .into_iter()
            .filter(|n| is_audio(n))
            .take(32)
            .collect();
        for (i, name) in names.iter().enumerate() {
            match &mut self.device {
                Some(d) => d.load_sample(Path::new(&join(path, name)), i)?,
                None => {
                    self.patches[i].sample = name.clone();
                    self.patches[i].name = name
                        .rsplit_once('.')
                        .map_or(name.as_str(), |(s, _)| s)
                        .into();
                    self.patches[i].dirty = true;
                }
            }
        }
        Ok(format!("Filled presets 01-{:02}", names.len()))
    }

    // ----- action sheet verbs -----

    fn run_act(&mut self, act: Act) {
        let Some(Layer::Sheet(sheet)) = self.files_ui.layers.pop() else {
            return;
        };
        let row = sheet.row.clone();
        match act {
            Act::Open => self.enter_row(&row),
            Act::Load => self.load_row(&row),
            Act::Fill => self.ask_fill(sheet.path),
            Act::Rename => {
                self.files_ui.layers.push(Layer::Name(NameEditor {
                    title: format!(
                        "Rename {}",
                        if row.role == Role::Folder {
                            "folder"
                        } else {
                            "file"
                        }
                    ),
                    chars: row.key.trim_end_matches('/').chars().collect(),
                    cursor: row.key.trim_end_matches('/').chars().count(),
                    limit: FILE_NAME_LIMIT,
                    purpose: NamePurpose::Rename(sheet.path),
                }));
            }
            Act::Unmount => {
                self.files_ui.layers.push(Layer::Confirm(Confirm {
                    lines: vec![format!("Unmount {}", row.label)],
                    loss: "Unfinished file writes end first.".into(),
                    verb: "Unmount".into(),
                    keycap: "",
                    on_verb: false,
                    action: Verb::Unmount,
                }));
            }
            Act::Delete => {
                self.files_ui.layers.push(Layer::Confirm(Confirm {
                    lines: vec![format!("Delete {}", row.label)],
                    loss: "It moves to this folder's trash.".into(),
                    verb: "Delete".into(),
                    keycap: "",
                    on_verb: false,
                    action: Verb::Delete(sheet.path),
                }));
            }
            Act::SaveHere(index) => {
                self.open_save_sheet(DataKind::Perf);
                if let Some(Layer::Save(s)) = self.files_ui.layers.last_mut() {
                    s.slot = Some(index);
                }
            }
        }
    }
    fn top_browser_place(&self) -> Option<Place> {
        self.files_ui.layers.iter().rev().find_map(|l| match l {
            Layer::Browser(b) => Some(b.place.clone()),
            _ => None,
        })
    }
    fn relist(&mut self, focus: Option<String>) {
        if let Some(place) = self.top_browser_place() {
            self.browse_to(place, focus);
        }
    }
    fn delete_path(&mut self, path: &str) {
        let result = match &mut self.device {
            Some(d) => d.trash_path(Path::new(path)),
            None => self.demo_edit(path, None),
        };
        match result {
            Ok(()) => {
                self.notify(&format!(
                    "Moved {} to the trash",
                    path.rsplit('/').next().unwrap_or_default()
                ));
                self.refresh_files_listing();
                self.relist(None);
            }
            Err(e) => self.error(&format!("Delete failed: {e}")),
        }
    }
    /// The desktop demo's listing edits: remove `path`, or rename it.
    pub(super) fn demo_edit(&mut self, path: &str, rename: Option<&str>) -> Result<(), String> {
        let (folder, name) = parent(path).ok_or("Invalid file")?;
        let mut names = self.list_folder(&folder)?;
        let at = names
            .iter()
            .position(|n| n.trim_end_matches('/') == name)
            .ok_or("No such file")?;
        let dir = names[at].ends_with('/');
        match rename {
            Some(new) => {
                if names.iter().any(|n| n.trim_end_matches('/') == new) {
                    return Err("Name already exists".into());
                }
                names[at] = format!("{new}{}", if dir { "/" } else { "" });
                if dir {
                    if let Some(contents) = self.folders.remove(path) {
                        self.folders.insert(join(&folder, new), contents);
                    }
                }
            }
            None => {
                names.remove(at);
            }
        }
        if folder == self.directory {
            self.files = names;
        } else {
            self.folders.insert(folder, names);
        }
        Ok(())
    }

    // ----- the Save sheet -----

    pub(super) fn open_save_sheet(&mut self, kind: DataKind) {
        self.live = None;
        self.modal = None;
        self.panel_sidebar = false;
        let name = match kind {
            DataKind::Patch => self.patch().name.clone(),
            DataKind::Sample => {
                let sample = self.patch().sample.clone();
                sample
                    .rsplit_once('.')
                    .map_or(sample.as_str(), |(s, _)| s)
                    .to_string()
            }
            DataKind::Perf => self.performance.clone(),
        };
        let folder = match self.files_ui.places.get(&(DataKind::Sample, true)) {
            Some(Place::Folder(path)) => path.clone(),
            _ => self.location_root(0).unwrap_or_default(),
        };
        let slot = (!self.imported_performance()).then_some(self.performance_slot);
        let names = if let Some(d) = &self.device {
            self.files_ui
                .folders
                .get(&folder)
                .cloned()
                .unwrap_or_else(|| {
                    if d.directory == Path::new(&folder) {
                        d.files.clone()
                    } else {
                        vec![]
                    }
                })
        } else {
            self.list_folder(&folder).unwrap_or_default()
        };
        // A recording is saved as a new take rather than over its source.
        let name = if kind == DataKind::Sample && self.recorded_here() {
            if self.device.is_some() {
                Self::take_name_with(&name, |candidate| {
                    crate::device::storage::sample_file_name(candidate)
                        .is_ok_and(|file| Path::new(&folder).join(file).exists())
                })
            } else {
                Self::take_name(&name, &names)
            }
        } else {
            name
        };
        self.files_ui.layers.push(Layer::Save(SaveSheet {
            kind,
            name,
            folder,
            names,
            slot,
            focus: SaveField::Save,
        }));
        self.navigation_dirty = true;
    }
    fn open_destination(&mut self) {
        let Some(Layer::Save(s)) = self.files_ui.layers.last() else {
            return;
        };
        match s.kind {
            DataKind::Sample => {
                let folder = s.folder.clone();
                self.open_browser_for(DataKind::Sample, Purpose::Folder);
                if self.location_of(&folder).is_some() {
                    self.browse_to(Place::Folder(folder), None);
                }
            }
            DataKind::Perf => {
                let slot = s.slot.unwrap_or(self.performance_slot);
                self.open_browser_for(DataKind::Perf, Purpose::Slot);
                if let Some(Layer::Browser(b)) = self.files_ui.layers.last_mut() {
                    b.list.focus = slot;
                    b.list.reveal(b.rows.len());
                }
            }
            DataKind::Patch => {}
        }
    }
    fn pick_slot(&mut self, index: usize) {
        self.pop_layer();
        self.files_ui
            .memory
            .insert("slot-pick".into(), (format!("{index}"), index));
        if let Some(Layer::Save(s)) = self.files_ui.layers.last_mut() {
            s.slot = Some(index);
            s.focus = SaveField::Save;
        }
    }
    /// The panel's Save on the sheet, or its Save field.
    fn write_save(&mut self, confirmed: bool) {
        let Some(Layer::Save(s)) = self.files_ui.layers.last() else {
            return;
        };
        let (kind, name, folder, slot) =
            (s.kind, s.name.trim().to_string(), s.folder.clone(), s.slot);
        if name.is_empty() {
            self.error("Name is empty");
            return;
        }
        match kind {
            DataKind::Patch => {
                let changed = name != self.patch().name;
                let result = if changed {
                    match &mut self.device {
                        Some(d) => d.rename_patch(&name),
                        None => {
                            self.patch_mut().name = name.clone();
                            Ok(())
                        }
                    }
                } else {
                    Ok(())
                };
                if let Err(e) = result {
                    self.error(&format!("Save failed: {e}"));
                    return;
                }
                self.live = None;
                let saved = match &mut self.device {
                    Some(d) => d.save_patch(),
                    None => {
                        let i = self.parts[self.part];
                        self.patch_mut().dirty = false;
                        self.saved[i] = self.patch().clone();
                        Ok(())
                    }
                };
                self.sync_device_view();
                match saved {
                    Ok(()) => {
                        self.close_overlay();
                        self.notify(&format!("Saved {name}"));
                    }
                    Err(e) => self.error(&format!("Save failed: {e}")),
                }
            }
            DataKind::Sample => {
                let file = match crate::device::storage::sample_file_name(&name) {
                    Ok(file) => file,
                    Err(e) => {
                        self.error(&e);
                        return;
                    }
                };
                if let Some((2, _)) = self.location_of(&folder) {
                    self.error("Factory files are read-only. Choose a user or USB folder.");
                    return;
                }
                let exists = if self.device.is_some() {
                    Path::new(&folder).join(&file).exists()
                } else {
                    self.list_folder(&folder).is_ok_and(|n| n.contains(&file))
                };
                if exists && !confirmed {
                    self.files_ui.layers.push(Layer::Confirm(Confirm {
                        lines: vec![
                            format!("Replace {file}"),
                            format!("in {}", self.place_title(&Place::Folder(folder.clone()))),
                        ],
                        loss: "The existing file moves to the folder's trash.".into(),
                        verb: "Replace".into(),
                        keycap: "Save",
                        on_verb: false,
                        action: Verb::ReplaceSample,
                    }));
                    return;
                }
                let result = match &mut self.device {
                    Some(d) => d.save_sample_to(Path::new(&folder), &file, exists),
                    None => {
                        let mut names = self.list_folder(&folder).unwrap_or_default();
                        if !exists {
                            names.push(file.clone());
                        }
                        if folder == self.directory {
                            self.files = names;
                        } else {
                            self.folders.insert(folder.clone(), names);
                        }
                        self.patch_mut().sample = file.clone();
                        Ok(())
                    }
                };
                match result {
                    Ok(()) => {
                        let previous = match self.files_ui.layers.last() {
                            Some(Layer::Save(s)) => s.names.clone(),
                            _ => vec![],
                        };
                        let names = self
                            .files_ui
                            .folders
                            .entry(folder.clone())
                            .or_insert(previous);
                        if !names.contains(&file) {
                            names.push(file.clone());
                        }
                        self.files_ui
                            .places
                            .insert((DataKind::Sample, true), Place::Folder(folder));
                        self.sync_device_view();
                        self.refresh_files_listing();
                        self.close_overlay();
                        self.notify(&format!("Saved {file}"));
                    }
                    Err(e) => self.error(&format!("Save failed: {e}")),
                }
            }
            DataKind::Perf => {
                let Some(slot) = slot else {
                    // A factory or USB performance needs a user slot first.
                    if let Some(Layer::Save(s)) = self.files_ui.layers.last_mut() {
                        s.focus = SaveField::Into;
                    }
                    self.open_destination();
                    return;
                };
                let (current, empty) = (self.slot_name(slot).0, self.slot_name(slot).1);
                let other = slot != self.performance_slot || self.imported_performance();
                if other && !empty && !confirmed {
                    self.files_ui.layers.push(Layer::Confirm(Confirm {
                        lines: vec![
                            format!("Replace slot {:02}", slot + 1),
                            current,
                            "with the working performance".into(),
                        ],
                        loss: "Its saved presets are overwritten.".into(),
                        verb: "Replace".into(),
                        keycap: "Save",
                        on_verb: false,
                        action: Verb::SavePerf(slot),
                    }));
                    return;
                }
                self.queue_save_perf(slot);
            }
        }
    }
    fn queue_save_perf(&mut self, slot: usize) {
        let name = self.files_ui.layers.iter().rev().find_map(|l| match l {
            Layer::Save(s) => Some(s.name.trim().to_string()),
            _ => None,
        });
        self.queue(
            format!("Saving performance {:02}", slot + 1),
            Verb::SavePerf(slot),
            name,
        );
    }

    /// Menu operations that open the file screens, in both modes.
    pub(super) fn u2_operation(&mut self, op: &str) -> bool {
        match op {
            "save_sample" => self.open_save_sheet(DataKind::Sample),
            "load_patch" => self.open_browser(DataKind::Patch),
            _ => return false,
        }
        true
    }

    // ----- the name editor -----

    pub(super) fn open_name_editor(&mut self, purpose: NamePurpose) {
        let (title, name, limit) = match &purpose {
            NamePurpose::Sheet => match self.files_ui.layers.last() {
                Some(Layer::Save(s)) => (
                    format!("Save {}", s.kind.word()),
                    s.name.clone(),
                    match s.kind {
                        DataKind::Patch => PATCH_NAME_LIMIT,
                        DataKind::Perf => PERF_NAME_LIMIT,
                        DataKind::Sample => FILE_NAME_LIMIT,
                    },
                ),
                _ => return,
            },
            NamePurpose::Patch => (
                "Name patch".into(),
                self.patch().name.clone(),
                PATCH_NAME_LIMIT,
            ),
            NamePurpose::Performance => (
                "Name performance".into(),
                self.performance.clone(),
                PERF_NAME_LIMIT,
            ),
            NamePurpose::NewFolder => ("New folder".into(), String::new(), FILE_NAME_LIMIT),
            NamePurpose::Slot(slot) => (
                format!("Rename slot {:02}", slot + 1),
                self.slot_name(*slot).0,
                PERF_NAME_LIMIT,
            ),
            NamePurpose::Rename(path) => (
                "Rename".into(),
                path.rsplit('/').next().unwrap_or_default().to_string(),
                FILE_NAME_LIMIT,
            ),
        };
        self.live = None;
        let chars: Vec<char> = name.chars().collect();
        self.files_ui.layers.push(Layer::Name(NameEditor {
            title,
            cursor: chars.len(),
            chars,
            limit,
            purpose,
        }));
        self.navigation_dirty = true;
    }
    fn press_strip(&mut self) {
        match strip_item(self.files_ui.strip) {
            Ok("Done") => self.apply_name(false),
            Ok(control) => {
                if let Some(Layer::Name(n)) = self.files_ui.layers.last_mut() {
                    match control {
                        "Delete" => {
                            if n.cursor > 0 {
                                n.cursor -= 1;
                                n.chars.remove(n.cursor);
                            }
                        }
                        "<" => n.cursor = n.cursor.saturating_sub(1),
                        ">" => n.cursor = (n.cursor + 1).min(n.chars.len()),
                        "Space" => n.insert(' '),
                        "Clear" => {
                            n.chars.clear();
                            n.cursor = 0;
                        }
                        _ => {}
                    }
                }
            }
            Err(ch) => {
                if let Some(Layer::Name(n)) = self.files_ui.layers.last_mut() {
                    n.insert(ch);
                }
            }
        }
    }
    /// Typed text from a USB keyboard (SDL), at the cursor.
    pub(super) fn name_text(&mut self, text: &str) -> bool {
        if !self.files_ui.policy.keyboard_text {
            return false;
        }
        let Some(Layer::Name(n)) = self.files_ui.layers.last_mut() else {
            return false;
        };
        for ch in text.chars().filter(|c| !c.is_control()) {
            n.insert(ch);
        }
        self.navigation_dirty = true;
        true
    }
    /// Keyboard keys while a file screen is open. A computer keyboard edits the
    /// name directly (manual p.50); other keys stand in for the panel and go
    /// through the panel dispatcher. None: not a key for these screens.
    pub(super) fn overlay_key(&mut self, key: &str) -> Option<Vec<PanelEvent>> {
        if !self.overlay_active() {
            return None;
        }
        let name = matches!(self.files_ui.layers.last(), Some(Layer::Name(_)));
        let event = |action, value| PanelEvent {
            action,
            value,
            text: None,
        };
        if name && self.files_ui.policy.keyboard_text {
            let handled = match key {
                "Return" | "Keypad Enter" => {
                    self.apply_name(false);
                    true
                }
                "Backspace" => {
                    self.overlay_shift_escape();
                    true
                }
                "Left" | "Right" => {
                    if let Some(Layer::Name(n)) = self.files_ui.layers.last_mut() {
                        n.cursor = if key == "Left" {
                            n.cursor.saturating_sub(1)
                        } else {
                            (n.cursor + 1).min(n.chars.len())
                        };
                    }
                    true
                }
                _ => false,
            };
            if handled {
                self.navigation_dirty = true;
                return Some(vec![]);
            }
        }
        Some(vec![match key {
            "Up" | "Left" => event(5, 255),
            "Down" | "Right" => event(5, 1),
            "Return" | "Keypad Enter" => event(7, 0),
            "Escape" => event(14, 0),
            "F5" => event(1, 0),
            "F6" => event(2, 1),
            _ => return None,
        }])
    }
    /// Done, or the panel's Save (`write`): apply the name to its purpose.
    fn apply_name(&mut self, write: bool) {
        let Some(Layer::Name(n)) = self.files_ui.layers.last() else {
            return;
        };
        let name: String = n.chars.iter().collect::<String>().trim().to_string();
        let purpose = n.purpose.clone();
        if name.is_empty() {
            self.error("Name is empty");
            return;
        }
        let result: Result<String, String> = match &purpose {
            NamePurpose::Sheet => {
                self.files_ui.layers.pop();
                if let Some(Layer::Save(s)) = self.files_ui.layers.last_mut() {
                    s.name = name;
                    s.focus = SaveField::Save;
                }
                if write {
                    self.write_save(false);
                }
                return;
            }
            NamePurpose::Patch => match &mut self.device {
                Some(d) => d.rename_patch(&name).map(|_| "Working name changed".into()),
                None => {
                    self.patch_mut().name = name.clone();
                    Ok("Working name changed".into())
                }
            },
            NamePurpose::Performance => {
                match &mut self.device {
                    Some(d) => d.performance.document.put("name", name.as_str()),
                    None => self.performance = name.clone(),
                }
                Ok("Working name changed".into())
            }
            NamePurpose::Slot(slot) => self.perf_rename_slot(*slot, &name),
            NamePurpose::Rename(path) => {
                let renamed = crate::device::storage::file_name(&name)
                    .map(|_| ())
                    .and_then(|_| match &mut self.device {
                        Some(d) => d.rename_path(Path::new(path), &name),
                        None => self.demo_edit(path, Some(&name)),
                    });
                renamed.map(|_| format!("Renamed to {name}"))
            }
            NamePurpose::NewFolder => {
                let made = crate::device::storage::file_name(&name)
                    .map(|_| ())
                    .and_then(|_| match &mut self.device {
                        Some(d) => d.mkdir(&name),
                        None if self.files.iter().any(|n| n.trim_end_matches('/') == name) => {
                            Err("Name already exists".into())
                        }
                        None => {
                            self.files.push(format!("{name}/"));
                            Ok(())
                        }
                    });
                made.map(|_| format!("Created {name}"))
            }
        };
        match result {
            Ok(text) => {
                self.files_ui.layers.pop();
                self.sync_device_view();
                self.refresh_files_listing();
                if !self.overlay_active() {
                    // The Files page follows the renamed or new entry.
                    if let Some(i) = self
                        .files
                        .iter()
                        .position(|n| n.trim_end_matches('/') == name)
                    {
                        self.selected = i;
                    }
                }
                if matches!(purpose, NamePurpose::Rename(_) | NamePurpose::NewFolder) {
                    self.perf_files_focus(&name);
                }
                if matches!(purpose, NamePurpose::Rename(_)) {
                    self.relist(Some(name.clone()));
                    // A renamed folder keeps its slash in the row key.
                    if let Some(Layer::Browser(b)) = self.files_ui.layers.last_mut() {
                        if let Some(i) = b
                            .rows
                            .iter()
                            .position(|r| r.key.trim_end_matches('/') == name)
                        {
                            b.list.focus = i;
                            b.list.reveal(b.rows.len());
                        }
                    }
                }
                self.notify(&text);
            }
            Err(e) => self.error(&e),
        }
    }
    /// Keep the Files page listing current after a browser edit.
    pub(super) fn refresh_files_listing(&mut self) {
        if let Some(d) = &self.device {
            let path = self.directory.clone();
            let focus = self
                .perf
                .files
                .rows
                .get(self.perf.files.list.focus)
                .map(|r| r.key.clone());
            let job = d.start_listing(path.clone().into());
            self.begin_listing(
                job,
                super::listing::Target::Files {
                    path,
                    focus,
                    at_locations: self.perf.files.at_locations,
                },
            );
        }
    }
}

impl NameEditor {
    fn insert(&mut self, ch: char) {
        let bytes: usize = self.chars.iter().map(|c| c.len_utf8()).sum();
        if self.chars.len() >= self.limit || bytes + ch.len_utf8() > 255 {
            return;
        }
        self.chars.insert(self.cursor, ch);
        self.cursor += 1;
    }
}

impl SaveSheet {
    /// Focusable fields in turn order; a patch's destination is fixed.
    pub(super) fn fields(&self) -> Vec<SaveField> {
        let mut fields = vec![SaveField::Type, SaveField::Name];
        if self.kind != DataKind::Patch {
            fields.push(SaveField::Into);
        }
        fields.extend([SaveField::Save, SaveField::Cancel]);
        fields
    }
}

impl Browser {
    fn snapshot(&self) -> Browser {
        Browser {
            kind: self.kind,
            purpose: self.purpose,
            place: self.place.clone(),
            rows: self.rows.clone(),
            list: self.list,
            details: None,
            error: None,
            real_files: self.real_files,
        }
    }
}
