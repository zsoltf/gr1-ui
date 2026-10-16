//! Connect the existing views to the instrument controller. No demo mutation runs
//! for an instrument action: errors stay visible and the next frame uses readback.
use super::*;
use crate::device::{
    self,
    storage::{self, Result},
    Device,
};

impl App {
    pub(super) fn sync_device_view(&mut self) {
        let Some(d) = self.device.as_ref() else {
            return;
        };
        for i in 0..32 {
            let state = &d.states[i];
            let doc = &d.documents[i];
            if self.device_states[i].as_ref() == Some(state)
                && self.patches[i].name == doc.text("name")
                && self.patches[i].sample == doc.text("sample")
                && self.patches[i].dirty == d.dirty[i]
                && self.patches[i].values.get("sample_size").copied()
                    == doc.number("samplesize").ok()
                && self.patches[i].values.get("normalize").copied() == Some(f32::from(d.normalize))
            {
                continue;
            }
            self.device_states[i] = Some(*state);
            let mut values = HashMap::new();
            for s in &self.catalog {
                if let Some(v) = device::bindings::patch(&s.id).and_then(|b| b.read(state)) {
                    values.insert(s.id.clone(), v);
                }
            }
            values.insert("sample_size".into(), doc.number("samplesize").unwrap_or(0.));
            values.insert("normalize".into(), f32::from(d.normalize));
            for n in 0..12 {
                values.insert(format!("scale{n}"), state.western_scale[n] as f32);
            }
            for n in 0..4 {
                values.insert(format!("voice{}_pitch", n + 1), d.voice_pitch[n]);
                values.insert(format!("voice{}_volume", n + 1), d.voice_volume[n]);
            }
            for id in ["direction", "pan"] {
                values.insert(
                    id.into(),
                    device::bindings::patch(id).unwrap().read(state).unwrap(),
                );
            }
            self.patches[i] = Patch {
                name: doc.text("name").into(),
                sample: doc.text("sample").into(),
                params: device::bindings::params(state),
                values,
                routes: std::array::from_fn(|r| {
                    std::array::from_fn(|c| state.mod_matrix[r * 11 + c])
                }),
                dirty: d.dirty[i],
            };
        }
        for s in &self.catalog {
            if let Some((key, scale)) = device::bindings::system(&s.id) {
                self.system.insert(
                    s.id.clone(),
                    d.system.number(key).unwrap_or(s.default / scale) * scale,
                );
            }
        }
        self.system
            .insert("matrix_mode".into(), f32::from(d.matrix_mode()));
        self.system
            .insert("samba".into(), d.system.number("samba").unwrap_or(0.));
        self.snapshot = d.performance.current;
        self.snapshots = std::array::from_fn(|i| {
            let m = &d.performance.multis[i];
            (
                m.patchnums.map(|i| i.max(0) as usize),
                m.partenables.map(|i| i != 0),
                m.part_volumes.map(|v| v * 100.),
            )
        });
        (self.parts, self.enabled, self.volumes) = self.snapshots[self.snapshot];
        self.part = d.part();
        let focused = d.slot();
        for n in 0..4 {
            self.patches[focused]
                .values
                .insert(format!("voice{}_pitch", n + 1), d.voice_pitch[n]);
            self.patches[focused]
                .values
                .insert(format!("voice{}_volume", n + 1), d.voice_volume[n]);
        }
        self.performance = d.performance.document.text("name").into();
        self.performance_slot = d.performance_slot;
        self.playing = d.playing;
        self.shift = d.shift;
        self.recording = d.recording;
        self.aux_recording = d.auxiliary_active();
        self.directory = d.directory.display().to_string();
        self.files = d.files.clone();
        self.clipboard = d
            .clipboard
            .iter()
            .map(|path| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        if self.metadata_sync.elapsed() >= Duration::from_secs(1) {
            self.usb_mounted = d.location(1).is_ok();
            self.metadata_sync = Instant::now();
        }
    }
    pub(super) fn refresh_performance_list(&mut self) {
        if let Some(d) = &self.device {
            self.performance_library = d
                .performance_paths
                .iter()
                .enumerate()
                .map(|(i, path)| {
                    let name = storage::Performance::read(&path.join("performance.grm"))
                        .map(|p| p.document.text("name").to_string())
                        .unwrap_or_else(|_| format!("Empty {:02}", i + 1));
                    (name, vec![])
                })
                .collect();
        }
    }
    pub(super) fn device_result<T>(&mut self, result: Result<T>) -> Option<T> {
        match result {
            Ok(v) => {
                self.sync_device_view();
                Some(v)
            }
            Err(e) => {
                eprintln!("GR-1: {e}");
                self.notify(&e);
                None
            }
        }
    }
    pub(super) fn device_set(&mut self, index: usize, value: f32) -> bool {
        if let Some(d) = &mut self.device {
            let result = d.setting(&self.catalog[index].id, value);
            self.device_result(result);
            true
        } else {
            false
        }
    }
    pub(super) fn device_live(&mut self, target: LiveTarget, value: f32) -> bool {
        let Some(d) = &mut self.device else {
            return false;
        };
        let result = match target {
            LiveTarget::Param(i, alt) => d.param(i, alt, value),
            LiveTarget::Setting(i) => d.setting(&self.catalog[i].id, value),
            LiveTarget::Route(r, c) => d.route(r, c, value),
            LiveTarget::PartVolume(i) => d.multi(|m| m.part_volumes[i] = value / 100.),
        };
        self.device_result(result);
        true
    }
    pub(super) fn device_action(&mut self, a: &Action) -> bool {
        let Some(mut d) = self.device.take() else {
            return false;
        };
        let mut consumed = true;
        let result: Result<()> = (|| {
            match a {
                Action::Part(i) | Action::EditVolume(i) => {
                    d.multi(|m| m.stencilnr_in_focus = *i as i32)?;
                    self.live = None;
                    if matches!(a, Action::EditVolume(_)) {
                        consumed = false;
                    }
                }
                Action::Play => d.play(),
                Action::Shift => {
                    self.latched = !self.shift;
                    d.shift(self.latched);
                }
                Action::Save => {
                    self.live = None;
                    d.save_patch()?;
                    self.notify("Patch saved");
                }
                Action::Preset(i) => {
                    let current = &d.performance.multis[d.performance.current];
                    let other = current.patchnums.iter().position(|n| *n == *i as i32);
                    d.multi(|m| {
                        if let Some(part) = other {
                            m.stencilnr_in_focus = part as i32
                        } else {
                            m.patchnums[m.stencilnr_in_focus as usize] = *i as i32
                        }
                    })?;
                    self.live = None;
                }
                Action::PartEnable(i) => {
                    d.multi(|m| m.partenables[*i] = i32::from(m.partenables[*i] == 0))?
                }
                Action::Snapshot(i) => {
                    d.select_multi(*i)?;
                    self.live = None;
                }
                Action::ChooseSlot(i) => {
                    if let Some(Modal::Slots { operation, .. }) = self.modal.take() {
                        if operation == "assign_part" {
                            d.multi(|m| m.patchnums[m.stencilnr_in_focus as usize] = *i as i32)?;
                        } else if operation == "load_patch" {
                            let path =
                                d.performance_paths[d.performance_slot].join(format!("{i:03}"));
                            d.load_patch(&path, d.slot())?;
                        } else {
                            d.copy(operation, *i)?;
                        }
                    }
                }
                _ => consumed = false,
            }
            Ok(())
        })();
        self.device = Some(d);
        self.device_result(result);
        consumed
    }
    pub(super) fn device_operation(&mut self, op: &'static str) -> bool {
        let Some(mut d) = self.device.take() else {
            return false;
        };
        let mut consumed = true;
        let result: Result<()> = (|| {
            match op {
                "record" => d.record(self.recording.is_none())?,
                "stop_record" => d.record(false)?,
                "aux" => d.aux(!d.auxiliary_active()),
                "save_perf" => {
                    if self.selected_performance != d.performance_slot || d.imported() {
                        self.modal = Some(Modal::Confirm("save_perf_as"));
                    } else {
                        d.save_performance(d.performance_slot)?;
                        self.notify("Performance saved");
                    }
                }
                "update" => self.notify(
                    "Firmware 3.4.4 target. Use the prepared stock update/recovery procedure.",
                ),
                _ => consumed = false,
            }
            Ok(())
        })();
        self.device = Some(d);
        self.device_result(result);
        if op == "save_perf" {
            self.refresh_performance_list();
        }
        consumed
    }
    /// The device operation behind a confirmed verb; None without a device.
    pub(super) fn run_device_op(&mut self, op: &str) -> Option<Result<()>> {
        let mut d = self.device.take()?;
        let slot = self.selected_performance;
        let result: Result<()> = (|| {
            match op {
                "clear_sample" => d.clear(true)?,
                "clear_patch" => d.clear(false)?,
                "revert_patch" => d.revert()?,
                "init_perf" => d.initialize_performance()?,
                "save_perf_as" => d.save_performance(slot)?,
                "load_perf" => d.load_performance(slot)?,
                "copy_all" | "template_all" => {
                    let source = d.slot();
                    for i in 0..32 {
                        if i != source {
                            d.copy(op, i)?;
                        }
                    }
                }
                "factory" => d.reset_settings()?,
                "shutdown" => d.shutdown()?,
                "unmount" => d.unmount()?,
                _ => return Err(format!("Unknown operation: {op}")),
            }
            Ok(())
        })();
        self.device = Some(d);
        if op == "unmount" {
            let d = self.device.as_ref().unwrap();
            let internal = d.layout.internal();
            let job = result.is_ok().then(|| d.start_listing(internal.clone()));
            return Some(self.complete_unmount(
                result,
                internal.to_string_lossy().into_owned(),
                job,
            ));
        }
        Some(result)
    }
    pub(super) fn device_confirm(&mut self, op: &str) -> bool {
        let Some(result) = self.run_device_op(op) else {
            return false;
        };
        if result.is_ok() {
            match op {
                "save_perf_as" => self.notify("Performance saved"),
                "load_perf" => self.notify("Performance loaded"),
                _ => {}
            }
        }
        self.device_result(result);
        self.refresh_performance_list();
        true
    }
    /// The engine screen id for the current interface state .
    pub(super) fn screen_id(&self) -> i32 {
        if let Some(screen) = self.overlay_screen_id() {
            screen
        } else if self.perf_active() {
            perf_root::SCREEN_ID
        } else if self.panel_sidebar && self.menu_root == 1 {
            3
        } else {
            match self.tab {
                0 => 0,
                1 | 2 => 4,
                3 => 3,
                4 | 5 => 5,
                _ => 0,
            }
        }
    }
    pub(super) fn device_context(&mut self) {
        let screen = self.screen_id();
        self.screen_sent = Some(screen);
        if let Some(d) = &mut self.device {
            d.screen(screen);
        }
    }
    pub(super) fn poll_device(&mut self, painter: &mut Painter) {
        let Some(d) = &mut self.device else { return };
        let started = Instant::now();
        let events = d.poll();
        self.native_poll_time = started.elapsed();
        let sync_started = Instant::now();
        let Some(events) = self.device_result(events) else {
            return;
        };
        if events
            .iter()
            .any(|e| matches!(e.action, 1 | 2 | 5 | 7..=16 | 20..=22 | 24))
        {
            self.input_received = Some(started);
        }
        self.dispatch_panel_events(painter, events);
        self.run_due_preview();
        self.device_context();
        self.view_sync_time = sync_started.elapsed();
    }
    pub(super) fn dispatch_panel_events(
        &mut self,
        painter: &mut Painter,
        events: Vec<device::native::PanelEvent>,
    ) {
        // Every event in one poll was emitted under the screen set before it.
        let arrival = self.arrival_screen();
        if !events.is_empty() {
            self.panel_batch += 1;
        }
        // Holding Shift selects the focused Files row .
        self.perf_shift_edge();
        for event in events {
            if self.navigation_dirty || self.rendered_context != self.focus_context() {
                self.render(painter);
            }
            let before = self.panel_trace.then(|| self.trace_state());
            self.handle_panel_event(&event);
            if let Some(before) = before {
                self.trace_panel_event(&event, arrival, &before);
            }
        }
        self.device_context();
    }
    fn arrival_screen(&self) -> Option<i32> {
        match &self.device {
            Some(d) => d.screen_id(),
            None => self.screen_sent,
        }
    }
    fn trace_state(&self) -> String {
        format!(
            "ctx={} focus={} live={}",
            self.focus_context(),
            // The Patch root names its focus in the context.
            self.focus_target
                .as_ref()
                .filter(|_| !self.in_patch_root())
                .map_or("-".into(), |a| format!("{a:?}").replace(' ', "")),
            u8::from(self.live.is_some())
        )
    }
    /// One line per received panel event (GR1_UI_PANEL_TRACE=1): its id, name
    /// and value, the screen id under which it arrived, the engine's Shift
    /// flags at that poll, and the interface state before and after it.
    fn trace_panel_event(
        &mut self,
        event: &device::native::PanelEvent,
        arrival: Option<i32>,
        before: &str,
    ) {
        self.panel_seq += 1;
        // WIN_NOTIFY, UPDATE_PATCH, UPDATE_MOD and CONTROL_NAME stream at
        // frame rate or faster: print the first of each in a run, then their
        // counts when another event arrives.
        if STREAMING.contains(&event.action) {
            match self.trace_run.iter_mut().find(|(a, _)| *a == event.action) {
                Some((_, count)) => {
                    *count += 1;
                    return;
                }
                None => self.trace_run.push((event.action, 0)),
            }
        } else {
            self.flush_trace_run();
        }
        let (shift, explicit) = match &self.device {
            Some(d) => (
                u8::from(d.shift).to_string(),
                u8::from(d.shift_explicit).to_string(),
            ),
            None => (u8::from(self.shift).to_string(), "-".into()),
        };
        let mut line = format!(
            "[gr1-panel] t={:.3} batch={} seq={} event={} {} value={}",
            self.demo_started.elapsed().as_secs_f64(),
            self.panel_batch,
            self.panel_seq,
            event.action,
            event_name(event.action),
            event.value
        );
        if event.action == 5 {
            line += &format!(" delta={}", encoder_delta(event.value));
        }
        if let Some(text) = &event.text {
            line += &format!(" text={text:?}");
        }
        line += &format!(
            " screen={} shift={shift} explicit={explicit} | {before} -> {} next_screen={}",
            arrival.map_or("-".into(), |s| s.to_string()),
            self.trace_state(),
            self.screen_id()
        );
        self.emit_trace(line);
    }
    fn flush_trace_run(&mut self) {
        let repeats: Vec<String> = self
            .trace_run
            .drain(..)
            .filter(|(_, count)| *count > 0)
            .map(|(action, count)| format!("{action} {} x{count}", event_name(action)))
            .collect();
        if !repeats.is_empty() {
            let line = format!(
                "[gr1-panel] t={:.3} repeated {}",
                self.demo_started.elapsed().as_secs_f64(),
                repeats.join(", ")
            );
            self.emit_trace(line);
        }
    }
    /// Keyboard input from SDL, which does not pass through the panel stream.
    pub(super) fn trace_sdl(&mut self, what: &str) {
        if self.panel_trace {
            let line = format!(
                "[gr1-panel] t={:.3} sdl {what} screen={} | {}",
                self.demo_started.elapsed().as_secs_f64(),
                self.arrival_screen().map_or("-".into(), |s| s.to_string()),
                self.trace_state()
            );
            self.emit_trace(line);
        }
    }
    fn emit_trace(&mut self, line: String) {
        eprintln!("{line}");
        #[cfg(test)]
        self.panel_trace_lines.push(line);
    }
    fn handle_panel_event(&mut self, event: &device::native::PanelEvent) {
        if self.files_ui.listing.is_some() {
            match event.action {
                14 => {
                    self.cancel_listing();
                    return;
                }
                6 | 10 | 12 | 13 | 15 | 33..=35 => {
                    self.cancel_listing();
                }
                1..=5 | 7 | 19 | 22 | 28 | 29 => return,
                _ => {}
            }
        }
        let had_overlay = self.overlay_active();
        if !had_overlay && self.perf_event(event) {
            return;
        }
        let sample_root = self.sample_root_active() && !self.overlay_active();
        self.sample_pre_event(event);
        // A panel verb or another root while a value is grabbed first keeps
        // the value, then acts .
        if self.in_patch_root() && matches!(event.action, 1..=4 | 12 | 15 | 18 | 19 | 29 | 32) {
            self.patch_keep();
        }
        let covered = self.overlay_active() || self.modal.is_some();
        if self.overlay_event(event) {
            return;
        }
        if event.action == 13 && (covered || self.tab != 1) {
            // Shift+Patch: the Patch root at its last page and field.
            self.open_patch_root();
            return;
        }
        if sample_root && self.sample_event(event) {
            return;
        }
        if self.in_patch_root() && !matches!(event.action, 8..=11 | 16 | 20 | 21 | 23 | 25..=27) {
            // An error notice stays until the next input.
            self.clear_error();
        }
        if self.in_patch_root() && self.patch_event(event) {
            return;
        }
        match event.action {
            // A preset button inside a menu returns to the main screen, as
            // stock does; the engine selects the preset.
            10 if self.tab != 0 || self.modal.is_some() => self.action(Action::Nav(0)),
            19 => self.action(Action::Save),
            // A sample armed by Copy to preset pastes into the focused preset.
            3 | 4 if self.tab == 0 && self.modal.is_none() && self.copy_arm.is_some() => {
                self.paste_armed_sample()
            }
            // With Perf lit, Copy on the main screen copies the preset to all
            // (manual p.73); Shift+Copy copies its settings (the template).
            3 | 4 if self.tab == 0 && self.files_ui.lit == overlay::DataKind::Perf => {
                let source = self.parts[self.part];
                let all = !self.shift;
                self.perf_ask_copy_all(source, all);
            }
            3 | 4 => self.action(Action::Op("copy_patch")),
            5 => self.panel_turn(encoder_delta(event.value)),
            6 => self.action(Action::Nav(0)),
            7 => self.panel_press(),
            12 => self.open_perf_root(!had_overlay),
            13 => self.open_panel_menu(0),
            14 => self.back(),
            15 => self.open_sample_root(false),
            17 => {}
            18 => self.action(Action::Op("update")),
            21 | 23 => {
                // The engine counts the C string's terminating NUL in the length.
                if let Some(text) = event.text.as_deref().map(|t| t.trim_end_matches('\0')) {
                    if event.action == 23 {
                        // The input device the engine names, for the Record page.
                        self.sample_ui.input = Some(text.to_string());
                    }
                    // A control the engine names becomes the readout, not a notice.
                    match (event.action == 21)
                        .then(|| self.engine_control(text))
                        .flatten()
                    {
                        Some(target) => self.touched(target),
                        None => self.notify(text),
                    }
                }
            }
            22 => self.panel_slider(event.value),
            28 => self.action(Action::Op("unmount")),
            29 => self.action(Action::Op(match event.value {
                0 => "clear_sample",
                2 => "init_perf",
                _ => "clear_patch",
            })),
            30 => self.notify("Preset is already assigned to another part"),
            32 => self.action(Action::Op("factory")),
            _ => {}
        }
    }
}

/// Events the trace prints once per run (see trace_panel_event).
const STREAMING: [u8; 4] = [9, 11, 16, 21];

/// The engine's signed encoder decoding: values above 128 are negative.
pub(super) fn encoder_delta(value: u8) -> i32 {
    if value > 128 {
        value as i32 - 256
    } else {
        value as i32
    }
}

/// GUI event names from the pinned firmware.
pub(super) fn event_name(action: u8) -> &'static str {
    match action {
        0 => "NONE",
        1 => "LOAD",
        2 => "SAVE",
        3 => "PASTE",
        4 => "COPY",
        5 => "ENC_ROT",
        6 => "MAIN",
        7 => "ENC_PUSH",
        8 => "ADSR_NOTIFY",
        9 => "WIN_NOTIFY",
        10 => "SET_PRESET",
        11 => "UPDATE_PATCH",
        12 => "PERF_MENU",
        13 => "PATCH_MENU",
        14 => "ESC",
        15 => "SAMPLE_MENU",
        16 => "UPDATE_MOD",
        17 => "REC",
        18 => "UPDATE_FW",
        19 => "QUICKSAVE",
        20 => "UPDATE_PARAM",
        21 => "CONTROL_NAME",
        22 => "SLIDER_WIDGET",
        23 => "CARD_NAME",
        24 => "NOTEON",
        25 => "SET_MULTIDISPLAY",
        26 => "SET_PARTFOCUS",
        27 => "SOFTSET_PRESET",
        28 => "UNMOUNT",
        29 => "KILL",
        30 => "DUPLICATE",
        31 => "SHIFT",
        32 => "FACTORY_RESET",
        33 => "SAMPLE_BTN",
        34 => "PATCH_BTN",
        35 => "PERF_BTN",
        36 => "SCREENSHOT",
        _ => "UNKNOWN",
    }
}

pub(super) fn command_line(args: &[String]) -> Result<Option<Device>> {
    let value = |flag: &str| -> Result<Option<&str>> {
        args.iter()
            .position(|a| a == flag)
            .map(|i| {
                args.get(i + 1)
                    .map(String::as_str)
                    .ok_or_else(|| format!("{flag} needs a path"))
            })
            .transpose()
    };
    let library = value("--engine")?.map(std::path::PathBuf::from).unwrap_or(
        std::env::current_exe()
            .map_err(|e| e.to_string())?
            .parent()
            .unwrap()
            .join("lib/libgr.so.1.0.1"),
    );
    if args.iter().any(|a| a == "--check-device") {
        println!("{}", device::native::preflight(&library)?);
        std::process::exit(0);
    }
    if let Some(path) = value("--check-files")? {
        let counts = storage::check_tree(
            Path::new(path),
            value("--export-checked-files")?.map(Path::new),
        )?;
        println!(
            "Read and round-tripped {} patches and {} performances; no engine loaded",
            counts.0, counts.1
        );
        std::process::exit(0);
    }
    if args.iter().any(|a| a == "--device") {
        if args.iter().any(|a| a == "--captures") {
            return Err("Capture mode is desktop-only".into());
        }
        Device::open(
            &library,
            Path::new(value("--data-root")?.unwrap_or("/media/internal")),
        )
        .map(Some)
    } else {
        Ok(None)
    }
}
