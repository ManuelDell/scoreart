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
pub mod sfz;
pub mod wav;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub use bank::{PresetInfo, Resolved, SoundBank, resolve};
use builtin::Builtin;
pub use builtin::Family;
use scoreart_midi::Sequence;
use scoreart_model::Score;
use sfz::SfzPlayer;

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
        }
    }

    fn uniform(n: usize) -> MixControls {
        MixControls {
            tracks: (0..n).map(|_| TrackControl::new(0.8, 0.0)).collect(),
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

enum Source {
    Builtin(Builtin),
    Sf2 {
        synth: Box<rustysynth::Synthesizer>,
        channel: i32,
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
                        let channel = if *bank == 128 { 9 } else { 0 };
                        let _ = catch_unwind(AssertUnwindSafe(|| {
                            synth.process_midi_message(channel, 0xB0, 0, i32::from(*bank).min(127));
                            synth.process_midi_message(channel, 0xC0, i32::from(*program), 0);
                        }));
                        Source::Sf2 {
                            synth: Box::new(synth),
                            channel,
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
        match self {
            Source::Builtin(b) => b.note_on(key, vel),
            Source::Sf2 {
                synth, channel, dead, ..
            } => {
                if !*dead {
                    let ch = *channel;
                    *dead = catch_unwind(AssertUnwindSafe(|| {
                        synth.note_on(ch, i32::from(key), i32::from(vel))
                    }))
                    .is_err();
                }
            }
            Source::Sfz(p) => p.note_on(key, vel),
        }
    }

    fn note_off(&mut self, key: u8) {
        match self {
            Source::Builtin(b) => b.note_off(key),
            Source::Sf2 {
                synth, channel, dead, ..
            } => {
                if !*dead {
                    let ch = *channel;
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
                synth, scratch, dead, ..
            } => {
                if *dead {
                    return;
                }
                let n = l.len().min(scratch.0.len());
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

/// A note-on (velocity > 0) or note-off at a time in seconds.
#[derive(Clone, Copy, Debug)]
struct TrackEvent {
    at: f64,
    key: u8,
    vel: u8,
    /// After an instrument change: the program that plays it (None = the part's own sound).
    program: Option<u8>,
}

struct Track {
    /// The part (= index into the mix controls).
    part: usize,
    events: Vec<TrackEvent>,
    next: usize,
    source: Source,
    /// Sounds of instruments changed to inside the part, by MIDI program.
    extra: Vec<(u8, Source)>,
    l: Vec<f32>,
    r: Vec<f32>,
}

impl Track {
    fn new(part: usize, mut events: Vec<TrackEvent>, source: Source) -> Track {
        // Note-offs before note-ons at the same moment (repeated notes).
        events.sort_by(|a, b| a.at.total_cmp(&b.at).then((a.vel > 0).cmp(&(b.vel > 0))));
        Track {
            part,
            events,
            next: 0,
            source,
            extra: Vec::new(),
            l: vec![0.0; BLOCK],
            r: vec![0.0; BLOCK],
        }
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
                let src = match e
                    .program
                    .and_then(|p| self.extra.iter().position(|(q, _)| *q == p))
                {
                    Some(i) => match self.extra.get_mut(i) {
                        Some((_, s)) => s,
                        None => &mut self.source,
                    },
                    None => &mut self.source,
                };
                if e.vel > 0 {
                    src.note_on(e.key, e.vel);
                } else {
                    src.note_off(e.key);
                }
            }
            let until = self
                .events
                .get(self.next)
                .map(|e| ((e.at * rate).ceil() as u64).saturating_sub(start) as usize)
                .unwrap_or(n)
                .clamp(done + 1, n);
            let (l, r) = (&mut self.l[done..until], &mut self.r[done..until]);
            self.source.render(l, r);
            for (_, s) in &mut self.extra {
                s.render(l, r);
            }
            done = until;
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
}

fn track_events(seq: &Sequence, from: u64, part: Option<usize>, own: u8) -> Vec<TrackEvent> {
    let t0 = seq.seconds_at(from);
    let mut out = Vec::new();
    for n in seq
        .notes
        .iter()
        .filter(|n| n.start >= from && part.is_none_or(|p| n.part == p))
    {
        let start = seq.seconds_at(n.start) - t0;
        let end = (seq.seconds_at(n.start + n.ticks) - t0).max(start + 0.03);
        let program = (n.program != own).then_some(n.program);
        out.push(TrackEvent {
            at: start,
            key: n.key.min(127),
            vel: n.velocity.clamp(1, 127),
            program,
        });
        out.push(TrackEvent {
            at: end,
            key: n.key.min(127),
            vel: 0,
            program,
        });
    }
    out
}

/// The programs of instrument changes in a track's events, in order of first use.
fn changed_programs(events: &[TrackEvent]) -> Vec<u8> {
    let mut v: Vec<u8> = Vec::new();
    for p in events.iter().filter_map(|e| e.program) {
        if !v.contains(&p) && v.len() < 16 {
            v.push(p);
        }
    }
    v
}

impl Synth {
    fn assemble(tracks: Vec<Track>, controls: Arc<MixControls>, rate: u32, music_end: f64) -> Synth {
        let tail = if tracks
            .iter()
            .any(|t| t.source.sampled() || t.extra.iter().any(|(_, s)| s.sampled()))
        {
            2.5
        } else {
            0.8
        };
        let last = tracks
            .iter()
            .flat_map(|t| t.events.iter().map(|e| e.at))
            .fold(0.0, f64::max);
        Synth {
            tracks,
            controls,
            rate: rate.clamp(8000, 192_000) as f32,
            frames: 0,
            end: music_end.max(last) + tail,
            volume: 1.0,
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
                let mut t = Track::new(
                    p,
                    track_events(seq, from, Some(p), program),
                    Source::Builtin(Builtin::new(Family::of_program(program), rate)),
                );
                for q in changed_programs(&t.events) {
                    t.extra
                        .push((q, Source::Builtin(Builtin::new(Family::of_program(q), rate))));
                }
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
        let mut warnings = Vec::new();
        let tracks = score
            .parts
            .iter()
            .enumerate()
            .map(|(pi, p)| {
                let (resolved, warning) = resolve(banks, &p.sound, p.program);
                if let Some(w) = warning {
                    warnings.push(w);
                }
                let mut t = Track::new(
                    pi,
                    track_events(seq, from, Some(pi), p.program),
                    Source::new(&resolved, rate),
                );
                // Instruments changed to inside the part sound automatically (or built-in when
                // the part is set to the built-in sound).
                let fallback = if p.sound.library == scoreart_model::PartSound::BUILT_IN {
                    p.sound.clone()
                } else {
                    scoreart_model::PartSound::default()
                };
                for q in changed_programs(&t.events) {
                    let (r, _) = resolve(banks, &fallback, q);
                    t.extra.push((q, Source::new(&r, rate)));
                }
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
                        program: None,
                    },
                    TrackEvent {
                        at: 0.45,
                        key: (*k).min(127),
                        vel: 0,
                        program: None,
                    },
                ]
            })
            .collect();
        let controls = Arc::new(MixControls::uniform(1));
        let mut s = Synth::assemble(
            vec![Track::new(0, events, Source::new(sound, rate))],
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
            // Gentle limiter.
            for v in chunk.iter_mut() {
                *v = (*v * g).tanh();
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
    /// with its own volume and pan but ignoring mute and solo. Capped at `max_secs`.
    pub fn render_stems(&mut self, max_secs: f64) -> Vec<(usize, Vec<f32>)> {
        let rate = f64::from(self.rate);
        let total = (self.end.min(max_secs).max(0.0) * rate) as u64;
        let g = self.volume * MASTER;
        let mut out: Vec<(usize, Vec<f32>)> = self
            .tracks
            .iter()
            .map(|t| (t.part, Vec::with_capacity(total as usize * 2)))
            .collect();
        while self.frames < total {
            let n = ((total - self.frames) as usize).min(BLOCK);
            for (t, (_, buf)) in self.tracks.iter_mut().zip(out.iter_mut()) {
                t.render(self.frames, n, rate);
                let (gl, gr) = self.controls.gains(t.part);
                for i in 0..n {
                    buf.push((t.l[i] * gl * g).tanh());
                    buf.push((t.r[i] * gr * g).tanh());
                }
            }
            self.frames += n as u64;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scoreart_model::edit::Pos;
    use scoreart_model::{
        Duration, EventKind, KeySig, Note, NoteValue, Pitch, Score, Step, TPQ, TimeSig, instrument,
    };

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
        assert!(matches!(syn.tracks[0].source, Source::Sfz(_)));
        assert!(matches!(syn.tracks[1].source, Source::Builtin(_)));
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
        assert!(matches!(syn.tracks[0].source, Source::Builtin(_)));
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
}
