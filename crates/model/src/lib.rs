//! The ScoreArt score model.
//!
//! A [`Score`] holds global measure information (time and key signatures, tempo, barlines) and a
//! list of [`Part`]s. Each part has one or more staves; each staff measure holds one or more
//! voices; each voice is a sequence of [`Event`]s (chords or rests) that exactly fills the measure.
//! Time is measured in ticks, [`TPQ`] per quarter note.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod edit;
mod pitch;
mod time;

pub use pitch::{Accidental, Pitch, Step};
pub use time::{Duration, NoteValue, TPQ, Tuplet};

use serde::{Deserialize, Serialize};

/// Hard limits that keep hostile input (files, commands) from exhausting memory.
pub mod limits {
    pub const MAX_MEASURES: usize = 10_000;
    pub const MAX_PARTS: usize = 64;
    pub const MAX_STAVES: usize = 4;
    pub const MAX_VOICES: usize = 4;
    pub const MAX_EVENTS_PER_VOICE: usize = 256;
    pub const MAX_CHORD_NOTES: usize = 16;
    pub const MAX_TEXT: usize = 1_000;
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum Error {
    #[error("no such {0}")]
    NotFound(&'static str),
    #[error("invalid: {0}")]
    Invalid(String),
    #[error("limit reached: {0}")]
    Limit(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Clef {
    #[default]
    Treble,
    Treble8vb,
    Bass,
    Alto,
    Tenor,
    Percussion,
}

impl Clef {
    pub const ALL: [Clef; 6] = [
        Clef::Treble,
        Clef::Treble8vb,
        Clef::Bass,
        Clef::Alto,
        Clef::Tenor,
        Clef::Percussion,
    ];

    /// Diatonic number (see [`Pitch::diatonic`]) of the pitch on the top staff line.
    pub fn top_line(self) -> i32 {
        match self {
            Clef::Treble | Clef::Percussion => 38, // F5
            Clef::Treble8vb => 31,                 // F4
            Clef::Bass => 26,                      // A3
            Clef::Alto => 32,                      // G4
            Clef::Tenor => 30,                     // E4
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Clef::Treble => "treble",
            Clef::Treble8vb => "treble-8vb",
            Clef::Bass => "bass",
            Clef::Alto => "alto",
            Clef::Tenor => "tenor",
            Clef::Percussion => "percussion",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Clef::Treble => "Treble clef",
            Clef::Treble8vb => "Treble clef 8vb",
            Clef::Bass => "Bass clef",
            Clef::Alto => "Alto clef",
            Clef::Tenor => "Tenor clef",
            Clef::Percussion => "Percussion clef",
        }
    }

    pub fn from_id(s: &str) -> Option<Clef> {
        Clef::ALL.into_iter().find(|c| c.id() == s)
    }
}

/// A key signature as a position on the circle of fifths: -7 (7 flats) … +7 (7 sharps).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct KeySig {
    pub fifths: i8,
}

impl KeySig {
    pub fn new(fifths: i32) -> KeySig {
        KeySig {
            fifths: fifths.clamp(-7, 7) as i8,
        }
    }

    /// Order in which sharps are added (F C G D A E B); flats go the other way.
    pub const SHARP_ORDER: [Step; 7] = [Step::F, Step::C, Step::G, Step::D, Step::A, Step::E, Step::B];

    /// The alteration this key gives to `step`.
    pub fn alter_for(self, step: Step) -> i8 {
        let n = self.fifths.unsigned_abs() as usize;
        if self.fifths > 0 {
            if KeySig::SHARP_ORDER.iter().take(n).any(|s| *s == step) {
                1
            } else {
                0
            }
        } else if KeySig::SHARP_ORDER.iter().rev().take(n).any(|s| *s == step) {
            -1
        } else {
            0
        }
    }

    pub fn name(self) -> &'static str {
        const MAJOR: [&str; 15] = [
            "Cb", "Gb", "Db", "Ab", "Eb", "Bb", "F", "C", "G", "D", "A", "E", "B", "F#", "C#",
        ];
        MAJOR
            .get((self.fifths as i32 + 7).clamp(0, 14) as usize)
            .copied()
            .unwrap_or("C")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TimeSymbol {
    #[default]
    Normal,
    Common,
    Cut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeSig {
    pub beats: u8,
    pub beat_type: u8,
    pub symbol: TimeSymbol,
}

impl Default for TimeSig {
    fn default() -> Self {
        TimeSig {
            beats: 4,
            beat_type: 4,
            symbol: TimeSymbol::Normal,
        }
    }
}

impl TimeSig {
    /// A time signature with sane bounds: 1–32 beats, beat type a power of two 1–32.
    pub fn new(beats: i64, beat_type: i64) -> Result<TimeSig> {
        if !(1..=32).contains(&beats) {
            return Err(Error::Invalid(format!("beats {beats} (1–32)")));
        }
        if ![1, 2, 4, 8, 16, 32].contains(&beat_type) {
            return Err(Error::Invalid(format!(
                "beat type {beat_type} (1, 2, 4, 8, 16, 32)"
            )));
        }
        Ok(TimeSig {
            beats: beats as u8,
            beat_type: beat_type as u8,
            symbol: TimeSymbol::Normal,
        })
    }

    pub fn measure_ticks(self) -> u32 {
        let bt = u32::from(self.beat_type.max(1));
        u32::from(self.beats.max(1)) * TPQ * 4 / bt
    }

    /// Compound meters (6/8, 9/8, 12/8 …) group in dotted beats.
    pub fn is_compound(self) -> bool {
        self.beat_type >= 8 && self.beats > 3 && self.beats.is_multiple_of(3)
    }

    /// Length of a beaming group in ticks.
    pub fn beat_group_ticks(self) -> u32 {
        let beat = TPQ * 4 / u32::from(self.beat_type.max(1));
        if self.is_compound() {
            beat * 3
        } else if self.beat_type >= 8 && self.beats <= 3 {
            beat * u32::from(self.beats.max(1))
        } else {
            beat
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Articulation {
    Staccato,
    Staccatissimo,
    Accent,
    Tenuto,
    Marcato,
    Fermata,
}

impl Articulation {
    pub const ALL: [Articulation; 6] = [
        Articulation::Staccato,
        Articulation::Staccatissimo,
        Articulation::Accent,
        Articulation::Tenuto,
        Articulation::Marcato,
        Articulation::Fermata,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Articulation::Staccato => "staccato",
            Articulation::Staccatissimo => "staccatissimo",
            Articulation::Accent => "accent",
            Articulation::Tenuto => "tenuto",
            Articulation::Marcato => "marcato",
            Articulation::Fermata => "fermata",
        }
    }
    pub fn from_id(s: &str) -> Option<Articulation> {
        Articulation::ALL.into_iter().find(|a| a.id() == s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dynamic {
    PPP,
    PP,
    P,
    MP,
    MF,
    F,
    FF,
    FFF,
    SF,
    SFZ,
    FP,
}

impl Dynamic {
    pub const ALL: [Dynamic; 11] = [
        Dynamic::PPP,
        Dynamic::PP,
        Dynamic::P,
        Dynamic::MP,
        Dynamic::MF,
        Dynamic::F,
        Dynamic::FF,
        Dynamic::FFF,
        Dynamic::SF,
        Dynamic::SFZ,
        Dynamic::FP,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Dynamic::PPP => "ppp",
            Dynamic::PP => "pp",
            Dynamic::P => "p",
            Dynamic::MP => "mp",
            Dynamic::MF => "mf",
            Dynamic::F => "f",
            Dynamic::FF => "ff",
            Dynamic::FFF => "fff",
            Dynamic::SF => "sf",
            Dynamic::SFZ => "sfz",
            Dynamic::FP => "fp",
        }
    }
    pub fn from_id(s: &str) -> Option<Dynamic> {
        Dynamic::ALL.into_iter().find(|d| d.id() == s)
    }
    /// MIDI velocity used for playback and MIDI export.
    pub fn velocity(self) -> u8 {
        match self {
            Dynamic::PPP => 16,
            Dynamic::PP => 33,
            Dynamic::P => 49,
            Dynamic::MP => 64,
            Dynamic::MF => 80,
            Dynamic::F => 96,
            Dynamic::FF => 112,
            Dynamic::FFF => 127,
            Dynamic::SF | Dynamic::SFZ => 112,
            Dynamic::FP => 96,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub pitch: Pitch,
    /// Tied to the same pitch in the next event of this voice.
    pub tie: bool,
    /// Accidental the user forced to show (courtesy); `None` = automatic.
    pub forced: Option<Accidental>,
}

impl Note {
    pub fn new(pitch: Pitch) -> Note {
        Note {
            pitch,
            tie: false,
            forced: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum EventKind {
    Rest,
    /// Notes sorted from lowest to highest; never empty.
    Chord(Vec<Note>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub kind: EventKind,
    pub duration: Duration,
    #[serde(default)]
    pub articulations: Vec<Articulation>,
    #[serde(default)]
    pub dynamic: Option<Dynamic>,
    #[serde(default)]
    pub lyric: Option<String>,
}

impl Event {
    pub fn rest(duration: Duration) -> Event {
        Event {
            kind: EventKind::Rest,
            duration,
            articulations: Vec::new(),
            dynamic: None,
            lyric: None,
        }
    }

    pub fn chord(notes: Vec<Note>, duration: Duration) -> Event {
        let mut e = Event::rest(duration);
        if !notes.is_empty() {
            let mut notes = notes;
            notes.sort_by_key(|n| (n.pitch.diatonic(), n.pitch.alter));
            e.kind = EventKind::Chord(notes);
        }
        e
    }

    pub fn note(pitch: Pitch, duration: Duration) -> Event {
        Event::chord(vec![Note::new(pitch)], duration)
    }

    pub fn ticks(&self) -> u32 {
        self.duration.ticks()
    }

    pub fn is_rest(&self) -> bool {
        matches!(self.kind, EventKind::Rest)
    }

    pub fn notes(&self) -> &[Note] {
        match &self.kind {
            EventKind::Rest => &[],
            EventKind::Chord(n) => n,
        }
    }

    pub fn notes_mut(&mut self) -> &mut [Note] {
        match &mut self.kind {
            EventKind::Rest => &mut [],
            EventKind::Chord(n) => n,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Voice {
    pub events: Vec<Event>,
}

impl Voice {
    pub fn ticks(&self) -> u32 {
        self.events
            .iter()
            .map(Event::ticks)
            .fold(0u32, u32::saturating_add)
    }

    /// Onset tick of every event.
    pub fn onsets(&self) -> Vec<u32> {
        let mut t = 0u32;
        self.events
            .iter()
            .map(|e| {
                let s = t;
                t = t.saturating_add(e.ticks());
                s
            })
            .collect()
    }

    /// Index and onset of the event sounding at `tick`.
    pub fn event_at(&self, tick: u32) -> Option<(usize, u32)> {
        let mut t = 0u32;
        for (i, e) in self.events.iter().enumerate() {
            let end = t.saturating_add(e.ticks());
            if tick >= t && tick < end {
                return Some((i, t));
            }
            t = end;
        }
        None
    }

    /// True when every event is a rest.
    pub fn is_empty(&self) -> bool {
        self.events.iter().all(Event::is_rest)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct StaffMeasure {
    /// Voice 0 always exists and always fills the measure; voices 1–3 are optional.
    pub voices: Vec<Voice>,
    /// A clef change at the start of this measure.
    #[serde(default)]
    pub clef: Option<Clef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct PartMeasure {
    pub staves: Vec<StaffMeasure>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub id: String,
    pub name: String,
    pub abbreviation: String,
    /// General MIDI program, 0-based (0 = acoustic grand piano).
    pub program: u8,
    /// Initial clef of every staff; its length is the staff count.
    pub clefs: Vec<Clef>,
    pub measures: Vec<PartMeasure>,
}

impl Part {
    pub fn staff_count(&self) -> usize {
        self.clefs.len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Barline {
    #[default]
    Regular,
    Double,
    Final,
    RepeatEnd,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tempo {
    /// Quarter notes per minute.
    pub bpm: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct MeasureInfo {
    /// A time signature change at the start of this measure (always set on measure 0).
    pub time: Option<TimeSig>,
    /// A key signature change at the start of this measure (always set on measure 0).
    pub key: Option<KeySig>,
    pub tempo: Option<Tempo>,
    pub barline: Barline,
    pub repeat_start: bool,
    /// Free text above the system (rehearsal mark, expression).
    pub text: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Meta {
    pub title: String,
    pub subtitle: String,
    pub composer: String,
    pub lyricist: String,
    pub copyright: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Score {
    pub meta: Meta,
    pub measures: Vec<MeasureInfo>,
    pub parts: Vec<Part>,
}

/// An instrument template for new parts. Programs follow General MIDI (0-based).
#[derive(Clone, Copy, Debug)]
pub struct Instrument {
    pub id: &'static str,
    pub name: &'static str,
    pub abbreviation: &'static str,
    pub program: u8,
    pub clefs: &'static [Clef],
}

/// The built-in instrument list (written for ScoreArt; names and GM programs are common knowledge).
pub const INSTRUMENTS: &[Instrument] = &[
    Instrument {
        id: "piano",
        name: "Piano",
        abbreviation: "Pno.",
        program: 0,
        clefs: &[Clef::Treble, Clef::Bass],
    },
    Instrument {
        id: "organ",
        name: "Organ",
        abbreviation: "Org.",
        program: 19,
        clefs: &[Clef::Treble, Clef::Bass],
    },
    Instrument {
        id: "voice",
        name: "Voice",
        abbreviation: "V.",
        program: 52,
        clefs: &[Clef::Treble],
    },
    Instrument {
        id: "soprano",
        name: "Soprano",
        abbreviation: "S.",
        program: 52,
        clefs: &[Clef::Treble],
    },
    Instrument {
        id: "alto",
        name: "Alto",
        abbreviation: "A.",
        program: 52,
        clefs: &[Clef::Treble],
    },
    Instrument {
        id: "tenor",
        name: "Tenor",
        abbreviation: "T.",
        program: 52,
        clefs: &[Clef::Treble8vb],
    },
    Instrument {
        id: "bass-voice",
        name: "Bass",
        abbreviation: "B.",
        program: 52,
        clefs: &[Clef::Bass],
    },
    Instrument {
        id: "flute",
        name: "Flute",
        abbreviation: "Fl.",
        program: 73,
        clefs: &[Clef::Treble],
    },
    Instrument {
        id: "oboe",
        name: "Oboe",
        abbreviation: "Ob.",
        program: 68,
        clefs: &[Clef::Treble],
    },
    Instrument {
        id: "bassoon",
        name: "Bassoon",
        abbreviation: "Bsn.",
        program: 70,
        clefs: &[Clef::Bass],
    },
    Instrument {
        id: "violin",
        name: "Violin",
        abbreviation: "Vln.",
        program: 40,
        clefs: &[Clef::Treble],
    },
    Instrument {
        id: "viola",
        name: "Viola",
        abbreviation: "Vla.",
        program: 41,
        clefs: &[Clef::Alto],
    },
    Instrument {
        id: "cello",
        name: "Cello",
        abbreviation: "Vc.",
        program: 42,
        clefs: &[Clef::Bass],
    },
    Instrument {
        id: "contrabass",
        name: "Contrabass",
        abbreviation: "Cb.",
        program: 43,
        clefs: &[Clef::Bass],
    },
    Instrument {
        id: "guitar",
        name: "Guitar",
        abbreviation: "Gtr.",
        program: 24,
        clefs: &[Clef::Treble8vb],
    },
    Instrument {
        id: "trombone",
        name: "Trombone",
        abbreviation: "Tbn.",
        program: 57,
        clefs: &[Clef::Bass],
    },
    Instrument {
        id: "tuba",
        name: "Tuba",
        abbreviation: "Tba.",
        program: 58,
        clefs: &[Clef::Bass],
    },
];

pub fn instrument(id: &str) -> Option<&'static Instrument> {
    INSTRUMENTS.iter().find(|i| i.id == id)
}

impl Score {
    /// A new score with `parts` instruments and `measures` empty measures.
    pub fn new(
        title: &str,
        instruments: &[&Instrument],
        time: TimeSig,
        key: KeySig,
        measures: usize,
    ) -> Score {
        let mut s = Score {
            meta: Meta {
                title: clip(title),
                ..Meta::default()
            },
            ..Score::default()
        };
        let n = measures.clamp(1, limits::MAX_MEASURES);
        for i in 0..n {
            let mut info = MeasureInfo::default();
            if i == 0 {
                info.time = Some(time);
                info.key = Some(key);
            }
            if i + 1 == n {
                info.barline = Barline::Final;
            }
            s.measures.push(info);
        }
        for inst in instruments.iter().take(limits::MAX_PARTS) {
            s.add_part(inst);
        }
        s.normalize();
        s
    }

    pub fn add_part(&mut self, inst: &Instrument) {
        let id = (1..)
            .map(|k| format!("P{k}"))
            .find(|id| self.parts.iter().all(|p| &p.id != id))
            .unwrap_or_default();
        self.parts.push(Part {
            id,
            name: inst.name.to_string(),
            abbreviation: inst.abbreviation.to_string(),
            program: inst.program,
            clefs: inst.clefs.to_vec(),
            measures: Vec::new(),
        });
        self.normalize();
    }

    pub fn time_at(&self, measure: usize) -> TimeSig {
        self.measures
            .iter()
            .take(measure.saturating_add(1))
            .rev()
            .find_map(|m| m.time)
            .unwrap_or_default()
    }

    pub fn key_at(&self, measure: usize) -> KeySig {
        self.measures
            .iter()
            .take(measure.saturating_add(1))
            .rev()
            .find_map(|m| m.key)
            .unwrap_or_default()
    }

    pub fn tempo_at(&self, measure: usize) -> u16 {
        self.measures
            .iter()
            .take(measure.saturating_add(1))
            .rev()
            .find_map(|m| m.tempo)
            .map(|t| t.bpm)
            .unwrap_or(100)
    }

    pub fn measure_ticks(&self, measure: usize) -> u32 {
        self.time_at(measure).measure_ticks()
    }

    /// The clef in effect for `staff` of `part` at the start of `measure`.
    pub fn clef_at(&self, part: usize, staff: usize, measure: usize) -> Clef {
        let Some(p) = self.parts.get(part) else {
            return Clef::Treble;
        };
        p.measures
            .iter()
            .take(measure.saturating_add(1))
            .rev()
            .find_map(|m| m.staves.get(staff).and_then(|s| s.clef))
            .or_else(|| p.clefs.get(staff).copied())
            .unwrap_or_default()
    }

    pub fn staff_measure(&self, part: usize, staff: usize, measure: usize) -> Option<&StaffMeasure> {
        self.parts.get(part)?.measures.get(measure)?.staves.get(staff)
    }

    pub fn staff_measure_mut(
        &mut self,
        part: usize,
        staff: usize,
        measure: usize,
    ) -> Option<&mut StaffMeasure> {
        self.parts
            .get_mut(part)?
            .measures
            .get_mut(measure)?
            .staves
            .get_mut(staff)
    }

    pub fn voice(&self, part: usize, staff: usize, measure: usize, voice: usize) -> Option<&Voice> {
        self.staff_measure(part, staff, measure)?.voices.get(voice)
    }

    pub fn voice_mut(
        &mut self,
        part: usize,
        staff: usize,
        measure: usize,
        voice: usize,
    ) -> Option<&mut Voice> {
        self.staff_measure_mut(part, staff, measure)?
            .voices
            .get_mut(voice)
    }

    /// Brings the score into its invariants: at least one measure, measure 0 carries time and key,
    /// every part has one `PartMeasure` per measure with one `StaffMeasure` per staff, voice 0
    /// exists everywhere and every voice exactly fills its measure (padding with rests, cutting
    /// overflow). Safe to call after any edit or import.
    pub fn normalize(&mut self) {
        self.measures.truncate(limits::MAX_MEASURES);
        self.parts.truncate(limits::MAX_PARTS);
        if self.measures.is_empty() {
            self.measures.push(MeasureInfo {
                barline: Barline::Final,
                ..MeasureInfo::default()
            });
        }
        if let Some(first) = self.measures.first_mut() {
            first.time.get_or_insert_with(TimeSig::default);
            first.key.get_or_insert_with(KeySig::default);
        }
        let n = self.measures.len();
        let lens: Vec<u32> = (0..n).map(|m| self.measure_ticks(m)).collect();
        for part in &mut self.parts {
            if part.clefs.is_empty() {
                part.clefs.push(Clef::Treble);
            }
            part.clefs.truncate(limits::MAX_STAVES);
            let staves = part.clefs.len();
            part.measures.resize_with(n, PartMeasure::default);
            for (pm, len) in part.measures.iter_mut().zip(lens.iter().copied()) {
                pm.staves.resize_with(staves, StaffMeasure::default);
                for sm in &mut pm.staves {
                    if sm.voices.is_empty() {
                        sm.voices.push(Voice::default());
                    }
                    sm.voices.truncate(limits::MAX_VOICES);
                    for (vi, v) in sm.voices.iter_mut().enumerate() {
                        fit_voice(v, len, vi == 0);
                    }
                    // Drop secondary voices that hold nothing but rests.
                    let mut i = 1;
                    while i < sm.voices.len() {
                        if sm.voices.get(i).is_some_and(Voice::is_empty) {
                            sm.voices.remove(i);
                        } else {
                            i += 1;
                        }
                    }
                }
            }
        }
        // Ties into a rest or a different pitch are dropped later by the engraver; nothing to do here.
    }

    pub fn total_ticks(&self) -> u64 {
        (0..self.measures.len())
            .map(|m| u64::from(self.measure_ticks(m)))
            .sum()
    }
}

/// Makes `v` last exactly `len` ticks: cut events that overflow, pad with rests.
fn fit_voice(v: &mut Voice, len: u32, _primary: bool) {
    v.events.truncate(limits::MAX_EVENTS_PER_VOICE);
    let mut t = 0u32;
    let mut keep = Vec::with_capacity(v.events.len());
    for e in v.events.drain(..) {
        let d = e.ticks();
        if d == 0 {
            continue;
        }
        if t.saturating_add(d) <= len {
            t += d;
            keep.push(e);
        } else {
            // Cut the overflowing event to what still fits.
            let room = len.saturating_sub(t);
            for piece in Duration::decompose(room) {
                let mut cut = e.clone();
                cut.duration = piece;
                t += piece.ticks();
                keep.push(cut);
            }
            break;
        }
    }
    if t < len {
        for piece in Duration::decompose(len - t) {
            keep.push(Event::rest(piece));
        }
    }
    v.events = keep;
}

/// Trims user text to [`limits::MAX_TEXT`] characters.
pub fn clip(s: &str) -> String {
    s.chars().take(limits::MAX_TEXT).collect()
}

#[cfg(test)]
mod tests;
