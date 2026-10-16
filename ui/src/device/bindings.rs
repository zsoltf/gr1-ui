//! Units at the UI/native boundary. Stock INI and IPC keep native units.
use super::abi::Ipc;

pub struct Binding {
    pub key: String,
    pub scale: f32,
    pub offset: f32,
    pub logarithmic: bool,
}
impl Binding {
    pub fn read(&self, state: &Ipc) -> Option<f32> {
        state.number(&self.key).map(|v| {
            if self.logarithmic {
                v.log2() * 12.
            } else {
                v * self.scale + self.offset
            }
        })
    }
    pub fn write(&self, state: &mut Ipc, value: f32) -> bool {
        state.set_number(
            &self.key,
            if self.logarithmic {
                2.0f32.powf(value / 12.)
            } else {
                (value - self.offset) / self.scale
            },
        )
    }
}
pub fn patch(id: &str) -> Option<Binding> {
    let (key, scale, offset) = match id {
        "pitch" => ("sample_tune", 1., 0.),
        "patch_volume" => ("volume", 100., 0.),
        "tape_slew" => ("tape_reactivity", -100., 100.),
        "sample_mode" => ("splmode", 1., 0.),
        "spawn" => ("spawnmode", 1., 0.),
        "stealing" => ("grainstealing_threshold", 1., 0.),
        "anti_alias" => ("antialias", 1., 0.),
        "glide" => ("grainsched", 1., 0.),
        "sample_loop" => ("sample_loop", 1., 0.),
        "grain_key" => ("grain_keysync", 1., 0.),
        "grain_clock" => ("grain_clocksync", 1., 0.),
        "scan_key" => ("scan_keysync", 1., 0.),
        "scan_loop" => ("scan_loop_confinement", 1., 0.),
        "scan_mode" => ("scan_mode", 1., 0.),
        "scan_clock" => ("scan_clocksync", 1., 0.),
        "responds" => ("responds_to_playmode", 1., 0.),
        "root" => ("basenote", 1., 0.),
        "range" => ("slider_semitones", 1., 0.),
        "free_scale" => ("freescale", 1., 0.),
        "fx_type" => ("fxtype", 1., 0.),
        "fx_map" => ("fxmapping", 1., 0.),
        "filter" => ("filtmode", 1., 0.),
        "cutoff" => ("cutoff", 1., 0.),
        "resonance" => ("resonance", 100., 0.),
        "sub_on" => ("subosc_enabled", 1., 0.),
        "sub_balance" => ("subosc_balance", 100., 0.),
        "sub_freq" => ("subosc_freq", 1., 0.),
        "sub_amp" => ("subosc_amp", 100., 0.),
        "delay_time" => ("delay_time", 1000., 0.),
        "delay_feedback" => ("delay_feedback", 100., 0.),
        "distortion" => ("distortion_level", 100., 0.),
        "bit_reduction" => ("reducer_bit", 100., 0.),
        "rate_reduction" => ("reducer_rate", 100., 0.),
        // Native fx_frac_to_reverb_time multiplies by this exact float.
        "reverb_time" => ("reverb_time", 100. / f32::from_bits(0x3dbe37c6), 0.),
        "reverb_amount" => ("reverb_amount", 100., 0.),
        "tuning" => ("kbtune", 1., 0.),
        "split1" => ("kbsplit1", 1., 0.),
        "split2" => ("kbsplit2", 1., 0.),
        "lfo_delay" => ("lfo_delay_enabled", 1., 0.),
        "cubic" => ("modamtcubic", 1., 0.),
        "crossmod" => ("lfo12xmod", 100., 0.),
        "mpe_clip" => ("mpe_pos_clip_enabled", 1., 0.),
        "rec_level" => ("rec_level", 100., 0.),
        "loop_rec" => ("loopingrec", 1., 0.),
        "start_mode" => ("startmode", 1., 0.),
        "sampling" => ("sampling_enabled", 1., 0.),
        "attack" => ("attack_time", 1., 0.),
        "decay" => ("decay_time", 1., 0.),
        "sustain" => ("sustain_level", 100., 0.),
        "release" => ("release_time", 1., 0.),
        "sides" => ("win_sides", 100., 0.),
        "tilt" => ("win_tilt", 50., 50.),
        "curve" => ("win_curve", 50., 50.),
        "direction" => ("fwdrev", 50., 50.),
        "pan" => ("pan", 100., 0.),
        "grain_mul" => ("dens_mul", 1., 0.),
        "grain_div" => ("dens_div", 1., 0.),
        "scan_mul" => ("scan_clock_mul", 1., 0.),
        "scan_div" => ("scan_clock_div", 1., 0.),
        _ => {
            for i in 1..=2 {
                let mapping = [
                    ("wave", "wvform", 1.),
                    ("rate", "speed", 1.),
                    ("amount", "amt", 100.),
                    ("sync", "_clocksync", 1.),
                    ("key", "_keysync", 1.),
                    ("max", "_maxfreq", 1.),
                    ("quant", "_quant", 1.),
                    ("mul", "_mul", 1.),
                    ("div", "_div", 1.),
                ];
                for (ui, native, scale) in mapping {
                    if id == format!("lfo{i}_{ui}") {
                        return Some(Binding {
                            key: format!("lfo{i}{native}"),
                            scale,
                            offset: 0.,
                            logarithmic: false,
                        });
                    }
                }
                if id == format!("lfo{i}_wheel") {
                    return Some(Binding {
                        key: format!("mod_to_lfo{i}amt"),
                        scale: 1.,
                        offset: 0.,
                        logarithmic: false,
                    });
                }
                if id == format!("cv{i}_amount") {
                    return Some(Binding {
                        key: format!("cv{i}amt"),
                        scale: 100.,
                        offset: 0.,
                        logarithmic: false,
                    });
                }
            }
            for i in 0..3 {
                for (ui, native, scale) in [
                    ("offset", "kbposoff", 100.),
                    ("scale", "kbposscale", 100.),
                    ("pitch", "kbpitch", 1.),
                ] {
                    if id == format!("zone{i}_{ui}") {
                        return Some(Binding {
                            key: format!("{native}{i}"),
                            scale,
                            offset: 0.,
                            logarithmic: false,
                        });
                    }
                }
            }
            return None;
        }
    };
    Some(Binding {
        key: key.into(),
        scale,
        offset,
        logarithmic: matches!(id, "pitch" | "sub_freq"),
    })
}
pub fn system(id: &str) -> Option<(&'static str, f32)> {
    Some(match id {
        "pickup" => ("setpoints", 1.),
        "direct_shift" => ("direct_shift", 1.),
        "adsr_mode" => ("adsr_slider_mode", 1.),
        "menu_slider" => ("menu_slider_enabled", 1.),
        "invert" => ("encoder_invert", 1.),
        "leds" => ("led_brightness", 1.),
        "cc_cv" => ("cc2cv", 1.),
        "cc_vis" => ("cc_vis_updates", 1.),
        "precision" => ("precision_cc_enabled", 1.),
        "mpe" => ("mpe_enabled", 1.),
        "bend" => ("pitchbend_range", 1.),
        "stop_grains" => ("grainstop", 1.),
        "dsp" => ("dspmetering", 1.),
        "convert" => ("conv44", 1.),
        "preview" => ("soundpreviews", 1.),
        "monitor" => ("monitor_level", 100.),
        "midi1" => ("midi_channel1", 1.),
        "midi2" => ("midi_channel2", 1.),
        "midi3" => ("midi_channel3", 1.),
        "midi4" => ("midi_channel4", 1.),
        _ => return None,
    })
}
pub fn params(p: &Ipc) -> [f32; 8] {
    [
        p.sample_pos * 100.,
        p.grain_size * 1000.,
        p.grain_spray * 100.,
        p.grain_dens,
        p.grain_scan,
        p.grain_panspray * 100.,
        p.startpos,
        p.endpos,
    ]
}
pub fn param(p: &mut Ipc, i: usize, value: f32) {
    match i {
        0 => p.sample_pos = value / 100.,
        1 => p.grain_size = value / 1000.,
        2 => p.grain_spray = value / 100.,
        3 => p.grain_dens = value,
        4 => p.grain_scan = value,
        5 => p.grain_panspray = value / 100.,
        6 => p.startpos = value.min(p.endpos),
        7 => p.endpos = value.max(p.startpos),
        _ => {}
    }
}
