//! ScoreArt's sound engine: a mixer with one track per instrument. Each track plays either
//! the built-in synthesizer (sounds generated in code, no samples needed), a preset of a
//! SoundFont 2 file, or an SFZ sample instrument the user loaded.
//!
//! [`Synth`] streams interleaved stereo `f32` frames from a [`Sequence`]; the platform (Web Audio,
//! cpal) pulls blocks with [`Synth::render`]. [`MixControls`] are shared with the UI so volume,
//! pan, mute and solo change while playing, and level meters can be shown.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod bank;
mod builtin;
pub mod catalog;
mod catalog_index;
pub mod files;
pub mod reverb;
pub mod sfz;
pub mod shape;
pub mod variants;
pub mod wav;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub use bank::{PresetInfo, Resolved, SoundBank, resolve, resolve_variant};
use builtin::Builtin;
pub use builtin::Family;
use scoreart_midi::{PlayNote, Sequence};
use scoreart_model::Score;
use sfz::SfzPlayer;
use shape::Shape;
use variants::Variant;

/// Frames rendered per step; events are placed on the exact frame inside it.
const BLOCK: usize = 64;
/// Master gain before the soft limiter.
const MASTER: f32 = 0.72;
/// Level of SoundFont output relative to the built-in synthesizer.
const SF2_GAIN: f32 = 6.5;
/// Level of SFZ output relative to the built-in synthesizer.
const SFZ_GAIN: f32 = 2.0;

/// Live mixer settings of one track, shared between the UI and the audio thread.
#[derive(Debug)]
pub struct TrackControl {
    volume: AtomicU32,
    pan: AtomicU32,
    mute: AtomicBool,
    solo: AtomicBool,
    peak: AtomicU32,
}

impl TrackControl {
    fn new(volume: f32, pan: f32) -> TrackControl {
        TrackControl {
            volume: AtomicU32::new(volume.clamp(0.0, 1.0).to_bits()),
            pan: AtomicU32::new(pan.clamp(-1.0, 1.0).to_bits()),
            mute: AtomicBool::new(false),
            solo: AtomicBool::new(false),
            peak: AtomicU32::new(0f32.to_bits()),
        }
    }
}

/// Mixer settings of every track (one per part), changeable while playing.
#[derive(Debug, Default)]
pub struct MixControls {
    tracks: Vec<TrackControl>,
    /// The room, 0 (dry) … 1.
    reverb: AtomicU32,
}

impl MixControls {
    /// Volume and pan from the score; nothing muted or soloed.
    pub fn from_score(score: &Score) -> MixControls {
        MixControls {
            tracks: score
                .parts
                .iter()
                .map(|p| TrackControl::new(f32::from(p.sound.volume) / 100.0, f32::from(p.sound.pan) / 100.0))
                .collect(),
            reverb: AtomicU32::new((f32::from(score.playback.reverb.min(100)) / 100.0).to_bits()),
        }
    }

    /// 0 (dry) … 1.
    pub fn set_reverb(&self, v: f32) {
        self.reverb.store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn reverb(&self) -> f32 {
        f32::from_bits(self.reverb.load(Ordering::Relaxed))
    }

    fn uniform(n: usize) -> MixControls {
        MixControls {
            tracks: (0..n).map(|_| TrackControl::new(0.8, 0.0)).collect(),
            reverb: AtomicU32::new(0f32.to_bits()),
        }
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// 0…1 (the score's volume / 100).
    pub fn set_volume(&self, track: usize, v: f32) {
        if let Some(t) = self.tracks.get(track) {
            t.volume.store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
        }
    }

    pub fn volume(&self, track: usize) -> f32 {
        self.tracks
            .get(track)
            .map_or(0.0, |t| f32::from_bits(t.volume.load(Ordering::Relaxed)))
    }

    /// -1 (left) … 1 (right).
    pub fn set_pan(&self, track: usize, v: f32) {
        if let Some(t) = self.tracks.get(track) {
            t.pan.store(v.clamp(-1.0, 1.0).to_bits(), Ordering::Relaxed);
        }
    }

    pub fn pan(&self, track: usize) -> f32 {
        self.tracks
            .get(track)
            .map_or(0.0, |t| f32::from_bits(t.pan.load(Ordering::Relaxed)))
    }

    pub fn set_mute(&self, track: usize, on: bool) {
        if let Some(t) = self.tracks.get(track) {
            t.mute.store(on, Ordering::Relaxed);
        }
    }

    pub fn mute(&self, track: usize) -> bool {
        self.tracks
            .get(track)
            .is_some_and(|t| t.mute.load(Ordering::Relaxed))
    }

    pub fn set_solo(&self, track: usize, on: bool) {
        if let Some(t) = self.tracks.get(track) {
            t.solo.store(on, Ordering::Relaxed);
        }
    }

    pub fn solo(&self, track: usize) -> bool {
        self.tracks
            .get(track)
            .is_some_and(|t| t.solo.load(Ordering::Relaxed))
    }

    /// Whether the track is heard (not muted, and soloed if any track is).
    pub fn audible(&self, track: usize) -> bool {
        let any_solo = self.tracks.iter().any(|t| t.solo.load(Ordering::Relaxed));
        !self.mute(track) && (!any_solo || self.solo(track))
    }

    /// The highest level since the last call (0…1+), for a meter.
    pub fn take_peak(&self, track: usize) -> f32 {
        self.tracks.get(track).map_or(0.0, |t| {
            f32::from_bits(t.peak.swap(0f32.to_bits(), Ordering::Relaxed))
        })
    }

    fn add_peak(&self, track: usize, v: f32) {
        if let Some(t) = self.tracks.get(track) {
            let old = f32::from_bits(t.peak.load(Ordering::Relaxed));
            if v > old {
                t.peak.store(v.to_bits(), Ordering::Relaxed);
            }
        }
    }

    /// Linear gain and equal-power left/right factors of a track.
    fn gains(&self, track: usize) -> (f32, f32) {
        let v = self.volume(track) / 0.8;
        let g = v * v;
        let angle = (self.pan(track) + 1.0) * std::f32::consts::FRAC_PI_4;
        let k = std::f32::consts::SQRT_2;
        (g * angle.cos() * k, g * angle.sin() * k)
    }
}

/// A SoundFont channel and the note it plays.
struct Chan {
    channel: i32,
    key: Option<u8>,
    shape: Option<Shape>,
    /// Seconds since its last note started.
    age: f32,
    /// The expression last sent (14 bits).
    sent: i32,
}

impl Chan {
    fn new(channel: i32) -> Chan {
        Chan {
            channel,
            key: None,
            shape: None,
            age: 1e6,
            sent: 16_383,
        }
    }

    /// Sends the shape's level as expression (CC 11 + 43); the synthesizer squares it.
    fn express(&mut self, synth: &mut rustysynth::Synthesizer) {
        let g = self.shape.map_or(1.0, |s| s.gain(self.age)).clamp(0.0, 1.0);
        let v = (g.sqrt() * 16_383.0).round() as i32;
        if v != self.sent {
            self.sent = v;
            let ch = self.channel;
            let _ = catch_unwind(AssertUnwindSafe(|| {
                synth.process_midi_message(ch, 0xB0, 11, v >> 7);
                synth.process_midi_message(ch, 0xB0, 43, v & 0x7F);
            }));
        }
    }
}

enum Source {
    Builtin(Builtin),
    Sf2 {
        synth: Box<rustysynth::Synthesizer>,
        /// The channels notes are spread over, so that each can be shaped on its own.
        chans: Vec<Chan>,
        rate: f32,
        scratch: (Vec<f32>, Vec<f32>),
        dead: bool,
    },
    Sfz(Box<SfzPlayer>),
}

impl Source {
    fn new(r: &Resolved, rate: u32) -> Source {
        match r {
            Resolved::Builtin(f) => Source::Builtin(Builtin::new(*f, rate)),
            Resolved::Sf2 { font, bank, program } => {
                let mut settings = rustysynth::SynthesizerSettings::new(rate.clamp(16_000, 192_000) as i32);
                settings.maximum_polyphony = 96;
                let made = catch_unwind(AssertUnwindSafe(|| rustysynth::Synthesizer::new(font, &settings)));
                match made {
                    Ok(Ok(mut synth)) => {
                        synth.set_master_volume(1.0);
                        // Percussion lives on channel 10; melodic sounds use all the others.
                        let channels: Vec<i32> = if *bank == 128 {
                            vec![9]
                        } else {
                            (0..16).filter(|c| *c != 9).collect()
                        };
                        let _ = catch_unwind(AssertUnwindSafe(|| {
                            for &c in &channels {
                                synth.process_midi_message(c, 0xB0, 0, i32::from(*bank).min(127));
                                synth.process_midi_message(c, 0xC0, i32::from(*program), 0);
                            }
                        }));
                        Source::Sf2 {
                            synth: Box::new(synth),
                            chans: channels.into_iter().map(Chan::new).collect(),
                            rate: rate.max(1000) as f32,
                            scratch: (vec![0.0; BLOCK], vec![0.0; BLOCK]),
                            dead: false,
                        }
                    }
                    _ => Source::Builtin(Builtin::new(Family::of_program(*program), rate)),
                }
            }
            Resolved::Sfz(inst) => Source::Sfz(Box::new(SfzPlayer::new(inst.clone(), rate))),
        }
    }

    fn note_on(&mut self, key: u8, vel: u8) {
        self.note_on_shaped(key, vel, None);
    }

    fn note_on_shaped(&mut self, key: u8, vel: u8, shape: Option<Shape>) {
        match self {
            Source::Builtin(b) => b.note_on_shaped(key, vel, shape),
            Source::Sf2 {
                synth, chans, dead, ..
            } => {
                if *dead {
                    return;
                }
                // A free channel (the one idle longest), else the oldest note's.
                let Some(i) = (0..chans.len()).max_by(|a, b| {
                    let (x, y) = (&chans[*a], &chans[*b]);
                    (x.key.is_none(), x.age)
                        .partial_cmp(&(y.key.is_none(), y.age))
                        .unwrap_or(std::cmp::Ordering::Equal)
                }) else {
                    return;
                };
                let Some(c) = chans.get_mut(i) else { return };
                if let Some(old) = c.key.take() {
                    let ch = c.channel;
                    let _ = catch_unwind(AssertUnwindSafe(|| synth.note_off(ch, i32::from(old))));
                }
                c.key = Some(key);
                c.shape = shape;
                c.age = 0.0;
                c.express(synth);
                let ch = c.channel;
                *dead = catch_unwind(AssertUnwindSafe(|| {
                    synth.note_on(ch, i32::from(key), i32::from(vel))
                }))
                .is_err();
            }
            Source::Sfz(p) => p.note_on_shaped(key, vel, shape),
        }
    }

    fn note_off(&mut self, key: u8) {
        match self {
            Source::Builtin(b) => b.note_off(key),
            Source::Sf2 {
                synth, chans, dead, ..
            } => {
                if *dead {
                    return;
                }
                if let Some(c) = chans.iter_mut().find(|c| c.key == Some(key)) {
                    c.key = None;
                    let ch = c.channel;
                    *dead = catch_unwind(AssertUnwindSafe(|| synth.note_off(ch, i32::from(key)))).is_err();
                }
            }
            Source::Sfz(p) => p.note_off(key),
        }
    }

    /// Adds the next `l.len()` frames into `l` and `r`.
    fn render(&mut self, l: &mut [f32], r: &mut [f32]) {
        match self {
            Source::Builtin(b) => b.render(l, r),
            Source::Sf2 {
                synth,
                scratch,
                dead,
                chans,
                rate,
            } => {
                if *dead {
                    return;
                }
                let n = l.len().min(scratch.0.len());
                // Each note's shape, as the expression of its channel.
                for c in chans.iter_mut() {
                    c.age += n as f32 / *rate;
                    if c.shape.is_some() {
                        c.express(synth);
                    }
                }
                let (sl, sr) = (&mut scratch.0[..n], &mut scratch.1[..n]);
                *dead = catch_unwind(AssertUnwindSafe(|| synth.render(sl, sr))).is_err();
                if !*dead {
                    for i in 0..n {
                        if let (Some(a), Some(b), Some(x), Some(y)) =
                            (l.get_mut(i), r.get_mut(i), sl.get(i), sr.get(i))
                        {
                            *a += x * SF2_GAIN;
                            *b += y * SF2_GAIN;
                        }
                    }
                }
            }
            Source::Sfz(p) => {
                let n = l.len();
                let mut a = vec![0.0f32; n];
                let mut b = vec![0.0f32; n];
                p.render(&mut a, &mut b);
                for i in 0..n {
                    if let (Some(lo), Some(ro), Some(x), Some(y)) =
                        (l.get_mut(i), r.get_mut(i), a.get(i), b.get(i))
                    {
                        *lo += x * SFZ_GAIN;
                        *ro += y * SFZ_GAIN;
                    }
                }
            }
        }
    }

    fn sampled(&self) -> bool {
        !matches!(self, Source::Builtin(_))
    }
}

/// A note-on (velocity > 0) or note-off at a time in seconds, for one of the track's sounds.
#[derive(Clone, Copy, Debug)]
struct TrackEvent {
    at: f64,
    key: u8,
    vel: u8,
    /// Index into the track's sounds (0 = the part's own).
    src: usize,
    /// How the note grows and lets go (note-ons only).
    shape: Option<Shape>,
}

/// A hairpin in seconds: the loudness goes from level `v0` to `v1`.
#[derive(Clone, Copy, Debug)]
struct Envelope {
    from: f64,
    to: f64,
    v0: f32,
    v1: f32,
}

impl Envelope {
    /// Gain relative to the level the swell starts from (sounds grow with velocity squared).
    fn gain(&self, t: f64) -> f32 {
        if t < self.from || t >= self.to {
            return 1.0;
        }
        let f = ((t - self.from) / (self.to - self.from).max(1e-6)) as f32;
        let v = self.v0 + (self.v1 - self.v0) * f;
        let r = curve(v) / curve(self.v0).max(1.0);
        r * r
    }
}

/// The velocity a sound gets for a written one: softer dynamics a little louder, so that
/// pp is quiet but still heard and ff not much louder than f (as players balance).
fn curve(v: f32) -> f32 {
    127.0 * (v.clamp(1.0, 127.0) / 127.0).powf(0.75)
}

struct Track {
    /// The part (= index into the mix controls).
    part: usize,
    events: Vec<TrackEvent>,
    next: usize,
    /// The part's own sound first, then instrument changes and technique variants; each with
    /// the gain that brings it to a common loudness.
    sources: Vec<(Source, f32)>,
    swells: Vec<Envelope>,
    l: Vec<f32>,
    r: Vec<f32>,
    scratch: (Vec<f32>, Vec<f32>),
}

impl Track {
    fn new(part: usize, mut events: Vec<TrackEvent>, sources: Vec<(Source, f32)>) -> Track {
        // Note-offs before note-ons at the same moment (repeated notes).
        events.sort_by(|a, b| a.at.total_cmp(&b.at).then((a.vel > 0).cmp(&(b.vel > 0))));
        Track {
            part,
            events,
            next: 0,
            sources,
            swells: Vec::new(),
            l: vec![0.0; BLOCK],
            r: vec![0.0; BLOCK],
            scratch: (vec![0.0; BLOCK], vec![0.0; BLOCK]),
        }
    }

    fn sampled(&self) -> bool {
        self.sources.iter().any(|(s, _)| s.sampled())
    }

    /// Renders `n` frames starting at frame `start` into the track buffers.
    fn render(&mut self, start: u64, n: usize, rate: f64) {
        self.l[..n].iter_mut().for_each(|v| *v = 0.0);
        self.r[..n].iter_mut().for_each(|v| *v = 0.0);
        let mut done = 0usize;
        while done < n {
            let now = (start + done as u64) as f64 / rate;
            while let Some(e) = self.events.get(self.next).copied() {
                if e.at > now {
                    break;
                }
                self.next += 1;
                if let Some((src, _)) = self.sources.get_mut(e.src) {
                    if e.vel > 0 {
                        src.note_on_shaped(e.key, e.vel, e.shape);
                    } else {
                        src.note_off(e.key);
                    }
                }
            }
            let until = self
                .events
                .get(self.next)
                .map(|e| ((e.at * rate).ceil() as u64).saturating_sub(start) as usize)
                .unwrap_or(n)
                .clamp(done + 1, n);
            for (src, gain) in &mut self.sources {
                let (sl, sr) = (
                    &mut self.scratch.0[..until - done],
                    &mut self.scratch.1[..until - done],
                );
                sl.iter_mut().for_each(|v| *v = 0.0);
                sr.iter_mut().for_each(|v| *v = 0.0);
                src.render(sl, sr);
                for i in 0..sl.len() {
                    if let (Some(a), Some(b), Some(x), Some(y)) = (
                        self.l.get_mut(done + i),
                        self.r.get_mut(done + i),
                        sl.get(i),
                        sr.get(i),
                    ) {
                        *a += x * *gain;
                        *b += y * *gain;
                    }
                }
            }
            done = until;
        }
        // Hairpins: the loudness moves also within held notes.
        if !self.swells.is_empty() {
            for i in 0..n {
                let t = (start + i as u64) as f64 / rate;
                let g: f32 = self.swells.iter().map(|s| s.gain(t)).product();
                if (g - 1.0).abs() > 1e-6 {
                    if let Some(v) = self.l.get_mut(i) {
                        *v *= g;
                    }
                    if let Some(v) = self.r.get_mut(i) {
                        *v *= g;
                    }
                }
            }
        }
    }
}

pub struct Synth {
    tracks: Vec<Track>,
    controls: Arc<MixControls>,
    rate: f32,
    /// Frames rendered so far.
    frames: u64,
    end: f64,
    pub volume: f32,
    /// The room (its amount follows the mix controls).
    reverb: reverb::Reverb,
}

/// A small, repeatable "random" number in -1…1 for a note (the same every time it plays).
fn jitter(a: u64, b: u64, c: u64) -> f64 {
    let mut x = a
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(b.wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add(c.wrapping_mul(0x94D0_49BB_1331_11EB));
    x ^= x >> 31;
    x = x.wrapping_mul(0xD6E8_FEB8_6659_FD93);
    x ^= x >> 32;
    (x % 2001) as f64 / 1000.0 - 1.0
}

/// How a part's notes map to its sounds: the sound index for a note, and whether a tremolo
/// is held (the sound has real tremolo) rather than played as repeated notes.
type Choose<'a> = &'a dyn Fn(&PlayNote) -> (usize, bool);

fn track_events(
    seq: &Sequence,
    from: u64,
    part: usize,
    choose: Choose,
    humanize: bool,
    optimize: bool,
) -> Vec<TrackEvent> {
    let t0 = seq.seconds_at(from);
    let mut out = Vec::new();
    let notes: Vec<&PlayNote> = seq
        .notes
        .iter()
        .filter(|n| n.start >= from && n.part == part)
        .collect();
    for (i, n) in notes.iter().enumerate() {
        let n = *n;
        // The next note of the line (soon after this one ends): which way the line goes.
        let step = notes
            .iter()
            .skip(i + 1)
            .find(|m| m.start > n.start)
            .filter(|m| m.start <= n.start + n.ticks + u64::from(scoreart_model::TPQ))
            .map_or(0, |m| i32::from(m.key) - i32::from(n.key));
        let family = scoreart_midi::Family::of_program(n.program);
        let (src, hold) = choose(n);
        let strokes = if hold { vec![*n] } else { n.strokes() };
        for s in strokes {
            let mut start = seq.seconds_at(s.start) - t0;
            let mut end = (seq.seconds_at(s.start + s.ticks) - t0).max(start + 0.03);
            let mut vel = f64::from(s.velocity);
            if humanize && !s.tech.ornament {
                // A shared nudge for everything starting together, a little more per note.
                let shift = jitter(part as u64, s.start, 0) * 0.007
                    + jitter(part as u64, s.start, u64::from(s.key)) * 0.003;
                start = (start + shift).max(0.0);
                end = (end + shift).max(start + 0.03);
                vel += jitter(part as u64, s.start, u64::from(s.key) + 128) * 3.0;
            }
            let vel = if optimize { curve(vel as f32) } else { vel as f32 };
            let vel = vel.round().clamp(1.0, 127.0) as u8;
            let key = s.key.min(127);
            let shape = if optimize {
                Shape::for_note(family, s.tech, vel, (end - start) as f32, step)
            } else {
                None
            };
            out.push(TrackEvent {
                at: start,
                key,
                vel,
                src,
                shape,
            });
            out.push(TrackEvent {
                at: end,
                key,
                vel: 0,
                src,
                shape: None,
            });
        }
    }
    out
}

/// The hairpins of a part, in seconds from `from`.
fn envelopes(seq: &Sequence, from: u64, part: usize) -> Vec<Envelope> {
    let t0 = seq.seconds_at(from);
    seq.swells
        .iter()
        .filter(|s| s.part == part && s.to > from && s.v0 != s.v1)
        .map(|s| Envelope {
            from: seq.seconds_at(s.from) - t0,
            to: seq.seconds_at(s.to) - t0,
            v0: f32::from(s.v0),
            v1: f32::from(s.v1),
        })
        .collect()
}

/// How loud a sound plays a mezzo-forte note: the loudest 50 ms (RMS) of its first 0.8 s, so
/// that a slow bow stroke and a quickly dying pluck are both measured at their full sound.
fn loudness(r: &Resolved, key: u8, rate: u32) -> f32 {
    let mut s = Source::new(r, rate);
    s.note_on(key, curve(80.0) as u8);
    let window = (rate as usize / 20).max(BLOCK);
    let frames = rate as usize * 8 / 10;
    let (mut l, mut rr) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
    let (mut best, mut sum, mut count) = (0.0f64, 0.0f64, 0usize);
    let mut done = 0usize;
    while done < frames {
        l.iter_mut().for_each(|v| *v = 0.0);
        rr.iter_mut().for_each(|v| *v = 0.0);
        s.render(&mut l, &mut rr);
        sum += l
            .iter()
            .chain(rr.iter())
            .map(|v| f64::from(*v) * f64::from(*v))
            .sum::<f64>();
        count += 2 * BLOCK;
        if count >= 2 * window {
            best = best.max(sum / count as f64);
            sum = 0.0;
            count = 0;
        }
        done += BLOCK;
    }
    best.sqrt() as f32
}

/// The gain that makes a sampled sound as loud as the built-in sound of its family; built-in
/// sounds are left as they are. A sound that is silent at `key` (out of its range) keeps 1.
fn balance(r: &Resolved, program: u8, keys: [u8; 3], rate: u32) -> f32 {
    // The average (in decibels) over the keys, so one odd sample does not decide.
    let db: f32 = keys
        .iter()
        .map(|k| balance_at(r, program, *k, rate).log10())
        .sum::<f32>()
        / 3.0;
    10f32.powf(db)
}

fn balance_at(r: &Resolved, program: u8, key: u8, rate: u32) -> f32 {
    // Measured once per sound, key and rate: starting playback stays quick.
    type Cache = std::sync::Mutex<std::collections::HashMap<(usize, u16, u8, u8, u8, u32), f32>>;
    static CACHE: std::sync::OnceLock<Cache> = std::sync::OnceLock::new();
    let id = match r {
        Resolved::Builtin(_) => return 1.0,
        Resolved::Sf2 { font, bank, program } => (Arc::as_ptr(font) as usize, *bank, *program),
        // The name too, so a later instrument at the same address is not mistaken for it.
        Resolved::Sfz(inst) => (
            Arc::as_ptr(inst) as usize
                ^ inst
                    .name
                    .bytes()
                    .fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(usize::from(b))),
            0,
            0,
        ),
    };
    let k = (id.0, id.1, id.2, program, key, rate);
    let cache = CACHE.get_or_init(Default::default);
    if let Some(g) = cache.lock().ok().and_then(|c| c.get(&k).copied()) {
        return g;
    }
    let g = measure_balance(r, program, key, rate);
    if let Ok(mut c) = cache.lock() {
        if c.len() > 4096 {
            c.clear();
        }
        c.insert(k, g);
    }
    g
}

fn measure_balance(r: &Resolved, program: u8, key: u8, rate: u32) -> f32 {
    let target = loudness(&Resolved::Builtin(Family::of_program(program)), key, rate);
    let have = loudness(r, key, rate);
    if have < 1e-5 || target < 1e-6 {
        return 1.0;
    }
    (target / have).clamp(1.0 / 8.0, 4.0)
}

/// Three keys across a part's range (lower quarter, middle, upper quarter), to measure its
/// sounds where it plays.
fn range_keys(seq: &Sequence, part: usize) -> [u8; 3] {
    let mut keys: Vec<u8> = seq
        .notes
        .iter()
        .filter(|n| n.part == part)
        .map(|n| n.key)
        .collect();
    keys.sort_unstable();
    let at = |f: usize| keys.get(keys.len() * f / 4).copied().unwrap_or(60);
    [at(1), at(2), at(3)]
}

impl Synth {
    fn assemble(tracks: Vec<Track>, controls: Arc<MixControls>, rate: u32, music_end: f64) -> Synth {
        let reverb = controls.reverb();
        let tail: f64 = if tracks.iter().any(Track::sampled) {
            2.5
        } else {
            0.8
        };
        let tail = if reverb > 0.0 { tail.max(2.0) } else { tail };
        let last = tracks
            .iter()
            .flat_map(|t| t.events.iter().map(|e| e.at))
            .fold(0.0, f64::max);
        let rate = rate.clamp(8000, 192_000) as f32;
        Synth {
            tracks,
            controls,
            rate,
            frames: 0,
            end: music_end.max(last) + tail,
            volume: 1.0,
            reverb: reverb::Reverb::new(rate, reverb),
        }
    }

    /// Plays `seq` from score tick `from` (repeats unrolled) with the built-in sounds, at
    /// `rate` Hz; parts are spread across the stereo field.
    pub fn new(seq: &Sequence, from: u64, rate: u32) -> Synth {
        let parts = seq.notes.iter().map(|n| n.part).max().map(|m| m + 1).unwrap_or(1);
        let controls = MixControls::uniform(parts);
        let tracks = (0..parts)
            .map(|p| {
                let program = seq.notes.iter().find(|n| n.part == p).map_or(0, |n| n.program);
                if parts > 1 {
                    controls.set_pan(p, (p as f32 / (parts - 1) as f32 - 0.5) * 0.7);
                }
                let mut programs: Vec<u8> = vec![program];
                for n in seq.notes.iter().filter(|n| n.part == p) {
                    if !programs.contains(&n.program) && programs.len() < 17 {
                        programs.push(n.program);
                    }
                }
                let choose =
                    |n: &PlayNote| (programs.iter().position(|q| *q == n.program).unwrap_or(0), false);
                let sources = programs
                    .iter()
                    .map(|q| (Source::Builtin(Builtin::new(Family::of_program(*q), rate)), 1.0))
                    .collect();
                let mut t = Track::new(p, track_events(seq, from, p, &choose, false, false), sources);
                t.swells = envelopes(seq, from, p);
                t
            })
            .collect();
        let end = seq.seconds_at(seq.end) - seq.seconds_at(from);
        Synth::assemble(tracks, Arc::new(controls), rate, end)
    }

    /// Plays `seq` (made from `score`) with each part's chosen sound from the loaded `banks`.
    /// Pass the UI's `controls` to change the mix while playing (they must have one entry per
    /// part); otherwise the score's volumes and pans are used. Also returns problems found
    /// (a library not loaded …); those parts fall back to automatic sounds.
    ///
    /// Notes marked pizzicato, tremolo, muted or short play the library's matching sound when
    /// it has one; every sound is brought to a common loudness; the score's playback style
    /// adds the room and a human touch.
    pub fn for_score(
        score: &Score,
        seq: &Sequence,
        from: u64,
        rate: u32,
        banks: &[Arc<SoundBank>],
        controls: Option<Arc<MixControls>>,
    ) -> (Synth, Vec<String>) {
        let controls = controls
            .filter(|c| c.len() == score.parts.len())
            .unwrap_or_else(|| Arc::new(MixControls::from_score(score)));
        let style = score.playback;
        let mut warnings = Vec::new();
        let tracks = score
            .parts
            .iter()
            .enumerate()
            .map(|(pi, p)| {
                let (own, warning) = resolve(banks, &p.sound, p.program);
                if let Some(w) = warning {
                    warnings.push(w);
                }
                let keys = range_keys(seq, pi);
                // Which sound each kind of note plays: (instrument change, variant) → index.
                #[derive(Clone, Copy, PartialEq, Eq, Hash)]
                enum Want {
                    Program(u8),
                    Variant(Variant),
                }
                let mut wants: Vec<(Want, usize)> = Vec::new();
                let mut sounds: Vec<Resolved> = vec![own.clone()];
                // Instruments changed to inside the part sound automatically (or built-in when
                // the part is set to the built-in sound).
                let fallback = if p.sound.library == scoreart_model::PartSound::BUILT_IN {
                    p.sound.clone()
                } else {
                    scoreart_model::PartSound::default()
                };
                for n in seq.notes.iter().filter(|n| n.part == pi) {
                    let want = if n.program != p.program {
                        Want::Program(n.program)
                    } else {
                        match Variant::of(n.tech) {
                            // Without sound optimisation the part's own sound plays everything.
                            Variant::Own => continue,
                            _ if !style.optimize => continue,
                            v => Want::Variant(v),
                        }
                    };
                    if wants.iter().any(|(w, _)| *w == want) || sounds.len() >= 24 {
                        continue;
                    }
                    let r = match want {
                        Want::Program(q) => Some(resolve(banks, &fallback, q).0),
                        Want::Variant(v) => bank::resolve_variant(banks, &p.sound, p.program, v),
                    };
                    match r {
                        Some(r) => {
                            sounds.push(r);
                            wants.push((want, sounds.len() - 1));
                        }
                        // No such sound: the part's own plays it.
                        None => wants.push((want, 0)),
                    }
                }
                let own_program = p.program;
                let choose = |n: &PlayNote| -> (usize, bool) {
                    let want = if n.program != own_program {
                        Want::Program(n.program)
                    } else {
                        Want::Variant(Variant::of(n.tech))
                    };
                    let i = wants.iter().find(|(w, _)| *w == want).map_or(0, |(_, i)| *i);
                    let held = i > 0 && want == Want::Variant(Variant::Tremolo);
                    (i, held)
                };
                let sources: Vec<(Source, f32)> = sounds
                    .iter()
                    .map(|r| {
                        let g = if style.optimize {
                            balance(r, p.program, keys, rate)
                        } else {
                            1.0
                        };
                        (Source::new(r, rate), g)
                    })
                    .collect();
                let mut t = Track::new(
                    pi,
                    track_events(seq, from, pi, &choose, style.humanize, style.optimize),
                    sources,
                );
                t.swells = envelopes(seq, from, pi);
                t
            })
            .collect();
        let end = seq.seconds_at(seq.end) - seq.seconds_at(from);
        (Synth::assemble(tracks, controls, rate, end), warnings)
    }

    /// A short audition of a chord (note input feedback) with the built-in sound.
    pub fn preview(keys: &[u8], program: u8, rate: u32) -> Synth {
        Synth::preview_sound(keys, &Resolved::Builtin(Family::of_program(program)), rate)
    }

    /// A short audition of a chord with any sound.
    pub fn preview_sound(keys: &[u8], sound: &Resolved, rate: u32) -> Synth {
        let events = keys
            .iter()
            .flat_map(|k| {
                [
                    TrackEvent {
                        at: 0.0,
                        key: (*k).min(127),
                        vel: 80,
                        src: 0,
                        shape: None,
                    },
                    TrackEvent {
                        at: 0.45,
                        key: (*k).min(127),
                        vel: 0,
                        src: 0,
                        shape: None,
                    },
                ]
            })
            .collect();
        let controls = Arc::new(MixControls::uniform(1));
        let mut s = Synth::assemble(
            vec![Track::new(0, events, vec![(Source::new(sound, rate), 1.0)])],
            controls,
            rate,
            0.45,
        );
        s.volume = 0.8;
        s.end = s.end.min(1.5);
        s
    }

    pub fn rate(&self) -> u32 {
        self.rate as u32
    }

    /// The live mixer settings.
    pub fn controls(&self) -> Arc<MixControls> {
        self.controls.clone()
    }

    /// Seconds rendered so far.
    pub fn position(&self) -> f64 {
        self.frames as f64 / f64::from(self.rate)
    }

    /// Total length in seconds (including the release tail).
    pub fn duration(&self) -> f64 {
        self.end
    }

    pub fn finished(&self) -> bool {
        self.position() >= self.end
    }

    /// Renders interleaved stereo frames into `out` (its length should be even). Silence after
    /// the end.
    pub fn render(&mut self, out: &mut [f32]) {
        self.mix(out, true);
    }

    /// Like [`Synth::render`] but without the limiter: the plain sum, which may exceed ±1. For
    /// files, where the whole piece can be scaled afterwards (see [`normalize`]).
    pub fn render_linear(&mut self, out: &mut [f32]) {
        self.mix(out, false);
    }

    fn mix(&mut self, out: &mut [f32], limit: bool) {
        let rate = f64::from(self.rate);
        let g = self.volume * MASTER;
        for chunk in out.chunks_mut(BLOCK * 2) {
            let n = chunk.len() / 2;
            chunk.iter_mut().for_each(|v| *v = 0.0);
            for t in &mut self.tracks {
                t.render(self.frames, n, rate);
                if !self.controls.audible(t.part) {
                    continue;
                }
                let (gl, gr) = self.controls.gains(t.part);
                let mut peak = 0.0f32;
                for i in 0..n {
                    let (a, b) = (t.l[i] * gl, t.r[i] * gr);
                    peak = peak.max(a.abs()).max(b.abs());
                    if let Some(x) = chunk.get_mut(2 * i) {
                        *x += a;
                    }
                    if let Some(y) = chunk.get_mut(2 * i + 1) {
                        *y += b;
                    }
                }
                self.controls.add_peak(t.part, peak * g);
            }
            self.reverb.mix = self.controls.reverb();
            self.reverb.process(chunk);
            // Gentle limiter (live), or just the master gain (files).
            for v in chunk.iter_mut() {
                *v = if limit { (*v * g).tanh() } else { *v * g };
            }
            if chunk.len() % 2 == 1
                && let Some(last) = chunk.last_mut()
            {
                *last = 0.0;
            }
            self.frames += n as u64;
        }
    }

    /// Renders the whole piece once more, one interleaved stereo buffer per part ("stems"), each
    /// with its own volume and pan but ignoring mute and solo, without a limiter (see
    /// [`normalize`]). Capped at `max_secs`.
    pub fn render_stems(&mut self, max_secs: f64) -> Vec<(usize, Vec<f32>)> {
        let rate = f64::from(self.rate);
        let total = (self.end.min(max_secs).max(0.0) * rate) as u64;
        let g = self.volume * MASTER;
        let mut out: Vec<(usize, Vec<f32>)> = self
            .tracks
            .iter()
            .map(|t| (t.part, Vec::with_capacity(total as usize * 2)))
            .collect();
        // Each stem its own room: the room is linear, so the stems still add up to the mix.
        let mut rooms: Vec<reverb::Reverb> = self
            .tracks
            .iter()
            .map(|_| reverb::Reverb::new(self.rate, self.controls.reverb()))
            .collect();
        while self.frames < total {
            let n = ((total - self.frames) as usize).min(BLOCK);
            for (out_i, (t, (_, buf))) in self.tracks.iter_mut().zip(out.iter_mut()).enumerate() {
                t.render(self.frames, n, rate);
                let (gl, gr) = self.controls.gains(t.part);
                let at = buf.len();
                for i in 0..n {
                    buf.push(t.l[i] * gl);
                    buf.push(t.r[i] * gr);
                }
                if let (Some(room), Some(chunk)) = (rooms.get_mut(out_i), buf.get_mut(at..)) {
                    room.process(chunk);
                    chunk.iter_mut().for_each(|v| *v *= g);
                }
            }
            self.frames += n as u64;
        }
        out
    }
}

/// Highest peak of a rendered file: about -1 dBFS.
const FILE_PEAK: f32 = 0.89;

/// The factor that brings a whole rendered piece under [`FILE_PEAK`]: loud pieces are turned
/// down as a whole, so loud and soft passages keep their distance instead of being squashed by
/// a limiter; quiet pieces are left as they are. `peak` is the largest sample of the mix.
pub fn normalize(peak: f32) -> f32 {
    if peak.is_finite() && peak > FILE_PEAK {
        FILE_PEAK / peak
    } else {
        1.0
    }
}

/// The largest absolute sample.
pub fn peak(pcm: &[f32]) -> f32 {
    pcm.iter()
        .fold(0.0f32, |m, v| if v.is_finite() { m.max(v.abs()) } else { m })
}

#[cfg(test)]
mod tests {
    use super::*;
    use scoreart_model::edit::Pos;
    use scoreart_model::{
        Duration, EventKind, KeySig, Note, NoteValue, Pitch, Score, Step, TPQ, TimeSig, instrument,
    };

    #[test]
    fn files_are_turned_down_only_when_too_loud() {
        assert_eq!(normalize(0.5), 1.0);
        assert!((normalize(4.0) * 4.0 - FILE_PEAK).abs() < 1e-6);
        assert_eq!(normalize(f32::NAN), 1.0);
        assert_eq!(peak(&[0.1, -0.7, f32::INFINITY, 0.3]), 0.7);
    }

    #[test]
    fn renders_audio_and_ends() {
        let piano = instrument("piano").unwrap();
        let mut s = Score::new("T", &[piano], TimeSig::default(), KeySig::default(), 1);
        let kind = EventKind::Chord(vec![Note::new(Pitch::new(Step::A, 0, 4))]);
        s.overwrite(Pos::default(), kind, Duration::new(NoteValue::Quarter, 0))
            .unwrap();
        let seq = scoreart_midi::sequence(&s);
        let mut syn = Synth::new(&seq, 0, 44_100);
        let mut buf = vec![0.0f32; 2 * 4410];
        syn.render(&mut buf);
        let peak = buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.05 && peak <= 1.0, "peak {peak}");
        assert!(buf.iter().all(|v| v.is_finite()));
        let mut rest = vec![0.0f32; 2 * 44_100 * 4];
        syn.render(&mut rest);
        assert!(syn.finished());
    }

    #[test]
    fn every_family_sounds() {
        for p in [0u8, 19, 24, 40, 52, 57, 68, 73] {
            let mut syn = Synth::preview(&[60, 64, 67], p, 22_050);
            let mut buf = vec![0.0f32; 2 * 2205];
            syn.render(&mut buf);
            assert!(buf.iter().any(|v| v.abs() > 0.01), "program {p}");
            assert!(buf.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
        }
    }

    fn two_parts() -> Score {
        let mut s = Score::new(
            "T",
            &[instrument("violin").unwrap(), instrument("flute").unwrap()],
            TimeSig::default(),
            KeySig::default(),
            1,
        );
        for part in 0..2 {
            let kind = EventKind::Chord(vec![Note::new(Pitch::new(Step::A, 0, 4))]);
            let pos = Pos {
                part,
                ..Pos::default()
            };
            s.overwrite(pos, kind, Duration::new(NoteValue::Half, 0)).unwrap();
        }
        s
    }

    /// A zip with one SFZ instrument and its sample, written in the test.
    fn sfz_zip() -> Vec<u8> {
        use std::io::Write;
        let tone: Vec<i16> = (0..8000)
            .map(|i| ((i as f32 * 0.06).sin() * 15000.0) as i16)
            .collect();
        let wav = crate::wav::tests::wav16(&tone, 1, 44_100, Some((1000, 7000)));
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("Lib/Strings/test.sfz", o).unwrap();
            z.write_all(b"<group> ampeg_release=0.2\n<region> sample=../samples/a.wav pitch_keycenter=69")
                .unwrap();
            z.start_file("Lib/samples/a.wav", o).unwrap();
            z.write_all(&wav).unwrap();
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn one_track_per_part_with_its_own_sound() {
        let mut s = two_parts();
        let bank = SoundBank::from_bytes("Lib.zip", sfz_zip()).unwrap();
        assert_eq!(bank.name, "Lib");
        assert_eq!(bank.presets()[0].name, "Lib/Strings/test");
        s.parts[0].sound.library = "Lib".into();
        s.parts[1].sound.pan = 100;
        let seq = scoreart_midi::sequence(&s);
        let banks = vec![Arc::new(bank)];
        let (mut syn, warnings) = Synth::for_score(&s, &seq, 0, 44_100, &banks, None);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(matches!(syn.tracks[0].sources[0].0, Source::Sfz(_)));
        assert!(matches!(syn.tracks[1].sources[0].0, Source::Builtin(_)));
        let stems = syn.render_stems(10.0);
        assert_eq!(stems.len(), 2);
        let energy = |b: &[f32], ch: usize| b.iter().skip(ch).step_by(2).map(|v| v * v).sum::<f32>();
        assert!(energy(&stems[0].1, 0) > 1.0, "the sampled violin sounds");
        // The flute is panned hard right.
        assert!(energy(&stems[1].1, 1) > 100.0 * energy(&stems[1].1, 0).max(1e-9));
        assert!(
            stems
                .iter()
                .all(|(_, b)| b.iter().all(|v| v.is_finite() && v.abs() <= 1.0))
        );
    }

    #[test]
    fn mute_and_solo() {
        let s = two_parts();
        let seq = scoreart_midi::sequence(&s);
        let controls = Arc::new(MixControls::from_score(&s));
        let (mut syn, _) = Synth::for_score(&s, &seq, 0, 22_050, &[], Some(controls.clone()));
        let mut buf = vec![0.0f32; 2 * 2048];
        controls.set_mute(0, true);
        controls.set_mute(1, true);
        syn.render(&mut buf);
        assert!(buf.iter().all(|v| *v == 0.0));
        controls.set_mute(1, false);
        controls.set_solo(0, true);
        assert!(!controls.audible(1), "solo silences the others");
        controls.set_mute(0, false);
        syn.render(&mut buf);
        assert!(buf.iter().any(|v| v.abs() > 0.01));
        assert!(controls.take_peak(0) > 0.0);
        assert_eq!(controls.take_peak(1), 0.0);
    }

    #[test]
    fn missing_library_falls_back() {
        let mut s = two_parts();
        s.parts[0].sound.library = "Nowhere".into();
        let seq = scoreart_midi::sequence(&s);
        let (syn, warnings) = Synth::for_score(&s, &seq, 0, 44_100, &[], None);
        assert_eq!(warnings.len(), 1);
        assert!(matches!(syn.tracks[0].sources[0].0, Source::Builtin(_)));
    }

    #[test]
    fn hostile_libraries_are_rejected() {
        for junk in [
            &b""[..],
            b"RIFF\0\0\0\0sfbk",
            b"RIFF\xff\xff\xff\x7fsfbkLIST\xff\xff\xff\xff",
            b"PK\x03\x04garbage",
        ] {
            assert!(SoundBank::from_bytes("x.sf2", junk.to_vec()).is_err());
        }
    }

    #[test]
    fn start_mid_score() {
        let s = scoreart_model::Score::new(
            "T",
            &[instrument("flute").unwrap()],
            TimeSig::default(),
            KeySig::default(),
            4,
        );
        let seq = scoreart_midi::sequence(&s);
        let syn = Synth::new(&seq, u64::from(TPQ) * 8, 48_000);
        assert!(syn.duration() > 0.0);
    }

    /// A library with a sustained and a pizzicato violin.
    fn variants_zip() -> Vec<u8> {
        use std::io::Write;
        // Same peak; the pizzicato dies away at once, so it sounds much quieter.
        let tone = |decay: f32| -> Vec<u8> {
            let t: Vec<i16> = (0..8000)
                .map(|i| ((i as f32 * 0.06).sin() * 16000.0 * (-(i as f32) / decay).exp()) as i16)
                .collect();
            crate::wav::tests::wav16(&t, 1, 44_100, None)
        };
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            for (name, wav) in [("ViolinSus", "loud"), ("ViolinPizz", "soft")] {
                z.start_file(format!("Lib/{name}.sfz"), o).unwrap();
                z.write_all(format!("<region> sample={wav}.wav pitch_keycenter=69").as_bytes())
                    .unwrap();
            }
            z.start_file("Lib/loud.wav", o).unwrap();
            z.write_all(&tone(1e9)).unwrap();
            z.start_file("Lib/soft.wav", o).unwrap();
            z.write_all(&tone(600.0)).unwrap();
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn pizzicato_plays_the_pizzicato_sound_at_the_same_loudness() {
        let mut s = two_parts();
        s.parts[0].sound.library = "Lib".into();
        s.parts[0].sound.preset = "Lib/ViolinSus".into();
        s.playback.humanize = false;
        // A second note, marked pizz.
        let kind = EventKind::Chord(vec![Note::new(Pitch::new(Step::A, 0, 4))]);
        s.overwrite(
            Pos {
                tick: TPQ * 2,
                ..Pos::default()
            },
            kind,
            Duration::new(NoteValue::Half, 0),
        )
        .unwrap();
        let r = scoreart_model::edit::EventRef {
            part: 0,
            staff: 0,
            voice: 0,
            measure: 0,
            index: 1,
        };
        s.event_mut(r).unwrap().text = Some("pizz.".into());
        let bank = SoundBank::from_bytes("Lib.zip", variants_zip()).unwrap();
        let seq = scoreart_midi::sequence(&s);
        let (syn, _) = Synth::for_score(&s, &seq, 0, 44_100, &[Arc::new(bank)], None);
        let t = &syn.tracks[0];
        assert_eq!(t.sources.len(), 2, "the own sound and the pizzicato");
        let ons: Vec<usize> = t.events.iter().filter(|e| e.vel > 0).map(|e| e.src).collect();
        assert_eq!(ons, [0, 1]);
        // The quieter pizzicato samples are brought up to the same loudness.
        let (g0, g1) = (t.sources[0].1, t.sources[1].1);
        assert!(g1 > g0 * 1.5, "{g0} {g1}");
    }

    #[test]
    fn a_hairpin_swells_a_held_note() {
        let flute = instrument("flute").unwrap();
        let mut s = Score::new("T", &[flute], TimeSig::default(), KeySig::default(), 1);
        let kind = EventKind::Chord(vec![Note::new(Pitch::new(Step::A, 0, 4))]);
        s.overwrite(Pos::default(), kind, Duration::new(NoteValue::Whole, 0))
            .unwrap();
        s.spanners.push(scoreart_model::Spanner {
            kind: scoreart_model::SpannerKind::Crescendo,
            part: 0,
            staff: 0,
            voice: 0,
            start: scoreart_model::Anchor { measure: 0, tick: 0 },
            end: scoreart_model::Anchor {
                measure: 0,
                tick: TPQ * 3,
            },
            dy: 0,
        });
        s.playback.reverb = 0;
        let seq = scoreart_midi::sequence(&s);
        assert_eq!(seq.notes.len(), 1);
        let (mut syn, _) = Synth::for_score(&s, &seq, 0, 44_100, &[], None);
        let mut buf = vec![0.0f32; 2 * 44_100 * 2];
        syn.render_linear(&mut buf);
        let rms = |a: usize, b: usize| {
            (buf[a * 2..b * 2].iter().map(|v| v * v).sum::<f32>() / (b - a) as f32).sqrt()
        };
        // 100 bpm: the swell runs from 0 to 1.8 s.
        assert!(
            rms(66_150, 75_000) > 1.3 * rms(8_000, 17_000),
            "louder towards the end"
        );
    }

    #[test]
    fn humanizing_is_small_and_repeatable() {
        let mut s = two_parts();
        s.playback.humanize = true;
        let seq = scoreart_midi::sequence(&s);
        let a = Synth::for_score(&s, &seq, 0, 44_100, &[], None).0;
        let b = Synth::for_score(&s, &seq, 0, 44_100, &[], None).0;
        let at = |x: &Synth| x.tracks[0].events.iter().map(|e| e.at).collect::<Vec<_>>();
        assert_eq!(at(&a), at(&b), "the same every time");
        assert!(a.tracks[0].events.iter().all(|e| e.at < 0.011 || e.vel == 0));
    }

    #[test]
    fn a_sampled_note_grows_and_lets_go() {
        // One half note on a sustained (looped) violin sample: no reverb, no human touch.
        let mut s = two_parts();
        s.parts[0].sound.library = "Lib".into();
        s.parts[1].sound.volume = 0;
        s.playback.reverb = 0;
        s.playback.humanize = false;
        let bank = SoundBank::from_bytes("Lib.zip", sfz_zip()).unwrap();
        let seq = scoreart_midi::sequence(&s);
        let (mut syn, _) = Synth::for_score(&s, &seq, 0, 44_100, &[Arc::new(bank)], None);
        let mut buf = vec![0.0f32; 2 * 44_100 * 2];
        syn.render_linear(&mut buf);
        let rms = |a: f32, b: f32| {
            let (a, b) = ((a * 44_100.0) as usize, (b * 44_100.0) as usize);
            (buf[a * 2..b * 2].iter().map(|v| v * v).sum::<f32>() / (b - a) as f32).sqrt()
        };
        // 100 bpm: the half note lasts 1.2 s and sounds about 1.1 s.
        let (start, middle, end) = (rms(0.0, 0.02), rms(0.3, 0.5), rms(1.0, 1.07));
        assert!(start < 0.8 * middle, "a soft start ({start} / {middle})");
        assert!(end < 0.85 * middle, "lets go at the end ({end} / {middle})");
    }

    #[test]
    fn sound_optimisation_can_be_turned_off() {
        let mut s = two_parts();
        s.parts[0].sound.library = "Lib".into();
        s.parts[0].sound.preset = "Lib/ViolinSus".into();
        let kind = EventKind::Chord(vec![Note::new(Pitch::new(Step::A, 0, 4))]);
        s.overwrite(
            Pos {
                tick: TPQ * 2,
                ..Pos::default()
            },
            kind,
            Duration::new(NoteValue::Half, 0),
        )
        .unwrap();
        let r = scoreart_model::edit::EventRef {
            part: 0,
            staff: 0,
            voice: 0,
            measure: 0,
            index: 1,
        };
        s.event_mut(r).unwrap().text = Some("pizz.".into());
        s.playback.optimize = false;
        let bank = Arc::new(SoundBank::from_bytes("Lib.zip", variants_zip()).unwrap());
        let seq = scoreart_midi::sequence(&s);
        let (syn, _) = Synth::for_score(&s, &seq, 0, 44_100, std::slice::from_ref(&bank), None);
        let t = &syn.tracks[0];
        assert_eq!(t.sources.len(), 1, "no pizzicato sample");
        assert_eq!(t.sources[0].1, 1.0, "no balancing");
        assert!(t.events.iter().all(|e| e.shape.is_none()), "no shaping");
        // On (the default): all of it.
        s.playback.optimize = true;
        let (syn, _) = Synth::for_score(&s, &seq, 0, 44_100, &[bank], None);
        assert_eq!(syn.tracks[0].sources.len(), 2);
        assert!(syn.tracks[0].events.iter().any(|e| e.shape.is_some()));
    }
}
