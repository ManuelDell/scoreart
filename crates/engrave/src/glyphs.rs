//! SMuFL code points and the few Bravura metrics the engraver needs, in staff spaces.
//! Code points follow SMuFL 1.4; metrics are read from Bravura's published metadata
//! (`bravura_metadata.json`, OFL-1.1, Steinberg Media Technologies GmbH).

pub const BRACE: char = '\u{E000}';
pub const BRACKET_TOP: char = '\u{E003}';
pub const BRACKET_BOTTOM: char = '\u{E004}';
pub const REPEAT_DOT: char = '\u{E044}';
pub const G_CLEF: char = '\u{E050}';
pub const G_CLEF_8VB: char = '\u{E052}';
pub const C_CLEF: char = '\u{E05C}';
pub const F_CLEF: char = '\u{E062}';
pub const PERC_CLEF: char = '\u{E069}';
pub const TIME_SIG_0: u32 = 0xE080;
pub const TIME_COMMON: char = '\u{E08A}';
pub const TIME_CUT: char = '\u{E08B}';
pub const HEAD_DOUBLE_WHOLE: char = '\u{E0A0}';
pub const HEAD_WHOLE: char = '\u{E0A2}';
pub const HEAD_HALF: char = '\u{E0A3}';
pub const HEAD_BLACK: char = '\u{E0A4}';
pub const DOT: char = '\u{E1E7}';
pub const FLAT: char = '\u{E260}';
pub const NATURAL: char = '\u{E261}';
pub const SHARP: char = '\u{E262}';
pub const DOUBLE_SHARP: char = '\u{E263}';
pub const DOUBLE_FLAT: char = '\u{E264}';
pub const REST_DOUBLE_WHOLE: char = '\u{E4E2}';
pub const REST_WHOLE: char = '\u{E4E3}';
pub const REST_HALF: char = '\u{E4E4}';
pub const REST_QUARTER: char = '\u{E4E5}';
pub const REST_8TH: char = '\u{E4E6}';
pub const REST_16TH: char = '\u{E4E7}';
pub const REST_32ND: char = '\u{E4E8}';
pub const REST_64TH: char = '\u{E4E9}';
pub const TUPLET_0: u32 = 0xE880;
pub const MET_QUARTER_UP: char = '\u{ECA5}';

/// Engraving defaults (Bravura `engravingDefaults`).
pub const STAFF_LINE: f32 = 0.13;
pub const STEM: f32 = 0.12;
pub const LEDGER: f32 = 0.16;
pub const LEDGER_EXT: f32 = 0.4;
pub const BEAM: f32 = 0.5;
pub const BEAM_SPACING: f32 = 0.25;
pub const THIN_BAR: f32 = 0.16;
pub const THICK_BAR: f32 = 0.5;
pub const BAR_SEPARATION: f32 = 0.4;
pub const TIE_MID: f32 = 0.22;

pub const HEAD_W: f32 = 1.18;
/// Stem attachment: up-stems at (1.18, +0.168), down-stems at (0, −0.168), SMuFL y-up.
pub const STEM_Y: f32 = 0.168;

pub fn notehead(v: scoreart_model::NoteValue) -> (char, f32) {
    use scoreart_model::NoteValue::*;
    match v {
        Breve => (HEAD_DOUBLE_WHOLE, 2.396),
        Whole => (HEAD_WHOLE, 1.688),
        Half => (HEAD_HALF, HEAD_W),
        _ => (HEAD_BLACK, HEAD_W),
    }
}

/// Flag glyph and width for `beams` flags, stem up or down.
pub fn flag(beams: u8, up: bool) -> Option<(char, f32)> {
    let (u, d) = match beams {
        1 => (('\u{E240}', 1.056), ('\u{E241}', 1.224)),
        2 => (('\u{E242}', 1.116), ('\u{E243}', 1.168)),
        3 => (('\u{E244}', 1.044), ('\u{E245}', 1.092)),
        4 => (('\u{E246}', 1.044), ('\u{E247}', 1.092)),
        _ => return None,
    };
    Some(if up { u } else { d })
}

pub fn rest(v: scoreart_model::NoteValue) -> (char, f32) {
    use scoreart_model::NoteValue::*;
    match v {
        Breve => (REST_DOUBLE_WHOLE, 0.504),
        Whole => (REST_WHOLE, 1.132),
        Half => (REST_HALF, 1.132),
        Quarter => (REST_QUARTER, 1.08),
        Eighth => (REST_8TH, 1.0),
        N16th => (REST_16TH, 1.28),
        N32nd => (REST_32ND, 1.452),
        N64th => (REST_64TH, 1.6),
    }
}

/// Accidental glyph, width, and its height above/below the origin.
pub fn accidental(a: scoreart_model::Accidental) -> (char, f32) {
    use scoreart_model::Accidental::*;
    match a {
        DoubleFlat => (DOUBLE_FLAT, 1.652),
        Flat => (FLAT, 0.904),
        Natural => (NATURAL, 0.672),
        Sharp => (SHARP, 0.996),
        DoubleSharp => (DOUBLE_SHARP, 1.0),
    }
}

/// Above/below glyph variants and widths.
pub fn articulation(a: scoreart_model::Articulation, above: bool) -> (char, f32) {
    use scoreart_model::Articulation::*;
    let (u, d, w) = match a {
        Accent => ('\u{E4A0}', '\u{E4A1}', 1.356),
        Staccato => ('\u{E4A2}', '\u{E4A3}', 0.336),
        Tenuto => ('\u{E4A4}', '\u{E4A5}', 1.352),
        Staccatissimo => ('\u{E4A6}', '\u{E4A7}', 0.408),
        Marcato => ('\u{E4AC}', '\u{E4AD}', 0.944),
        Fermata => ('\u{E4C0}', '\u{E4C1}', 2.42),
    };
    (if above { u } else { d }, w)
}

/// Dynamic letters and their advance widths.
pub fn dynamic_letter(c: char) -> Option<(char, f32)> {
    Some(match c {
        'p' => ('\u{E520}', 1.46),
        'm' => ('\u{E521}', 1.748),
        'f' => ('\u{E522}', 1.456),
        'r' => ('\u{E523}', 1.108),
        's' => ('\u{E524}', 0.916),
        'z' => ('\u{E525}', 0.976),
        _ => return None,
    })
}

pub fn digit(d: u32, base: u32) -> char {
    char::from_u32(base + d.min(9)).unwrap_or(' ')
}

pub const TIME_DIGIT_W: f32 = 1.88;

pub fn clef(c: scoreart_model::Clef) -> (char, f32, f32) {
    use scoreart_model::Clef::*;
    // (glyph, width, y of the glyph origin in staff spaces from the top line)
    match c {
        Treble => (G_CLEF, 2.684, 3.0),
        Treble8vb => (G_CLEF_8VB, 2.684, 3.0),
        Bass => (F_CLEF, 2.736, 1.0),
        Alto => (C_CLEF, 2.796, 2.0),
        Tenor => (C_CLEF, 2.796, 1.0),
        Percussion => (PERC_CLEF, 1.528, 2.0),
    }
}
