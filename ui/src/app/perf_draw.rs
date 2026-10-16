//! Draws the panel interface from shared components.
use super::components::{self, Glyph, ListRow, SheetLine};
use super::perf_pages::SlotRow;
use super::perf_root::*;
use super::*;

/// Free space below which Disk free turns red.
const LOW_DISK: u64 = 256_000_000;
const SYSTEM_TOP: i32 = 52;
const SECTION_ROW: i32 = 32;
const FIELD_ROW: i32 = 40;

impl App {
    pub(super) fn draw_perf_root(&mut self, p: &mut Painter) {
        let right = match self.perf.page {
            PERFORMANCES => {
                if self.imported_working() {
                    "Working: from USB or Factory".to_string()
                } else {
                    format!(
                        "Working: {:02} {}",
                        self.performance_slot + 1,
                        self.performance
                    )
                }
            }
            PRESETS => match self.copy_arm {
                Some(copy) => format!("Copy {:02} to", copy.from + 1),
                None => format!("Focused part {}", self.part + 1),
            },
            SYSTEM => "Firmware 3.4.4".into(),
            _ => String::new(),
        };
        self.perf_header(p, &right);
        match self.perf.page {
            PERFORMANCES => self.draw_performances(p),
            PRESETS => self.draw_presets(p),
            PARTS => self.draw_parts(p),
            FILES => self.draw_files(p),
            _ => self.draw_system(p),
        }
        self.draw_perf_layers(p);
        // The shared menu status bar: identity, the live strip and a moved
        // knob's readout or a notice, as in the Patch and Sample roots.
        let grabbed = self.perf.grab.is_some();
        self.menu_status_bar(p, grabbed);
        self.navigation_dirty = false;
        self.rendered_context = self.focus_context();
    }
    /// The shared page strip , with the page's context at the
    /// right when the names leave room for it.
    fn perf_header(&mut self, p: &mut Painter, right: &str) {
        let boxes = components::draw_page_strip(p, "Perf", &PAGES, self.perf.page, self.perf.strip);
        let x = boxes.last().map_or(120, |r| r.0 + r.2) + 12;
        if !self.panel_mode {
            for (i, r) in boxes.iter().enumerate() {
                self.hits.push(Hit {
                    r: *r,
                    a: Action::OpenMenu(4, i),
                });
            }
        }
        if !right.is_empty() {
            let width = p.width_in(Face::Condensed, right, 16).round() as i32;
            if 784 - width > x {
                p.text_right(Face::Condensed, 784, 13, 16, right, INK2);
            }
        }
    }
    /// A thin line under the header: what the list is and a count or state.
    fn perf_subheader(&mut self, p: &mut Painter, left: &str, right: &str, right_color: Color) {
        let start = p.text_right(Face::Condensed, 776, 50, 16, right, right_color);
        p.fit_in(Face::Condensed, 24, 50, 16, start - 40, left, INK2);
    }

    // ----- Performances  -----

    fn draw_performances(&mut self, p: &mut Painter) {
        let rows = self.perf_slot_rows();
        let used = (0..50).filter(|i| !self.slot_name(*i).1).count();
        self.perf_subheader(p, "User slots", &format!("{used} saved"), INK3);
        let second = match rows.get(self.perf.slots.focus) {
            Some(SlotRow::Slot(slot)) => {
                let (_, empty) = self.slot_name(*slot);
                if empty {
                    "Empty slot".to_string()
                } else {
                    let (saved, count) = self.perf_slot_info(*slot);
                    let mut text = format!("{saved}     {count} presets");
                    if self.is_working_slot(*slot) && self.patches.iter().any(|p| p.dirty) {
                        text += "     unsaved changes";
                    }
                    text
                }
            }
            _ => "Save it to a user slot to keep it".to_string(),
        };
        let names: Vec<(String, bool)> = (0..50).map(|i| self.slot_name(i)).collect();
        let mut cursor = self.perf.slots;
        if self.perf.strip {
            // Focus is on the page strip; the list shows no focused row.
            cursor.focus = usize::MAX;
        }
        components::draw_list(p, cursor, rows.len(), None, |i, focused| {
            let (glyph, label, current, dim) = match rows[i] {
                SlotRow::Imported => (Glyph::Document, "Working".to_string(), true, false),
                SlotRow::Slot(slot) => {
                    let (name, empty) = &names[slot];
                    (
                        Glyph::Number(slot + 1),
                        if *empty { "Empty".into() } else { name.clone() },
                        self.is_working_slot(slot),
                        *empty,
                    )
                }
            };
            ListRow {
                glyph,
                label,
                right: String::new(),
                second: if focused {
                    second.clone()
                } else {
                    String::new()
                },
                current,
                dim,
                keycap: None,
                check: None,
                press: None,
            }
        });
    }

    // ----- Presets  -----

    fn draw_presets(&mut self, p: &mut Painter) {
        for bank in 0..4 {
            p.text_in(
                Face::Condensed,
                24,
                84 + bank * 92,
                16,
                &(bank + 1).to_string(),
                INK2,
            );
        }
        let playing = self.parts[self.part];
        for cell in 0..32 {
            let r = cell_rect(cell);
            let focused = cell == self.perf.cell && !self.perf.strip;
            let copying = self.copy_arm.is_some_and(|c| c.from == cell);
            p.round_rect(r, 4, if focused { BLUE } else { WELL });
            if focused {
                p.round_rect(Rect(r.0 + 2, r.1 + 2, r.2 - 4, r.3 - 4), 3, RAISED);
            }
            if copying {
                // The armed source: an amber frame (the Copy chip names it).
                p.outline(Rect(r.0 + 1, r.1 + 1, r.2 - 2, r.3 - 2), 2, AMBER);
            }
            let patch = &self.patches[cell];
            let empty = patch.sample.is_empty()
                || patch.sample.eq_ignore_ascii_case("empty.wav")
                || patch.sample == "Empty sample.wav";
            let number_end = p.text_in(
                Face::Condensed,
                r.0 + 8,
                r.1 + 4,
                16,
                &format!("{:02}", cell + 1),
                INK3,
            );
            if patch.dirty {
                p.disc((number_end + 8) as f32, (r.1 + 13) as f32, 3.5, AMBER);
            }
            // Part badges on the presets each part plays.
            let mut bx = r.0 + r.2 - 6;
            for part in (0..4).rev() {
                if self.parts[part] == cell && (self.enabled[part] || part == self.part) {
                    let b = Rect(bx - 20, r.1 + 4, 20, 20);
                    p.rect(b, if part == self.part { BLUE } else { BLUE_WASH });
                    let label = (part + 1).to_string();
                    let w = p.width_in(Face::Bold, &label, 16).round() as i32;
                    p.text_in(
                        Face::Bold,
                        b.0 + (20 - w) / 2,
                        b.1 + 1,
                        16,
                        &label,
                        if part == self.part { BACKGROUND } else { BLUE },
                    );
                    bx -= 24;
                }
            }
            let color = if empty {
                INK3
            } else if cell == playing {
                BLUE
            } else {
                INK
            };
            if empty {
                p.text_in(Face::Condensed, r.0 + 8, r.1 + 24, 20, "Empty", INK3);
            } else {
                let lines = name_lines(p, &patch.name, r.2 - 14);
                for (i, line) in lines.iter().enumerate() {
                    p.fit_in(
                        Face::Condensed,
                        r.0 + 8,
                        r.1 + 24 + i as i32 * 20,
                        20,
                        r.2 - 14,
                        line,
                        color,
                    );
                }
            }
            self.mini_wave(p, Rect(r.0 + 8, r.1 + 66, r.2 - 16, 16), cell, empty, false);
            if !self.panel_mode {
                self.hits.push(Hit {
                    r,
                    a: Action::Preset(cell),
                });
            }
        }
    }
    /// A preset's waveform downsampled from the cached stencil by each
    /// column's extremes ; an empty preset is a flat line.
    fn mini_wave(&self, p: &mut Painter, r: Rect, slot: usize, empty: bool, bright: bool) {
        let center = r.1 + r.3 / 2;
        let tone = if bright { WAVEFORM } else { WAVEFORM_DIM };
        if empty {
            p.rect(Rect(r.0, center, r.2, 1), INK3);
            return;
        }
        match &self.device {
            Some(d) => {
                let w = &d.display.waves[slot];
                if w.frames <= 0 || w.columns.is_empty() {
                    p.rect(Rect(r.0, center, r.2, 1), INK3);
                    return;
                }
                let n = w.columns.len();
                let half = r.3 as f32 / 2.;
                for x in 0..r.2 {
                    let a = x as usize * n / r.2 as usize;
                    let b = ((x as usize + 1) * n / r.2 as usize).max(a + 1).min(n);
                    let (lo, hi) = w.columns[a..b]
                        .iter()
                        .filter(|(lo, hi)| lo.is_finite() && hi.is_finite())
                        .fold((f32::NAN, f32::NAN), |(lo, hi), (a, b)| {
                            (lo.min(a.min(*b)), hi.max(a.max(*b)))
                        });
                    if !lo.is_finite() {
                        continue;
                    }
                    let top = (center as f32 - (hi - 0.5) * 2. * half).round() as i32;
                    let bottom = (center as f32 - (lo - 0.5) * 2. * half).round() as i32;
                    p.rect(
                        Rect(r.0 + x, top.min(bottom), 1, (bottom - top).abs() + 1),
                        tone,
                    );
                }
            }
            None => {
                let sample = &self.patches[slot].sample;
                for x in 0..r.2 {
                    let fraction = (x as f32 + 0.5) / r.2 as f32;
                    let a = (Self::sample_amplitude(sample, fraction) * r.3 as f32 * 0.46) as i32;
                    p.rect(Rect(r.0 + x, center - a, 1, 2 * a + 1), tone);
                }
            }
        }
    }

    // ----- Parts  -----

    fn draw_parts(&mut self, p: &mut Painter) {
        let grab = self.perf.grab;
        // The four multitimbral setups (manual p.87).
        let multi_focused = self.perf.part_row == 0 && !self.perf.strip;
        if multi_focused {
            p.round_rect(Rect(8, 50, 776, 42), 3, RAISED);
            p.rect(Rect(8, 50, 3, 42), BLUE);
        }
        p.text_in(Face::Condensed, 24, 60, 16, "Multi", INK2);
        let shown = match grab {
            Some(g) if g.field == Field::Multi => g.value as usize,
            _ => self.snapshot,
        };
        let grabbed = grab.is_some_and(|g| g.field == Field::Multi);
        for i in 0..4 {
            let r = Rect(80 + i as i32 * 44, 56, 36, 30);
            if i == shown && grabbed {
                p.round_rect(r, 3, AMBER);
                p.round_rect(Rect(r.0 + 1, r.1 + 1, r.2 - 2, r.3 - 2), 2, AMBER_WASH);
                let label = (i + 1).to_string();
                let w = p.width_in(Face::Bold, &label, 16).round() as i32;
                p.text_in(Face::Bold, r.0 + (36 - w) / 2, r.1 + 4, 16, &label, AMBER);
            } else {
                components::chip(p, r, &(i + 1).to_string(), i == self.snapshot);
            }
        }
        for part in 0..4 {
            self.draw_part_row(p, part);
        }
    }
    fn draw_part_row(&mut self, p: &mut Painter, part: usize) {
        let y = 100 + part as i32 * 80;
        let row_focused = self.perf.part_row == part + 1 && !self.perf.strip;
        let inside = if row_focused { self.perf.inside } else { None };
        let grab = self.perf.grab;
        if row_focused {
            p.round_rect(Rect(8, y, 776, 76), 3, RAISED);
            p.rect(Rect(8, y, 3, 76), BLUE);
        }
        let on = self.enabled[part];
        let slot = self.parts[part];
        let ink = if on { INK } else { INK3 };
        let ink2 = if on { INK2 } else { INK3 };
        p.text_in(
            Face::Bold,
            24,
            y + 22,
            28,
            &(part + 1).to_string(),
            if row_focused && on { BLUE } else { ink },
        );
        // The fields inside the row, each boxed while focused or grabbed.
        let boxes = [
            Rect(58, y + 16, 50, 44),
            Rect(112, y + 8, 264, 60),
            Rect(584, y + 12, 116, 52),
            Rect(708, y + 16, 74, 44),
        ];
        for (index, r) in boxes.iter().enumerate() {
            let field = part_field(part, index);
            let grabbed = grab.is_some_and(|g| g.field == field);
            if grabbed {
                p.round_rect(*r, 4, AMBER);
                p.round_rect(Rect(r.0 + 2, r.1 + 2, r.2 - 4, r.3 - 4), 3, AMBER_WASH);
            } else if inside == Some(index) {
                p.round_rect(*r, 4, BLUE);
                p.round_rect(Rect(r.0 + 2, r.1 + 2, r.2 - 4, r.3 - 4), 3, RAISED);
            }
        }
        p.text_in(
            Face::Condensed,
            66,
            y + 26,
            20,
            if on { "On" } else { "Off" },
            if on { BLUE } else { INK3 },
        );
        let preset_grab = grab.filter(|g| g.field == Field::PartPreset(part));
        let shown = preset_grab.map_or(slot, |g| g.value as usize);
        let name = format!("{:02}  {}", shown + 1, self.patches[shown].name);
        let preset_color = if preset_grab.is_some() { AMBER } else { ink };
        if preset_grab.is_some() {
            p.fit_in(
                Face::Bold,
                122,
                y + 16,
                20,
                244,
                &format!("\u{2039} {name} \u{203a}"),
                preset_color,
            );
        } else {
            p.fit_in(Face::Condensed, 122, y + 16, 20, 244, &name, preset_color);
        }
        p.fit_in(
            Face::Condensed,
            122,
            y + 42,
            16,
            244,
            &self.patches[shown].sample,
            ink2,
        );
        let sample = &self.patches[shown].sample;
        let empty = sample.is_empty()
            || sample.eq_ignore_ascii_case("empty.wav")
            || sample == "Empty sample.wav";
        p.rect(Rect(388, y + 14, 186, 48), WELL);
        self.mini_wave(p, Rect(390, y + 16, 182, 44), shown, empty, on);
        // Level.
        let level_grab = grab.filter(|g| g.field == Field::PartLevel(part));
        let level = level_grab.map_or(self.volumes[part], |g| g.value);
        let level_color = if level_grab.is_some() { AMBER } else { ink };
        p.text_in(
            if level_grab.is_some() {
                Face::Bold
            } else {
                Face::Regular
            },
            594,
            y + 18,
            22,
            &format!("{level:.0}%"),
            level_color,
        );
        p.rect(Rect(594, y + 48, 96, 5), OUTLINE);
        p.rect(
            Rect(594, y + 48, (96. * level / 100.).round() as i32, 5),
            if level_grab.is_some() {
                AMBER
            } else if on {
                BLUE
            } else {
                INK3
            },
        );
        if let Some(g) = level_grab {
            p.rect(
                Rect(
                    594 + (96. * g.original / 100.).round() as i32,
                    y + 45,
                    1,
                    11,
                ),
                INK,
            );
        }
        // MIDI channel; Off disables MIDI for the part (manual p.66).
        let channel_grab = grab.filter(|g| g.field == Field::PartChannel(part));
        let channel = channel_grab.map_or(self.get(&format!("midi{}", part + 1)), |g| g.value);
        let text = self.perf_field_text(Field::PartChannel(part), channel);
        p.text_right(
            if channel_grab.is_some() {
                Face::Bold
            } else {
                Face::Condensed
            },
            774,
            y + 26,
            20,
            &text,
            if channel_grab.is_some() {
                AMBER
            } else if channel < 0.5 {
                INK3
            } else {
                ink
            },
        );
    }

    // ----- Files  -----

    fn draw_files(&mut self, p: &mut Painter) {
        self.perf_files_relist();
        let files = &self.perf.files;
        let left = if files.at_locations {
            "Locations".to_string()
        } else {
            self.place_title(&overlay::Place::Folder(self.directory.clone()))
        };
        let clip = match self.clipboard.len() {
            0 => "Clipboard empty".to_string(),
            1 => "Clipboard 1 item".to_string(),
            n => format!("Clipboard {n} items"),
        };
        let right = if files.at_locations {
            clip
        } else {
            format!("{} selected     {clip}", files.selection.len())
        };
        let color = if files.selection.is_empty() && self.clipboard.is_empty() {
            INK3
        } else {
            BLUE
        };
        self.perf_subheader(p, &left, &right, color);
        let files = &self.perf.files;
        let empty = (!files.at_locations && files.rows.is_empty())
            .then_some(("This folder is empty", false));
        let device = self.device.is_some();
        let mut cursor = files.list;
        if self.perf.strip {
            cursor.focus = usize::MAX;
        }
        components::draw_list(p, cursor, files.rows.len(), empty, |i, focused| {
            let row = &files.rows[i];
            let audio = overlay::is_audio(&row.key);
            ListRow {
                glyph: if row.folder {
                    Glyph::Folder
                } else if audio {
                    Glyph::Wave
                } else {
                    Glyph::Document
                },
                label: row.label.clone(),
                right: row.count.clone(),
                second: if !focused {
                    String::new()
                } else if row.location.is_some() {
                    String::new()
                } else if row.folder {
                    match row.count.as_str() {
                        "1" => "1 item".into(),
                        n => format!("{n} items"),
                    }
                } else {
                    file_detail(device, &row.key)
                },
                current: false,
                dim: false,
                keycap: None,
                check: row
                    .location
                    .is_none()
                    .then(|| files.selection.contains(&row.key)),
                press: row.folder.then_some("Open"),
            }
        });
    }

    // ----- System  -----

    fn draw_system(&mut self, p: &mut Painter) {
        let sections = self.perf_system_rows();
        // Display rows: (top, height, section or field index).
        let mut rows: Vec<(i32, i32, Result<&str, (usize, Field)>)> = vec![];
        let mut y = 0;
        let mut index = 0;
        for (name, fields) in &sections {
            rows.push((y, SECTION_ROW, Ok(*name)));
            y += SECTION_ROW;
            for field in fields {
                rows.push((y, FIELD_ROW, Err((index, *field))));
                y += FIELD_ROW;
                index += 1;
            }
        }
        let total = y;
        let focus = self.perf.system.min(index.saturating_sub(1));
        let at = rows
            .iter()
            .position(|r| matches!(r.2, Err((i, _)) if i == focus))
            .unwrap_or(0);
        // Scroll by the least amount that shows the focused row, with its
        // section name when it is the section's first field.
        let view = LIST_BOTTOM_SYSTEM - SYSTEM_TOP;
        let mut top = self.perf.system_top.min(rows.len().saturating_sub(1));
        let first = if at > 0 && matches!(rows[at - 1].2, Ok(_)) {
            at - 1
        } else {
            at
        };
        if first < top {
            top = first;
        }
        while rows[at].0 + rows[at].1 - rows[top].0 > view && top < at {
            top += 1;
        }
        self.perf.system_top = top;
        let offset = rows[top].0;
        p.clip(Some(Rect(0, SYSTEM_TOP, 800, view)));
        let grab = self.perf.grab;
        for (row_y, height, what) in &rows {
            let y = SYSTEM_TOP + row_y - offset;
            if y + height <= SYSTEM_TOP || y >= LIST_BOTTOM_SYSTEM {
                continue;
            }
            match what {
                Ok(name) => {
                    let end = p.text_in(Face::Condensed, 24, y + 10, 16, name, INK2);
                    p.rect(Rect(end + 12, y + 20, 776 - end - 12, 1), RULE);
                }
                Err((i, field)) => {
                    let focused = *i == focus && !self.perf.strip;
                    let grabbed = grab.filter(|g| g.field == *field);
                    self.draw_field_row(p, y, *field, focused, grabbed);
                }
            }
        }
        p.clip(None);
        if total > view {
            let track = view;
            p.rect(Rect(790, SYSTEM_TOP, 3, track), RULE);
            let thumb = (track * track / total).max(24);
            let at = SYSTEM_TOP + (track - thumb) * offset / (total - view).max(1);
            p.rect(
                Rect(790, at.min(SYSTEM_TOP + track - thumb), 3, thumb),
                INK2,
            );
        }
    }
    /// A System row on the shared field row : label and value,
    /// a grabbed choice's dots, a grabbed number's range and restore tick.
    fn draw_field_row(
        &mut self,
        p: &mut Painter,
        y: i32,
        field: Field,
        focused: bool,
        grab: Option<Grab>,
    ) {
        let label = match field {
            Field::Setting(i) => self.catalog[i].name.clone(),
            Field::Appearance => "Appearance".into(),
            Field::Brightness => "Brightness".into(),
            Field::Disk => "Disk free".into(),
            Field::Firmware => "Firmware information".into(),
            Field::Reset => "Reset settings".into(),
            Field::Shutdown => "Shut down".into(),
            _ => String::new(),
        };
        let kind = self.perf_field_kind(field);
        let value = grab.map_or(self.perf_field_value(field), |g| g.value);
        let mut row = components::Field::plain(&label, self.perf_field_text(field, value));
        row.state = match (grab.is_some(), focused) {
            (true, _) => components::FieldState::Grabbed,
            (false, true) => components::FieldState::Focused,
            _ => components::FieldState::Normal,
        };
        row.on = matches!(kind, FieldKind::Toggle) && value > 0.;
        if let Some(g) = grab {
            match kind {
                FieldKind::Choice(options) => {
                    row.dots = Some((options.len(), g.value.round() as usize));
                }
                FieldKind::Number { min, max, .. } if max > min => {
                    let frac = |v: f32| (v - min) / (max - min);
                    row.range = Some((frac(g.value), frac(g.original)));
                }
                _ => {}
            }
        }
        let r = Rect(14, y + 2, 770, FIELD_ROW - 4);
        components::draw_field(p, r, &row);
        // Disk free turns red when low .
        if let (Field::Disk, Some(Ok(bytes))) = (field, &self.perf.free) {
            if *bytes < LOW_DISK {
                let text_y = r.1 + (r.3 - 24) / 2;
                let right = r.0 + r.2 - 14;
                let w = p.width_in(Face::Regular, &row.value, 20).round() as i32;
                let bg = if focused { RAISED } else { BACKGROUND };
                p.rect(Rect(right - w - 2, r.1 + 2, w + 4, r.3 - 4), bg);
                p.text_right(Face::Regular, right, text_y, 20, &row.value, RED);
            }
        }
    }

    // ----- sheets and confirmations -----

    /// The root's sheets beside the focused row, and confirmations in their
    /// place; over the main screen a confirmation is centered.
    pub(super) fn draw_perf_layers(&mut self, p: &mut Painter) {
        let (x, row_y) = self.perf_layer_anchor();
        for layer in &self.perf.layers {
            match layer {
                Layer::Sheet(s) => {
                    let lines: Vec<SheetLine> = s
                        .items
                        .iter()
                        .map(|i| SheetLine {
                            label: &i.label,
                            keycap: i.keycap,
                            destructive: i.destructive,
                        })
                        .collect();
                    components::draw_sheet(p, x, &s.title, &lines, s.focus, row_y);
                }
                Layer::Ask(a) => components::draw_question(
                    p,
                    x,
                    &a.lines,
                    &a.loss,
                    &a.verb_label,
                    a.keycap,
                    a.on_verb,
                ),
            }
        }
    }
    /// Where sheets open: beside the focused row or cell, never over it.
    fn perf_layer_anchor(&self) -> (i32, i32) {
        if self.tab != 4 {
            return (234, 150);
        }
        match self.perf.page {
            PERFORMANCES => (452, self.perf.slots.focus_y()),
            FILES => (452, self.perf.files.list.focus_y()),
            PRESETS => {
                let r = cell_rect(self.perf.cell);
                (if r.0 < 400 { 452 } else { 16 }, r.1)
            }
            PARTS => (452, 100 + self.perf.part_row.saturating_sub(1) as i32 * 80),
            _ => (234, 150),
        }
    }
}

const LIST_BOTTOM_SYSTEM: i32 = 424;

/// The cell of preset `cell` in the 4x8 grid laid out like the panel's bank
/// and preset buttons .
pub(super) fn cell_rect(cell: usize) -> Rect {
    let (bank, preset) = ((cell / 8) as i32, (cell % 8) as i32);
    Rect(48 + preset * 92, 52 + bank * 92, 88, 88)
}

/// A name on up to two lines at word boundaries, cut at the end.
fn name_lines(p: &Painter, name: &str, width: i32) -> Vec<String> {
    let mut lines: Vec<String> = vec![];
    for word in name.split_whitespace() {
        let joined = lines.last().map(|line| format!("{line} {word}"));
        match joined {
            Some(joined)
                if lines.len() == 2 || p.width_in(Face::Condensed, &joined, 20) <= width as f32 =>
            {
                *lines.last_mut().unwrap() = joined;
            }
            _ => lines.push(word.into()),
        }
    }
    lines
}

/// A file's second line: its type, and a WAV's length.
fn file_detail(device: bool, name: &str) -> String {
    let kind = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_uppercase())
        .unwrap_or_default();
    let _ = device;
    kind
}
