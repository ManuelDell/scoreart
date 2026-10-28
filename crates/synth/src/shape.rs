//! The life of a single note: how it starts, swells and lets go.
//!
//! Played notes are not boxes. A player's tone grows into its sound, rounds a little and lets
//! go at the end, like a spoken syllable; there is hardly ever a flat plateau. How that happens
//! depends on how the note is written and played:
//!
//! - staccato: a quick start and an early, fast fall;
//! - accent: a sharp start with a peak that falls back;
//! - a plain note: a soft start, a gentle arch, a short taper before the next note;
//! - legato: a softer start that carries on into the next note without tapering;
//! - long notes: a slow, wide arch (messa di voce).
//!
//! A note leaning towards a higher next note peaks a little later, one falling to a lower
//! note a little earlier (it "leans" in the direction of the line).
//!
//! The shape multiplies the sound's own envelope (the samples' attack and release stay as
//! recorded). It is kept subtle: about one decibel of arch, a few decibels of taper; only short
//! and accented notes move more. Keys, plucked strings and percussion shape their own sound
//! and are left alone.

use scoreart_midi::{Family, Tech};

/// The loudness curve of one note (a multiplier, about 0.3 … 1.4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    /// Sounding length in seconds.
    pub len: f32,
    /// Level at the very start, and how long it takes to grow into the tone.
    start: f32,
    attack: f32,
    /// Where the arch peaks (share of the length) and how deep it is.
    peak: f32,
    depth: f32,
    /// An accent's extra push at the start, and how fast it falls back.
    push: f32,
    push_fall: f32,
    /// The taper at the end: seconds, and the level reached.
    taper: f32,
    end: f32,
}

/// Seconds to grow into the tone, by family (bows and voices slower, brass quickest).
fn base_attack(f: Family) -> f32 {
    match f {
        Family::Bowed => 0.06,
        Family::Voice => 0.07,
        _ => 0.035,
    }
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

impl Shape {
    /// The shape for a note of `len` seconds at velocity `vel`, `step` semitones to the next
    /// note of the line (0 when none follows soon). None for families that shape themselves.
    pub fn for_note(family: Family, tech: Tech, vel: u8, len: f32, step: i32) -> Option<Shape> {
        if !matches!(family, Family::Bowed | Family::Wind | Family::Voice) || !len.is_finite() || len <= 0.0 {
            return None;
        }
        // Louder notes speak faster.
        let speed = 1.25 - 0.5 * f32::from(vel.min(127)) / 127.0;
        let attack = base_attack(family) * speed;
        let lean = (step as f32 * 0.02).clamp(-0.15, 0.15);
        let mut s = if tech.ornament {
            Shape {
                len,
                start: 0.85,
                attack: 0.006,
                peak: 0.5,
                depth: 0.0,
                push: 0.0,
                push_fall: 0.1,
                taper: 0.0,
                end: 1.0,
            }
        } else if tech.short {
            Shape {
                len,
                start: 0.4,
                attack: 0.01 * speed,
                peak: 0.12,
                depth: 0.35,
                push: 0.08,
                push_fall: 0.05,
                taper: len * 0.5,
                end: 0.35,
            }
        } else if tech.legato {
            Shape {
                len,
                start: 0.75,
                attack: attack * 1.5,
                peak: 0.5 + lean,
                depth: 0.08,
                push: 0.0,
                push_fall: 0.1,
                taper: 0.0,
                end: 1.0,
            }
        } else {
            Shape {
                len,
                start: 0.55,
                attack,
                peak: 0.35 + lean,
                depth: 0.1,
                push: 0.0,
                push_fall: 0.1,
                taper: (len * 0.3).min(0.15),
                end: 0.65,
            }
        };
        if tech.accent && !tech.ornament {
            s.start = s.start.max(0.6);
            s.attack = s.attack.min(0.008);
            s.peak = (s.peak - 0.15).max(0.08);
            s.push = 0.3;
            s.push_fall = 0.12;
        }
        // Long notes: a slow, wider arch.
        if len > 1.2 && !tech.short && !tech.accent && !tech.ornament {
            s.depth = 0.14;
            s.peak = (0.45 + lean).clamp(0.2, 0.7);
            s.taper = (len * 0.2).min(0.25);
            s.end = if tech.legato { 0.9 } else { 0.8 };
        }
        s.attack = s.attack.min(len * 0.4).max(0.002);
        s.peak = s.peak.clamp(0.05, 0.9);
        Some(s)
    }

    /// The level `t` seconds after the note started (after its end it holds the end level).
    pub fn gain(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, self.len);
        // Growing into the tone.
        let grow = self.start + (1.0 - self.start) * smooth(t / self.attack);
        // The arch: rising to the peak, then falling, never a flat plateau.
        let u = t / self.len.max(1e-4);
        let bump = if u < self.peak {
            (std::f32::consts::FRAC_PI_2 * u / self.peak).sin()
        } else {
            (std::f32::consts::FRAC_PI_2 * (u - self.peak) / (1.0 - self.peak).max(1e-4)).cos()
        };
        let arch = 1.0 - self.depth + self.depth * bump;
        // An accent's push.
        let push = 1.0 + self.push * (-t / self.push_fall.max(1e-3)).exp();
        // Letting go.
        let taper = if self.taper > 0.0 && t > self.len - self.taper {
            1.0 - (1.0 - self.end) * smooth((t - (self.len - self.taper)) / self.taper)
        } else {
            1.0
        };
        grow * arch * push * taper
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tech(short: bool, legato: bool, accent: bool) -> Tech {
        Tech {
            short,
            legato,
            accent,
            ..Tech::default()
        }
    }

    #[test]
    fn notes_grow_round_and_let_go() {
        let s = Shape::for_note(Family::Bowed, tech(false, false, false), 80, 0.6, 0).unwrap();
        let g = |t: f32| s.gain(t);
        assert!(g(0.0) < 0.6, "a soft start");
        let top = (0..60).map(|i| g(i as f32 * 0.01)).fold(0.0, f32::max);
        assert!(top > 0.95 && top <= 1.0);
        assert!(g(0.59) < 0.75 * top, "lets go at the end");
        // No plateau: the level keeps moving in the middle.
        assert!((g(0.2) - g(0.4)).abs() > 0.005);
    }

    #[test]
    fn staccato_is_quick_and_short_accent_pushes_legato_carries_on() {
        let st = Shape::for_note(Family::Wind, tech(true, false, false), 80, 0.15, 0).unwrap();
        assert!(st.gain(0.012) > 0.9, "fast start");
        assert!(st.gain(0.15) < 0.4, "fast fall");
        let ac = Shape::for_note(Family::Bowed, tech(false, false, true), 100, 0.5, 0).unwrap();
        assert!(ac.gain(0.01) > 1.15, "the accent's peak");
        assert!(ac.gain(0.3) < ac.gain(0.01));
        let le = Shape::for_note(Family::Bowed, tech(false, true, false), 80, 0.5, 0).unwrap();
        assert!(
            le.gain(0.0) >= 0.65 && le.gain(0.5) > 0.85,
            "no gap before the next note"
        );
        // Rising lines peak later than falling ones.
        let up = Shape::for_note(Family::Wind, tech(false, false, false), 80, 0.5, 5).unwrap();
        let down = Shape::for_note(Family::Wind, tech(false, false, false), 80, 0.5, -5).unwrap();
        assert!(up.peak > down.peak);
        // Keys shape themselves.
        assert!(Shape::for_note(Family::Keys, tech(false, false, false), 80, 0.5, 0).is_none());
        assert!(Shape::for_note(Family::Wind, Tech::default(), 80, f32::NAN, 0).is_none());
    }
}
