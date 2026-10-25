use serde::{Deserialize, Serialize};

use crate::KeySig;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Step {
    C,
    D,
    E,
    F,
    G,
    A,
    B,
}

impl Step {
    pub const ALL: [Step; 7] = [Step::C, Step::D, Step::E, Step::F, Step::G, Step::A, Step::B];

    pub fn index(self) -> i32 {
        self as i32
    }

    pub fn from_index(i: i32) -> Step {
        Step::ALL
            .get(i.rem_euclid(7) as usize)
            .copied()
            .unwrap_or(Step::C)
    }

    /// Semitones above C in the natural scale.
    pub fn semitones(self) -> i32 {
        match self {
            Step::C => 0,
            Step::D => 2,
            Step::E => 4,
            Step::F => 5,
            Step::G => 7,
            Step::A => 9,
            Step::B => 11,
        }
    }

    pub fn letter(self) -> char {
        match self {
            Step::C => 'C',
            Step::D => 'D',
            Step::E => 'E',
            Step::F => 'F',
            Step::G => 'G',
            Step::A => 'A',
            Step::B => 'B',
        }
    }

    pub fn from_letter(c: char) -> Option<Step> {
        match c.to_ascii_uppercase() {
            'C' => Some(Step::C),
            'D' => Some(Step::D),
            'E' => Some(Step::E),
            'F' => Some(Step::F),
            'G' => Some(Step::G),
            'A' => Some(Step::A),
            'B' | 'H' => Some(Step::B),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Accidental {
    DoubleFlat,
    Flat,
    Natural,
    Sharp,
    DoubleSharp,
}

impl Accidental {
    pub fn from_alter(alter: i8) -> Accidental {
        match alter {
            i8::MIN..=-2 => Accidental::DoubleFlat,
            -1 => Accidental::Flat,
            0 => Accidental::Natural,
            1 => Accidental::Sharp,
            _ => Accidental::DoubleSharp,
        }
    }
    pub fn alter(self) -> i8 {
        match self {
            Accidental::DoubleFlat => -2,
            Accidental::Flat => -1,
            Accidental::Natural => 0,
            Accidental::Sharp => 1,
            Accidental::DoubleSharp => 2,
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Accidental::DoubleFlat => "double-flat",
            Accidental::Flat => "flat",
            Accidental::Natural => "natural",
            Accidental::Sharp => "sharp",
            Accidental::DoubleSharp => "double-sharp",
        }
    }
    pub fn from_id(s: &str) -> Option<Accidental> {
        [
            Accidental::DoubleFlat,
            Accidental::Flat,
            Accidental::Natural,
            Accidental::Sharp,
            Accidental::DoubleSharp,
        ]
        .into_iter()
        .find(|a| a.id() == s)
    }
}

/// A spelled pitch: step, alteration (-2…+2 semitones) and octave (scientific: C4 = middle C).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pitch {
    pub step: Step,
    pub alter: i8,
    pub octave: i8,
}

impl Pitch {
    pub const MIN_OCTAVE: i8 = 0;
    pub const MAX_OCTAVE: i8 = 9;

    pub fn new(step: Step, alter: i8, octave: i32) -> Pitch {
        Pitch {
            step,
            alter: alter.clamp(-2, 2),
            octave: octave.clamp(i32::from(Pitch::MIN_OCTAVE), i32::from(Pitch::MAX_OCTAVE)) as i8,
        }
    }

    /// Diatonic step number: octave × 7 + step (C4 = 28).
    pub fn diatonic(self) -> i32 {
        i32::from(self.octave) * 7 + self.step.index()
    }

    pub fn from_diatonic(d: i32, alter: i8) -> Pitch {
        Pitch::new(Step::from_index(d.rem_euclid(7)), alter, d.div_euclid(7))
    }

    /// MIDI key number (C4 = 60), clamped to 0–127.
    pub fn midi(self) -> u8 {
        let m = 12 * (i32::from(self.octave) + 1) + self.step.semitones() + i32::from(self.alter);
        m.clamp(0, 127) as u8
    }

    /// Spells a MIDI key number, preferring sharps in sharp keys and flats in flat keys, and
    /// the key's own notes first.
    pub fn from_midi(midi: i32, key: KeySig) -> Pitch {
        let midi = midi.clamp(0, 127);
        let octave = midi.div_euclid(12) - 1;
        let pc = midi.rem_euclid(12);
        // A note of the key itself wins.
        for step in Step::ALL {
            let alter = key.alter_for(step);
            let s = step.semitones() + i32::from(alter);
            if s.rem_euclid(12) == pc {
                // midi = 12 × (octave + 1) + s, so B#3 and Cb5 land in the right octave.
                return Pitch::new(step, alter, (midi - s).div_euclid(12) - 1);
            }
        }
        let sharps = [
            (Step::C, 0),
            (Step::C, 1),
            (Step::D, 0),
            (Step::D, 1),
            (Step::E, 0),
            (Step::F, 0),
            (Step::F, 1),
            (Step::G, 0),
            (Step::G, 1),
            (Step::A, 0),
            (Step::A, 1),
            (Step::B, 0),
        ];
        let flats = [
            (Step::C, 0),
            (Step::D, -1),
            (Step::D, 0),
            (Step::E, -1),
            (Step::E, 0),
            (Step::F, 0),
            (Step::G, -1),
            (Step::G, 0),
            (Step::A, -1),
            (Step::A, 0),
            (Step::B, -1),
            (Step::B, 0),
        ];
        let table = if key.fifths < 0 { flats } else { sharps };
        let (step, alter) = table.get(pc as usize).copied().unwrap_or((Step::C, 0));
        Pitch::new(step, alter, octave)
    }

    /// Up or down by semitones, re-spelled for `key`.
    pub fn transpose_chromatic(self, semitones: i32, key: KeySig) -> Pitch {
        let m = i32::from(self.midi()) + semitones;
        Pitch::from_midi(m.clamp(0, 127), key)
    }

    /// Up or down by diatonic steps, taking the key's alteration for the new step.
    pub fn transpose_diatonic(self, steps: i32, key: KeySig) -> Pitch {
        let d = self.diatonic() + steps;
        let step = Step::from_index(d.rem_euclid(7));
        Pitch::from_diatonic(d, key.alter_for(step))
    }

    pub fn name(self) -> String {
        let acc = match self.alter {
            -2 => "bb",
            -1 => "b",
            1 => "#",
            2 => "##",
            _ => "",
        };
        format!("{}{}{}", self.step.letter(), acc, self.octave)
    }
}
