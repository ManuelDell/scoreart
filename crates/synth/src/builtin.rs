//! The built-in synthesizer: every sound is generated in code (wavetables built from harmonic
//! recipes written for ScoreArt, with envelopes per instrument family), so no sample library or
//! soundfont is needed.

use std::f32::consts::TAU;
use std::sync::OnceLock;

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

    pub(crate) fn index(self) -> usize {
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

struct Voice {
    key: u8,
    freq: f32,
    gain: f32,
    phase: f32,
    /// Seconds since the note started.
    age: f32,
    /// Level and age when the release began (None = held).
    released: Option<(f32, f32)>,
    level: f32,
    noise: u32,
}

/// One built-in instrument (one mixer track).
pub(crate) struct Builtin {
    family: Family,
    voices: Vec<Voice>,
    rate: f32,
    seed: u32,
}

impl Builtin {
    pub(crate) fn new(family: Family, rate: u32) -> Builtin {
        Builtin {
            family,
            voices: Vec::new(),
            rate: rate.max(1000) as f32,
            seed: 0x1234_5678,
        }
    }

    pub(crate) fn note_on(&mut self, key: u8, velocity: u8) {
        if self.voices.len() >= MAX_VOICES {
            self.voices.remove(0);
        }
        self.seed = self.seed.wrapping_add(0x9E37_79B9);
        self.voices.push(Voice {
            key,
            freq: 440.0 * 2f32.powf((f32::from(key.min(127)) - 69.0) / 12.0),
            gain: (f32::from(velocity.min(127)) / 127.0).powf(1.2),
            phase: 0.0,
            age: 0.0,
            released: None,
            level: 0.0,
            noise: self.seed,
        });
    }

    pub(crate) fn note_off(&mut self, key: u8) {
        if let Some(v) = self
            .voices
            .iter_mut()
            .find(|v| v.key == key && v.released.is_none())
        {
            v.released = Some((v.level, v.age));
        }
    }

    /// Adds `l.len()` frames into `l` and `r` (mono, the mixer pans).
    pub(crate) fn render(&mut self, l: &mut [f32], r: &mut [f32]) {
        let dt = 1.0 / self.rate;
        let fam = self.family;
        let (attack, decay, sustain, release, bright_decay) = fam.envelope();
        let tables = tables();
        let (tb, tm) = match tables.get(fam.index()) {
            Some(t) => (t.0.as_slice(), t.1.as_slice()),
            None => (&[][..], &[][..]),
        };
        for (lo, ro) in l.iter_mut().zip(r.iter_mut()) {
            let mut acc = 0.0f32;
            for v in &mut self.voices {
                let mut env = if v.age < attack {
                    v.age / attack.max(1e-4)
                } else if decay > 0.0 {
                    // Lower notes ring longer.
                    let tau = decay * (261.6 / v.freq.max(20.0)).powf(0.35);
                    (-(v.age - attack) / tau).exp().max(sustain)
                } else {
                    sustain
                };
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
                bright *= (1.0 - (v.freq - 700.0) / 1500.0).clamp(0.0, 1.0);
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
                v.phase += v.freq * (1.0 + vib) * dt;
                v.phase -= v.phase.floor();
                v.age += dt;
                acc += s * env * v.gain;
            }
            *lo += acc * 0.5;
            *ro += acc * 0.5;
            self.voices.retain(|v| match v.released {
                Some((_, at)) => v.age - at < release + 0.01,
                None => v.level > 1e-4 || v.age < 0.05 || decay == 0.0,
            });
        }
    }
}
