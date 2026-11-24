use serde::{Deserialize, Serialize};

/// Ticks per quarter note. 13440 = 2⁷·3·5·7, so 64ths, triplets, quintuplets and septuplets are exact.
pub const TPQ: u32 = 13440;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum NoteValue {
    /// Longa: four whole notes (early music).
    Long,
    Breve,
    Whole,
    Half,
    Quarter,
    Eighth,
    N16th,
    N32nd,
    N64th,
    N128th,
    N256th,
}

impl NoteValue {
    pub const ALL: [NoteValue; 11] = [
        NoteValue::Long,
        NoteValue::Breve,
        NoteValue::Whole,
        NoteValue::Half,
        NoteValue::Quarter,
        NoteValue::Eighth,
        NoteValue::N16th,
        NoteValue::N32nd,
        NoteValue::N64th,
        NoteValue::N128th,
        NoteValue::N256th,
    ];

    pub fn ticks(self) -> u32 {
        match self {
            NoteValue::Long => TPQ * 16,
            NoteValue::Breve => TPQ * 8,
            NoteValue::Whole => TPQ * 4,
            NoteValue::Half => TPQ * 2,
            NoteValue::Quarter => TPQ,
            NoteValue::Eighth => TPQ / 2,
            NoteValue::N16th => TPQ / 4,
            NoteValue::N32nd => TPQ / 8,
            NoteValue::N64th => TPQ / 16,
            NoteValue::N128th => TPQ / 32,
            NoteValue::N256th => TPQ / 64,
        }
    }

    /// Number of flags or beams.
    pub fn beams(self) -> u8 {
        match self {
            NoteValue::Eighth => 1,
            NoteValue::N16th => 2,
            NoteValue::N32nd => 3,
            NoteValue::N64th => 4,
            NoteValue::N128th => 5,
            NoteValue::N256th => 6,
            _ => 0,
        }
    }

    pub fn has_stem(self) -> bool {
        self >= NoteValue::Half
    }

    /// MusicXML `<type>` name.
    pub fn xml_name(self) -> &'static str {
        match self {
            NoteValue::Long => "long",
            NoteValue::Breve => "breve",
            NoteValue::Whole => "whole",
            NoteValue::Half => "half",
            NoteValue::Quarter => "quarter",
            NoteValue::Eighth => "eighth",
            NoteValue::N16th => "16th",
            NoteValue::N32nd => "32nd",
            NoteValue::N64th => "64th",
            NoteValue::N128th => "128th",
            NoteValue::N256th => "256th",
        }
    }

    pub fn from_xml_name(s: &str) -> Option<NoteValue> {
        NoteValue::ALL.into_iter().find(|v| v.xml_name() == s)
    }

    /// Shortcut digit used for note input: 1 = 64th … 5 = quarter … 8 = breve (the values
    /// without a key: 9 = longa, 10 = 128th, 11 = 256th).
    pub fn digit(self) -> u8 {
        match self {
            NoteValue::Long => 9,
            NoteValue::N128th => 10,
            NoteValue::N256th => 11,
            NoteValue::N64th => 1,
            NoteValue::N32nd => 2,
            NoteValue::N16th => 3,
            NoteValue::Eighth => 4,
            NoteValue::Quarter => 5,
            NoteValue::Half => 6,
            NoteValue::Whole => 7,
            NoteValue::Breve => 8,
        }
    }

    pub fn from_digit(d: u8) -> Option<NoteValue> {
        NoteValue::ALL.into_iter().find(|v| v.digit() == d)
    }

    pub fn from_id(s: &str) -> Option<NoteValue> {
        match s {
            "256th" | "256" => Some(NoteValue::N256th),
            "128th" | "128" => Some(NoteValue::N128th),
            "64th" | "64" => Some(NoteValue::N64th),
            "32nd" | "32" => Some(NoteValue::N32nd),
            "16th" | "16" => Some(NoteValue::N16th),
            "eighth" | "8th" | "8" => Some(NoteValue::Eighth),
            "quarter" | "4" => Some(NoteValue::Quarter),
            "half" | "2" => Some(NoteValue::Half),
            "whole" | "1" => Some(NoteValue::Whole),
            "breve" => Some(NoteValue::Breve),
            "long" | "longa" => Some(NoteValue::Long),
            _ => None,
        }
    }
}

/// A tuplet ratio: `actual` notes in the time of `normal` (a triplet is 3:2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Tuplet {
    pub actual: u8,
    pub normal: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Duration {
    pub value: NoteValue,
    /// 0–4 augmentation dots.
    pub dots: u8,
    pub tuplet: Option<Tuplet>,
}

impl Duration {
    pub fn new(value: NoteValue, dots: u8) -> Duration {
        Duration {
            value,
            dots: dots.min(4),
            tuplet: None,
        }
    }

    pub fn ticks(self) -> u32 {
        let base = self.value.ticks();
        let mut t = base;
        let mut add = base;
        for _ in 0..self.dots.min(4) {
            add /= 2;
            t += add;
        }
        match self.tuplet {
            Some(tu) if tu.actual > 0 && tu.normal > 0 => t * u32::from(tu.normal) / u32::from(tu.actual),
            _ => t,
        }
    }

    /// Splits `ticks` into the fewest plain or single-dotted durations, longest first.
    pub fn decompose(ticks: u32) -> Vec<Duration> {
        let mut out = Vec::new();
        let mut left = ticks;
        while left > 0 && out.len() < 64 {
            let pick = NoteValue::ALL
                .into_iter()
                .flat_map(|v| [Duration::new(v, 1), Duration::new(v, 0)])
                .find(|d| d.ticks() <= left);
            match pick {
                Some(d) => {
                    left -= d.ticks();
                    out.push(d);
                }
                // Less than a 64th is left (only from odd imported durations): drop it.
                None => break,
            }
        }
        out
    }

    /// Durations that add up to exactly `ticks` (for rests filling a gap): plain values, or
    /// tuplet values when the gap is a tuplet's length (a triplet eighth's room in a triplet).
    pub fn fill(ticks: u32) -> Vec<Duration> {
        let plain = Duration::decompose(ticks);
        if plain.iter().map(|d| d.ticks()).sum::<u32>() == ticks {
            return plain;
        }
        for (actual, normal) in [(3u32, 2u32), (5, 4), (6, 4), (7, 4), (9, 8)] {
            let Some(scaled) = ticks.checked_mul(actual) else {
                continue;
            };
            if !scaled.is_multiple_of(normal) {
                continue;
            }
            let parts = Duration::decompose(scaled / normal);
            if parts.iter().map(|d| d.ticks()).sum::<u32>() == scaled / normal {
                return parts
                    .into_iter()
                    .map(|mut d| {
                        d.tuplet = Some(Tuplet {
                            actual: actual as u8,
                            normal: normal as u8,
                        });
                        d
                    })
                    .collect();
            }
        }
        plain
    }

    /// The single duration of exactly `ticks`, if there is one (plain, dotted or double-dotted).
    pub fn from_ticks(ticks: u32) -> Option<Duration> {
        NoteValue::ALL
            .into_iter()
            .flat_map(|v| [Duration::new(v, 0), Duration::new(v, 1), Duration::new(v, 2)])
            .find(|d| d.ticks() == ticks)
    }
}
