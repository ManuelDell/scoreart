//! Time as players shape it: gradual tempo changes (accel., rit. …), fermatas that hold,
//! breath marks and caesuras that pause, and swing.
//!
//! Everything here bends seconds, never score ticks: a held fermata is a slower tempo for the
//! length of the note, a caesura a very slow stretch at the end of the note before it (whose
//! sound stops early, so the stretch is heard as silence).

use scoreart_model::{Score, TPQ};

use crate::TempoChange;

/// What a tempo word asks for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TempoWord {
    /// accel., stringendo …: faster.
    Faster,
    /// rit., rall., allargando …: slower.
    Slower,
    /// a tempo, Tempo I: back to the marked tempo.
    Back,
}

/// Reads a tempo word ("rit.", "accelerando", "a tempo" …), ignoring case and dots.
pub fn tempo_word(text: &str) -> Option<TempoWord> {
    let t: String = text
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();
    let t = t.trim();
    let starts = |ws: &[&str]| ws.iter().any(|w| t.starts_with(w));
    if starts(&[
        "a tempo",
        "tempo i",
        "tempo primo",
        "tempo 1",
        "im tempo",
        "wieder im",
    ]) {
        Some(TempoWord::Back)
    } else if starts(&[
        "accel",
        "string",
        "piu mosso",
        "più mosso",
        "poco a poco accel",
        "stretto",
        "schneller",
    ]) {
        Some(TempoWord::Faster)
    } else if starts(&[
        "rit",
        "rall",
        "allarg",
        "slent",
        "calando",
        "morendo",
        "smorz",
        "meno mosso",
        "langsamer",
        "zurückh",
        "poco rit",
        "poco rall",
    ]) {
        Some(TempoWord::Slower)
    } else {
        None
    }
}

/// Swing on ("Swing", "Shuffle") or off ("Straight", "no swing").
pub fn swing_word(text: &str) -> Option<bool> {
    let t = text.to_lowercase();
    let t = t.trim();
    if t.starts_with("straight") || t.starts_with("no swing") || t.starts_with("even 8") {
        Some(false)
    } else if t.starts_with("swing") || t.starts_with("shuffle") || t.contains("swing 8") {
        Some(true)
    } else {
        None
    }
}

/// How much longer a fermata holds its note.
pub fn fermata_stretch(id: &str) -> Option<f64> {
    Some(match id {
        "fermata" => 2.0,
        "fermata-short" | "fermata-short-henze" => 1.5,
        "fermata-very-short" => 1.3,
        "fermata-long" | "fermata-long-henze" => 2.5,
        "fermata-very-long" => 3.0,
        _ => return None,
    })
}

/// The silence after a note with a breath mark or caesura, in seconds.
pub fn pause_secs(id: &str) -> Option<f64> {
    Some(match id {
        "breath" | "breath-tick" | "breath-salzedo" | "breath-upbow" => 0.25,
        "caesura-short" => 0.4,
        "caesura" | "caesura-thick" | "caesura-curved" => 0.75,
        _ => return None,
    })
}

/// The stretch of ticks at the end of a note that a pause takes (its sound stops before).
pub fn pause_ticks(note_ticks: u64) -> u64 {
    (note_ticks / 4).min(u64::from(TPQ) / 2).max(1)
}

/// A span of ticks played `factor` times longer.
#[derive(Clone, Copy, Debug)]
pub struct Stretch {
    pub from: u64,
    pub to: u64,
    pub factor: f64,
}

/// A span of ticks that lasts at least `secs`.
#[derive(Clone, Copy, Debug)]
pub struct Pause {
    pub from: u64,
    pub to: u64,
    pub secs: f64,
}

/// A gradual change from `bpm0` to `bpm1` over ticks `from..to`.
#[derive(Clone, Copy, Debug)]
struct Ramp {
    from: u64,
    to: u64,
    bpm0: f64,
    bpm1: f64,
}

/// How far an unmarked accel. or rit. goes, and over how many measures at most.
const RAMP_FASTER: f64 = 1.25;
const RAMP_SLOWER: f64 = 0.75;
const RAMP_MEASURES: usize = 4;

/// The words of a measure that can change tempo: tempo text, system text, then staff text
/// of any part.
fn words(score: &Score, m: usize) -> Option<TempoWord> {
    let info = score.measures.get(m)?;
    let staff_texts = score
        .parts
        .iter()
        .filter_map(move |p| p.measures.get(m))
        .flat_map(|pm| pm.staves.iter())
        .flat_map(|sm| sm.voices.iter())
        .flat_map(|v| v.events.iter())
        .filter_map(|e| e.text.as_deref());
    [info.tempo_text.as_deref(), info.text.as_deref()]
        .into_iter()
        .flatten()
        .chain(staff_texts)
        .find_map(tempo_word)
}

/// The tempo map of the played measures `played` ((measure, start tick)), ending at `end`.
pub fn tempo_map(
    score: &Score,
    played: &[(usize, u64)],
    end: u64,
    stretches: &[Stretch],
    pauses: &[Pause],
) -> Vec<TempoChange> {
    let start_of = |k: usize| played.get(k).map_or(end, |p| p.1);
    // Per played measure: the steady tempo, and ramps over measures.
    let mut steady: Vec<f64> = Vec::with_capacity(played.len());
    let mut ramps: Vec<Ramp> = Vec::new();
    let mut held: Option<f64> = None;
    let mut k = 0;
    while k < played.len() {
        let m = played.get(k).map_or(0, |p| p.0);
        let marked = score.measures.get(m).and_then(|i| i.tempo).is_some();
        let base = f64::from(score.tempo_at(m).max(1));
        let word = words(score, m);
        if marked || word == Some(TempoWord::Back) {
            held = None;
        }
        let now = held.unwrap_or(base);
        steady.push(now);
        if let Some(w @ (TempoWord::Faster | TempoWord::Slower)) = word {
            // The ramp runs to the next tempo mark or "a tempo" (within a few measures), or
            // for a few measures to a tempo of its own.
            let stop = (k + 1..played.len()).take(RAMP_MEASURES).find(|&j| {
                let mj = played.get(j).map_or(0, |p| p.0);
                score.measures.get(mj).and_then(|i| i.tempo).is_some()
                    || words(score, mj) == Some(TempoWord::Back)
            });
            let (j, target) = match stop {
                Some(j) => {
                    let mj = played.get(j).map_or(0, |p| p.0);
                    let t = score
                        .measures
                        .get(mj)
                        .and_then(|i| i.tempo)
                        .map_or(base, |t| f64::from(t.bpm.max(1)));
                    (j, t)
                }
                None => {
                    let j = (k + RAMP_MEASURES).min(played.len());
                    let f = if w == TempoWord::Faster {
                        RAMP_FASTER
                    } else {
                        RAMP_SLOWER
                    };
                    (j, now * f)
                }
            };
            ramps.push(Ramp {
                from: start_of(k),
                to: start_of(j),
                bpm0: now,
                bpm1: target,
            });
            // Measures inside the ramp keep the ramp's start as their steady tempo (the ramp
            // overrides it); after an open ramp the reached tempo holds.
            for _ in k + 1..j {
                steady.push(now);
            }
            if stop.is_none() {
                held = Some(target);
            }
            k = j;
            continue;
        }
        k += 1;
    }
    let bpm_at = |t: u64| -> f64 {
        if let Some(r) = ramps.iter().find(|r| r.from <= t && t < r.to) {
            let f = (t - r.from) as f64 / (r.to - r.from).max(1) as f64;
            return r.bpm0 + (r.bpm1 - r.bpm0) * f;
        }
        let k = played.partition_point(|p| p.1 <= t).saturating_sub(1);
        steady.get(k).copied().unwrap_or(100.0)
    };
    // Points where the tempo may change.
    let mut points: Vec<u64> = played.iter().map(|p| p.1).collect();
    let step = u64::from(TPQ) / 4;
    for r in &ramps {
        let mut t = r.from;
        while t < r.to && points.len() < 200_000 {
            points.push(t);
            t += step;
        }
        points.push(r.to);
    }
    for s in stretches {
        points.push(s.from);
        points.push(s.to);
    }
    for p in pauses {
        points.push(p.from);
        points.push(p.to);
    }
    points.push(0);
    points.retain(|t| *t <= end);
    points.sort_unstable();
    points.dedup();
    let mut out: Vec<TempoChange> = Vec::new();
    for (i, &t) in points.iter().enumerate() {
        let next = points.get(i + 1).copied().unwrap_or(end.max(t + 1));
        // Ramps: the tempo in the middle of the step.
        let mid = t + (next.saturating_sub(t)) / 2;
        let in_ramp = ramps.iter().any(|r| r.from <= t && t < r.to);
        let mut bpm = if in_ramp { bpm_at(mid) } else { bpm_at(t) };
        let stretch = stretches
            .iter()
            .filter(|s| s.from <= t && t < s.to)
            .map(|s| s.factor)
            .fold(1.0, f64::max);
        bpm /= stretch;
        for p in pauses.iter().filter(|p| p.from <= t && t < p.to) {
            // Slow enough for the span to last `secs`.
            let need = (p.to - p.from) as f64 / f64::from(TPQ) * 60.0 / p.secs.max(0.01);
            bpm = bpm.min(need);
        }
        let bpm = bpm.clamp(1.0, 2000.0);
        if out.last().is_none_or(|l| (l.bpm - bpm).abs() > 1e-6) {
            out.push(TempoChange { tick: t, bpm });
        }
    }
    out
}

/// Where swing moves an eighth note: (later start, longer or shorter length), in ticks.
/// `on` is the onset within the beat's measure, `d` the written length. Only eighths that
/// fall on a beat or halfway are swung (2:1); shorter notes keep their place.
pub fn swing(on: u64, d: u64) -> (u64, i64) {
    let beat = u64::from(TPQ);
    let half = beat / 2;
    let third = beat / 6;
    match (on % beat, d) {
        (p, d) if p == half && d <= half => (third, if d == half { -(third as i64) } else { 0 }),
        (0, d) if d == half => (0, third as i64),
        _ => (0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_tempo_words() {
        assert_eq!(tempo_word("rit."), Some(TempoWord::Slower));
        assert_eq!(tempo_word("Allargando"), Some(TempoWord::Slower));
        assert_eq!(tempo_word("accel."), Some(TempoWord::Faster));
        assert_eq!(tempo_word("A tempo"), Some(TempoWord::Back));
        assert_eq!(tempo_word("Tempo I"), Some(TempoWord::Back));
        assert_eq!(tempo_word("Allegro"), None);
        assert_eq!(swing_word("Swing"), Some(true));
        assert_eq!(swing_word("Straight 8ths"), Some(false));
        assert_eq!(swing_word("dolce"), None);
    }

    #[test]
    fn swing_moves_offbeat_eighths() {
        let q = u64::from(TPQ);
        assert_eq!(swing(0, q / 2), (0, (q / 6) as i64));
        assert_eq!(swing(q / 2, q / 2), (q / 6, -((q / 6) as i64)));
        assert_eq!(swing(q / 4, q / 4), (0, 0), "sixteenths stay");
        assert_eq!(swing(0, q), (0, 0), "quarters stay");
    }
}
