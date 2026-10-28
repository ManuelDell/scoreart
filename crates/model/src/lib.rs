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

mod arrange;
pub mod edit;
pub mod marks;
mod pitch;
mod time;

pub use arrange::{Line, Rearrange};
pub use marks::{
    Anchor, Arpeggio, Articulation, Dynamic, Grace, Jump, NoteHead, Spanner, SpannerKind, Tremolo,
};
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
    pub const MAX_SPANNERS: usize = 20_000;
    pub const MAX_GRACE: usize = 8;
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub pitch: Pitch,
    /// Tied to the same pitch in the next event of this voice.
    pub tie: bool,
    /// Accidental the user forced to show (courtesy); `None` = automatic.
    pub forced: Option<Accidental>,
    #[serde(default)]
    pub head: NoteHead,
    /// Notehead in parentheses (ghost note).
    #[serde(default)]
    pub paren: bool,
    /// Fingering shown above the note ("1", "3-4", "p" …).
    #[serde(default)]
    pub fingering: Option<String>,
}

impl Note {
    pub fn new(pitch: Pitch) -> Note {
        Note {
            pitch,
            tie: false,
            forced: None,
            head: NoteHead::Normal,
            paren: false,
            fingering: None,
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
    /// Lyric syllables by verse (index 0 = verse 1; "" = none in that verse). A syllable ending
    /// in `-` is joined to the next one by a hyphen ("Freu-" "de").
    #[serde(default)]
    pub lyrics: Vec<String>,
    /// Grace notes before this event.
    #[serde(default)]
    pub grace: Vec<Grace>,
    #[serde(default)]
    pub tremolo: Tremolo,
    #[serde(default)]
    pub arpeggio: Arpeggio,
    /// Chord symbol above the staff ("Cmaj7", "F#m7b5/E" …).
    #[serde(default)]
    pub chord_symbol: Option<String>,
    /// Staff text / expression ("dolce", "pizz." …).
    #[serde(default)]
    pub text: Option<String>,
    /// Where the staff text was moved to.
    #[serde(default)]
    pub text_place: Option<Placement>,
    /// How far the dynamic was moved from its place under the staff.
    #[serde(default)]
    pub dynamic_place: Option<Placement>,
}

/// Most lyric verses kept per note.
pub const MAX_VERSES: usize = 12;

impl Event {
    /// The syllable of `verse` (0-based), if any.
    pub fn lyric(&self, verse: usize) -> Option<&str> {
        self.lyrics
            .get(verse)
            .map(String::as_str)
            .filter(|l| !l.is_empty())
    }

    /// Sets (or with `None`/"" clears) the syllable of `verse` (0-based).
    pub fn set_lyric(&mut self, verse: usize, text: Option<String>) {
        if verse >= MAX_VERSES {
            return;
        }
        let text = text.unwrap_or_default();
        if self.lyrics.len() <= verse {
            if text.is_empty() {
                return;
            }
            self.lyrics.resize(verse + 1, String::new());
        }
        if let Some(l) = self.lyrics.get_mut(verse) {
            *l = text;
        }
        while self.lyrics.last().is_some_and(String::is_empty) {
            self.lyrics.pop();
        }
    }

    /// Number of verses with a syllable on this event (the highest verse used).
    pub fn verse_count(&self) -> usize {
        self.lyrics.len()
    }

    pub fn rest(duration: Duration) -> Event {
        Event {
            kind: EventKind::Rest,
            duration,
            articulations: Vec::new(),
            dynamic: None,
            lyrics: Vec::new(),
            grace: Vec::new(),
            tremolo: Tremolo::None,
            arpeggio: Arpeggio::None,
            chord_symbol: None,
            text: None,
            text_place: None,
            dynamic_place: None,
        }
    }

    /// Clears everything attached to the event except its kind and duration.
    pub fn clear_marks(&mut self) {
        let keep = (self.kind.clone(), self.duration);
        *self = Event::rest(keep.1);
        self.kind = keep.0;
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
    /// Shown as a one-measure repeat sign (and played like the previous measure).
    #[serde(default)]
    pub measure_repeat: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct PartMeasure {
    pub staves: Vec<StaffMeasure>,
    /// From this measure on the part plays another instrument ("To Flute").
    #[serde(default)]
    pub instrument_change: Option<InstrumentChange>,
}

/// An instrument change inside a part, e.g. a player switching from flute to piccolo.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstrumentChange {
    pub name: String,
    pub abbreviation: String,
    /// General MIDI program, 0-based.
    pub program: u8,
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
    /// Playback sound and mixer settings.
    #[serde(default)]
    pub sound: PartSound,
    /// Staves not shown (by staff index; missing = shown). Hidden staves still play.
    #[serde(default)]
    pub hidden: Vec<bool>,
}

/// Which sound plays a part, and its mixer channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartSound {
    /// Name of a loaded sound library (SoundFont or SFZ); empty = automatic (the first loaded
    /// SoundFont with the part's MIDI program, else the built-in synthesizer).
    pub library: String,
    /// Preset (SoundFont) or instrument (SFZ) inside the library.
    pub preset: String,
    /// 0…100.
    pub volume: u8,
    /// -100 (left) … 100 (right).
    pub pan: i8,
}

impl Default for PartSound {
    fn default() -> Self {
        PartSound {
            library: String::new(),
            preset: String::new(),
            volume: 80,
            pan: 0,
        }
    }
}

impl PartSound {
    /// The library name that forces the built-in synthesizer.
    pub const BUILT_IN: &'static str = "ScoreArt";
}

impl Part {
    pub fn staff_count(&self) -> usize {
        self.clefs.len()
    }

    pub fn staff_visible(&self, staff: usize) -> bool {
        !self.hidden.get(staff).copied().unwrap_or(false)
    }

    /// Whether any staff of the part is shown.
    pub fn visible(&self) -> bool {
        (0..self.staff_count()).any(|s| self.staff_visible(s))
    }

    /// Shows or hides every staff of the part.
    pub fn set_visible(&mut self, on: bool) {
        self.hidden = if on {
            Vec::new()
        } else {
            vec![true; self.staff_count()]
        };
    }

    /// The MIDI program sounding in `measure` (after any instrument change).
    pub fn program_at(&self, measure: usize) -> u8 {
        self.measures
            .iter()
            .take(measure.saturating_add(1))
            .rev()
            .find_map(|m| m.instrument_change.as_ref().map(|c| c.program))
            .unwrap_or(self.program)
    }

    /// The instrument name in `measure` (after any instrument change).
    pub fn name_at(&self, measure: usize) -> &str {
        self.measures
            .iter()
            .take(measure.saturating_add(1))
            .rev()
            .find_map(|m| m.instrument_change.as_ref().map(|c| c.name.as_str()))
            .unwrap_or(&self.name)
    }

    /// Replaces the instrument of the whole part: name, abbreviation, MIDI program and staves.
    /// Staves beyond the new instrument's count are removed (with their music); new ones start
    /// with whole-measure rests. The chosen sound goes back to automatic; volume and pan stay.
    pub fn change_instrument(&mut self, inst: &Instrument) {
        self.name = inst.name.to_string();
        self.abbreviation = inst.abbreviation.to_string();
        self.program = inst.program;
        let n = inst.clefs.len().max(1);
        self.clefs = inst.clefs.to_vec();
        for m in &mut self.measures {
            m.staves.truncate(n);
        }
        self.sound.library.clear();
        self.sound.preset.clear();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Barline {
    #[default]
    Regular,
    Double,
    Final,
    RepeatEnd,
    /// No barline is drawn (e.g. between two halves of one written measure).
    Hidden,
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
    /// Tempo word shown before the metronome mark ("Allegro").
    #[serde(default)]
    pub tempo_text: Option<String>,
    /// Boxed rehearsal mark ("A", "12").
    #[serde(default)]
    pub rehearsal: Option<String>,
    /// Volta (ending) number 1–9; consecutive measures with the same number share a bracket.
    #[serde(default)]
    pub volta: Option<u8>,
    #[serde(default)]
    pub jumps: Vec<Jump>,
    /// A new system starts after this measure.
    #[serde(default)]
    pub system_break: bool,
    /// A new page starts after this measure.
    #[serde(default)]
    pub page_break: bool,
    /// Length in ticks when it differs from the time signature (pickup, split measure).
    #[serde(default)]
    pub actual: Option<u32>,
    /// Left out of the measure numbering (e.g. a pickup).
    #[serde(default)]
    pub uncounted: bool,
    /// The volta's printed label when it is not "n." (e.g. "1." on the ending for pass 2).
    #[serde(default)]
    pub volta_text: Option<String>,
    /// Where the rehearsal mark was moved to (None = default place above the staff).
    #[serde(default)]
    pub rehearsal_place: Option<Placement>,
}

/// A text moved from its default place: offset in tenths of a staff space (y downwards), and
/// whether it sits below the staff instead of above.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    pub dx: i16,
    pub dy: i16,
    pub below: bool,
}

impl Placement {
    /// From an offset in staff spaces.
    pub fn from_sp(dx: f64, dy: f64, below: bool) -> Placement {
        let t = |v: f64| (v * 10.0).round().clamp(-2000.0, 2000.0) as i16;
        Placement {
            dx: t(dx),
            dy: t(dy),
            below,
        }
    }

    pub fn dx_sp(&self) -> f32 {
        f32::from(self.dx) / 10.0
    }

    pub fn dy_sp(&self) -> f32 {
        f32::from(self.dy) / 10.0
    }
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
    /// Slurs, hairpins, octave lines, pedal, trill lines, glissandos …
    #[serde(default)]
    pub spanners: Vec<Spanner>,
    /// Page size, margins and spacing.
    #[serde(default)]
    pub page: PageFormat,
    /// How the score is played back.
    #[serde(default)]
    pub playback: PlaybackStyle,
}

/// How a score is played back (not printed): the room and a human touch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlaybackStyle {
    /// Room (reverb), 0 (dry) … 100.
    pub reverb: u8,
    /// Small, natural variations in timing and loudness.
    pub humanize: bool,
}

impl Default for PlaybackStyle {
    fn default() -> Self {
        PlaybackStyle {
            reverb: 25,
            humanize: true,
        }
    }
}

/// Page size, margins and spacing of the printed score.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PageFormat {
    pub width_mm: f32,
    pub height_mm: f32,
    pub margin_top_mm: f32,
    pub margin_bottom_mm: f32,
    pub margin_left_mm: f32,
    pub margin_right_mm: f32,
    /// Staff size: the distance between two staff lines.
    pub staff_space_mm: f32,
    /// Gap between the staves of one instrument (bottom line to top line), staff spaces.
    pub staff_gap: f32,
    /// Gap between instruments, staff spaces.
    pub part_gap: f32,
    /// Gap between systems, staff spaces.
    pub system_gap: f32,
    /// Horizontal note spacing (1.0 = normal).
    pub spacing: f32,
}

impl Default for PageFormat {
    fn default() -> Self {
        PageFormat {
            width_mm: 210.0,
            height_mm: 297.0,
            margin_top_mm: 15.0,
            margin_bottom_mm: 15.0,
            margin_left_mm: 15.0,
            margin_right_mm: 15.0,
            staff_space_mm: 1.75,
            staff_gap: 6.0,
            part_gap: 7.0,
            system_gap: 9.0,
            spacing: 1.0,
        }
    }
}

impl PageFormat {
    /// Common paper sizes, portrait, in millimetres.
    pub const SIZES: &'static [(&'static str, f32, f32)] = &[
        ("A4", 210.0, 297.0),
        ("Letter", 215.9, 279.4),
        ("A3", 297.0, 420.0),
        ("A5", 148.0, 210.0),
        ("B4", 250.0, 353.0),
        ("Legal", 215.9, 355.6),
        ("Tabloid", 279.4, 431.8),
    ];

    pub fn landscape(&self) -> bool {
        self.width_mm > self.height_mm
    }

    /// The paper size's name, if it is a common one (either orientation).
    pub fn size_name(&self) -> Option<&'static str> {
        let (a, b) = if self.landscape() {
            (self.height_mm, self.width_mm)
        } else {
            (self.width_mm, self.height_mm)
        };
        Self::SIZES
            .iter()
            .find(|(_, w, h)| (w - a).abs() < 1.0 && (h - b).abs() < 1.0)
            .map(|s| s.0)
    }

    /// Keeps every value in a sensible range (also for hostile files).
    pub fn clamped(self) -> PageFormat {
        let c = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        let d = PageFormat::default();
        PageFormat {
            width_mm: c(self.width_mm, 50.0, 2000.0, d.width_mm),
            height_mm: c(self.height_mm, 50.0, 2000.0, d.height_mm),
            margin_top_mm: c(self.margin_top_mm, 0.0, 100.0, d.margin_top_mm),
            margin_bottom_mm: c(self.margin_bottom_mm, 0.0, 100.0, d.margin_bottom_mm),
            margin_left_mm: c(self.margin_left_mm, 0.0, 100.0, d.margin_left_mm),
            margin_right_mm: c(self.margin_right_mm, 0.0, 100.0, d.margin_right_mm),
            staff_space_mm: c(self.staff_space_mm, 0.5, 10.0, d.staff_space_mm),
            staff_gap: c(self.staff_gap, 1.0, 40.0, d.staff_gap),
            part_gap: c(self.part_gap, 1.0, 40.0, d.part_gap),
            system_gap: c(self.system_gap, 1.0, 60.0, d.system_gap),
            spacing: c(self.spacing, 0.5, 2.0, d.spacing),
        }
    }
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
        id: "women",
        name: "Women",
        abbreviation: "W.",
        program: 52,
        clefs: &[Clef::Treble, Clef::Treble],
    },
    Instrument {
        id: "men",
        name: "Men",
        abbreviation: "M.",
        program: 52,
        clefs: &[Clef::Treble8vb, Clef::Bass],
    },
    Instrument {
        id: "choir",
        name: "Choir (SATB)",
        abbreviation: "Ch.",
        program: 52,
        clefs: &[Clef::Treble, Clef::Treble, Clef::Treble8vb, Clef::Bass],
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
        let id = self.free_part_id();
        self.parts.push(Part {
            id,
            name: inst.name.to_string(),
            abbreviation: inst.abbreviation.to_string(),
            program: inst.program,
            clefs: inst.clefs.to_vec(),
            measures: Vec::new(),
            sound: Default::default(),
            hidden: Vec::new(),
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
        self.measures
            .get(measure)
            .and_then(|i| i.actual)
            .filter(|t| *t > 0)
            .map(|t| t.min(TPQ * 64))
            .unwrap_or_else(|| self.time_at(measure).measure_ticks())
    }

    /// The number printed for a measure (uncounted measures are skipped).
    pub fn measure_number(&self, measure: usize) -> usize {
        1 + self
            .measures
            .iter()
            .take(measure)
            .filter(|i| !i.uncounted)
            .count()
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
        self.page = self.page.clamped();
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
                        // Marks in catalogue order, each once.
                        for e in &mut v.events {
                            e.articulations.sort();
                            e.articulations.dedup();
                        }
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
        // Spanners must point at existing staves and measures, start before they end.
        let n = self.measures.len();
        let parts: Vec<usize> = self.parts.iter().map(Part::staff_count).collect();
        let lens: Vec<u32> = (0..n).map(|m| self.measure_ticks(m)).collect();
        self.spanners.truncate(limits::MAX_SPANNERS);
        self.spanners.retain_mut(|sp| {
            let ok = parts.get(sp.part).is_some_and(|st| sp.staff < *st)
                && sp.voice < limits::MAX_VOICES
                && sp.start.measure < n
                && sp.end.measure < n;
            if !ok {
                return false;
            }
            if (sp.end.measure, sp.end.tick) < (sp.start.measure, sp.start.tick) {
                std::mem::swap(&mut sp.start, &mut sp.end);
            }
            for a in [&mut sp.start, &mut sp.end] {
                let len = lens.get(a.measure).copied().unwrap_or(1);
                a.tick = a.tick.min(len.saturating_sub(1));
            }
            true
        });
        let mut seen = std::collections::HashSet::new();
        self.spanners.retain(|sp| seen.insert(*sp));
        // A canonical order, so equal scores compare equal whatever order lines were added in.
        self.spanners.sort_by_key(|sp| {
            (
                sp.part,
                sp.staff,
                sp.voice,
                sp.start.measure,
                sp.start.tick,
                sp.end.measure,
                sp.end.tick,
                sp.kind.id(),
            )
        });
        for m in &mut self.measures {
            m.volta = m.volta.filter(|v| (1..=9).contains(v));
            m.jumps.truncate(4);
        }
    }

    /// Octaves the notes at this point are displayed away from their pitch by an octave line
    /// (8va = +1 means written an octave lower than sounding).
    pub fn ottava_at(&self, part: usize, staff: usize, measure: usize, tick: u32) -> i8 {
        self.spanners
            .iter()
            .filter(|s| s.part == part && s.staff == staff)
            .filter_map(|s| match s.kind {
                SpannerKind::Ottava(o) => Some((o, s)),
                _ => None,
            })
            .find(|(_, s)| {
                (s.start.measure, s.start.tick) <= (measure, tick)
                    && (measure, tick) <= (s.end.measure, s.end.tick)
            })
            .map(|(o, _)| o)
            .unwrap_or(0)
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
