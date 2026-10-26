//! The main screen's status bar  and the
//! readout of the last moved control.
use super::*;

/// How long the readout and the touched-control mark stay after the last change.
pub(super) const TOUCH_HOLD: Duration = Duration::from_millis(1500);
/// The panel's legends for the eight sound controls.
const PANEL_NAMES: [&str; 8] = [
    "Position",
    "Grain Size",
    "Spray",
    "Density",
    "Scan",
    "Pan Spray",
    "Loop Start",
    "Loop End",
];
/// Readback that only an operator changes: the patch's base Position and
/// Grain Size. The pinned engine writes these fields only from the panel pot
/// and encoder handlers, MIDI CC and the host's own writes; modulation writes
/// per-voice tables instead. The engine names no control for these two.
const BASE_TARGETS: [LiveTarget; 2] = [LiveTarget::Param(0, false), LiveTarget::Param(1, false)];
/// gr_patch2ipc copies these base controls, not the modulated GrainWin or
/// Matrix outputs. The panel changes them without a control-name event.
const BASE_SETTINGS: [&str; 7] = [
    "sides",
    "tilt",
    "curve",
    "lfo1_rate",
    "lfo2_rate",
    "lfo1_wave",
    "lfo2_wave",
];
const BASE_COUNT: usize = BASE_TARGETS.len() + BASE_SETTINGS.len();
/// The live strip in a menu's status bar .
pub(super) const STRIP: Rect = Rect(244, 430, 300, 44);

/// Which control the operator moved last. It is interface state: it never
/// writes the engine or a file.
#[derive(Default)]
pub(super) struct Touch {
    key: Option<(usize, usize, String, String)>,
    values: [f32; BASE_COUNT],
    lfo_destinations: [Option<usize>; 2],
    lfo_amounts: [f32; 2],
    pub last: Option<(LiveTarget, Instant)>,
}

fn differs(a: f32, b: f32) -> bool {
    if a.is_nan() || b.is_nan() {
        return a.is_nan() != b.is_nan();
    }
    (a - b).abs() > 1e-6 * a.abs().max(b.abs()).max(1.)
}

impl App {
    pub(super) fn toast_visible(&self) -> bool {
        !self.toast.is_empty() && Instant::now() < self.toast_until
    }

    /// Map the engine's control-name text (event 21) to the control it names.
    /// Texts the table does not name exactly, such as clock ratios, tuning,
    /// "---" for a disabled voice or part, or LFO recording, map to nothing.
    pub(super) fn engine_control(&self, text: &str) -> Option<LiveTarget> {
        let setting = |id: &str| Some(LiveTarget::Setting(self.setting_index(id)));
        let number = |rest: &str| {
            rest.trim()
                .trim_start_matches('[')
                .split([' ', ']'])
                .next()
                .and_then(|n| n.parse::<usize>().ok())
        };
        if let Some((name, _)) = text.split_once(':') {
            let target = match name {
                "Density" => Some(LiveTarget::Param(3, false)),
                "Spray" => Some(LiveTarget::Param(2, false)),
                "Pan spray" => Some(LiveTarget::Param(5, false)),
                "Panning" => Some(LiveTarget::Param(5, true)),
                "Scan" => Some(LiveTarget::Param(4, false)),
                "Fwdrev bias" => Some(LiveTarget::Param(4, true)),
                "Attack" => setting("attack"),
                "Decay" => setting("decay"),
                "Sustain" => setting("sustain"),
                "Release" => setting("release"),
                "Cutoff" => setting("cutoff"),
                "Resonance" => setting("resonance"),
                "Patch vol" => setting("patch_volume"),
                "Tape slew" => setting("tape_slew"),
                "Delay feedback" => setting("delay_feedback"),
                "Distortion" => setting("distortion"),
                "Reducer bits" => setting("bit_reduction"),
                "Reducer rate" => setting("rate_reduction"),
                "Reverb amount" => setting("reverb_amount"),
                _ => None,
            };
            if target.is_some() {
                return target;
            }
            // "Voice %d: %s" names that voice's slider pitch (1-based).
            if let Some(n) = name.strip_prefix("Voice ").and_then(number) {
                return (1..=4)
                    .contains(&n)
                    .then(|| setting(&format!("voice{n}_pitch")))
                    .flatten();
            }
            return None;
        }
        let has_number = |rest: &str| rest.trim().parse::<f32>().is_ok();
        if let Some(rest) = text.strip_prefix("Loop start ") {
            return has_number(rest).then_some(LiveTarget::Param(6, false));
        }
        if let Some(rest) = text.strip_prefix("Loop end ") {
            return has_number(rest).then_some(LiveTarget::Param(7, false));
        }
        // "Voice %d volume %1.2f" and "Part [%d] volume [%3.1f] dB", 1-based.
        if let Some(rest) = text.strip_prefix("Voice ") {
            let n = number(rest).filter(|n| (1..=4).contains(n))?;
            return rest
                .contains(" volume ")
                .then(|| setting(&format!("voice{n}_volume")))
                .flatten()
                .filter(|_| !rest.ends_with("---"));
        }
        if let Some(rest) = text.strip_prefix("Part [") {
            let n = number(rest).filter(|n| (1..=4).contains(n))?;
            return rest
                .contains("] volume [")
                .then_some(LiveTarget::PartVolume(n - 1));
        }
        None
    }

    /// The operator moved `target` now.
    pub(super) fn touched(&mut self, target: LiveTarget) {
        self.touch.last = Some((target, Instant::now()));
    }

    /// Follow unnamed base controls, DEST and the selected LFO amounts.
    /// A preset, part, performance or sample change re-bases without a touch.
    /// Native recall events also reset tracking, even for the same preset.
    pub(super) fn track_touch(&mut self) {
        let (slot, patch) = (self.parts[self.part], self.patch());
        let same = self.touch.key.as_ref().is_some_and(|(s, p, name, sample)| {
            (*s, *p) == (self.snapshot, slot) && *name == patch.name && *sample == patch.sample
        });
        let targets: [LiveTarget; BASE_COUNT] = std::array::from_fn(|i| {
            if i < BASE_TARGETS.len() {
                BASE_TARGETS[i]
            } else {
                LiveTarget::Setting(self.setting_index(BASE_SETTINGS[i - BASE_TARGETS.len()]))
            }
        });
        let values = targets.map(|t| self.target_value(t));
        let destinations = patch.lfo_destinations;
        let amounts =
            std::array::from_fn(|lfo| destinations[lfo].map_or(0., |dest| patch.routes[lfo][dest]));
        if same {
            let changed: Vec<LiveTarget> = targets
                .iter()
                .zip(values.iter().zip(&self.touch.values))
                .filter(|(_, (a, b))| differs(**a, **b))
                .map(|(t, _)| *t)
                .collect();
            if let [target] = changed[..] {
                self.touched(target);
            }
            for (lfo, destination) in destinations.iter().copied().enumerate() {
                if let Some(dest) = destination {
                    if self.touch.lfo_destinations[lfo].is_some()
                        && (self.touch.lfo_destinations[lfo] != destination
                            || differs(self.touch.lfo_amounts[lfo], amounts[lfo]))
                    {
                        self.touched(LiveTarget::Route(lfo, dest));
                    }
                }
            }
        } else {
            self.touch.last = None;
            self.touch.key = Some((
                self.snapshot,
                slot,
                self.patch().name.clone(),
                self.patch().sample.clone(),
            ));
        }
        self.touch.values = values;
        self.touch.lfo_destinations = destinations;
        self.touch.lfo_amounts = amounts;
    }

    /// The control the readout and the waveform mark answer: an edit in
    /// progress, else the last change within the hold time.
    pub(super) fn readout_target(&self) -> Option<LiveTarget> {
        if let Some(edit) = self.live {
            return Some(edit.target);
        }
        self.touch
            .last
            .filter(|(_, at)| at.elapsed() < TOUCH_HOLD)
            .map(|(target, _)| target)
    }

    fn readout_text(&self, target: LiveTarget) -> (String, String) {
        let value = self.target_value(target);
        match target {
            LiveTarget::Param(i, alt) => (
                if alt && i == 4 {
                    "Direction"
                } else if alt && i == 5 {
                    "Pan"
                } else {
                    PANEL_NAMES[i]
                }
                .into(),
                self.param_value(i, value, alt),
            ),
            LiveTarget::Setting(i) => (
                match self.catalog[i].id.as_str() {
                    "sides" => "Window sides".into(),
                    "tilt" => "Window tilt".into(),
                    "curve" => "Window curve".into(),
                    "lfo1_rate" => "LFO 1 frequency".into(),
                    "lfo2_rate" => "LFO 2 frequency".into(),
                    "lfo1_wave" => "LFO 1 shape".into(),
                    "lfo2_wave" => "LFO 2 shape".into(),
                    _ => self.catalog[i].name.clone(),
                },
                Self::val_text(&self.catalog[i], value),
            ),
            LiveTarget::Route(r, c) => (
                format!("{} > {}", catalog::SOURCES[r], catalog::DESTINATIONS[c]),
                format!("{:+.0}%", value * 100.),
            ),
            LiveTarget::PartVolume(i) => (format!("Part {} level", i + 1), format!("{value:.0}%")),
        }
    }

    fn readout_shape(&self, target: LiveTarget) -> Option<usize> {
        match target {
            LiveTarget::Setting(i)
                if matches!(self.catalog[i].id.as_str(), "lfo1_wave" | "lfo2_wave") =>
            {
                Some(self.target_value(target) as usize)
            }
            _ => None,
        }
    }

    /// The selected shape's icon, not a synthesized replacement for its live trace.
    fn shape_readout(&self, p: &mut Painter, target: LiveTarget, r: Rect, size: u16) {
        let shape = self.readout_shape(target).unwrap();
        let (_, value) = self.readout_text(target);
        let glyph_width = (r.2 / 3).clamp(40, 120);
        p.fit_in(
            Face::Bold,
            r.0,
            r.1,
            size,
            r.2 - glyph_width - 16,
            &value,
            AMBER,
        );
        Self::wave_glyph(
            p,
            shape,
            Rect(r.0 + r.2 - glyph_width, r.1 + 2, glyph_width, 24),
            2,
            AMBER,
        );
    }

    /// The knob position as a fraction of its travel.
    fn readout_fraction(&self, target: LiveTarget) -> f32 {
        let value = self.target_value(target);
        let (min, max) = match target {
            LiveTarget::Param(i, alt) => {
                let (min, max, _) = self.param_range(i, alt);
                (min, max)
            }
            LiveTarget::Setting(i) => match &self.catalog[i].kind {
                Kind::Choice(options) => (0., options.len().saturating_sub(1).max(1) as f32),
                Kind::Toggle => (0., 1.),
                Kind::Number { min, max, .. } => (*min, *max),
            },
            LiveTarget::Route(..) => (-1., 1.),
            LiveTarget::PartVolume(_) => (0., 100.),
        };
        let fraction = if max <= min {
            0.
        } else if min > 0. && max / min >= 1000. {
            (value / min).ln() / (max / min).ln()
        } else {
            (value - min) / (max - min)
        };
        fraction.clamp(0., 1.)
    }

    pub(super) fn status_bar(&mut self, p: &mut Painter, touched: Option<LiveTarget>) {
        self.status_bar_in(p, touched, false);
    }

    /// The status bar of a menu root : identity at the left,
    /// the live strip in the middle, the readout of a moved knob, notices and
    /// chips at the right. `grabbed` lets the strip freeze when that fallback
    /// is chosen.
    pub(super) fn menu_status_bar(&mut self, p: &mut Painter, grabbed: bool) {
        // In menus the readout answers knobs only; a grabbed field already
        // shows its value in its row.
        let grab = self.live.map(|l| l.target);
        let touched = self
            .touch
            .last
            .filter(|(target, at)| at.elapsed() < TOUCH_HOLD && Some(*target) != grab)
            .map(|(target, _)| target);
        let middle_taken = self.status_bar_in(p, touched, true);
        if self.strip_policy.shown && !middle_taken {
            self.menu_strip(p, grabbed);
        }
    }

    /// Draw the live strip, or put back its last pixels when the rate
    /// fallback says this frame keeps them. The drawing time goes to the
    /// readback trace.
    fn menu_strip(&mut self, p: &mut Painter, grabbed: bool) {
        use super::patch_root::StripRate;
        let policy = self.strip_policy;
        let key = (
            p.theme,
            p.brightness,
            self.part,
            self.parts[self.part],
            policy.grains,
        );
        let keep = match (&self.strip_cache, policy.rate) {
            (Some((k, _, _)), _) if *k != key => false,
            (Some(_), StripRate::Every) | (None, _) => false,
            (Some((_, _, at)), StripRate::Half) => at.elapsed() < Duration::from_millis(30),
            (Some(_), StripRate::FreezeWhileGrabbed) => grabbed,
        };
        if keep {
            if let Some((_, pixels, _)) = &self.strip_cache {
                p.restore(STRIP, pixels);
            }
            return;
        }
        let started = Instant::now();
        let view = self.strip_view(STRIP.2 as usize, policy.grains);
        super::components::draw_strip(p, STRIP, &view);
        self.strip_drawn = Some((started.elapsed(), policy.mode()));
        if policy.rate != StripRate::Every {
            self.strip_cache = Some((key, p.save(STRIP), Instant::now()));
        }
    }

    /// Returns true when, in a menu, a readout or notice took the strip's
    /// place because the chips left too little room at the right.
    fn status_bar_in(&mut self, p: &mut Painter, touched: Option<LiveTarget>, menu: bool) -> bool {
        p.rect(Rect(0, 424, 800, 56), BACKGROUND);
        p.rect(Rect(0, 424, 800, 1), RULE);
        if self.get("dsp") != 0. {
            // Simulated on the desktop, which has no engine to meter.
            let load = self.device.as_ref().map(|d| d.dsp).unwrap_or(0.43);
            let color = if load > 0.9 {
                RED
            } else if load > 0.8 {
                AMBER
            } else {
                BLUE
            };
            p.rect(
                Rect(0, 424, (800. * load.clamp(0., 1.)).round() as i32, 4),
                color,
            );
        }
        // Identity: preset, patch name, unsaved dot; sample name and length.
        // With several parts the focused row already names the preset and
        // carries the unsaved dot, so only the sample remains here.
        let several = self.main_rows().len() > 1;
        let patch = self.patch();
        let (name, sample, dirty) = (patch.name.clone(), patch.sample.clone(), patch.dirty);
        if !several {
            let end = p.text_in(
                Face::Bold,
                16,
                428,
                22,
                &format!("{:02}", self.parts[self.part] + 1),
                BLUE,
            );
            let name_x = 52.max(end + 10);
            let name_end = p.fit_in(
                Face::Regular,
                name_x,
                428,
                22,
                // The name may use the gap before the readout at 296, or
                // before the live strip in menus.
                if menu { 224 } else { 272 } - name_x,
                &name,
                INK,
            );
            if dirty {
                p.disc((name_end + 10) as f32, 443., 4., AMBER);
            }
        }
        let sample_y = if several { 442 } else { 456 };
        let length = self.get("sample_size");
        let length_x = if length > 0. {
            p.text_right(
                Face::Condensed,
                236,
                sample_y,
                16,
                &format!("{length:.2} s"),
                INK2,
            )
        } else {
            236
        };
        p.fit_in(
            Face::Condensed,
            16,
            sample_y,
            16,
            length_x - 24,
            &sample,
            INK2,
        );
        if !self.panel_mode {
            self.hits.push(Hit {
                r: Rect(0, 424, 280, 56),
                a: Action::Name,
            });
        }
        // Readout of the touched control.
        if let (Some(target), false) = (touched, menu) {
            let (name, value) = self.readout_text(target);
            p.fit_in(Face::Condensed, 296, 430, 16, 284, &name, INK2);
            if self.readout_shape(target).is_some() {
                self.shape_readout(p, target, Rect(296, 446, 284, 28), 28);
            } else {
                let end = p.fit_in(Face::Bold, 296, 446, 28, 284, &value, AMBER);
                let bar = 430.max(end + 14);
                if 580 - bar >= 40 {
                    p.rect(Rect(bar, 462, 580 - bar, 4), OUTLINE);
                    let filled =
                        ((580 - bar) as f32 * self.readout_fraction(target)).round() as i32;
                    p.rect(Rect(bar, 462, filled, 4), AMBER);
                }
            }
        }
        // Chips, right to left. Activity marks wait for phui_get_activity.
        let mut right = 784;
        let mut chip = |p: &mut Painter, label: &str, color: Color| {
            let width = p.width_in(Face::Bold, label, 14).round() as i32 + 16;
            let r = Rect(right - width, 440, width, 22);
            p.round_rect(r, 4, color);
            p.round_rect(Rect(r.0 + 2, r.1 + 2, r.2 - 4, r.3 - 4), 3, BACKGROUND);
            p.text_in(Face::Bold, r.0 + 8, r.1 + 2, 14, label, color);
            right = r.0 - 8;
        };
        if self.device.is_none() {
            chip(p, "Demo", INK3);
        }
        if self.usb_mounted {
            chip(p, "USB", INK2);
        }
        if self.shift {
            chip(p, "Shift", BLUE);
        }
        if self.copy_arm.is_some() {
            chip(p, "Copy", BLUE);
        }
        if self.recording.is_some() {
            chip(p, "Rec", RED);
        }
        if menu {
            // Menus: the readout of a moved knob, else a notice, between the
            // strip and the chips; when the chips leave too little room, in
            // the strip's place while it shows.
            let mut left = STRIP.0 + STRIP.2 + 12;
            let mut width = right - left - 4;
            let shows = touched.is_some() || self.toast_visible();
            let shape = touched
                .and_then(|target| self.readout_shape(target))
                .is_some();
            let middle = shows && width < if shape { 200 } else { 140 };
            if middle {
                (left, width) = (STRIP.0, STRIP.2);
            }
            if let Some(target) = touched {
                let (name, value) = self.readout_text(target);
                p.fit_in(Face::Condensed, left, 428, 16, width, &name, INK2);
                if shape {
                    self.shape_readout(p, target, Rect(left, 448, width, 26), 22);
                } else {
                    p.fit_in(Face::Bold, left, 448, 22, width, &value, AMBER);
                }
            } else if self.toast_visible() {
                for (i, line) in super::components::wrap(p, &self.toast, width)
                    .iter()
                    .take(2)
                    .enumerate()
                {
                    p.text_in(
                        Face::Condensed,
                        left,
                        432 + i as i32 * 20,
                        16,
                        line,
                        if self.toast_error { RED } else { INK },
                    );
                }
            }
            return middle;
        }
        if self.toast_visible() {
            let left = if touched.is_some() { 596 } else { 296 };
            if right - left > 40 {
                p.fit_in(
                    Face::Condensed,
                    left,
                    438,
                    20,
                    right - left - 4,
                    &self.toast,
                    if self.toast_error { RED } else { INK },
                );
            }
        }
        false
    }
}
