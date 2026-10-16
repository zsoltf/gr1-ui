use crate::appearance::Theme;
use fontdue::{Font, FontSettings, Metrics};
use std::{collections::HashMap, path::Path};

#[derive(Clone, Copy)]
pub struct Color([u8; 3], [u8; 3]);
impl Color {
    pub fn rgb(self, theme: Theme, brightness: u8) -> [u8; 3] {
        let rgb = match theme {
            Theme::White => self.0,
            Theme::Dark => self.1,
        };
        // This scales UI pixels; it does not control the HDMI panel backlight.
        rgb.map(|channel| (u16::from(channel) * u16::from(brightness) / 100) as u8)
    }
}

// Section 5.2 of the redesign: each role has a White and a Dark value. No
// screen owns its own palette, so dialogs, focus, plots and lists change together.
/// Screen. Dark is true black: the backlight is the floor.
pub const BACKGROUND: Color = Color([233, 234, 230], [0, 0, 0]);
/// Plot wells and cells.
pub const WELL: Color = Color([220, 223, 219], [12, 18, 23]);
/// Focused rows and sheets.
pub const RAISED: Color = Color([250, 250, 248], [23, 34, 43]);
/// Separators.
pub const RULE: Color = Color([198, 203, 200], [30, 42, 51]);
/// Brackets, outlines and empty bar tracks.
pub const OUTLINE: Color = Color([167, 175, 172], [46, 62, 74]);
/// Primary text.
pub const INK: Color = Color([17, 21, 24], [212, 221, 226]);
/// Labels and secondary text.
pub const INK2: Color = Color([74, 86, 94], [138, 154, 165]);
/// Dimmed, inactive and empty.
pub const INK3: Color = Color([140, 150, 156], [79, 93, 103]);
/// Focus, current page, structure strokes, On.
pub const BLUE: Color = Color([15, 104, 176], [74, 163, 227]);
/// Focus and selection backgrounds.
pub const BLUE_WASH: Color = Color([207, 227, 244], [14, 42, 64]);
/// Changing now: grabbed value, touched control, unsaved.
pub const AMBER: Color = Color([154, 88, 0], [240, 168, 69]);
/// Grabbed row.
pub const AMBER_WASH: Color = Color([245, 225, 194], [58, 40, 16]);
/// Recording, clipping, data-losing actions, errors.
pub const RED: Color = Color([179, 50, 42], [242, 100, 90]);
/// Background of the Record button while recording and of a latched Clip
/// mark (Dark from wireframe 12; White toward the background).
pub const RED_WASH: Color = Color([245, 218, 214], [58, 21, 18]);
/// Waveform columns.
pub const WAVEFORM: Color = Color([163, 182, 196], [43, 66, 82]);
/// Columns inside the spray range.
pub const SPOTLIGHT: Color = Color([47, 106, 146], [76, 120, 148]);
/// Columns outside the loop.
pub const OUTSIDE_LOOP: Color = Color([210, 216, 219], [20, 29, 36]);
/// Grain squares.
pub const GRAIN: Color = Color([11, 47, 74], [232, 244, 251]);
/// Scan center.
pub const PLAYHEAD: Color = Color([15, 94, 156], [155, 211, 247]);
/// Faint band behind the spray range .
pub const SPRAY_BAND: Color = Color([220, 230, 236], [6, 18, 26]);
/// Fill below envelope and window curves .
pub const CURVE_FILL: Color = Color([211, 227, 239], [11, 28, 41]);
/// Unfocused part rows (Dark from the wireframe; White 35% toward background).
pub const WAVEFORM_DIM: Color = Color([188, 200, 208], [28, 43, 54]);
/// Spray range in unfocused part rows (as above).
pub const SPOTLIGHT_DIM: Color = Color([112, 151, 175], [54, 85, 106]);

// The earlier role names, mapped one to one onto the tokens so menus, dialogs
// and browsers keep their behavior under the new palette.
pub const BG: Color = BACKGROUND;
pub const PANEL: Color = RAISED;
pub const LINE: Color = OUTLINE;
pub const MUTED: Color = INK2;
pub const ACTIVE: Color = BLUE;
pub const SELECTED: Color = BLUE_WASH;
pub const PRIMARY: Color = BLUE_WASH;
pub const ON_PRIMARY: Color = BLUE;
pub const EDGE: Color = INK3;
pub const NAV: Color = WELL;
pub const NOTICE: Color = AMBER_WASH;
pub const DANGER: Color = RED;

#[derive(Clone, Copy, Debug)]
pub struct Rect(pub i32, pub i32, pub i32, pub i32);
impl Rect {
    pub fn intersection(self, other: Self) -> Self {
        let x = self.0.max(other.0);
        let y = self.1.max(other.1);
        Self(
            x,
            y,
            (self.0 + self.2)
                .min(other.0 + other.2)
                .saturating_sub(x)
                .max(0),
            (self.1 + self.3)
                .min(other.1 + other.3)
                .saturating_sub(y)
                .max(0),
        )
    }
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.0 && y >= self.1 && x < self.0 + self.2 && y < self.1 + self.3
    }
}
/// The type scale's two widths and two weights .
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Face {
    Regular,
    Bold,
    Condensed,
}
pub struct Painter {
    pub pixels: Vec<u8>,
    pub theme: Theme,
    pub brightness: u8,
    clip: Rect,
    fonts: [Font; 3],
    glyphs: HashMap<(Face, char, u16), (Metrics, Vec<u8>)>,
}
impl Painter {
    pub fn new() -> Result<Self, String> {
        let paths = [
            std::env::var("GR1_UI_FONT").unwrap_or_default(),
            "/System/Library/Fonts/Supplemental/Arial.ttf".into(),
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".into(),
            "/usr/share/fonts/TTF/DejaVuSans.ttf".into(),
        ];
        let (path, bytes) = paths
            .iter()
            .find_map(|p| std::fs::read(p).ok().map(|bytes| (p.clone(), bytes)))
            .ok_or("Set GR1_UI_FONT to a TrueType font path")?;
        let regular = Font::from_bytes(bytes, FontSettings::default()).map_err(str::to_string)?;
        // The device package keeps every face beside the regular one; a source
        // checkout keeps the two added DejaVu faces in ui/fonts.
        let directories = [
            Path::new(&path).parent().map(Path::to_path_buf),
            Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("fonts")),
        ];
        let face = |name: &str| {
            let found = directories
                .iter()
                .flatten()
                .find_map(|dir| std::fs::read(dir.join(name)).ok())
                .and_then(|bytes| Font::from_bytes(bytes, FontSettings::default()).ok());
            found.unwrap_or_else(|| {
                eprintln!("GR-1 UI: {name} not found; drawing it with the regular face");
                regular.clone()
            })
        };
        let bold = face("DejaVuSans-Bold.ttf");
        let condensed = face("DejaVuSansCondensed.ttf");
        Ok(Self {
            pixels: vec![0; 800 * 480 * 4],
            theme: Theme::default(),
            brightness: 100,
            clip: Rect(0, 0, 800, 480),
            fonts: [regular, bold, condensed],
            glyphs: HashMap::new(),
        })
    }
    fn font(&self, face: Face) -> &Font {
        &self.fonts[match face {
            Face::Regular => 0,
            Face::Bold => 1,
            Face::Condensed => 2,
        }]
    }
    pub fn rect(&mut self, r: Rect, color: Color) {
        let color = color.rgb(self.theme, self.brightness);
        let rgba = [color[0], color[1], color[2], 255];
        let r = r.intersection(self.clip);
        let start = r.0.clamp(0, 800) as usize * 4;
        let end = (r.0 + r.2).clamp(0, 800) as usize * 4;
        for y in r.1.max(0)..(r.1 + r.3).min(480) {
            let row = y as usize * 800 * 4;
            for pixel in self.pixels[row + start..row + end].chunks_exact_mut(4) {
                pixel.copy_from_slice(&rgba);
            }
        }
    }
    pub fn round_rect(&mut self, r: Rect, radius: i32, color: Color) {
        let radius = radius.min(r.2 / 2).min(r.3 / 2).max(0);
        for row in 0..r.3 {
            let dy = if row < radius {
                radius - row - 1
            } else if row >= r.3 - radius {
                row - (r.3 - radius)
            } else {
                0
            };
            let inset = if dy > 0 {
                radius - ((radius * radius - dy * dy) as f32).sqrt() as i32
            } else {
                0
            };
            self.rect(Rect(r.0 + inset, r.1 + row, r.2 - 2 * inset, 1), color);
        }
    }
    pub fn clear(&mut self) {
        self.clip = Rect(0, 0, 800, 480);
        self.rect(Rect(0, 0, 800, 480), BG);
    }
    pub fn clip(&mut self, bounds: Option<Rect>) {
        self.clip = bounds.unwrap_or(Rect(0, 0, 800, 480));
    }
    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Color) {
        if x0 == x1 {
            self.rect(Rect(x0, y0.min(y1), 1, (y1 - y0).abs() + 1), color);
            return;
        }
        if y0 == y1 {
            self.rect(Rect(x0.min(x1), y0, (x1 - x0).abs() + 1, 1), color);
            return;
        }
        let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        for i in 0..=steps {
            self.rect(
                Rect(x0 + (x1 - x0) * i / steps, y0 + (y1 - y0) * i / steps, 1, 1),
                color,
            );
        }
    }
    pub fn border(&mut self, r: Rect, c: Color) {
        self.rect(Rect(r.0, r.1, r.2, 1), c);
        self.rect(Rect(r.0, r.1 + r.3 - 1, r.2, 1), c);
        self.rect(Rect(r.0, r.1, 1, r.3), c);
        self.rect(Rect(r.0 + r.2 - 1, r.1, 1, r.3), c);
    }
    /// Fill a convex shape given as one horizontal span per row.
    fn spans(&mut self, rows: impl Iterator<Item = (i32, f32, f32)>, color: Color) {
        for (y, x0, x1) in rows {
            let (a, b) = (x0.round() as i32, x1.round() as i32);
            if b > a {
                self.rect(Rect(a, y, b - a, 1), color);
            }
        }
    }
    pub fn disc(&mut self, cx: f32, cy: f32, radius: f32, color: Color) {
        let top = (cy - radius).floor() as i32;
        let bottom = (cy + radius).ceil() as i32;
        self.spans(
            (top..bottom).filter_map(|y| {
                let dy = y as f32 + 0.5 - cy;
                let half = (radius * radius - dy * dy).max(0.).sqrt();
                (half > 0.).then_some((y, cx - half, cx + half))
            }),
            color,
        );
    }
    /// An upward triangle with its base on `base` (exclusive) and apex `height` above.
    pub fn triangle_up(&mut self, cx: f32, base: i32, width: f32, height: i32, color: Color) {
        self.spans(
            (base - height..base).map(|y| {
                let half = width / 2. * (y - (base - height)) as f32 / height.max(1) as f32;
                (y, cx - half - 0.5, cx + half + 0.5)
            }),
            color,
        );
    }
    /// A polyline drawn with square pens of `width` pixels.
    pub fn stroke(&mut self, points: &[(f32, f32)], width: i32, color: Color) {
        let offset = (width - 1) / 2;
        for pair in points.windows(2) {
            let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
            let steps = (x1 - x0).abs().max((y1 - y0).abs()).ceil().max(1.) as i32;
            let (mut last_x, mut last_y) = (i32::MIN, i32::MIN);
            for i in 0..=steps {
                let t = i as f32 / steps as f32;
                let x = (x0 + (x1 - x0) * t).round() as i32;
                let y = (y0 + (y1 - y0) * t).round() as i32;
                if (x, y) != (last_x, last_y) {
                    self.rect(Rect(x - offset, y - offset, width, width), color);
                    (last_x, last_y) = (x, y);
                }
            }
        }
    }
    pub fn outline(&mut self, r: Rect, width: i32, color: Color) {
        self.rect(Rect(r.0, r.1, r.2, width), color);
        self.rect(Rect(r.0, r.1 + r.3 - width, r.2, width), color);
        self.rect(Rect(r.0, r.1, width, r.3), color);
        self.rect(Rect(r.0 + r.2 - width, r.1, width, r.3), color);
    }
    /// Text in a face; `y + size` is the baseline. Returns the end x.
    pub fn text_in(
        &mut self,
        face: Face,
        x: i32,
        y: i32,
        size: u16,
        text: &str,
        color: Color,
    ) -> i32 {
        let color = color.rgb(self.theme, self.brightness);
        let mut px = x as f32;
        for ch in text.chars() {
            let font = &self.fonts[match face {
                Face::Regular => 0,
                Face::Bold => 1,
                Face::Condensed => 2,
            }];
            let (m, bits) = self
                .glyphs
                .entry((face, ch, size))
                .or_insert_with(|| font.rasterize(ch, size as f32));
            let top = y + size as i32 - m.height as i32 - m.ymin;
            for row in 0..m.height {
                for col in 0..m.width {
                    let xx = px.round() as i32 + m.xmin + col as i32;
                    let yy = top + row as i32;
                    if !(0..800).contains(&xx)
                        || !(0..480).contains(&yy)
                        || !self.clip.contains(xx, yy)
                    {
                        continue;
                    }
                    let a = bits[row * m.width + col] as u32;
                    let i = (yy as usize * 800 + xx as usize) * 4;
                    for (k, c) in color.iter().enumerate() {
                        self.pixels[i + k] =
                            ((self.pixels[i + k] as u32 * (255 - a) + *c as u32 * a) / 255) as u8;
                    }
                }
            }
            px += m.advance_width;
        }
        px.round() as i32
    }
    pub fn text(&mut self, x: i32, y: i32, size: u16, text: &str, color: Color) {
        self.text_in(Face::Regular, x, y, size, text, color);
    }
    /// Right-aligned text ending at `right`; returns its start x.
    pub fn text_right(
        &mut self,
        face: Face,
        right: i32,
        y: i32,
        size: u16,
        text: &str,
        color: Color,
    ) -> i32 {
        let x = right - self.width_in(face, text, size).round() as i32;
        self.text_in(face, x, y, size, text, color);
        x
    }
    /// Text shortened with an ellipsis to fit `width`; returns the end x.
    pub fn fit_in(
        &mut self,
        face: Face,
        x: i32,
        y: i32,
        size: u16,
        width: i32,
        text: &str,
        color: Color,
    ) -> i32 {
        let mut result = String::new();
        let mut total = 0.;
        let reserve = self.font(face).metrics('.', size as f32).advance_width * 3.;
        for ch in text.chars() {
            let advance = self.font(face).metrics(ch, size as f32).advance_width;
            if total + advance > width as f32 {
                while self.width_in(face, &result, size) + reserve > width as f32 {
                    if result.pop().is_none() {
                        break;
                    }
                }
                result.push_str("...");
                break;
            }
            total += advance;
            result.push(ch);
        }
        self.text_in(face, x, y, size, &result, color)
    }
    pub fn fit(&mut self, x: i32, y: i32, size: u16, width: i32, text: &str, color: Color) {
        self.fit_in(Face::Regular, x, y, size, width, text, color);
    }
    pub fn width_in(&self, face: Face, text: &str, size: u16) -> f32 {
        let font = self.font(face);
        text.chars()
            .map(|ch| font.metrics(ch, size as f32).advance_width)
            .sum()
    }
    pub fn wrap(&mut self, x: i32, y: i32, width: usize, text: &str, color: Color) {
        let mut line = String::new();
        let mut yy = y;
        for word in text.split_whitespace() {
            if line.len() + word.len() + 1 > width {
                self.text(x, yy, 14, &line, color);
                line.clear();
                yy += 21;
            }
            if !line.is_empty() {
                line.push(' ')
            }
            line.push_str(word);
        }
        self.text(x, yy, 14, &line, color);
    }
    /// The pixels of `r` (inside the screen), to put back later with `restore`.
    pub fn save(&self, r: Rect) -> Vec<u8> {
        let r = r.intersection(Rect(0, 0, 800, 480));
        let mut out = Vec::with_capacity((r.2.max(0) * r.3.max(0) * 4) as usize);
        for y in r.1..r.1 + r.3 {
            let row = (y as usize * 800 + r.0 as usize) * 4;
            out.extend_from_slice(&self.pixels[row..row + r.2 as usize * 4]);
        }
        out
    }
    /// Put back pixels taken with `save` from the same rectangle.
    pub fn restore(&mut self, r: Rect, pixels: &[u8]) {
        let r = r.intersection(Rect(0, 0, 800, 480));
        let width = r.2.max(0) as usize * 4;
        if pixels.len() != width * r.3.max(0) as usize {
            return;
        }
        for (i, y) in (r.1..r.1 + r.3).enumerate() {
            let row = (y as usize * 800 + r.0 as usize) * 4;
            self.pixels[row..row + width].copy_from_slice(&pixels[i * width..(i + 1) * width]);
        }
    }
    pub fn png(&self, path: &Path) -> Result<(), String> {
        let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
        let mut enc = png::Encoder::new(file, 800, 480);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .map_err(|e| e.to_string())?
            .write_image_data(&self.pixels)
            .map_err(|e| e.to_string())
    }
}
