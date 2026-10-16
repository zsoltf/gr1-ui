//! Stock-compatible documents. Live synthesis state stays in the native engine.
use super::abi::{Ipc, Multi, PATCH_FIELDS};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Debug, PartialEq)]
pub struct Document(pub BTreeMap<String, String>);

impl Document {
    pub fn defaults(kind: &str) -> Self {
        let all: Value =
            serde_json::from_str(include_str!("defaults.json")).expect("generated defaults");
        Self(
            all[kind]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| {
                    let s = match v {
                        Value::String(s) => s.clone(),
                        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
                        Value::Array(a) => {
                            a.iter().map(Value::to_string).collect::<Vec<_>>().join(" ")
                        }
                        _ => v.to_string(),
                    };
                    (k.clone(), s)
                })
                .collect(),
        )
    }
    pub fn parse(text: &str, section: &str) -> Result<Self> {
        let mut active = false;
        let mut found = false;
        let mut values = BTreeMap::new();
        let mut previous = None::<String>;
        for (n, line) in text.lines().enumerate() {
            let trim = line.trim();
            if trim.is_empty() || trim.starts_with(['#', ';']) {
                continue;
            }
            if trim.starts_with('[') && trim.ends_with(']') {
                active = &trim[1..trim.len() - 1] == section;
                found |= active;
                previous = None;
            } else if active {
                if line.starts_with([' ', '\t']) && previous.is_some() {
                    let value: &mut String = values.get_mut(previous.as_ref().unwrap()).unwrap();
                    value.push(' ');
                    value.push_str(trim);
                } else {
                    let (k, v) = trim
                        .split_once('=')
                        .or_else(|| trim.split_once(':'))
                        .ok_or_else(|| format!("Invalid INI line {}", n + 1))?;
                    let key = k.trim().to_lowercase();
                    if values
                        .insert(key.clone(), v.trim().replace("%%", "%"))
                        .is_some()
                    {
                        return Err(format!("Duplicate INI key {key}"));
                    }
                    previous = Some(key);
                }
            }
        }
        if !found {
            return Err(format!("Missing [{section}]"));
        }
        Ok(Self(values))
    }
    pub fn read(path: &Path, section: &str) -> Result<Self> {
        Self::parse(
            &fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?,
            section,
        )
    }
    pub fn merge(&mut self, other: Self) {
        self.0.extend(other.0);
    }
    pub fn text(&self, key: &str) -> &str {
        self.0.get(key).map(String::as_str).unwrap_or("")
    }
    pub fn number(&self, key: &str) -> Result<f32> {
        number(self.text(key)).map_err(|e| format!("{key}: {e}"))
    }
    pub fn put(&mut self, key: &str, value: impl ToString) {
        self.0.insert(key.into(), value.to_string());
    }
    pub fn render(&self, section: &str) -> String {
        let mut out = format!("[{section}]\n");
        for (k, v) in &self.0 {
            out.push_str(&format!("{k} = {}\n", v.replace('%', "%%")));
        }
        out
    }
    pub fn save(&self, path: &Path, section: &str) -> Result<()> {
        atomic_write(path, self.render(section).as_bytes())
    }
    pub fn patch(path: &Path) -> Result<Self> {
        let file = Self::read(path, "gr-1 patch")?;
        if !file.0.contains_key("sample") {
            return Err("Patch has no sample reference".into());
        }
        let mut doc = Self::defaults("patch");
        doc.merge(file);
        doc.ipc().map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(doc)
    }
    pub fn ipc(&self) -> Result<Ipc> {
        let mut ipc = Ipc::default();
        for key in PATCH_FIELDS {
            ipc.set_number(key, self.number(key)?);
        }
        set_text(&mut ipc.version, "3.4.4")?;
        set_text(&mut ipc.name, self.text("name"))?;
        set_text(&mut ipc.sample, self.text("sample"))?;
        if !(0..=1024).contains(&ipc.aux_len) {
            return Err("AUX recording exceeds 1024 entries".into());
        }
        let aux = numbers(self.text("aux_buf"))?;
        if aux.len() < ipc.aux_len as usize {
            return Err("Incomplete AUX recording".into());
        }
        ipc.aux_buf[..ipc.aux_len as usize].copy_from_slice(&aux[..ipc.aux_len as usize]);
        let scale = numbers(self.text("western_scale"))?;
        if scale.len() != 12 {
            return Err("Scale must contain 12 notes".into());
        }
        for (dst, src) in ipc.western_scale.iter_mut().zip(scale) {
            *dst = i32::from(src != 0.);
        }
        let rows: Vec<_> = self.text("matrix_rows").split(',').map(str::trim).collect();
        let cols: Vec<_> = self.text("matrix_cols").split(',').map(str::trim).collect();
        let matrix = numbers(self.text("mod_matrix"))?;
        if matrix.len() != rows.len() * cols.len() {
            return Err("Modulation matrix dimensions disagree".into());
        }
        let expected = Self::defaults("patch");
        let r: Vec<_> = expected
            .text("matrix_rows")
            .split(',')
            .map(str::trim)
            .collect();
        let c: Vec<_> = expected
            .text("matrix_cols")
            .split(',')
            .map(str::trim)
            .collect();
        for (ri, name) in rows.iter().enumerate() {
            for (ci, dest) in cols.iter().enumerate() {
                if let (Some(dr), Some(dc)) = (
                    r.iter().position(|s| s == name),
                    c.iter().position(|s| s == dest),
                ) {
                    ipc.mod_matrix[dr * 11 + dc] = matrix[ri * cols.len() + ci];
                }
            }
        }
        for key in ["lfo1dest", "lfo2dest", "cv1dest", "cv2dest"] {
            let old = self.number(key)? as usize;
            let new = cols
                .get(old)
                .and_then(|s| c.iter().position(|n| n == s))
                .ok_or("Invalid modulation destination")?;
            ipc.set_number(key, new as f32);
        }
        validate_ipc(&ipc)?;
        Ok(ipc)
    }
    pub fn update(&mut self, ipc: &Ipc) {
        for key in PATCH_FIELDS {
            self.put(key, ipc.number(key).unwrap());
        }
        self.put("version", "3.4.4");
        self.put("name", text(&ipc.name));
        self.put("sample", text(&ipc.sample));
        for (key, values) in [
            ("mod_matrix", &ipc.mod_matrix[..]),
            (
                "aux_buf",
                &ipc.aux_buf[..ipc.aux_len.clamp(0, 1024) as usize],
            ),
        ] {
            self.put(
                key,
                values
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }
        self.put(
            "western_scale",
            ipc.western_scale
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" "),
        );
        self.put(
            "matrix_rows",
            "LFO1, LFO2, CV1, CV2, TIMBRE, AFTERTOUCH, LIFT, VELO, PITCH",
        );
        self.put(
            "matrix_cols",
            "TUNE, POS, SPRAY, SIZE, DENS, AMP, PANSPRAY, CUTOFF, SIDES, TILT, CURVE",
        );
    }
}

pub fn validate_ipc(p: &Ipc) -> Result<()> {
    for (value, max, name) in [
        (p.fxtype, 4, "effect"),
        (p.filtmode, 3, "filter"),
        (p.fxmapping, 5, "effect mapping"),
        (p.scan_keysync, 2, "scan sync"),
        (p.splmode, 2, "sample mode"),
        (p.spawnmode, 3, "spawn"),
        (p.startmode, 3, "record mode"),
        (p.scan_mode, 2, "scan mode"),
        (p.lfo1wvform, 4, "LFO 1"),
        (p.lfo2wvform, 4, "LFO 2"),
        (p.voicestealing, 2, "voice stealing"),
    ] {
        if !(0..=max).contains(&value) {
            return Err(format!("Invalid {name} value {value}"));
        }
    }
    for n in [
        p.lfo1_div,
        p.lfo2_div,
        p.dens_div,
        p.scan_clock_div,
        p.lfo1_mul,
        p.lfo2_mul,
        p.dens_mul,
        p.scan_clock_mul,
        p.lfo1_quant,
        p.lfo2_quant,
    ] {
        if !(1..=1024).contains(&n) {
            return Err("Invalid clock ratio or quantization".into());
        }
    }
    if p.grain_size < 0.
        || p.sample_tune <= 0.
        || p.startpos < 0.
        || p.endpos > 100.
        || p.startpos > p.endpos
    {
        return Err("Invalid grain size, tuning or sample loop".into());
    }
    Ok(())
}
pub fn number(s: &str) -> Result<f32> {
    match s.trim().to_lowercase().as_str() {
        "true" | "yes" | "on" => Ok(1.),
        "false" | "no" | "off" => Ok(0.),
        _ => s
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("Invalid number {s:?}")),
    }
}
fn numbers(s: &str) -> Result<Vec<f32>> {
    s.split_whitespace().map(number).collect()
}
pub fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len())])
        .into()
}
pub fn set_text(bytes: &mut [u8], value: &str) -> Result<()> {
    if value.len() >= bytes.len() || value.contains(['\0', '\n', '\r']) {
        return Err("Name is too long or contains a line break".into());
    }
    bytes.fill(0);
    bytes[..value.len()].copy_from_slice(value.as_bytes());
    Ok(())
}
pub fn file_name(name: &str) -> Result<&str> {
    if name.trim().is_empty()
        || matches!(name, "." | "..")
        || name.contains(['/', '\\', '\0', '\n', '\r'])
    {
        return Err("Enter a single file or folder name".into());
    }
    Ok(name)
}
/// The native sample writer always emits WAV. Use this name before checking
/// or confirming an overwrite as well as when handing the path to the writer.
pub fn sample_file_name(name: &str) -> Result<String> {
    file_name(name)?;
    Ok(if name.to_ascii_lowercase().ends_with(".wav") {
        name.into()
    } else {
        format!("{name}.wav")
    })
}
/// One sample target for collision checks, recoverable replacement and writing.
/// The callback is the native WAV writer in device mode.
pub fn save_sample_with(
    directory: &Path,
    name: &str,
    replace: bool,
    write: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let target = directory.join(sample_file_name(name)?);
    if target.exists() {
        if !replace {
            return Err("File already exists".into());
        }
        trash_path(&target)?;
    }
    write(&target)
}
/// Move an entry to its folder's recoverable trash on the same filesystem.
/// The device checks location and Factory permissions before calling this.
pub fn trash_path(path: &Path) -> Result<()> {
    let folder = path.parent().ok_or("Invalid file")?;
    let trash = folder.join(".gr1-ui-trash");
    fs::create_dir_all(&trash).map_err(|e| e.to_string())?;
    let mut dest = trash.join(path.file_name().ok_or("Invalid file")?);
    let mut n = 2;
    while dest.exists() {
        dest = trash.join(format!(
            "{n}-{}",
            path.file_name().unwrap().to_string_lossy()
        ));
        n += 1;
    }
    fs::rename(path, dest).map_err(|e| e.to_string())
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("File has no parent folder")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp = parent.join(format!(
        ".gr1-ui-{}-{}.tmp",
        std::process::id(),
        path.file_name().unwrap().to_string_lossy()
    ));
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        f.write_all(bytes)
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string())?;
        fs::rename(&tmp, path).map_err(|e| e.to_string())?;
        fs::File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}
pub fn patch_file(path: &Path) -> Result<PathBuf> {
    if path.is_file() {
        return Ok(path.into());
    }
    let mut files: Vec<_> = fs::read_dir(path)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().is_some_and(|e| e == "grp")
                && !p.file_name().unwrap().to_string_lossy().starts_with('.')
        })
        .collect();
    files.sort();
    if files.len() != 1 {
        return Err(format!(
            "{}: expected one patch, found {}",
            path.display(),
            files.len()
        ));
    }
    Ok(files.remove(0))
}

#[derive(Clone)]
pub struct Performance {
    pub document: Document,
    pub multis: [Multi; 4],
    pub current: usize,
}
impl Default for Performance {
    fn default() -> Self {
        let mut document = Document::defaults("system");
        document.0.remove("systab");
        document.0.remove("current_performance");
        document.put("name", "New");
        let mut multi = Multi {
            patchnums: [0, 1, 2, 3],
            partenables: [1, 0, 0, 0],
            part_volumes: [1.; 4],
            ..Multi::default()
        };
        set_text(&mut multi.name, "monotimbral1").unwrap();
        Self {
            document,
            multis: [multi; 4],
            current: 0,
        }
    }
}
impl Performance {
    pub fn read(path: &Path) -> Result<Self> {
        let mut perf = Self::default();
        let doc = Document::read(path, "performance")?;
        if doc.0.contains_key("multi_presets") {
            let value = literal(doc.text("multi_presets"))?;
            let items = value
                .as_array()
                .filter(|a| a.len() == 4)
                .ok_or("Expected four multi presets")?;
            for (m, v) in perf.multis.iter_mut().zip(items) {
                set_text(&mut m.name, v["name"].as_str().ok_or("Missing multi name")?)?;
                m.stencilnr_in_focus = json_number(&v["stencilnr_in_focus"])? as i32;
                for (key, is_float) in [
                    ("patchnums", false),
                    ("partenables", false),
                    ("part_volumes", true),
                ] {
                    let a = v[key]
                        .as_array()
                        .filter(|a| a.len() == 4)
                        .ok_or("Expected four part values")?;
                    for (i, v) in a.iter().enumerate() {
                        let n = json_number(v)?;
                        if is_float {
                            m.part_volumes[i] = n;
                        } else if key == "patchnums" {
                            m.patchnums[i] = n as i32;
                        } else {
                            m.partenables[i] = i32::from(n != 0.);
                        }
                    }
                }
            }
        } else {
            for key in ["patchnums", "partenables"] {
                if doc.0.contains_key(key) {
                    let a = numbers(doc.text(key))?;
                    if a.len() != 4 {
                        return Err("Expected four legacy part values".into());
                    }
                    for (i, n) in a.into_iter().enumerate() {
                        if key == "patchnums" {
                            perf.multis[0].patchnums[i] = n as i32;
                        } else {
                            perf.multis[0].partenables[i] = i32::from(n != 0.);
                        }
                    }
                }
            }
        }
        if doc.0.contains_key("midi_channel") {
            for i in 1..=4 {
                perf.document
                    .put(&format!("midi_channel{i}"), doc.text("midi_channel"));
            }
        }
        perf.current = doc
            .0
            .get("current_multi_preset")
            .map(|s| number(s))
            .transpose()?
            .unwrap_or(0.) as usize;
        perf.document.merge(doc);
        perf.validate()?;
        Ok(perf)
    }
    pub fn validate(&self) -> Result<()> {
        if self.current >= 4 {
            return Err("Invalid multi preset".into());
        }
        for m in &self.multis {
            if !(0..4).contains(&m.stencilnr_in_focus)
                || m.patchnums.iter().any(|n| !(0..32).contains(n))
                || m.part_volumes
                    .iter()
                    .any(|v| !v.is_finite() || *v < 0. || *v > 1.)
            {
                return Err("Invalid multi preset parts".into());
            }
            let mut assigned = std::collections::HashSet::new();
            if m.patchnums
                .iter()
                .zip(m.partenables)
                .any(|(n, on)| on != 0 && !assigned.insert(n))
            {
                return Err("A preset cannot occupy two parts".into());
            }
        }
        Ok(())
    }
    pub fn document(&self) -> Document {
        let mut doc = self.document.clone();
        let values:Vec<_>=self.multis.iter().map(|m|serde_json::json!({"name":text(&m.name),"stencilnr_in_focus":m.stencilnr_in_focus,"patchnums":m.patchnums,"partenables":m.partenables,"part_volumes":m.part_volumes})).collect();
        // JSON with numeric flags is also a Python literal; no eval is needed here.
        doc.put("multi_presets", serde_json::to_string(&values).unwrap());
        doc.put("current_multi_preset", self.current);
        doc
    }
}
fn json_number(v: &Value) -> Result<f32> {
    if let Some(b) = v.as_bool() {
        return Ok(if b { 1. } else { 0. });
    }
    v.as_f64()
        .map(|v| v as f32)
        .filter(|v| v.is_finite())
        .ok_or_else(|| "Expected a finite number".into())
}
// The stock GRM embeds Python list/dict literals. Only data syntax is accepted.
// Strings are decoded first, so quote/backslash characters are never rewritten as code.
fn literal(input: &str) -> Result<Value> {
    struct Parser<'a> {
        s: &'a [u8],
        i: usize,
    }
    impl Parser<'_> {
        fn ws(&mut self) {
            while self.s.get(self.i).is_some_and(u8::is_ascii_whitespace) {
                self.i += 1;
            }
        }
        fn take(&mut self, b: u8) -> bool {
            self.ws();
            if self.s.get(self.i) == Some(&b) {
                self.i += 1;
                true
            } else {
                false
            }
        }
        fn value(&mut self, depth: usize) -> Result<Value> {
            if depth > 16 {
                return Err("Performance nesting is too deep".into());
            }
            self.ws();
            match self
                .s
                .get(self.i)
                .copied()
                .ok_or("Incomplete performance value")?
            {
                b'[' => {
                    self.i += 1;
                    let mut a = vec![];
                    while !self.take(b']') {
                        a.push(self.value(depth + 1)?);
                        if self.take(b']') {
                            break;
                        }
                        if !self.take(b',') {
                            return Err("Expected comma".into());
                        }
                    }
                    Ok(Value::Array(a))
                }
                b'{' => {
                    self.i += 1;
                    let mut m = serde_json::Map::new();
                    while !self.take(b'}') {
                        let key = self
                            .value(depth + 1)?
                            .as_str()
                            .ok_or("Expected a key")?
                            .to_string();
                        if !self.take(b':') {
                            return Err("Expected colon".into());
                        }
                        let v = self.value(depth + 1)?;
                        m.insert(key, v);
                        if self.take(b'}') {
                            break;
                        }
                        if !self.take(b',') {
                            return Err("Expected comma".into());
                        }
                    }
                    Ok(Value::Object(m))
                }
                quote @ (b'\'' | b'"') => {
                    self.i += 1;
                    let mut out = Vec::new();
                    loop {
                        let b = *self.s.get(self.i).ok_or("Unterminated string")?;
                        self.i += 1;
                        if b == quote {
                            break;
                        }
                        if b == b'\\' {
                            let c = *self.s.get(self.i).ok_or("Unterminated escape")?;
                            self.i += 1;
                            match c {
                                b'n' => out.push(b'\n'),
                                b'r' => out.push(b'\r'),
                                b't' => out.push(b'\t'),
                                b'\\' | b'\'' | b'"' => out.push(c),
                                b'u' => {
                                    let end = self.i + 4;
                                    let hex = std::str::from_utf8(
                                        self.s
                                            .get(self.i..end)
                                            .ok_or("Incomplete unicode escape")?,
                                    )
                                    .map_err(|e| e.to_string())?;
                                    let ch = char::from_u32(
                                        u32::from_str_radix(hex, 16).map_err(|e| e.to_string())?,
                                    )
                                    .ok_or("Invalid unicode")?;
                                    let mut buf = [0; 4];
                                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                                    self.i = end;
                                }
                                _ => return Err("Unsupported string escape".into()),
                            }
                        } else {
                            out.push(b);
                        }
                    }
                    Ok(Value::String(
                        String::from_utf8(out).map_err(|e| e.to_string())?,
                    ))
                }
                _ => {
                    let start = self.i;
                    while self
                        .s
                        .get(self.i)
                        .is_some_and(|b| !b.is_ascii_whitespace() && !b",]}".contains(b))
                    {
                        self.i += 1;
                    }
                    let s =
                        std::str::from_utf8(&self.s[start..self.i]).map_err(|e| e.to_string())?;
                    match s {
                        "True" | "true" => Ok(Value::Bool(true)),
                        "False" | "false" => Ok(Value::Bool(false)),
                        _ => serde_json::from_str::<Value>(s)
                            .ok()
                            .filter(Value::is_number)
                            .ok_or_else(|| "Only literal data is accepted in a performance".into()),
                    }
                }
            }
        }
    }
    let mut p = Parser {
        s: input.as_bytes(),
        i: 0,
    };
    let v = p.value(0)?;
    p.ws();
    if p.i != p.s.len() {
        return Err("Trailing performance data".into());
    }
    Ok(v)
}

/// Read real factory/user documents and compare the complete IPC representation
/// after a stock-format serialize/parse cycle. Does not load the vendor engine.
pub fn check_tree(root: &Path, output: Option<&Path>) -> Result<(usize, usize)> {
    fn walk(
        root: &Path,
        path: &Path,
        output: Option<&Path>,
        counts: &mut (usize, usize),
    ) -> Result<()> {
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            // A stock image has symlinks into the eventual data mount. Inspect
            // each real fixture once, and never recurse through a link cycle.
            if entry.file_type().map_err(|e| e.to_string())?.is_symlink() {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, output, counts)?;
                continue;
            }
            match path.extension().and_then(|e| e.to_str()) {
                Some("grp") => {
                    let original = Document::patch(&path)?;
                    let before = original.ipc()?;
                    let mut normalized = original.clone();
                    normalized.update(&before);
                    let result =
                        Document::parse(&normalized.render("gr-1 patch"), "gr-1 patch")?.ipc()?;
                    if before != result {
                        return Err(format!("Patch round trip changed {}", path.display()));
                    }
                    if let Some(out) = output {
                        normalized
                            .save(&out.join(path.strip_prefix(root).unwrap()), "gr-1 patch")?;
                    }
                    counts.0 += 1;
                }
                Some("grm") => {
                    let original = Performance::read(&path)?;
                    let document = original.document();
                    let parsed = Document::parse(&document.render("performance"), "performance")?;
                    if literal(parsed.text("multi_presets"))?
                        != literal(document.text("multi_presets"))?
                    {
                        return Err(format!("Performance round trip changed {}", path.display()));
                    }
                    if let Some(out) = output {
                        document
                            .save(&out.join(path.strip_prefix(root).unwrap()), "performance")?;
                    }
                    counts.1 += 1;
                }
                _ => {}
            }
        }
        Ok(())
    }
    if let Some(out) = output {
        if out.exists() {
            return Err("Checked-file output must be a new directory".into());
        }
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        let parent = out
            .parent()
            .unwrap_or(Path::new("."))
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if parent.starts_with(root) {
            return Err("Export checked files outside the input tree".into());
        }
        fs::create_dir(out).map_err(|e| e.to_string())?;
    }
    let mut counts = (0, 0);
    walk(root, root, output, &mut counts)?;
    Ok(counts)
}
