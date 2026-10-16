#[derive(Clone, Debug)]
pub enum Kind {
    Toggle,
    Choice(Vec<&'static str>),
    Number {
        min: f32,
        max: f32,
        step: f32,
        unit: &'static str,
    },
}
#[derive(Clone, Debug)]
pub struct Setting {
    pub id: String,
    pub page: &'static str,
    pub group: &'static str,
    pub name: String,
    /// The panel's word for the setting where its page already gives the
    /// context ("Mode" on Timing's Scan group); `name` stays the full name
    /// for the readout and the unported roots.
    pub label: String,
    pub _description: String,
    pub kind: Kind,
    pub default: f32,
    pub system: bool,
}
fn choice(
    id: &str,
    page: &'static str,
    group: &'static str,
    name: &str,
    options: &[&'static str],
    help: &str,
) -> Setting {
    Setting {
        id: id.into(),
        page,
        group,
        name: name.into(),
        label: name.into(),
        _description: help.into(),
        kind: Kind::Choice(options.to_vec()),
        default: 0.,
        system: page == "System"
            || matches!(
                id,
                "convert" | "preview" | "input_device" | "matrix_mode" | "monitor"
            ),
    }
}
fn toggle(
    id: &str,
    page: &'static str,
    group: &'static str,
    name: &str,
    on: bool,
    help: &str,
) -> Setting {
    let mut s = choice(id, page, group, name, &[], help);
    s.kind = Kind::Toggle;
    s.default = if on { 1. } else { 0. };
    s
}
fn number(
    id: &str,
    page: &'static str,
    group: &'static str,
    name: &str,
    range: (f32, f32, f32, f32, &'static str),
    help: &str,
) -> Setting {
    let mut s = choice(id, page, group, name, &[], help);
    s.kind = Kind::Number {
        min: range.0,
        max: range.1,
        step: range.2,
        unit: range.4,
    };
    s.default = range.3;
    s
}
pub fn catalog() -> Vec<Setting> {
    let mut v=vec![
        number("pitch","Patch","Playback","Pitch",(-12.,12.,0.01,0.,"st"),"Transpose sample playback. Fine adjustment is in hundredths of a semitone."),
        number("tape_slew","Patch","Playback","Tape slew",(0.,100.,1.,0.,"%"),"Tape slew control; normalized prototype display range."),
        number("patch_volume","Patch","Playback","Patch volume",(0.,400.,1.,80.,"%"),"Patch level, also assigned to Shift plus FX2 on the physical panel."),
        choice("sample_mode","Patch","Playback","Sample mode",&["Granular","Audition","Tape"],"Choose granular playback, direct sample audition, or tape-style movement."),
        choice("spawn","Patch","Playback","Spawn mode",&["Gradual","Direct","Direct reset","Recycle"],"How a cloud of grains begins when you play a note."),
        number("stealing","Patch","Playback","Grain stealing threshold",(0.,1.,0.01,0.,""),"Zero disables stealing. Higher values replace grains more aggressively."),
        toggle("anti_alias","Patch","Playback","Resample anti-aliasing",true,"Filter resampling artifacts. The original engine's quality and polyphony tradeoff is retained."),
        toggle("glide","Patch","Playback","Granular glide",true,"Let existing grains keep their pitch as new notes arrive."),
        toggle("sample_loop","Patch","Playback","Sample loop confinement",false,"Keep sample playback within the loop start and end points."),
        toggle("grain_key","Patch","Timing","Grain key sync",false,"Start a grain immediately when a new note arrives."),
        toggle("grain_clock","Patch","Timing","Grain clock sync",false,"Use incoming MIDI clock for density. Density controls multiply or divide the clock."),
        choice("scan_key","Patch","Timing","Scan key sync",&["Off","On","Legato"],"Reset scan position on each note, or only the first note of a phrase."),
        toggle("scan_loop","Patch","Timing","Scan loop confinement",false,"Keep scanning within the loop. This is separate from wrapping individual grains."),
        choice("scan_mode","Patch","Timing","Scan mode",&["Loop","One shot","Ping pong"],"Choose what the scan cursor does at the ends of its range."),
        toggle("scan_clock","Patch","Timing","Scan clock sync",false,"Tie scan movement to incoming MIDI clock."),
        toggle("responds","Patch","Play mode","Responds to Play",true,"Allow the physical Play button to trigger this patch."),
        number("root","Patch","Play mode","Slider root note",(0.,127.,1.,60.,"MIDI"),"Base note for the pitch sliders. The original instrument applies it when Play is restarted."),
        number("range","Patch","Play mode","Slider pitch range",(0.,48.,1.,24.,"st"),"Pitch travel of the voice sliders, measured in semitones."),
        toggle("free_scale","Patch","Play mode","Free scale",false,"Allow continuous pitch movement rather than stepping through enabled scale notes."),
        choice("fx_type","Patch","Effects","FX type",&["None","Delay","Distortion","Reducer","Reverb"],"One effect is active at a time. Filter and sub-oscillator remain available."),
        choice("fx_map","Patch","Effects","FX knob mapping",&["Filter","Sub-oscillator","Delay","Distortion","Reducer","Reverb"],"Choose what the two physical FX knobs adjust. This does not change the active effect."),
        choice("filter","Patch","Effects","Filter mode",&["None","Low-pass","Band-pass","High-pass"],"Choose the filter response."),
        number("cutoff","Patch","Effects","Filter cutoff",(0.,20000.,10.,8000.,"Hz"),"Cutoff frequency of the part filter."),
        number("resonance","Patch","Effects","Resonance",(0.,100.,1.,10.,"%"),"Emphasize frequencies around the cutoff."),
        toggle("sub_on","Patch","Effects","Sub-oscillator",false,"Add the native sub-oscillator to the patch."),
        number("sub_balance","Patch","Effects","Sub-osc balance",(0.,100.,1.,50.,"%"),"Bias the sub-oscillator's contribution across low and high notes."),
        number("sub_freq","Patch","Effects","Sub-osc tuning",(-24.,0.,1.,-12.,"st"),"Tune the sub-oscillator below the played note."),
        number("sub_amp","Patch","Effects","Sub-osc amplitude",(0.,100.,1.,50.,"%"),"Level of the sub-oscillator."),
        number("delay_time","Patch","Effects","Delay time",(0.,5000.,1.,350.,"ms"),"Delay spacing. Prototype display range; engine binding is deferred."),
        number("delay_feedback","Patch","Effects","Delay feedback",(0.,100.,1.,30.,"%"),"Amount fed back into the delay."),
        number("distortion","Patch","Effects","Distortion amount",(0.,100.,1.,20.,"%"),"Intensity of the distortion effect."),
        number("bit_reduction","Patch","Effects","Bit reduction",(0.,100.,1.,0.,"%"),"Resolution reduction amount; normalized for this UI study."),
        number("rate_reduction","Patch","Effects","Rate reduction",(0.,100.,1.,0.,"%"),"Sample-rate reduction amount; normalized for this UI study."),
        number("reverb_time","Patch","Effects","Reverb time",(0.,100.,1.,50.,"%"),"Length control for the native reverb; normalized UI value."),
        number("reverb_amount","Patch","Effects","Reverb amount",(0.,100.,1.,20.,"%"),"Wet reverb level."),
        number("tuning","Patch","Keyboard","Keyboard tuning",(0.,127.,1.,60.,"MIDI"),"Set the note that matches the source sample. MIDI learn awaits device integration."),
        number("split1","Patch","Keyboard","Keyboard split 1",(-1.,127.,1.,48.,"MIDI"),"Upper boundary of area 0. Split 2 cannot be below this note."),
        number("split2","Patch","Keyboard","Keyboard split 2",(-1.,127.,1.,72.,"MIDI"),"Upper boundary of area 1. Area 2 starts above it."),
        toggle("lfo_delay","Mod","Options","LFO 2 -> delay sync",false,"Use the LFO 2 period to control delay timing."),
        toggle("cubic","Mod","Options","Cubic LFO amount curve",true,"Give more control around low modulation amounts."),
        number("crossmod","Mod","Options","LFO 1 -> 2 cross modulation",(0.,100.,1.,0.,"%"),"Let LFO 1 change the frequency of LFO 2."),
        toggle("mpe_clip","Mod","Options","MPE position clipping",false,"Clip position modulation at the sample boundary."),
        choice("matrix_mode","Mod","Options","Panel routing mode",&["One-to-one","Matrix"],"Choose whether destination controls replace a route or preserve multiple routes."),
        number("rec_level","Sample","Recording","Record level",(0.,200.,1.,100.,"%"),"Input gain for the recording buffer. This study has no live audio input."),
        number("monitor","Sample","Recording","Monitor level",(0.,100.,1.,0.,"%"),"Dry input level mixed with the engine output."),
        toggle("loop_rec","Sample","Recording","Looping recording",false,"Continue recording around the sample buffer until stopped."),
        choice("start_mode","Sample","Recording","Record start mode",&["Off","Current position","Wiper","Start"],"Choose where recording resumes in the sample buffer."),
        number("sample_size","Sample","Recording","Sample size",(0.01,112.,0.1,8.,"s"),"Length of the recording buffer. Resize only while recording is stopped."),
        toggle("sampling","Sample","Recording","Sampling enabled",false,"Enable recording into the current sample."),
        toggle("convert","Sample","Import","Convert to 44.1 kHz",true,"Use the instrument sample rate when importing audio."),
        toggle("normalize","Sample","Import","Normalize on load",true,"Normalize a loaded sample before it enters the patch."),
        toggle("preview","Sample","Import","Sound previews",false,"Audition files in the browser. Audio remains disconnected in this study."),
        toggle("pickup","System","Controls","Knob pickup",true,"Pick up stored values before a physical knob takes control."),
        toggle("direct_shift","System","Controls","Direct Shift",true,"On: hold Shift for alternate controls. Off: Shift latches."),
        choice("adsr_mode","System","Controls","ADSR sliders",&["Envelope","Voice volume","Part volume"],"Keep the current slider roles visible on the Play screen."),
        toggle("menu_slider","System","Controls","Position slider in menus",false,"Use Position for menu values, or keep it available for the sound."),
        toggle("invert","System","Controls","Select encoder invert",false,"Reverse menu movement to match the installed encoder."),
        number("leds","System","Controls","LED brightness",(0.,100.,1.,70.,"%"),"Brightness of the physical panel lights."),
        toggle("mpe","System","MIDI","MPE",false,"Use per-note expression. Review pitchbend range when enabling."),
        number("bend","System","MIDI","Pitchbend range",(0.,96.,1.,2.,"st"),"Pitchbend range for MIDI and MPE controllers."),
        toggle("cc_cv","System","MIDI","CC to CV",false,"Use controller messages in place of the CV inputs."),
        toggle("cc_vis","System","MIDI","CC updates display",true,"Reflect external controller changes on the display."),
        toggle("precision","System","MIDI","Precision CC",false,"Send higher-resolution NRPN messages for panel changes."),
        toggle("stop_grains","System","Audio","Patch switch stops grains",true,"Stop old grains on patch changes, or let them finish for smoother transitions."),
        toggle("dsp","System","Audio","DSP meter",false,"Show the engine load meter once telemetry is connected."),
        toggle("samba","System","Device","File sharing (Samba)",false,"Enable network file access on the instrument. Simulated here."),
    ];
    for voice in 1..=4 {
        v.push(number(&format!("voice{voice}_pitch"), "Patch", "Play mode", &format!("Voice {voice} pitch"), (0., 127., 1., 60., "MIDI"), "Demo voice note. Untouched notes follow the slider root; physical slider mapping remains unconnected."));
        v.push(number(
            &format!("voice{voice}_volume"),
            "Patch",
            "Play mode",
            &format!("Voice {voice} volume"),
            (0., 100., 1., 100., "%"),
            "Individual Play-mode voice level.",
        ));
    }
    for prefix in ["grain", "scan"] {
        v.push(number(
            &format!("{prefix}_mul"),
            "Patch",
            "Timing",
            &format!(
                "{} multiplier",
                if prefix == "grain" { "Grain" } else { "Scan" }
            ),
            (1., 16., 1., 1., "x"),
            "Clock multiplier; provisional UI range.",
        ));
        v.push(number(
            &format!("{prefix}_div"),
            "Patch",
            "Timing",
            &format!(
                "{} divider",
                if prefix == "grain" { "Grain" } else { "Scan" }
            ),
            (1., 16., 1., 1., "/"),
            "Clock divider; provisional UI range.",
        ));
    }
    for i in 0..3 {
        v.push(number(
            &format!("zone{i}_offset"),
            "Patch",
            "Keyboard",
            &format!("Area {i}: position offset"),
            (0., 100., 1., 0., "%"),
            "Offset sample position for this keyboard area.",
        ));
        v.push(number(
            &format!("zone{i}_scale"),
            "Patch",
            "Keyboard",
            &format!("Area {i}: keyboard -> position"),
            (-100., 100., 1., 0., "%"),
            "Map keyboard pitch to a position offset within this area.",
        ));
        v.push(toggle(
            &format!("zone{i}_pitch"),
            "Patch",
            "Keyboard",
            &format!("Area {i}: pitch enabled"),
            true,
            "Allow played notes to change pitch in this keyboard area.",
        ));
    }
    for i in 1..=2 {
        let group = if i == 1 { "LFO 1" } else { "LFO 2" };
        v.push(choice(
            &format!("lfo{i}_wave"),
            "Mod",
            group,
            "Waveform",
            &["Sine", "Random", "Saw", "Square", "AUX"],
            "Choose the modulation shape. AUX uses a recorded movement.",
        ));
        v.push(number(
            &format!("lfo{i}_rate"),
            "Mod",
            group,
            "Rate",
            (0., 50., 0.01, 0.12, "Hz"),
            "Modulation frequency. Clock synchronization uses multiplier and divider instead.",
        ));
        v.push(number(
            &format!("lfo{i}_amount"),
            "Mod",
            group,
            "Amount",
            (0., 100., 1., 20., "%"),
            "Depth of this modulation source.",
        ));
        v.push(toggle(
            &format!("lfo{i}_sync"),
            "Mod",
            group,
            "Clock sync",
            false,
            "Synchronize the LFO to incoming MIDI clock.",
        ));
        v.push(toggle(
            &format!("lfo{i}_key"),
            "Mod",
            group,
            "Key sync",
            false,
            "Restart the LFO on a note trigger.",
        ));
        v.push(number(
            &format!("lfo{i}_max"),
            "Mod",
            group,
            "Maximum frequency",
            (0., 50., 0.01, 50., "Hz"),
            "Set the physical frequency control's range.",
        ));
        v.push(toggle(
            &format!("lfo{i}_wheel"),
            "Mod",
            group,
            "Mod wheel -> amount",
            false,
            "Scale the source depth using the modulation wheel.",
        ));
        v.push(number(
            &format!("lfo{i}_quant"),
            "Mod",
            group,
            "Phase quantization",
            (1., 1024., 1., 1., ""),
            "One leaves the waveform smooth. Higher values make stepped movement.",
        ));
        v.push(number(
            &format!("lfo{i}_mul"),
            "Mod",
            group,
            "Clock multiplier",
            (1., 16., 1., 1., "x"),
            "Multiplier when clock sync is enabled.",
        ));
        v.push(number(
            &format!("lfo{i}_div"),
            "Mod",
            group,
            "Clock divider",
            (1., 16., 1., 1., "/"),
            "Divider when clock sync is enabled.",
        ));
        v.push(number(
            &format!("cv{i}_amount"),
            "Mod",
            "CV",
            "CV amount",
            (0., 100., 1., 0., "%"),
            "Depth for the CV source. Assign destinations in Routes.",
        ));
        v.last_mut().unwrap().name = format!("CV {i} amount");
    }
    for i in 1..=4 {
        let mut s = number(
            &format!("midi{i}"),
            "System",
            "Parts",
            &format!("Part {i} MIDI channel"),
            (0., 16., 1., i as f32, ""),
            "Zero disables MIDI input for this part.",
        );
        s.system = true;
        v.push(s);
    }
    for (id, name, def, max, unit) in [
        ("attack", "Attack", 0.3, 12., "s"),
        ("decay", "Decay", 1.2, 12., "s"),
        ("sustain", "Sustain", 70., 100., "%"),
        ("release", "Release", 2.4, 12., "s"),
        ("sides", "Grain window sides", 50., 100., "%"),
        ("tilt", "Grain window tilt", 50., 100., "%"),
        ("curve", "Grain window curve", 50., 100., "%"),
    ] {
        v.push(number(
            id,
            "Patch",
            "Envelope",
            name,
            (0., max, if max == 12. { 0.1 } else { 1. }, def, unit),
            "Shape the envelope while retaining the existing physical slider and window controls.",
        ));
    }
    for (id, page, group, label) in placement() {
        let s = v
            .iter_mut()
            .find(|s| s.id == id)
            .expect("placed setting exists");
        s.page = page;
        s.group = group;
        s.label = label.into();
    }
    v
}

/// The Patch root's pages .
pub const PATCH_PAGES: [&str; 9] = [
    "Play", "Timing", "Shape", "FX", "Voices", "Keys", "Matrix", "LFO 1", "LFO 2",
];

/// Where the Patch root shows each patch and modulation setting , in focus order within each group: id, page, group and the
/// panel's word for it.
pub fn placement() -> Vec<(String, &'static str, &'static str, String)> {
    let mut v: Vec<(String, &'static str, &'static str, String)> = vec![];
    let mut put = |id: &str, page, group, label: &str| {
        v.push((id.into(), page, group, label.into()));
    };
    for (id, label) in [
        ("sample_mode", "Sample mode"),
        ("pitch", "Pitch"),
        ("tape_slew", "Tape slew"),
        ("patch_volume", "Patch volume"),
        ("spawn", "Spawn mode"),
    ] {
        put(id, "Play", "Playback", label);
    }
    for (id, label) in [
        ("stealing", "Stealing threshold"),
        ("anti_alias", "Anti-aliasing"),
        ("glide", "Granular glide"),
        ("sample_loop", "Loop confinement"),
    ] {
        put(id, "Play", "Grains", label);
    }
    for (id, label) in [
        ("grain_key", "Key sync"),
        ("grain_clock", "Clock sync"),
        ("grain_mul", "Multiplier"),
        ("grain_div", "Divider"),
    ] {
        put(id, "Timing", "Grains", label);
    }
    for (id, label) in [
        ("scan_key", "Key sync"),
        ("scan_mode", "Mode"),
        ("scan_loop", "Loop confinement"),
        ("scan_clock", "Clock sync"),
        ("scan_mul", "Multiplier"),
        ("scan_div", "Divider"),
    ] {
        put(id, "Timing", "Scan", label);
    }
    for (id, label) in [
        ("attack", "Attack"),
        ("decay", "Decay"),
        ("sustain", "Sustain"),
        ("release", "Release"),
    ] {
        put(id, "Shape", "Amplitude", label);
    }
    for (id, label) in [("sides", "Sides"), ("tilt", "Tilt"), ("curve", "Curve")] {
        put(id, "Shape", "Grain window", label);
    }
    put("fx_map", "FX", "Knobs", "FX knobs control");
    for (id, label) in [
        ("fx_type", "Type"),
        ("delay_time", "Time"),
        ("delay_feedback", "Feedback"),
        ("distortion", "Amount"),
        ("bit_reduction", "Bits"),
        ("rate_reduction", "Rate"),
        ("reverb_time", "Time"),
        ("reverb_amount", "Amount"),
    ] {
        put(id, "FX", "Effect", label);
    }
    for (id, label) in [
        ("filter", "Mode"),
        ("cutoff", "Cutoff"),
        ("resonance", "Resonance"),
    ] {
        put(id, "FX", "Filter", label);
    }
    for (id, label) in [
        ("sub_on", "Sub-osc"),
        ("sub_freq", "Tuning"),
        ("sub_amp", "Level"),
        ("sub_balance", "Balance"),
    ] {
        put(id, "FX", "Sub-oscillator", label);
    }
    for (id, label) in [
        ("responds", "Responds to Play"),
        ("root", "Slider root note"),
        ("range", "Slider range"),
        ("free_scale", "Free scale"),
    ] {
        put(id, "Voices", "Play mode", label);
    }
    for i in 1..=4 {
        put(&format!("voice{i}_pitch"), "Voices", "Voices", "Pitch");
    }
    for i in 1..=4 {
        put(&format!("voice{i}_volume"), "Voices", "Voices", "Level");
    }
    for (id, label) in [
        ("tuning", "Tuning"),
        ("split1", "Split 1"),
        ("split2", "Split 2"),
    ] {
        put(id, "Keys", "Keyboard", label);
    }
    for (i, group) in ["Area 0", "Area 1", "Area 2"].into_iter().enumerate() {
        put(&format!("zone{i}_offset"), "Keys", group, "Position offset");
        put(&format!("zone{i}_scale"), "Keys", group, "Keys to position");
        put(&format!("zone{i}_pitch"), "Keys", group, "Pitch");
    }
    for (id, label) in [
        ("cv1_amount", "CV 1 amount"),
        ("cv2_amount", "CV 2 amount"),
        ("matrix_mode", "Panel routing"),
        ("cubic", "Amount curve"),
        ("mpe_clip", "MPE position clip"),
    ] {
        put(id, "Matrix", "Matrix", label);
    }
    for (i, page) in [(1, "LFO 1"), (2, "LFO 2")] {
        for (suffix, label) in [
            ("wave", "Wave"),
            ("rate", "Rate"),
            ("amount", "Amount"),
            ("sync", "Clock sync"),
            ("mul", "Multiplier"),
            ("div", "Divider"),
            ("key", "Key sync"),
            ("max", "Max frequency"),
            ("wheel", "Mod wheel to amount"),
            ("quant", "Phase quantize"),
        ] {
            put(&format!("lfo{i}_{suffix}"), page, page, label);
        }
    }
    put("crossmod", "LFO 1", "LFO 1", "Cross-mod to LFO 2");
    put("lfo_delay", "LFO 2", "LFO 2", "LFO 2 to delay sync");
    v
}
pub const SOURCES: [&str; 9] = [
    "LFO 1",
    "LFO 2",
    "CV 1",
    "CV 2",
    "Timbre",
    "Aftertouch",
    "Lift",
    "Velocity",
    "Pitch",
];
/// Stock's short destination names for the matrix grid (manual p.29).
pub const DESTINATION_SHORT: [&str; 11] = [
    "Tune", "Pos", "Spray", "Size", "Dens", "Amp", "PSpr", "Cutoff", "Sides", "Tilt", "Curve",
];
pub const DESTINATIONS: [&str; 11] = [
    "Tune",
    "Position",
    "Spray",
    "Size",
    "Density",
    "Amplitude",
    "Pan spray",
    "Cutoff",
    "Sides",
    "Tilt",
    "Curve",
];
pub fn route_allowed(source: usize, dest: usize) -> bool {
    source < 4 || (dest != 0 && dest < 8 && (dest != 7 || source == 8))
}
pub fn groups(page: &str) -> Vec<&'static str> {
    match page {
        "Sample" => vec!["Recording", "Import"],
        _ => vec![],
    }
}
