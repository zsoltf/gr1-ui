use std::time::{Duration, Instant};

/// Opt-in measurements from the running path, not a target-frame-rate claim.
pub(super) struct FrameTiming {
    enabled: bool,
    last: Option<Instant>,
    intervals: Vec<f64>,
    stages: [f64; 4],
    frames: usize,
    event_max: f64,
}
impl FrameTiming {
    pub fn new() -> Self {
        Self {
            enabled: std::env::var_os("GR1_UI_TRACE").is_some(),
            last: None,
            intervals: Vec::with_capacity(300),
            stages: [0.; 4],
            frames: 0,
            event_max: 0.,
        }
    }
    pub fn frame(
        &mut self,
        poll: Duration,
        sync: Duration,
        draw: Duration,
        present: Duration,
        input: Option<Instant>,
    ) {
        if !self.enabled {
            return;
        }
        let now = Instant::now();
        if let Some(last) = self.last {
            self.intervals.push((now - last).as_secs_f64() * 1000.);
        }
        self.last = Some(now);
        self.frames += 1;
        for (i, t) in [poll, sync, draw, present].into_iter().enumerate() {
            self.stages[i] += t.as_secs_f64() * 1000.;
        }
        if let Some(input) = input {
            self.event_max = self.event_max.max((now - input).as_secs_f64() * 1000.);
        }
        if self.intervals.len() == 300 {
            self.intervals.sort_by(f64::total_cmp);
            eprintln!("[gr1-frame] interval_ms p50={:.2} p95={:.2} p99={:.2} max={:.2}; mean_ms native={:.2} view={:.2} draw={:.2} present={:.2}; received_event_to_present_max_ms={:.2}",
                self.intervals[150],self.intervals[285],self.intervals[297],self.intervals[299],self.stages[0]/self.frames as f64,self.stages[1]/self.frames as f64,self.stages[2]/self.frames as f64,self.stages[3]/self.frames as f64,self.event_max);
            self.intervals.clear();
            self.stages = [0.; 4];
            self.frames = 0;
            self.event_max = 0.;
        }
    }
}

/// What the readback check compares from one poll to the next: the grain
/// coordinates, the focused part's scan, position and spray, and both LFO
/// outputs .
#[derive(Clone, Default, PartialEq)]
pub(super) struct Readback {
    pub grains: Vec<(f32, f32)>,
    pub scan: f32,
    pub position: f32,
    pub spray: f32,
    pub lfo: [f32; 2],
}

/// GR1_UI_TRACE=1: once per second, and whenever the screen id or the
/// focused part changes, one `[gr1-readback]` line: the screen id the engine
/// ran under, whether each readback changed during that span, the most grains
/// seen, and the cost of drawing the live strip on the frames that drew it.
pub(super) struct ReadbackTrace {
    enabled: bool,
    since: Option<Instant>,
    screen: Option<i32>,
    part: usize,
    frames: usize,
    last: Option<Readback>,
    /// grains, scan, position, spray, LFO 1, LFO 2
    changed: [bool; 6],
    grains_max: usize,
    strip_frames: usize,
    strip_total: f64,
    strip_max: f64,
    strip_mode: &'static str,
}
impl ReadbackTrace {
    pub fn new() -> Self {
        Self::with(std::env::var_os("GR1_UI_TRACE").is_some())
    }
    pub fn with(enabled: bool) -> Self {
        Self {
            enabled,
            since: None,
            screen: None,
            part: 0,
            frames: 0,
            last: None,
            changed: [false; 6],
            grains_max: 0,
            strip_frames: 0,
            strip_total: 0.,
            strip_max: 0.,
            strip_mode: "none",
        }
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    /// One frame's readback under `screen`, and the strip's drawing time and
    /// mode if this frame drew it. Returns the line when a span ends.
    pub fn frame(
        &mut self,
        now: Instant,
        t: f64,
        screen: Option<i32>,
        part: usize,
        readback: Readback,
        strip: Option<(Duration, &'static str)>,
    ) -> Option<String> {
        if !self.enabled {
            return None;
        }
        let mut line = None;
        let started = *self.since.get_or_insert(now);
        if self.frames > 0
            && (screen != self.screen
                || part != self.part
                || now - started >= Duration::from_secs(1))
        {
            line = Some(self.line(t, (now - started).as_secs_f64()));
            self.since = Some(now);
            self.frames = 0;
            self.changed = [false; 6];
            self.grains_max = 0;
            self.strip_frames = 0;
            self.strip_total = 0.;
            self.strip_max = 0.;
            self.strip_mode = "none";
        }
        self.screen = screen;
        self.part = part;
        self.frames += 1;
        if let Some(last) = &self.last {
            let differs = |a: f32, b: f32| a.to_bits() != b.to_bits();
            for (flag, moved) in self.changed.iter_mut().zip([
                last.grains != readback.grains,
                differs(last.scan, readback.scan),
                differs(last.position, readback.position),
                differs(last.spray, readback.spray),
                differs(last.lfo[0], readback.lfo[0]),
                differs(last.lfo[1], readback.lfo[1]),
            ]) {
                *flag |= moved;
            }
        }
        self.grains_max = self.grains_max.max(readback.grains.len());
        self.last = Some(readback);
        if let Some((time, mode)) = strip {
            let ms = time.as_secs_f64() * 1000.;
            self.strip_frames += 1;
            self.strip_total += ms;
            self.strip_max = self.strip_max.max(ms);
            self.strip_mode = mode;
        }
        line
    }
    fn line(&self, t: f64, seconds: f64) -> String {
        let [grains, scan, position, spray, lfo1, lfo2] = self.changed.map(u8::from);
        let mut line = format!(
            "[gr1-readback] t={t:.3} screen={} seconds={seconds:.2} frames={} part={} \
             changed grains={grains} scan={scan} position={position} spray={spray} \
             lfo1={lfo1} lfo2={lfo2} grains_max={} strip={}",
            self.screen.map_or("-".into(), |s| s.to_string()),
            self.frames,
            self.part + 1,
            self.grains_max,
            self.strip_mode,
        );
        if self.strip_frames > 0 {
            line += &format!(
                " strip_frames={} strip_draw_ms_mean={:.3} strip_draw_ms_max={:.3}",
                self.strip_frames,
                self.strip_total / self.strip_frames as f64,
                self.strip_max
            );
        }
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(scan: f32, lfo: f32, grains: usize) -> Readback {
        Readback {
            grains: vec![(0.5, 0.5); grains],
            scan,
            position: 0.25,
            spray: 0.1,
            lfo: [lfo, 0.],
        }
    }

    #[test]
    fn readback_line_reports_each_second_and_each_screen_change() {
        let mut trace = ReadbackTrace::with(true);
        let start = Instant::now();
        let ms = |n: u64| start + Duration::from_millis(n);
        let strip = Some((Duration::from_micros(400), "live-60hz"));
        assert!(trace
            .frame(ms(0), 0., Some(4), 0, sample(0.1, 0.0, 3), strip)
            .is_none());
        assert!(trace
            .frame(ms(500), 0.5, Some(4), 0, sample(0.2, 0.5, 5), strip)
            .is_none());
        let line = trace
            .frame(ms(1000), 1., Some(4), 0, sample(0.2, 0.5, 5), None)
            .expect("one line per second");
        assert!(
            line.starts_with("[gr1-readback] t=1.000 screen=4 seconds=1.00 frames=2 part=1"),
            "{line}"
        );
        assert!(
            line.contains(
                "changed grains=1 scan=1 position=0 spray=0 lfo1=1 lfo2=0 grains_max=5 \
                 strip=live-60hz strip_frames=2 strip_draw_ms_mean=0.400 strip_draw_ms_max=0.400"
            ),
            "{line}"
        );
        // A new screen id closes the span at once; nothing moved under it.
        let line = trace
            .frame(ms(1100), 1.1, Some(0), 0, sample(0.2, 0.5, 5), None)
            .expect("a screen change ends the span");
        assert!(line.contains("screen=4 seconds=0.10 frames=1"), "{line}");
        assert!(
            line.contains(
                "changed grains=0 scan=0 position=0 spray=0 lfo1=0 lfo2=0 grains_max=5 strip=none"
            ),
            "{line}"
        );
        assert!(!line.contains("strip_frames"), "{line}");
    }

    #[test]
    fn the_trace_is_silent_unless_enabled() {
        let mut trace = ReadbackTrace::with(false);
        let now = Instant::now();
        for i in 0..200 {
            let at = now + Duration::from_millis(i * 20);
            assert!(trace
                .frame(at, 0., Some(4), 0, sample(i as f32, 0., 1), None)
                .is_none());
        }
    }
}
