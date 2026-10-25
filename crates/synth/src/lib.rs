//! ScoreArt's built-in synthesizer. Every sound is generated in code (wavetables built from
//! harmonic recipes written for ScoreArt, with envelopes per instrument family), so no sample
//! library or soundfont is needed or bundled.
//!
//! [`Synth`] streams interleaved stereo `f32` frames from a [`Sequence`]; the platform (Web Audio,
//! cpal) pulls blocks with [`Synth::render`].
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::f32::consts::TAU;
use std::sync::OnceLock;

use scoreart_midi::Sequence;

const TABLE: usize = 2048;
/// Simultaneous voices; the oldest is dropped beyond this.
const MAX_VOICES: usize = 64;

/// Instrument families with their own timbre and envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Piano,
    Organ,
    Guitar,
    Strings,
    Voice,
    Brass,
    Reed,
    Flute,
}

impl Family {
    /// The family for a General MIDI program (0-based).
    pub fn of_program(p: u8) -> Family {
        match p {
            0..=15 => Family::Piano,
            16..=23 => Family::Organ,
            24..=39 => Family::Guitar,
            40..=51 => Family::Strings,
            52..=55 => Family::Voice,
            56..=63 => Family::Brass,
            64..=71 => Family::Reed,
            72..=79 => Family::Flute,
            _ => Family::Piano,
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    /// Harmonic amplitudes for the bright and the mellow table.
    fn recipe(self) -> (&'static [f32], &'static [f32]) {
        match self {
            Family::Piano => (
                &[1.0, 0.55, 0.32, 0.22, 0.14, 0.09, 0.06, 0.04, 0.03],
                &[1.0, 0.25, 0.08, 0.03],
            ),
            Family::Organ => (
                &[1.0, 0.75, 0.5, 0.0, 0.32, 0.0, 0.2, 0.18],
                &[1.0, 0.6, 0.35, 0.0, 0.2],
            ),
            Family::Guitar => (
                &[1.0, 0.7, 0.45, 0.3, 0.2, 0.14, 0.1, 0.06],
                &[1.0, 0.3, 0.1, 0.04],
            ),
            Family::Strings => (
                &[1.0, 0.5, 0.33, 0.25, 0.2, 0.16, 0.14, 0.12, 0.1, 0.09, 0.08],
                &[1.0, 0.45, 0.26, 0.15, 0.08],
            ),
            Family::Voice => (&[1.0, 0.6, 0.9, 0.35, 0.15, 0.08], &[1.0, 0.5, 0.5, 0.15]),
            Family::Brass => (
                &[1.0, 0.8, 0.65, 0.5, 0.4, 0.3, 0.22, 0.16, 0.1],
                &[1.0, 0.5, 0.25, 0.1],
            ),
            Family::Reed => (
                &[1.0, 0.2, 0.7, 0.15, 0.5, 0.1, 0.3, 0.08, 0.15],
                &[1.0, 0.15, 0.4, 0.05, 0.15],
            ),
            Family::Flute => (&[1.0, 0.2, 0.08, 0.03], &[1.0, 0.1, 0.02]),
        }
    }

    /// (attack s, decay time constant s or 0 = sustained, sustain level, release s, brightness decay s)
    fn envelope(self) -> (f32, f32, f32, f32, f32) {
        match self {
            Family::Piano => (0.004, 1.8, 0.0, 0.12, 0.35),
            Family::Organ => (0.012, 0.0, 1.0, 0.06, 0.0),
            Family::Guitar => (0.003, 1.1, 0.0, 0.1, 0.25),
            Family::Strings => (0.08, 0.0, 0.9, 0.18, 0.0),
            Family::Voice => (0.06, 0.0, 0.9, 0.15, 0.0),
            Family::Brass => (0.04, 0.0, 0.85, 0.1, 0.0),
            Family::Reed => (0.03, 0.0, 0.9, 0.08, 0.0),
            Family::Flute => (0.05, 0.0, 0.9, 0.1, 0.0),
        }
    }

    fn vibrato(self) -> f32 {
        match self {
            Family::Strings | Family::Voice => 0.004,
            Family::Flute | Family::Reed => 0.002,
            _ => 0.0,
        }
    }
}

/// Bright and mellow single-cycle tables per family, built once.
fn tables() -> &'static Vec<(Vec<f32>, Vec<f32>)> {
    static T: OnceLock<Vec<(Vec<f32>, Vec<f32>)>> = OnceLock::new();
    T.get_or_init(|| {
        let build = |h: &[f32]| -> Vec<f32> {
            let mut t = vec![0.0f32; TABLE + 1];
            for (i, v) in t.iter_mut().enumerate() {
                let ph = i as f32 / TABLE as f32 * TAU;
                *v = h
                    .iter()
                    .enumerate()
                    .map(|(k, a)| a * ((k + 1) as f32 * ph).sin())
                    .sum();
            }
            let peak = t.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
            t.iter_mut().for_each(|v| *v /= peak);
            t
        };
        [
            Family::Piano,
            Family::Organ,
            Family::Guitar,
            Family::Strings,
            Family::Voice,
            Family::Brass,
            Family::Reed,
            Family::Flute,
        ]
        .iter()
        .map(|f| {
            let (b, m) = f.recipe();
            (build(b), build(m))
        })
        .collect()
    })
}

fn lookup(t: &[f32], phase: f32) -> f32 {
    let x = phase * TABLE as f32;
    let i = (x as usize).min(TABLE - 1);
    let frac = x - i as f32;
    let a = t.get(i).copied().unwrap_or(0.0);
    let b = t.get(i + 1).copied().unwrap_or(0.0);
    a + (b - a) * frac
}

/// A note to play, in seconds from the start of playback.
#[derive(Clone, Copy, Debug)]
struct Event {
    start: f64,
    end: f64,
    freq: f32,
    gain: f32,
    family: Family,
    pan: f32,
}

struct Voice {
    ev: Event,
    phase: f32,
    /// Seconds since the note started.
    age: f32,
    /// Level when the release began (None = not released).
    released: Option<(f32, f32)>,
    level: f32,
    noise: u32,
}

pub struct Synth {
    events: Vec<Event>,
    next: usize,
    voices: Vec<Voice>,
    rate: f32,
    /// Frames rendered so far.
    frames: u64,
    end: f64,
    pub volume: f32,
}

impl Synth {
    /// Plays `seq` from score tick `from` (repeats unrolled), at `rate` Hz.
    pub fn new(seq: &Sequence, from: u64, rate: u32) -> Synth {
        let t0 = seq.seconds_at(from);
        let parts = seq.notes.iter().map(|n| n.part).max().map(|m| m + 1).unwrap_or(1);
        let mut events: Vec<Event> = seq
            .notes
            .iter()
            .filter(|n| n.start >= from)
            .map(|n| {
                let start = seq.seconds_at(n.start) - t0;
                let end = seq.seconds_at(n.start + n.ticks) - t0;
                let pan = if parts > 1 {
                    (n.part as f32 / (parts - 1) as f32 - 0.5) * 0.7
                } else {
                    0.0
                };
                Event {
                    start,
                    end: end.max(start + 0.03),
                    freq: 440.0 * 2f32.powf((f32::from(n.key) - 69.0) / 12.0),
                    gain: (f32::from(n.velocity) / 127.0).powf(1.2),
                    family: Family::of_program(n.program),
                    pan,
                }
            })
            .collect();
        events.sort_by(|a, b| a.start.total_cmp(&b.start));
        let end = (seq.seconds_at(seq.end) - t0).max(events.iter().map(|e| e.end).fold(0.0, f64::max)) + 0.8;
        Synth {
            events,
            next: 0,
            voices: Vec::new(),
            rate: rate.clamp(8000, 192_000) as f32,
            frames: 0,
            end,
            volume: 0.8,
        }
    }

    /// A short audition of a chord (note input feedback).
    pub fn preview(keys: &[u8], program: u8, rate: u32) -> Synth {
        let events = keys
            .iter()
            .map(|k| Event {
                start: 0.0,
                end: 0.45,
                freq: 440.0 * 2f32.powf((f32::from(*k) - 69.0) / 12.0),
                gain: 0.45,
                family: Family::of_program(program),
                pan: 0.0,
            })
            .collect();
        Synth {
            events,
            next: 0,
            voices: Vec::new(),
            rate: rate.clamp(8000, 192_000) as f32,
            frames: 0,
            end: 1.0,
            volume: 0.6,
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate as u32
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
        let dt = 1.0 / self.rate;
        let tables = tables();
        for frame in out.chunks_mut(2) {
            let t = self.frames as f64 / f64::from(self.rate);
            // Start due notes.
            while let Some(ev) = self.events.get(self.next).copied() {
                if ev.start > t {
                    break;
                }
                self.next += 1;
                if self.voices.len() >= MAX_VOICES {
                    self.voices.remove(0);
                }
                self.voices.push(Voice {
                    ev,
                    phase: 0.0,
                    age: 0.0,
                    released: None,
                    level: 0.0,
                    noise: 0x1234_5678 ^ self.next as u32,
                });
            }
            let (mut l, mut r) = (0.0f32, 0.0f32);
            for v in &mut self.voices {
                let fam = v.ev.family;
                let (attack, decay, sustain, release, bright_decay) = fam.envelope();
                // Envelope.
                let mut env = if v.age < attack {
                    v.age / attack.max(1e-4)
                } else if decay > 0.0 {
                    // Lower notes ring longer.
                    let tau = decay * (261.6 / v.ev.freq.max(20.0)).powf(0.35);
                    (-(v.age - attack) / tau).exp().max(sustain)
                } else {
                    sustain
                };
                if v.released.is_none() && t >= v.ev.end {
                    v.released = Some((v.level, v.age));
                }
                if let Some((lvl, at)) = v.released {
                    let k = ((v.age - at) / release.max(1e-3)).min(1.0);
                    env = lvl * (1.0 - k);
                }
                v.level = env;
                // Timbre: bright → mellow over time, and mellow for high notes (no aliasing).
                let mut bright = if bright_decay > 0.0 {
                    (-v.age / bright_decay).exp()
                } else {
                    0.75
                };
                bright *= (1.0 - (v.ev.freq - 700.0) / 1500.0).clamp(0.0, 1.0);
                let (tb, tm) = match tables.get(fam.index()) {
                    Some(t) => (t.0.as_slice(), t.1.as_slice()),
                    None => (&[][..], &[][..]),
                };
                let mut s = lookup(tb, v.phase) * bright + lookup(tm, v.phase) * (1.0 - bright);
                if fam == Family::Flute {
                    // A breath of noise.
                    v.noise = v.noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    s += ((v.noise >> 9) as f32 / (1u32 << 23) as f32 - 1.0) * 0.04;
                }
                let vib = fam.vibrato()
                    * if v.age > 0.25 {
                        (TAU * 5.5 * v.age).sin()
                    } else {
                        0.0
                    };
                v.phase += v.ev.freq * (1.0 + vib) * dt;
                v.phase -= v.phase.floor();
                v.age += dt;
                let a = s * env * v.ev.gain;
                l += a * (0.5 - v.ev.pan * 0.5);
                r += a * (0.5 + v.ev.pan * 0.5);
            }
            self.voices.retain(|v| match v.released {
                Some((_, at)) => v.age - at < v.ev.family.envelope().3 + 0.01,
                None => v.level > 1e-4 || v.age < 0.05 || v.ev.family.envelope().1 == 0.0,
            });
            // Gentle limiter.
            let g = self.volume * 0.9;
            if let [a, b] = frame {
                *a = (l * g).tanh();
                *b = (r * g).tanh();
            } else if let [a] = frame {
                *a = ((l + r) * g * 0.5).tanh();
            }
            self.frames += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scoreart_model::edit::Pos;
    use scoreart_model::*;

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
