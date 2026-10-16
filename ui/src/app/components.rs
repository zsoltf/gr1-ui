//! Shared panel components of the redesign : the
//! continuous list, the screen header, keycaps and chips, the action sheet,
//! the confirmation and the name editor. They draw what they are given and
//! hold no instrument state, so the later Patch, Perf and Sample roots build
//! on the same pieces.
use super::overlay::{ActionSheet, Confirm, NameEditor};
use super::*;

/// The list well between the header (with its path row) and the status bar.
pub(super) const LIST_TOP: i32 = 76;
pub(super) const LIST_BOTTOM: i32 = 424;
/// Rows are 40 pixels; the focused row grows and shows a second line.
pub(super) const ROW: i32 = 40;
const FOCUSED_ROW: i32 = 62;
/// Rows fully visible with one expanded row .
pub(super) const VISIBLE_ROWS: usize = 8;

/// Focus and view of a continuous list. Turning moves focus by row and
/// never selects; the view moves only when focus would leave it (3.5.1).
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) struct ListCursor {
    pub focus: usize,
    /// First visible row.
    pub top: usize,
}
impl ListCursor {
    pub(super) fn turn(&mut self, delta: i32, len: usize) {
        let last = len.saturating_sub(1) as i32;
        self.focus = (self.focus as i32 + delta).clamp(0, last) as usize;
        self.reveal(len);
    }
    /// Scroll by the least amount that shows the focused row; never recenter.
    pub(super) fn reveal(&mut self, len: usize) {
        self.focus = self.focus.min(len.saturating_sub(1));
        if self.focus < self.top {
            self.top = self.focus;
        } else if self.focus >= self.top + VISIBLE_ROWS {
            self.top = self.focus + 1 - VISIBLE_ROWS;
        }
        self.top = self.top.min(len.saturating_sub(VISIBLE_ROWS));
    }
    /// The focused row's top edge on screen.
    pub(super) fn focus_y(&self) -> i32 {
        LIST_TOP + (self.focus as i32 - self.top as i32) * ROW
    }
}

pub(super) enum Glyph {
    Folder,
    Wave,
    Document,
    /// A slot or preset number in place of an icon.
    Number(usize),
}

/// One row as the list shows it.
pub(super) struct ListRow {
    pub glyph: Glyph,
    pub label: String,
    /// Right column: count, length, annotation.
    pub right: String,
    /// The focused row's second line.
    pub second: String,
    /// The loaded or working item carries a blue "Current".
    pub current: bool,
    /// Empty slots and inactive rows.
    pub dim: bool,
    /// The panel button that acts on the focused row.
    pub keycap: Option<&'static str>,
    /// A selectable row's check box (Files): Some(selected).
    pub check: Option<bool>,
    /// The press's action, shown at the focused row's end when it is not
    /// obvious ("Open" on a folder whose press opens it).
    pub press: Option<&'static str>,
}

/// Draw a continuous list; `row(i, focused)` describes the visible rows.
pub(super) fn draw_list(
    p: &mut Painter,
    cursor: ListCursor,
    len: usize,
    empty: Option<(&str, bool)>,
    mut row: impl FnMut(usize, bool) -> ListRow,
) {
    let bounds = Rect(0, LIST_TOP, 800, LIST_BOTTOM - LIST_TOP);
    p.clip(Some(bounds));
    if let Some((text, error)) = empty {
        p.fit_in(
            Face::Condensed,
            24,
            LIST_TOP + 10,
            20,
            750,
            text,
            if error { RED } else { INK2 },
        );
    }
    let mut y = LIST_TOP;
    for i in cursor.top..len {
        if y >= LIST_BOTTOM {
            break;
        }
        let focused = i == cursor.focus;
        let r = row(i, focused);
        let top = if focused { y + 2 } else { y };
        if focused {
            p.round_rect(Rect(8, top, 776, 58), 3, RAISED);
            p.rect(Rect(8, top, 3, 58), BLUE);
        }
        let text_y = top + 6;
        // A selected row is tinted and its box checked .
        let shift = match r.check {
            Some(selected) => {
                if selected {
                    let h = if focused { 58 } else { ROW - 4 };
                    p.round_rect(Rect(11, top + 1, 773, h - 2), 3, BLUE_WASH);
                }
                let color = if selected { BLUE } else { INK2 };
                p.outline(Rect(24, text_y + 3, 16, 16), 1, color);
                if selected {
                    p.stroke(
                        &[
                            (27., (text_y + 11) as f32),
                            (31., (text_y + 15) as f32),
                            (37., (text_y + 6) as f32),
                        ],
                        2,
                        BLUE,
                    );
                }
                26
            }
            None => 0,
        };
        let label_x = shift
            + match r.glyph {
                Glyph::Number(n) => {
                    p.text_in(
                        Face::Condensed,
                        24,
                        text_y + 4,
                        16,
                        &format!("{n:02}"),
                        INK2,
                    );
                    56
                }
                Glyph::Folder => {
                    folder_icon(p, 24 + shift, text_y + 8, INK2);
                    50
                }
                Glyph::Wave => {
                    wave_icon(p, 24 + shift, text_y + 7, INK2);
                    50
                }
                Glyph::Document => {
                    p.outline(Rect(26 + shift, text_y + 6, 12, 15), 1, INK2);
                    p.rect(Rect(29 + shift, text_y + 11, 6, 1), INK2);
                    p.rect(Rect(29 + shift, text_y + 15, 6, 1), INK2);
                    50
                }
            };
        // The right column: the keycap of the press, then the detail.
        let mut right = 772;
        if let (true, Some(key)) = (focused, r.keycap) {
            right = keycap(p, 772, top + 9, key, BLUE, RAISED) - 12;
        }
        if let (true, Some(press)) = (focused, r.press) {
            right = p.text_right(Face::Bold, right, text_y + 3, 16, press, BLUE) - 12;
        }
        if !r.right.is_empty() {
            right = p.text_right(Face::Condensed, right, text_y + 4, 16, &r.right, INK2) - 12;
        }
        let end = p.fit_in(
            if focused { Face::Bold } else { Face::Condensed },
            label_x,
            text_y,
            20,
            right - label_x - if r.current { 72 } else { 0 },
            &r.label,
            if r.dim { INK3 } else { INK },
        );
        if r.current {
            p.text_in(Face::Condensed, end + 10, text_y + 4, 16, "Current", BLUE);
        }
        if focused {
            p.fit_in(Face::Condensed, label_x, top + 30, 16, 700, &r.second, INK2);
        }
        y += if focused { FOCUSED_ROW } else { ROW };
    }
    p.clip(None);
    // A scrollbar shows the visible share; there are no pages.
    if len > VISIBLE_ROWS {
        let track = LIST_BOTTOM - LIST_TOP;
        p.rect(Rect(790, LIST_TOP, 3, track), RULE);
        let thumb = (track * VISIBLE_ROWS as i32 / len as i32).max(24);
        let span = (len - VISIBLE_ROWS).max(1);
        let at = LIST_TOP + (track - thumb) * cursor.top.min(span) as i32 / span as i32;
        p.rect(Rect(790, at, 3, thumb), INK2);
    }
}

/// The screen header: verb in bold blue, the rest in ink, optional chips at
/// the right, then the rule at 44.
pub(super) fn draw_header(p: &mut Painter, verb: &str, rest: &str, chips: &[(&str, bool)]) {
    let end = p.text_in(Face::Bold, 16, 9, 20, verb, BLUE);
    let limit = if chips.is_empty() {
        784
    } else {
        784 - chips.len() as i32 * 70 - 16
    };
    p.fit_in(Face::Condensed, end + 8, 9, 20, limit - end - 8, rest, INK);
    let first = 784 - chips.len() as i32 * 70 + 6;
    for (i, (label, lit)) in chips.iter().enumerate() {
        chip(p, Rect(first + i as i32 * 70, 8, 64, 28), label, *lit);
    }
    p.rect(Rect(0, 44, 800, 1), RULE);
}

/// A panel keycap: the button's legend in a thin box ending at `right`, on
/// the background `bg`. Returns its left edge.
pub(super) fn keycap(
    p: &mut Painter,
    right: i32,
    top: i32,
    label: &str,
    color: Color,
    bg: Color,
) -> i32 {
    let width = p.width_in(Face::Condensed, label, 14).round() as i32 + 12;
    let r = Rect(right - width, top, width, 20);
    p.round_rect(r, 3, color);
    p.round_rect(Rect(r.0 + 1, r.1 + 1, r.2 - 2, r.3 - 2), 2, bg);
    p.text_in(Face::Condensed, r.0 + 6, top + 1, 14, label, color);
    r.0
}

/// A type chip; the lit one mirrors the panel's lit button.
pub(super) fn chip(p: &mut Painter, r: Rect, label: &str, lit: bool) {
    if lit {
        p.round_rect(r, 3, BLUE);
        p.round_rect(Rect(r.0 + 1, r.1 + 1, r.2 - 2, r.3 - 2), 2, BLUE_WASH);
    } else {
        p.round_rect(r, 3, OUTLINE);
        p.round_rect(Rect(r.0 + 1, r.1 + 1, r.2 - 2, r.3 - 2), 2, BACKGROUND);
    }
    let width = p.width_in(Face::Condensed, label, 16).round() as i32;
    p.text_in(
        Face::Condensed,
        r.0 + (r.2 - width) / 2,
        r.1 + (r.3 - 20) / 2,
        16,
        label,
        if lit { BLUE } else { INK2 },
    );
}

/// A button of a sheet or confirmation; focus is a blue frame on blue wash.
pub(super) fn button(
    p: &mut Painter,
    r: Rect,
    label: &str,
    focused: bool,
    color: Color,
    key: Option<(&str, Color)>,
) {
    if focused {
        p.round_rect(r, 4, BLUE);
        p.round_rect(Rect(r.0 + 2, r.1 + 2, r.2 - 4, r.3 - 4), 3, BLUE_WASH);
    } else {
        p.round_rect(r, 4, OUTLINE);
        p.round_rect(Rect(r.0 + 1, r.1 + 1, r.2 - 2, r.3 - 2), 3, RAISED);
    }
    p.text_in(
        if focused { Face::Bold } else { Face::Condensed },
        r.0 + 16,
        r.1 + 13,
        20,
        label,
        color,
    );
    if let Some((key, key_color)) = key {
        let bg = if focused { BLUE_WASH } else { RAISED };
        keycap(p, r.0 + r.2 - 16, r.1 + 16, key, key_color, bg);
    }
}

/// One action of a sheet as drawn.
pub(super) struct SheetLine<'a> {
    pub label: &'a str,
    pub keycap: Option<&'a str>,
    pub destructive: bool,
}

/// The action sheet beside the focused row at `row_y` .
pub(super) fn draw_action_sheet(p: &mut Painter, s: &ActionSheet, row_y: i32) {
    let lines: Vec<SheetLine> = s
        .items
        .iter()
        .map(|i| SheetLine {
            label: &i.label,
            keycap: i.keycap,
            destructive: i.destructive,
        })
        .collect();
    draw_sheet(p, 452, &s.title, &lines, s.focus, row_y);
}

/// A sheet of at most six actions with its left edge at `x`, beside the row
/// at `row_y`: the title, the primary action first, a separator before the
/// destructive ones and the keycaps of panel buttons that also perform them.
pub(super) fn draw_sheet(
    p: &mut Painter,
    x: i32,
    title: &str,
    lines: &[SheetLine],
    focus: usize,
    row_y: i32,
) {
    let separator = lines.iter().position(|i| i.destructive);
    let height = 52 + lines.len() as i32 * 40 + if separator.is_some() { 8 } else { 0 };
    let top = (row_y - 24).clamp(52, LIST_BOTTOM - 8 - height);
    let r = Rect(x, top, 332, height);
    p.round_rect(r, 4, OUTLINE);
    p.round_rect(Rect(r.0 + 1, r.1 + 1, r.2 - 2, r.3 - 2), 3, RAISED);
    p.fit_in(Face::Condensed, x + 16, top + 8, 16, 300, title, INK2);
    let mut y = top + 38;
    for (i, item) in lines.iter().enumerate() {
        if Some(i) == separator {
            p.rect(Rect(x + 12, y + 2, 308, 1), OUTLINE);
            y += 8;
        }
        let focused = i == focus;
        if focused {
            p.round_rect(Rect(x + 6, y, 320, 36), 3, BLUE_WASH);
            p.rect(Rect(x + 6, y, 3, 36), BLUE);
        }
        p.fit_in(
            if focused { Face::Bold } else { Face::Condensed },
            x + 18,
            y + 6,
            20,
            if item.keycap.is_some() { 230 } else { 300 },
            item.label,
            if item.destructive { RED } else { INK },
        );
        if let Some(key) = item.keycap {
            keycap(
                p,
                x + 314,
                y + 8,
                key,
                BLUE,
                if focused { BLUE_WASH } else { RAISED },
            );
        }
        y += 40;
    }
}

/// A confirmation : what happens, what is lost, Cancel and the
/// verb. Beside a list row it takes the sheet's place; else it is centered.
pub(super) fn draw_confirm(p: &mut Painter, c: &Confirm, beside_row: bool) {
    let x = if beside_row { 452 } else { 234 };
    draw_question(p, x, &c.lines, &c.loss, &c.verb, c.keycap, c.on_verb);
}

/// The confirmation's box with its left edge at `x`; Cancel or the verb
/// (`on_verb`) is focused, and `keycap` names the panel button that confirms.
pub(super) fn draw_question(
    p: &mut Painter,
    x: i32,
    lines: &[String],
    loss: &str,
    verb: &str,
    keycap: &str,
    on_verb: bool,
) {
    let r = Rect(x, 150, 332, 220);
    p.round_rect(r, 4, OUTLINE);
    p.round_rect(Rect(r.0 + 1, r.1 + 1, r.2 - 2, r.3 - 2), 3, RAISED);
    for (i, line) in lines.iter().enumerate().take(3) {
        p.fit_in(
            if i == 0 { Face::Bold } else { Face::Condensed },
            x + 18,
            164 + i as i32 * 26,
            20,
            296,
            line,
            INK,
        );
    }
    for (i, line) in wrap(p, loss, 296).iter().take(2).enumerate() {
        p.text_in(Face::Condensed, x + 18, 250 + i as i32 * 19, 16, line, INK2);
    }
    button(
        p,
        Rect(x + 12, 298, 140, 52),
        "Cancel",
        !on_verb,
        if on_verb { INK } else { BLUE },
        None,
    );
    let key = (!keycap.is_empty()).then_some((keycap, RED));
    button(p, Rect(x + 164, 298, 156, 52), verb, on_verb, RED, key);
}

/// The name editor : the name with its insert cursor, the
/// character strip around the focused item, and the finishing keys.
pub(super) fn draw_name_editor(p: &mut Painter, n: &NameEditor, strip: usize, save: &str) {
    use super::overlay::{strip_group_starts, strip_item, strip_len};
    p.text_in(Face::Bold, 16, 9, 20, &n.title, BLUE);
    p.rect(Rect(0, 44, 800, 1), RULE);
    p.text_in(Face::Regular, 16, 62, 22, "Name", INK);
    p.text_right(
        Face::Condensed,
        784,
        68,
        16,
        &format!("{} / {}", n.chars.len(), n.limit),
        INK3,
    );
    p.round_rect(Rect(16, 100, 768, 70), 4, WELL);
    // Keep the cursor inside the well.
    let before: String = n.chars[..n.cursor].iter().collect();
    let width = p.width_in(Face::Regular, &before, 30).round() as i32;
    let shift = (width - 700).max(0);
    p.clip(Some(Rect(20, 100, 760, 70)));
    let text: String = n.chars.iter().collect();
    p.text_in(Face::Regular, 36 - shift, 118, 30, &text, INK);
    p.rect(Rect(36 - shift + width, 116, 3, 40), AMBER);
    p.clip(None);
    // The strip: the focused item enlarged in amber, neighbors fading.
    let widths: Vec<i32> = (0..strip_len())
        .map(|i| match strip_item(i) {
            Ok(word) => p.width_in(Face::Condensed, word, 18).round() as i32 + 22,
            Err(_) => 34,
        })
        .collect();
    let starts = strip_group_starts();
    let mut centers = vec![0; strip_len()];
    centers[strip] = 400;
    // The focused item is wider than its neighbors by its frame.
    let pad = |i: usize| if i == strip { 11 } else { 0 };
    for i in (0..strip).rev() {
        centers[i] = centers[i + 1] - (widths[i] + widths[i + 1]) / 2 - pad(i + 1);
    }
    for i in strip + 1..strip_len() {
        centers[i] = centers[i - 1] + (widths[i] + widths[i - 1]) / 2 + pad(i - 1);
    }
    for (i, center) in centers.iter().enumerate() {
        if !(-40..840).contains(center) {
            continue;
        }
        if starts.contains(&i) && i > 0 {
            let x = center - widths[i] / 2 - pad(i);
            p.rect(Rect(x, 222, 1, 44), OUTLINE);
        }
        let distance = (i as i32 - strip as i32).unsigned_abs();
        if i == strip {
            let w = widths[i].max(56);
            p.round_rect(Rect(center - w / 2, 206, w, 76), 4, AMBER);
            p.round_rect(Rect(center - w / 2 + 2, 208, w - 4, 72), 3, AMBER_WASH);
        }
        let color = if i == strip {
            AMBER
        } else if distance <= 3 {
            INK
        } else if distance <= 6 {
            INK2
        } else {
            INK3
        };
        let (label, face, size, y) = match strip_item(i) {
            Ok(word) if i == strip => (word.to_string(), Face::Bold, 20, 233),
            Ok(word) => (word.to_string(), Face::Condensed, 18, 234),
            Err(ch) if i == strip => (ch.to_string(), Face::Bold, 32, 226),
            Err(ch) => (ch.to_string(), Face::Regular, 22, 232),
        };
        let w = p.width_in(face, &label, size).round() as i32;
        p.text_in(face, center - w / 2, y, size, &label, color);
    }
    let mut x = 24;
    for (key, label, color) in [
        ("Save", save, BLUE),
        ("Esc", "Cancel", INK2),
        ("Shift Esc", "Delete", INK2),
    ] {
        let width = p.width_in(Face::Condensed, key, 14).round() as i32 + 12;
        keycap(p, x + width, 340, key, color, BACKGROUND);
        x = p.text_in(Face::Condensed, x + width + 8, 338, 16, label, INK2) + 24;
    }
}

/// A menu root's header : the root's name, then its pages.
/// The current page is bold with a 3-pixel blue underline; with page-strip
/// focus it also stands on a blue background. Returns each page's box.
pub(super) fn draw_page_strip(
    p: &mut Painter,
    root: &str,
    pages: &[&str],
    current: usize,
    strip_focus: bool,
) -> Vec<Rect> {
    let mut x = p.text_in(Face::Bold, 16, 10, 20, root, BLUE) + 22;
    let widths: Vec<i32> = pages
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let face = if i == current {
                Face::Bold
            } else {
                Face::Condensed
            };
            p.width_in(face, name, 17).round() as i32
        })
        .collect();
    // Spread the names over the rest of the header.
    let gap = ((784 - x - widths.iter().sum::<i32>()) / pages.len() as i32).clamp(12, 40);
    let mut boxes = vec![];
    for (i, name) in pages.iter().enumerate() {
        let r = Rect(x - gap / 2, 0, widths[i] + gap, 44);
        if i == current {
            if strip_focus {
                p.round_rect(Rect(x - 8, 6, widths[i] + 16, 32), 3, BLUE_WASH);
            }
            p.text_in(
                Face::Bold,
                x,
                12,
                17,
                name,
                if strip_focus { BLUE } else { INK },
            );
            p.rect(Rect(x, 40, widths[i], 3), BLUE);
        } else {
            p.text_in(Face::Condensed, x, 12, 17, name, INK2);
        }
        boxes.push(r);
        x += widths[i] + gap;
    }
    p.rect(Rect(0, 44, 800, 1), RULE);
    boxes
}

/// How a field row is focused .
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum FieldState {
    Normal,
    /// Moving focus: a raised row with a blue bar.
    Focused,
    /// Grabbed: an amber row, the value between angle marks.
    Grabbed,
}

/// One single-line field: label left, value right .
pub(super) struct Field<'a> {
    pub label: String,
    pub value: String,
    pub state: FieldState,
    /// Inactive but focusable (3.5.7).
    pub dim: bool,
    /// On toggles read in blue.
    pub on: bool,
    /// Panel keycaps after the label (FX-1, FX-2).
    pub keycaps: &'a [&'a str],
    /// A grabbed number: (value, restore value) as fractions of the range.
    pub range: Option<(f32, f32)>,
    /// A grabbed choice: (option count, the option shown).
    pub dots: Option<(usize, usize)>,
}
impl Field<'_> {
    pub(super) fn plain(label: &str, value: String) -> Field<'static> {
        Field {
            label: label.into(),
            value,
            state: FieldState::Normal,
            dim: false,
            on: false,
            keycaps: &[],
            range: None,
            dots: None,
        }
    }
}

/// Draw a field row in `r` (its height is the row's, usually 40).
pub(super) fn draw_field(p: &mut Painter, r: Rect, f: &Field) {
    let (row_bg, bar) = match f.state {
        FieldState::Normal => (None, None),
        FieldState::Focused => (Some(RAISED), Some(BLUE)),
        FieldState::Grabbed => (Some(AMBER_WASH), Some(AMBER)),
    };
    if let Some(bg) = row_bg {
        p.round_rect(r, 3, bg);
    }
    if let Some(bar) = bar {
        p.rect(Rect(r.0, r.1, 3, r.3), bar);
    }
    let text_y = r.1 + (r.3 - 24) / 2;
    let label_color = if f.dim { INK3 } else { INK2 };
    let mut x = p.fit_in(
        Face::Condensed,
        r.0 + 14,
        text_y,
        20,
        r.2 / 2 + 40,
        &f.label,
        if f.state == FieldState::Normal {
            label_color
        } else {
            INK
        },
    );
    for key in f.keycaps {
        let width = p.width_in(Face::Condensed, key, 14).round() as i32 + 12;
        let bg = row_bg.unwrap_or(BACKGROUND);
        keycap(p, x + 10 + width, r.1 + (r.3 - 20) / 2, key, BLUE, bg);
        x += width + 6;
    }
    let right = r.0 + r.2 - 14;
    if f.state == FieldState::Grabbed {
        let close = p.text_right(Face::Bold, right, text_y, 20, "\u{203a}", AMBER);
        let start = p.text_right(Face::Bold, close - 10, text_y, 20, &f.value, AMBER);
        p.text_right(Face::Bold, start - 10, text_y, 20, "\u{2039}", AMBER);
        if let Some((count, at)) = f.dots {
            // Under the value, centred: how many options and which is shown.
            let width = count as i32 * 8;
            let middle = (start + close) / 2;
            for i in 0..count {
                let cx = (middle - width / 2 + 4 + i as i32 * 8) as f32;
                let color = if i == at { AMBER } else { OUTLINE };
                p.disc(cx, (r.1 + r.3 - 5) as f32, 2.5, color);
            }
        }
        if let Some((value, restore)) = f.range {
            let (x0, w) = (r.0 + 14, r.2 - 28);
            let y = r.1 + r.3 - 4;
            p.rect(Rect(x0, y, w, 2), OUTLINE);
            p.rect(
                Rect(x0, y, (w as f32 * value.clamp(0., 1.)).round() as i32, 2),
                AMBER,
            );
            let tick = x0 + (w as f32 * restore.clamp(0., 1.)).round() as i32;
            p.rect(Rect(tick - 1, y - 4, 2, 8), INK2);
        }
    } else {
        let color = if f.dim {
            INK3
        } else if f.on {
            BLUE
        } else {
            INK
        };
        p.text_right(Face::Regular, right, text_y, 20, &f.value, color);
    }
}

/// A group's title above its fields.
pub(super) fn group_title(p: &mut Painter, x: i32, y: i32, title: &str) {
    p.text_in(Face::Condensed, x + 14, y, 16, title, INK2);
}

/// The live strip's content : the focused part's
/// waveform downsampled by column extremes, its loop, spray, playhead and
/// grains, all as fractions of the sample.
pub(super) struct StripView {
    /// Per strip column, (min, max) in 0..1 with 0.5 as silence.
    pub columns: Vec<(f32, f32)>,
    pub loop_range: (f32, f32),
    pub spray: Vec<(f32, f32)>,
    pub scan: Option<f32>,
    pub grains: Vec<(f32, f32)>,
    /// The recording position while this buffer records.
    pub wiper: Option<f32>,
}

/// Columns of `width` pixels from a cached stencil, each spanning the
/// extremes of the stencil columns under it.
pub(super) fn strip_columns(stencil: &[(f32, f32)], width: usize) -> Vec<(f32, f32)> {
    let n = stencil.len();
    (0..width)
        .map(|j| {
            let (a, b) = (j * n / width, ((j + 1) * n / width).max(j * n / width + 1));
            stencil[a.min(n)..b.min(n)]
                .iter()
                .filter(|(lo, hi)| lo.is_finite() && hi.is_finite())
                .fold((f32::NAN, f32::NAN), |(lo, hi), (a, b)| {
                    (lo.min(a.min(*b)), hi.max(a.max(*b)))
                })
        })
        .collect()
}

pub(super) fn draw_strip(p: &mut Painter, r: Rect, s: &StripView) {
    p.clip(Some(r));
    let x_of = |fraction: f32| r.0 as f32 + fraction * (r.2 - 1) as f32;
    for (lo, hi) in &s.spray {
        let x = r.0 + (lo * r.2 as f32) as i32;
        p.rect(
            Rect(x, r.1, ((hi - lo) * r.2 as f32).ceil() as i32, r.3),
            SPRAY_BAND,
        );
    }
    let center = r.1 as f32 + r.3 as f32 / 2.;
    let half = r.3 as f32 * 0.46;
    let count = s.columns.len().max(1);
    for (i, (lo, hi)) in s.columns.iter().copied().enumerate() {
        if !lo.is_finite() || !hi.is_finite() {
            continue;
        }
        let fraction = (i as f32 + 0.5) / count as f32;
        let tone = if fraction < s.loop_range.0 || fraction > s.loop_range.1 {
            OUTSIDE_LOOP
        } else if s
            .spray
            .iter()
            .any(|(a, b)| fraction >= *a && fraction <= *b)
        {
            SPOTLIGHT
        } else {
            WAVEFORM
        };
        let top = (center - (hi - 0.5) * 2. * half).round() as i32;
        let bottom = (center - (lo - 0.5) * 2. * half).round() as i32;
        let x = r.0 + (i * r.2 as usize / count) as i32;
        p.rect(Rect(x, top.min(bottom), 1, (bottom - top).abs() + 1), tone);
    }
    if let Some(scan) = s.scan.filter(|v| v.is_finite()) {
        p.rect(
            Rect(x_of(scan.rem_euclid(1.)).round() as i32, r.1, 2, r.3),
            PLAYHEAD,
        );
    }
    for (gx, gy) in &s.grains {
        let x = x_of(gx.clamp(0., 1.)).round() as i32 - 1;
        let y = (center - (gy.clamp(0., 1.) - 0.5) * 2. * half).round() as i32 - 1;
        p.rect(Rect(x - 1, y - 1, 5, 5), BACKGROUND);
        p.rect(Rect(x, y, 3, 3), GRAIN);
    }
    if let Some(wiper) = s.wiper.filter(|v| v.is_finite()) {
        p.rect(
            Rect(x_of(wiper.clamp(0., 1.)).round() as i32, r.1, 2, r.3),
            RED,
        );
    }
    p.clip(None);
}

/// Words in lines of at most `width` pixels of 16-pixel condensed text.
pub(super) fn wrap(p: &Painter, text: &str, width: i32) -> Vec<String> {
    let mut lines: Vec<String> = vec![];
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line)
                if p.width_in(Face::Condensed, &format!("{line} {word}"), 16) <= width as f32 =>
            {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.into()),
        }
    }
    lines
}

fn folder_icon(p: &mut Painter, x: i32, y: i32, color: Color) {
    let (x, y) = (x as f32, y as f32);
    p.stroke(
        &[
            (x, y + 11.),
            (x, y),
            (x + 6., y),
            (x + 8., y + 2.),
            (x + 16., y + 2.),
            (x + 16., y + 11.),
            (x, y + 11.),
        ],
        1,
        color,
    );
}
fn wave_icon(p: &mut Painter, x: i32, y: i32, color: Color) {
    let ys = [3., 8.5, 0., 7., 1.5, 10., 3., 8.5, 0.];
    let points: Vec<(f32, f32)> = ys
        .iter()
        .enumerate()
        .map(|(i, dy)| (x as f32 + i as f32 * 2., y as f32 + dy))
        .collect();
    p.stroke(&points, 1, color);
}
