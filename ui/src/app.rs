mod components;
mod device_ui;
mod listing;
#[cfg(test)]
mod main_tests;
mod navigation;
mod overlay;
mod overlay_draw;
#[cfg(test)]
mod panel_tests;
mod patch_draw;
mod patch_root;
mod perf_draw;
mod perf_files;
mod perf_pages;
mod perf_root;
mod performance;
mod sample_draw;
mod sample_root;
#[cfg(test)]
mod sample_tests;
mod status;
mod timing;
#[cfg(test)]
mod u2_tests;
#[cfg(test)]
mod u3_tests;
#[cfg(test)]
mod u4_tests;
use crate::{
    appearance::{Appearance, Theme},
    catalog::{self, Kind, Setting},
    draw::*,
};
use sdl2::{
    event::{Event, WindowEvent},
    keyboard::{Keycode, Mod},
    mouse::{MouseButton, MouseWheelDirection},
    pixels::PixelFormatEnum,
};
use std::{
    collections::HashMap,
    path::Path,
    time::{Duration, Instant},
};

const TABS: [&str; 6] = ["Sound", "Patch", "Mod", "Sample", "Perform", "System"];
const PARAMS: [&str; 8] = [
    "Position",
    "Size",
    "Spray",
    "Density",
    "Scan",
    "Pan spray",
    "Loop start",
    "Loop end",
];
#[derive(Clone)]
struct Patch {
    name: String,
    sample: String,
    params: [f32; 8],
    values: HashMap<String, f32>,
    routes: [[f32; 11]; 9],
    /// The engine's current DEST selection, independent of route amounts.
    lfo_destinations: [Option<usize>; 2],
    dirty: bool,
}
impl Patch {
    fn new(i: usize) -> Self {
        let mut routes = [[0.; 11]; 9];
        routes[0][1] = 0.22;
        Self {
            name: match i {
                0 => "Frozen reeds".into(),
                1 => "Glass tide".into(),
                2 => "Low orbit".into(),
                3 => "Paper chorus".into(),
                _ => format!("Empty {:02}", i + 1),
            },
            sample: match i {
                0 => "REEDS_04.wav",
                1 => "GLASS_BOWL.wav",
                2 => "SLOW_WATER.wav",
                3 => "PAPER_FOLD.wav",
                _ => "Empty sample.wav",
            }
            .into(),
            params: match i {
                1 => [17., 420., 32., 12., 0.25, 60., 0., 100.],
                2 => [61., 900., 12., 8., -0.10, 20., 0., 100.],
                3 => [83., 70., 24., 48., 0.50, 80., 0., 100.],
                _ => [42., 180., 18., 24., 0., 30., 0., 100.],
            },
            values: HashMap::new(),
            routes,
            lfo_destinations: [Some(1), Some(7)],
            dirty: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
enum Action {
    Nav(usize),
    Group(usize),
    Section(usize),
    Edit(usize),
    Adjust(f32),
    Set(f32),
    Apply,
    Cancel,
    Param(usize),
    Part(usize),
    Shift,
    Play,
    Save,
    Name,
    Op(&'static str),
    Confirm,
    Select(usize),
    Preset(usize),
    PartEnable(usize),
    EditVolume(usize),
    Snapshot(usize),
    ChooseSlot(usize),
    SlotBank(usize),
    LiveSet(f32),
    Nudge(f32),
    UndoLive,
    FinishLive,
    Choose(f32),
    PartsOverview,
    Slider(usize),
    Theme(Theme),
    Brightness(u8),
    OpenMenu(usize, usize),
    /// Desktop pointer in the Patch root: a page of the strip.
    PatchPage(usize),
    /// Desktop pointer in the Patch root: focus an item and press it.
    PatchItem(patch_root::Item),
}
#[derive(Clone)]
struct Hit {
    r: Rect,
    a: Action,
}
#[derive(Clone)]
enum Modal {
    Edit {
        index: usize,
        value: f32,
    },
    Confirm(&'static str),
    Slots {
        operation: &'static str,
        bank: usize,
    },
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum LiveTarget {
    Param(usize, bool),
    Setting(usize),
    Route(usize, usize),
    PartVolume(usize),
}
#[derive(Clone, Copy)]
struct LiveEdit {
    target: LiveTarget,
    original: f32,
    was_dirty: bool,
    draft: Option<f32>,
}
#[derive(Clone, Copy)]
enum Drag {
    Position,
    Editor,
    Live,
    List {
        start_x: i32,
        start_y: i32,
        offset: i32,
        moved: bool,
    },
    Scrollbar {
        grab: i32,
    },
}
#[derive(Clone, Copy)]
struct ListView {
    bounds: Rect,
    content: i32,
    offset: i32,
}
impl ListView {
    fn max_offset(self) -> i32 {
        (self.content - self.bounds.3).max(0)
    }
    fn thumb(self) -> Rect {
        let height = (self.bounds.3 * self.bounds.3 / self.content.max(1))
            .max(30)
            .min(self.bounds.3);
        let y = self.bounds.1 + (self.bounds.3 - height) * self.offset / self.max_offset().max(1);
        Rect(771, y, 5, height)
    }
}
struct App {
    demo_started: Instant,
    /// Tests pin the desktop demo's clock so a simulated trace is the same
    /// every run.
    #[cfg(test)]
    demo_clock: Option<f32>,
    #[cfg(test)]
    render_passes: usize,
    panel_mode: bool,
    menu_root: usize,
    panel_sidebar: bool,
    list_action_origin: Option<Action>,
    page_return: Option<(usize, usize, usize, usize, Action)>,
    focus_target: Option<Action>,
    focus_memory: HashMap<String, Action>,
    rendered_context: String,
    navigation_dirty: bool,
    device_states: Vec<Option<crate::device::abi::Ipc>>,
    metadata_sync: Instant,
    native_poll_time: Duration,
    view_sync_time: Duration,
    input_received: Option<Instant>,
    appearance: Appearance,
    device: Option<crate::device::Device>,
    catalog: Vec<Setting>,
    patches: Vec<Patch>,
    saved: Vec<Patch>,
    system: HashMap<String, f32>,
    tab: usize,
    navigation: [(usize, usize); 6],
    live: Option<LiveEdit>,
    group: usize,
    page: usize,
    part: usize,
    parts: [usize; 4],
    enabled: [bool; 4],
    volumes: [f32; 4],
    snapshot: usize,
    snapshots: [([usize; 4], [bool; 4], [f32; 4]); 4],
    bank: usize,
    selected: usize,
    selected_performance: usize,
    source: usize,
    parts_overview: bool,
    shift: bool,
    latched: bool,
    playing: bool,
    recording: Option<usize>,
    aux_recording: bool,
    modal: Option<Modal>,
    hits: Vec<Hit>,
    focus: Option<usize>,
    drag: Option<Drag>,
    list: Option<ListView>,
    list_hits: Vec<(usize, Rect)>,
    scroll_offsets: HashMap<(usize, usize, usize), i32>,
    toast: String,
    toast_until: Instant,
    files: Vec<String>,
    clipboard: Vec<String>,
    directory: String,
    usb_mounted: bool,
    folders: HashMap<String, Vec<String>>,
    performance: String,
    selected_param: usize,
    performance_slot: usize,
    performance_library: Vec<(String, Vec<Patch>)>,
    performance_parts: Vec<([usize; 4], [bool; 4], [f32; 4])>,
    performance_configs: Vec<[([usize; 4], [bool; 4], [f32; 4]); 4]>,
    performance_snapshot: Vec<usize>,
    touch: status::Touch,
    /// GR1_UI_PANEL_TRACE=1: log every received panel event (device_ui.rs).
    panel_trace: bool,
    panel_batch: u64,
    panel_seq: u64,
    /// Streaming events in the current trace run, with their suppressed counts.
    trace_run: Vec<(u8, u64)>,
    /// The screen id last given to the engine (mirrors the device in device mode).
    screen_sent: Option<i32>,
    #[cfg(test)]
    panel_trace_lines: Vec<String>,
    /// The file screens: Load browser, Save sheet, name editor, sheets.
    files_ui: overlay::FilesUi,
    /// The notice is an error: it stays, in red, until the next input.
    toast_error: bool,
    /// The Sample Sample root: Record and Sample pages.
    sample_ui: sample_root::SampleUi,
    /// The Perf root (Perf): pages, focus, grabs, sheets and the Files page.
    perf: perf_root::PerfRoot,
    /// Stock's copy arm (the Copy chip): a preset's sample or patch waiting
    /// for its paste, armed from the Sample root or the Presets page.
    copy_arm: Option<perf_root::CopyArm>,
    /// The live strip's switches .
    strip_policy: patch_root::StripPolicy,
    /// The strip's last pixels, for the 30 Hz and freeze fallbacks.
    strip_cache: Option<((Theme, u8, usize, usize, bool), Vec<u8>, Instant)>,
    /// The strip's drawing time and mode in this frame, for the readback trace.
    strip_drawn: Option<(Duration, &'static str)>,
    /// The Patch root's pages, focus memory and grab .
    patch_ui: patch_root::PatchRoot,
}
impl App {
    fn new() -> Self {
        let patches = (0..32).map(Patch::new).collect::<Vec<_>>();
        let performance_library = [
            "Night studies",
            "Field notes",
            "Empty performance",
            "Factory collection",
        ]
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let mut items = patches.clone();
            if i == 1 {
                items[0].name = "Forest canopy".into();
                items[0].sample = "FOREST.wav".into();
            }
            if i == 2 {
                for (j, p) in items.iter_mut().enumerate() {
                    p.name = format!("Empty {:02}", j + 1);
                    p.sample = "Empty sample.wav".into();
                }
            }
            (name.to_string(), items)
        })
        .collect();
        let catalog = catalog::catalog();
        Self {
            patch_ui: patch_root::PatchRoot::new(&catalog),
            catalog,
            demo_started: Instant::now(),
            #[cfg(test)]
            demo_clock: None,
            #[cfg(test)]
            render_passes: 0,
            panel_mode: false,
            menu_root: 0,
            panel_sidebar: false,
            list_action_origin: None,
            page_return: None,
            focus_target: None,
            focus_memory: HashMap::new(),
            rendered_context: String::new(),
            navigation_dirty: true,
            device_states: vec![None; 32],
            metadata_sync: Instant::now() - Duration::from_secs(2),
            native_poll_time: Duration::ZERO,
            view_sync_time: Duration::ZERO,
            input_received: None,
            device: None,
            appearance: Appearance::default(),
            performance_slot: 0,
            performance_library,
            performance_parts: vec![([0, 1, 2, 3], [true, true, false, false], [80.; 4]); 4],
            performance_configs: vec![[([0, 1, 2, 3], [true, true, false, false], [80.; 4]); 4]; 4],
            performance_snapshot: vec![0; 4],
            touch: status::Touch::default(),
            saved: patches.clone(),
            patches,
            system: HashMap::new(),
            tab: 0,
            navigation: [(0, 0); 6],
            live: None,
            group: 0,
            page: 0,
            part: 0,
            parts: [0, 1, 2, 3],
            enabled: [true, false, false, false],
            volumes: [80.; 4],
            snapshot: 0,
            snapshots: [([0, 1, 2, 3], [true, true, false, false], [80.; 4]); 4],
            bank: 0,
            selected: 0,
            selected_performance: 0,
            source: 0,
            parts_overview: false,
            shift: false,
            latched: false,
            playing: false,
            recording: None,
            aux_recording: false,
            modal: None,
            hits: vec![],
            focus: None,
            drag: None,
            list: None,
            list_hits: vec![],
            scroll_offsets: HashMap::new(),
            toast: String::new(),
            toast_until: Instant::now(),
            files: vec![
                "REEDS_04.wav".into(),
                "GLASS_BOWL.wav".into(),
                "PAPER_FOLD.wav".into(),
                "NIGHT_TRAIN.wav".into(),
                "SLOW_WATER.wav".into(),
                "Field recordings/".into(),
            ],
            clipboard: vec![],
            directory: "/Internal".into(),
            usb_mounted: true,
            folders: HashMap::from([(
                "/Internal/Field recordings".into(),
                vec!["BIRDS_AT_DAWN.wav".into(), "RIVER.wav".into()],
            )]),
            performance: "Night studies".into(),
            selected_param: 1,
            panel_trace: std::env::var_os("GR1_UI_PANEL_TRACE").is_some(),
            panel_batch: 0,
            panel_seq: 0,
            trace_run: vec![],
            screen_sent: None,
            #[cfg(test)]
            panel_trace_lines: vec![],
            files_ui: overlay::FilesUi::default(),
            toast_error: false,
            sample_ui: sample_root::SampleUi::default(),
            perf: perf_root::PerfRoot::default(),
            copy_arm: None,
            strip_policy: patch_root::StripPolicy::from_env(),
            strip_cache: None,
            strip_drawn: None,
        }
    }
    fn patch(&self) -> &Patch {
        &self.patches[self.parts[self.part]]
    }
    fn patch_mut(&mut self) -> &mut Patch {
        let i = self.parts[self.part];
        &mut self.patches[i]
    }
    fn value(&self, s: &Setting) -> f32 {
        if s.system {
            self.system.get(&s.id)
        } else {
            self.patch().values.get(&s.id)
        }
        .copied()
        .unwrap_or_else(|| {
            if s.id.starts_with("voice") && s.id.ends_with("_pitch") {
                self.get("root")
            } else {
                s.default
            }
        })
    }
    fn get(&self, id: &str) -> f32 {
        self.catalog
            .iter()
            .find(|s| s.id == id)
            .map(|s| self.value(s))
            .unwrap_or(0.)
    }
    fn set(&mut self, index: usize, value: f32) {
        if self.device_set(index, value) {
            return;
        }
        let s = self.catalog[index].clone();
        if s.id == "sampling" && value == 0. && self.recording == Some(self.parts[self.part]) {
            self.recording = None;
        }
        if s.system {
            self.system.insert(s.id, value);
        } else {
            let p = self.patch_mut();
            p.values.insert(s.id, value);
            p.dirty = true;
        }
    }
    fn notify(&mut self, text: &str) {
        self.toast_error = false;
        self.toast = text.into();
        self.toast_until = Instant::now() + Duration::from_secs(4);
    }
    fn button(&mut self, p: &mut Painter, r: Rect, label: &str, a: Action, active: bool) {
        let navigation = matches!(a, Action::Nav(_) | Action::Group(_) | Action::Section(_));
        let primary = active && !navigation;
        let destructive = matches!(a, Action::Op("delete_file" | "clear_sample" | "factory"));
        if navigation {
            if active {
                p.rect(r, SELECTED);
                if matches!(a, Action::Group(_)) {
                    p.rect(Rect(r.0, r.1 + 10, 3, r.3 - 20), ACTIVE);
                } else {
                    p.rect(Rect(r.0 + 12, r.1 + r.3 - 3, r.2 - 24, 2), ACTIVE);
                }
            }
        } else {
            p.rect(r, if primary { PRIMARY } else { PANEL });
            p.border(r, if primary { PRIMARY } else { EDGE });
        }
        let padding = 12;
        p.fit(
            r.0 + padding,
            r.1 + (r.3 - 17) / 2 - 2,
            16,
            r.2 - padding * 2,
            label,
            if destructive {
                DANGER
            } else if primary {
                ON_PRIMARY
            } else if active {
                ACTIVE
            } else if navigation {
                MUTED
            } else {
                INK
            },
        );
        self.hits.push(Hit { r, a });
    }
    fn val_text(s: &Setting, value: f32) -> String {
        match &s.kind {
            Kind::Toggle => if value > 0. { "On" } else { "Off" }.into(),
            Kind::Choice(opts) => opts.get(value as usize).unwrap_or(&"-").to_string(),
            Kind::Number { unit: "MIDI", .. } => {
                format!("{} / {:.0}", Self::note(value as usize), value)
            }
            Kind::Number { step, unit, .. } => format!(
                "{} {}",
                if *step < 1. {
                    format!("{value:.2}")
                } else {
                    format!("{value:.0}")
                },
                unit
            ),
        }
    }
    fn param_value(&self, index: usize, value: f32, alternate: bool) -> String {
        match index {
            0 | 2 | 5 | 6 | 7 => format!("{value:.1}%"),
            1 => format!("{value:.0} ms"),
            3 => format!("{value:.1} Hz"),
            4 => {
                if alternate {
                    format!("{value:.1}% fwd")
                } else {
                    format!("{value:+.2}x")
                }
            }
            _ => String::new(),
        }
    }
    fn render(&mut self, p: &mut Painter) {
        self.render_pass(p);
        if self.navigation_dirty {
            // One layout correction can reveal a focused row after an editor resizes a list.
            self.render_pass(p);
        }
    }
    fn render_pass(&mut self, p: &mut Painter) {
        #[cfg(test)]
        {
            self.render_passes += 1;
        }
        p.theme = self.appearance.theme;
        p.brightness = self.appearance.brightness;
        p.clear();
        self.hits.clear();
        self.list = None;
        self.list_hits.clear();
        // Every frame, so the readout never reports a change older than its hold.
        self.track_touch();
        self.observe_sample_state();
        if self.overlay_active() {
            self.draw_overlay(p);
            return;
        }
        if self.sample_root_active() {
            self.draw_sample_root(p);
            self.rendered_context = self.focus_context();
            self.navigation_dirty = false;
            return;
        }
        if self.tab == 4 {
            self.draw_perf_root(p);
            return;
        }
        if self.tab == 0 && self.modal.is_none() {
            self.performance_screen_main(p);
            self.draw_perf_layers(p);
            self.finish_focus(p);
            return;
        }
        if self.tab == 1 && self.modal.is_none() {
            self.draw_patch_root(p);
            return;
        }
        p.text(20, 5, 17, "GR-1", BLUE);
        p.fit(74, 9, 10, 158, &self.performance.to_uppercase(), MUTED);
        p.fit(
            20,
            27,
            23,
            260,
            &format!("{:02}  {}", self.parts[self.part] + 1, self.patch().name),
            INK,
        );
        if !self.panel_mode {
            self.hits.push(Hit {
                r: Rect(16, 4, 296, 56),
                a: Action::Name,
            });
        }
        if self.patch().dirty {
            p.text(282, 35, 10, "EDITED", AMBER);
        }
        if !self.panel_mode {
            for i in 0..4 {
                let r = Rect(342 + i as i32 * 45, 9, 42, 46);
                p.rect(r, if self.part == i { SELECTED } else { PANEL });
                p.border(r, if self.part == i { BLUE } else { LINE });
                p.text(
                    r.0 + 13,
                    34,
                    11,
                    if self.enabled[i] { "On" } else { "Off" },
                    if self.enabled[i] { ACTIVE } else { MUTED },
                );
                p.text(
                    r.0 + 15,
                    10,
                    20,
                    &format!("{}", i + 1),
                    if self.part == i { ACTIVE } else { INK },
                );
                p.rect(
                    Rect(r.0 + 12, 51, 18, 2),
                    if self.enabled[i] { ACTIVE } else { LINE },
                );
                self.hits.push(Hit {
                    r,
                    a: Action::Part(i),
                });
            }
            self.button(p, Rect(528, 9, 68, 46), "Play", Action::Play, self.playing);
            self.button(p, Rect(604, 9, 60, 46), "Shift", Action::Shift, self.shift);
            self.button(p, Rect(672, 9, 112, 46), "Save patch", Action::Save, false);
        } else {
            p.text(
                414,
                11,
                13,
                &format!("Part {}  /  {}", self.part + 1, TABS[self.tab]),
                BLUE,
            );
            p.text(
                414,
                34,
                12,
                if self.enabled[self.part] {
                    "ENABLED"
                } else {
                    "DISABLED"
                },
                MUTED,
            );
        }
        if let Some(slot) = self.recording {
            self.button(
                p,
                Rect(244, 5, 88, 24),
                &format!("REC {:02}", slot + 1),
                Action::Op("stop_record"),
                true,
            );
        } else if let Some(device) = self.device.as_ref().filter(|_| self.get("dsp") != 0.) {
            p.text(282, 9, 10, &format!("DSP {:.0}%", device.dsp * 100.), MUTED);
        } else if !self.panel_mode {
            p.text(282, 9, 10, "Rename", MUTED);
        }
        p.line(16, 64, 784, 64, LINE);
        if self.modal.is_none() {
            self.home(p);
            p.rect(Rect(0, 430, 800, 50), NAV);
            p.line(0, 430, 799, 430, LINE);
            if !self.panel_mode {
                for (i, label) in TABS.iter().enumerate() {
                    self.button(
                        p,
                        Rect(i as i32 * 133, 430, if i == 5 { 135 } else { 132 }, 50),
                        label,
                        Action::Nav(i),
                        self.tab == i,
                    );
                }
            } else {
                p.text(
                    20,
                    448,
                    13,
                    &format!(
                        "Select  /  {}     Esc  /  {}",
                        if self.live.is_some() {
                            "Done"
                        } else if self.panel_sidebar {
                            "Open"
                        } else if self.list_action_origin.is_some() {
                            "Run"
                        } else if matches!(self.focus_target, Some(Action::Select(_))) {
                            "Actions"
                        } else {
                            "Edit"
                        },
                        if self.live.is_some() {
                            "Cancel edit"
                        } else if self.panel_sidebar {
                            "Sound"
                        } else if self.list_action_origin.is_some() {
                            "List"
                        } else {
                            "Menu"
                        }
                    ),
                    MUTED,
                );
            }
            if self.live.is_some() && self.modal.is_none() {
                self.live_strip(p);
            }
        }
        if self.modal.is_some() {
            self.draw_modal(p);
        }
        if !self.toast.is_empty() && Instant::now() < self.toast_until {
            if self.modal.is_some() {
                p.round_rect(Rect(16, 436, 768, 30), 4, NOTICE);
                p.fit(28, 443, 13, 740, &self.toast, AMBER);
            } else {
                p.rect(Rect(16, 3, 222, 21), BG);
                p.fit(20, 6, 12, 214, &self.toast, AMBER);
            }
        }
        self.finish_focus(p);
    }
    fn home(&mut self, p: &mut Painter) {
        self.performance_screen_main(p);
    }
    fn sample_amplitude(sample: &str, position: f32) -> f32 {
        if sample == "Empty sample.wav" {
            return 0.;
        }
        let seed = sample.bytes().fold(0u32, |seed, byte| {
            seed.wrapping_mul(31).wrapping_add(byte as u32)
        });
        let phase = (seed % 1024) as f32 / 100.;
        let z = position * 255.;
        ((z * 0.61 + phase).sin().abs() * 0.52 + (z * 0.137 + phase).sin().abs() * 0.35 + 0.1)
            * ((z * 0.027 + phase).sin().abs() * 0.7 + 0.23)
    }
    fn slider_role(&self) -> usize {
        if self.shift && self.playing {
            3
        } else {
            self.get("adsr_mode") as usize
        }
    }
    fn slider_target(&self, slider: usize) -> LiveTarget {
        match self.slider_role() {
            0 => LiveTarget::Setting(
                self.setting_index(["attack", "decay", "sustain", "release"][slider]),
            ),
            1 => LiveTarget::Setting(self.setting_index(&format!("voice{}_volume", slider + 1))),
            2 => LiveTarget::PartVolume(slider),
            _ => LiveTarget::Setting(self.setting_index(&format!("voice{}_pitch", slider + 1))),
        }
    }
    fn setting_index(&self, id: &str) -> usize {
        self.catalog
            .iter()
            .position(|s| s.id == id)
            .expect("known UI setting")
    }
    fn main_target(&self, index: usize) -> LiveTarget {
        let prefix = if index == 3 { "grain" } else { "scan" };
        if (index == 3 || index == 4) && self.get(&format!("{prefix}_clock")) > 0. {
            LiveTarget::Setting(self.setting_index(&format!(
                "{prefix}_{}",
                if self.shift { "div" } else { "mul" }
            )))
        } else {
            LiveTarget::Param(index, self.shift)
        }
    }
    fn target_value(&self, target: LiveTarget) -> f32 {
        if let Some(value) = self
            .live
            .filter(|edit| edit.target == target)
            .and_then(|edit| edit.draft)
        {
            return value;
        }
        match target {
            LiveTarget::Param(i, alternate) => {
                if alternate && i == 4 {
                    self.patch()
                        .values
                        .get("direction")
                        .copied()
                        .unwrap_or(100.)
                } else if alternate && i == 5 {
                    self.patch().values.get("pan").copied().unwrap_or(50.)
                } else {
                    self.patch().params[i]
                }
            }
            LiveTarget::Setting(i) => self.value(&self.catalog[i]),
            LiveTarget::Route(source, dest) => self.patch().routes[source][dest],
            LiveTarget::PartVolume(part) => self.volumes[part],
        }
    }
    fn target_range(&self, target: LiveTarget) -> (f32, f32, f32) {
        match target {
            LiveTarget::Param(i, alt) => {
                let range = self.param_range(i, alt);
                if i == 6 {
                    (0., self.patch().params[7], range.2)
                } else if i == 7 {
                    (self.patch().params[6], 100., range.2)
                } else {
                    range
                }
            }
            LiveTarget::Setting(i) => match self.catalog[i].kind {
                Kind::Number { min, max, step, .. } => (min, max, step),
                _ => (0., 1., 1.),
            },
            LiveTarget::Route(..) => (-1., 1., 0.01),
            LiveTarget::PartVolume(_) => (0., 100., 1.),
        }
    }
    fn start_live(&mut self, target: LiveTarget) {
        if self.live.map(|l| l.target) == Some(target) {
            return;
        }
        self.live = Some(LiveEdit {
            target,
            original: self.target_value(target),
            was_dirty: self.patch().dirty,
            draft: None,
        });
        self.modal = None;
    }
    fn write_live(&mut self, value: f32) {
        let Some(edit) = self.live else { return };
        let (min, max, _) = self.target_range(edit.target);
        // Cancellation must preserve valid legacy values below the UI minimum.
        let value = if value == edit.original {
            value
        } else {
            value.clamp(min, max)
        };
        if matches!(edit.target, LiveTarget::Setting(i) if self.catalog[i].id == "sample_size") {
            self.live.as_mut().unwrap().draft = Some(value);
            return;
        }
        if self.device_live(edit.target, value) {
            return;
        }
        match edit.target {
            LiveTarget::Param(i, alt) => {
                if alt && i == 4 {
                    self.patch_mut().values.insert("direction".into(), value);
                } else if alt && i == 5 {
                    self.patch_mut().values.insert("pan".into(), value);
                } else {
                    self.patch_mut().params[i] = value;
                }
                self.patch_mut().dirty = true;
            }
            LiveTarget::Setting(i) => {
                // Keep keyboard zones ordered while adjusting either boundary.
                let s = &self.catalog[i];
                let value = if s.id == "split1" {
                    value.min(self.get("split2"))
                } else if s.id == "split2" {
                    value.max(self.get("split1"))
                } else {
                    value
                };
                self.set(i, value);
            }
            LiveTarget::PartVolume(part) => self.volumes[part] = value,
            LiveTarget::Route(source, dest) => {
                self.patch_mut().routes[source][dest] = value;
                self.patch_mut().dirty = true;
            }
        }
    }
    fn live_slider(&self) -> Rect {
        if self.tab == 0 {
            Rect(598, 188, 172, 56)
        } else {
            Rect(202, 373, 260, 36)
        }
    }
    fn live_strip(&mut self, p: &mut Painter) {
        let Some(edit) = self.live else { return };
        let value = self.target_value(edit.target);
        let (name, display) = match edit.target {
            LiveTarget::Param(i, alt) => (
                (if alt && i == 4 {
                    "Direction"
                } else if alt && i == 5 {
                    "Pan offset"
                } else {
                    PARAMS[i]
                })
                .into(),
                self.param_value(i, value, alt),
            ),
            LiveTarget::Setting(i) => (
                self.catalog[i].name.clone(),
                Self::val_text(&self.catalog[i], value),
            ),
            LiveTarget::PartVolume(part) => {
                (format!("Part {} volume", part + 1), format!("{value:.0}%"))
            }
            LiveTarget::Route(source, dest) => (
                format!(
                    "{} > {}",
                    catalog::SOURCES[source],
                    catalog::DESTINATIONS[dest]
                ),
                format!("{:+.0}%", value * 100.),
            ),
        };
        if self.tab == 0 {
            p.fit(594, 88, 18, 182, &name, AMBER);
            p.fit(594, 127, 32, 182, &display, INK);
            let (min, max, _) = self.target_range(edit.target);
            let fraction = if max <= min {
                0.
            } else if min > 0. && max / min >= 1000. {
                (value / min).ln() / (max / min).ln()
            } else {
                (value - min) / (max - min)
            };
            let slider = self.live_slider();
            p.rect(Rect(slider.0, slider.1 + 27, slider.2, 3), LINE);
            p.rect(
                Rect(
                    slider.0,
                    slider.1 + 27,
                    (fraction * slider.2 as f32) as i32,
                    3,
                ),
                AMBER,
            );
            p.round_rect(
                Rect(
                    slider.0 - 7 + (fraction * slider.2 as f32) as i32,
                    slider.1 + 10,
                    14,
                    38,
                ),
                3,
                AMBER,
            );
            self.button(p, Rect(584, 262, 96, 56), "-", Action::Nudge(-1.), false);
            self.button(p, Rect(688, 262, 96, 56), "+", Action::Nudge(1.), false);
            self.button(p, Rect(584, 366, 96, 56), "Undo", Action::UndoLive, false);
            self.button(p, Rect(688, 366, 96, 56), "Done", Action::FinishLive, true);
            return;
        }
        let (x, y, w) = (188, 350, 592);
        if self.panel_mode {
            p.rect(Rect(x, y, w, 62), PANEL);
            p.fit(x + 12, y + 9, 13, 400, &name, MUTED);
            p.fit(x + 12, y + 30, 22, 400, &display, AMBER);
            p.fit(x + w - 170, y + 23, 12, 158, if matches!(edit.target, LiveTarget::Setting(i) if self.catalog[i].id == "sample_size") { "Select / Resize buffer" } else { "Turn to adjust" }, MUTED);
            return;
        }
        p.round_rect(Rect(x, y, w, 62), 5, PANEL);
        p.fit(
            x + 12,
            y + 5,
            12,
            w - 325,
            &format!("{name}   {display}"),
            AMBER,
        );
        p.text(x + w - 270, y + 7, 10, "LIVE", MUTED);
        let (min, max, _) = self.target_range(edit.target);
        let fraction = if max <= min {
            0.
        } else if min > 0. && max / min >= 1000. {
            (value / min).ln() / (max / min).ln()
        } else {
            (value - min) / (max - min)
        };
        let sw = w - 332;
        p.rect(Rect(x + 14, y + 39, sw, 3), LINE);
        p.rect(
            Rect(x + 14, y + 39, (fraction * sw as f32) as i32, 3),
            AMBER,
        );
        p.round_rect(
            Rect(x + 8 + (fraction * sw as f32) as i32, y + 27, 12, 27),
            3,
            AMBER,
        );
        self.hits.push(Hit {
            r: Rect(x + 8, y + 23, sw + 14, 36),
            a: Action::LiveSet(value),
        });
        self.button(
            p,
            Rect(x + w - 298, y + 17, 48, 42),
            "-",
            Action::Nudge(-1.),
            false,
        );
        self.button(
            p,
            Rect(x + w - 242, y + 17, 48, 42),
            "+",
            Action::Nudge(1.),
            false,
        );
        self.button(
            p,
            Rect(x + w - 184, y + 17, 82, 42),
            "Undo",
            Action::UndoLive,
            false,
        );
        self.button(
            p,
            Rect(x + w - 94, y + 17, 82, 42),
            "Done",
            Action::FinishLive,
            true,
        );
    }
    fn note(value: usize) -> String {
        format!(
            "{}{}",
            ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"][value % 12],
            value as i32 / 12 - 1
        )
    }
    fn envelope_values(&self, grain: bool) -> Vec<f32> {
        (0..=96)
            .map(|i| {
                let t = i as f32 / 96.;
                if grain {
                    let peak = (self.get("tilt") / 100.).clamp(0.02, 0.98);
                    let sides = (self.get("sides") / 100.).max(0.02);
                    let attack = (peak * sides).max(0.01);
                    let release = ((1. - peak) * sides).max(0.01);
                    let ramp = if t < attack {
                        t / attack
                    } else if t > 1. - release {
                        (1. - t) / release
                    } else {
                        1.
                    };
                    ramp.max(0.).powf(0.25 + self.get("curve") / 100. * 3.)
                } else {
                    let a = self.get("attack").max(0.02);
                    let d = self.get("decay").max(0.02);
                    let r = self.get("release").max(0.02);
                    let sustain = self.get("sustain") / 100.;
                    let time = t * (a + d + r + 2.);
                    if time < a {
                        time / a
                    } else if time < a + d {
                        1. - (1. - sustain) * (time - a) / d
                    } else if time < a + d + 2. {
                        sustain
                    } else {
                        sustain * (1. - (time - a - d - 2.) / r)
                    }
                }
            })
            .collect()
    }
    fn fx_controls(&self) -> &'static [&'static str] {
        match self.get("fx_map") as usize {
            0 => &["cutoff", "resonance"],
            1 => &["sub_freq", "sub_amp"],
            2 => &["delay_time", "delay_feedback"],
            3 => &["distortion"],
            4 => &["bit_reduction", "rate_reduction"],
            _ => &["reverb_time", "reverb_amount"],
        }
    }
    fn scroll_key(&self) -> (usize, usize, usize) {
        (self.tab, self.group, 0)
    }
    fn scroll_to(&mut self, offset: i32) {
        if self.modal.is_some() {
            return;
        }
        if let Some(list) = &mut self.list {
            list.offset = offset.clamp(0, list.max_offset());
            let offset = list.offset;
            self.scroll_offsets.insert(self.scroll_key(), offset);
            self.focus = None;
            self.focus_target = None;
        }
    }
    fn scroll_by(&mut self, delta: i32) {
        if let Some(list) = self.list {
            self.scroll_to(list.offset + delta);
        }
    }
    // Encoder/Tab navigates controls, including rows outside the viewport.
    // Pointer/wheel scrolling still never changes a selection or setting.
    fn move_focus(&mut self, delta: i32) {
        self.navigate_focus(delta);
    }
    fn reset_file_scroll(&mut self) {
        self.scroll_offsets.insert((5, 3, 0), 0);
    }
    fn draw_modal(&mut self, p: &mut Painter) {
        let modal = self.modal.clone().unwrap();
        self.hits.clear();
        p.rect(Rect(0, 64, 800, 416), BG);
        p.rect(Rect(336, 0, 464, 64), BG);
        p.line(16, 64, 784, 64, LINE);
        match modal {
            Modal::Edit { index, value } => {
                let s = self.catalog[index].clone();
                p.text(24, 81, 25, &s.name, INK);
                match &s.kind {
                    Kind::Toggle => {
                        for (i, label) in ["Off", "On"].iter().enumerate() {
                            self.button(
                                p,
                                Rect(24 + i as i32 * 230, 202, 210, 70),
                                label,
                                Action::Set(i as f32),
                                value == i as f32,
                            );
                        }
                    }
                    Kind::Choice(opts) => {
                        for (i, label) in opts.iter().enumerate() {
                            self.button(
                                p,
                                Rect(
                                    24 + (i % 3) as i32 * 253,
                                    183 + (i / 3) as i32 * 71,
                                    239,
                                    60,
                                ),
                                label,
                                Action::Choose(i as f32),
                                value == i as f32,
                            );
                        }
                    }
                    Kind::Number { min, max, step, .. } => {
                        p.text(26, 178, 38, &Self::val_text(&s, value), ACTIVE);
                        self.slider(p, value, *min, *max);
                        self.button(p, Rect(24, 295, 98, 52), "-", Action::Adjust(-*step), false);
                        self.button(p, Rect(132, 295, 98, 52), "+", Action::Adjust(*step), false);
                    }
                }
                if matches!(s.kind, Kind::Choice(_)) {
                    self.button(p, Rect(24, 365, 150, 52), "Back", Action::Cancel, false);
                } else {
                    self.modal_footer(p);
                }
            }
            Modal::Slots { operation, bank } => {
                p.text(
                    24,
                    80,
                    23,
                    if operation == "assign_part" {
                        "Choose a preset for this part"
                    } else if operation == "load_patch" {
                        "Load saved patch into current slot"
                    } else if operation == "copy_sample" {
                        "Copy sample to preset"
                    } else {
                        "Copy current patch to slot"
                    },
                    INK,
                );
                for i in 0..4 {
                    self.button(
                        p,
                        Rect(24 + i as i32 * 189, 121, 176, 44),
                        &format!("Bank {}", i + 1),
                        Action::SlotBank(i),
                        i == bank,
                    );
                }
                for i in 0..8 {
                    let n = bank * 8 + i;
                    self.button(
                        p,
                        Rect(
                            24 + (i % 2) as i32 * 378,
                            179 + (i / 2) as i32 * 45,
                            366,
                            42,
                        ),
                        &format!("{:02}  {}", n + 1, self.patches[n].name),
                        Action::ChooseSlot(n),
                        false,
                    );
                }
                self.button(p, Rect(24, 369, 150, 48), "Cancel", Action::Cancel, false);
            }
            Modal::Confirm(op) => {
                let (title, body) = operation_text(op, self.device.is_some());
                p.text(24, 87, 27, title, INK);
                p.wrap(24, 140, 78, body, MUTED);
                self.button(p, Rect(24, 353, 160, 56), "Cancel", Action::Cancel, false);
                self.button(
                    p,
                    Rect(584, 353, 192, 56),
                    match op {
                        "clear_sample" | "clear_patch" => "Clear",
                        "revert_patch" => "Revert",
                        "unmount" => "Unmount",
                        "shutdown" => "Shut down",
                        "factory" => "Reset",
                        "init_perf" => "Initialize",
                        _ => "Apply",
                    },
                    Action::Confirm,
                    true,
                );
            }
        }
    }
    fn modal_footer(&mut self, p: &mut Painter) {
        self.button(p, Rect(24, 365, 150, 52), "Cancel", Action::Cancel, false);
        self.button(p, Rect(626, 365, 150, 52), "Apply", Action::Apply, true);
    }
    fn slider(&mut self, p: &mut Painter, value: f32, min: f32, max: f32) {
        let r = Rect(24, 233, 752, 48);
        p.rect(Rect(34, 253, 732, 5), LINE);
        let x = 34 + ((value - min) / (max - min) * 732.).clamp(0., 732.) as i32;
        p.rect(Rect(34, 253, x - 34, 5), ACTIVE);
        p.rect(Rect(x - 8, 237, 16, 37), AMBER);
        p.text(24, 279, 10, &format!("{min}"), MUTED);
        p.text(731, 279, 10, &format!("{max}"), MUTED);
        self.hits.push(Hit {
            r,
            a: Action::Set(value),
        });
    }
    fn param_range(&self, index: usize, alternate: bool) -> (f32, f32, f32) {
        match index {
            1 => (5., 5000., 1.),
            3 => (0.1, 1000., 0.1),
            4 if !alternate => (-2., 2., 0.01),
            _ => (0., 100., 0.1),
        }
    }
    fn back(&mut self) {
        self.navigation_dirty = true;
        let before = self.remember_focus();
        if self.modal.is_some() {
            self.modal = None;
        } else if self.live.is_some() {
            self.action(Action::UndoLive);
        } else if self.list_action_origin.take().is_some() {
            // Leaving item actions restores the row remembered before entry.
        } else if let Some((tab, group, page, source, action)) = self.page_return.take() {
            self.tab = tab;
            self.group = group;
            self.page = page;
            self.source = source;
            self.focus_memory.insert(self.focus_context(), action);
        } else if self.tab != 0 && self.panel_mode {
            if self.panel_sidebar {
                self.tab = 0;
                self.group = 0;
                self.page = 0;
                self.panel_sidebar = false;
            } else {
                self.panel_sidebar = true;
                self.focus_memory.insert(
                    format!("sidebar:{}", self.menu_root),
                    Action::OpenMenu(self.tab, self.group),
                );
            }
        } else if self.tab != 0 {
            self.navigation[self.tab] = (self.group, self.page);
            self.tab = 0;
            self.group = 0;
            self.page = 0;
        }
        self.restore_focus(&before);
        self.device_context();
        self.drag = None;
    }
    fn action(&mut self, a: Action) {
        self.navigation_dirty = true;
        let before = self.remember_focus();
        let location = (self.tab, self.group, self.page, self.directory.clone());
        let files = self.files.clone();
        if self.device_action(&a) {
            self.finish_action_context(&before, location, files);
            return;
        }
        match a {
            Action::PatchPage(page) => {
                self.patch_keep();
                self.patch_ui.entered = None;
                self.patch_ui.on_strip = false;
                self.patch_ui.page = page;
            }
            Action::PatchItem(item) => self.click_patch_item(item),
            // The Perf root's page strip, clicked on the desktop.
            Action::OpenMenu(4, page) => {
                self.open_perf_root(false);
                self.perf.page = page;
                self.perf.strip = false;
                self.perf_enter_page();
            }
            Action::OpenMenu(tab, group) => {
                self.page_return = None;
                self.modal = None;
                self.live = None;
                self.tab = tab;
                self.group = group;
                self.page = 0;
                self.panel_sidebar = false;
                self.list_action_origin = None;
            }
            Action::Theme(theme) => {
                if let Err(error) = self.appearance.select(theme) {
                    eprintln!("Appearance preference: {error}");
                    self.notify("Appearance changed; could not save preference");
                }
            }
            Action::Brightness(brightness) => {
                if let Err(error) = self.appearance.dim(brightness) {
                    eprintln!("Brightness preference: {error}");
                    self.notify("Brightness changed; could not save preference");
                }
            }
            // Perform and System are the Perf root (Perf).
            Action::Nav(tab @ (4 | 5)) => {
                self.open_perf_root(false);
                if tab == 5 {
                    self.perf.page = perf_root::SYSTEM;
                    self.perf_enter_page();
                }
            }
            Action::Nav(tab) => {
                // The Mod tab's pages live in the Patch root now.
                let tab = if tab == 2 {
                    self.patch_ui.page = patch_root::MATRIX;
                    1
                } else {
                    tab
                };
                self.patch_ui.choice = None;
                self.patch_ui.entered = None;
                self.page_return = None;
                self.navigation[self.tab] = (self.group, self.page);
                self.modal = None;
                self.live = None;
                self.tab = tab;
                self.panel_sidebar = false;
                self.list_action_origin = None;
                self.menu_root = match tab {
                    1 | 2 => 0,
                    3 => 1,
                    _ => 2,
                };
                (self.group, self.page) = self.navigation[tab];
            }
            Action::Group(g) => {
                self.live = None;
                self.list_action_origin = None;
                self.group = g;
                self.page = 0;
            }
            Action::Section(section) => {
                self.live = None;
                self.list_action_origin = None;
                self.page = section;
            }
            Action::Edit(index) => {
                if self.recording == Some(self.parts[self.part])
                    && self.catalog[index].id == "sample_size"
                {
                    self.notify("Recording: sample size locked");
                } else {
                    let value = self.value(&self.catalog[index]);
                    match self.catalog[index].kind {
                        Kind::Toggle => {
                            self.live = None;
                            self.set(index, if value > 0. { 0. } else { 1. });
                        }
                        Kind::Number { .. } => self.start_live(LiveTarget::Setting(index)),
                        Kind::Choice(_) => {
                            self.live = None;
                            self.modal = Some(Modal::Edit { index, value });
                        }
                    }
                }
            }
            Action::Adjust(delta) => {
                let mut next = None;
                match &self.modal {
                    Some(Modal::Edit { index, value }) => {
                        if let Kind::Number { min, max, .. } = self.catalog[*index].kind {
                            next = Some((*value + delta).clamp(min, max));
                        }
                    }
                    _ => {}
                }
                if let Some(value) = next {
                    self.action(Action::Set(value));
                }
            }
            Action::Set(value) => match &mut self.modal {
                Some(Modal::Edit { value: v, .. }) => *v = value,
                _ => {}
            },
            Action::Apply => {
                if let Some(modal) = self.modal.take() {
                    match modal {
                        Modal::Edit { index, value } => {
                            let id = self.catalog[index].id.clone();
                            if id == "split1" && value > self.get("split2") {
                                let other =
                                    self.catalog.iter().position(|s| s.id == "split2").unwrap();
                                self.set(other, value);
                            }
                            if id == "split2" && value < self.get("split1") {
                                let other =
                                    self.catalog.iter().position(|s| s.id == "split1").unwrap();
                                self.set(other, value);
                            }
                            self.set(index, value);
                        }
                        Modal::Slots { .. } | Modal::Confirm(_) => {}
                    }
                }
            }
            Action::Cancel => self.back(),
            Action::Param(index) => {
                self.selected_param = index.min(5);
                self.start_live(self.main_target(index));
            }
            Action::Part(part) => {
                self.live = None;
                self.part = part;
            }
            Action::PartsOverview => {
                self.parts_overview = !self.parts_overview;
                self.drag = None;
            }
            Action::Slider(i) => {
                let target = self.slider_target(i);
                if let LiveTarget::PartVolume(part) = target {
                    self.action(Action::EditVolume(part));
                } else {
                    self.start_live(target);
                }
            }
            Action::LiveSet(value) => self.write_live(value),
            Action::Nudge(direction) => {
                if let Some(edit) = self.live {
                    let step = self.target_range(edit.target).2;
                    self.write_live(self.target_value(edit.target) + direction * step);
                }
            }
            Action::UndoLive => {
                if let Some(edit) = self.live {
                    if edit.draft.is_none() {
                        self.write_live(edit.original);
                    }
                    if self.device.is_none() {
                        self.patch_mut().dirty = edit.was_dirty;
                    }
                    self.live = None;
                }
            }
            Action::FinishLive => {
                if let Some(LiveEdit {
                    target: LiveTarget::Setting(i),
                    draft: Some(value),
                    ..
                }) = self.live.take()
                {
                    self.set(i, value);
                }
            }
            Action::Choose(value) => {
                if let Some(Modal::Edit { index, .. }) = self.modal.take() {
                    self.set(index, value);
                }
            }
            Action::Shift => {
                self.latched = !self.latched;
                self.shift = self.latched;
            }
            Action::Play => {
                self.playing = !self.playing;
            }
            Action::Save => {
                self.live = None;
                let i = self.parts[self.part];
                self.patch_mut().dirty = false;
                self.saved[i] = self.patch().clone();
                self.notify("Patch saved");
            }
            Action::Name => self.open_name_editor(overlay::NamePurpose::Patch),
            Action::Select(i) => self.selected = i,
            Action::Preset(i) => {
                self.live = None;
                if let Some(other) = self
                    .parts
                    .iter()
                    .enumerate()
                    .find(|(part, p)| *part != self.part && **p == i)
                    .map(|(part, _)| part)
                {
                    self.part = other;
                    self.notify(&format!("Assigned to Part {}", other + 1));
                } else {
                    self.parts[self.part] = i;
                }
            }
            Action::PartEnable(i) => self.enabled[i] = !self.enabled[i],
            Action::EditVolume(i) => {
                if self.part != i {
                    self.live = None;
                    self.part = i;
                }
                self.start_live(LiveTarget::PartVolume(i));
            }
            Action::Snapshot(i) => {
                self.live = None;
                self.snapshot = i;
                let (parts, enabled, volumes) = self.snapshots[i];
                self.parts = parts;
                self.enabled = enabled;
                self.volumes = volumes;
            }
            Action::SlotBank(i) => {
                if let Some(Modal::Slots { bank, .. }) = &mut self.modal {
                    *bank = i;
                }
            }
            Action::ChooseSlot(i) => {
                if let Some(Modal::Slots { operation, .. }) = self.modal.take() {
                    if operation == "assign_part" {
                        if self
                            .parts
                            .iter()
                            .enumerate()
                            .any(|(part, p)| part != self.part && *p == i)
                        {
                            self.notify("Preset assigned to another part");
                        } else {
                            self.parts[self.part] = i;
                        }
                    } else if operation == "copy_sample" {
                        if self.recording == Some(i) {
                            self.recording = None;
                        }
                        self.patches[i].sample = self.patch().sample.clone();
                        let size = self.get("sample_size");
                        self.patches[i].values.insert("sample_size".into(), size);
                        self.patches[i].dirty = true;
                        self.notify(&format!("Sample copied to preset {:02}.", i + 1));
                    } else if operation == "load_patch" {
                        let target = self.parts[self.part];
                        if self.recording == Some(target) {
                            self.recording = None;
                        }
                        self.patches[target] = self.saved[i].clone();
                        self.patches[target].dirty = true;
                        self.notify("Patch loaded");
                    } else {
                        if self.recording == Some(i) {
                            self.recording = None;
                        }
                        self.patches[i] = self.patch().clone();
                        self.patches[i].dirty = true;
                        self.notify(&format!("Patch copied to slot {:02}.", i + 1));
                    }
                }
            }
            Action::Op(op) => self.operation(op),
            Action::Confirm => {
                if let Some(Modal::Confirm(op)) = self.modal.take() {
                    self.confirm(op);
                }
            }
        }
        self.finish_action_context(&before, location, files);
    }
    fn finish_action_context(
        &mut self,
        before: &str,
        location: (usize, usize, usize, String),
        files: Vec<String>,
    ) {
        if location != (self.tab, self.group, self.page, self.directory.clone())
            || files != self.files
        {
            self.list_action_origin = None;
            if files != self.files && self.tab == 5 && self.group == 3 && !self.files.is_empty() {
                self.focus_memory.insert(
                    self.focus_context(),
                    Action::Select(self.selected.min(self.files.len() - 1)),
                );
            }
        }
        if location.3 != self.directory {
            self.focus_memory.remove(&self.focus_context());
            self.focus_target = None;
            self.focus = None;
        }
        self.restore_focus(before);
        self.device_context();
    }
    fn operation(&mut self, op: &'static str) {
        self.live = None;
        // The main screen's graphs open their Patch page (desktop pointer).
        let patch = match op {
            "voices" => Some((patch_root::VOICES, None)),
            "scale" => Some((patch_root::VOICES, Some(patch_root::Item::Scale))),
            "fx_knobs" | "effects" => Some((patch_root::FX, None)),
            "timing" => Some((patch_root::TIMING, None)),
            "envelope" => Some((
                patch_root::SHAPE,
                Some(patch_root::Item::Setting(self.setting_index("attack"))),
            )),
            "grain_window" => Some((
                patch_root::SHAPE,
                Some(patch_root::Item::Setting(self.setting_index("sides"))),
            )),
            "lfo1" => Some((patch_root::LFO1, None)),
            "lfo2" => Some((patch_root::LFO1 + 1, None)),
            "cv1" | "cv2" => Some((
                patch_root::MATRIX,
                Some(patch_root::Item::Setting(self.setting_index(
                    if op == "cv1" {
                        "cv1_amount"
                    } else {
                        "cv2_amount"
                    },
                ))),
            )),
            _ => None,
        };
        if let Some((page, item)) = patch {
            self.open_patch_page(page, item);
            return;
        }
        if self.u2_operation(op) || self.device_operation(op) {
            return;
        }
        match op {
            "stop_record" => self.recording = None,
            "controls" => {
                self.navigation[self.tab] = (self.group, self.page);
                self.tab = 5;
                self.group = 0;
                self.page = 0;
            }
            "copy_sample" => {
                self.modal = Some(Modal::Slots {
                    operation: "copy_sample",
                    bank: self.bank,
                });
            }
            "tune_sample" => {
                let index = self.catalog.iter().position(|s| s.id == "tuning").unwrap();
                self.action(Action::Edit(index));
            }
            "record" => {
                let slot = self.parts[self.part];
                if self.recording == Some(slot) {
                    self.recording = None;
                } else {
                    self.recording = Some(slot);
                    let index = self.setting_index("sampling");
                    self.set(index, 1.);
                }
            }
            "aux" => {
                self.aux_recording = !self.aux_recording;
                self.notify(if self.aux_recording {
                    "AUX recording armed"
                } else {
                    "AUX recording stopped"
                });
            }
            "save_perf" => {
                if self.selected_performance != self.performance_slot {
                    self.modal = Some(Modal::Confirm("save_perf_as"));
                    return;
                }
                for p in &mut self.patches {
                    p.dirty = false;
                }
                self.saved = self.patches.clone();
                self.performance_library[self.performance_slot] =
                    (self.performance.clone(), self.patches.clone());
                self.performance_parts[self.performance_slot] =
                    (self.parts, self.enabled, self.volumes);
                self.performance_configs[self.performance_slot] = self.snapshots;
                self.performance_snapshot[self.performance_slot] = self.snapshot;
                self.notify("Performance saved");
            }
            "copy_patch" => {
                self.modal = Some(Modal::Slots {
                    operation: op,
                    bank: self.bank,
                });
            }
            "update" => self.notify("Updates unavailable in this demo"),
            _ => self.modal = Some(Modal::Confirm(op)),
        }
    }
    fn change_directory(&mut self, path: String) {
        let next = self
            .folders
            .remove(&path)
            .unwrap_or_else(|| match path.as_str() {
                "/USB" => vec!["FOREST.wav".into(), "TAPE_CHORD.wav".into()],
                "/Factory" => vec!["CHOIR.wav".into(), "STRINGS.wav".into()],
                _ => vec![],
            });
        let current = std::mem::replace(&mut self.files, next);
        self.folders.insert(self.directory.clone(), current);
        self.directory = path;
        self.reset_file_scroll();
        self.selected = 0;
        self.page = 0;
    }
    fn confirm(&mut self, op: &str) {
        if self.device_confirm(op) {
            return;
        }
        match op {
            "clear_sample" => {
                if self.recording == Some(self.parts[self.part]) {
                    self.recording = None;
                }
                self.patch_mut().sample = "Empty sample.wav".into();
                self.patch_mut().dirty = true;
            }
            "revert_patch" => {
                let i = self.parts[self.part];
                if self.recording == Some(i) {
                    self.recording = None;
                }
                self.patches[i] = self.saved[i].clone();
            }
            "clear_patch" => {
                let i = self.parts[self.part];
                if self.recording == Some(i) {
                    self.recording = None;
                }
                self.patches[i] = Patch::new(i);
                self.patches[i].name = "Empty patch".into();
                self.patches[i].sample = "Empty sample.wav".into();
                self.patches[i].dirty = true;
            }
            "init_perf" => {
                self.recording = None;
                self.patches = (0..32)
                    .map(|i| {
                        let mut p = Patch::new(i);
                        p.name = format!("Empty {:02}", i + 1);
                        p.sample = "Empty sample.wav".into();
                        p.dirty = true;
                        p
                    })
                    .collect();
                self.performance = "New performance".into();
            }
            "save_perf_as" => {
                self.performance_slot = self.selected_performance;
                self.operation("save_perf");
            }
            "load_perf" => {
                self.recording = None;
                self.performance_slot = self
                    .selected_performance
                    .min(self.performance_library.len() - 1);
                let (name, patches) = self.performance_library[self.performance_slot].clone();
                self.performance = name;
                self.patches = patches;
                self.saved = self.patches.clone();
                (self.parts, self.enabled, self.volumes) =
                    self.performance_parts[self.performance_slot];
                self.snapshots = self.performance_configs[self.performance_slot];
                self.snapshot = self.performance_snapshot[self.performance_slot];
            }
            "copy_all" | "template_all" => {
                self.recording = None;
                let source = self.patch().clone();
                for p in &mut self.patches {
                    let sample = p.sample.clone();
                    *p = source.clone();
                    if op == "template_all" {
                        p.sample = sample;
                    }
                    p.dirty = true;
                }
            }
            "factory" => {
                self.system.clear();
            }
            "shutdown" => self.playing = false,
            "unmount" => {
                let internal = "/Internal".to_string();
                let names = self.list_folder(&internal).unwrap_or_default();
                self.files_ui.folders.insert(internal.clone(), names);
                self.complete_unmount(Ok(()), internal, None).unwrap();
            }
            _ => {}
        }
        self.toast.clear();
    }
    fn release_pointer(&mut self, x: i32, y: i32) {
        if let Some(Drag::List {
            start_x,
            start_y,
            moved: false,
            ..
        }) = self.drag.take()
        {
            if (x - start_x).abs() < 7 && (y - start_y).abs() < 7 {
                if let Some(hit) = self.hits.iter().rev().find(|h| h.r.contains(x, y)).cloned() {
                    self.action(hit.a);
                    if self.live.is_some() && hit.r.1 + hit.r.3 > 344 {
                        if let Some(list) = self.list {
                            self.scroll_offsets
                                .insert(self.scroll_key(), list.offset + hit.r.1 + hit.r.3 - 344);
                        }
                    }
                }
            }
        }
    }
    fn pointer(&mut self, x: i32, y: i32, down: bool) {
        if down && self.sample_pointer(x, y) {
            return;
        }
        if down {
            self.focus = None;
            self.focus_target = None;
            if self.modal.is_none() {
                if let Some(list) = self.list {
                    if list.bounds.contains(x, y) {
                        self.drag = Some(if x >= 764 && list.max_offset() > 0 {
                            let thumb = list.thumb();
                            Drag::Scrollbar {
                                grab: if y >= thumb.1 && y < thumb.1 + thumb.3 {
                                    y - thumb.1
                                } else {
                                    thumb.3 / 2
                                },
                            }
                        } else {
                            Drag::List {
                                start_x: x,
                                start_y: y,
                                offset: list.offset,
                                moved: false,
                            }
                        });
                        if matches!(self.drag, Some(Drag::List { .. })) {
                            return;
                        }
                    }
                }
            }
            if matches!(self.drag, Some(Drag::Scrollbar { .. })) {
                // The scroll rail owns this gesture.
            } else if self.modal.is_none()
                && self.tab == 0
                && !self.parts_overview
                && self.enabled.iter().filter(|v| **v).count() <= 1
                && Rect(0, 23, 544, 399).contains(x, y)
            {
                self.drag = Some(Drag::Position);
            } else if self.modal.is_none() && self.live.is_some() && self.tab != 1 && {
                let r = self.live_slider();
                Rect(r.0 - 6, r.1, r.2 + 14, r.3).contains(x, y)
            } {
                self.drag = Some(Drag::Live);
            } else if match &self.modal {
                Some(Modal::Edit { index, .. }) => {
                    matches!(self.catalog[*index].kind, Kind::Number { .. })
                }
                _ => false,
            } && Rect(24, 233, 752, 48).contains(x, y)
            {
                self.drag = Some(Drag::Editor);
            } else {
                let hit = self.hits.iter().rev().find(|h| h.r.contains(x, y)).cloned();
                if let Some(hit) = hit {
                    self.focus_target = Some(hit.a.clone());
                    self.action(hit.a);
                }
                return;
            }
        }
        match self.drag {
            Some(Drag::List {
                start_x,
                start_y,
                offset,
                moved,
            }) => {
                if moved || (y - start_y).abs() >= 7 || (x - start_x).abs() >= 7 {
                    self.drag = Some(Drag::List {
                        start_x,
                        start_y,
                        offset,
                        moved: true,
                    });
                    self.scroll_to(offset + start_y - y);
                }
            }
            Some(Drag::Scrollbar { grab }) => {
                if let Some(list) = self.list {
                    let travel = (list.bounds.3 - list.thumb().3).max(1);
                    self.scroll_to((y - list.bounds.1 - grab) * list.max_offset() / travel);
                }
            }
            Some(Drag::Position) => {
                let value = (x as f32 / 543. * 100.).clamp(0., 100.);
                self.start_live(LiveTarget::Param(0, false));
                self.write_live(value);
            }
            Some(Drag::Live) => {
                if let Some(edit) = self.live {
                    let slider = self.live_slider();
                    let (min, max, step) = self.target_range(edit.target);
                    let fraction = ((x - slider.0) as f32 / slider.2 as f32).clamp(0., 1.);
                    let value = if min > 0. && max / min >= 1000. {
                        min * (max / min).powf(fraction)
                    } else {
                        min + fraction * (max - min)
                    };
                    self.write_live((value / step).round() * step);
                }
            }
            Some(Drag::Editor) => {
                let range = match &self.modal {
                    Some(Modal::Edit { index, .. }) => match self.catalog[*index].kind {
                        Kind::Number { min, max, step, .. } => Some((min, max, step)),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some((min, max, step)) = range {
                    let value = (min + ((x - 34) as f32 / 732.).clamp(0., 1.) * (max - min)) / step;
                    self.action(Action::Set((value.round() * step).clamp(min, max)));
                }
            }
            None => {}
        }
    }
}
fn operation_text(op: &str, device: bool) -> (&str, &str) {
    if device {
        match op {
            "init_perf" => return ("Initialize performance?", "Start with 32 empty preset slots. Unsaved changes are discarded; saved files remain available."),
            "factory" => return ("Reset device settings?", "Restore and save the original device settings. Patch and sample files remain available."),
            "shutdown" => return ("Shut down the instrument?", "Stop playback and power off the GR-1. Unsaved changes will be lost."),
            "unmount" => return ("Unmount USB drive?", "Release the USB drive after file writes finish so it can be removed."),
            _ => {}
        }
    }
    match op {
    "clear_sample"=>("Clear this sample?","Replace the current sample with an empty recording buffer. The saved patch remains available."),
    "revert_patch"=>("Revert this patch?","Restore the last saved sample and sound settings for this preset. Unsaved changes will be discarded."),
    "clear_patch"=>("Clear this patch?","Reset the current preset's sample and sound parameters. Saving is a separate action."),
    "delete_file"=>("Delete selected items?","Move these items to the recoverable trash folder."),
    "init_perf"=>("Initialize performance?","Start with 32 empty preset slots. The prototype's saved state is retained until another save."),
    "save_perf_as"=>("Save to this slot?","Replace the selected saved performance with the current performance. The source slot remains available."),
    "load_perf"=>("Load performance?","Replace the current working performance with the selected one."),
    "import_perf"=>("Load performance?","Replace the current working performance with the selected file. Save it to a user slot to keep it."),
    "copy_all"=>("Copy patch to all presets?","Copy the current sample and settings to all 32 preset slots."),
    "template_all"=>("Copy settings to all presets?","Copy the current patch settings while keeping each preset's sample."),
    "from_samples"=>("Build from samples?","Load WAV files from this folder into successive presets."),
    "factory"=>("Reset device settings?","Restore the prototype's device settings to their initial values."),
    "shutdown"=>("Shut down the instrument?","On the device this would stop the engine and shut down. Here it stops only the visual Play preview."),
    "unmount"=>("Unmount USB drive?","On the instrument this would finish file operations before releasing the drive. No drive is accessed here."),
    _=>("Continue?","Apply this operation to the current instrument state.")}
}

pub fn run() -> Result<(), String> {
    let args = std::env::args().collect::<Vec<_>>();
    let theme_override = args
        .iter()
        .position(|a| a == "--theme")
        .map(|i| Theme::parse(args.get(i + 1).ok_or("--theme requires light or dark")?))
        .transpose()?;
    let mut app = App::new();
    app.device = device_ui::command_line(&args)?;
    app.panel_mode = app.device.is_some() || args.iter().any(|a| a == "--panel-preview");
    let appearance_file = app.device.as_ref().map(|d| d.layout.appearance());
    app.appearance = Appearance::load(appearance_file.as_deref());
    if let Some(theme) = theme_override {
        app.appearance.theme = theme;
    }
    app.sync_device_view();
    app.refresh_performance_list();
    let mut painter = Painter::new()?;
    if let Some(i) = args.iter().position(|a| a == "--benchmark-render") {
        let frames = args
            .get(i + 1)
            .ok_or("--benchmark-render needs a frame count")?
            .parse::<usize>()
            .map_err(|e| e.to_string())?
            .clamp(60, 10000);
        let mut elapsed = Vec::with_capacity(frames);
        app.render(&mut painter);
        for _ in 0..frames {
            let start = Instant::now();
            app.render(&mut painter);
            elapsed.push(start.elapsed().as_secs_f64() * 1000.);
        }
        elapsed.sort_by(f64::total_cmp);
        println!("CPU render only, {frames} frames: p50={:.3}ms p95={:.3}ms max={:.3}ms; engine/presentation not measured",
            elapsed[frames / 2], elapsed[frames * 95 / 100], elapsed[frames - 1]);
        return Ok(());
    }
    if let Some(i) = args.iter().position(|a| a == "--captures") {
        let directory = Path::new(args.get(i + 1).ok_or("--captures requires a directory")?);
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        for tab in 0..6 {
            app.action(Action::Nav(tab));
            app.render(&mut painter);
            painter.png(&directory.join(format!("{tab}-{}.png", TABS[tab].to_lowercase())))?;
        }
        app.action(Action::Nav(0));
        app.action(Action::Shift);
        app.render(&mut painter);
        painter.png(&directory.join("shift.png"))?;
        app.action(Action::Shift);
        app.action(Action::PartsOverview);
        app.render(&mut painter);
        painter.png(&directory.join("play-parts.png"))?;
        app.action(Action::PartsOverview);
        app.action(Action::Param(1));
        app.render(&mut painter);
        painter.png(&directory.join("size-editor.png"))?;
        app.modal = None;
        app.action(Action::Name);
        app.render(&mut painter);
        painter.png(&directory.join("name-editor-rename.png"))?;
        app.close_overlay();
        // The Patch and Sample roots have their own pages, captured below.
        for tab in [4, 5] {
            for (group, label) in catalog::groups(TABS[tab]).iter().enumerate() {
                app.action(Action::Nav(tab));
                app.action(Action::Group(group));
                let pages = match (tab, group) {
                    (1, 3) => 4,
                    (1, 4) => 3,
                    (1, 2 | 5) | (2, 1 | 2) | (4, 2) => 2,
                    _ => 1,
                };
                for page in 0..pages {
                    app.action(Action::Section(page));
                    app.render(&mut painter);
                    painter.png(&directory.join(format!(
                        "{}-{}-{}.png",
                        TABS[tab].to_lowercase(),
                        label.replace([' ', '&'], "-"),
                        page
                    )))?;
                }
            }
        }
        app.action(Action::Nav(0));
        app.action(Action::Op("copy_patch"));
        app.render(&mut painter);
        painter.png(&directory.join("copy-destination.png"))?;
        // Main-screen states of the redesign, from simulated desktop values.
        app.modal = None;
        app.action(Action::Nav(0));
        app.playing = true;
        app.patch_mut().params[6] = 8.;
        app.patch_mut().params[7] = 86.;
        app.patch_mut().dirty = true;
        app.patch_mut().routes[1][3] = -0.3;
        let dsp = app.setting_index("dsp");
        app.set(dsp, 1.);
        app.render(&mut painter);
        app.touch.last = None;
        app.render(&mut painter);
        painter.png(&directory.join("main-playing.png"))?;
        app.action(Action::Param(2));
        app.render(&mut painter);
        painter.png(&directory.join("main-spray.png"))?;
        app.live = None;
        app.enabled = [true, true, false, true];
        app.action(Action::Part(1));
        app.action(Action::Param(2));
        app.render(&mut painter);
        painter.png(&directory.join("main-parts.png"))?;
        app.live = None;
        app.action(Action::Part(0));
        app.enabled = [true, false, false, false];
        let mode = app.setting_index("adsr_mode");
        app.set(mode, 1.);
        app.render(&mut painter);
        painter.png(&directory.join("main-voice-level.png"))?;
        app.set(mode, 0.);
        // LFO 1 between the wave and the band, LFO 2 in the band.
        for (id, rate) in [("lfo1_rate", 6.), ("lfo2_rate", 10.)] {
            let i = app.setting_index(id);
            app.set(i, rate);
        }
        app.render(&mut painter);
        painter.png(&directory.join("main-lfo-fast.png"))?;
        for i in ["lfo1_rate", "lfo2_rate"] {
            let i = app.setting_index(i);
            let default = app.catalog[i].default;
            app.set(i, default);
        }
        // file screens, reached through the panel dispatcher.
        app.files
            .extend(["Glass tide.grp".into(), "Night pad.grp".into()]);
        let panel = |app: &mut App, painter: &mut Painter, events: &[(u8, u8)]| {
            let events = events
                .iter()
                .map(|(action, value)| crate::device::native::PanelEvent {
                    action: *action,
                    value: *value,
                    text: None,
                })
                .collect();
            app.dispatch_panel_events(painter, events);
            app.render(painter);
            app.run_pending();
            app.render(painter);
        };
        // Each list starts from the main screen; the overlay closes between them.
        for (name, events) in [
            ("load-sample", &[(33, 0), (1, 0), (5, 3)][..]),
            ("load-sheet", &[(28, 0)]),
            ("load-locations", &[(14, 0), (14, 0)]),
            ("load-patch", &[(14, 0), (1, 1), (5, 1)]),
            ("load-perf", &[(14, 0), (14, 0), (1, 2), (5, 1)]),
            ("confirm-load", &[(1, 0)]),
            ("save-patch", &[(14, 0), (14, 0), (14, 0), (34, 0), (2, 1)]),
            ("name-editor", &[(5, 255), (7, 0), (5, 9)]),
            ("save-sample", &[(14, 0), (33, 0)]),
            ("save-perf", &[(35, 0), (5, 1)]),
            ("pick-slot", &[(7, 0), (5, 1)]),
            ("confirm-replace", &[(7, 0), (2, 1)]),
            ("pick-folder", &[(14, 0), (33, 0), (5, 255), (7, 0)]),
        ] {
            panel(&mut app, &mut painter, events);
            painter.png(&directory.join(format!("{name}.png")))?;
        }
        // An error notice, seeded: it stays in red until the next input.
        panel(&mut app, &mut painter, &[(14, 0)]);
        app.error("Save failed: 120 MB free");
        app.render(&mut painter);
        painter.png(&directory.join("error-notice.png"))?;
        app.close_overlay();
        // Sample Sample root, through the panel dispatcher, from the main screen.
        // Recording, its level and position are the demo's simulation.
        app.toast.clear();
        app.toast_error = false;
        app.action(Action::Nav(0));
        for (name, events) in [
            ("sample-record", &[(15, 0)][..]),
            ("sample-record-size", &[(5, 5), (7, 0), (5, 40)]),
            (
                "sample-record-start-mode",
                &[(14, 0), (5, 1), (7, 0), (5, 2)],
            ),
            ("sample-record-recording", &[(7, 0), (5, 250), (7, 0)]),
        ] {
            panel(&mut app, &mut painter, events);
            if name == "sample-record-recording" {
                app.sample_ui.demo_record_started =
                    Some(Instant::now() - Duration::from_millis(3210));
                app.render(&mut painter);
            }
            painter.png(&directory.join(format!("{name}.png")))?;
        }
        app.sample_ui.clip = true;
        app.render(&mut painter);
        painter.png(&directory.join("sample-record-clip.png"))?;
        for (name, events) in [
            ("sample-confirm-clear", &[(7, 0), (5, 1), (7, 0)][..]),
            ("sample-save-sheet", &[(14, 0), (5, 1), (7, 0)]),
            ("sample-strip", &[(14, 0), (14, 0)]),
            ("sample-sample-strip", &[(5, 1)]),
            ("sample-sample", &[(7, 0)]),
            ("sample-sample-tuning", &[(5, 1), (7, 0), (5, 2)]),
            ("sample-sample-copy", &[(14, 0), (5, 5), (7, 0)]),
        ] {
            panel(&mut app, &mut painter, events);
            painter.png(&directory.join(format!("{name}.png")))?;
        }
        panel(&mut app, &mut painter, &[(14, 0), (14, 0)]);
        // Patch: every Patch page, opened with Shift+Patch and stepped with the
        // same chord, then the grab, strip and container states.
        app.action(Action::Nav(0));
        app.patch_ui.page = patch_root::PLAY;
        app.patch_mut().routes[0][2] = 0.4;
        app.patch_mut().routes[0][9] = 0.15;
        app.patch_mut().routes[2][0] = 0.2;
        for (i, page) in patch_root::PAGES.iter().enumerate() {
            panel(&mut app, &mut painter, &[(13, 0)]);
            let name = page.to_lowercase().replace(' ', "");
            painter.png(&directory.join(format!("patch-{}-{name}.png", i + 1)))?;
        }
        for (name, events) in [
            // From LFO 2: Escape to the strip and turn back to Timing.
            ("patch-strip", &[(14, 0), (5, 249)][..]),
            // Enter Timing, turn to Scan mode, grab it, turn two options.
            ("patch-timing-grab", &[(7, 0), (5, 5), (7, 0), (5, 2)]),
            // Escape restores; to the strip, Matrix, the LFO 2 row, Size.
            (
                "patch-matrix-row",
                &[(14, 0), (14, 0), (5, 5), (7, 0), (5, 1), (7, 0), (5, 3)],
            ),
            ("patch-matrix-grab", &[(7, 0)]),
            // Keep, leave the row, to Shape, Decay grabbed.
            (
                "patch-shape-decay",
                &[(7, 0), (14, 0), (14, 0), (5, 252), (7, 0), (5, 1), (7, 0)],
            ),
            // Restore, to Voices, the scale entered on D.
            (
                "patch-voices-scale",
                &[(14, 0), (14, 0), (5, 2), (7, 0), (5, 4), (7, 0), (5, 2)],
            ),
            // Leave the scale, to LFO 1, Rate grabbed.
            (
                "patch-lfo-rate",
                &[(14, 0), (14, 0), (5, 3), (7, 0), (5, 1), (7, 0)],
            ),
        ] {
            panel(&mut app, &mut painter, events);
            painter.png(&directory.join(format!("{name}.png")))?;
        }
        panel(&mut app, &mut painter, &[(14, 0), (14, 0), (14, 0)]);
        // Perf Perf root, through the panel dispatcher, from the main screen.
        // The demo library, presets and folders are desktop demonstration data.
        app.copy_arm = None;
        app.toast.clear();
        app.toast_error = false;
        app.live = None;
        app.touch.last = None;
        app.files_ui.lit = overlay::DataKind::Patch;
        app.action(Action::Nav(0));
        app.perf.page = perf_root::PERFORMANCES;
        app.perf.slots = Default::default();
        app.perf_demo_slots(15);
        for (slot, name) in [
            (9, "Drone sketches"),
            (11, "Field notes"),
            (12, "Choir loops"),
            (14, "Tape studies"),
        ] {
            app.performance_library[slot].0 = name.into();
        }
        for (i, (name, sample)) in [
            ("Spatial Strings", "STRINGS_C3.wav"),
            ("Dust motes", "DUST.wav"),
            ("Low bells", "BELLS_LOW.wav"),
            ("Granite", "GRANITE.wav"),
            ("Breath pad", "BREATH.wav"),
            ("Choir drift", "CHOIR.wav"),
            ("Frozen reeds", "REEDS_04.wav"),
            ("Wire hum", "WIRE.wav"),
            ("Organ ghost", "ORGAN.wav"),
            ("Rain glass", "RAIN.wav"),
            ("Metal rain", "METAL.wav"),
            ("Glass bowl", "GLASS_BOWL.wav"),
            ("Paper fold", "PAPER_FOLD.wav"),
            ("Slow water", "SLOW_WATER.wav"),
            ("Kalimba", "KALIMBA.wav"),
            ("Bow drone", "BOW.wav"),
            ("Night train", "TRAIN_NIGHT.wav"),
            ("Tape loop", "TAPE_LOOP.wav"),
        ]
        .into_iter()
        .enumerate()
        {
            let slot = if i < 16 { i } else { i + 2 };
            app.patches[slot].name = name.into();
            app.patches[slot].sample = sample.into();
        }
        app.patches[6].dirty = true;
        app.parts = [6, 11, 2, 18];
        app.enabled = [true, true, false, true];
        app.volumes = [80., 70., 80., 65.];
        app.part = 0;
        app.files = vec![
            "Drone samples/".into(),
            "Field samples/".into(),
            "Patches/".into(),
            "factory/".into(),
            "performance01/".into(),
            "performance02/".into(),
            "performance03/".into(),
            "performance04/".into(),
            "performance05/".into(),
            "REEDS_04.wav".into(),
        ];
        for (i, folder) in ["Drone samples", "Field samples", "Patches"]
            .iter()
            .enumerate()
        {
            app.folders.insert(
                format!("/Internal/{folder}"),
                (0..[18, 24, 40][i])
                    .map(|n| format!("TAKE_{n:02}.wav"))
                    .collect(),
            );
        }
        for (name, events) in [
            ("perf-performances", &[(12, 0), (5, 11)][..]),
            ("perf-slot-sheet", &[(7, 0)]),
            ("perf-confirm-replace", &[(14, 0), (2, 1)]),
            ("perf-strip", &[(14, 0), (14, 0)]),
            ("perf-presets", &[(5, 1), (7, 0), (5, 11)]),
            ("perf-preset-sheet", &[(7, 0)]),
            ("perf-preset-copy", &[(14, 0), (4, 0), (5, 3)]),
            ("perf-parts", &[(14, 0), (12, 0), (5, 2)]),
            ("perf-parts-level", &[(7, 0), (7, 0), (5, 246)]),
            ("perf-files", &[(7, 0), (14, 0), (12, 0)]),
        ] {
            panel(&mut app, &mut painter, events);
            painter.png(&directory.join(format!("{name}.png")))?;
        }
        // Holding Shift selects: a range of two folders, then performance03.
        app.shift = true;
        panel(&mut app, &mut painter, &[]);
        panel(&mut app, &mut painter, &[(5, 10)]);
        app.shift = false;
        panel(&mut app, &mut painter, &[(5, 5)]);
        app.shift = true;
        panel(&mut app, &mut painter, &[]);
        app.shift = false;
        panel(&mut app, &mut painter, &[]);
        app.render(&mut painter);
        painter.png(&directory.join("perf-files-select.png"))?;
        for (name, events) in [
            ("perf-files-sheet", &[(28, 0)][..]),
            ("perf-locations", &[(14, 0), (14, 0)]),
            ("perf-system", &[(14, 0), (12, 0)]),
            ("perf-system-brightness", &[(5, 1), (7, 0), (5, 255)]),
            ("perf-system-scrolled", &[(14, 0), (5, 15)]),
            ("perf-system-shutdown", &[(5, 10), (7, 0)]),
        ] {
            panel(&mut app, &mut painter, events);
            painter.png(&directory.join(format!("{name}.png")))?;
        }
        // Copy preset 07 to all from the main screen with Perf lit (J8).
        panel(
            &mut app,
            &mut painter,
            &[(14, 0), (14, 0), (14, 0), (35, 0), (4, 0)],
        );
        painter.png(&directory.join("perf-copy-all.png"))?;
        panel(&mut app, &mut painter, &[(14, 0)]);
        println!("Rendered native UI states to {}", directory.display());
        return Ok(());
    }
    sdl2::hint::set("SDL_TOUCH_MOUSE_EVENTS", "0");
    sdl2::hint::set("SDL_MOUSE_TOUCH_EVENTS", "0");
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let mut window_builder = video.window(
        if app.device.is_some() {
            "GR-1"
        } else {
            "GR-1 UI study - Rust / simulated engine"
        },
        if app.device.is_some() { 800 } else { 1000 },
        if app.device.is_some() { 480 } else { 600 },
    );
    if app.device.is_some() {
        window_builder.fullscreen();
    } else {
        window_builder.position_centered().resizable();
    }
    let window = window_builder.build().map_err(|e| e.to_string())?;
    let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
    eprintln!("GR-1 renderer: {}", canvas.info().name);
    if app.device.is_some() {
        sdl.mouse().show_cursor(false);
    }
    canvas
        .set_logical_size(800, 480)
        .map_err(|e| e.to_string())?;
    let creator = canvas.texture_creator();
    let mut texture = creator
        .create_texture_streaming(PixelFormatEnum::RGBA32, 800, 480)
        .map_err(|e| e.to_string())?;
    let mut events = sdl.event_pump()?;
    video.text_input().start();
    // Complete graphics initialization while the calling thread still has
    // its original scheduling policy. Starting native synthesis changes it.
    if let Some(d) = &mut app.device {
        d.start()?;
        app.sync_device_view();
    }
    let mut dirty = true;
    let period = Duration::from_nanos(1_000_000_000 / 60);
    let mut deadline = Instant::now();
    let mut timing = timing::FrameTiming::new();
    let mut readback = timing::ReadbackTrace::new();
    let mut trace_dir = std::env::var_os("GR1_UI_TRACE_DIR").map(std::path::PathBuf::from);
    let mut captured_generation = None;
    'running: loop {
        let started = Instant::now();
        while deadline <= started {
            deadline += period;
        }
        // The screen id the engine runs under during this frame's poll.
        let polled_screen = app.device.as_ref().and_then(|d| d.screen_id());
        if app.device.is_some() {
            app.poll_device(&mut painter);
            dirty = true;
        }
        // A running paste and the Perf root's stencils take a slice per frame.
        if app.perf_frame() {
            dirty = true;
        }
        if app.listing_frame() {
            dirty = true;
        }
        if app.device.is_none() && matches!(app.tab, 0 | 1 | 3) {
            dirty = true;
        }
        if dirty {
            let draw_started = Instant::now();
            app.render(&mut painter);
            let draw_time = draw_started.elapsed();
            let present_started = Instant::now();
            texture
                .update(None, &painter.pixels, 800 * 4)
                .map_err(|e| e.to_string())?;
            canvas.clear();
            canvas.copy(&texture, None, None)?;
            canvas.present();
            timing.frame(
                app.native_poll_time,
                app.view_sync_time,
                draw_time,
                present_started.elapsed(),
                app.input_received.take(),
            );
            let strip = app.strip_drawn.take();
            if let (true, Some(d)) = (readback.enabled(), &app.device) {
                let part = d.part();
                let sample = timing::Readback {
                    grains: d.display.grains.clone(),
                    scan: d.display.scan[part],
                    position: d.display.position[part],
                    spray: d.display.spray[part],
                    lfo: d
                        .display
                        .lfo_history
                        .back()
                        .map_or([f32::NAN; 2], |(_, v)| *v),
                };
                let t = app.demo_started.elapsed().as_secs_f64();
                if let Some(line) =
                    readback.frame(Instant::now(), t, polled_screen, part, sample, strip)
                {
                    eprintln!("{line}");
                }
            }
            if let (Some(dir), Some(d)) = (&trace_dir, &app.device) {
                if app.tab == 0
                    && app.modal.is_none()
                    && captured_generation != Some(d.display.generation)
                {
                    let path = dir.join(format!(
                        "wave-{:06}-slot-{:02}.png",
                        d.display.generation,
                        d.slot()
                    ));
                    let result = std::fs::create_dir_all(dir)
                        .map_err(|e| e.to_string())
                        .and_then(|_| painter.png(&path));
                    if let Err(e) = result {
                        eprintln!("[gr1-display] framebuffer capture disabled: {e}");
                        trace_dir = None;
                    } else {
                        eprintln!("[gr1-display] framebuffer={}", path.display());
                    }
                    captured_generation = Some(d.display.generation);
                }
            }
            dirty = false;
            if app.files_ui.pending.is_some() {
                // The frame naming the bulk operation is on screen; run it.
                app.run_pending();
                dirty = true;
            }
        }
        let wait = if app.device.is_some()
            || matches!(app.tab, 0 | 1 | 3)
            || app.perf.files.job.is_some()
            || app.files_ui.listing.is_some()
        {
            deadline
                .saturating_duration_since(Instant::now())
                .as_micros()
                .div_ceil(1000) as u32
        } else {
            250
        };
        let event = events.wait_event_timeout(wait);
        if !app.toast.is_empty() && Instant::now() >= app.toast_until {
            app.toast.clear();
            dirty = true;
        }
        let Some(event) = event else { continue };
        match event {
            Event::Quit { .. } => break 'running,
            Event::Window {
                win_event: WindowEvent::Exposed | WindowEvent::SizeChanged(..),
                ..
            } => dirty = true,
            Event::Window {
                win_event: WindowEvent::FocusLost,
                ..
            } => {
                app.shift = app.latched;
                if let Some(d) = &mut app.device {
                    d.shift(app.latched);
                }
                app.drag = None;
                dirty = true;
            }
            Event::MouseButtonDown {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } => {
                app.pointer(x, y, true);
                dirty = true;
            }
            Event::MouseMotion {
                x, y, mousestate, ..
            } if mousestate.left() => {
                app.pointer(x, y, false);
                dirty = true;
            }
            Event::MouseButtonUp {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } => {
                app.release_pointer(x, y);
                dirty = true;
            }
            Event::FingerDown { x, y, .. } => {
                app.pointer((x * 800.) as i32, (y * 480.) as i32, true);
                dirty = true;
            }
            Event::FingerMotion { x, y, .. } => {
                app.pointer((x * 800.) as i32, (y * 480.) as i32, false);
                dirty = true;
            }
            Event::FingerUp { x, y, .. } => {
                app.release_pointer((x * 800.) as i32, (y * 480.) as i32);
                dirty = true;
            }
            Event::MouseWheel {
                precise_y,
                direction,
                mouse_x,
                mouse_y,
                ..
            } => {
                if app
                    .list
                    .map(|l| l.bounds.contains(mouse_x, mouse_y))
                    .unwrap_or(false)
                {
                    let sign = if direction == MouseWheelDirection::Flipped {
                        1.
                    } else {
                        -1.
                    };
                    app.scroll_by((precise_y * sign * 36.).round() as i32);
                    dirty = true;
                }
            }
            Event::TextInput { text, .. } => {
                app.trace_sdl(&format!("text={text:?}"));
                if app.name_text(&text) {
                    dirty = true;
                }
            }
            Event::KeyDown {
                keycode: Some(key),
                keymod,
                repeat,
                ..
            } => {
                app.trace_sdl(&format!(
                    "key={} mod={keymod:?} repeat={repeat}",
                    key.name()
                ));
                if let Some(events) = app.sample_key(&key.name()) {
                    app.dispatch_panel_events(&mut painter, events);
                    dirty = true;
                    continue;
                }
                if let Some(events) = app.overlay_key(&key.name()) {
                    app.dispatch_panel_events(&mut painter, events);
                    dirty = true;
                    continue;
                }
                // Inside the Perf root (or its confirmation) the keys stand in
                // for the panel, as for the other roots.
                if let Some(events) = app.perf_key(&key.name()) {
                    app.dispatch_panel_events(&mut painter, events);
                    dirty = true;
                    continue;
                }
                // F1 is Shift+Patch; inside the Patch root the arrows, Return
                // and Escape are the encoder, Select and Escape, through the
                // same dispatcher as the panel.
                let patch_key = match key {
                    Keycode::F1 => Some((13, 0)),
                    _ if !app.in_patch_root() => None,
                    Keycode::Up | Keycode::Left => Some((5, 255)),
                    Keycode::Down | Keycode::Right => Some((5, 1)),
                    Keycode::Return | Keycode::KpEnter => Some((7, 0)),
                    Keycode::Escape => Some((14, 0)),
                    _ => None,
                };
                if let Some((action, value)) = patch_key {
                    app.dispatch_panel_events(
                        &mut painter,
                        vec![crate::device::native::PanelEvent {
                            action,
                            value,
                            text: None,
                        }],
                    );
                    dirty = true;
                    continue;
                }
                if app.overlay_active()
                    && matches!(key, Keycode::F1 | Keycode::F2 | Keycode::F3 | Keycode::F4)
                {
                    app.close_overlay();
                }
                if matches!(key, Keycode::F2 | Keycode::F4) {
                    app.perf_release();
                }
                match key {
                    Keycode::F2 => app.open_sample_root(false),
                    Keycode::F3 => app.open_panel_menu(2),
                    Keycode::F4 => app.action(Action::Nav(0)),
                    Keycode::F5 | Keycode::F6 => app.dispatch_panel_events(
                        &mut painter,
                        vec![crate::device::native::PanelEvent {
                            action: if key == Keycode::F5 { 1 } else { 2 },
                            value: if key == Keycode::F5 { 0 } else { 1 },
                            text: None,
                        }],
                    ),
                    Keycode::Up | Keycode::Down | Keycode::Left | Keycode::Right
                        if app.panel_mode =>
                    {
                        app.panel_turn(if matches!(key, Keycode::Up | Keycode::Left) {
                            -1
                        } else {
                            1
                        });
                    }
                    Keycode::Return | Keycode::KpEnter if app.panel_mode => app.panel_press(),
                    Keycode::Escape => app.back(),
                    Keycode::LShift | Keycode::RShift => {
                        if !repeat {
                            app.shift = true;
                            if let Some(d) = &mut app.device {
                                d.shift(true);
                            }
                            app.perf_shift_edge();
                        }
                    }
                    Keycode::F12 => {
                        let path = std::env::temp_dir().join("gr1-ui-current.png");
                        painter.png(&path)?;
                        app.notify("Screen captured");
                    }
                    Keycode::Up | Keycode::Down
                        if app.list.is_some() && app.modal.is_none() && app.live.is_none() =>
                    {
                        app.scroll_by(if key == Keycode::Up { -56 } else { 56 });
                    }
                    Keycode::Home | Keycode::End if app.list.is_some() && app.modal.is_none() => {
                        app.scroll_to(if key == Keycode::Home { 0 } else { i32::MAX });
                    }
                    Keycode::Tab | Keycode::Down | Keycode::Up => {
                        let backwards = key == Keycode::Up
                            || keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD);
                        app.move_focus(if backwards { -1 } else { 1 });
                    }
                    Keycode::Return | Keycode::KpEnter => {
                        if app.live.is_some() && app.modal.is_none() {
                            app.action(Action::FinishLive);
                        } else if let Some(i) = app.focus {
                            if let Some(hit) = app.hits.get(i).cloned() {
                                app.action(hit.a);
                            }
                        } else if app.modal.is_some() {
                            app.action(Action::Apply);
                        }
                    }
                    Keycode::Num1
                    | Keycode::Num2
                    | Keycode::Num3
                    | Keycode::Num4
                    | Keycode::Num5
                    | Keycode::Num6
                    | Keycode::Num7
                    | Keycode::Num8
                        if app.modal.is_none() && app.tab == 0 =>
                    {
                        let index = match key {
                            Keycode::Num1 => 0,
                            Keycode::Num2 => 1,
                            Keycode::Num3 => 2,
                            Keycode::Num4 => 3,
                            Keycode::Num5 => 4,
                            Keycode::Num6 => 5,
                            Keycode::Num7 => 6,
                            _ => 7,
                        };
                        app.action(Action::Param(index));
                    }
                    Keycode::Left | Keycode::Right => {
                        let sign = if key == Keycode::Left { -1. } else { 1. };
                        if app.live.is_some() && app.modal.is_none() {
                            app.action(Action::Nudge(sign));
                            dirty = true;
                            continue;
                        }
                        let step = match &app.modal {
                            Some(Modal::Edit { index, .. }) => match app.catalog[*index].kind {
                                Kind::Number { step, .. } => step,
                                _ => 1.,
                            },
                            _ => 0.,
                        };
                        if step > 0. {
                            app.action(Action::Adjust(sign * step));
                        }
                    }
                    Keycode::Space if !app.overlay_active() => app.action(Action::Play),
                    _ => {}
                }
                dirty = true;
            }
            Event::KeyUp {
                keycode: Some(Keycode::LShift | Keycode::RShift),
                ..
            } => {
                app.shift = app.latched;
                if let Some(d) = &mut app.device {
                    d.shift(app.latched);
                }
                app.perf_shift_edge();
                dirty = true;
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod workflow_tests {
    use super::*;

    #[test]
    fn sample_copy_preserves_destination_sound_and_can_be_reverted() {
        let mut app = App::new();
        app.patch_mut().sample = "New take.wav".into();
        app.patches[1].params[1] = 777.;
        app.saved[1] = app.patches[1].clone();
        let saved_sample = app.saved[1].sample.clone();
        app.operation("copy_sample");
        app.action(Action::ChooseSlot(1));
        assert_eq!(app.patches[1].sample, "New take.wav");
        assert_eq!(app.patches[1].params[1], 777.);
        app.action(Action::Part(1));
        app.confirm("revert_patch");
        assert_eq!(app.patch().sample, saved_sample);
        assert_eq!(app.patch().params[1], 777.);
    }

    #[test]
    fn saving_another_performance_slot_keeps_the_source() {
        let mut app = App::new();
        app.volumes[3] = 79.;
        app.selected_performance = 1;
        app.operation("save_perf");
        assert!(matches!(app.modal, Some(Modal::Confirm("save_perf_as"))));
        app.action(Action::Confirm);
        assert_eq!(app.performance_slot, 1);
        assert_eq!(app.performance_parts[0].2[3], 80.);
        assert_eq!(app.performance_parts[1].2[3], 79.);
        app.selected_performance = 0;
        app.confirm("load_perf");
        assert_eq!(app.volumes[3], 80.);
    }

    #[test]
    fn recording_stays_bound_to_its_buffer_across_part_focus() {
        let mut app = App::new();
        app.operation("record");
        app.action(Action::Part(1));
        let sampling = app.setting_index("sampling");
        app.set(sampling, 0.);
        assert_eq!(app.recording, Some(0));
        app.action(Action::Part(0));
        app.set(sampling, 0.);
        assert_eq!(app.recording, None);
    }

    #[test]
    fn clock_edit_captures_its_shift_role_without_changing_free_rate() {
        let mut app = App::new();
        let sync = app.setting_index("grain_clock");
        app.set(sync, 1.);
        app.action(Action::Shift);
        app.action(Action::Param(3));
        app.action(Action::Shift);
        app.action(Action::Nudge(1.));
        assert_eq!(app.get("grain_div"), 2.);
        assert_eq!(app.get("grain_mul"), 1.);
        assert_eq!(app.patch().params[3], 24.);
        app.action(Action::UndoLive);
        assert_eq!(app.get("grain_div"), 1.);
    }
}
