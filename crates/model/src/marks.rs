//! The catalogue of notation marks: articulations, fermatas, breaths and pauses, ornaments,
//! techniques, dynamics, noteheads, tremolos, arpeggios, grace notes, lines (spanners), voltas and
//! jumps. The list covers what common scorewriters offer in their palettes; names, glyphs and
//! file encodings come from the public SMuFL 1.4 and MusicXML 4.0 specifications.

use serde::{Deserialize, Serialize};

use crate::{NoteValue, Pitch};

/// Where a mark sits relative to its note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// Next to the notehead, opposite the stem (staccato, accent …).
    Near,
    /// Above the staff (or above high notes): fermatas, ornaments, bowings.
    Above,
    /// After the note, at the top of the staff (breath marks, caesuras).
    Breath,
    /// Left of the notehead (scoop, plop).
    Before,
    /// Right of the notehead (doit, fall).
    After,
}

/// The MusicXML `<notations>` group a mark is written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XmlGroup {
    Articulations,
    Fermata,
    Ornaments,
    Technical,
}

#[derive(Debug)]
pub struct MarkDef {
    pub id: &'static str,
    pub label: &'static str,
    /// Palette section.
    pub group: &'static str,
    pub place: Place,
    /// SMuFL glyph names above / below the note (the same name when there is no variant).
    pub glyph_above: &'static str,
    pub glyph_below: &'static str,
    pub xml: XmlGroup,
    /// MusicXML element name (for fermatas: the `shape` value).
    pub xml_name: &'static str,
    /// An extra attribute that distinguishes it (`("value", "tick")` …).
    pub xml_attr: Option<(&'static str, &'static str)>,
}

const fn m(
    id: &'static str,
    label: &'static str,
    group: &'static str,
    place: Place,
    glyph_above: &'static str,
    glyph_below: &'static str,
    xml: XmlGroup,
    xml_name: &'static str,
    xml_attr: Option<(&'static str, &'static str)>,
) -> MarkDef {
    MarkDef {
        id,
        label,
        group,
        place,
        glyph_above,
        glyph_below,
        xml,
        xml_name,
        xml_attr,
    }
}

use Place::*;
use XmlGroup::*;

pub const ARTICULATIONS: &str = "Articulations";
pub const FERMATAS: &str = "Fermatas";
pub const BREATHS: &str = "Breaths & pauses";
pub const ORNAMENTS: &str = "Ornaments";
pub const TECHNIQUES: &str = "Techniques";
pub const SLIDES: &str = "Slides";

pub static MARKS: &[MarkDef] = &[
    // Articulations
    m(
        "accent",
        "Accent",
        ARTICULATIONS,
        Near,
        "articAccentAbove",
        "articAccentBelow",
        Articulations,
        "accent",
        None,
    ),
    m(
        "staccato",
        "Staccato",
        ARTICULATIONS,
        Near,
        "articStaccatoAbove",
        "articStaccatoBelow",
        Articulations,
        "staccato",
        None,
    ),
    m(
        "staccatissimo",
        "Staccatissimo",
        ARTICULATIONS,
        Near,
        "articStaccatissimoAbove",
        "articStaccatissimoBelow",
        Articulations,
        "staccatissimo",
        None,
    ),
    m(
        "staccatissimo-stroke",
        "Staccatissimo stroke",
        ARTICULATIONS,
        Near,
        "articStaccatissimoStrokeAbove",
        "articStaccatissimoStrokeBelow",
        Articulations,
        "spiccato",
        None,
    ),
    m(
        "staccatissimo-wedge",
        "Staccatissimo wedge",
        ARTICULATIONS,
        Near,
        "articStaccatissimoWedgeAbove",
        "articStaccatissimoWedgeBelow",
        Articulations,
        "other-articulation",
        Some(("smufl", "articStaccatissimoWedgeAbove")),
    ),
    m(
        "tenuto",
        "Tenuto",
        ARTICULATIONS,
        Near,
        "articTenutoAbove",
        "articTenutoBelow",
        Articulations,
        "tenuto",
        None,
    ),
    m(
        "portato",
        "Portato (louré)",
        ARTICULATIONS,
        Near,
        "articTenutoStaccatoAbove",
        "articTenutoStaccatoBelow",
        Articulations,
        "detached-legato",
        None,
    ),
    m(
        "marcato",
        "Marcato",
        ARTICULATIONS,
        Near,
        "articMarcatoAbove",
        "articMarcatoBelow",
        Articulations,
        "strong-accent",
        None,
    ),
    m(
        "stress",
        "Stress",
        ARTICULATIONS,
        Near,
        "articStressAbove",
        "articStressBelow",
        Articulations,
        "stress",
        None,
    ),
    m(
        "unstress",
        "Unstress",
        ARTICULATIONS,
        Near,
        "articUnstressAbove",
        "articUnstressBelow",
        Articulations,
        "unstress",
        None,
    ),
    m(
        "soft-accent",
        "Soft accent",
        ARTICULATIONS,
        Near,
        "articSoftAccentAbove",
        "articSoftAccentBelow",
        Articulations,
        "soft-accent",
        None,
    ),
    m(
        "laissez-vibrer",
        "Laissez vibrer",
        ARTICULATIONS,
        Near,
        "articLaissezVibrerAbove",
        "articLaissezVibrerBelow",
        Articulations,
        "other-articulation",
        Some(("smufl", "articLaissezVibrerAbove")),
    ),
    // Fermatas (MusicXML `<fermata>` shapes)
    m(
        "fermata",
        "Fermata",
        FERMATAS,
        Above,
        "fermataAbove",
        "fermataBelow",
        Fermata,
        "normal",
        None,
    ),
    m(
        "fermata-short",
        "Short fermata",
        FERMATAS,
        Above,
        "fermataShortAbove",
        "fermataShortBelow",
        Fermata,
        "angled",
        None,
    ),
    m(
        "fermata-long",
        "Long fermata",
        FERMATAS,
        Above,
        "fermataLongAbove",
        "fermataLongBelow",
        Fermata,
        "square",
        None,
    ),
    m(
        "fermata-very-short",
        "Very short fermata",
        FERMATAS,
        Above,
        "fermataVeryShortAbove",
        "fermataVeryShortBelow",
        Fermata,
        "double-angled",
        None,
    ),
    m(
        "fermata-very-long",
        "Very long fermata",
        FERMATAS,
        Above,
        "fermataVeryLongAbove",
        "fermataVeryLongBelow",
        Fermata,
        "double-square",
        None,
    ),
    m(
        "fermata-long-henze",
        "Long fermata (Henze)",
        FERMATAS,
        Above,
        "fermataLongHenzeAbove",
        "fermataLongHenzeBelow",
        Fermata,
        "double-dot",
        None,
    ),
    m(
        "fermata-short-henze",
        "Short fermata (Henze)",
        FERMATAS,
        Above,
        "fermataShortHenzeAbove",
        "fermataShortHenzeBelow",
        Fermata,
        "half-curve",
        None,
    ),
    // Breaths and pauses
    m(
        "breath",
        "Breath mark (comma)",
        BREATHS,
        Breath,
        "breathMarkComma",
        "breathMarkComma",
        Articulations,
        "breath-mark",
        None,
    ),
    m(
        "breath-tick",
        "Breath mark (tick)",
        BREATHS,
        Breath,
        "breathMarkTick",
        "breathMarkTick",
        Articulations,
        "breath-mark",
        Some(("value", "tick")),
    ),
    m(
        "breath-salzedo",
        "Breath mark (Salzedo)",
        BREATHS,
        Breath,
        "breathMarkSalzedo",
        "breathMarkSalzedo",
        Articulations,
        "breath-mark",
        Some(("value", "salzedo")),
    ),
    m(
        "breath-upbow",
        "Breath mark (up-bow like)",
        BREATHS,
        Breath,
        "breathMarkUpbow",
        "breathMarkUpbow",
        Articulations,
        "breath-mark",
        Some(("value", "upbow")),
    ),
    m(
        "caesura",
        "Caesura",
        BREATHS,
        Breath,
        "caesura",
        "caesura",
        Articulations,
        "caesura",
        None,
    ),
    m(
        "caesura-thick",
        "Thick caesura",
        BREATHS,
        Breath,
        "caesuraThick",
        "caesuraThick",
        Articulations,
        "caesura",
        Some(("value", "thick")),
    ),
    m(
        "caesura-short",
        "Short caesura",
        BREATHS,
        Breath,
        "caesuraShort",
        "caesuraShort",
        Articulations,
        "caesura",
        Some(("value", "short")),
    ),
    m(
        "caesura-curved",
        "Curved caesura",
        BREATHS,
        Breath,
        "caesuraCurved",
        "caesuraCurved",
        Articulations,
        "caesura",
        Some(("value", "curved")),
    ),
    // Ornaments
    m(
        "trill",
        "Trill",
        ORNAMENTS,
        Above,
        "ornamentTrill",
        "ornamentTrill",
        Ornaments,
        "trill-mark",
        None,
    ),
    m(
        "short-trill",
        "Short trill (prall)",
        ORNAMENTS,
        Above,
        "ornamentShortTrill",
        "ornamentShortTrill",
        Ornaments,
        "inverted-mordent",
        None,
    ),
    m(
        "mordent",
        "Mordent",
        ORNAMENTS,
        Above,
        "ornamentMordent",
        "ornamentMordent",
        Ornaments,
        "mordent",
        None,
    ),
    m(
        "tremblement",
        "Tremblement (prall-prall)",
        ORNAMENTS,
        Above,
        "ornamentTremblement",
        "ornamentTremblement",
        Ornaments,
        "inverted-mordent",
        Some(("long", "yes")),
    ),
    m(
        "prall-mordent",
        "Prall-mordent",
        ORNAMENTS,
        Above,
        "ornamentPrecompTrillWithMordent",
        "ornamentPrecompTrillWithMordent",
        Ornaments,
        "other-ornament",
        Some(("smufl", "ornamentPrecompTrillWithMordent")),
    ),
    m(
        "up-prall",
        "Up prall",
        ORNAMENTS,
        Above,
        "ornamentPrecompSlide",
        "ornamentPrecompSlide",
        Ornaments,
        "other-ornament",
        Some(("smufl", "ornamentPrecompSlide")),
    ),
    m(
        "down-prall",
        "Down prall",
        ORNAMENTS,
        Above,
        "ornamentPrecompDescendingSlide",
        "ornamentPrecompDescendingSlide",
        Ornaments,
        "other-ornament",
        Some(("smufl", "ornamentPrecompDescendingSlide")),
    ),
    m(
        "up-mordent",
        "Up mordent",
        ORNAMENTS,
        Above,
        "ornamentPrecompMordentUpperPrefix",
        "ornamentPrecompMordentUpperPrefix",
        Ornaments,
        "other-ornament",
        Some(("smufl", "ornamentPrecompMordentUpperPrefix")),
    ),
    m(
        "down-mordent",
        "Down mordent",
        ORNAMENTS,
        Above,
        "ornamentPrecompPortDeVoixMordent",
        "ornamentPrecompPortDeVoixMordent",
        Ornaments,
        "other-ornament",
        Some(("smufl", "ornamentPrecompPortDeVoixMordent")),
    ),
    m(
        "prall-down",
        "Prall down",
        ORNAMENTS,
        Above,
        "ornamentPrecompTrillLowerSuffix",
        "ornamentPrecompTrillLowerSuffix",
        Ornaments,
        "other-ornament",
        Some(("smufl", "ornamentPrecompTrillLowerSuffix")),
    ),
    m(
        "prall-up",
        "Prall up",
        ORNAMENTS,
        Above,
        "ornamentPrecompInvertedMordentUpperPrefix",
        "ornamentPrecompInvertedMordentUpperPrefix",
        Ornaments,
        "other-ornament",
        Some(("smufl", "ornamentPrecompInvertedMordentUpperPrefix")),
    ),
    m(
        "turn",
        "Turn",
        ORNAMENTS,
        Above,
        "ornamentTurn",
        "ornamentTurn",
        Ornaments,
        "turn",
        None,
    ),
    m(
        "inverted-turn",
        "Inverted turn",
        ORNAMENTS,
        Above,
        "ornamentTurnInverted",
        "ornamentTurnInverted",
        Ornaments,
        "inverted-turn",
        None,
    ),
    m(
        "turn-slash",
        "Turn with slash",
        ORNAMENTS,
        Above,
        "ornamentTurnSlash",
        "ornamentTurnSlash",
        Ornaments,
        "turn",
        Some(("slash", "yes")),
    ),
    m(
        "turn-up",
        "Vertical turn",
        ORNAMENTS,
        Above,
        "ornamentTurnUp",
        "ornamentTurnUp",
        Ornaments,
        "vertical-turn",
        None,
    ),
    m(
        "shake",
        "Shake",
        ORNAMENTS,
        Above,
        "ornamentShake3",
        "ornamentShake3",
        Ornaments,
        "shake",
        None,
    ),
    m(
        "schleifer",
        "Schleifer",
        ORNAMENTS,
        Above,
        "ornamentSchleifer",
        "ornamentSchleifer",
        Ornaments,
        "schleifer",
        None,
    ),
    m(
        "haydn",
        "Haydn ornament",
        ORNAMENTS,
        Above,
        "ornamentHaydn",
        "ornamentHaydn",
        Ornaments,
        "haydn",
        None,
    ),
    // Techniques
    m(
        "up-bow",
        "Up bow",
        TECHNIQUES,
        Above,
        "stringsUpBow",
        "stringsUpBow",
        Technical,
        "up-bow",
        None,
    ),
    m(
        "down-bow",
        "Down bow",
        TECHNIQUES,
        Above,
        "stringsDownBow",
        "stringsDownBow",
        Technical,
        "down-bow",
        None,
    ),
    m(
        "harmonic",
        "Harmonic",
        TECHNIQUES,
        Above,
        "stringsHarmonic",
        "stringsHarmonic",
        Technical,
        "harmonic",
        None,
    ),
    m(
        "thumb-position",
        "Thumb position",
        TECHNIQUES,
        Above,
        "stringsThumbPosition",
        "stringsThumbPosition",
        Technical,
        "thumb-position",
        None,
    ),
    m(
        "open",
        "Open",
        TECHNIQUES,
        Above,
        "brassMuteOpen",
        "brassMuteOpen",
        Technical,
        "open",
        None,
    ),
    m(
        "stopped",
        "Stopped / closed (+)",
        TECHNIQUES,
        Above,
        "brassMuteClosed",
        "brassMuteClosed",
        Technical,
        "stopped",
        None,
    ),
    m(
        "half-open",
        "Half open",
        TECHNIQUES,
        Above,
        "brassMuteHalfClosed",
        "brassMuteHalfClosed",
        Technical,
        "half-muted",
        None,
    ),
    m(
        "snap-pizzicato",
        "Snap pizzicato",
        TECHNIQUES,
        Above,
        "pluckedSnapPizzicatoAbove",
        "pluckedSnapPizzicatoBelow",
        Technical,
        "snap-pizzicato",
        None,
    ),
    m(
        "left-hand-pizzicato",
        "Left-hand pizzicato",
        TECHNIQUES,
        Above,
        "pluckedLeftHandPizzicato",
        "pluckedLeftHandPizzicato",
        Technical,
        "other-technical",
        Some(("smufl", "pluckedLeftHandPizzicato")),
    ),
    // Slides into and out of notes
    m(
        "scoop",
        "Scoop",
        SLIDES,
        Before,
        "brassScoop",
        "brassScoop",
        Articulations,
        "scoop",
        None,
    ),
    m(
        "plop",
        "Plop",
        SLIDES,
        Before,
        "brassPlop",
        "brassPlop",
        Articulations,
        "plop",
        None,
    ),
    m(
        "doit",
        "Doit",
        SLIDES,
        After,
        "brassDoitMedium",
        "brassDoitMedium",
        Articulations,
        "doit",
        None,
    ),
    m(
        "fall",
        "Fall",
        SLIDES,
        After,
        "brassFallLipMedium",
        "brassFallLipMedium",
        Articulations,
        "falloff",
        None,
    ),
];

/// A mark on a note or rest (an index into [`MARKS`], stored by id in files).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Articulation(u16);

impl Articulation {
    pub fn def(self) -> &'static MarkDef {
        MARKS.get(usize::from(self.0)).unwrap_or(&MARKS[0])
    }
    pub fn id(self) -> &'static str {
        self.def().id
    }
    pub fn from_id(s: &str) -> Option<Articulation> {
        MARKS
            .iter()
            .position(|d| d.id == s)
            .map(|i| Articulation(i as u16))
    }
    pub fn all() -> impl Iterator<Item = Articulation> {
        (0..MARKS.len()).map(|i| Articulation(i as u16))
    }
    pub fn is_fermata(self) -> bool {
        self.def().xml == XmlGroup::Fermata
    }
}

impl std::fmt::Debug for Articulation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id())
    }
}

impl Serialize for Articulation {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.id())
    }
}

impl<'de> Deserialize<'de> for Articulation {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Articulation::from_id(&s).ok_or_else(|| serde::de::Error::custom(format!("unknown mark {s}")))
    }
}

/// Dynamics, from pppppp to ffffff and the accents in between (id, MIDI velocity).
pub static DYNAMICS: &[(&str, u8)] = &[
    ("pppppp", 4),
    ("ppppp", 8),
    ("pppp", 12),
    ("ppp", 16),
    ("pp", 33),
    ("p", 49),
    ("mp", 64),
    ("mf", 80),
    ("f", 96),
    ("ff", 112),
    ("fff", 122),
    ("ffff", 125),
    ("fffff", 126),
    ("ffffff", 127),
    ("fp", 96),
    ("pf", 49),
    ("sf", 112),
    ("sfz", 112),
    ("sff", 118),
    ("sffz", 122),
    ("sfp", 112),
    ("sfpp", 112),
    ("rfz", 104),
    ("rf", 104),
    ("fz", 104),
    ("m", 72),
    ("r", 96),
    ("s", 96),
    ("z", 96),
    ("n", 1),
];

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Dynamic(u8);

impl Dynamic {
    pub fn id(self) -> &'static str {
        DYNAMICS.get(usize::from(self.0)).map(|d| d.0).unwrap_or("mf")
    }
    pub fn from_id(s: &str) -> Option<Dynamic> {
        DYNAMICS.iter().position(|d| d.0 == s).map(|i| Dynamic(i as u8))
    }
    pub fn all() -> impl Iterator<Item = Dynamic> {
        (0..DYNAMICS.len()).map(|i| Dynamic(i as u8))
    }
    /// MIDI velocity used for playback and MIDI export.
    pub fn velocity(self) -> u8 {
        DYNAMICS.get(usize::from(self.0)).map(|d| d.1).unwrap_or(80)
    }
    /// A plain level (p…f) that holds until the next dynamic; accents like sfz don't.
    pub fn is_level(self) -> bool {
        self.id().chars().all(|c| matches!(c, 'p' | 'm' | 'f')) && !matches!(self.id(), "fp" | "pf")
    }
}

impl std::fmt::Debug for Dynamic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id())
    }
}

impl Serialize for Dynamic {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.id())
    }
}

impl<'de> Deserialize<'de> for Dynamic {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Dynamic::from_id(&s).ok_or_else(|| serde::de::Error::custom(format!("unknown dynamic {s}")))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum NoteHead {
    #[default]
    Normal,
    X,
    Diamond,
    Slash,
    Triangle,
    CircleX,
    Cross,
    Circled,
}

impl NoteHead {
    pub const ALL: [NoteHead; 8] = [
        NoteHead::Normal,
        NoteHead::X,
        NoteHead::Diamond,
        NoteHead::Slash,
        NoteHead::Triangle,
        NoteHead::CircleX,
        NoteHead::Cross,
        NoteHead::Circled,
    ];
    /// Id, which is also the MusicXML `<notehead>` value.
    pub fn id(self) -> &'static str {
        match self {
            NoteHead::Normal => "normal",
            NoteHead::X => "x",
            NoteHead::Diamond => "diamond",
            NoteHead::Slash => "slash",
            NoteHead::Triangle => "triangle",
            NoteHead::CircleX => "circle-x",
            NoteHead::Cross => "cross",
            NoteHead::Circled => "circled",
        }
    }
    pub fn from_id(s: &str) -> Option<NoteHead> {
        NoteHead::ALL.into_iter().find(|h| h.id() == s)
    }
}

/// Single-note tremolo strokes through the stem, or a buzz roll.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Tremolo {
    #[default]
    None,
    Strokes(u8),
    Buzz,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Arpeggio {
    #[default]
    None,
    Normal,
    Up,
    Down,
    /// A bracket: play the chord together (non-arpeggio).
    Bracket,
}

impl Arpeggio {
    pub fn id(self) -> &'static str {
        match self {
            Arpeggio::None => "none",
            Arpeggio::Normal => "arpeggio",
            Arpeggio::Up => "arpeggio-up",
            Arpeggio::Down => "arpeggio-down",
            Arpeggio::Bracket => "non-arpeggio",
        }
    }
    pub fn from_id(s: &str) -> Option<Arpeggio> {
        [
            Arpeggio::None,
            Arpeggio::Normal,
            Arpeggio::Up,
            Arpeggio::Down,
            Arpeggio::Bracket,
        ]
        .into_iter()
        .find(|a| a.id() == s)
    }
}

/// A grace note (or grace chord) before the main note.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Grace {
    pub pitches: Vec<Pitch>,
    /// Written value: eighth (default), 16th or 32nd.
    pub value: NoteValue,
    /// Slashed stem: acciaccatura; without: appoggiatura.
    pub slash: bool,
}

/// Repeat navigation marks on a measure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Jump {
    Segno,
    Coda,
    VariedCoda,
    Fine,
    ToCoda,
    DaCapo,
    DaCapoAlFine,
    DaCapoAlCoda,
    DalSegno,
    DalSegnoAlFine,
    DalSegnoAlCoda,
}

impl Jump {
    pub const ALL: [Jump; 11] = [
        Jump::Segno,
        Jump::Coda,
        Jump::VariedCoda,
        Jump::Fine,
        Jump::ToCoda,
        Jump::DaCapo,
        Jump::DaCapoAlFine,
        Jump::DaCapoAlCoda,
        Jump::DalSegno,
        Jump::DalSegnoAlFine,
        Jump::DalSegnoAlCoda,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Jump::Segno => "segno",
            Jump::Coda => "coda",
            Jump::VariedCoda => "varied-coda",
            Jump::Fine => "fine",
            Jump::ToCoda => "to-coda",
            Jump::DaCapo => "da-capo",
            Jump::DaCapoAlFine => "da-capo-al-fine",
            Jump::DaCapoAlCoda => "da-capo-al-coda",
            Jump::DalSegno => "dal-segno",
            Jump::DalSegnoAlFine => "dal-segno-al-fine",
            Jump::DalSegnoAlCoda => "dal-segno-al-coda",
        }
    }
    pub fn from_id(s: &str) -> Option<Jump> {
        Jump::ALL.into_iter().find(|j| j.id() == s)
    }
    /// The printed text for text jumps; `None` for the sign glyphs.
    pub fn text(self) -> Option<&'static str> {
        match self {
            Jump::Segno | Jump::Coda | Jump::VariedCoda => None,
            Jump::Fine => Some("Fine"),
            Jump::ToCoda => Some("To Coda"),
            Jump::DaCapo => Some("D.C."),
            Jump::DaCapoAlFine => Some("D.C. al Fine"),
            Jump::DaCapoAlCoda => Some("D.C. al Coda"),
            Jump::DalSegno => Some("D.S."),
            Jump::DalSegnoAlFine => Some("D.S. al Fine"),
            Jump::DalSegnoAlCoda => Some("D.S. al Coda"),
        }
    }
    /// Signs at the start of a measure, instructions at its end.
    pub fn at_end(self) -> bool {
        !matches!(self, Jump::Segno | Jump::Coda | Jump::VariedCoda)
    }
}

/// Lines that run from one note to another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpannerKind {
    Slur,
    Crescendo,
    Diminuendo,
    /// "cresc." followed by a dashed line.
    CrescendoLine,
    /// "dim." followed by a dashed line.
    DiminuendoLine,
    /// Octave lines: +1 = 8va, −1 = 8vb, ±2 = 15ma/15mb, ±3 = 22ma/22mb.
    Ottava(i8),
    Pedal,
    TrillLine,
    Vibrato,
    LetRing,
    PalmMute,
    Glissando,
    WavyGlissando,
}

impl SpannerKind {
    pub const ALL: [SpannerKind; 18] = [
        SpannerKind::Slur,
        SpannerKind::Crescendo,
        SpannerKind::Diminuendo,
        SpannerKind::CrescendoLine,
        SpannerKind::DiminuendoLine,
        SpannerKind::Ottava(1),
        SpannerKind::Ottava(-1),
        SpannerKind::Ottava(2),
        SpannerKind::Ottava(-2),
        SpannerKind::Ottava(3),
        SpannerKind::Ottava(-3),
        SpannerKind::Pedal,
        SpannerKind::TrillLine,
        SpannerKind::Vibrato,
        SpannerKind::LetRing,
        SpannerKind::PalmMute,
        SpannerKind::Glissando,
        SpannerKind::WavyGlissando,
    ];
    pub fn id(self) -> &'static str {
        match self {
            SpannerKind::Slur => "slur",
            SpannerKind::Crescendo => "crescendo",
            SpannerKind::Diminuendo => "diminuendo",
            SpannerKind::CrescendoLine => "cresc-line",
            SpannerKind::DiminuendoLine => "dim-line",
            SpannerKind::Ottava(1) => "8va",
            SpannerKind::Ottava(-1) => "8vb",
            SpannerKind::Ottava(2) => "15ma",
            SpannerKind::Ottava(-2) => "15mb",
            SpannerKind::Ottava(3) => "22ma",
            SpannerKind::Ottava(-3) => "22mb",
            SpannerKind::Ottava(_) => "8va",
            SpannerKind::Pedal => "pedal",
            SpannerKind::TrillLine => "trill-line",
            SpannerKind::Vibrato => "vibrato",
            SpannerKind::LetRing => "let-ring",
            SpannerKind::PalmMute => "palm-mute",
            SpannerKind::Glissando => "glissando",
            SpannerKind::WavyGlissando => "wavy-glissando",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            SpannerKind::Slur => "Slur",
            SpannerKind::Crescendo => "Crescendo hairpin",
            SpannerKind::Diminuendo => "Diminuendo hairpin",
            SpannerKind::CrescendoLine => "cresc. line",
            SpannerKind::DiminuendoLine => "dim. line",
            SpannerKind::Ottava(1) => "8va alta",
            SpannerKind::Ottava(-1) => "8va bassa",
            SpannerKind::Ottava(2) => "15ma alta",
            SpannerKind::Ottava(-2) => "15ma bassa",
            SpannerKind::Ottava(3) => "22ma alta",
            SpannerKind::Ottava(-3) => "22ma bassa",
            SpannerKind::Ottava(_) => "Ottava",
            SpannerKind::Pedal => "Pedal",
            SpannerKind::TrillLine => "Trill line",
            SpannerKind::Vibrato => "Vibrato",
            SpannerKind::LetRing => "Let ring",
            SpannerKind::PalmMute => "Palm mute",
            SpannerKind::Glissando => "Glissando",
            SpannerKind::WavyGlissando => "Wavy glissando",
        }
    }
    pub fn from_id(s: &str) -> Option<SpannerKind> {
        SpannerKind::ALL.into_iter().find(|k| k.id() == s)
    }
}

/// A point in one voice: measure and onset tick of an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Anchor {
    pub measure: usize,
    pub tick: u32,
}

/// A line from the event at `start` to the event at `end` (inclusive) in one voice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Spanner {
    pub kind: SpannerKind,
    pub part: usize,
    pub staff: usize,
    pub voice: usize,
    pub start: Anchor,
    pub end: Anchor,
}
