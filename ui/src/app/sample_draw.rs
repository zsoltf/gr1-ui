//! The Sample root's pages at the coordinates of wireframe 12 (Record) and
//! the 4.7 sketch (Sample), in section 5's tokens: the page strip, the
//! recording buffer with its wiper, the input meter, the fields and actions;
//! the page strip and the status bar with the live strip are the shared ones.
use super::components;
use super::sample_root::{label, Field, Page, METER_FLOOR};
use super::*;

/// The recording buffer (item 2) and the meter's well (item 3).
const BUFFER: Rect = Rect(16, 56, 768, 104);
/// The meter stops short of the Clip mark so the loudest levels stay visible.
const METER: Rect = Rect(16, 172, 712, 20);
const CLIP: Rect = Rect(740, 172, 44, 20);
/// The Sample page's waveform strip.
const STRIP: Rect = Rect(16, 56, 768, 80);
const ROW: i32 = 44;

/// Where a field sits on its page; pointer hits use the same rectangles.
pub(super) fn field_rect(page: Page, field: Field) -> Rect {
    let row = |column: i32, i: i32, top: i32| {
        Rect(if column == 0 { 16 } else { 408 }, top + i * ROW, 376, ROW)
    };
    match (page, field) {
        (Page::Record, Field::Record) => Rect(16, 224, 200, 48),
        (Page::Record, Field::Clear) => Rect(232, 224, 180, 48),
        (Page::Record, Field::Save) => Rect(428, 224, 220, 48),
        (Page::Sample, Field::Save) => Rect(16, 356, 220, 48),
        (Page::Sample, Field::Copy) => Rect(252, 356, 248, 48),
        (Page::Sample, Field::Clear) => Rect(516, 356, 180, 48),
        (Page::Record, Field::Setting(id)) => match id {
            "rec_level" => row(0, 0, 286),
            "monitor" => row(0, 1, 286),
            "sample_size" => row(0, 2, 286),
            "start_mode" => row(1, 0, 286),
            "loop_rec" => row(1, 1, 286),
            _ => row(1, 2, 286),
        },
        (Page::Sample, Field::Setting(id)) => match id {
            "pitch" => row(0, 0, 206),
            "tuning" => row(0, 1, 206),
            "normalize" => row(1, 0, 206),
            "convert" => row(1, 1, 206),
            _ => row(1, 2, 206),
        },
        _ => Rect(0, 0, 0, 0),
    }
}

/// x of a level on the meter.
fn meter_x(db: f32) -> i32 {
    let fraction = ((db - METER_FLOOR) / -METER_FLOOR).clamp(0., 1.);
    METER.0 + (fraction * METER.2 as f32).round() as i32
}

impl App {
    pub(super) fn draw_sample_root(&mut self, p: &mut Painter) {
        self.draw_sample_header(p);
        match self.sample_ui.page {
            Page::Record => self.draw_record_page(p),
            Page::Sample => self.draw_sample_page(p),
        }
        // The shared menu status bar: identity, the live strip with the
        // recording wiper, and a moved knob's readout or a notice.
        let grabbed = self.sample_ui.grab.is_some();
        self.menu_status_bar(p, grabbed);
    }

    /// Root name, then its pages; the current page is bold and underlined,
    /// and with page-strip focus it also gets the blue wash (4.1).
    fn draw_sample_header(&mut self, p: &mut Painter) {
        let names: Vec<&str> = Page::ALL.iter().map(|page| page.name()).collect();
        let boxes = components::draw_page_strip(
            p,
            "Sample",
            &names,
            self.sample_ui.page.index(),
            self.sample_ui.strip,
        );
        let x = boxes.last().map_or(120, |r| r.0 + r.2);
        let tabs: Vec<(Page, Rect)> = Page::ALL.into_iter().zip(boxes).collect();
        self.sample_ui.tabs = tabs;
        // Item 1: the input the engine reported (CARD_NAME).
        let input = match (&self.device, &self.sample_ui.input) {
            (_, Some(name)) => Some(format!("Input: {name}")),
            (None, None) => Some("Input: demo".into()),
            (Some(_), None) => None,
        };
        if let Some(input) = input {
            let width = p.width_in(Face::Condensed, &input, 16).round() as i32;
            let room = 784 - (x + 24);
            if width <= room {
                p.text_right(Face::Condensed, 784, 13, 16, &input, INK2);
            } else {
                p.fit_in(Face::Condensed, x + 24, 13, 16, room, &input, INK2);
            }
        }
    }

    fn draw_record_page(&mut self, p: &mut Painter) {
        let slot = self.recording.unwrap_or(self.parts[self.part]);
        let length = self.sample_length(slot);
        let position = self.record_fraction();
        let preview = self
            .sample_ui
            .grab
            .filter(|g| self.catalog[g.index].id == "sample_size")
            .map(|g| g.draft);
        // Item 2: the buffer, the red wiper and the position within it.
        p.rect(BUFFER, WELL);
        let span = match preview {
            Some(new) if length > 0. => new / length,
            _ => 1.,
        };
        let center = BUFFER.1 as f32 + BUFFER.3 as f32 / 2.;
        let half = BUFFER.3 as f32 * 0.46;
        let columns = self.sample_columns(slot);
        if let Some(columns) = &columns {
            for x in 0..BUFFER.2 {
                let f = (x as f32 + 0.5) / BUFFER.2 as f32 * span;
                if f >= 1. {
                    break;
                }
                let (lo, hi) =
                    columns[((f * columns.len() as f32) as usize).min(columns.len() - 1)];
                if !lo.is_finite() || !hi.is_finite() {
                    continue;
                }
                let top = (center - (hi - 0.5) * 2. * half).round() as i32;
                let bottom = (center - (lo - 0.5) * 2. * half).round() as i32;
                // Not yet overwritten in this pass: the dim tone.
                let ahead = position.is_some_and(|pos| f > pos);
                p.rect(
                    Rect(BUFFER.0 + x, top.min(bottom), 1, (bottom - top).abs() + 1),
                    if ahead { OUTSIDE_LOOP } else { WAVEFORM },
                );
            }
        }
        if preview.is_some() && span > 1. {
            // The current buffer ends here; the rest is the added length.
            let x = BUFFER.0 + (BUFFER.2 as f32 / span).round() as i32;
            p.rect(Rect(x, BUFFER.1, 1, BUFFER.3), AMBER);
        }
        if length <= 0. || columns.is_none() {
            p.text_in(
                Face::Condensed,
                BUFFER.0 + 16,
                center as i32 - 10,
                20,
                "No sample loaded",
                INK2,
            );
        }
        let (text, color) = match (position, preview) {
            (_, Some(new)) => (format!("\u{2039} {new:.2} s \u{203a}"), AMBER),
            (Some(pos), None) => (format!("{:.2} s / {length:.2} s", pos * length), INK),
            (None, None) => (format!("{length:.2} s"), INK2),
        };
        let width = p.width_in(Face::Condensed, &text, 16).round() as i32;
        p.rect(
            Rect(
                BUFFER.0 + BUFFER.2 - width - 16,
                BUFFER.1 + 2,
                width + 12,
                22,
            ),
            WELL,
        );
        p.text_right(
            Face::Condensed,
            BUFFER.0 + BUFFER.2 - 8,
            BUFFER.1 + 3,
            16,
            &text,
            color,
        );
        if let Some(pos) = position.filter(|_| preview.is_none()) {
            let x = BUFFER.0 + (pos.clamp(0., 1.) * (BUFFER.2 - 1) as f32).round() as i32;
            p.rect(Rect(x, BUFFER.1, 2, BUFFER.3), RED);
        }
        self.draw_meter(p);
        // Item 4: Record and Stop, then the other actions.
        let page = Page::Record;
        for field in [Field::Record, Field::Clear, Field::Save] {
            self.draw_action(p, page, field);
        }
        for field in &page.fields()[3..] {
            self.draw_field(p, page, *field);
        }
    }

    /// Item 3: blue, amber within the last 6 dB behind a thin line, a
    /// one-second peak tick and a latching Clip mark. The fill is 10 pixels
    /// tall so a full-scale bar stays under 2% of the screen in Dark (5.5).
    fn draw_meter(&mut self, p: &mut Painter) {
        p.rect(METER, WELL);
        let level = self.input_level();
        if let Some((db, _)) = level.filter(|(db, _)| *db > METER_FLOOR) {
            let x = meter_x(db);
            let knee = meter_x(-6.);
            p.rect(Rect(METER.0, METER.1 + 5, x.min(knee) - METER.0, 10), BLUE);
            if x > knee {
                p.rect(Rect(knee, METER.1 + 5, x - knee, 10), AMBER);
            }
        }
        p.rect(Rect(meter_x(-6.), METER.1 - 4, 1, METER.3 + 8), AMBER);
        if let Some((peak, _)) = self.sample_ui.peak.filter(|(db, _)| *db > METER_FLOOR) {
            p.rect(Rect(meter_x(peak) - 1, METER.1, 2, METER.3), INK);
        }
        let clip = self.sample_ui.clip;
        p.round_rect(CLIP, 2, if clip { RED } else { INK3 });
        p.round_rect(
            Rect(CLIP.0 + 1, CLIP.1 + 1, CLIP.2 - 2, CLIP.3 - 2),
            2,
            if clip { RED_WASH } else { BACKGROUND },
        );
        let face = if clip { Face::Bold } else { Face::Condensed };
        let width = p.width_in(face, "Clip", 14).round() as i32;
        p.text_in(
            face,
            CLIP.0 + (CLIP.2 - width) / 2,
            CLIP.1 + 1,
            14,
            "Clip",
            if clip { RED } else { INK3 },
        );
        for db in [-48, -36, -24, -12, -6] {
            let text = format!("{db}");
            let width = p.width_in(Face::Condensed, &text, 14).round() as i32;
            p.text_in(
                Face::Condensed,
                meter_x(db as f32) - width / 2,
                METER.1 + METER.3 + 4,
                14,
                &text,
                INK3,
            );
        }
        p.text_right(
            Face::Condensed,
            meter_x(0.),
            METER.1 + METER.3 + 4,
            14,
            "0 dB",
            INK3,
        );
    }

    fn draw_sample_page(&mut self, p: &mut Painter) {
        let slot = self.parts[self.part];
        let patch = self.patches[slot].clone();
        // The focused preset's sample with its loop, spray and playhead.
        p.rect(STRIP, WELL);
        let (scan, radius) = match &self.device {
            Some(d) => (d.display.scan[self.part], d.display.spray[self.part]),
            None => (patch.params[0] / 100., patch.params[2] / 100.),
        };
        let spray = performance::spray_ranges(scan, radius);
        let (loop_lo, loop_hi) = (patch.params[6] / 100., patch.params[7] / 100.);
        let center = STRIP.1 as f32 + STRIP.3 as f32 / 2.;
        let half = STRIP.3 as f32 * 0.46;
        let columns = self.sample_columns(slot);
        if let Some(columns) = &columns {
            for x in 0..STRIP.2 {
                let f = (x as f32 + 0.5) / STRIP.2 as f32;
                let (lo, hi) =
                    columns[((f * columns.len() as f32) as usize).min(columns.len() - 1)];
                if !lo.is_finite() || !hi.is_finite() {
                    continue;
                }
                let top = (center - (hi - 0.5) * 2. * half).round() as i32;
                let bottom = (center - (lo - 0.5) * 2. * half).round() as i32;
                let tone = if f < loop_lo || f > loop_hi {
                    OUTSIDE_LOOP
                } else if spray.iter().any(|(a, b)| f >= *a && f <= *b) {
                    SPOTLIGHT
                } else {
                    WAVEFORM
                };
                p.rect(
                    Rect(STRIP.0 + x, top.min(bottom), 1, (bottom - top).abs() + 1),
                    tone,
                );
            }
        } else {
            p.text_in(
                Face::Condensed,
                STRIP.0 + 16,
                center as i32 - 10,
                20,
                "No sample loaded",
                INK2,
            );
        }
        if scan.is_finite() && columns.is_some() {
            let x = STRIP.0 + (scan.rem_euclid(1.) * (STRIP.2 - 1) as f32).round() as i32;
            p.rect(Rect(x, STRIP.1, 2, STRIP.3), PLAYHEAD);
        }
        // Name, length, rate and channels.
        let length = self.sample_length(slot);
        let info = self.focused_sample_info();
        p.fit_in(Face::Condensed, 16, 146, 20, 290, &patch.sample, INK);
        p.text_in(Face::Regular, 330, 146, 20, &format!("{length:.2} s"), INK);
        if let Some(info) = info {
            p.text_in(
                Face::Regular,
                480,
                146,
                20,
                &format!("{:.1} kHz", info.rate as f32 / 1000.),
                INK,
            );
            let channels = match info.channels {
                1 => "Mono".to_string(),
                2 => "Stereo".to_string(),
                n => format!("{n} channels"),
            };
            p.text_in(Face::Regular, 630, 146, 20, &channels, INK);
        }
        p.text_in(Face::Condensed, 30, 184, 16, "Tuning", INK2);
        p.text_in(Face::Condensed, 422, 184, 16, "Import", INK2);
        let page = Page::Sample;
        for field in &page.fields()[..5] {
            self.draw_field(p, page, *field);
        }
        for field in [Field::Save, Field::Copy, Field::Clear] {
            self.draw_action(p, page, field);
        }
    }

    /// The buffer's min/max columns, 0..1 bottom-up: the native stencil on
    /// the device, the demo's shape on the desktop. None without a sample.
    fn sample_columns(&self, slot: usize) -> Option<Vec<(f32, f32)>> {
        match &self.device {
            Some(d) => {
                let w = &d.display.waves[slot];
                (w.frames > 0 && !w.columns.is_empty()).then(|| w.columns.clone())
            }
            None => {
                let sample = &self.patches[slot].sample;
                (sample != "Empty sample.wav").then(|| {
                    (0..544)
                        .map(|i| {
                            let a = Self::sample_amplitude(sample, (i as f32 + 0.5) / 544.) / 2.;
                            (0.5 - a, 0.5 + a)
                        })
                        .collect()
                })
            }
        }
    }

    fn focused(&self, page: Page, field: Field) -> bool {
        !self.sample_ui.strip && self.sample_ui.page == page && self.sample_field() == field
    }

    /// Record/Stop, Clear sample, Save sample and Copy to preset, with the
    /// keycap of the panel button that performs the same action.
    fn draw_action(&mut self, p: &mut Painter, page: Page, field: Field) {
        let r = field_rect(page, field);
        let focused = self.focused(page, field);
        if field == Field::Record {
            let recording = self.recording.is_some();
            if recording {
                p.round_rect(r, 4, RED);
                p.round_rect(Rect(r.0 + 2, r.1 + 2, r.2 - 4, r.3 - 4), 3, RED_WASH);
                p.rect(Rect(r.0 + 20, r.1 + 17, 14, 14), RED);
                p.text_in(Face::Bold, r.0 + 46, r.1 + 13, 20, "Stop", RED);
                if focused {
                    // Focus never relies on color alone: a frame outside.
                    p.outline(Rect(r.0 - 5, r.1 - 5, r.2 + 10, r.3 + 10), 2, BLUE);
                }
            } else {
                components::button(p, r, "", focused, INK, None);
                p.disc((r.0 + 27) as f32, (r.1 + 24) as f32, 7., RED);
                p.text_in(
                    if focused { Face::Bold } else { Face::Condensed },
                    r.0 + 46,
                    r.1 + 13,
                    20,
                    "Record",
                    if focused { BLUE } else { INK },
                );
            }
            return;
        }
        let key = match field {
            // Panel Save opens the sheet for the lit type (J6), so its keycap
            // shows only while Sample is lit.
            Field::Save => {
                (self.files_ui.lit == overlay::DataKind::Sample).then_some(("Save", BLUE))
            }
            Field::Copy => Some(("Copy", BLUE)),
            _ => None,
        };
        components::button(
            p,
            r,
            label(field),
            focused,
            if focused { BLUE } else { INK },
            key,
        );
    }

    /// A field row: label left, value right. Moving focus is a blue bar on a
    /// raised row; a grab is an amber row with the value between angle marks,
    /// a range bar for numbers and a row of dots for choices (3.5, 3.6).
    fn draw_field(&mut self, p: &mut Painter, page: Page, field: Field) {
        let Field::Setting(id) = field else {
            return;
        };
        let r = field_rect(page, field);
        let index = self.setting_index(id);
        let setting = self.catalog[index].clone();
        let grab = self.sample_ui.grab.filter(|g| g.index == index);
        let focused = self.focused(page, field);
        // Inactive while recording, still focusable (rule 3.5.7).
        let inactive = id == "sample_size" && self.recording.is_some();
        let value = grab.map_or_else(|| self.value(&setting), |g| g.draft);
        if grab.is_some() {
            p.round_rect(r, 3, AMBER_WASH);
            p.rect(Rect(r.0, r.1, 3, r.3), AMBER);
        } else if focused {
            p.round_rect(r, 3, RAISED);
            p.rect(Rect(r.0, r.1, 3, r.3), BLUE);
        }
        let text_y = r.1 + 9;
        p.fit_in(
            Face::Condensed,
            r.0 + 14,
            text_y,
            20,
            220,
            label(field),
            if inactive {
                INK3
            } else if focused {
                INK
            } else {
                INK2
            },
        );
        let text = self.sample_value_text(id, &setting.kind, value);
        let right = r.0 + r.2 - 14;
        if grab.is_some() {
            p.text_right(
                Face::Bold,
                right,
                text_y,
                20,
                &format!("\u{2039} {text} \u{203a}"),
                AMBER,
            );
        } else {
            let color = if inactive {
                INK3
            } else {
                match setting.kind {
                    Kind::Toggle if value > 0. => BLUE,
                    Kind::Toggle => INK2,
                    _ => INK,
                }
            };
            p.text_right(Face::Regular, right, text_y, 20, &text, color);
        }
        if let Some(grab) = grab {
            match &setting.kind {
                Kind::Number { min, max, .. } => {
                    let bar = Rect(r.0 + 14, r.1 + r.3 - 7, r.2 - 28, 2);
                    let at = |v: f32| {
                        bar.0
                            + (((v - min) / (max - min)).clamp(0., 1.) * bar.2 as f32).round()
                                as i32
                    };
                    p.rect(bar, OUTLINE);
                    p.rect(Rect(bar.0, bar.1, at(grab.draft) - bar.0, 2), AMBER);
                    p.rect(Rect(at(grab.original) - 1, bar.1 - 3, 2, 8), INK);
                }
                Kind::Choice(options) => {
                    let n = options.len() as i32;
                    for i in 0..n {
                        let x = right - (n - 1 - i) * 12 - 3;
                        let current = i == grab.draft as i32;
                        p.disc(
                            x as f32,
                            (r.1 + r.3 - 6) as f32,
                            if current { 3. } else { 2. },
                            if current { AMBER } else { OUTLINE },
                        );
                    }
                }
                Kind::Toggle => {}
            }
        }
    }

    fn sample_value_text(&self, id: &str, kind: &Kind, value: f32) -> String {
        match (id, kind) {
            (_, Kind::Toggle) => if value > 0. { "On" } else { "Off" }.into(),
            (_, Kind::Choice(options)) => options
                .get(value.round().max(0.) as usize)
                .copied()
                .unwrap_or("-")
                .into(),
            ("pitch", _) => format!("{value:+.2} st"),
            ("tuning", _) => Self::note(value.round().clamp(0., 127.) as usize),
            ("sample_size", _) => format!("{value:.2} s"),
            (_, Kind::Number { unit: "%", .. }) => format!("{value:.0}%"),
            (_, Kind::Number { unit, .. }) => format!("{value:.0} {unit}"),
        }
    }
}
