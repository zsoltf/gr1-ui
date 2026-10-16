//! Draws the panel interface from shared components.
use super::components::{self, Glyph, ListRow};
use super::overlay::*;
use super::*;

impl App {
    /// Draw the file layers: the bottom screen, then overlays above it.
    pub(super) fn draw_overlay(&mut self, p: &mut Painter) {
        let layers = std::mem::take(&mut self.files_ui.layers);
        // The bottom full screen: a browser, the Save sheet or the name
        // editor; without one, the page that opened the sheet stays below.
        let full = layers
            .iter()
            .rposition(|l| !matches!(l, Layer::Sheet(_) | Layer::Confirm(_)));
        let underlay = full.is_none() && self.sample_root_active();
        if underlay {
            self.draw_sample_root(p);
        }
        let base = full.unwrap_or(0);
        match &layers[base] {
            Layer::Browser(b) => self.draw_browser(p, b),
            Layer::Save(s) => self.draw_save_sheet(p, s),
            Layer::Name(n) => {
                let save = match &n.purpose {
                    NamePurpose::Sheet => n.title.clone(),
                    _ => "Apply".into(),
                };
                components::draw_name_editor(p, n, self.files_ui.strip, &save);
            }
            _ => {}
        }
        let row_y = match &layers[base] {
            Layer::Browser(b) => Some(b.list.focus_y()),
            _ => None,
        };
        for layer in &layers[full.map_or(0, |b| b + 1)..] {
            match layer {
                Layer::Sheet(s) => components::draw_action_sheet(p, s, row_y.unwrap_or(150)),
                Layer::Confirm(c) => components::draw_confirm(p, c, row_y.is_some()),
                _ => {}
            }
        }
        self.files_ui.layers = layers;
        if !underlay {
            let touched = self.readout_target();
            self.status_bar(p, touched);
        }
        // Overlay containers own their focus. Complete the layout after the
        // stack is restored so streaming panel events do not redraw it again.
        self.rendered_context = self.focus_context();
        self.navigation_dirty = false;
    }

    fn draw_browser(&mut self, p: &mut Painter, b: &Browser) {
        let slot = self.parts[self.part];
        let name = self.patches[slot].name.clone();
        let chips: Vec<(&str, bool)> = [DataKind::Sample, DataKind::Patch, DataKind::Perf]
            .iter()
            .map(|k| (k.chip(), *k == b.kind))
            .collect();
        match b.purpose {
            Purpose::Load => {
                let rest = match b.kind {
                    DataKind::Perf => "as the working performance".to_string(),
                    _ => format!("into {:02} {name}", slot + 1),
                };
                components::draw_header(p, &format!("Load {}", b.kind.word()), &rest, &chips);
            }
            Purpose::Folder => components::draw_header(p, "Save sample", "into a folder", &[]),
            Purpose::Slot => components::draw_header(p, "Save performance", "into a slot", &[]),
        }
        // The path row: where the list is, and how long it is.
        let mut right = 776;
        if b.purpose == Purpose::Folder && matches!(b.place, Place::Folder(_)) {
            right = components::keycap(p, 776, 50, "Save", BLUE, BACKGROUND) - 10;
        }
        let count = match b.rows.len() {
            1 => "1 item".to_string(),
            n => format!("{n} items"),
        };
        let start = p.text_right(Face::Condensed, right, 50, 16, &count, INK3);
        let title = self.place_title(&b.place);
        p.fit_in(Face::Condensed, 24, 50, 16, start - 40, &title, INK2);
        let empty = match (&b.error, b.rows.is_empty(), b.purpose) {
            (Some(error), _, _) => Some((error.clone(), true)),
            (None, true, Purpose::Folder) => Some(("No folders here".to_string(), false)),
            (None, true, _) => Some((format!("No {}s in this folder", b.kind.word()), false)),
            _ => None,
        };
        components::draw_list(
            p,
            b.list,
            b.rows.len(),
            empty.as_ref().map(|(t, e)| (t.as_str(), *e)),
            |i, focused| {
                let row = &b.rows[i];
                ListRow {
                    glyph: match &row.role {
                        Role::Slot { index, .. } => Glyph::Number(index + 1),
                        Role::Folder | Role::Location(_) => Glyph::Folder,
                        Role::Item if b.kind == DataKind::Sample => Glyph::Wave,
                        Role::Item => Glyph::Document,
                    },
                    label: row.label.clone(),
                    right: self.row_detail(b, row),
                    second: if focused {
                        self.row_second_line(b, row)
                    } else {
                        String::new()
                    },
                    current: row.current,
                    dim: matches!(row.role, Role::Slot { empty: true, .. }),
                    keycap: match (&row.role, b.purpose) {
                        (Role::Item | Role::Slot { empty: false, .. }, Purpose::Load) => {
                            Some("Load")
                        }
                        (Role::Slot { .. }, Purpose::Slot) => Some("Save"),
                        _ => None,
                    },
                    check: None,
                    press: None,
                }
            },
        );
    }
    /// The right column of a row.
    fn row_detail(&mut self, b: &Browser, row: &Row) -> String {
        match (&row.role, b.kind) {
            (Role::Item, DataKind::Sample) => {
                let path = b.row_path(row);
                self.cached_info(&path, &row.key, b.real_files)
                    .map(|i| format!("{:.1} s", i.seconds()))
                    .unwrap_or_default()
            }
            (
                Role::Slot {
                    empty: false,
                    index,
                },
                _,
            ) if *index == self.performance_slot => "Working".into(),
            _ => row.right.clone(),
        }
    }
    /// The focused row's second line: format, size and use; slot date.
    fn row_second_line(&mut self, b: &Browser, row: &Row) -> String {
        let path = b.row_path(row);
        match (&row.role, b.kind) {
            (Role::Item, DataKind::Sample) => {
                let mut parts = vec![];
                if let Some(info) = self.cached_info(&path, &row.key, b.real_files) {
                    parts.push(format!("{:.1} kHz", info.rate as f32 / 1000.));
                    parts.push(match info.channels {
                        1 => "Mono".into(),
                        2 => "Stereo".into(),
                        n => format!("{n} channels"),
                    });
                    parts.push(megabytes(info.bytes));
                } else {
                    parts.push(extension_label(&row.key));
                    if let Ok(meta) = std::fs::metadata(&path) {
                        parts.push(megabytes(meta.len()));
                    }
                }
                let users: Vec<String> = self
                    .patches
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.sample == row.key)
                    .map(|(i, _)| format!("{:02}", i + 1))
                    .take(4)
                    .collect();
                if !users.is_empty() {
                    parts.push(format!("in preset {}", users.join(", ")));
                }
                parts.join("     ")
            }
            (Role::Slot { empty: true, .. }, _) => "Empty slot".into(),
            (Role::Slot { index, .. }, _) => match &self.device {
                Some(d) => d
                    .performance_paths
                    .get(*index)
                    .and_then(|p| std::fs::metadata(p.join("performance.grm")).ok())
                    .and_then(|m| m.modified().ok())
                    .map(|t| format!("Saved {}", local_time(t)))
                    .unwrap_or_default(),
                None => "Saved 3 Oct 21:40".into(),
            },
            (Role::Folder, _) => match row.right.as_str() {
                "1" => "1 item".into(),
                n => format!("{n} items"),
            },
            (Role::Location(_), _) => String::new(),
            (Role::Item, kind) => format!("{} {}", kind.chip(), extension_label(&row.key)),
        }
    }

    fn draw_save_sheet(&mut self, p: &mut Painter, s: &SaveSheet) {
        let r = Rect(140, 64, 520, 330);
        p.round_rect(r, 6, OUTLINE);
        p.round_rect(Rect(r.0 + 1, r.1 + 1, r.2 - 2, r.3 - 2), 5, RAISED);
        p.text_in(
            Face::Regular,
            164,
            82,
            22,
            &format!("Save {}", s.kind.word()),
            INK,
        );
        let row_focus = |p: &mut Painter, y: i32, field: SaveField| {
            if s.focus == field {
                p.round_rect(Rect(150, y, 500, 38), 3, BLUE_WASH);
                p.rect(Rect(150, y, 3, 38), BLUE);
            }
        };
        row_focus(p, 124, SaveField::Type);
        p.text_in(Face::Condensed, 164, 130, 20, "Type", INK2);
        for (i, k) in [DataKind::Sample, DataKind::Patch, DataKind::Perf]
            .into_iter()
            .enumerate()
        {
            components::chip(
                p,
                Rect(290 + i as i32 * 72, 128, 64, 30),
                k.chip(),
                k == s.kind,
            );
        }
        row_focus(p, 172, SaveField::Name);
        p.text_in(Face::Condensed, 164, 177, 20, "Name", INK2);
        value_right(p, 177, &format!("{}  \u{203a}", s.name));
        row_focus(p, 221, SaveField::Into);
        p.text_in(Face::Condensed, 164, 226, 20, "Into", INK2);
        let (into, note) = self.save_destination(s);
        let into = if s.kind == DataKind::Patch {
            into
        } else {
            format!("{into}  \u{203a}")
        };
        value_right(p, 226, &into);
        p.fit_in(Face::Condensed, 164, 266, 16, 472, &note, INK2);
        let save = s.focus == SaveField::Save;
        components::button(
            p,
            Rect(164, 314, 220, 52),
            "Save",
            save,
            if save { BLUE } else { INK },
            Some(("Save", BLUE)),
        );
        let cancel = s.focus == SaveField::Cancel;
        components::button(
            p,
            Rect(400, 314, 220, 52),
            "Cancel",
            cancel,
            if cancel { BLUE } else { INK },
            Some(("Esc", INK2)),
        );
    }
    /// The sheet's destination and the sentence saying what it replaces.
    fn save_destination(&self, s: &SaveSheet) -> (String, String) {
        match s.kind {
            DataKind::Patch => {
                let slot = self.parts[self.part];
                (
                    format!(
                        "Preset {:02} of {:02} {}",
                        slot + 1,
                        self.performance_slot + 1,
                        self.performance
                    ),
                    format!("Replaces the saved preset {:02} on disk.", slot + 1),
                )
            }
            DataKind::Sample => {
                let file = match crate::device::storage::sample_file_name(s.name.trim()) {
                    Ok(file) => file,
                    Err(e) => return (self.place_title(&Place::Folder(s.folder.clone())), e),
                };
                let exists = s.names.contains(&file);
                (
                    self.place_title(&Place::Folder(s.folder.clone())),
                    if exists {
                        format!("Replaces {file} in this folder.")
                    } else {
                        format!("Writes {file}.")
                    },
                )
            }
            DataKind::Perf => match s.slot {
                None => (
                    "Choose a user slot".into(),
                    "This performance is not in a user slot.".into(),
                ),
                Some(slot) => {
                    let (name, empty) = self.slot_name(slot);
                    let working = slot == self.performance_slot
                        && !self.device.as_ref().is_some_and(|d| d.imported());
                    (
                        format!(
                            "Slot {:02} {}",
                            slot + 1,
                            if empty { "" } else { name.as_str() }
                        ),
                        if empty {
                            "Writes an empty slot.".into()
                        } else if working {
                            "Replaces this slot's saved presets.".into()
                        } else {
                            format!("Replaces {name}; its saved presets are overwritten.")
                        },
                    )
                }
            },
        }
    }
}

/// A Save sheet value ending at the sheet's right margin.
fn value_right(p: &mut Painter, y: i32, text: &str) {
    let width = p.width_in(Face::Condensed, text, 20).round() as i32;
    if width <= 386 {
        p.text_right(Face::Condensed, 636, y, 20, text, INK);
    } else {
        p.fit_in(Face::Condensed, 250, y, 20, 386, text, INK);
    }
}

impl App {
    /// Sample headers are read once per listing, not every frame.
    fn cached_info(&mut self, path: &str, name: &str, real_files: bool) -> Option<SampleInfo> {
        *self
            .files_ui
            .info
            .entry(path.to_string())
            .or_insert_with(|| sample_info(real_files, path, name))
    }
}

fn megabytes(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.)
    } else {
        format!("{} kB", bytes.div_ceil(1000))
    }
}
fn extension_label(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_uppercase())
        .unwrap_or_default()
}

/// What the browser shows for a sample: read from the WAV header only.
#[derive(Clone, Copy)]
pub(super) struct SampleInfo {
    pub rate: u32,
    pub channels: u16,
    pub frames: u64,
    pub bytes: u64,
}
impl SampleInfo {
    pub fn seconds(&self) -> f32 {
        self.frames as f32 / self.rate.max(1) as f32
    }
}
/// Header facts of a WAV file. Other formats show their type and size; the
/// native loader decides whether it can read them. The desktop demo has no
/// files and derives stand-in values from the name.
pub(super) fn sample_info(device: bool, path: &str, name: &str) -> Option<SampleInfo> {
    if !device {
        let seed = name
            .bytes()
            .fold(7u32, |s, b| s.wrapping_mul(31).wrapping_add(b as u32));
        let frames = 44_100 * (1 + (seed % 450) as u64) / 10;
        return Some(SampleInfo {
            rate: 44_100,
            channels: if seed % 3 == 0 { 2 } else { 1 },
            frames,
            bytes: frames * 2,
        });
    }
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let bytes = file.metadata().ok()?.len();
    let mut head = [0; 12];
    file.read_exact(&mut head).ok()?;
    if &head[0..4] != b"RIFF" || &head[8..12] != b"WAVE" {
        return None;
    }
    let (mut rate, mut channels, mut block) = (0u32, 0u16, 0u16);
    for _ in 0..32 {
        let mut chunk = [0; 8];
        file.read_exact(&mut chunk).ok()?;
        let size = u32::from_le_bytes(chunk[4..8].try_into().ok()?) as u64;
        match &chunk[0..4] {
            b"fmt " => {
                let mut fmt = [0; 16];
                file.read_exact(&mut fmt).ok()?;
                channels = u16::from_le_bytes([fmt[2], fmt[3]]);
                rate = u32::from_le_bytes(fmt[4..8].try_into().ok()?);
                block = u16::from_le_bytes([fmt[12], fmt[13]]);
                file.seek(SeekFrom::Current(size as i64 - 16 + (size & 1) as i64))
                    .ok()?;
            }
            b"data" => {
                if rate == 0 || block == 0 {
                    return None;
                }
                return Some(SampleInfo {
                    rate,
                    channels,
                    frames: size / block as u64,
                    bytes,
                });
            }
            _ => {
                file.seek(SeekFrom::Current(size as i64 + (size & 1) as i64))
                    .ok()?;
            }
        }
    }
    None
}

/// "3 Oct 21:40" in the instrument's local time.
pub(super) fn local_time(time: std::time::SystemTime) -> String {
    let seconds = time
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as libc::time_t)
        .unwrap_or(0);
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&seconds, &mut tm) }.is_null() {
        return String::new();
    }
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{} {} {:02}:{:02}",
        tm.tm_mday,
        MONTHS[tm.tm_mon.clamp(0, 11) as usize],
        tm.tm_hour,
        tm.tm_min
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::SystemTime};

    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn wav(path: &Path, rate: u32, channels: u16, seconds: u32) {
        let block = channels * 2;
        let data = rate * seconds * block as u32;
        let mut bytes = vec![];
        bytes.extend(b"RIFF");
        bytes.extend((36 + data).to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(channels.to_le_bytes());
        bytes.extend(rate.to_le_bytes());
        bytes.extend((rate * block as u32).to_le_bytes());
        bytes.extend(block.to_le_bytes());
        bytes.extend(16u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(data.to_le_bytes());
        bytes.resize((44 + data) as usize, 0);
        fs::write(path, bytes).unwrap();
    }
    #[test]
    fn real_wav_metadata_renders_from_each_browser_folder_while_layers_are_taken() {
        let root = std::env::temp_dir().join(format!(
            "gr1-metadata-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let _scratch = Scratch(root.clone());
        let mut app = App::new();
        let mut painter = Painter::new().unwrap();
        for (directory, rate, channels, seconds, detail) in [
            ("first", 44_100, 1, 2, "44.1 kHz     Mono"),
            ("second", 48_000, 2, 3, "48.0 kHz     Stereo"),
        ] {
            let folder = root.join(directory);
            fs::create_dir(&folder).unwrap();
            wav(&folder.join("SAME.wav"), rate, channels, seconds);
            let row = Row {
                key: "SAME.wav".into(),
                label: "SAME.wav".into(),
                role: Role::Item,
                right: String::new(),
                current: false,
            };
            let browser = Browser {
                kind: DataKind::Sample,
                purpose: Purpose::Load,
                place: Place::Folder(folder.to_string_lossy().into_owned()),
                rows: vec![row.clone()],
                list: components::ListCursor::default(),
                details: None,
                error: None,
                real_files: true,
            };
            // Rendering takes the layers out; row metadata must still use
            // this browser's directory, not CWD or a demo's name-derived data.
            app.files_ui.layers = vec![Layer::Browser(browser)];
            app.render(&mut painter);
            if let Some(captures) = std::env::var_os("GR1_UI_TEST_CAPTURE_DIR") {
                painter
                    .png(
                        &std::path::PathBuf::from(captures)
                            .join(format!("metadata-{directory}.png")),
                    )
                    .unwrap();
            }
            let browser = match app.files_ui.layers.pop().unwrap() {
                Layer::Browser(b) => b,
                _ => panic!(),
            };
            assert_eq!(
                app.row_detail(&browser, &row),
                format!("{:.1} s", seconds as f32)
            );
            assert!(app.row_second_line(&browser, &row).starts_with(detail));
            let full = folder.join("SAME.wav").to_string_lossy().into_owned();
            let info = app.files_ui.info.get(&full).unwrap().unwrap();
            assert_eq!(
                (info.rate, info.channels, info.frames),
                (rate, channels, rate as u64 * seconds as u64)
            );
            assert!(!app.files_ui.info.contains_key("SAME.wav"));
            println!(
                "Rendered {}: {seconds:.1} s; {detail}; absolute cache key {full}",
                directory
            );
        }
        assert_eq!(
            app.files_ui.info.len(),
            2,
            "identical basenames keep distinct headers"
        );
    }
    #[test]
    fn sample_save_sheet_names_the_writers_actual_collision() {
        let app = App::new();
        for name in [
            "take.ogg",
            "take.flac",
            "take.aiff",
            "take.wave",
            "take.WAV",
        ] {
            let file = crate::device::storage::sample_file_name(name).unwrap();
            let sheet = SaveSheet {
                kind: DataKind::Sample,
                name: name.into(),
                folder: "/Internal".into(),
                names: vec![file.clone()],
                slot: None,
                focus: SaveField::Save,
            };
            assert_eq!(
                app.save_destination(&sheet).1,
                format!("Replaces {file} in this folder.")
            );
        }
    }
}
