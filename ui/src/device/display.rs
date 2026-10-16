//! Display readback stays on the same serialized native caller as synthesis control.
use super::Device;
use std::{collections::VecDeque, time::Instant};

pub const WAVE_COLUMNS: usize = 544;
const STENCIL_HEIGHT: i32 = 1024;

pub struct Waveform {
    pub frames: i32,
    /// Native bottom-up coordinates, normalized to 0..1 without losing polarity.
    pub columns: Vec<(f32, f32)>,
    raw: Vec<f32>,
    recording_position: f32,
}
impl Default for Waveform {
    fn default() -> Self {
        Self {
            frames: -1,
            columns: vec![],
            raw: vec![0.; WAVE_COLUMNS * 4],
            recording_position: 0.,
        }
    }
}

/// Pixel columns of the main screen's LFO traces.
pub const LFO_COLUMNS: usize = 222;
const LFO_SHORTEST: f64 = 2.;
const LFO_LONGEST: f64 = 60.;

/// The trace window: the longer of 2 seconds and two periods at `rate` Hz,
/// capped at 60 seconds .
pub fn lfo_window(rate: f32) -> f64 {
    if rate.is_nan() {
        return LFO_SHORTEST;
    }
    (2. / f64::from(rate.max(0.))).clamp(LFO_SHORTEST, LFO_LONGEST)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LfoBucket {
    /// Position on the time axis in units of the current bucket duration.
    pub index: i64,
    pub min: f32,
    pub max: f32,
    pub last: f32,
}

/// One engine LFO output kept as one minimum/maximum bucket per pixel column.
/// Bucket durations are 2 s / LFO_COLUMNS times a power of two, so a longer
/// window merges neighbours instead of keeping more observations.
#[derive(Default)]
pub struct LfoTrace {
    level: u32,
    buckets: VecDeque<LfoBucket>,
}
impl LfoTrace {
    pub fn bucket_seconds(&self) -> f64 {
        Self::duration(self.level)
    }
    fn duration(level: u32) -> f64 {
        LFO_SHORTEST / LFO_COLUMNS as f64 * f64::from(1u32 << level)
    }
    fn level_for(window: f64) -> u32 {
        (0..=5)
            .find(|level| Self::duration(*level) * LFO_COLUMNS as f64 >= window - 1e-9)
            .unwrap_or(5)
    }
    pub fn clear(&mut self) {
        self.buckets.clear();
    }
    pub fn buckets(&self) -> &VecDeque<LfoBucket> {
        &self.buckets
    }
    /// The bucket index `seconds` falls in at the current level.
    pub fn index_at(&self, seconds: f64) -> i64 {
        (seconds / self.bucket_seconds()).floor() as i64
    }
    pub fn push(&mut self, seconds: f64, value: f32, window: f64) {
        let level = Self::level_for(window);
        while self.level < level {
            // Merge pairs of neighbours: the history stays, at half the resolution.
            let mut merged: VecDeque<LfoBucket> = VecDeque::with_capacity(LFO_COLUMNS);
            for b in self.buckets.drain(..) {
                let index = b.index.div_euclid(2);
                match merged.back_mut() {
                    Some(m) if m.index == index => {
                        m.min = m.min.min(b.min);
                        m.max = m.max.max(b.max);
                        m.last = b.last;
                    }
                    _ => merged.push_back(LfoBucket { index, ..b }),
                }
            }
            self.buckets = merged;
            self.level += 1;
        }
        while self.level > level {
            // A shorter window cannot recover detail; old buckets are repeated.
            let split: VecDeque<_> = self
                .buckets
                .drain(..)
                .flat_map(|b| {
                    [0, 1].map(|half| LfoBucket {
                        index: b.index * 2 + half,
                        ..b
                    })
                })
                .collect();
            self.buckets = split;
            self.level -= 1;
        }
        let index = self.index_at(seconds);
        match self.buckets.back_mut() {
            Some(b) if b.index == index => {
                b.min = b.min.min(value);
                b.max = b.max.max(value);
                b.last = value;
            }
            _ => self.buckets.push_back(LfoBucket {
                index,
                min: value,
                max: value,
                last: value,
            }),
        }
        while self.buckets.len() > LFO_COLUMNS
            || self
                .buckets
                .front()
                .is_some_and(|b| b.index <= index - LFO_COLUMNS as i64)
        {
            self.buckets.pop_front();
        }
    }
}

pub struct Display {
    pub waves: [Waveform; 32],
    pub slots: [usize; 4],
    pub grains: Vec<(f32, f32)>,
    pub scan: [f32; 4],
    pub position: [f32; 4],
    pub spray: [f32; 4],
    pub grain_window: [f32; 256],
    pub envelope: [f32; 4],
    pub lfo_speeds: [f32; 2],
    pub lfo_history: VecDeque<(Instant, [f32; 2])>,
    /// The main screen's adaptive traces of the same readback.
    pub lfo_traces: [LfoTrace; 2],
    pub epoch: Instant,
    lfo_slot: Option<usize>,
    pub amounts: [f32; 4],
    pub record_position: Option<f32>,
    pub trace: bool,
    pub generation: u64,
}
impl Default for Display {
    fn default() -> Self {
        Self {
            waves: std::array::from_fn(|_| Waveform::default()),
            slots: [0, 1, 2, 3],
            grains: Vec::with_capacity(128),
            scan: [0.; 4],
            position: [0.; 4],
            spray: [0.; 4],
            grain_window: [0.; 256],
            envelope: [0.; 4],
            lfo_speeds: [0.; 2],
            lfo_history: VecDeque::with_capacity(256),
            lfo_traces: Default::default(),
            epoch: Instant::now(),
            lfo_slot: None,
            amounts: [0.; 4],
            record_position: None,
            trace: std::env::var_os("GR1_UI_TRACE").is_some(),
            generation: 0,
        }
    }
}
impl Display {
    pub fn invalidate(&mut self) {
        self.lfo_history.clear();
        self.lfo_traces.iter_mut().for_each(LfoTrace::clear);
        self.lfo_slot = None;
        for w in &mut self.waves {
            w.frames = -1;
        }
    }
}

/// Stock's stencil is x,y_min,x,y_max, in upward-positive pixel coordinates.
pub fn stencil_columns(raw: &[f32], height: i32) -> Vec<(f32, f32)> {
    raw.chunks_exact(4)
        .map(|p| (p[1] / height as f32, p[3] / height as f32))
        .collect()
}

impl Device {
    pub(super) fn poll_display(&mut self) {
        let part = self.part();
        let slot = self.slot();
        if self.display.lfo_slot != Some(slot) {
            self.display.lfo_history.clear();
            self.display.lfo_traces.iter_mut().for_each(LfoTrace::clear);
            self.display.lfo_slot = Some(slot);
        }
        let now = Instant::now();
        let lfo_values = self.engine.lfo_outputs();
        if let Some(values) = lfo_values {
            self.display.lfo_history.push_back((now, values));
        }
        while self.display.lfo_history.len() > 256
            || self
                .display
                .lfo_history
                .front()
                .is_some_and(|(t, _)| now.duration_since(*t).as_secs_f32() > 2.)
        {
            self.display.lfo_history.pop_front();
        }
        let state = self.states[self.slot()];
        let enabled = self.performance.multis[self.performance.current].partenables;
        let api = &self.engine.api;
        unsafe {
            (api.parts_get_scan_array)(self.display.scan.as_mut_ptr());
            (api.parts_get_pos_array)(self.display.position.as_mut_ptr());
            (api.parts_get_spray_array)(self.display.spray.as_mut_ptr());
            (api.gr_fetch_grainwin)(self.display.grain_window.as_mut_ptr());
            (api.gr_get_lfo_speeds)(self.display.lfo_speeds.as_mut_ptr());
            if let Some(values) = lfo_values {
                let seconds = now.duration_since(self.display.epoch).as_secs_f64();
                for (lfo, trace) in self.display.lfo_traces.iter_mut().enumerate() {
                    let window = lfo_window(self.display.lfo_speeds[lfo]);
                    trace.push(seconds, values[lfo], window);
                }
            }
            self.display.amounts = [
                (api.gr_get_lfo1amt)(),
                (api.gr_get_lfo2amt)(),
                (api.gr_get_cv1amt)(),
                (api.gr_get_cv2amt)(),
            ];
            self.display.envelope = [
                (api.conv_lin_time_to_frac)(state.attack_time),
                (api.conv_exp_time_to_frac)(state.decay_time),
                state.sustain_level,
                (api.conv_exp_time_to_frac)(state.release_time),
            ];
            self.display.record_position = self.recording.map(|_| (api.gr_get_recpos)());
            for (i, enabled) in enabled.iter().enumerate() {
                let slot = (api.gr_get_part_patchnum)(i as i32);
                if !(0..32).contains(&slot) {
                    continue;
                }
                let slot = slot as usize;
                self.display.slots[i] = slot;
                if *enabled == 0 && i != part {
                    continue;
                }
                let frames = (api.gr_get_samplelength)(slot as i32);
                let w = &mut self.display.waves[slot];
                let changed = w.frames != frames;
                let recording = self.recording == Some(slot);
                if changed || recording {
                    if frames <= 0 {
                        w.raw.fill(STENCIL_HEIGHT as f32 / 2.);
                    } else if changed {
                        // Explicit part->slot, as stock does; never key the cache only by focus.
                        w.raw.fill(f32::NAN);
                        (api.gr_calc_stencil)(
                            slot as i32,
                            WAVE_COLUMNS as i32,
                            STENCIL_HEIGHT,
                            0,
                            w.raw.as_mut_ptr(),
                        );
                    } else {
                        (api.gr_calc_stencil_rec)(
                            slot as i32,
                            WAVE_COLUMNS as i32,
                            STENCIL_HEIGHT,
                            0,
                            w.raw.as_mut_ptr(),
                            w.recording_position,
                        );
                    }
                    w.columns = stencil_columns(&w.raw, STENCIL_HEIGHT);
                    w.frames = frames;
                    w.recording_position = self.display.record_position.unwrap_or(0.);
                    if changed {
                        self.display.generation += 1;
                        self.documents[slot].put("samplesize", frames.max(0) as f32 / 44100.);
                    }
                    if self.display.trace && changed {
                        let valid = w
                            .columns
                            .iter()
                            .filter(|(a, b)| a.is_finite() && b.is_finite())
                            .count();
                        let peak = w
                            .columns
                            .iter()
                            .map(|(a, b)| (b - a).abs())
                            .fold(0., f32::max);
                        eprintln!("[gr1-display] part={} slot={} frames={} columns={} valid={} peak={:.5}",i+1,slot,frames,w.columns.len(),valid,peak);
                    }
                }
            }
            let mut x = [0.; 128];
            let mut y = [0.; 128];
            let mut count = 0;
            if self.display.waves[self.display.slots[part]].frames > 0 {
                (api.gr_get_grain_positions)(-1, x.as_mut_ptr(), y.as_mut_ptr(), &mut count);
            }
            self.display.grains.clear();
            self.display.grains.extend(
                x.into_iter()
                    .zip(y)
                    .take(count.clamp(0, 128) as usize)
                    .filter(|(x, y)| x.is_finite() && y.is_finite()),
            );
        }
    }
}

impl Device {
    /// Stencils of the presets no enabled part plays, for the mini waveforms
    /// of the Perf root . Each frame computes at most `budget`
    /// stencils, and a preset only when its sample length changed or the
    /// cache was invalidated; poll_display keeps the playing presets current.
    pub fn fill_stencils(&mut self, budget: usize) {
        let multi = self.performance.multis[self.performance.current];
        let playing: Vec<usize> = (0..4)
            .filter(|i| multi.partenables[*i] != 0 || *i == self.part())
            .map(|i| self.display.slots[i])
            .collect();
        let mut left = budget;
        for slot in 0..32 {
            if left == 0 {
                break;
            }
            if playing.contains(&slot) || self.recording == Some(slot) {
                continue;
            }
            let api = &self.engine.api;
            let frames = unsafe { (api.gr_get_samplelength)(slot as i32) };
            let w = &mut self.display.waves[slot];
            if w.frames == frames {
                continue;
            }
            if frames <= 0 {
                w.raw.fill(STENCIL_HEIGHT as f32 / 2.);
            } else {
                w.raw.fill(f32::NAN);
                unsafe {
                    (api.gr_calc_stencil)(
                        slot as i32,
                        WAVE_COLUMNS as i32,
                        STENCIL_HEIGHT,
                        0,
                        w.raw.as_mut_ptr(),
                    );
                }
                left -= 1;
            }
            w.columns = stencil_columns(&w.raw, STENCIL_HEIGHT);
            w.frames = frames;
            self.documents[slot].put("samplesize", frames.max(0) as f32 / 44100.);
        }
    }
}
