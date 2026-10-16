//! Draws the panel interface from shared components.
use super::components::{self, draw_field, group_title, Field, FieldState};
use super::patch_root::*;
use super::*;
use crate::device::display::lfo_window;

/// Two columns of related groups .
const LEFT: i32 = 16;
const RIGHT: i32 = 408;
const HALF: i32 = 376;
/// Three columns: the FX and Keys pages and the matrix's own fields.
const THIRDS: [i32; 3] = [16, 276, 536];
const THIRD: i32 = 248;
const ROW_H: i32 = 40;
const PITCH: i32 = 44;

/// Fields stacked down a column.
fn column(items: &[Item], x: i32, y: i32, w: i32, pitch: i32, h: i32) -> Vec<(Item, Rect)> {
    items
        .iter()
        .enumerate()
        .map(|(i, item)| (*item, Rect(x, y + i as i32 * pitch, w, h)))
        .collect()
}

fn note_name(value: f32) -> String {
    if value < 0. {
        "None".into()
    } else {
        App::note(value.round() as usize)
    }
}

/// A number with a true minus sign.
fn signed(value: f32, decimals: usize) -> String {
    let text = format!("{:.*}", decimals, value.abs());
    if value < 0. && text.trim_start_matches(['0', '.']) != "" {
        format!("\u{2212}{text}")
    } else {
        text
    }
}

impl App {
    /// A setting's value as its page shows it.
    fn field_text(&self, index: usize, value: f32) -> String {
        let s = &self.catalog[index];
        match (&s.kind, s.id.as_str()) {
            (Kind::Toggle, "cubic") => if value > 0. { "Cubic" } else { "Linear" }.into(),
            (Kind::Choice(_), "matrix_mode") => if value > 0. { "Matrix" } else { "1-to-1" }.into(),
            (Kind::Number { unit: "MIDI", .. }, _) => note_name(value),
            (_, id) if id.ends_with("_mul") => format!("\u{d7}{value:.0}"),
            (_, id) if id.ends_with("_div") => format!("\u{f7}{value:.0}"),
            (Kind::Number { .. }, "cutoff") if value >= 1000. => {
                format!("{:.2} kHz", value / 1000.)
            }
            (Kind::Number { step, unit, .. }, _) => {
                let decimals = if *step < 1. { 2 } else { 0 };
                let number = signed(value, decimals);
                if unit.is_empty() {
                    number
                } else if *unit == "%" {
                    format!("{number}%")
                } else {
                    format!("{number} {unit}")
                }
            }
            _ => Self::val_text(s, value),
        }
    }

    /// Clock multipliers and dividers wait for their clock sync (3.5.7).
    fn field_dim(&self, index: usize) -> bool {
        let id = self.catalog[index].id.as_str();
        let sync = match id {
            "grain_mul" | "grain_div" => "grain_clock",
            "scan_mul" | "scan_div" => "scan_clock",
            "lfo1_mul" | "lfo1_div" => "lfo1_sync",
            "lfo2_mul" | "lfo2_div" => "lfo2_sync",
            _ => return false,
        };
        self.get(sync) == 0.
    }

    /// A target's value as a fraction of its range, for the grab's bar.
    fn range_fraction(&self, target: LiveTarget, value: f32) -> f32 {
        let (min, max, _) = self.target_range(target);
        if max <= min {
            0.
        } else if min > 0. && max / min >= 1000. {
            (value / min).ln() / (max / min).ln()
        } else {
            (value - min) / (max - min)
        }
    }

    fn item_state(&self, item: Item) -> FieldState {
        let (active, grabbed) = self.patch_ui.drawn;
        if grabbed == Some(item) {
            FieldState::Grabbed
        } else if !self.patch_ui.on_strip && active == Some(item) {
            FieldState::Focused
        } else {
            FieldState::Normal
        }
    }

    /// The row of one item, with its focus, grab and value.
    fn item_field(&self, item: Item) -> Field<'static> {
        let state = self.item_state(item);
        let grab_range = |target: LiveTarget, value: f32| {
            let original = self.live.map_or(value, |l| l.original);
            Some((
                self.range_fraction(target, value),
                self.range_fraction(target, original),
            ))
        };
        match item {
            Item::Setting(i) => {
                let s = &self.catalog[i];
                let pending = self
                    .patch_ui
                    .choice
                    .filter(|(c, _)| *c == i)
                    .map(|(_, v)| v);
                let value = pending.unwrap_or_else(|| self.target_value(LiveTarget::Setting(i)));
                let mut f = Field::plain(&s.label, self.field_text(i, value));
                f.state = state;
                f.dim = self.field_dim(i);
                f.on = matches!(s.kind, Kind::Toggle) && value > 0.;
                if state == FieldState::Grabbed {
                    match &s.kind {
                        Kind::Choice(options) => f.dots = Some((options.len(), value as usize)),
                        Kind::Number { .. } => f.range = grab_range(LiveTarget::Setting(i), value),
                        Kind::Toggle => {}
                    }
                }
                f
            }
            Item::GrainSize => {
                let target = LiveTarget::Param(1, false);
                let value = self.target_value(target);
                let mut f = Field::plain("Grain Size", self.param_value(1, value, false));
                f.state = state;
                if state == FieldState::Grabbed {
                    f.range = grab_range(target, value);
                }
                f
            }
            Item::Aux => {
                let mut f = Field::plain(
                    "AUX shape",
                    if self.aux_recording { "Stop" } else { "Record" }.into(),
                );
                f.state = state;
                f.on = true;
                f
            }
            _ => Field::plain("", String::new()),
        }
    }

    /// Draw items as field rows and give the desktop pointer their boxes.
    fn draw_fields(&mut self, p: &mut Painter, rows: &[(Item, Rect)]) {
        for (item, r) in rows {
            let mut f = self.item_field(*item);
            let keys = self.fx_keycaps(*item);
            f.keycaps = keys;
            draw_field(p, *r, &f);
            self.hits.push(Hit {
                r: *r,
                a: Action::PatchItem(*item),
            });
        }
    }

    /// The two parameters the FX knobs turn carry the FX-1 and FX-2 keycaps
    /// .
    fn fx_keycaps(&self, item: Item) -> &'static [&'static str] {
        let Item::Setting(i) = item else { return &[] };
        if self.patch_ui.page != FX {
            return &[];
        }
        match self
            .fx_controls()
            .iter()
            .position(|id| *id == self.catalog[i].id)
        {
            Some(0) => &["FX-1"],
            Some(_) => &["FX-2"],
            None if self.catalog[i].id == "fx_map" => &["FX-1", "FX-2"],
            None => &[],
        }
    }

    /// The Patch root: page strip, the page, the status bar with the strip.
    pub(super) fn draw_patch_root(&mut self, p: &mut Painter) {
        // Focus and grab, resolved once for this frame's fields.
        self.patch_ui.drawn = (Some(self.patch_active()), self.patch_grabbed());
        let page = self.patch_ui.page;
        let boxes = components::draw_page_strip(p, "Patch", &PAGES, page, self.patch_ui.on_strip);
        for (i, r) in boxes.into_iter().enumerate() {
            self.hits.push(Hit {
                r,
                a: Action::PatchPage(i),
            });
        }
        match page {
            PLAY => self.draw_play(p),
            TIMING => self.draw_timing(p),
            SHAPE => self.draw_shape(p),
            FX => self.draw_fx(p),
            VOICES => self.draw_voices(p),
            KEYS => self.draw_keys(p),
            MATRIX => self.draw_matrix(p),
            _ => self.draw_lfo(p, page - LFO1),
        }
        let grabbed = self.patch_ui.drawn.1.is_some();
        self.menu_status_bar(p, grabbed);
        // Focus here is by identity, not by drawn hits: one pass is enough.
        self.navigation_dirty = false;
        self.rendered_context = self.focus_context();
    }

    fn settings_of(&self, page: usize, group: &str) -> Vec<Item> {
        self.group_settings(page, group)
            .into_iter()
            .map(Item::Setting)
            .collect()
    }

    fn draw_play(&mut self, p: &mut Painter) {
        let items = self.page_items(PLAY);
        let grains = self.settings_of(PLAY, "Grains");
        let playback: Vec<Item> = items
            .iter()
            .copied()
            .filter(|i| !grains.contains(i))
            .collect();
        group_title(p, LEFT, 52, "Playback");
        group_title(p, RIGHT, 52, "Grains");
        let mut rows = column(&playback, LEFT, 78, HALF, PITCH, ROW_H);
        rows.extend(column(&grains, RIGHT, 78, HALF, PITCH, ROW_H));
        self.draw_fields(p, &rows);
    }

    fn draw_timing(&mut self, p: &mut Painter) {
        group_title(p, LEFT, 52, "Grains");
        group_title(p, RIGHT, 52, "Scan");
        let mut rows = column(
            &self.settings_of(TIMING, "Grains"),
            LEFT,
            78,
            HALF,
            PITCH,
            ROW_H,
        );
        rows.extend(column(
            &self.settings_of(TIMING, "Scan"),
            RIGHT,
            78,
            HALF,
            PITCH,
            ROW_H,
        ));
        self.draw_fields(p, &rows);
    }

    fn draw_shape(&mut self, p: &mut Painter) {
        let active = (!self.patch_ui.on_strip).then(|| self.patch_active());
        let lit = ["attack", "decay", "sustain", "release"]
            .iter()
            .position(|id| active == Some(Item::Setting(self.setting_index(id))));
        group_title(p, LEFT, 52, "Amplitude");
        let well = Rect(LEFT, 76, HALF, 160);
        p.rect(well, WELL);
        let corners = self.draw_envelope(p, (20., 388., 82., 232.), lit);
        for (x, y) in corners {
            p.rect(Rect(x.round() as i32 - 3, y.round() as i32 - 3, 6, 6), INK2);
        }
        // The grain window from the engine's 128-point curve.
        group_title(p, RIGHT, 52, "Grain window");
        let size = self.param_value(1, self.patch().params[1], false);
        let size_lit = active == Some(Item::GrainSize);
        p.text_right(
            if size_lit {
                Face::Bold
            } else {
                Face::Condensed
            },
            784,
            52,
            16,
            &format!("Grain Size {size}"),
            if size_lit { AMBER } else { INK },
        );
        let well = Rect(RIGHT, 76, HALF, 160);
        p.rect(well, WELL);
        let (left, right, top, bottom) = (412., 780., 82., 232.);
        let points: Vec<(f32, f32)> = self
            .envelope_curve(true)
            .into_iter()
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .map(|(x, y)| {
                (
                    left + x.clamp(0., 1.) * (right - left),
                    bottom - y.clamp(0., 1.) * (bottom - top),
                )
            })
            .collect();
        for pair in points.windows(2) {
            let ((x0, y0), (x1, _)) = (pair[0], pair[1]);
            for x in x0.round() as i32..x1.round().max(x0.round() + 1.) as i32 {
                p.rect(
                    Rect(x, y0.round() as i32, 1, bottom as i32 - y0.round() as i32),
                    CURVE_FILL,
                );
            }
        }
        let window_lit = ["sides", "tilt", "curve"]
            .iter()
            .any(|id| active == Some(Item::Setting(self.setting_index(id))));
        p.stroke(
            &points,
            if window_lit { 4 } else { 2 },
            if window_lit { INK } else { BLUE },
        );
        let mut rows = column(
            &self.settings_of(SHAPE, "Amplitude"),
            LEFT,
            246,
            HALF,
            PITCH,
            ROW_H,
        );
        let mut window = self.settings_of(SHAPE, "Grain window");
        window.push(Item::GrainSize);
        rows.extend(column(&window, RIGHT, 246, HALF, PITCH, ROW_H));
        self.draw_fields(p, &rows);
    }

    fn draw_fx(&mut self, p: &mut Painter) {
        let knobs = self.settings_of(FX, "Knobs");
        let mut rows = column(&knobs, LEFT, 50, 520, PITCH, ROW_H);
        for (i, title) in ["Effect", "Filter", "Sub-oscillator"].iter().enumerate() {
            group_title(p, THIRDS[i], 102, title);
        }
        let effect: Vec<Item> = self
            .effect_settings()
            .into_iter()
            .map(Item::Setting)
            .collect();
        rows.extend(column(&effect, THIRDS[0], 126, THIRD, PITCH, ROW_H));
        rows.extend(column(
            &self.settings_of(FX, "Filter"),
            THIRDS[1],
            126,
            THIRD,
            PITCH,
            ROW_H,
        ));
        rows.extend(column(
            &self.settings_of(FX, "Sub-oscillator"),
            THIRDS[2],
            126,
            THIRD,
            PITCH,
            ROW_H,
        ));
        self.draw_fields(p, &rows);
    }

    fn draw_voices(&mut self, p: &mut Painter) {
        group_title(p, LEFT, 52, "Play mode");
        let rows = column(
            &self.settings_of(VOICES, "Play mode"),
            LEFT,
            78,
            HALF,
            PITCH,
            ROW_H,
        );
        self.draw_fields(p, &rows);
        // The scale: twelve keys, lit when on; entered with a press.
        let scale = Rect(LEFT, 254, HALF, 132);
        let state = self.item_state(Item::Scale);
        let entered = self.patch_ui.entered == Some(Item::Scale);
        if state == FieldState::Focused || entered {
            p.round_rect(scale, 3, RAISED);
            p.rect(Rect(scale.0, scale.1, 3, scale.3), BLUE);
        }
        p.text_in(
            Face::Condensed,
            LEFT + 14,
            262,
            20,
            "Scale",
            if state == FieldState::Normal && !entered {
                INK2
            } else {
                INK
            },
        );
        let notes = self.scale_notes();
        let on = notes.iter().filter(|n| **n).count();
        p.text_right(
            Face::Regular,
            LEFT + HALF - 14,
            262,
            20,
            &format!("{on} of 12"),
            INK,
        );
        let focus_key = match (entered, self.patch_active()) {
            (true, Item::Key(k)) => Some(k),
            _ => None,
        };
        const WHITE: [usize; 7] = [0, 2, 4, 5, 7, 9, 11];
        const BLACK: [(usize, usize); 5] = [(1, 0), (3, 1), (6, 3), (8, 4), (10, 5)];
        let (kx, ky, kw, kh) = (LEFT + 14, 296, 46, 80);
        let key_box = |p: &mut Painter, n: usize, r: Rect, hits: &mut Vec<Hit>| {
            let fill = if notes[n] { BLUE_WASH } else { WELL };
            p.rect(r, OUTLINE);
            p.rect(Rect(r.0 + 1, r.1 + 1, r.2 - 2, r.3 - 2), fill);
            if notes[n] {
                p.disc((r.0 + r.2 / 2) as f32, (r.1 + r.3 - 10) as f32, 4., BLUE);
            }
            hits.push(Hit {
                r,
                a: Action::PatchItem(Item::Key(n)),
            });
        };
        let mut hits = vec![];
        for (i, n) in WHITE.iter().enumerate() {
            key_box(p, *n, Rect(kx + i as i32 * kw, ky, kw + 1, kh), &mut hits);
        }
        // A focused white key's frame lies under the black keys; a black
        // key's over its neighbours.
        if let Some(i) = focus_key.and_then(|n| WHITE.iter().position(|w| *w == n)) {
            p.outline(Rect(kx + i as i32 * kw, ky, kw + 1, kh), 2, BLUE);
        }
        let black = |after: usize| Rect(kx + (after as i32 + 1) * kw - 14, ky, 28, 48);
        for (n, after) in BLACK {
            key_box(p, n, black(after), &mut hits);
            if focus_key == Some(n) {
                p.outline(black(after), 2, BLUE);
            }
        }
        self.hits.extend(hits);
        self.hits.push(Hit {
            r: Rect(LEFT, 254, HALF, 36),
            a: Action::PatchItem(Item::Scale),
        });
        // The four voices mirror the ADSR sliders in play mode (manual p.14).
        group_title(p, RIGHT, 52, "Voices");
        let voice_x = |n: usize| RIGHT + 96 + n as i32 * 70;
        for n in 0..4 {
            let label = format!("{}", n + 1);
            let w = p.width_in(Face::Condensed, &label, 16).round() as i32;
            p.text_in(
                Face::Condensed,
                voice_x(n) + 33 - w / 2,
                52,
                16,
                &label,
                INK2,
            );
        }
        let voices = self.settings_of(VOICES, "Voices");
        for (row, (title, y, h)) in [("Pitch", 78, ROW_H), ("Level", 126, 112)]
            .into_iter()
            .enumerate()
        {
            p.text_in(Face::Condensed, RIGHT + 14, y + 8, 20, title, INK2);
            for n in 0..4 {
                let item = voices[row * 4 + n];
                let r = Rect(voice_x(n), y, 66, h);
                self.voice_cell(p, item, r, row == 1);
            }
        }
    }

    /// One voice's pitch or level cell.
    fn voice_cell(&mut self, p: &mut Painter, item: Item, r: Rect, level: bool) {
        let Item::Setting(i) = item else { return };
        let state = self.item_state(item);
        let value = self.target_value(LiveTarget::Setting(i));
        match state {
            FieldState::Grabbed => {
                p.round_rect(r, 3, AMBER_WASH);
                p.outline(r, 2, AMBER);
            }
            FieldState::Focused => {
                p.round_rect(r, 3, RAISED);
                p.rect(Rect(r.0, r.1, 3, r.3), BLUE);
            }
            FieldState::Normal => {}
        }
        let color = if state == FieldState::Grabbed {
            AMBER
        } else {
            INK
        };
        let text = if level {
            format!("{value:.0}%")
        } else {
            note_name(value)
        };
        let w = p.width_in(Face::Regular, &text, 20).round() as i32;
        if level {
            let fraction = (value / 100.).clamp(0., 1.);
            let h = (fraction * 64.).round() as i32;
            p.rect(Rect(r.0 + r.2 / 2 - 5, r.1 + 8, 10, 64), OUTLINE);
            p.rect(Rect(r.0 + r.2 / 2 - 5, r.1 + 72 - h, 10, h), BLUE);
            p.text_in(
                Face::Regular,
                r.0 + r.2 / 2 - w / 2,
                r.1 + 80,
                20,
                &text,
                color,
            );
        } else {
            p.text_in(
                Face::Regular,
                r.0 + r.2 / 2 - w / 2,
                r.1 + 8,
                20,
                &text,
                color,
            );
        }
        self.hits.push(Hit {
            r,
            a: Action::PatchItem(item),
        });
    }

    fn draw_keys(&mut self, p: &mut Painter) {
        // The keyboard: three areas split at Split 1 and Split 2, the tuning
        // note marked (manual p.64).
        let bar = Rect(LEFT, 54, 768, 30);
        let x_of = |note: f32| bar.0 as f32 + (note + 1.).clamp(0., 128.) / 128. * bar.2 as f32;
        let (s1, s2) = (self.get("split1"), self.get("split2"));
        let tints = [WELL, RAISED, BLUE_WASH];
        let bounds = [(-1., s1), (s1, s2), (s2, 127.)];
        for (i, (lo, hi)) in bounds.iter().enumerate() {
            let (a, b) = (x_of(*lo + 1.).round() as i32, x_of(*hi + 1.).round() as i32);
            if b > a {
                p.rect(Rect(a, bar.1, b - a, bar.3), tints[i]);
            }
        }
        for split in [s1, s2] {
            p.rect(
                Rect(x_of(split + 1.).round() as i32, bar.1 - 2, 2, bar.3 + 4),
                OUTLINE,
            );
        }
        let tuning = self.get("tuning");
        p.triangle_up(x_of(tuning + 0.5), bar.1 + bar.3 + 12, 12., 10, INK);
        p.text_in(Face::Condensed, bar.0 + 6, bar.1 + 5, 16, "C-1", INK2);
        p.text_right(
            Face::Condensed,
            bar.0 + bar.2 - 6,
            bar.1 + 5,
            16,
            "G9",
            INK2,
        );
        let items = self.page_items(KEYS);
        let top: Vec<Item> = items[..3].to_vec();
        let mut rows: Vec<(Item, Rect)> = top
            .iter()
            .enumerate()
            .map(|(i, item)| (*item, Rect(THIRDS[i], 102, THIRD, ROW_H)))
            .collect();
        for area in 0..3 {
            let (lo, hi) = (bounds[area].0 + 1., bounds[area].1);
            let range = if lo > hi {
                "empty".into()
            } else {
                format!("{} \u{2013} {}", note_name(lo), note_name(hi))
            };
            group_title(p, THIRDS[area], 160, &format!("Area {area}   {range}"));
            rows.extend(column(
                &self.settings_of(KEYS, &format!("Area {area}")),
                THIRDS[area],
                186,
                THIRD,
                PITCH,
                ROW_H,
            ));
        }
        self.draw_fields(p, &rows);
    }

    fn draw_matrix(&mut self, p: &mut Painter) {
        const NAMES: i32 = 16;
        const CELL_X: i32 = 120;
        const CELL_PITCH: i32 = 60;
        const ROW_Y: i32 = 80;
        const ROW_PITCH: i32 = 28;
        let active = (!self.patch_ui.on_strip).then(|| self.patch_active());
        let entered = match self.patch_ui.entered {
            Some(Item::Row(r)) => Some(r),
            _ => None,
        };
        let grabbed = self.patch_grabbed();
        let column_lit = match active {
            Some(Item::Cell(_, c)) => Some(c),
            _ => None,
        };
        for (c, name) in catalog::DESTINATION_SHORT.iter().enumerate() {
            let face = if column_lit == Some(c) {
                Face::Bold
            } else {
                Face::Condensed
            };
            let w = p.width_in(face, name, 18).round() as i32;
            let x = CELL_X + c as i32 * CELL_PITCH + 28 - w / 2;
            let color = if column_lit == Some(c) { INK } else { INK2 };
            p.text_in(face, x, 52, 18, name, color);
        }
        for (r, source) in catalog::SOURCES.iter().enumerate() {
            let y = ROW_Y + r as i32 * ROW_PITCH;
            let row_active = active == Some(Item::Row(r)) || entered == Some(r);
            if row_active {
                p.round_rect(Rect(8, y - 1, 776, ROW_PITCH - 1), 3, RAISED);
                p.rect(Rect(8, y - 1, 3, ROW_PITCH - 1), BLUE);
            }
            p.fit_in(
                Face::Condensed,
                NAMES + 8,
                y + 1,
                20,
                if r < 2 { 54 } else { 96 },
                source,
                if row_active { INK } else { INK2 },
            );
            if r < 2 {
                // LFO rows carry a small live trace.
                let rate = self
                    .device
                    .as_ref()
                    .map(|d| d.display.lfo_speeds[r])
                    .unwrap_or_else(|| self.get(&format!("lfo{}_rate", r + 1)));
                let well = Rect(82, y + 4, 32, 18);
                self.draw_lfo_trace(p, r, rate, well, (112., 30.));
            }
            self.hits.push(Hit {
                r: Rect(8, y, 108, ROW_PITCH - 2),
                a: Action::PatchItem(Item::Row(r)),
            });
            for c in 0..catalog::DESTINATIONS.len() {
                if !catalog::route_allowed(r, c) {
                    continue;
                }
                let cell = Rect(CELL_X + c as i32 * CELL_PITCH, y + 1, 56, ROW_PITCH - 4);
                let item = Item::Cell(r, c);
                let value = self.target_value(LiveTarget::Route(r, c));
                self.matrix_cell(p, cell, value, grabbed == Some(item), active == Some(item));
                self.hits.push(Hit {
                    r: cell,
                    a: Action::PatchItem(item),
                });
            }
        }
        let fields = self.settings_of(MATRIX, "Matrix");
        let rows: Vec<(Item, Rect)> = fields
            .iter()
            .enumerate()
            .map(|(i, item)| {
                (
                    *item,
                    Rect(THIRDS[i % 3], 338 + (i / 3) as i32 * 42, THIRD, 38),
                )
            })
            .collect();
        self.draw_fields(p, &rows);
    }

    /// A matrix cell: the signed percent and a bar from the centre, right
    /// for positive and left for negative; zero shows a dot.
    fn matrix_cell(&self, p: &mut Painter, r: Rect, value: f32, grabbed: bool, focused: bool) {
        p.rect(r, if grabbed { AMBER_WASH } else { WELL });
        let center = r.0 + r.2 / 2;
        let percent = (value * 100.).round();
        if grabbed {
            p.outline(r, 2, AMBER);
        } else if focused {
            p.outline(r, 2, BLUE);
        }
        if percent == 0. && !grabbed {
            p.disc(center as f32, (r.1 + r.3 / 2) as f32, 1.5, INK3);
            return;
        }
        let mut text = signed(percent, 0);
        if percent > 0. {
            text.insert(0, '+');
        }
        if grabbed {
            text.push('%');
        }
        let (face, size) = if grabbed {
            (Face::Bold, 16)
        } else {
            (Face::Regular, 18)
        };
        let w = p.width_in(face, &text, size).round() as i32;
        p.text_in(
            face,
            center - w / 2,
            r.1 + 18 - size as i32,
            size,
            &text,
            if grabbed { AMBER } else { INK },
        );
        let length = ((value.abs().min(1.)) * (r.2 / 2 - 2) as f32).round() as i32;
        let x = if value < 0. { center - length } else { center };
        p.rect(
            Rect(x, r.1 + r.3 - 3, length.max(1), 2),
            if grabbed { AMBER } else { BLUE },
        );
    }

    fn draw_lfo(&mut self, p: &mut Painter, lfo: usize) {
        let rate = self
            .device
            .as_ref()
            .map(|d| d.display.lfo_speeds[lfo])
            .unwrap_or_else(|| self.get(&format!("lfo{}_rate", lfo + 1)));
        let well = Rect(LEFT, 52, 768, 100);
        self.draw_lfo_trace(p, lfo, rate, well, (776., 760.));
        // The -1 and +1 range.
        for y in [well.1 + 4, well.1 + well.3 - 5] {
            for x in (well.0..well.0 + well.2).step_by(6) {
                p.rect(Rect(x, y, 2, 1), RULE);
            }
        }
        let window = lfo_window(rate);
        p.text_right(
            Face::Condensed,
            well.0 + well.2 - 8,
            well.1 + well.3 - 22,
            14,
            &if window < 10. {
                format!("{window:.1} s")
            } else {
                format!("{window:.0} s")
            },
            INK3,
        );
        let routes = self.route_amounts(lfo);
        let end = p.text_in(Face::Condensed, LEFT + 14, 160, 16, "Routes", INK2);
        p.fit_in(
            Face::Condensed,
            end + 16,
            160,
            16,
            740 - end,
            if routes.is_empty() { "None" } else { &routes },
            if routes.is_empty() { INK3 } else { INK },
        );
        let items = self.page_items(LFO1 + lfo);
        let mut rows = column(&items[..6], LEFT, 186, HALF, 39, 37);
        rows.extend(column(&items[6..], RIGHT, 186, HALF, 39, 37));
        self.draw_fields(p, &rows);
    }
}
