//! The main screen of the redesign : the waveform
//! field, the right column of envelopes, LFOs and CV, and the status bar.
//! Device mode draws engine readback only; desktop values are simulated.
use super::*;
use crate::device::display::{lfo_window, LfoBucket, LFO_COLUMNS};

/// Split a circular spray radius without clipping grains at either end of the sample.
pub(super) fn spray_ranges(position: f32, radius: f32) -> Vec<(f32, f32)> {
    if !position.is_finite() || !radius.is_finite() {
        return vec![];
    }
    if radius >= 0.5 {
        return vec![(0., 1.)];
    }
    let pos = position.rem_euclid(1.);
    let radius = radius.max(0.);
    let (lo, hi) = (pos - radius, pos + radius);
    if lo < 0. {
        vec![(0., hi), (lo + 1., 1.)]
    } else if hi > 1. {
        vec![(lo, 1.), (0., hi - 1.)]
    } else {
        vec![(lo, hi)]
    }
}

/// The desktop preview's simulated oscillator; device mode never uses it.
fn demo_lfo_value(shape: usize, phase: f32) -> f32 {
    match shape {
        0 => (phase * std::f32::consts::TAU).cos(),
        1 => ((phase.floor() * 12.9898).sin() * 43758.547).fract(),
        2 => phase.rem_euclid(1.) * 2. - 1.,
        3 => {
            if phase.rem_euclid(1.) < 0.5 {
                1.
            } else {
                -1.
            }
        }
        _ => 0.,
    }
}

/// The desktop preview's simulated grains; device mode draws engine readback.
fn demo_grains(scan: f32, radius: f32) -> impl Iterator<Item = (f32, f32)> {
    (0..32).map(move |i| {
        (
            (scan + (i as f32 * 1.73).sin() * radius).rem_euclid(1.),
            0.5 + (i as f32 * 0.63).sin() * 0.42,
        )
    })
}

/// Seconds as the right column prints them.
fn seconds(value: f32) -> String {
    if value < 1. {
        format!("{value:.2} s")
    } else {
        format!("{value:.1} s")
    }
}

/// A signed percentage with a true minus sign.
fn signed_percent(value: f32) -> String {
    let percent = (value * 100.).round();
    if percent < 0. {
        format!("\u{2212}{:.0}%", -percent)
    } else {
        format!("+{percent:.0}%")
    }
}

const FIELD: Rect = Rect(0, 0, 544, 424);
/// Inner plot boxes of the right column, from the wireframes.
const PLOT_LEFT: f32 = 564.;
const PLOT_RIGHT: f32 = 788.;
const TRACE_RIGHT: f32 = 784.;
/// The engine is read once per frame.
const DISPLAY_HZ: f32 = 60.;
/// At or above this many readings per period the drawn wave is honest.
const WAVE_SAMPLES: f32 = 12.;
/// At or below this many the trace is only the swept range of each period.
const BAND_SAMPLES: f32 = 8.;
/// From this rate each column's swept range is a wash with thin blue edges
/// rather than a 2-pixel blue bar (dark studio, redesign 5.5).
const SWEPT_HZ: f32 = 2.;

/// Seconds of readings each trace column also spans as a min/max band.
/// Zero at 12 or more readings per period (5 Hz at 60 fps); one whole period
/// at 8 or fewer (7.5 Hz); in between the span grows smoothly from the two
/// readings a line segment already joins to a whole period, so the wave
/// thickens into the band instead of switching. Nothing is interpolated.
pub(super) fn lfo_band_seconds(rate: f32) -> f32 {
    if !(rate > 0.) {
        return 0.;
    }
    let per_period = DISPLAY_HZ / rate;
    let readings = if per_period >= WAVE_SAMPLES {
        return 0.;
    } else if per_period <= BAND_SAMPLES {
        per_period
    } else {
        let t = (WAVE_SAMPLES - per_period) / (WAVE_SAMPLES - BAND_SAMPLES);
        2. + t * t * (per_period - 2.)
    };
    readings / DISPLAY_HZ
}

/// Fill below a polyline down to `bottom`, one column at a time.
fn fill_under(p: &mut Painter, points: &[(f32, f32)], bottom: f32, color: Color) {
    for pair in points.windows(2) {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        let (a, b) = (x0.round() as i32, x1.round() as i32);
        for x in a..b.max(a + 1) {
            let t = if b > a {
                (x - a) as f32 / (b - a) as f32
            } else {
                0.
            };
            let y = (y0 + (y1 - y0) * t).round() as i32;
            p.rect(Rect(x, y, 1, bottom.round() as i32 - y), color);
        }
    }
}

impl App {
    /// Parts drawn as rows: every enabled part plus the focused one.
    pub(super) fn main_rows(&self) -> Vec<usize> {
        let several = self.parts_overview || self.enabled.iter().filter(|v| **v).count() > 1;
        if !several {
            return vec![self.part];
        }
        (0..4)
            .filter(|i| self.parts_overview || self.enabled[*i] || *i == self.part)
            .collect()
    }

    pub(super) fn performance_screen_main(&mut self, p: &mut Painter) {
        let touched = self.readout_target();
        let rows = self.main_rows();
        if rows.len() > 1 {
            let n = rows.len() as i32;
            let height = (FIELD.3 - 5 * (n - 1)) / n;
            for (i, part) in rows.iter().enumerate() {
                let r = Rect(0, i as i32 * (height + 5), FIELD.2, height);
                self.part_field(p, *part, r, false, touched);
            }
        } else {
            self.part_field(p, self.part, FIELD, true, touched);
        }
        self.main_column(p, rows.len() > 1, touched);
        self.status_bar(p, touched);
    }

    /// The waveform field of one part: spotlight, columns, loop, playhead,
    /// grains, touched-control marks and the Position marker.
    fn part_field(
        &mut self,
        p: &mut Painter,
        part: usize,
        r: Rect,
        single: bool,
        touched: Option<LiveTarget>,
    ) {
        let slot = self.parts[part];
        let patch = &self.patches[slot];
        let focused = part == self.part;
        let granular = patch.values.get("sample_mode").copied().unwrap_or(0.) == 0.;
        let params = patch.params;
        let (scan, radius) = match &self.device {
            Some(d) => (d.display.scan[part], d.display.spray[part]),
            None => (params[0] / 100., params[2] / 100.),
        };
        let spray = if self.enabled[part] && granular {
            spray_ranges(scan, radius)
        } else {
            vec![]
        };
        let (loop_lo, loop_hi) = (params[6] / 100., params[7] / 100.);
        let x_of = |fraction: f32| r.0 as f32 + fraction * (r.2 - 1) as f32;
        let center = r.1 as f32 + r.3 as f32 / 2.;
        // Columns span 92% of the half-height around the centre line.
        let half = r.3 as f32 * 0.46;
        let y_of = |v: f32| center - (v - 0.5) * 2. * half;
        p.clip(Some(r));
        for (lo, hi) in &spray {
            let x = r.0 + (lo * r.2 as f32) as i32;
            p.rect(
                Rect(x, r.1, ((hi - lo) * r.2 as f32).ceil() as i32, r.3),
                SPRAY_BAND,
            );
        }
        let (wave, spot) = if focused {
            (WAVEFORM, SPOTLIGHT)
        } else {
            (WAVEFORM_DIM, SPOTLIGHT_DIM)
        };
        let tone = |fraction: f32| {
            if fraction < loop_lo || fraction > loop_hi {
                OUTSIDE_LOOP
            } else if spray
                .iter()
                .any(|(lo, hi)| fraction >= *lo && fraction <= *hi)
            {
                spot
            } else {
                wave
            }
        };
        let mut notice = None;
        if let Some(d) = &self.device {
            let w = &d.display.waves[d.display.slots[part]];
            let count = w.columns.len().max(1);
            for (i, (lo, hi)) in w.columns.iter().copied().enumerate() {
                if !lo.is_finite() || !hi.is_finite() {
                    continue;
                }
                let x = r.0 + i as i32 * r.2 / count as i32;
                let fraction = (i as f32 + 0.5) / count as f32;
                // Keep a quiet or asymmetric segment visible as at least one pixel.
                let (top, bottom) = (y_of(hi).round() as i32, y_of(lo).round() as i32);
                p.rect(
                    Rect(x, top.min(bottom), 1, (bottom - top).abs() + 1),
                    tone(fraction),
                );
            }
            if w.frames <= 0 {
                notice = Some("No sample loaded");
            } else if w
                .columns
                .iter()
                .any(|(a, b)| !a.is_finite() || !b.is_finite())
            {
                notice = Some("Waveform data unavailable");
            }
        } else {
            for i in 0..r.2 {
                let fraction = (i as f32 + 0.5) / r.2 as f32;
                let a = (Self::sample_amplitude(&patch.sample, fraction) * half) as i32;
                p.rect(
                    Rect(r.0 + i, center as i32 - a, 1, 2 * a + 1),
                    tone(fraction),
                );
            }
        }
        if single {
            for (value, target) in [(loop_lo, 6), (loop_hi, 7)] {
                let x = x_of(value).round() as i32;
                let lit = touched == Some(LiveTarget::Param(target, false));
                p.rect(Rect(x, r.1, 1, r.3), if lit { AMBER } else { OUTLINE });
                let tick = if target == 6 { x } else { x - 6 };
                for y in [r.1, r.1 + r.3 - 2] {
                    p.rect(Rect(tick, y, 7, 2), if lit { AMBER } else { INK2 });
                }
            }
        }
        if scan.is_finite() {
            let x = x_of(scan.rem_euclid(1.)).round() as i32;
            if focused {
                p.rect(Rect(x, r.1, 2, r.3), PLAYHEAD);
            } else {
                p.rect(Rect(x, r.1, 1, r.3), INK2);
            }
        }
        if focused && granular {
            let size = if single { 7 } else { 6 };
            let square = |p: &mut Painter, gx: f32, gy: f32| {
                let x = x_of(gx).round() as i32 - size / 2;
                let y = y_of(gy).round() as i32 - size / 2;
                p.rect(Rect(x - 1, y - 1, size + 2, size + 2), BACKGROUND);
                p.rect(Rect(x, y, size, size), GRAIN);
            };
            if let Some(d) = &self.device {
                for (x, y) in &d.display.grains {
                    square(p, *x, *y);
                }
            } else if self.playing && self.enabled[part] {
                for (x, y) in demo_grains(scan, radius) {
                    square(p, x, y);
                }
            }
        }
        if focused {
            match touched {
                Some(LiveTarget::Param(1, false)) if single => {
                    let length = self.get("sample_size");
                    if length > 0. && scan.is_finite() {
                        // Keep the minimum two-pixel bracket inside the field
                        // when the playhead rounds to its final column.
                        let x = (x_of(scan.rem_euclid(1.)).round() as i32).min(r.0 + r.2 - 2);
                        let width = ((params[1] / 1000. / length) * r.2 as f32).round() as i32;
                        let width = width.clamp(2, r.0 + r.2 - x);
                        let y = center as i32 - 6;
                        p.rect(Rect(x, y - 1, width, 3), AMBER);
                        p.rect(Rect(x, y - 8, 2, 16), AMBER);
                        p.rect(Rect(x + width - 2, y - 8, 2, 16), AMBER);
                    }
                }
                Some(LiveTarget::Param(2, false)) => {
                    for (lo, hi) in &spray {
                        let x = r.0 + (lo * r.2 as f32) as i32;
                        let w = ((hi - lo) * r.2 as f32).ceil() as i32;
                        p.outline(Rect(x, r.1 + 1, w.max(2), r.3 - 2), 2, AMBER);
                    }
                }
                _ => {}
            }
        }
        if single {
            let lit = touched == Some(LiveTarget::Param(0, false));
            p.triangle_up(
                x_of(params[0] / 100.),
                r.1 + r.3,
                14.,
                10,
                if lit { AMBER } else { BLUE },
            );
        }
        if let (Some(d), Some(recording)) = (&self.device, self.recording) {
            if recording == d.display.slots[part] {
                if let Some(position) = d.display.record_position {
                    let x = x_of(position.clamp(0., 1.)).round() as i32;
                    p.rect(Rect(x, r.1, 2, r.3), RED);
                    let length = self.patches[slot]
                        .values
                        .get("sample_size")
                        .copied()
                        .unwrap_or(0.);
                    let label = format!("Rec {:.1} / {:.1} s", position * length, length);
                    let right = r.0 + if single { r.2 - 12 } else { 474 };
                    let width = p.width_in(Face::Condensed, &label, 16).round() as i32;
                    p.rect(Rect(right - width - 4, r.1 + 4, width + 8, 24), BACKGROUND);
                    p.text_right(Face::Condensed, right, r.1 + 6, 16, &label, RED);
                }
            }
        }
        if let Some(text) = notice {
            p.text_in(
                Face::Condensed,
                r.0 + 16,
                center as i32 - 10,
                20,
                text,
                INK2,
            );
        }
        if !single {
            self.part_header(p, part, r);
        } else if !self.enabled[part] {
            p.text_in(
                Face::Condensed,
                r.0 + 14,
                r.1 + 6,
                16,
                &format!("Part {}  off", part + 1),
                INK3,
            );
        }
        p.clip(None);
        self.hits.push(Hit {
            r,
            a: if focused {
                Action::Param(0)
            } else {
                Action::Part(part)
            },
        });
    }

    /// The focused part as the menus' live strip shows it, `width` columns
    /// wide . Without grain readback (`grains` false) it keeps
    /// the waveform, the loop and the last scan position only.
    pub(super) fn strip_view(&self, width: usize, grains: bool) -> components::StripView {
        let part = self.part;
        let patch = self.patch();
        let params = patch.params;
        let granular = patch.values.get("sample_mode").copied().unwrap_or(0.) == 0.;
        let (scan, radius) = match &self.device {
            Some(d) => (d.display.scan[part], d.display.spray[part]),
            None => (params[0] / 100., params[2] / 100.),
        };
        let columns = match &self.device {
            Some(d) => {
                let w = &d.display.waves[d.display.slots[part]];
                components::strip_columns(&w.columns, width)
            }
            None => (0..width)
                .map(|i| {
                    let a = Self::sample_amplitude(&patch.sample, (i as f32 + 0.5) / width as f32);
                    (0.5 - a / 2., 0.5 + a / 2.)
                })
                .collect(),
        };
        let live = grains && granular && self.enabled[part];
        components::StripView {
            columns,
            loop_range: (params[6] / 100., params[7] / 100.),
            spray: if live {
                spray_ranges(scan, radius)
            } else {
                vec![]
            },
            scan: Some(scan),
            grains: match &self.device {
                _ if !live => vec![],
                Some(d) => d.display.grains.clone(),
                None if self.playing => demo_grains(scan, radius).collect(),
                None => vec![],
            },
            wiper: if self.recording == Some(self.parts[part]) {
                self.record_fraction()
            } else {
                None
            },
        }
    }

    /// A part row's number, preset and level on the background, so they read
    /// over the waveform; the focused row gets a blue bar at the left edge.
    fn part_header(&self, p: &mut Painter, part: usize, r: Rect) {
        let focused = part == self.part;
        let slot = self.parts[part];
        let label = format!("{:02}  {}", slot + 1, self.patches[slot].name);
        let y = r.1 + 6;
        let end = 36 + p.width_in(Face::Condensed, &label, 16).round() as i32;
        let end = end.min(r.0 + 400);
        // In the parts view the unsaved dot belongs to the row, not the status bar.
        let dirty = self.patches[slot].dirty;
        let dot = if dirty { 14 } else { 0 };
        p.rect(Rect(r.0 + 6, r.1 + 4, end - 6 + 8 + dot, 24), BACKGROUND);
        p.text_in(
            Face::Bold,
            r.0 + 14,
            y,
            16,
            &format!("{}", part + 1),
            if focused { BLUE } else { INK2 },
        );
        p.fit_in(
            Face::Condensed,
            r.0 + 36,
            y,
            16,
            end - 36 + 4,
            &label,
            if focused { INK } else { INK2 },
        );
        if dirty {
            p.disc((end + 10) as f32, (r.1 + 16) as f32, 4., AMBER);
        }
        let level = format!(
            "{:.0}%{}",
            self.volumes[part],
            if self.enabled[part] { "" } else { "  off" }
        );
        let width = p.width_in(Face::Condensed, &level, 16).round() as i32;
        p.rect(Rect(r.0 + 526 - width, r.1 + 4, width + 8, 24), BACKGROUND);
        p.text_right(Face::Condensed, r.0 + 530, y, 16, &level, INK2);
        if focused {
            p.rect(Rect(r.0, r.1, 3, r.3), BLUE);
        }
    }

    /// Normalized amplitude envelope stages from native time conversions on the
    /// device, simulated on the desktop.
    fn envelope_fractions(&self) -> [f32; 4] {
        let envelope = if let Some(d) = &self.device {
            d.display.envelope
        } else {
            [
                (self.get("attack") / 10.).sqrt(),
                (self.get("decay") / 10.).sqrt(),
                self.get("sustain") / 100.,
                (self.get("release") / 10.).sqrt(),
            ]
        };
        envelope.map(|v| v.clamp(0., 1.))
    }

    fn main_column(&mut self, p: &mut Painter, several: bool, touched: Option<LiveTarget>) {
        if several || self.part != 0 {
            p.text_right(
                Face::Condensed,
                792,
                2,
                16,
                &format!("Part {}", self.part + 1),
                INK2,
            );
        }
        match self.slider_role() {
            0 => self.amplitude(p, touched),
            role => self.slider_levels(p, role),
        }
        self.grain_window(p, touched);
        for lfo in 0..2 {
            self.lfo_block(p, lfo, touched);
        }
        for cv in 0..2 {
            self.cv_block(p, cv);
        }
    }

    /// The amplitude envelope inside a plot box: a dark fill below 2-pixel
    /// blue strokes, the `lit` stage (0 attack to 3 release) in 4-pixel ink.
    /// Returns the stage corners for breakpoint marks.
    pub(super) fn draw_envelope(
        &self,
        p: &mut Painter,
        (left, right, top, bottom): (f32, f32, f32, f32),
        lit: Option<usize>,
    ) -> [(f32, f32); 3] {
        let [a, d, s, r] = self.envelope_fractions();
        let y = |level: f32| bottom - level * (bottom - top);
        let width = right - left;
        let xa = left + a / 3. * width;
        let xd = left + (a + d) / 3. * width;
        let xr = (right - r / 3. * width).max(xd);
        let fall = |t: f32| ((-4. * t).exp() - (-4f32).exp()) / (1. - (-4f32).exp());
        let curve = |x0: f32, x1: f32, from: f32, to: f32| -> Vec<(f32, f32)> {
            (0..=12)
                .map(|i| {
                    let t = i as f32 / 12.;
                    (x0 + (x1 - x0) * t, y(to + (from - to) * fall(t)))
                })
                .collect()
        };
        let segments = [
            vec![(left, bottom), (xa, top)],
            curve(xa, xd, 1., s),
            vec![(xd, y(s)), (xr, y(s))],
            curve(xr, right, s, 0.),
        ];
        let all: Vec<_> = segments.iter().flatten().copied().collect();
        fill_under(p, &all, bottom, CURVE_FILL);
        for (i, segment) in segments.iter().enumerate() {
            if lit != Some(i) {
                p.stroke(segment, 2, BLUE);
            }
        }
        if let Some(i) = lit {
            p.stroke(&segments[i], 4, INK);
        }
        [(xa, top), (xd, y(s)), (xr, y(s))]
    }

    fn amplitude(&mut self, p: &mut Painter, touched: Option<LiveTarget>) {
        p.text_in(Face::Condensed, 560, 2, 16, "Amplitude", INK2);
        p.rect(Rect(560, 26, 232, 60), WELL);
        let lit = ["attack", "decay", "sustain", "release"]
            .iter()
            .position(|id| touched == Some(LiveTarget::Setting(self.setting_index(id))));
        self.draw_envelope(p, (PLOT_LEFT, PLOT_RIGHT, 30., 84.), lit);
        p.fit_in(
            Face::Condensed,
            560,
            89,
            14,
            232,
            &format!(
                "A {}   D {}   S {:.0}%   R {}",
                seconds(self.get("attack")),
                seconds(self.get("decay")),
                self.get("sustain"),
                seconds(self.get("release"))
            ),
            INK2,
        );
        self.hits.push(Hit {
            r: Rect(552, 0, 248, 108),
            a: Action::Op("envelope"),
        });
    }

    /// The ADSR sliders' alternate roles: voice or part levels, or voice pitches.
    fn slider_levels(&mut self, p: &mut Painter, role: usize) {
        let title = ["Amplitude", "Voice level", "Part level", "Voice pitch"][role.min(3)];
        p.text_in(Face::Condensed, 560, 2, 16, title, INK2);
        p.rect(Rect(560, 26, 232, 60), WELL);
        for i in 0..4 {
            let x = 560 + i as i32 * 58;
            let value = self.target_value(self.slider_target(i));
            if role == 3 {
                let note = Self::note(value as usize);
                let w = p.width_in(Face::Regular, &note, 20).round() as i32;
                p.text_in(Face::Regular, x + 29 - w / 2, 44, 20, &note, INK);
            } else {
                let level = (value / 100.).clamp(0., 1.);
                let h = (level * 52.).round() as i32;
                p.rect(Rect(x + 24, 30, 10, 52), OUTLINE);
                p.rect(Rect(x + 24, 82 - h, 10, h), BLUE);
            }
            p.text_in(Face::Condensed, x + 5, 66, 14, &format!("{}", i + 1), INK2);
            if role != 3 {
                let label = format!("{value:.0}%");
                let w = p.width_in(Face::Condensed, &label, 14).round() as i32;
                p.text_in(Face::Condensed, x + 29 - w / 2, 89, 14, &label, INK2);
            }
            self.hits.push(Hit {
                r: Rect(x, 20, 58, 88),
                a: Action::Slider(i),
            });
        }
    }

    fn grain_window(&mut self, p: &mut Painter, touched: Option<LiveTarget>) {
        p.text_in(Face::Condensed, 560, 110, 16, "Grain window", INK2);
        let length = self.param_value(1, self.patch().params[1], false);
        if touched == Some(LiveTarget::Param(1, false)) {
            p.text_right(Face::Bold, 792, 110, 16, &length, AMBER);
        } else {
            p.text_right(Face::Regular, 792, 110, 16, &length, INK);
        }
        p.rect(Rect(560, 134, 232, 60), WELL);
        let (top, bottom) = (140., 192.);
        let points: Vec<(f32, f32)> = self
            .envelope_curve(true)
            .into_iter()
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .map(|(x, y)| {
                (
                    PLOT_LEFT + x.clamp(0., 1.) * (PLOT_RIGHT - PLOT_LEFT),
                    bottom - y.clamp(0., 1.) * (bottom - top),
                )
            })
            .collect();
        fill_under(p, &points, bottom, CURVE_FILL);
        let lit = ["sides", "tilt", "curve"]
            .iter()
            .any(|id| touched == Some(LiveTarget::Setting(self.setting_index(id))));
        if lit {
            p.stroke(&points, 4, INK);
        } else {
            p.stroke(&points, 2, BLUE);
        }
        p.fit_in(
            Face::Condensed,
            560,
            197,
            14,
            232,
            &format!(
                "Sides {:.0}%   Tilt {:.0}%   Curve {:.0}%",
                self.get("sides"),
                self.get("tilt"),
                self.get("curve")
            ),
            INK2,
        );
        self.hits.push(Hit {
            r: Rect(552, 108, 248, 108),
            a: Action::Op("grain_window"),
        });
    }

    /// One LFO: title, wave glyph, engine-reported rate, the rolling output
    /// over the adaptive window, and its routes.
    fn lfo_block(&mut self, p: &mut Painter, lfo: usize, touched: Option<LiveTarget>) {
        let base = 218 + lfo as i32 * 88;
        p.text_in(
            Face::Condensed,
            560,
            base,
            16,
            &format!("LFO {}", lfo + 1),
            INK,
        );
        let shape = self.get(&format!("lfo{}_wave", lfo + 1)) as usize;
        Self::wave_glyph(p, shape, 606., base as f32 + 10.);
        let rate = self
            .device
            .as_ref()
            .map(|d| d.display.lfo_speeds[lfo])
            .unwrap_or_else(|| self.get(&format!("lfo{}_rate", lfo + 1)));
        let rate_lit = touched
            == Some(LiveTarget::Setting(
                self.setting_index(&format!("lfo{}_rate", lfo + 1)),
            ));
        p.text_right(
            Face::Condensed,
            792,
            base,
            16,
            &format!("{rate:.2} Hz"),
            if rate_lit { AMBER } else { INK2 },
        );
        let well = Rect(560, base + 24, 232, 40);
        self.draw_lfo_trace(p, lfo, rate, well, (TRACE_RIGHT, LFO_COLUMNS as f32));
        let routes = self.route_amounts(lfo);
        p.fit_in(
            Face::Condensed,
            560,
            base + 67,
            14,
            232,
            if routes.is_empty() {
                "No route"
            } else {
                &routes
            },
            if routes.is_empty() { INK3 } else { INK2 },
        );
        self.hits.push(Hit {
            r: Rect(552, base - 2, 248, 88),
            a: Action::Op(if lfo == 0 { "lfo1" } else { "lfo2" }),
        });
    }

    /// The engine output of one LFO in `well` over the adaptive window: a
    /// zero line, the wave or its swept band, a dot at the current value.
    /// `(right, width)` place the newest column and the window's span.
    pub(super) fn draw_lfo_trace(
        &self,
        p: &mut Painter,
        lfo: usize,
        rate: f32,
        well: Rect,
        (right, width): (f32, f32),
    ) {
        p.rect(well, WELL);
        let middle = (well.1 + well.3 / 2) as f32;
        p.rect(Rect(well.0, middle as i32, well.2, 1), RULE);
        let window = lfo_window(rate);
        let (points, step) = self.lfo_points(lfo, rate, window, (right, width));
        let y = |v: f32| middle - v.clamp(-1., 1.) * (well.3 as f32 / 2. - 4.);
        p.clip(Some(well));
        let band = lfo_band_seconds(rate);
        if band > 0. {
            // Each column spans the readings of the last `band` seconds.
            // Dark studio: a wash fill with thin blue edges, never a bright block.
            let span = (band / step).ceil().max(1.) as usize;
            let mut tops = Vec::with_capacity(points.len());
            let mut bottoms = Vec::with_capacity(points.len());
            for (i, (x, ..)) in points.iter().enumerate() {
                let range = &points[i.saturating_sub(span - 1)..=i];
                let lo = range.iter().map(|b| b.1).fold(f32::INFINITY, f32::min);
                let hi = range.iter().map(|b| b.2).fold(f32::NEG_INFINITY, f32::max);
                let (top, bottom) = (y(hi), y(lo));
                let width = points
                    .get(i + 1)
                    .map_or(2, |next| (next.0 - x).ceil().max(1.) as i32);
                p.rect(
                    Rect(
                        x.round() as i32,
                        top.round() as i32,
                        width,
                        (bottom - top).round() as i32 + 1,
                    ),
                    BLUE_WASH,
                );
                tops.push((*x, top));
                bottoms.push((*x, bottom));
            }
            p.stroke(&tops, 1, BLUE);
            p.stroke(&bottoms, 1, BLUE);
        }
        // The wave is drawn up to 5 Hz; from there the band alone shows it,
        // since a line would only zigzag at that width.
        if band == 0. {
            let line: Vec<(f32, f32)> = points
                .iter()
                .map(|(x, _, _, last)| (*x, y(*last)))
                .collect();
            if rate >= SWEPT_HZ {
                // From 2 Hz each column's swept range is drawn as the band
                // is: a wash with its extremes joined as 1-pixel blue edges,
                // so a wide trace never becomes a bright block.
                let mut tops = Vec::with_capacity(points.len());
                let mut bottoms = Vec::with_capacity(points.len());
                for (i, (x, lo, hi, _)) in points.iter().enumerate() {
                    let (top, bottom) = (y(*hi), y(*lo));
                    let width = points
                        .get(i + 1)
                        .map_or(2, |next| (next.0 - x).ceil().max(1.) as i32);
                    p.rect(
                        Rect(
                            x.round() as i32,
                            top.round() as i32,
                            width,
                            (bottom - top).round() as i32 + 1,
                        ),
                        BLUE_WASH,
                    );
                    tops.push((*x, top));
                    bottoms.push((*x, bottom));
                }
                p.stroke(&tops, 1, BLUE);
                p.stroke(&bottoms, 1, BLUE);
            } else if rate < SWEPT_HZ {
                for (x, lo, hi, _) in &points {
                    let (top, bottom) = (y(*hi), y(*lo));
                    if bottom - top > 2. {
                        p.rect(
                            Rect(
                                x.round() as i32,
                                top.round() as i32,
                                2,
                                (bottom - top).round() as i32,
                            ),
                            BLUE,
                        );
                    }
                }
            }
            p.stroke(&line, 2, BLUE);
        }
        p.clip(None);
        if let Some((x, _, _, last)) = points.last() {
            p.disc(*x, y(*last), if well.3 < 30 { 2. } else { 3.5 }, INK);
        }
    }

    /// Trace columns as (x, min, max, last) from the newest at the right edge,
    /// with the duration one column covers. Device columns are engine readback.
    fn lfo_points(
        &self,
        lfo: usize,
        rate: f32,
        window: f64,
        (right, span): (f32, f32),
    ) -> (Vec<(f32, f32, f32, f32)>, f32) {
        let width = LFO_COLUMNS as f32;
        // Pixels per stored column.
        let scale = span / width;
        if let Some(d) = &self.device {
            let trace = &d.display.lfo_traces[lfo];
            let step = trace.bucket_seconds();
            let visible = (window / step).ceil().clamp(1., LFO_COLUMNS as f64);
            let now = trace.index_at(Instant::now().duration_since(d.display.epoch).as_secs_f64());
            let newest = trace.buckets().back().map(|b| b.index).unwrap_or(now);
            let now = now.max(newest);
            let pixels = span / visible as f32;
            let points = trace
                .buckets()
                .iter()
                .filter(|b| ((now - b.index) as f64) < visible)
                .map(
                    |&LfoBucket {
                         index,
                         min,
                         max,
                         last,
                     }| {
                        (right - (now - index) as f32 * pixels, min, max, last)
                    },
                )
                .collect();
            (points, step as f32)
        } else {
            let elapsed = self.demo_elapsed();
            let shape = self.get(&format!("lfo{}_wave", lfo + 1)) as usize;
            let step = window as f32 / width;
            let points = (0..LFO_COLUMNS)
                .rev()
                .map(|k| {
                    // Sampled once per frame like the engine readback.
                    let t = ((elapsed - k as f32 * step) * DISPLAY_HZ).floor() / DISPLAY_HZ;
                    let v = demo_lfo_value(shape, t * rate);
                    (right - k as f32 * scale, v, v, v)
                })
                .collect();
            (points, step)
        }
    }

    /// Seconds since the desktop demo started, for its simulated LFOs.
    fn demo_elapsed(&self) -> f32 {
        #[cfg(test)]
        if let Some(t) = self.demo_clock {
            return t;
        }
        self.demo_started.elapsed().as_secs_f32()
    }

    fn wave_glyph(p: &mut Painter, shape: usize, x: f32, y: f32) {
        let points: Vec<(f32, f32)> = match shape {
            0 => (0..=16)
                .map(|i| {
                    (
                        i as f32,
                        -(i as f32 / 16. * std::f32::consts::TAU).sin() * 5.,
                    )
                })
                .collect(),
            1 => vec![
                (0., 2.),
                (3., 2.),
                (3., -4.),
                (7., -4.),
                (7., 4.),
                (11., 4.),
                (11., -1.),
                (16., -1.),
            ],
            2 => vec![(0., 5.), (8., -5.), (8., 5.), (16., -5.)],
            3 => vec![
                (0., 5.),
                (0., -5.),
                (8., -5.),
                (8., 5.),
                (16., 5.),
                (16., -5.),
            ],
            _ => vec![
                (0., 2.),
                (3., -3.),
                (6., 1.),
                (9., -4.),
                (12., 3.),
                (16., 0.),
            ],
        };
        let points: Vec<_> = points.into_iter().map(|(a, b)| (x + a, y + b)).collect();
        p.stroke(&points, 1, INK2);
    }

    fn cv_block(&mut self, p: &mut Painter, cv: usize) {
        let x = 560 + cv as i32 * 124;
        p.text_in(Face::Condensed, x, 392, 16, &format!("CV {}", cv + 1), INK);
        let routes = self.route_amounts(cv + 2);
        p.fit_in(
            Face::Condensed,
            x + 38,
            394,
            14,
            78,
            if routes.is_empty() { "Off" } else { &routes },
            if routes.is_empty() { INK3 } else { INK2 },
        );
        let amount = self
            .device
            .as_ref()
            .map(|d| d.display.amounts[cv + 2])
            .unwrap_or_else(|| self.get(&format!("cv{}_amount", cv + 1)) / 100.);
        p.rect(Rect(x, 415, 108, 4), OUTLINE);
        p.rect(
            Rect(
                x,
                415,
                (108. * amount.abs().clamp(0., 1.)).round() as i32,
                4,
            ),
            BLUE,
        );
        self.hits.push(Hit {
            r: Rect(x - 6, 388, 124, 36),
            a: Action::Op(if cv == 0 { "cv1" } else { "cv2" }),
        });
    }

    /// A source's routes in the focused patch, as "Spray +40%   Tilt +15%".
    pub(super) fn route_amounts(&self, source: usize) -> String {
        self.patch().routes[source]
            .iter()
            .enumerate()
            .filter(|(_, v)| v.abs() > 0.001)
            .map(|(i, v)| format!("{} {}", catalog::DESTINATIONS[i], signed_percent(*v)))
            .collect::<Vec<_>>()
            .join("   ")
    }

    pub(super) fn envelope_curve(&self, grain: bool) -> Vec<(f32, f32)> {
        if grain {
            return if let Some(d) = &self.device {
                d.display
                    .grain_window
                    .chunks_exact(2)
                    .map(|p| (p[0] / 127., p[1]))
                    .collect()
            } else {
                self.envelope_values(true)
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| (i as f32 / 96., v))
                    .collect()
            };
        }
        let [a, d, s, r] = self.envelope_fractions();
        vec![
            (0., 0.),
            (a / 3., 1.),
            ((a + d) / 3., s),
            (1. - r / 3., s),
            (1., 0.),
        ]
    }
}
