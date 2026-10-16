pub mod abi;
pub mod bindings;
pub mod copy;
pub mod display;
pub mod layout;
pub mod listing;
pub mod native;
pub mod storage;
#[cfg(test)]
mod tests;

use abi::{Ipc, Multi};
use native::{Engine, PanelEvent};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use storage::{Document, Performance, Result};

pub struct Device {
    engine: Engine,
    /// The data layout under the data root, detected once at startup.
    pub layout: layout::Layout,
    pub documents: Vec<Document>,
    pub states: Vec<Ipc>,
    pub dirty: [bool; 32],
    pub system: Document,
    pub performance: Performance,
    pub performance_slot: usize,
    pub performance_paths: Vec<PathBuf>,
    performance_source: PathBuf,
    pub directory: PathBuf,
    pub files: Vec<String>,
    pub clipboard: Vec<PathBuf>,
    pub display: display::Display,
    pub playing: bool,
    pub recording: Option<usize>,
    pub shift: bool,
    /// Shift physically held, as opposed to latched (`gr_get_shift_explicitly_pressed`).
    pub shift_explicit: bool,
    pub vu: f32,
    pub clipped: bool,
    pub dsp: f32,
    pub voice_pitch: [f32; 4],
    pub voice_volume: [f32; 4],
    pub normalize: bool,
    last_slot: usize,
    screen: Option<i32>,
    audio_cards: String,
    last_hotplug: std::time::Instant,
}
impl Device {
    pub fn open(library: &Path, root: &Path) -> Result<Self> {
        let layout = layout::Layout::detect(root);
        let mut system = Document::defaults("system");
        let system_path = layout.system_settings();
        if system_path.exists() {
            system.merge(Document::read(&system_path, "system")?);
        }
        let slot = system.number("current_performance")?.clamp(0., 49.) as usize;
        let performance_paths = layout.slots();
        // Files are parsed and validated before native state or audio is changed.
        let (perf, docs) = read_performance(&layout.slot(slot))?;
        let engine = Engine::open(library)?;
        let mut result = Self {
            engine,
            documents: docs.clone(),
            states: vec![Ipc::default(); 32],
            dirty: [false; 32],
            system,
            performance: perf,
            performance_slot: slot,
            performance_paths,
            performance_source: layout.slot(slot),
            directory: layout.internal(),
            layout,
            files: vec![],
            clipboard: vec![],
            display: display::Display::default(),
            playing: false,
            recording: None,
            shift: false,
            shift_explicit: false,
            vu: 0.,
            clipped: false,
            dsp: 0.,
            voice_pitch: [60.; 4],
            voice_volume: [100.; 4],
            normalize: true,
            last_slot: usize::MAX,
            screen: None,
            audio_cards: fs::read_to_string("/proc/asound/cards").unwrap_or_default(),
            last_hotplug: std::time::Instant::now(),
        };
        result.system.merge(result.performance.document.clone());
        result.send_settings()?;
        result.install_performance(docs)?;
        result.change_directory(result.layout.internal())?;
        Ok(result)
    }
    pub fn start(&mut self) -> Result<()> {
        // SDL/VideoCore must create their threads first, as in the stock GUI.
        // libgr_start_synthesis pins its caller to CPU 0 with SCHED_FIFO;
        // graphics threads created afterwards inherit those constraints.
        self.engine.start()?;
        self.poll()?;
        Ok(())
    }
    pub fn slot(&self) -> usize {
        self.performance.multis[self.performance.current].patchnums[self.part()] as usize
    }
    pub fn part(&self) -> usize {
        self.performance.multis[self.performance.current].stencilnr_in_focus as usize
    }
    fn capture(&mut self) -> Result<usize> {
        let slot = unsafe { (self.engine.api.gr_get_patchnum)() } as usize;
        if slot >= 32 {
            return Err("Native engine returned an invalid preset".into());
        }
        let state = self.engine.snapshot();
        if self.states[slot] != state {
            self.states[slot] = state;
            let before = self.documents[slot].clone();
            self.documents[slot].update(&state);
            if before != self.documents[slot] {
                self.dirty[slot] = true;
            }
        }
        self.performance.current = state.current_multi_preset.clamp(0, 3) as usize;
        self.performance.multis = state.multi_presets;
        self.performance.validate()?;
        Ok(slot)
    }
    pub fn poll(&mut self) -> Result<Vec<PanelEvent>> {
        if self.last_hotplug.elapsed() >= std::time::Duration::from_secs(1) {
            let cards = fs::read_to_string("/proc/asound/cards").map_err(|e| e.to_string())?;
            if cards != self.audio_cards {
                unsafe {
                    (self.engine.api.alsamidi_restart)();
                    (self.engine.api.arecord_connect_sampler)();
                }
                self.audio_cards = cards;
            }
            self.last_hotplug = std::time::Instant::now();
        }
        let synchronized = self.engine.encoder_reference().is_some();
        let events = self.engine.events()?;
        if !synchronized
            && (std::env::var_os("GR1_UI_TRACE").is_some()
                || std::env::var_os("GR1_UI_PANEL_TRACE").is_some())
        {
            if let Some(value) = self.engine.encoder_reference() {
                eprintln!(
                    "[gr1-panel] event=5 ENC_ROT value={value} screen={} skipped=first-report",
                    self.screen.map_or("-".into(), |s| s.to_string())
                );
            }
        }
        let slot = self.capture()?;
        self.playing = self.engine.playing();
        let was_recording = self.recording;
        unsafe {
            self.shift = (self.engine.api.gr_get_shift_enabled)() != 0;
            self.shift_explicit = (self.engine.api.gr_get_shift_explicitly_pressed)() != 0;
            self.recording = if (self.engine.api.gr_is_recording)() != 0 {
                self.recording.or(Some(slot))
            } else {
                None
            };
            self.vu = (self.engine.api.gr_get_vu)();
            self.clipped = self.engine.input_clipped();
            self.dsp = (self.engine.api.get_dsp_usage)();
        }
        let (sliders, levels) = self.engine.voice_controls(self.part());
        for i in 0..4 {
            if let Some(v) = sliders[i] {
                self.voice_pitch[i] = self.states[slot].basenote as f32
                    + v as f32 / 1023. * self.states[slot].slider_semitones as f32;
            }
            if let Some(v) = levels[i] {
                self.voice_volume[i] = v * 100.;
            }
        }
        if self.last_slot == usize::MAX || was_recording.is_some() && self.recording.is_none() {
            self.display.invalidate();
        }
        // UPDATE_PATCH (11) is emitted while turning sound controls. Like
        // stock, refresh parameter readback without rescanning unchanged PCM.
        // Loads/edits invalidate explicitly; recording and length changes are
        // handled separately. Preset recall may restore PCM of the same length.
        if events.iter().any(|e| matches!(e.action, 10 | 27)) {
            self.display.waves[slot].frames = -1;
        }
        self.last_slot = slot;
        self.poll_display();
        Ok(events)
    }
    fn refresh_waveform(&mut self) {
        self.display.invalidate();
        self.poll_display();
    }
    fn editable(&self) -> Result<()> {
        if self.performance.multis[self.performance.current].partenables[self.part()] == 0
            || unsafe { (self.engine.api.gr_get_patchnum)() } != self.slot() as i32
        {
            return Err("Enable this part before editing its sound".into());
        }
        Ok(())
    }
    fn apply(&mut self, p: Ipc) -> Result<()> {
        self.editable()?;
        storage::validate_ipc(&p)?;
        self.engine.apply_patch(&p);
        let slot = self.capture()?;
        self.dirty[slot] = true;
        Ok(())
    }
    pub fn param(&mut self, index: usize, alternate: bool, value: f32) -> Result<()> {
        let mut p = self.engine.snapshot();
        if alternate && matches!(index, 4 | 5) {
            bindings::patch(if index == 4 { "direction" } else { "pan" })
                .unwrap()
                .write(&mut p, value);
        } else {
            bindings::param(&mut p, index, value);
        }
        self.apply(p)
    }
    pub fn setting(&mut self, id: &str, value: f32) -> Result<()> {
        if !value.is_finite() {
            return Err("Setting must be finite".into());
        }
        if let Some((key, scale)) = bindings::system(id) {
            let old = self.system.clone();
            self.system.put(key, value / scale);
            if let Err(e) = self.send_settings() {
                self.system = old;
                return Err(e);
            }
            self.system.save(&self.layout.system_settings(), "system")?;
            self.performance.document.put(key, value / scale);
        } else if id == "sample_size" {
            self.editable()?;
            if self.recording.is_some() {
                return Err("Stop recording before resizing the sample".into());
            }
            if !(0.01..=112.).contains(&value) {
                return Err("Sample size must be 0.01 to 112 seconds".into());
            }
            unsafe {
                (self.engine.api.gr_init_current_sample)(value, 0);
            }
            self.dirty[self.slot()] = true;
            self.refresh_waveform();
        } else if id == "normalize" {
            self.normalize = value != 0.;
        } else if id == "samba" {
            self.samba(value != 0.)?;
            self.system.put("samba", value);
        } else if let Some(rest) = id.strip_prefix("voice") {
            self.editable()?;
            let index = rest
                .chars()
                .next()
                .and_then(|c| c.to_digit(10))
                .filter(|i| (1..=4).contains(i))
                .ok_or("Invalid voice")? as usize
                - 1;
            if id.ends_with("_pitch") {
                let p = self.engine.snapshot();
                let range = p.slider_semitones as f32;
                if range <= 0. || value < p.basenote as f32 || value > p.basenote as f32 + range {
                    return Err(format!(
                        "Voice slider range is {} to {}",
                        p.basenote,
                        p.basenote + p.slider_semitones
                    ));
                }
                unsafe {
                    (self.engine.api.handle_note_slider)(
                        ((value - p.basenote as f32) / range * 1023.).round() as i32,
                        index as i32,
                    );
                }
                self.voice_pitch[index] = value;
            } else {
                unsafe {
                    (self.engine.api.set_voice_vol)(index as i32, value / 100.);
                }
                self.voice_volume[index] = value;
            }
        } else if id == "matrix_mode" {
            self.engine.set_panel_mode(value != 0.);
        } else if let Some(binding) = bindings::patch(id) {
            let mut p = self.engine.snapshot();
            binding.write(&mut p, value);
            self.apply(p)?;
            if id == "sampling" {
                unsafe {
                    (self.engine.api.gr_set_sampling_enabled)(i32::from(value != 0.));
                }
            }
        } else {
            return Err(format!("No native binding for {id}"));
        }
        self.capture()?;
        Ok(())
    }
    pub fn route(&mut self, source: usize, dest: usize, value: f32) -> Result<()> {
        self.editable()?;
        if source >= 9 || dest >= 11 || !value.is_finite() {
            return Err("Invalid modulation route".into());
        }
        unsafe {
            (self.engine.api.gr_set_modmatrix_entry_float)(source as i32, dest as i32, value);
        }
        self.capture()?;
        self.dirty[self.slot()] = true;
        Ok(())
    }
    pub fn scale(&mut self, notes: [bool; 12]) -> Result<()> {
        let mut p = self.engine.snapshot();
        p.western_scale = notes.map(i32::from);
        self.apply(p)
    }
    fn send_settings(&mut self) -> Result<()> {
        let s = &self.system;
        let mut channels = [0; 4];
        for (i, ch) in channels.iter_mut().enumerate() {
            *ch = s.number(&format!("midi_channel{}", i + 1))? as i32;
            if !(0..=16).contains(ch) {
                return Err("MIDI channel must be 0 to 16".into());
            }
        }
        let direct = s.number("direct_shift")? as i32;
        let menu = s.number("menu_slider_enabled")? as i32;
        let invert = s.number("encoder_invert")? as i32;
        let conv = s.number("conv44")? as i32;
        unsafe {
            for (i, ch) in channels.iter().enumerate() {
                (self.engine.api.midi_set_channel)(i as i32, *ch);
            }
            (self.engine.api.set_led_brightness)(
                (s.number("led_brightness")?.clamp(0., 100.) * 2.55) as i32,
            );
            (self.engine.api.phui_set_setpoints)(s.number("setpoints")? as i32);
            (self.engine.api.phui_set_playtogglemode)(s.number("playtogglemode")? as i32);
            (self.engine.api.gr_set_adsr_slider_mode)(s.number("adsr_slider_mode")? as i32);
            (self.engine.api.gr_set_cc2cv)(s.number("cc2cv")? as i32);
            (self.engine.api.gr_set_ccvis)(s.number("cc_vis_updates")? as i32);
            (self.engine.api.gr_set_grainstop)(s.number("grainstop")? as i32);
            (self.engine.api.midi_set_mpe_enabled)(s.number("mpe_enabled")? as i32);
            (self.engine.api.gr_set_pitchbend_range)(s.number("pitchbend_range")? / 12.);
            (self.engine.api.set_precision_cc_enabled)(s.number("precision_cc_enabled")? as i32);
            (self.engine.api.gr_set_monitor_level)(s.number("monitor_level")?);
        }
        self.engine.edit_global(|p| {
            p.direct_shift = direct;
            p.menu_slider_enabled = menu;
            p.encoder_invert = invert;
            p.conv44 = conv;
        });
        Ok(())
    }
    /// The screen id last given to the engine; panel events are emitted under it.
    pub fn screen_id(&self) -> Option<i32> {
        self.screen
    }
    pub fn screen(&mut self, screen: i32) {
        if self.screen != Some(screen) {
            unsafe {
                (self.engine.api.set_screen)(screen);
            }
            if screen != 1 {
                self.stop_preview();
            }
            self.screen = Some(screen);
        }
    }
    pub fn play(&mut self) {
        unsafe {
            (self.engine.api.phui_actuate_button)(14, 1);
            (self.engine.api.phui_actuate_button)(14, 0);
        }
    }
    pub fn shift(&mut self, on: bool) {
        unsafe {
            (self.engine.api.phui_actuate_button)(11, i32::from(on));
        }
    }
    pub fn record(&mut self, on: bool) -> Result<()> {
        if on {
            self.editable()?;
        }
        let mut p = self.engine.snapshot();
        if on {
            if p.startmode == 0 {
                p.startmode = 1;
            }
            p.sampling_enabled = 1;
            self.apply(p)?;
        }
        unsafe {
            (self.engine.api.gr_set_sampling_enabled)(i32::from(on));
            (self.engine.api.gr_enable_rec_to_patch)(i32::from(on));
        }
        self.dirty[self.slot()] = true;
        Ok(())
    }
    pub fn aux(&mut self, on: bool) {
        self.engine.aux_record(on);
    }
    pub fn multi(&mut self, f: impl FnOnce(&mut Multi)) -> Result<()> {
        self.capture()?;
        let current = self.performance.current;
        let before = self.performance.multis[current];
        f(&mut self.performance.multis[current]);
        if let Err(e) = self.performance.validate() {
            self.performance.multis[current] = before;
            return Err(e);
        }
        let multi = self.performance.multis[current];
        self.engine
            .edit_global(|p| p.multi_presets[current] = multi);
        unsafe {
            (self.engine.api.gr_multitimbral_ipc2engine)();
        }
        self.capture()?;
        Ok(())
    }
    pub fn select_multi(&mut self, index: usize) -> Result<()> {
        if index >= 4 {
            return Err("Invalid multi preset".into());
        }
        self.capture()?;
        unsafe {
            (self.engine.api.synth_set_multi_preset)(index as i32);
        }
        self.capture()?;
        Ok(())
    }
    pub fn load_sample(&mut self, path: &Path, slot: usize) -> Result<()> {
        if slot >= 32 || !path.is_file() {
            return Err("Select an audio file".into());
        }
        let c = Engine::string(path)?;
        let name = path.file_name().unwrap().to_string_lossy();
        let preset = path.file_stem().unwrap().to_string_lossy();
        let sample = std::ffi::CString::new(name.as_bytes()).map_err(|e| e.to_string())?;
        let preset = std::ffi::CString::new(preset.as_bytes()).map_err(|e| e.to_string())?;
        if name.len() > 255 || preset.as_bytes().len() > 255 {
            return Err("Sample filename exceeds native limit".into());
        }
        let loaded = unsafe {
            (self.engine.api.gr_load_sample)(c.as_ptr(), slot as i32, i32::from(self.normalize))
        };
        if loaded < 0 {
            return Err(format!("Could not load {}", path.display()));
        }
        unsafe {
            (self.engine.api.gr_set_sample_name)(slot as i32, sample.as_ptr());
            (self.engine.api.gr_set_preset_name)(slot as i32, preset.as_ptr());
            (self.engine.api.gr_backup_patch)(slot as i32);
        }
        self.documents[slot].put("sample", name);
        self.documents[slot].put("name", preset.to_string_lossy());
        self.documents[slot].put("samplesize", loaded as f32 / 44100.);
        self.dirty[slot] = true;
        self.capture()?;
        self.refresh_waveform();
        Ok(())
    }
    pub fn preview(&mut self, path: &Path) -> Result<()> {
        unsafe {
            (self.engine.api.gr_disable_preview)();
        }
        if self.system.number("soundpreviews")? != 0. && path.is_file() {
            let c = Engine::string(path)?;
            let result = unsafe { (self.engine.api.gr_load_temp_sample)(c.as_ptr()) };
            if result < 0 {
                return Err("Could not preview this sample".into());
            }
            unsafe {
                (self.engine.api.gr_enable_preview)();
            }
        }
        Ok(())
    }
    pub fn stop_preview(&mut self) {
        unsafe {
            (self.engine.api.gr_disable_preview)();
        }
    }
    pub fn load_patch(&mut self, path: &Path, slot: usize) -> Result<()> {
        self.editable()?;
        if slot != self.slot() {
            return Err("Select the destination part first".into());
        }
        let path = storage::patch_file(path)?;
        let doc = Document::patch(&path)?;
        let sample = resolve_sample(&path, &doc)?;
        self.install_patch(slot, &doc, &sample)?;
        self.documents[slot] = doc;
        self.dirty[slot] = true;
        self.capture()?;
        self.refresh_waveform();
        Ok(())
    }
    fn install_patch(&mut self, slot: usize, doc: &Document, sample: &Path) -> Result<i32> {
        let p = doc.ipc()?;
        let c = Engine::string(sample)?;
        unsafe {
            (self.engine.api.gr_set_preset)(slot as i32);
        }
        let loaded = unsafe { (self.engine.api.gr_load_sample)(c.as_ptr(), slot as i32, 0) };
        if loaded < 0 {
            return Err(format!("Engine could not load {}", sample.display()));
        }
        // gr_load_sample leaves a reversible edit. Without the stock GUI's
        // sample backup, the next preset selection restores the previous PCM
        // and length (one frame for a slot that has never been loaded).
        unsafe {
            (self.engine.api.gr_duplicate_sample)();
        }
        self.engine.apply_patch(&p);
        self.states[slot] = self.engine.snapshot();
        unsafe {
            (self.engine.api.gr_backup_patch)(slot as i32);
        }
        if self.display.trace {
            eprintln!(
                "[gr1-load] slot={slot} frames={loaded} path={}",
                sample.display()
            );
        }
        Ok(loaded)
    }
    fn install_performance(&mut self, docs: Vec<Document>) -> Result<()> {
        let root = self.performance_source.clone();
        let mut loaded_frames = [0; 32];
        unsafe {
            (self.engine.api.gr_visual_updates_off)();
            (self.engine.api.gr_set_monotimbral)();
            (self.engine.api.gr_mute_sound)();
        }
        let result: Result<()> = (|| {
            for (i, doc) in docs.iter().enumerate() {
                let path = storage::patch_file(&root.join(format!("{i:03}")))?;
                loaded_frames[i] = self.install_patch(i, doc, &resolve_sample(&path, doc)?)?;
            }
            Ok(())
        })();
        // Failed loads remain muted and return the failing file; never claim success.
        result?;
        self.documents = docs;
        let multis = self.performance.multis;
        let current = self.performance.current as i32;
        self.engine.edit_global(|p| {
            p.multi_presets = multis;
            p.current_multi_preset = current;
        });
        unsafe {
            (self.engine.api.gr_multitimbral_ipc2engine)();
        }
        // Check after every preset has been visited and the active multi has
        // been recalled: checking immediately after load misses sample rollback.
        for (slot, expected) in loaded_frames.iter().enumerate() {
            let actual = unsafe { (self.engine.api.gr_get_samplelength)(slot as i32) };
            if actual != *expected {
                return Err(format!(
                    "Preset {} sample changed during recall: {expected} to {actual} frames",
                    slot + 1
                ));
            }
        }
        if self.display.trace {
            eprintln!("[gr1-load] performance retained all 32 sample lengths after recall");
        }
        unsafe {
            (self.engine.api.gr_visual_updates_on)();
            (self.engine.api.gr_unmute_sound)();
        }
        self.capture()?;
        self.dirty = [false; 32];
        self.last_slot = usize::MAX;
        Ok(())
    }
    pub fn load_performance(&mut self, index: usize) -> Result<()> {
        let path = self
            .performance_paths
            .get(index)
            .ok_or("Invalid performance slot")?
            .clone();
        self.load_performance_path(&path)?;
        self.performance_slot = index;
        self.system.put("current_performance", index);
        self.system.save(&self.layout.system_settings(), "system")
    }
    pub fn load_performance_path(&mut self, path: &Path) -> Result<()> {
        let (perf, docs) = read_performance(path)?;
        let path = if path.is_file() {
            path.parent().ok_or("Performance has no folder")?
        } else {
            path
        };
        self.performance = perf;
        self.performance_source = path.into();
        self.system.merge(self.performance.document.clone());
        self.send_settings()?;
        self.install_performance(docs)
    }
    pub fn imported(&self) -> bool {
        self.performance_source != self.performance_paths[self.performance_slot]
    }
    fn capture_all(&mut self) -> Result<()> {
        self.record(false)?;
        self.capture()?;
        let perf = self.performance.clone();
        // Follow the stock save flow: preserve active edits before preset recall,
        // then silence playback while visiting all 32 presets.
        unsafe {
            for (slot, on) in perf.multis[perf.current]
                .patchnums
                .iter()
                .zip(perf.multis[perf.current].partenables)
            {
                if on != 0 {
                    (self.engine.api.gr_backup_patch)(*slot);
                }
            }
            // Match stock save_perf: keep the current sample edit while
            // visiting other presets to collect their state for saving.
            (self.engine.api.gr_clear_sample_dirty)();
            (self.engine.api.gr_visual_updates_off)();
            (self.engine.api.gr_all_notes_off)();
            (self.engine.api.gr_mute_sound)();
            (self.engine.api.gr_set_monotimbral)();
            for i in 0..32 {
                (self.engine.api.gr_set_preset)(i as i32);
                let state = self.engine.snapshot();
                self.states[i] = state;
                self.documents[i].update(&state);
            }
        }
        self.performance = perf;
        self.restore_multi();
        unsafe {
            (self.engine.api.gr_visual_updates_on)();
            (self.engine.api.gr_unmute_sound)();
        }
        Ok(())
    }
    fn restore_multi(&mut self) {
        let multis = self.performance.multis;
        let current = self.performance.current as i32;
        self.engine.edit_global(|p| {
            p.multi_presets = multis;
            p.current_multi_preset = current;
        });
        unsafe {
            (self.engine.api.gr_multitimbral_ipc2engine)();
        }
    }
    pub fn auxiliary_active(&self) -> bool {
        self.engine.aux_active()
    }
    pub fn matrix_mode(&self) -> bool {
        self.engine.panel_mode()
    }
    fn write_patch_dir(&mut self, directory: &Path, slot: usize) -> Result<()> {
        fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let mut doc = self.documents[slot].clone();
        let mut sample = doc.text("sample").to_string();
        if sample.is_empty() {
            sample = "empty.wav".into();
        }
        storage::file_name(&sample)?;
        let sample_path = directory.join(&sample);
        let c = Engine::string(&sample_path)?;
        let ret = unsafe {
            if (self.engine.api.gr_get_samplelength)(slot as i32) > 0 {
                (self.engine.api.gr_save_sample)(c.as_ptr(), slot as i32)
            } else {
                (self.engine.api.gr_save_empty_sample)(c.as_ptr())
            }
        };
        if ret != 0 {
            return Err(format!("Could not save {}", sample_path.display()));
        }
        fs::File::open(&sample_path)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        doc.put("sample", sample);
        doc.put("sample_dirty", "False");
        let name = if doc.text("name").is_empty() {
            "empty"
        } else {
            doc.text("name")
        };
        storage::file_name(name)?;
        doc.save(&directory.join(format!("{name}.grp")), "gr-1 patch")
    }
    pub fn save_patch(&mut self) -> Result<()> {
        self.editable()?;
        self.record(false)?;
        self.capture()?;
        let slot = self.slot();
        if self.performance_source != self.performance_paths[self.performance_slot] {
            return Err(
                "Save this imported performance to a user slot before saving a patch".into(),
            );
        }
        let dest = self.performance_paths[self.performance_slot].join(format!("{slot:03}"));
        let stage = staging(&dest)?;
        let result =
            preserve_extras(&dest, &stage, true).and_then(|_| self.write_patch_dir(&stage, slot));
        if let Err(e) = result {
            let _ = fs::remove_dir_all(&stage);
            return Err(e);
        }
        replace_directory(&stage, &dest)?;
        self.dirty[slot] = false;
        unsafe {
            (self.engine.api.gr_backup_patch)(slot as i32);
            (self.engine.api.gr_clear_sample_dirty)();
        }
        Ok(())
    }
    pub fn save_performance(&mut self, index: usize) -> Result<()> {
        let dest = self
            .performance_paths
            .get(index)
            .ok_or("Invalid performance slot")?
            .clone();
        self.capture_all()?;
        let stage = staging(&dest)?;
        let result: Result<()> = (|| {
            preserve_extras(&dest, &stage, false)?;
            for i in 0..32 {
                let directory = stage.join(format!("{i:03}"));
                fs::create_dir(&directory).map_err(|e| e.to_string())?;
                preserve_extras(&dest.join(format!("{i:03}")), &directory, true)?;
                self.write_patch_dir(&directory, i)?;
            }
            self.performance
                .document()
                .save(&stage.join("performance.grm"), "performance")
        })();
        if let Err(e) = result {
            let _ = fs::remove_dir_all(&stage);
            return Err(e);
        }
        replace_directory(&stage, &dest)?;
        self.performance_slot = index;
        self.performance_source = dest;
        self.dirty = [false; 32];
        self.system.put("current_performance", index);
        self.system.save(&self.layout.system_settings(), "system")
    }
    /// Write the focused preset's sample as a WAV file in `directory`. An
    /// existing file of that name is moved to the folder's trash only when
    /// `replace` is set (the operator confirmed it).
    pub fn save_sample_to(&mut self, directory: &Path, name: &str, replace: bool) -> Result<()> {
        self.reachable(directory)?;
        self.writable(directory)?;
        self.record(false)?;
        storage::save_sample_with(directory, name, replace, |target| {
            let c = Engine::string(target)?;
            if unsafe { (self.engine.api.gr_save_sample)(c.as_ptr(), self.slot() as i32) } != 0 {
                return Err("Sample save failed".into());
            }
            Ok(())
        })
    }
    /// The file the preset's sample was loaded from, in the performance's
    /// preset folder, when it exists.
    pub fn sample_path(&self, slot: usize) -> Option<PathBuf> {
        let doc = self.documents.get(slot)?;
        let patch =
            storage::patch_file(&self.performance_source.join(format!("{slot:03}"))).ok()?;
        resolve_sample(&patch, doc).ok()
    }
    pub fn rename_patch(&mut self, name: &str) -> Result<()> {
        storage::file_name(name)?;
        let mut p = self.engine.snapshot();
        storage::set_text(&mut p.name, name)?;
        self.apply(p)
    }
    pub fn copy(&mut self, kind: &str, destination: usize) -> Result<()> {
        // The focused preset as the engine reports it now.
        self.capture()?;
        let source = self.slot();
        self.copy_from(kind, source, destination)
    }
    /// Copy `kind` from preset `source` into `destination`, as stock's
    /// copy and paste between presets does (manual p.49).
    pub fn copy_from(&mut self, kind: &str, source: usize, destination: usize) -> Result<()> {
        self.editable()?;
        if destination >= 32 || source >= 32 {
            return Err("Invalid preset slot".into());
        }
        self.capture()?;
        if destination == source {
            return Ok(());
        }
        let perf = self.performance.clone();
        let sample = if kind == "template_all" {
            self.documents[destination].text("sample").to_string()
        } else {
            self.documents[source].text("sample").to_string()
        };
        let c = std::ffi::CString::new(sample.as_bytes()).map_err(|e| e.to_string())?;
        unsafe {
            for (slot, on) in perf.multis[perf.current]
                .patchnums
                .iter()
                .zip(perf.multis[perf.current].partenables)
            {
                if on != 0 {
                    (self.engine.api.gr_backup_patch)(*slot);
                }
            }
            (self.engine.api.gr_all_notes_off)();
            (self.engine.api.gr_mute_sound)();
            (self.engine.api.gr_set_monotimbral)();
            (self.engine.api.gr_set_preset)(destination as i32);
            match kind {
                "copy_sample" => {
                    (self.engine.api.gr_copy_sample)(destination as i32, source as i32)
                }
                "template_all" => {
                    (self.engine.api.gr_copy_template)(destination as i32, source as i32)
                }
                _ => (self.engine.api.gr_copy_patch)(destination as i32, source as i32),
            }
            (self.engine.api.gr_set_sample_name)(destination as i32, c.as_ptr());
            (self.engine.api.gr_backup_patch)(destination as i32);
        }
        self.states[destination] = self.engine.snapshot();
        self.documents[destination].update(&self.states[destination]);
        self.documents[destination].put(
            "samplesize",
            unsafe { (self.engine.api.gr_get_samplelength)(destination as i32) } as f32 / 44100.,
        );
        self.performance = perf;
        self.restore_multi();
        unsafe {
            (self.engine.api.gr_unmute_sound)();
        }
        self.dirty[destination] = true;
        self.last_slot = usize::MAX;
        Ok(())
    }
    pub fn clear(&mut self, sample_only: bool) -> Result<()> {
        self.editable()?;
        self.record(false)?;
        unsafe {
            (self.engine.api.gr_init_current_sample)(1., 1);
        }
        if !sample_only {
            self.apply(Document::defaults("patch").ipc()?)?;
        }
        self.refresh_waveform();
        self.dirty[self.slot()] = true;
        Ok(())
    }
    pub fn revert(&mut self) -> Result<()> {
        let slot = self.slot();
        let dir = self.performance_source.join(format!("{slot:03}"));
        self.load_patch(&dir, slot)?;
        self.dirty[slot] = false;
        Ok(())
    }
    pub fn initialize_performance(&mut self) -> Result<()> {
        unsafe {
            (self.engine.api.gr_all_notes_off)();
            (self.engine.api.gr_mute_sound)();
            (self.engine.api.gr_set_monotimbral)();
        }
        let defaults = Document::defaults("patch").ipc()?;
        for i in 0..32 {
            unsafe {
                (self.engine.api.gr_set_preset)(i);
                (self.engine.api.gr_init_current_sample)(1., 1);
            }
            self.engine.apply_patch(&defaults);
            self.states[i as usize] = self.engine.snapshot();
            self.documents[i as usize].update(&self.states[i as usize]);
            self.documents[i as usize].put("samplesize", 1.);
            unsafe {
                (self.engine.api.gr_duplicate_sample)();
                (self.engine.api.gr_backup_patch)(i);
            }
        }
        self.performance = Performance::default();
        self.restore_multi();
        unsafe {
            (self.engine.api.gr_unmute_sound)();
        }
        self.dirty = [true; 32];
        self.last_slot = usize::MAX;
        Ok(())
    }
    fn reachable(&self, path: &Path) -> Result<()> {
        let actual = path.canonicalize().map_err(|e| e.to_string())?;
        let root = self
            .layout
            .root()
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let usb = self.location(1).ok().and_then(|p| p.canonicalize().ok());
        if !actual.starts_with(root) && !usb.is_some_and(|p| actual.starts_with(p)) {
            return Err("Choose a folder on Internal, Factory or the mounted USB drive".into());
        }
        Ok(())
    }
    pub fn start_listing(&self, path: PathBuf) -> listing::ListingJob {
        let mut roots = vec![self.layout.root().to_path_buf()];
        if let Ok(usb) = self.location(1) {
            roots.push(usb);
        }
        listing::ListingJob::start(path, roots)
    }
    /// Adopt a completed, location-checked read on the UI thread.
    pub fn accept_listing(&mut self, path: PathBuf, names: Vec<String>) {
        self.directory = path;
        self.files = names;
        self.stop_preview();
    }
    pub fn change_directory(&mut self, path: PathBuf) -> Result<()> {
        self.reachable(&path)?;
        let files = list_files(&path)?;
        self.directory = path;
        self.files = files;
        self.stop_preview();
        Ok(())
    }
    pub fn location(&self, index: usize) -> Result<PathBuf> {
        match index {
            0 => Ok(self.layout.internal()),
            2 => Ok(self.layout.factory()),
            1 => {
                let mounts = fs::read_to_string("/proc/mounts").map_err(|e| e.to_string())?;
                mounts
                    .lines()
                    .filter_map(|l| l.split_whitespace().nth(1))
                    .find(|p| p.starts_with("/media/") && *p != "/media/internal")
                    .map(PathBuf::from)
                    .ok_or_else(|| "No mounted USB drive".into())
            }
            _ => Err("Unknown location".into()),
        }
    }
    /// Rename a file or folder in place.
    pub fn rename_path(&mut self, source: &Path, name: &str) -> Result<()> {
        let folder = source.parent().ok_or("Invalid file")?;
        self.reachable(folder)?;
        self.writable(folder)?;
        storage::file_name(name)?;
        let dest = folder.join(name);
        if dest.exists() {
            return Err("Name already exists".into());
        }
        fs::rename(source, dest).map_err(|e| e.to_string())
    }
    pub fn mkdir(&mut self, name: &str) -> Result<()> {
        self.writable_directory()?;
        storage::file_name(name)?;
        fs::create_dir(self.directory.join(name)).map_err(|e| e.to_string())
    }
    /// Move a file or folder into its folder's recoverable trash.
    pub fn trash_path(&mut self, path: &Path) -> Result<()> {
        let folder = path.parent().ok_or("Invalid file")?;
        self.reachable(folder)?;
        self.writable(folder)?;
        storage::trash_path(path)
    }
    fn writable_directory(&self) -> Result<()> {
        self.writable(&self.directory)
    }
    fn writable(&self, directory: &Path) -> Result<()> {
        let current = directory.canonicalize().map_err(|e| e.to_string())?;
        let factory = self
            .layout
            .factory()
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if current.starts_with(factory) {
            return Err("Factory files are read-only. Choose a user or USB folder.".into());
        }
        Ok(())
    }
    /// Rename the saved performance in a user slot on disk; the working
    /// performance is unchanged.
    pub fn rename_slot(&mut self, index: usize, name: &str) -> Result<()> {
        let file = self
            .performance_paths
            .get(index)
            .ok_or("Invalid performance slot")?
            .join("performance.grm");
        let name = name.trim();
        if name.is_empty() || name.contains(['\n', '\r']) {
            return Err("Invalid performance name".into());
        }
        let mut doc = Document::read(&file, "performance")?;
        doc.put("name", name);
        doc.save(&file, "performance")
    }
    /// Write an initialized performance (32 empty presets) into a user slot
    /// on disk, replacing what it held; the working performance is unchanged.
    pub fn initialize_slot(&mut self, index: usize) -> Result<()> {
        let dest = self
            .performance_paths
            .get(index)
            .ok_or("Invalid performance slot")?
            .clone();
        let stage = staging(&dest)?;
        let result: Result<()> = (|| {
            preserve_extras(&dest, &stage, false)?;
            let mut doc = Document::defaults("patch");
            doc.put("sample", "empty.wav");
            doc.put("sample_dirty", "False");
            let name = match doc.text("name") {
                "" => "empty".to_string(),
                name => name.to_string(),
            };
            storage::file_name(&name)?;
            for i in 0..32 {
                let directory = stage.join(format!("{i:03}"));
                fs::create_dir(&directory).map_err(|e| e.to_string())?;
                let sample = directory.join("empty.wav");
                let c = Engine::string(&sample)?;
                if unsafe { (self.engine.api.gr_save_empty_sample)(c.as_ptr()) } != 0 {
                    return Err(format!("Could not save {}", sample.display()));
                }
                fs::File::open(&sample)
                    .and_then(|f| f.sync_all())
                    .map_err(|e| e.to_string())?;
                doc.save(&directory.join(format!("{name}.grp")), "gr-1 patch")?;
            }
            Performance::default()
                .document()
                .save(&stage.join("performance.grm"), "performance")
        })();
        if let Err(e) = result {
            let _ = fs::remove_dir_all(&stage);
            return Err(e);
        }
        replace_directory(&stage, &dest)
    }
    /// Plan pasting the clipboard into the Files directory, a slice per frame.
    pub fn start_paste(&self) -> Result<copy::CopyJob> {
        self.reachable(&self.directory)?;
        self.writable_directory()?;
        copy::CopyJob::new(&self.clipboard, &self.directory)
    }
    /// Free bytes on the filesystem holding the Files directory.
    pub fn free_bytes(&self) -> Result<u64> {
        let path = std::ffi::CString::new(self.directory.as_os_str().as_encoded_bytes())
            .map_err(|e| e.to_string())?;
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(stat.f_bavail as u64 * stat.f_frsize as u64)
    }
    pub fn unmount(&mut self) -> Result<()> {
        let target = self.location(1)?;
        self.stop_preview();
        command("umount", &[target.to_str().ok_or("Invalid USB path")?])
    }
    pub fn reset_settings(&mut self) -> Result<()> {
        self.system = Document::defaults("system");
        self.send_settings()?;
        self.system.save(&self.layout.system_settings(), "system")
    }
    pub fn shutdown(&mut self) -> Result<()> {
        unsafe {
            (self.engine.api.gr_mute_sound)();
        }
        command("systemctl", &["poweroff"])
    }
    fn samba(&mut self, on: bool) -> Result<()> {
        if on {
            command("mount", &["-o", "rw,remount,noatime", "/"])?;
        }
        command(
            "systemctl",
            &[
                if on { "start" } else { "stop" },
                "smbd.service",
                "nmbd.service",
            ],
        )
    }
}
pub fn read_performance(path: &Path) -> Result<(Performance, Vec<Document>)> {
    let file = if path.is_file() {
        path.into()
    } else {
        path.join("performance.grm")
    };
    let perf = Performance::read(&file)?;
    let path = file.parent().ok_or("Performance has no folder")?;
    let mut docs = vec![];
    for i in 0..32 {
        let p = storage::patch_file(&path.join(format!("{i:03}")))?;
        let doc = Document::patch(&p)?;
        resolve_sample(&p, &doc)?;
        docs.push(doc);
    }
    Ok((perf, docs))
}
fn resolve_sample(patch: &Path, doc: &Document) -> Result<PathBuf> {
    let sample = Path::new(doc.text("sample"));
    let path = if sample.is_absolute() {
        sample.into()
    } else {
        patch.parent().unwrap().join(sample)
    };
    if !path.is_file() {
        return Err(format!("Missing sample {}", path.display()));
    }
    Ok(path)
}
fn list_files(path: &Path) -> Result<Vec<String>> {
    let mut files = vec![];
    for e in fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))? {
        let e = e.map_err(|e| e.to_string())?;
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        files.push(if e.path().is_dir() {
            format!("{name}/")
        } else {
            name
        });
    }
    files.sort_by_key(|s| (!s.ends_with('/'), s.to_lowercase()));
    Ok(files)
}
fn staging(target: &Path) -> Result<PathBuf> {
    let path = target.with_file_name(format!(
        ".{}.gr1-ui-stage",
        target.file_name().unwrap().to_string_lossy()
    ));
    fs::create_dir(&path).map_err(|e| format!("Cannot stage save at {}: {e}", path.display()))?;
    Ok(path)
}
// Preserve files that the synthesizer does not own when replacing a slot.
fn preserve_extras(source: &Path, target: &Path, patch: bool) -> Result<()> {
    if !source.exists() {
        return Ok(());
    }
    let old_sample = if patch {
        storage::patch_file(source)
            .and_then(|p| Document::patch(&p))
            .ok()
            .map(|d| d.text("sample").to_string())
    } else {
        None
    };
    for e in fs::read_dir(source).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let name = e.file_name();
        let name = name.to_string_lossy();
        let owned = if patch {
            name.ends_with(".grp") || old_sample.as_deref() == Some(name.as_ref())
        } else {
            name == "performance.grm"
                || (name.len() == 3 && name.parse::<usize>().is_ok_and(|i| i < 32))
        };
        if !owned {
            copy_tree(&e.path(), &target.join(name.as_ref()))?;
        }
    }
    Ok(())
}
fn replace_directory(stage: &Path, target: &Path) -> Result<()> {
    let backup = target.with_file_name(format!(
        ".{}.gr1-ui-previous",
        target.file_name().unwrap().to_string_lossy()
    ));
    if backup.exists() {
        fs::remove_dir_all(&backup).map_err(|e| e.to_string())?;
    }
    let existed = target.exists();
    if existed {
        fs::rename(target, &backup).map_err(|e| e.to_string())?;
    }
    if let Err(e) = fs::rename(stage, target) {
        if existed {
            let _ = fs::rename(&backup, target);
        }
        return Err(e.to_string());
    }
    fs::File::open(target.parent().unwrap())
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    if source
        .symlink_metadata()
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Copy the original file instead of a symbolic link".into());
    }
    if source.is_dir() {
        let source = source.canonicalize().map_err(|e| e.to_string())?;
        let parent = target
            .parent()
            .unwrap()
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if parent.starts_with(&source) {
            return Err("Cannot copy a folder into itself".into());
        }
        fs::create_dir(target).map_err(|e| e.to_string())?;
        for e in fs::read_dir(source).map_err(|e| e.to_string())? {
            let e = e.map_err(|e| e.to_string())?;
            copy_tree(&e.path(), &target.join(e.file_name()))?;
        }
    } else {
        fs::copy(source, target).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn command(name: &str, args: &[&str]) -> Result<()> {
    let result = Command::new(name)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if result.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{name}: {}",
            String::from_utf8_lossy(&result.stderr)
        ))
    }
}
