//! The Patch root of the redesign : nine pages
//! under one page strip, the field grammar and the live strip's switches.
//! The engine owns every value; a grabbed number is applied live through the
//! existing device operations, a choice only on Select. Focus memory per page
//! and per container, the grab's restore value and the strip position are
//! interface state.
use super::device_ui::encoder_delta;
use super::*;
use crate::device::native::PanelEvent;

pub(super) use catalog::PATCH_PAGES as PAGES;
pub(super) const PLAY: usize = 0;
pub(super) const TIMING: usize = 1;
pub(super) const SHAPE: usize = 2;
pub(super) const FX: usize = 3;
pub(super) const VOICES: usize = 4;
pub(super) const KEYS: usize = 5;
pub(super) const MATRIX: usize = 6;
pub(super) const LFO1: usize = 7;

/// One focusable thing on a Patch page, by identity .
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) enum Item {
    /// A catalog setting.
    Setting(usize),
    /// Grain Size on the Shape page, the panel knob's own value.
    GrainSize,
    /// The Voices page's scale keyboard, entered with a press.
    Scale,
    /// One key of the entered scale, C = 0.
    Key(usize),
    /// A matrix source row, entered with a press.
    Row(usize),
    /// A matrix cell: source, destination.
    Cell(usize, usize),
    /// The LFO page's AUX shape Record/Stop.
    Aux,
}

/// Patch-root behavior that depends on instrument checks not yet observed.
#[derive(Clone, Copy, Debug)]
pub(super) struct PatchPolicy {
    /// Unobserved: NOTEON (24) under id 4. While Keyboard tuning is grabbed,
    /// a played note sets it (stock's learn, only while grabbed). If the
    /// event does not arrive, tuning is set by turning, which always works.
    pub noteon_learns_tuning: bool,
}
impl Default for PatchPolicy {
    fn default() -> Self {
        Self {
            noteon_learns_tuning: true,
        }
    }
}

#[derive(Default)]
pub(super) struct PatchRoot {
    pub page: usize,
    /// Focus is on the page strip (after Escape from a page).
    pub on_strip: bool,
    /// Focus per page, by identity, with its position for the nearest
    /// neighbor when the item is gone (3.5.3).
    focus: [Option<(Item, usize)>; 9],
    /// The matrix row or the scale entered with a press.
    pub entered: Option<Item>,
    /// Focus inside each container: a row's cell, the scale's key.
    inner: HashMap<Item, Item>,
    /// A choice being turned: its setting and the option shown in amber.
    /// Nothing reaches the engine until Select.
    pub choice: Option<(usize, f32)>,
    pub policy: PatchPolicy,
    /// Catalog indices of the placed settings, in placement order.
    placed: Vec<usize>,
    /// The active and grabbed items as the current frame draws them.
    pub drawn: (Option<Item>, Option<Item>),
}

impl PatchRoot {
    /// Catalog indices of every placed setting, in placement order.
    pub(super) fn new(catalog: &[Setting]) -> Self {
        let placed = catalog::placement()
            .iter()
            .filter_map(|(id, ..)| catalog.iter().position(|s| s.id == *id))
            .collect();
        Self {
            placed,
            ..Default::default()
        }
    }
}

/// The effect type's parameters (FX page, effect column).
fn effect_params(fx_type: usize) -> &'static [&'static str] {
    match fx_type {
        0 => &[],
        1 => &["delay_time", "delay_feedback"],
        2 => &["distortion"],
        3 => &["bit_reduction", "rate_reduction"],
        _ => &["reverb_time", "reverb_amount"],
    }
}

impl App {
    /// The Patch root is on screen and receives the panel.
    pub(super) fn in_patch_root(&self) -> bool {
        self.tab == 1 && self.modal.is_none() && !self.overlay_active()
    }

    /// Settings of one group of a page, in placement order.
    pub(super) fn group_settings(&self, page: usize, group: &str) -> Vec<usize> {
        self.patch_ui
            .placed
            .iter()
            .copied()
            .filter(|i| {
                let s = &self.catalog[*i];
                s.page == PAGES[page] && s.group == group
            })
            .collect()
    }

    /// The effect column: Type, then the parameters of the chosen type.
    pub(super) fn effect_settings(&self) -> Vec<usize> {
        let mut v = vec![self.setting_index("fx_type")];
        v.extend(
            effect_params(self.get("fx_type") as usize)
                .iter()
                .map(|id| self.setting_index(id)),
        );
        v
    }

    /// Page items in focus order: down the left column, then the right
    /// .
    pub(super) fn page_items(&self, page: usize) -> Vec<Item> {
        let settings = |groups: &[&str]| -> Vec<Item> {
            groups
                .iter()
                .flat_map(|g| self.group_settings(page, g))
                .map(Item::Setting)
                .collect()
        };
        match page {
            PLAY => {
                // In Tape mode the Pitch row reads Tape slew (manual p.27).
                let hide = if self.get("sample_mode") == 2. {
                    "pitch"
                } else {
                    "tape_slew"
                };
                let hide = self.setting_index(hide);
                settings(&["Playback", "Grains"])
                    .into_iter()
                    .filter(|i| *i != Item::Setting(hide))
                    .collect()
            }
            TIMING => settings(&["Grains", "Scan"]),
            SHAPE => {
                let mut v = settings(&["Amplitude", "Grain window"]);
                v.push(Item::GrainSize);
                v
            }
            FX => {
                let mut v = settings(&["Knobs"]);
                v.extend(self.effect_settings().into_iter().map(Item::Setting));
                v.extend(settings(&["Filter", "Sub-oscillator"]));
                v
            }
            VOICES => {
                let mut v = settings(&["Play mode"]);
                v.push(Item::Scale);
                v.extend(settings(&["Voices"]));
                v
            }
            KEYS => settings(&["Keyboard", "Area 0", "Area 1", "Area 2"]),
            MATRIX => {
                let mut v: Vec<Item> = (0..catalog::SOURCES.len()).map(Item::Row).collect();
                v.extend(settings(&["Matrix"]));
                v
            }
            _ => {
                let mut v = settings(&[PAGES[page]]);
                v.push(Item::Aux);
                v
            }
        }
    }

    /// Items inside an entered container: a row's available cells, the keys.
    pub(super) fn inner_items(&self, container: Item) -> Vec<Item> {
        match container {
            Item::Row(r) => (0..catalog::DESTINATIONS.len())
                .filter(|c| catalog::route_allowed(r, *c))
                .map(|c| Item::Cell(r, c))
                .collect(),
            Item::Scale => (0..12).map(Item::Key).collect(),
            _ => vec![],
        }
    }

    /// The focused item of a page: the remembered one, or its nearest
    /// remaining neighbor.
    pub(super) fn page_focus(&self, page: usize) -> Item {
        let items = self.page_items(page);
        match self.patch_ui.focus[page] {
            Some((item, _)) if items.contains(&item) => item,
            Some((_, at)) => items[at.min(items.len() - 1)],
            None => items[0],
        }
    }

    /// The item the next press acts on: inside an entered container, else
    /// the page's focus.
    pub(super) fn patch_active(&self) -> Item {
        if let Some(container) = self.patch_ui.entered {
            let items = self.inner_items(container);
            let remembered = self.patch_ui.inner.get(&container).copied();
            return remembered.filter(|i| items.contains(i)).unwrap_or(items[0]);
        }
        self.page_focus(self.patch_ui.page)
    }

    /// What a grabbed number changes live.
    pub(super) fn item_target(&self, item: Item) -> Option<LiveTarget> {
        match item {
            Item::Setting(i) if matches!(self.catalog[i].kind, Kind::Number { .. }) => {
                Some(LiveTarget::Setting(i))
            }
            Item::GrainSize => Some(LiveTarget::Param(1, false)),
            Item::Cell(r, c) => Some(LiveTarget::Route(r, c)),
            _ => None,
        }
    }

    /// The grabbed item, if any: a choice being turned or a number changing.
    pub(super) fn patch_grabbed(&self) -> Option<Item> {
        if let Some((i, _)) = self.patch_ui.choice {
            return Some(Item::Setting(i));
        }
        let live = self.live?;
        let item = self.patch_active();
        (self.item_target(item) == Some(live.target)).then_some(item)
    }

    /// Interface context for the panel trace.
    pub(super) fn patch_context(&self) -> String {
        let at = if self.patch_ui.on_strip {
            "strip".to_string()
        } else {
            format!("{:?}", self.patch_active()).replace(' ', "")
        };
        let grab = if self.patch_grabbed().is_some() {
            ":grab"
        } else {
            ""
        };
        format!(
            "patch:{}:{at}{grab}",
            PAGES[self.patch_ui.page].replace(' ', "")
        )
    }

    fn set_page_focus(&mut self, item: Item) {
        let page = self.patch_ui.page;
        let at = self.page_items(page).iter().position(|i| *i == item);
        if let Some(at) = at {
            self.patch_ui.focus[page] = Some((item, at));
        }
    }

    /// Shift+Patch from anywhere outside the root: its last page and field.
    pub(super) fn open_patch_root(&mut self) {
        self.close_overlay();
        self.action(Action::Nav(1));
        self.patch_ui.on_strip = false;
        self.patch_ui.entered = None;
    }

    /// Open the root on one page, for the main screen's pointer shortcuts.
    pub(super) fn open_patch_page(&mut self, page: usize, item: Option<Item>) {
        self.open_patch_root();
        self.patch_ui.page = page;
        if let Some(item) = item {
            self.set_page_focus(item);
        }
    }

    /// Panel events while the root is on screen. True when consumed.
    pub(super) fn patch_event(&mut self, event: &PanelEvent) -> bool {
        match event.action {
            5 => self.patch_turn(encoder_delta(event.value)),
            7 => self.patch_press(),
            // Shift+Escape: restore any grab, then the main screen.
            14 if self.files_ui.policy.shifted_escape && self.shift => {
                self.patch_restore();
                self.action(Action::Nav(0));
            }
            14 => self.patch_escape(),
            // The same chord again: the next page.
            13 => {
                self.patch_keep();
                self.patch_ui.entered = None;
                self.patch_ui.page = (self.patch_ui.page + 1) % PAGES.len();
            }
            22 => self.patch_slider(event.value),
            // Shift+press may arrive as UNMOUNT ; a field has
            // no sheet, and the root never unmounts the stick.
            28 => {}
            24 => self.patch_note(event.value),
            _ => return false,
        }
        true
    }

    pub(super) fn patch_turn(&mut self, delta: i32) {
        if delta == 0 {
            return;
        }
        if self.patch_ui.on_strip {
            let last = PAGES.len() as i32 - 1;
            self.patch_ui.page = (self.patch_ui.page as i32 + delta).clamp(0, last) as usize;
            return;
        }
        if let Some((i, pending)) = self.patch_ui.choice {
            // The option cycles in amber; nothing reaches the engine.
            if let Kind::Choice(options) = &self.catalog[i].kind {
                let next = (pending as i32 + delta).rem_euclid(options.len() as i32);
                self.patch_ui.choice = Some((i, next as f32));
            }
            return;
        }
        if let Some(edit) = self.live {
            // The engine already scales a Shift-held detent by ten.
            let (_, _, step) = self.target_range(edit.target);
            let value = self.target_value(edit.target) + delta as f32 * step;
            self.write_live((value / step).round() * step);
            return;
        }
        let move_in = |items: &[Item], at: Item| {
            let i = items.iter().position(|x| *x == at).unwrap_or(0) as i32;
            items[(i + delta).clamp(0, items.len() as i32 - 1) as usize]
        };
        if let Some(container) = self.patch_ui.entered {
            let next = move_in(&self.inner_items(container), self.patch_active());
            self.patch_ui.inner.insert(container, next);
        } else {
            let page = self.patch_ui.page;
            let next = move_in(&self.page_items(page), self.page_focus(page));
            self.set_page_focus(next);
        }
    }

    pub(super) fn patch_press(&mut self) {
        if self.patch_ui.on_strip {
            self.patch_ui.on_strip = false;
            return;
        }
        if self.patch_ui.choice.is_some() || self.live.is_some() {
            self.patch_keep();
            return;
        }
        let item = self.patch_active();
        match item {
            Item::Setting(i) => {
                let value = self.value(&self.catalog[i]);
                match self.catalog[i].kind {
                    Kind::Toggle => self.set(i, if value > 0. { 0. } else { 1. }),
                    Kind::Choice(_) => self.patch_ui.choice = Some((i, value)),
                    Kind::Number { .. } => self.start_live(LiveTarget::Setting(i)),
                }
            }
            Item::Row(_) | Item::Scale => self.patch_ui.entered = Some(item),
            Item::Key(n) => self.toggle_scale_key(n),
            Item::Aux => self.operation("aux"),
            Item::GrainSize | Item::Cell(..) => {
                if let Some(target) = self.item_target(item) {
                    self.start_live(target);
                }
            }
        }
    }

    fn patch_escape(&mut self) {
        if self.patch_grabbed().is_some() || self.live.is_some() {
            self.patch_restore();
        } else if self.patch_ui.entered.is_some() {
            self.patch_ui.entered = None;
        } else if !self.patch_ui.on_strip {
            self.patch_ui.on_strip = true;
        } else {
            self.action(Action::Nav(0));
        }
    }

    /// Select keeps the grab: a choice applies now; a number already did.
    pub(super) fn patch_keep(&mut self) {
        if let Some((i, value)) = self.patch_ui.choice.take() {
            if self.value(&self.catalog[i]) != value {
                self.set(i, value);
            }
        }
        if self.live.is_some() {
            self.live = None;
        }
    }

    /// Escape restores the value from the moment of the grab.
    fn patch_restore(&mut self) {
        self.patch_ui.choice = None;
        if let Some(edit) = self.live {
            self.write_live(edit.original);
            if self.device.is_none() {
                self.patch_mut().dirty = edit.was_dirty;
            }
            self.live = None;
        }
    }

    /// The Position slider sets a grabbed value; otherwise it is the engine's.
    fn patch_slider(&mut self, value: u8) {
        let fraction = value as f32 / 255.;
        if let Some((i, _)) = self.patch_ui.choice {
            if let Kind::Choice(options) = &self.catalog[i].kind {
                let last = options.len().saturating_sub(1) as f32;
                self.patch_ui.choice = Some((i, (fraction * last).round()));
            }
        } else if let Some(edit) = self.live {
            let (lo, hi, step) = self.target_range(edit.target);
            self.write_live(((lo + (hi - lo) * fraction) / step).round() * step);
        }
    }

    /// Keyboard tuning learns the next played note, only while grabbed.
    fn patch_note(&mut self, note: u8) {
        let tuning = LiveTarget::Setting(self.setting_index("tuning"));
        if self.patch_ui.policy.noteon_learns_tuning && self.live.map(|l| l.target) == Some(tuning)
        {
            self.write_live(note as f32);
        }
    }

    /// The scale's notes as the patch holds them; unset notes are on.
    pub(super) fn scale_notes(&self) -> [bool; 12] {
        std::array::from_fn(|n| {
            self.patch()
                .values
                .get(&format!("scale{n}"))
                .copied()
                .unwrap_or(1.)
                > 0.
        })
    }

    /// A press on a scale key toggles it at once.
    fn toggle_scale_key(&mut self, key: usize) {
        let mut notes = self.scale_notes();
        notes[key] = !notes[key];
        if let Some(d) = &mut self.device {
            let result = d.scale(notes);
            self.device_result(result);
        } else {
            for (n, on) in notes.iter().enumerate() {
                self.patch_mut()
                    .values
                    .insert(format!("scale{n}"), f32::from(u8::from(*on)));
            }
            self.patch_mut().dirty = true;
        }
    }

    /// Desktop pointer: focus an item and press it.
    pub(super) fn click_patch_item(&mut self, item: Item) {
        self.patch_keep();
        self.patch_ui.on_strip = false;
        match item {
            Item::Cell(r, _) => {
                self.patch_ui.entered = Some(Item::Row(r));
                self.patch_ui.inner.insert(Item::Row(r), item);
            }
            Item::Key(_) => {
                self.patch_ui.entered = Some(Item::Scale);
                self.patch_ui.inner.insert(Item::Scale, item);
            }
            _ => {
                self.patch_ui.entered = None;
                self.set_page_focus(item);
            }
        }
        self.patch_press();
    }
}

/// How often the live strip redraws .
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StripRate {
    /// Every frame, 60 Hz.
    Every,
    /// Fallback: every other frame, 30 Hz.
    Half,
    /// Fallback: 60 Hz, but frozen while a value is grabbed.
    FreezeWhileGrabbed,
}

/// Live strip behavior that depends on the pre-Patch instrument checks . Each fallback is one switch. `GR1_UI_STRIP` sets them for
/// a trial without a rebuild: a comma list of `off`, `no-grains`, `30hz` and
/// `freeze-grab`; unset, the strip is live at 60 Hz with grains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StripPolicy {
    /// Off only to measure frames without the strip; the status bar then
    /// leaves its middle empty.
    pub shown: bool,
    /// Unobserved: whether grain, scan and spray readback continue under menu
    /// id 4. Fallback (false): the waveform, the loop and the last scan
    /// position, without grains or the spray spotlight.
    pub grains: bool,
    /// Unobserved: the strip's frame cost. Fallbacks: 30 Hz, or frozen while
    /// a value is grabbed.
    pub rate: StripRate,
}
impl Default for StripPolicy {
    fn default() -> Self {
        Self {
            shown: true,
            grains: true,
            rate: StripRate::Every,
        }
    }
}
impl StripPolicy {
    pub(super) fn from_env() -> Self {
        Self::parse(&std::env::var("GR1_UI_STRIP").unwrap_or_default())
    }
    pub(super) fn parse(text: &str) -> Self {
        let mut policy = Self::default();
        for word in text.split(',').map(str::trim) {
            match word {
                "off" => policy.shown = false,
                "no-grains" => policy.grains = false,
                "30hz" => policy.rate = StripRate::Half,
                "freeze-grab" => policy.rate = StripRate::FreezeWhileGrabbed,
                "" => {}
                other => eprintln!("GR1_UI_STRIP: unknown switch {other:?}"),
            }
        }
        policy
    }
    /// The mode as the readback trace names it.
    pub(super) fn mode(&self) -> &'static str {
        match (self.grains, self.rate) {
            (true, StripRate::Every) => "live-60hz",
            (true, StripRate::Half) => "live-30hz",
            (true, StripRate::FreezeWhileGrabbed) => "live-freeze-grab",
            (false, StripRate::Every) => "no-grains-60hz",
            (false, StripRate::Half) => "no-grains-30hz",
            (false, StripRate::FreezeWhileGrabbed) => "no-grains-freeze-grab",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_switches_parse_from_one_variable() {
        assert_eq!(StripPolicy::parse(""), StripPolicy::default());
        assert_eq!(StripPolicy::default().mode(), "live-60hz");
        let p = StripPolicy::parse("no-grains, 30hz");
        assert!(p.shown && !p.grains);
        assert_eq!(p.rate, StripRate::Half);
        assert_eq!(p.mode(), "no-grains-30hz");
        assert!(!StripPolicy::parse("off").shown);
        assert_eq!(
            StripPolicy::parse("freeze-grab").rate,
            StripRate::FreezeWhileGrabbed
        );
    }
}
