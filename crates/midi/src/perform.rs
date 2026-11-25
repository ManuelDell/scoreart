//! How the written notes are played: per instrument family (how long a plain, a staccato or a
//! slurred note sounds, how strong an accent is), playing techniques asked for in the text
//! ("pizz.", "con sord." …) and ornaments written out into notes.
//!
//! The marks themselves also travel with every played note ([`Tech`]), so a sound library that
//! has real staccato, pizzicato or tremolo samples can use them instead of an imitation.

use scoreart_model::{KeySig, Pitch, TPQ};

/// Instrument families, from the General MIDI program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// Piano, organ, synthesizers: held as written.
    Keys,
    /// Guitar, harp, mallets, pizzicato: the sound dies away by itself.
    Plucked,
    Bowed,
    Wind,
    Voice,
    Percussion,
}

impl Family {
    pub fn of_program(p: u8) -> Family {
        match p {
            0..=7 | 16..=23 | 80..=103 => Family::Keys,
            8..=15 | 24..=39 | 45 | 46 | 104..=111 => Family::Plucked,
            40..=44 | 48..=51 => Family::Bowed,
            52..=55 => Family::Voice,
            47 | 112..=127 => Family::Percussion,
            _ => Family::Wind,
        }
    }
}

/// How a family plays note lengths and accents.
#[derive(Clone, Copy, Debug)]
pub struct Profile {
    /// Share of the written length a plain note sounds.
    pub plain: f64,
    pub staccato: f64,
    pub staccatissimo: f64,
    pub portato: f64,
    /// Ticks a slurred note runs into the next one.
    pub overlap: u64,
    pub accent: i32,
    pub marcato: i32,
}

pub fn profile(f: Family) -> Profile {
    let q = u64::from(TPQ);
    match f {
        Family::Keys => Profile {
            plain: 0.99,
            staccato: 0.5,
            staccatissimo: 0.3,
            portato: 0.8,
            overlap: q / 16,
            accent: 18,
            marcato: 26,
        },
        Family::Plucked | Family::Percussion => Profile {
            plain: 1.0,
            staccato: 0.55,
            staccatissimo: 0.35,
            portato: 0.85,
            overlap: 0,
            accent: 18,
            marcato: 26,
        },
        Family::Bowed => Profile {
            plain: 0.98,
            staccato: 0.45,
            staccatissimo: 0.25,
            portato: 0.75,
            overlap: q / 12,
            accent: 16,
            marcato: 24,
        },
        Family::Wind | Family::Voice => Profile {
            plain: 0.97,
            staccato: 0.4,
            staccatissimo: 0.25,
            portato: 0.7,
            overlap: q / 16,
            accent: 16,
            marcato: 24,
        },
    }
}

/// A playing technique that changes the sound itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Technique {
    #[default]
    Normal,
    Pizzicato,
    Tremolo,
    /// With mute (con sordino).
    Muted,
}

/// What the notation asks of a played note, for sounds that can do it for real.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tech {
    pub technique: Technique,
    /// Staccato, staccatissimo or spiccato.
    pub short: bool,
    /// Under a slur (not the last note).
    pub legato: bool,
    pub accent: bool,
    /// A note written out from an ornament, glissando or grace note.
    pub ornament: bool,
    /// A note of a chord symbol (the part's harmony channel).
    pub harmony: bool,
}

/// A technique word in staff text, and what it switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TechWord {
    Set(Technique),
    /// arco, ord., senza sord. …: back to normal (only the given technique ends).
    End(Technique),
    /// ord., nat.: everything back to normal.
    Normal,
}

pub fn tech_word(text: &str) -> Option<TechWord> {
    let t = text.to_lowercase();
    let t = t.trim();
    let starts = |ws: &[&str]| ws.iter().any(|w| t.starts_with(w));
    Some(if starts(&["pizz"]) {
        TechWord::Set(Technique::Pizzicato)
    } else if starts(&["arco"]) {
        TechWord::End(Technique::Pizzicato)
    } else if starts(&["trem"]) {
        TechWord::Set(Technique::Tremolo)
    } else if starts(&[
        "con sord",
        "mute",
        "muted",
        "mit dämpfer",
        "mit dpf",
        "straight mute",
        "cup mute",
        "harmon",
    ]) {
        TechWord::Set(Technique::Muted)
    } else if starts(&["senza sord", "open", "ohne dämpfer", "ohne dpf"]) {
        TechWord::End(Technique::Muted)
    } else if starts(&["ord", "nat", "normale", "modo ord"]) {
        TechWord::Normal
    } else {
        return None;
    })
}

/// One written-out note of an ornament: offset from the note's start, length, pitch.
pub type OrnNote = (u64, u64, u8);

/// Notes per beat of a trill at `bpm` (about 12 a second, between 2 and 8 per beat).
fn per_beat(bpm: f64) -> u64 {
    (12.0 * 60.0 / bpm.max(1.0)).round().clamp(2.0, 8.0) as u64
}

/// Plays `pattern` (pitches) quickly at the start of `d` ticks, then holds the last one.
fn quick(pattern: &[u8], d: u64, unit: u64) -> Vec<OrnNote> {
    let n = pattern.len() as u64;
    let unit = unit.min(d / (n + 1)).max(1);
    let mut out: Vec<OrnNote> = Vec::new();
    for (i, k) in pattern.iter().enumerate() {
        let i = i as u64;
        let last = i + 1 == n;
        let len = if last {
            d.saturating_sub(i * unit).max(1)
        } else {
            unit
        };
        out.push((i * unit, len, *k));
    }
    out
}

/// Writes out an ornament on a note of `d` ticks, or None for marks that are not ornaments.
pub fn ornament(id: &str, main: Pitch, key: KeySig, d: u64, bpm: f64) -> Option<Vec<OrnNote>> {
    let m = main.midi();
    let up = main.transpose_diatonic(1, key).midi();
    let down = main.transpose_diatonic(-1, key).midi();
    let unit = (u64::from(TPQ) / per_beat(bpm)).max(1);
    let fast = (unit * 3 / 4).max(1);
    Some(match id {
        "trill" | "shake" | "tremblement" => {
            let count = (d / unit).max(3);
            let unit = (d / count).max(1);
            let mut out = Vec::new();
            for i in 0..count {
                let k = if i % 2 == 0 { m } else { up };
                let len = if i + 1 == count {
                    d.saturating_sub(i * unit).max(1)
                } else {
                    unit
                };
                out.push((i * unit, len, k));
            }
            // End on the main note.
            if count.is_multiple_of(2)
                && let Some(last) = out.last_mut()
            {
                last.2 = m;
            }
            out
        }
        "short-trill" | "up-prall" | "down-prall" | "prall-up" | "prall-down" | "up-mordent" => {
            quick(&[m, up, m], d, fast)
        }
        "prall-mordent" => quick(&[m, up, m, up, m], d, fast),
        "mordent" | "down-mordent" => quick(&[m, down, m], d, fast),
        "turn" | "turn-up" | "haydn" => quick(&[up, m, down, m], d, fast),
        "inverted-turn" | "turn-slash" => quick(&[down, m, up, m], d, fast),
        "schleifer" => {
            let a = main.transpose_diatonic(-2, key).midi();
            quick(&[a, down, m], d, fast)
        }
        // Slides (jazz): into or away from the note.
        "scoop" => quick(
            &[m.saturating_sub(3), m.saturating_sub(2), m.saturating_sub(1), m],
            d,
            fast / 2 + 1,
        ),
        "plop" => quick(&[m + 4, m + 3, m + 2, m + 1, m], d, fast / 2 + 1),
        "doit" | "fall" => {
            let body = d * 2 / 3;
            let rest = d.saturating_sub(body);
            let steps = 5u64;
            let each = (rest / steps).max(1);
            let mut out = vec![(0, body.max(1), m)];
            for i in 1..=steps {
                let k = if id == "doit" {
                    m.saturating_add(i as u8)
                } else {
                    m.saturating_sub(i as u8)
                };
                out.push((body + (i - 1) * each, each, k));
            }
            out
        }
        _ => return None,
    })
}

/// A glissando from `from` towards `to` over `d` ticks (the target is the next note's own).
pub fn glissando(from: u8, to: u8, d: u64) -> Vec<OrnNote> {
    let steps = i32::from(to) - i32::from(from);
    if steps == 0 {
        return vec![(0, d.max(1), from)];
    }
    let n = steps.unsigned_abs() as u64;
    // At most one note per 64th.
    let max = (d / (u64::from(TPQ) / 16).max(1)).max(1);
    let count = n.min(max);
    let each = (d / count).max(1);
    (0..count)
        .map(|i| {
            let k = i32::from(from) + steps * i as i32 / count as i32;
            let len = if i + 1 == count {
                d.saturating_sub(i * each).max(1)
            } else {
                each
            };
            (i * each, len, k.clamp(0, 127) as u8)
        })
        .collect()
}

/// The repeated notes of a tremolo with `strokes` (1 = eighths, 2 = sixteenths, 3 = 32nds …).
pub fn tremolo_unit(strokes: u8) -> u64 {
    (u64::from(TPQ) / 2) >> u64::from(strokes.saturating_sub(1).min(4))
}

#[cfg(test)]
mod tests {
    use super::*;
    use scoreart_model::Step;

    #[test]
    fn families_and_words() {
        assert_eq!(Family::of_program(0), Family::Keys);
        assert_eq!(Family::of_program(40), Family::Bowed);
        assert_eq!(Family::of_program(45), Family::Plucked);
        assert_eq!(Family::of_program(73), Family::Wind);
        assert_eq!(tech_word("pizz."), Some(TechWord::Set(Technique::Pizzicato)));
        assert_eq!(tech_word("arco"), Some(TechWord::End(Technique::Pizzicato)));
        assert_eq!(tech_word("con sord."), Some(TechWord::Set(Technique::Muted)));
        assert_eq!(tech_word("ord."), Some(TechWord::Normal));
        assert_eq!(tech_word("dolce"), None);
    }

    #[test]
    fn a_trill_alternates_with_the_upper_note_in_the_key() {
        // B in D major: the upper neighbour is C sharp.
        let b = Pitch::new(Step::B, 0, 4);
        let notes = ornament("trill", b, KeySig::new(2), u64::from(TPQ), 100.0).unwrap();
        let keys: Vec<u8> = notes.iter().map(|n| n.2).collect();
        assert!(keys.len() >= 4);
        assert_eq!(keys[0], 71);
        assert_eq!(keys[1], 73);
        assert_eq!(*keys.last().unwrap(), 71, "ends on the main note");
        let total: u64 = notes.iter().map(|n| n.1).sum();
        assert_eq!(total, u64::from(TPQ), "fills the note exactly");
    }

    #[test]
    fn mordent_turn_glissando() {
        let c = Pitch::new(Step::C, 0, 4);
        let k = KeySig::new(0);
        let q = u64::from(TPQ);
        let keys = |v: Vec<OrnNote>| v.iter().map(|n| n.2).collect::<Vec<_>>();
        assert_eq!(keys(ornament("mordent", c, k, q, 100.0).unwrap()), [60, 59, 60]);
        assert_eq!(keys(ornament("turn", c, k, q, 100.0).unwrap()), [62, 60, 59, 60]);
        assert!(ornament("accent", c, k, q, 100.0).is_none());
        assert_eq!(keys(glissando(60, 64, q)), [60, 61, 62, 63]);
        assert_eq!(tremolo_unit(1), q / 2);
        assert_eq!(tremolo_unit(3), q / 8);
    }
}
