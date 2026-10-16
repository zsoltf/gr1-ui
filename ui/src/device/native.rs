//! The only native-call owner. Vendor workers retain both library and IPC until exit.
use super::{abi::Ipc, storage::Result};
use libloading::Library;
use sha2::{Digest, Sha256};
use std::{
    ffi::{c_char, CString},
    fs::{self, File, OpenOptions},
    io::Read,
    marker::PhantomData,
    mem::ManuallyDrop,
    os::{
        fd::AsRawFd,
        unix::fs::{FileTypeExt, OpenOptionsExt},
    },
    path::Path,
    ptr::NonNull,
    rc::Rc,
};

pub const ENGINE_SHA: &str = "3dbdc64487b33a28eaf55f12fe66d4184d64c85c6d88c1b18d80a93ed42bc682";
macro_rules! api {
    ($($name:ident ($($arg:ty),*) $(-> $ret:ty)?;)+) => {
        #[allow(dead_code)]
        pub struct Api { $(pub $name: unsafe extern "C" fn($($arg),*) $(-> $ret)?,)+ }
        impl Api {
            unsafe fn load(lib:&Library)->Result<Self>{Ok(Self{
                $($name:*lib.get(concat!(stringify!($name),"\0").as_bytes()).map_err(|e|e.to_string())?,)+
            })}
        }
    }
}
// Shapes are recovered for the pinned binary, not ctypes' default return types.
api! {
    libgr_create(); ipc_init(*mut Ipc); phui_init(); gr_init_synthesis()->i32;
    libgr_start_synthesis(); start_audio(); gr_mute_sound(); gr_unmute_sound(); gr_all_notes_off();
    gr_visual_updates_off(); gr_visual_updates_on(); gr_set_monotimbral();
    gr_patch2ipc(); gr_ipc2patch(); gr_multitimbral_ipc2engine(); gr_get_patchnum()->i32;
    gr_set_preset(i32); gr_backup_patch(i32); gr_duplicate_sample();
    gr_load_sample(*const c_char,i32,i32)->i32; gr_save_sample(*const c_char,i32)->i32;
    gr_save_empty_sample(*const c_char)->i32; gr_get_samplelength(i32)->i32;
    gr_load_temp_sample(*const c_char)->i32; gr_enable_preview(); gr_disable_preview();
    gr_set_preset_name(i32,*const c_char); gr_set_sample_name(i32,*const c_char);
    gr_copy_patch(i32,i32); gr_copy_sample(i32,i32); gr_copy_template(i32,i32);
    gr_init_current_sample(f32,i32); gr_clear_sample_dirty();
    gr_set_sampling_enabled(i32)->i32; gr_enable_rec_to_patch(i32); gr_is_recording()->i32;
    gr_is_part_recording()->i32; gr_get_recpos()->f32;
    gr_set_modmatrix_entry_float(i32,i32,f32);
    gr_calc_stencil(i32,i32,i32,i32,*mut f32);
    gr_calc_stencil_rec(i32,i32,i32,i32,*mut f32,f32);
    gr_get_part_patchnum(i32)->i32;
    gr_get_grain_positions(i32,*mut f32,*mut f32,*mut i32);
    parts_get_scan_array(*mut f32); parts_get_pos_array(*mut f32); parts_get_spray_array(*mut f32);
    gr_fetch_grainwin(*mut f32); gr_get_lfo_speeds(*mut f32);
    gr_get_lfo1amt()->f32; gr_get_lfo2amt()->f32;
    gr_get_cv1amt()->f32; gr_get_cv2amt()->f32;
    conv_lin_time_to_frac(f32)->f32; conv_exp_time_to_frac(f32)->f32;
    gr_get_vu()->f32; gr_get_vu_clipped()->i32; get_dsp_usage()->f32;
    gr_get_sample_pos()->f32; gr_get_center_pos()->f32;
    gr_get_shift_enabled()->i32; gr_get_shift_explicitly_pressed()->i32;
    set_screen(i32); phui_actuate_button(i32,i32);
    phui_set_setpoints(i32); set_led_brightness(i32); gr_set_adsr_slider_mode(i32);
    gr_set_cc2cv(i32); gr_set_ccvis(i32); gr_set_grainstop(i32);
    midi_set_mpe_enabled(i32); midi_set_channel(i32,i32); gr_set_pitchbend_range(f32);
    gr_set_monitor_level(f32); set_precision_cc_enabled(i32);
    phui_get_activity(*mut f32,*mut f32,*mut u8,*mut f32,*mut f32);
    // Additional exports inspected at 0x22cac, 0x18aac, 0x23778, 0x232e8.
    phui_set_playtogglemode(i32); synth_set_multi_preset(i32);
    set_voice_vol(i32,f32); handle_note_slider(i32,i32); recstop_lfo_wave();
    synth_focus_part(i32);
    alsamidi_restart(); arecord_connect_sampler();
}

pub fn preflight(path: &Path) -> Result<String> {
    if !cfg!(all(
        target_os = "linux",
        target_arch = "arm",
        target_pointer_width = "32",
        target_env = "gnu"
    )) {
        return Err(
            "Device mode requires 32-bit ARM Linux on the GR-1. Use desktop mode on this computer."
                .into(),
        );
    }
    let model = fs::read_to_string("/proc/device-tree/model").map_err(|e| e.to_string())?;
    if !model.contains("Raspberry Pi") {
        return Err("The target is not a Raspberry Pi".into());
    }
    let cards = fs::read_to_string("/proc/asound/cards").map_err(|e| e.to_string())?;
    if !cards.contains("sndrpihifiberry") {
        return Err(
            "GR-1 audio card is absent; native initialization can reboot without it".into(),
        );
    }
    if !Path::new("/dev/ttyAMA0").exists() {
        return Err("GR-1 panel UART is absent".into());
    }
    for entry in fs::read_dir("/proc").map_err(|e| e.to_string())?.flatten() {
        if entry.file_name().to_string_lossy().parse::<u32>().is_ok() {
            if let Ok(bytes) = fs::read(entry.path().join("cmdline")) {
                let command = String::from_utf8_lossy(&bytes);
                if command.contains("grmain_") || command.contains("grmain.exe") {
                    return Err("Stock GR-1 application is still running; stop it before launching this host".into());
                }
            }
        }
    }
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if format!("{:x}", Sha256::digest(&bytes)) != ENGINE_SHA {
        return Err("Engine is not the pinned original 3.4.4 payload".into());
    }
    Ok(format!(
        "{}; original 3.4.4 engine; GR-1 audio and UART present",
        model.trim_end_matches('\0')
    ))
}

pub struct Engine {
    pub api: Api,
    ipc: NonNull<Ipc>,
    _library: ManuallyDrop<Library>,
    playing: *const u8,
    sliders: *const i32,
    voice_map: *const i32,
    voice_amplitudes: *const f32,
    one_to_one: *mut u8,
    aux_recording: *const u8,
    active_patch: *const *const u8,
    vu_clipped: *const i32,
    vu_soft_clipped: *const i32,
    lfo_values: unsafe extern "C" fn(*const u8, *mut f32),
    _single_caller: PhantomData<Rc<()>>,
    events: File,
    blink: File,
    _panel: File,
    panel_stream: PanelStream,
    _host_lock: File,
}
#[derive(Debug, PartialEq)]
pub struct PanelEvent {
    pub action: u8,
    pub value: u8,
    pub text: Option<String>,
}
#[derive(Default)]
pub struct PanelStream {
    buffer: Vec<u8>,
    first_encoder: Option<u8>,
}
impl PanelStream {
    pub fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }
    pub fn drain(&mut self) -> Vec<PanelEvent> {
        frames(&mut self.buffer)
            .into_iter()
            .filter(|event| {
                if event.action == 5 && self.first_encoder.is_none() {
                    // The MCU retains its absolute count across host restarts.
                    // Native enc_val starts at zero, so its first delta has no
                    // valid reference. By this report the engine has updated
                    // enc_val; subsequent reports are real movement. Sync once
                    // per connection, including reports on the main screen.
                    self.first_encoder = Some(event.value);
                    false
                } else {
                    true
                }
            })
            .collect()
    }
}
pub fn frames(buffer: &mut Vec<u8>) -> Vec<PanelEvent> {
    let mut out = vec![];
    let mut consumed = 0;
    while buffer.len() - consumed >= 2 {
        let action = buffer[consumed];
        let value = buffer[consumed + 1];
        let text_len = if action == 21 || action == 23 {
            value as usize
        } else {
            0
        };
        if buffer.len() - consumed < 2 + text_len {
            break;
        }
        out.push(PanelEvent {
            action,
            value,
            text: if text_len > 0 {
                Some(String::from_utf8_lossy(&buffer[consumed + 2..consumed + 2 + text_len]).into())
            } else {
                None
            },
        });
        consumed += 2 + text_len;
    }
    buffer.drain(..consumed);
    out
}
fn pipe(path: &str) -> Result<File> {
    if !fs::metadata(path)
        .map_err(|e| format!("{path}: {e}"))?
        .file_type()
        .is_fifo()
    {
        return Err(format!("{path} is not a named pipe"));
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| e.to_string())
}
impl Engine {
    pub fn input_clipped(&self) -> bool {
        // gr_get_vu_clipped reads these two flags, but also logs every call.
        unsafe { self.vu_clipped.read_volatile() != 0 || self.vu_soft_clipped.read_volatile() != 0 }
    }
    /// The same active Patch/Matrix used by gr_get_lfo_speeds. Read only;
    /// the native audio thread remains the owner of oscillator phase/output.
    pub fn lfo_outputs(&self) -> Option<[f32; 2]> {
        let patch = unsafe { self.active_patch.read_volatile() };
        if patch.is_null() {
            return None;
        }
        let mut values = [0.; 2];
        unsafe {
            (self.lfo_values)(patch.add(0x3dc), values.as_mut_ptr());
        }
        values.iter().all(|v| v.is_finite()).then_some(values)
    }
    pub fn open(path: &Path) -> Result<Self> {
        preflight(path)?;
        // Two replacement processes must not acquire the same panel/audio device.
        let host_lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open("/tmp/gr1-ui-host.lock")
            .map_err(|e| e.to_string())?;
        if unsafe { libc::flock(host_lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err("Another Rust GR-1 host is running".into());
        }
        // Resolve everything before native initialization changes device state.
        let lib = unsafe { Library::new(path) }.map_err(|e| e.to_string())?;
        let api = unsafe { Api::load(&lib) }?;
        let playing = unsafe {
            *lib.get::<*const u8>(b"playing\0")
                .map_err(|e| e.to_string())?
        };
        let one_to_one = unsafe {
            *lib.get::<*mut u8>(b"one2one_flag\0")
                .map_err(|e| e.to_string())?
        };
        let aux_recording = unsafe {
            *lib.get::<*const u8>(b"lfo_record_active\0")
                .map_err(|e| e.to_string())?
        };
        let sliders = unsafe {
            *lib.get::<*const i32>(b"slider_values\0")
                .map_err(|e| e.to_string())?
        };
        let voice_map = unsafe {
            *lib.get::<*const i32>(b"playmode2engine_voicemap\0")
                .map_err(|e| e.to_string())?
        };
        let voice_amplitudes = unsafe {
            *lib.get::<*const f32>(b"voice_amps\0")
                .map_err(|e| e.to_string())?
        };
        // Pinned 3.4.4 readback contract: oscillator readback as f32 values.
        let vu_clipped = unsafe {
            *lib.get::<*const i32>(b"vu_clipped\0")
                .map_err(|e| e.to_string())?
        };
        let vu_soft_clipped = unsafe {
            *lib.get::<*const i32>(b"vu_soft_clipped\0")
                .map_err(|e| e.to_string())?
        };
        let active_patch = unsafe {
            *lib.get::<*const *const u8>(b"active_patch\0")
                .map_err(|e| e.to_string())?
        };
        let lfo_values = unsafe {
            *lib.get::<unsafe extern "C" fn(*const u8, *mut f32)>(
                b"_ZN6Matrix14get_lfo_valuesEPf\0",
            )
            .map_err(|e| e.to_string())?
        };
        let ipc = NonNull::new(unsafe { libc::calloc(1, std::mem::size_of::<Ipc>()) } as *mut Ipc)
            .ok_or("Cannot allocate native state")?;
        // Once initialized, neither this memory nor the library may be released.
        let library = ManuallyDrop::new(lib);
        unsafe {
            (api.libgr_create)();
            (api.ipc_init)(ipc.as_ptr());
            (api.phui_init)();
        }
        let events = pipe("/tmp/grui_pipe")?;
        let blink = pipe("/tmp/blinkpipe")?;
        let panel = pipe("/tmp/phui_pipe")?;
        unsafe {
            (api.gr_init_synthesis)();
        }
        Ok(Self {
            api,
            ipc,
            _library: library,
            playing,
            sliders,
            voice_map,
            voice_amplitudes,
            one_to_one,
            aux_recording,
            active_patch,
            vu_clipped,
            vu_soft_clipped,
            lfo_values,
            _single_caller: PhantomData,
            events,
            blink,
            _panel: panel,
            panel_stream: PanelStream::default(),
            _host_lock: host_lock,
        })
    }
    pub fn start(&mut self) -> Result<()> {
        trace_threads("before-native-start");
        unsafe {
            (self.api.libgr_start_synthesis)();
            (self.api.start_audio)();
            (self.api.gr_disable_preview)();
            (self.api.set_screen)(0);
        }
        #[cfg(target_os = "linux")]
        {
            // The vendor start call promotes its caller to FIFO priority 1.
            // CPU rasterization at that priority caused audio underruns on the
            // GR-1; changing only this thread to SCHED_OTHER removed them in
            // the same-process comparison. Keep CPU 0 affinity and leave the
            // native audio/grain workers' real-time policies untouched.
            let param = libc::sched_param { sched_priority: 0 };
            if unsafe { libc::sched_setscheduler(0, libc::SCHED_OTHER, &param) } != 0 {
                return Err(format!(
                    "Cannot return UI thread to normal scheduling: {}",
                    std::io::Error::last_os_error()
                ));
            }
        }
        trace_threads("after-native-start");
        Ok(())
    }
    pub fn snapshot(&mut self) -> Ipc {
        unsafe {
            (self.api.gr_patch2ipc)();
            std::ptr::read_volatile(self.ipc.as_ptr())
        }
    }
    pub fn raw_snapshot(&self) -> Ipc {
        unsafe { std::ptr::read_volatile(self.ipc.as_ptr()) }
    }
    pub fn apply_patch(&mut self, value: &Ipc) {
        // Preserve global settings and multi configuration; this copies patch fields only.
        // Like the stock host, copies are not atomic with respect to native workers.
        let before = self.raw_snapshot();
        let mut state = before;
        for key in super::abi::PATCH_FIELDS {
            state.set_number(key, value.number(key).unwrap());
        }
        state.name = value.name;
        state.sample = value.sample;
        state.version = value.version;
        state.aux_buf = value.aux_buf;
        state.mod_matrix = value.mod_matrix;
        state.western_scale = value.western_scale;
        unsafe {
            self.write_changes(&before, &state);
            (self.api.gr_ipc2patch)();
        }
    }
    pub fn edit_global(&mut self, f: impl FnOnce(&mut Ipc)) {
        let before = self.raw_snapshot();
        let mut state = before;
        f(&mut state);
        unsafe {
            self.write_changes(&before, &state);
        }
    }
    unsafe fn write_changes(&mut self, before: &Ipc, after: &Ipc) {
        let a = before as *const Ipc as *const u32;
        let b = after as *const Ipc as *const u32;
        for i in 0..std::mem::size_of::<Ipc>() / 4 {
            if *a.add(i) != *b.add(i) {
                std::ptr::write_volatile((self.ipc.as_ptr() as *mut u32).add(i), *b.add(i));
            }
        }
    }
    pub fn set_panel_mode(&mut self, matrix: bool) {
        unsafe {
            std::ptr::write_volatile(self.one_to_one, u8::from(!matrix));
        }
    }
    pub fn panel_mode(&self) -> bool {
        unsafe { std::ptr::read_volatile(self.one_to_one) == 0 }
    }
    pub fn aux_active(&self) -> bool {
        unsafe { std::ptr::read_volatile(self.aux_recording) != 0 }
    }
    pub fn aux_record(&mut self, on: bool) {
        if self.aux_active() != on {
            unsafe {
                (self.api.recstop_lfo_wave)();
            }
        }
    }
    pub fn voice_controls(&self, part: usize) -> ([Option<i32>; 4], [Option<f32>; 4]) {
        let sliders = std::array::from_fn(|i| {
            let v = unsafe { std::ptr::read_volatile(self.sliders.add(i)) };
            (0..=1023).contains(&v).then_some(v)
        });
        let levels = std::array::from_fn(|i| {
            let voice = unsafe { std::ptr::read_volatile(self.voice_map.add(i * 4 + part)) };
            if (0..88).contains(&voice) {
                Some(unsafe { std::ptr::read_volatile(self.voice_amplitudes.add(voice as usize)) })
            } else {
                None
            }
        });
        (sliders, levels)
    }
    pub fn playing(&self) -> bool {
        unsafe { std::ptr::read_volatile(self.playing) != 0 }
    }
    pub fn events(&mut self) -> Result<Vec<PanelEvent>> {
        let mut bytes = [0; 4096];
        // Bounded work per frame so a busy MIDI/panel stream cannot starve rendering.
        for _ in 0..8 {
            match self.events.read(&mut bytes) {
                Ok(0) => break,
                Ok(n) => self.panel_stream.push(&bytes[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.to_string()),
            }
        }
        for _ in 0..8 {
            match self.blink.read(&mut bytes) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(self.panel_stream.drain())
    }
    /// The first encoder report, which set the reference and was not delivered.
    pub fn encoder_reference(&self) -> Option<u8> {
        self.panel_stream.first_encoder
    }
    pub fn string(path: &Path) -> Result<CString> {
        CString::new(path.to_str().ok_or("Path is not UTF-8")?).map_err(|e| e.to_string())
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        // Process exit reclaims native workers. Its stop_all_threads can hang and
        // does not join the fan, so unloading or freeing IPC would be unsafe.
        unsafe {
            (self.api.gr_mute_sound)();
            (self.api.gr_all_notes_off)();
        }
    }
}

#[cfg(target_os = "linux")]
fn trace_threads(phase: &str) {
    if std::env::var_os("GR1_UI_TRACE").is_none() {
        return;
    }
    let Ok(entries) = fs::read_dir("/proc/self/task") else {
        return;
    };
    let mut tasks = entries.flatten().map(|e| e.path()).collect::<Vec<_>>();
    tasks.sort();
    for task in tasks {
        let Some(tid) = task
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.parse::<i32>().ok())
        else {
            continue;
        };
        let status = fs::read_to_string(task.join("status")).unwrap_or_default();
        let name = fs::read_to_string(task.join("comm")).unwrap_or_default();
        let cpus = status
            .lines()
            .find_map(|l| l.strip_prefix("Cpus_allowed_list:"))
            .unwrap_or("unknown")
            .trim();
        let mut param = libc::sched_param { sched_priority: 0 };
        let policy = unsafe { libc::sched_getscheduler(tid) };
        if unsafe { libc::sched_getparam(tid, &mut param) } == 0 {
            eprintln!("[gr1-thread] phase={phase} tid={tid} name={} policy={policy} priority={} cpus={cpus}", name.trim(), param.sched_priority);
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn trace_threads(_phase: &str) {}
