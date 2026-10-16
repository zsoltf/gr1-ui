//! The Sample root : Shift+Sample opens
//! the Record and Sample pages under screen id 3. Escape steps from a page to
//! its page strip and from the strip to the main screen; the same chord steps
//! to the next page. Focus belongs to a field's identity and each page keeps
//! its own. The engine owns recording and the sample: Record, Stop, resize,
//! clear and the settings go through the existing device operations, and the
//! Save sheet is file's. Only the grab's draft, the focus, the meter's peak and
//! clip latch, the sample copy arm and the input name are interface state.
use super::device_ui::encoder_delta;
use super::overlay::{Confirm, DataKind, Layer, Verb};
use super::overlay_draw::{sample_info, SampleInfo};
use super::*;
use crate::device::native::PanelEvent;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum Page {
    #[default]
    Record,
    Sample,
}
impl Page {
    pub(super) const ALL: [Page; 2] = [Page::Record, Page::Sample];
    pub(super) fn index(self) -> usize {
        self as usize
    }
    pub(super) fn name(self) -> &'static str {
        match self {
            Page::Record => "Record",
            Page::Sample => "Sample",
        }
    }
    pub(super) fn fields(self) -> &'static [Field] {
        match self {
            Page::Record => &RECORD_FIELDS,
            Page::Sample => &SAMPLE_FIELDS,
        }
    }
}

/// A focusable item of a page, by identity.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Field {
    /// Record, or Stop while recording.
    Record,
    Clear,
    Save,
    Copy,
    Setting(&'static str),
}

/// Record and Stop first (rule 3.5.4), then the actions, then the left
/// column and the right column (4.4: focus runs down the left, then the right).
pub(super) const RECORD_FIELDS: [Field; 9] = [
    Field::Record,
    Field::Clear,
    Field::Save,
    Field::Setting("rec_level"),
    Field::Setting("monitor"),
    Field::Setting("sample_size"),
    Field::Setting("start_mode"),
    Field::Setting("loop_rec"),
    Field::Setting("sampling"),
];
pub(super) const SAMPLE_FIELDS: [Field; 8] = [
    Field::Setting("pitch"),
    Field::Setting("tuning"),
    Field::Setting("normalize"),
    Field::Setting("convert"),
    Field::Setting("preview"),
    Field::Save,
    Field::Copy,
    Field::Clear,
];

/// The panel's words for the fields .
pub(super) fn label(field: Field) -> &'static str {
    match field {
        Field::Record => "Record",
        Field::Clear => "Clear sample",
        Field::Save => "Save sample",
        Field::Copy => "Copy to preset",
        Field::Setting(id) => match id {
            "rec_level" => "Record level",
            "monitor" => "Monitor level",
            "sample_size" => "Sample size",
            "start_mode" => "Start mode",
            "loop_rec" => "Looping",
            "sampling" => "Sampling",
            "pitch" => "Pitch",
            "tuning" => "Keyboard tuning",
            "normalize" => "Normalize on load",
            "convert" => "Convert to 44.1 kHz",
            "preview" => "Sound previews",
            other => other,
        },
    }
}

/// A grabbed value: the draft shown in amber and the value at the grab,
/// which Escape restores (3.2). Numbers other than Sample size are sent to
/// the engine as they turn; Sample size and choices wait for the press.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Grab {
    pub index: usize,
    pub original: f32,
    pub draft: f32,
}

#[derive(Default)]
pub(super) struct SampleUi {
    pub page: Page,
    /// Focus is on the page strip (after Escape from a page).
    pub strip: bool,
    /// The focused field per page, as an index into `Page::fields`.
    pub focus: [usize; 2],
    pub grab: Option<Grab>,
    /// The engine's CARD_NAME (event 23) text, when one has arrived.
    pub input: Option<String>,
    /// The meter's one-second peak (dB) and when it was set.
    pub peak: Option<(f32, Instant)>,
    /// The Clip mark latches until the next Record or a Record level grab.
    pub clip: bool,
    /// Presets recorded into this session, with the sample name they had.
    pub recorded: Vec<(usize, String)>,
    last_recording: Option<usize>,
    /// Desktop demo only: when the simulated recording started.
    pub demo_record_started: Option<Instant>,
    /// The page names' rectangles in the header, as last drawn (pointer).
    pub tabs: Vec<(Page, Rect)>,
    /// The focused sample's file header, keyed by slot, name and length.
    info: Option<((usize, String, i64), Option<SampleInfo>)>,
}

/// The meter's floor; levels below it draw nothing.
pub(super) const METER_FLOOR: f32 = -60.;

impl App {
    pub(super) fn sample_root_active(&self) -> bool {
        self.tab == 3 && self.modal.is_none()
    }
    pub(super) fn sample_field(&self) -> Field {
        let page = self.sample_ui.page;
        let fields = page.fields();
        fields[self.sample_ui.focus[page.index()].min(fields.len() - 1)]
    }
    /// The trace's view of the root: page, strip or field, grab.
    pub(super) fn sample_context(&self) -> String {
        let s = &self.sample_ui;
        let at = if s.strip {
            "strip".to_string()
        } else {
            format!("{:?}", self.sample_field()).replace([' ', '"'], "")
        };
        format!(
            "sample:{}:{at}{}",
            s.page.name(),
            if s.grab.is_some() { ":grab" } else { "" }
        )
    }

    /// Shift+Sample: open the root at its last page and field (3.5.4), or,
    /// when it was on screen (`step`), step to the next page.
    pub(super) fn open_sample_root(&mut self, step: bool) {
        if step && self.tab == 3 && !self.overlay_active() {
            self.keep_grab();
            let next = (self.sample_ui.page.index() + 1) % Page::ALL.len();
            self.sample_ui.page = Page::ALL[next];
        } else {
            self.close_overlay();
            self.sample_ui.grab = None;
            // The old menus' hit focus does not apply here.
            self.focus = None;
            self.focus_target = None;
            self.modal = None;
            self.live = None;
            self.panel_sidebar = false;
            self.list_action_origin = None;
            self.page_return = None;
            self.menu_root = 1;
            self.tab = 3;
            self.group = 0;
            self.page = 0;
        }
        self.sample_ui.strip = false;
        self.navigation_dirty = true;
        self.device_context();
    }
    fn leave_sample_root(&mut self) {
        self.sample_ui.grab = None;
        self.sample_ui.strip = false;
        self.tab = 0;
        self.group = 0;
        self.page = 0;
        self.device_context();
    }

    /// Before the file overlay sees a panel verb: a grabbed value is kept
    /// first (3.4). A preset press has already changed the engine's preset,
    /// so the grab is dropped without writing anything.
    pub(super) fn sample_pre_event(&mut self, event: &PanelEvent) {
        if self.tab != 3 || self.overlay_active() || self.sample_ui.grab.is_none() {
            return;
        }
        match event.action {
            1 | 2 | 3 | 4 | 12 | 13 | 29 => self.keep_grab(),
            6 | 10 => self.sample_ui.grab = None,
            _ => {}
        }
    }

    /// Panel events while the root is on screen. True when consumed.
    pub(super) fn sample_event(&mut self, event: &PanelEvent) -> bool {
        if !self.sample_root_active() || self.overlay_active() {
            return false;
        }
        if !matches!(event.action, 8..=11 | 16 | 17 | 20 | 21 | 23 | 25..=27 | 30 | 31) {
            // An error notice stays until the next input.
            if self.toast_error {
                self.toast.clear();
                self.toast_error = false;
            }
        }
        match event.action {
            5 => self.sample_turn(encoder_delta(event.value)),
            7 => self.sample_press(),
            14 if self.files_ui.policy.shifted_escape && self.shift => {
                self.cancel_grab();
                self.leave_sample_root();
            }
            14 => self.sample_escape(),
            15 => self.open_sample_root(true),
            // Copy arms the sample copy, as Copy to preset does.
            3 | 4 => self.arm_sample_copy(),
            22 => self.sample_slider(event.value),
            24 => self.sample_note(event.value),
            // Shift+press has no row sheet here; never unmount by accident.
            28 => {}
            29 => self.ask_clear(match event.value {
                0 => "clear_sample",
                2 => "init_perf",
                _ => "clear_patch",
            }),
            _ => return false,
        }
        self.navigation_dirty = true;
        true
    }

    fn sample_turn(&mut self, delta: i32) {
        if delta == 0 {
            return;
        }
        if let Some(grab) = self.sample_ui.grab {
            self.turn_grab(grab, delta);
            return;
        }
        let ui = &mut self.sample_ui;
        if ui.strip {
            // The page's content shows at once; nothing else changes.
            let last = Page::ALL.len() as i32 - 1;
            let at = (ui.page.index() as i32 + delta.signum()).clamp(0, last);
            ui.page = Page::ALL[at as usize];
            return;
        }
        let page = ui.page.index();
        let last = ui.page.fields().len() as i32 - 1;
        ui.focus[page] = (ui.focus[page] as i32 + delta).clamp(0, last) as usize;
    }
    fn sample_press(&mut self) {
        if self.sample_ui.grab.is_some() {
            self.keep_grab();
        } else if self.sample_ui.strip {
            self.sample_ui.strip = false;
        } else {
            let field = self.sample_field();
            self.sample_act(field);
        }
    }
    fn sample_escape(&mut self) {
        if self.sample_ui.grab.is_some() {
            self.cancel_grab();
        } else if self.sample_ui.strip {
            self.leave_sample_root();
        } else {
            self.sample_ui.strip = true;
        }
    }

    /// Press on a field: an action runs, a toggle flips, a value is grabbed.
    pub(super) fn sample_act(&mut self, field: Field) {
        match field {
            Field::Record => {
                // Stop stops whichever buffer records, even after a part change.
                let starting = self.recording.is_none();
                self.operation(if starting { "record" } else { "stop_record" });
                if starting {
                    self.sample_ui.clip = false;
                    self.sample_ui.peak = None;
                }
            }
            Field::Clear => self.ask_clear("clear_sample"),
            Field::Save => self.open_save_sheet(DataKind::Sample),
            Field::Copy => self.arm_sample_copy(),
            Field::Setting(id) => {
                let index = self.setting_index(id);
                let value = self.value(&self.catalog[index]);
                match self.catalog[index].kind {
                    Kind::Toggle => self.set(index, if value > 0. { 0. } else { 1. }),
                    Kind::Number { .. } if id == "sample_size" && self.recording.is_some() => {
                        self.error("Stop recording to resize the sample");
                    }
                    Kind::Number { .. } | Kind::Choice(_) => {
                        if id == "rec_level" {
                            self.sample_ui.clip = false;
                        }
                        self.sample_ui.grab = Some(Grab {
                            index,
                            original: value,
                            draft: value,
                        });
                    }
                }
            }
        }
    }
    fn turn_grab(&mut self, grab: Grab, delta: i32) {
        let draft = match &self.catalog[grab.index].kind {
            Kind::Number { min, max, step, .. } => {
                // The engine applies Shift (10 per detent); never multiply again.
                let value = grab.draft + delta as f32 * step;
                ((value / step).round() * step).clamp(*min, *max)
            }
            // Options cycle in place, one per detent; nothing is sent. With
            // Shift the engine reports ten per detent, which means one here.
            Kind::Choice(options) => {
                let step = if self.shift { delta.signum() } else { delta };
                (grab.draft as i32 + step).rem_euclid(options.len().max(1) as i32) as f32
            }
            Kind::Toggle => return,
        };
        self.set_draft(grab, draft);
    }
    fn set_draft(&mut self, grab: Grab, draft: f32) {
        self.sample_ui.grab = Some(Grab { draft, ..grab });
        if self.sends_live(grab.index) && draft != grab.draft {
            self.set(grab.index, draft);
        }
    }
    /// Numbers reach the engine as they turn, except the buffer size.
    fn sends_live(&self, index: usize) -> bool {
        matches!(self.catalog[index].kind, Kind::Number { .. })
            && self.catalog[index].id != "sample_size"
    }
    /// Select: keep the grabbed value. Sample size and choices apply now.
    pub(super) fn keep_grab(&mut self) {
        let Some(grab) = self.sample_ui.grab.take() else {
            return;
        };
        if !self.sends_live(grab.index) && grab.draft != grab.original {
            self.set(grab.index, grab.draft);
        }
    }
    /// Escape: restore the value from the moment of the grab and release.
    fn cancel_grab(&mut self) {
        let Some(grab) = self.sample_ui.grab.take() else {
            return;
        };
        if self.sends_live(grab.index) && grab.draft != grab.original {
            self.set(grab.index, grab.original);
        }
    }
    /// The Position slider sets a grabbed number when Big slider controls
    /// menus is On (manual p.67); otherwise it stays the engine's.
    fn sample_slider(&mut self, value: u8) {
        let Some(grab) = self.sample_ui.grab else {
            return;
        };
        if self.get("menu_slider") == 0. {
            return;
        }
        if let Kind::Number { min, max, step, .. } = self.catalog[grab.index].kind {
            let v = min + (max - min) * value as f32 / 255.;
            self.set_draft(grab, ((v / step).round() * step).clamp(min, max));
        }
    }
    /// Keyboard tuning learns the next played note, only while grabbed.
    fn sample_note(&mut self, note: u8) {
        if let Some(grab) = self.sample_ui.grab {
            if self.catalog[grab.index].id == "tuning" && note <= 127 {
                self.set_draft(grab, note as f32);
            }
        }
    }

    /// A clear asks first, opening on Cancel (3.6). `op` is the existing
    /// confirmation operation, run through the device when confirmed.
    fn ask_clear(&mut self, op: &'static str) {
        let slot = self.parts[self.part];
        let (lines, loss, verb) = match op {
            "clear_sample" => (
                vec![
                    "Clear sample".to_string(),
                    format!("{} in {:02}", self.patches[slot].sample, slot + 1),
                ],
                "The audio in this preset is erased; files on disk stay.",
                "Clear",
            ),
            "clear_patch" => (
                vec![
                    "Clear patch".to_string(),
                    format!("{:02} {}", slot + 1, self.patches[slot].name),
                ],
                "Its settings and sample are erased; files on disk stay.",
                "Clear",
            ),
            _ => (
                vec![
                    "Initialize performance".to_string(),
                    self.performance.clone(),
                ],
                "All 32 presets start empty; saved files stay.",
                "Initialize",
            ),
        };
        self.files_ui.layers.push(Layer::Confirm(Confirm {
            lines,
            loss: loss.into(),
            verb: verb.into(),
            keycap: "",
            on_verb: false,
            action: Verb::Run { op, slot },
        }));
    }

    /// Run a confirmed clear and name what it did.
    pub(super) fn run_clear(&mut self, op: &'static str, slot: usize) {
        let focused = self
            .device
            .as_ref()
            .map_or(self.parts[self.part], |d| d.slot());
        if matches!(op, "clear_sample" | "clear_patch") && slot != focused {
            self.error("Clear cancelled: the focused preset changed");
            return;
        }
        let sample = self.patches[slot].sample.clone();
        self.toast.clear();
        self.toast_error = false;
        self.confirm(op);
        if self.toast.is_empty() {
            self.notify(&match op {
                "clear_sample" => format!("Cleared {sample} in {:02}", slot + 1),
                "clear_patch" => format!("Cleared patch {:02}", slot + 1),
                _ => "Initialized the performance".into(),
            });
        }
    }

    // ----- copy to preset -----

    /// Arm stock's copy with the focused preset's sample: the next preset
    /// button and Copy paste it (manual p.49).
    pub(super) fn arm_sample_copy(&mut self) {
        let slot = self.parts[self.part];
        self.copy_arm = Some(super::perf_root::CopyArm {
            from: slot,
            kind: "copy_sample",
        });
        let sample = self.patches[slot].sample.clone();
        self.notify(&format!("Copied {sample} from {:02}", slot + 1));
    }
    /// Copy on the main screen while a sample copy is armed: paste it into
    /// the focused preset. Copy on the source preset disarms it.
    pub(super) fn paste_armed_sample(&mut self) {
        let Some(arm) = self.copy_arm.take() else {
            return;
        };
        let (source, dest) = (arm.from, self.parts[self.part]);
        if dest == source {
            self.notify("Copy cancelled");
            return;
        }
        let sample = self.patches[source].sample.clone();
        let result = match &mut self.device {
            Some(d) => d.copy_from(arm.kind, source, dest),
            None => {
                if self.recording == Some(dest) {
                    self.recording = None;
                }
                if arm.kind == "copy_sample" {
                    let size = self.patches[source].values.get("sample_size").copied();
                    let p = &mut self.patches[dest];
                    p.sample = sample.clone();
                    if let Some(size) = size {
                        p.values.insert("sample_size".into(), size);
                    }
                } else {
                    self.patches[dest] = self.patches[source].clone();
                }
                self.patches[dest].dirty = true;
                Ok(())
            }
        };
        match result {
            Ok(()) => {
                self.sync_device_view();
                self.notify(&if arm.kind == "copy_sample" {
                    format!("Pasted {sample} into {:02}", dest + 1)
                } else {
                    format!("Pasted the patch of {:02} into {:02}", source + 1, dest + 1)
                });
            }
            Err(e) => self.error(&format!("Paste failed: {e}")),
        }
    }

    // ----- readback, once per frame -----

    /// Follow recording, the input level and the clip flag. Runs every
    /// frame whichever screen is up, so the latch and the recorded-preset
    /// mark do not depend on the page being open.
    pub(super) fn observe_sample_state(&mut self) {
        let now = self.recording;
        match (self.sample_ui.last_recording, now) {
            (Some(slot), None) => {
                let sample = self.patches[slot].sample.clone();
                self.sample_ui.recorded.retain(|(s, _)| *s != slot);
                self.sample_ui.recorded.push((slot, sample));
                self.sample_ui.demo_record_started = None;
            }
            (None, Some(_)) => {
                self.sample_ui.clip = false;
                if self.device.is_none() {
                    self.sample_ui.demo_record_started = Some(Instant::now());
                }
            }
            _ => {}
        }
        self.sample_ui.last_recording = now;
        if let Some((db, clipped)) = self.input_level() {
            let peak = self.sample_ui.peak;
            if peak.is_none_or(|(p, at)| db >= p || at.elapsed() >= Duration::from_secs(1)) {
                self.sample_ui.peak = Some((db, Instant::now()));
            }
            if clipped {
                self.sample_ui.clip = true;
            }
        }
    }
    /// The input level in dB and the engine's clip flags. The device reads
    /// `gr_get_vu` and `vu_clipped`/`vu_soft_clipped`;
    /// the desktop demo has no input and simulates a level only while it
    /// records or monitors.
    pub(super) fn input_level(&self) -> Option<(f32, bool)> {
        match &self.device {
            Some(d) => Some((20. * d.vu.max(1e-5).log10(), d.clipped)),
            None => {
                if self.recording.is_none() && self.get("monitor") == 0. {
                    return None;
                }
                let t = self.demo_started.elapsed().as_secs_f32();
                Some((-15. + 4. * (t * 2.3).sin() + 2. * (t * 7.1).sin(), false))
            }
        }
    }
    /// The recording position as a fraction of the buffer: the engine's
    /// `gr_get_recpos` on the device, elapsed time in the desktop demo.
    pub(super) fn record_fraction(&self) -> Option<f32> {
        let slot = self.recording?;
        match &self.device {
            Some(d) => d.display.record_position,
            None => {
                let started = self.sample_ui.demo_record_started?;
                let length = self.sample_length(slot);
                (length > 0.).then(|| (started.elapsed().as_secs_f32() / length).rem_euclid(1.))
            }
        }
    }
    pub(super) fn sample_length(&self, slot: usize) -> f32 {
        self.patches[slot]
            .values
            .get("sample_size")
            .copied()
            .unwrap_or_else(|| self.get("sample_size"))
    }
    /// Rate and channels of the focused sample, from its file's header,
    /// shown only while the buffer still has the file's length (a recording
    /// or resize makes the file's header untrue for the buffer).
    pub(super) fn focused_sample_info(&mut self) -> Option<SampleInfo> {
        let slot = self.parts[self.part];
        let name = self.patches[slot].sample.clone();
        let length = self.sample_length(slot);
        let key = (slot, name.clone(), (length * 1000.).round() as i64);
        if self.sample_ui.info.as_ref().is_none_or(|(k, _)| *k != key) {
            let info = match &self.device {
                Some(d) => d
                    .sample_path(slot)
                    .and_then(|p| sample_info(true, &p.to_string_lossy(), &name)),
                None => sample_info(false, &name, &name),
            };
            self.sample_ui.info = Some((key, info));
        }
        let info = self.sample_ui.info.as_ref()?.1?;
        ((info.seconds() - length).abs() < 0.02 || self.device.is_none()).then_some(info)
    }

    // ----- the Save sheet's name for a recording -----

    /// The focused preset was recorded into since its sample was loaded.
    pub(super) fn recorded_here(&self) -> bool {
        let slot = self.parts[self.part];
        self.sample_ui
            .recorded
            .iter()
            .any(|(s, name)| *s == slot && *name == self.patches[slot].sample)
    }
    /// A recording keeps its sample's name with a take number, the first
    /// that is free in the destination folder, so saving it replaces nothing.
    pub(super) fn take_name(stem: &str, names: &[String]) -> String {
        Self::take_name_with(stem, |name| {
            names
                .iter()
                .any(|f| f.eq_ignore_ascii_case(&format!("{name}.wav")))
        })
    }
    pub(super) fn take_name_with(stem: &str, mut exists: impl FnMut(&str) -> bool) -> String {
        let base = match stem.rsplit_once('-') {
            Some((base, n)) if !base.is_empty() && n.parse::<u32>().is_ok() => base,
            _ => stem,
        };
        (2..)
            .map(|n| format!("{base}-{n}"))
            .find(|name| !exists(name))
            .unwrap_or_else(|| stem.to_string())
    }

    // ----- desktop preview -----

    /// Keyboard stand-ins for the panel while the root is open; they go
    /// through the panel dispatcher like the instrument's events.
    pub(super) fn sample_key(&self, key: &str) -> Option<Vec<PanelEvent>> {
        if !self.sample_root_active() || self.overlay_active() {
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
            "F2" => event(15, 0),
            "F5" => event(1, 0),
            "F6" => event(2, 1),
            _ => return None,
        }])
    }
    /// Pointer: a page name opens that page; a field is focused and pressed.
    pub(super) fn sample_pointer(&mut self, x: i32, y: i32) -> bool {
        if !self.sample_root_active() || self.overlay_active() {
            return false;
        }
        for (page, r) in self.sample_ui.tabs.clone() {
            if r.contains(x, y) {
                self.cancel_grab();
                self.sample_ui.page = page;
                self.sample_ui.strip = false;
                self.navigation_dirty = true;
                return true;
            }
        }
        let page = self.sample_ui.page;
        for (i, field) in page.fields().iter().enumerate() {
            if super::sample_draw::field_rect(page, *field).contains(x, y) {
                if self.sample_ui.grab.is_some() {
                    self.keep_grab();
                }
                self.sample_ui.strip = false;
                self.sample_ui.focus[page.index()] = i;
                self.sample_act(*field);
                self.navigation_dirty = true;
                return true;
            }
        }
        true
    }
}
