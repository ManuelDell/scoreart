//! Which notation symbol a glyph of a music font is: by its SMuFL code point (Bravura, Leland,
//! Petaluma, Finale Maestro … — the code points are the public SMuFL 1.4 standard), or by the
//! keyboard layout of the older "Sonata-compatible" music fonts (Opus in Sibelius, Maestro in
//! Finale, Petrucci, Engraver, Jazz …), read off PDFs those programs wrote.

use scoreart_model::{Articulation, NoteValue};

use crate::collect::Glyph;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClefSym {
    G,
    G8vb,
    G8va,
    F,
    F8vb,
    C,
    Percussion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeadSym {
    Black,
    Half,
    Whole,
    Breve,
    /// Cross, diamond, slash … heads: counted as black.
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Sym {
    Clef(ClefSym),
    Head(HeadSym),
    Rest(NoteValue),
    /// A flag with this many beams' worth (1 = eighth).
    Flag(u8),
    Accidental(i8),
    /// Augmentation dot (also a staccato dot: the place decides).
    Dot,
    TimeDigit(u8),
    TimeCommon,
    TimeCut,
    /// A dynamic, as its letters ("mf", "sfz").
    Dynamic(&'static str),
    Articulation(Articulation),
    Tuplet(u8),
    Brace,
    RepeatDots,
    /// A music-font glyph of no interest here.
    Other,
}

/// Music fonts with the Sonata layout (letters and symbols on the keyboard positions).
fn is_sonata(font: &str) -> bool {
    let f = font.to_ascii_lowercase();
    !f.contains("text")
        && [
            "opus", "maestro", "petrucci", "engraver", "jazz", "sonata", "helsinki", "inkpen", "reprise",
            "norfolk", "broadway", "golden", "tamburo", "pmusic", "mscore1",
        ]
        .iter()
        .any(|n| f.contains(n))
}

/// Text fonts that come with music fonts but hold letters (Opus Text, Maestro Times …).
fn is_text_companion(font: &str) -> bool {
    let f = font.to_ascii_lowercase();
    f.contains("text") || f.contains("times") || f.contains("chords") || f.contains("figured")
}

/// Whether a glyph belongs to a music font (else it is text).
pub(crate) fn is_music(g: &Glyph) -> bool {
    if let Some(c) = g.ch
        && ('\u{E000}'..='\u{F8FF}').contains(&c)
    {
        return true;
    }
    is_sonata(&g.font) && !is_text_companion(&g.font)
}

pub(crate) fn classify(g: &Glyph) -> Sym {
    let Some(c) = g.ch else {
        return Sym::Other;
    };
    if ('\u{E000}'..='\u{F8FF}').contains(&c) {
        // Optional glyphs (Dorico uses Bravura's alternate noteheads, for one) stand for their base.
        let base = crate::alternates::ALTERNATES
            .binary_search_by_key(&(c as u32), |a| a.0)
            .ok()
            .and_then(|i| crate::alternates::ALTERNATES.get(i))
            .and_then(|a| char::from_u32(a.1))
            .unwrap_or(c);
        return smufl(base);
    }
    if is_sonata(&g.font) && !is_text_companion(&g.font) {
        let special = g.font.to_ascii_lowercase().contains("special");
        return if special { Sym::Other } else { sonata(c) };
    }
    Sym::Other
}

fn smufl(c: char) -> Sym {
    use HeadSym::*;
    let u = c as u32;
    match u {
        0xE000 => Sym::Brace,
        0xE050 | 0xE07A => Sym::Clef(ClefSym::G),
        0xE052 | 0xE055 | 0xE056 => Sym::Clef(ClefSym::G8vb),
        0xE053 => Sym::Clef(ClefSym::G8va),
        0xE05C | 0xE07B => Sym::Clef(ClefSym::C),
        0xE062 | 0xE07C => Sym::Clef(ClefSym::F),
        0xE064 => Sym::Clef(ClefSym::F8vb),
        0xE069 | 0xE06A => Sym::Clef(ClefSym::Percussion),
        0xE080..=0xE089 => Sym::TimeDigit((u - 0xE080) as u8),
        0xE08A => Sym::TimeCommon,
        0xE08B => Sym::TimeCut,
        0xE0A0 | 0xE0A1 => Sym::Head(Breve),
        0xE0A2 => Sym::Head(Whole),
        0xE0A3 => Sym::Head(Half),
        0xE0A4 => Sym::Head(Black),
        0xE0A5..=0xE0FF => Sym::Head(Other),
        0xE040..=0xE043 => Sym::RepeatDots,
        0xE1E7 => Sym::Dot,
        0xE240 | 0xE241 => Sym::Flag(1),
        0xE242 | 0xE243 => Sym::Flag(2),
        0xE244 | 0xE245 => Sym::Flag(3),
        0xE246 | 0xE247 => Sym::Flag(4),
        0xE248 | 0xE249 => Sym::Flag(5),
        0xE24A | 0xE24B => Sym::Flag(6),
        0xE260 => Sym::Accidental(-1),
        0xE261 => Sym::Accidental(0),
        0xE262 => Sym::Accidental(1),
        0xE263 => Sym::Accidental(2),
        0xE264 => Sym::Accidental(-2),
        0xE4E1 => Sym::Rest(NoteValue::Long),
        0xE4E2 => Sym::Rest(NoteValue::Breve),
        0xE4E3 => Sym::Rest(NoteValue::Whole),
        0xE4E4 => Sym::Rest(NoteValue::Half),
        0xE4E5 => Sym::Rest(NoteValue::Quarter),
        0xE4E6 => Sym::Rest(NoteValue::Eighth),
        0xE4E7 => Sym::Rest(NoteValue::N16th),
        0xE4E8 => Sym::Rest(NoteValue::N32nd),
        0xE4E9 => Sym::Rest(NoteValue::N64th),
        0xE4EA => Sym::Rest(NoteValue::N128th),
        0xE4EB => Sym::Rest(NoteValue::N256th),
        0xE880..=0xE889 => Sym::Tuplet((u - 0xE880) as u8),
        0xE520 => Sym::Dynamic("p"),
        0xE521 => Sym::Dynamic("m"),
        0xE522 => Sym::Dynamic("f"),
        0xE523 => Sym::Dynamic("r"),
        0xE524 => Sym::Dynamic("s"),
        0xE525 => Sym::Dynamic("z"),
        0xE526 => Sym::Dynamic("n"),
        0xE527 => Sym::Dynamic("pppppp"),
        0xE528 => Sym::Dynamic("ppppp"),
        0xE529 => Sym::Dynamic("pppp"),
        0xE52A => Sym::Dynamic("ppp"),
        0xE52B => Sym::Dynamic("pp"),
        0xE52C => Sym::Dynamic("mp"),
        0xE52D => Sym::Dynamic("mf"),
        0xE52E => Sym::Dynamic("pf"),
        0xE52F => Sym::Dynamic("ff"),
        0xE530 => Sym::Dynamic("fff"),
        0xE531 => Sym::Dynamic("ffff"),
        0xE532 => Sym::Dynamic("fffff"),
        0xE533 => Sym::Dynamic("ffffff"),
        0xE534 => Sym::Dynamic("fp"),
        0xE535 => Sym::Dynamic("fz"),
        0xE536 => Sym::Dynamic("sf"),
        0xE537 => Sym::Dynamic("sfp"),
        0xE538 => Sym::Dynamic("sfpp"),
        0xE539 => Sym::Dynamic("sfz"),
        0xE53A => Sym::Dynamic("sfzp"),
        0xE53B => Sym::Dynamic("sffz"),
        0xE53C => Sym::Dynamic("rf"),
        0xE53D => Sym::Dynamic("rfz"),
        _ => articulation(c).map_or(Sym::Other, Sym::Articulation),
    }
}

/// The catalogue mark drawn with this SMuFL glyph (above or below).
fn articulation(c: char) -> Option<Articulation> {
    scoreart_model::marks::MARKS.iter().find_map(|m| {
        let a = Articulation::from_id(m.id)?;
        let d = a.def();
        let hit = [d.glyph_above, d.glyph_below]
            .iter()
            .any(|name| scoreart_engrave::smufl::get(name).is_some_and(|g| g.0 == c));
        hit.then_some(a)
    })
}

/// The Sonata keyboard layout (characters as the PDF's ToUnicode gives them).
fn sonata(c: char) -> Sym {
    use HeadSym::*;
    let art = |id: &str| Articulation::from_id(id).map_or(Sym::Other, Sym::Articulation);
    match c {
        '&' => Sym::Clef(ClefSym::G),
        'V' => Sym::Clef(ClefSym::G8vb),
        '?' => Sym::Clef(ClefSym::F),
        'B' => Sym::Clef(ClefSym::C),
        '/' => Sym::Clef(ClefSym::Percussion),
        '0'..='9' => Sym::TimeDigit(c as u8 - b'0'),
        'c' => Sym::TimeCommon,
        'C' => Sym::TimeCut,
        'œ' => Sym::Head(Black),
        '˙' => Sym::Head(Half),
        'w' => Sym::Head(Whole),
        'W' => Sym::Head(Breve),
        'Ï' | 'Ò' | '§' => Sym::Head(Other),
        'b' => Sym::Accidental(-1),
        'n' => Sym::Accidental(0),
        '#' => Sym::Accidental(1),
        'Ü' => Sym::Accidental(2),
        'º' => Sym::Accidental(-2),
        '.' => Sym::Dot,
        'Œ' => Sym::Rest(NoteValue::Quarter),
        '‰' => Sym::Rest(NoteValue::Eighth),
        '≈' => Sym::Rest(NoteValue::N16th),
        '®' => Sym::Rest(NoteValue::N32nd),
        'Ù' => Sym::Rest(NoteValue::N64th),
        '∑' => Sym::Rest(NoteValue::Whole),
        'Ó' | '∫' => Sym::Rest(NoteValue::Half),
        'j' | 'J' => Sym::Flag(1),
        'r' | 'R' => Sym::Flag(2),
        'Â' | 'Ê' => Sym::Flag(3),
        'p' => Sym::Dynamic("p"),
        'm' => Sym::Dynamic("m"),
        'f' => Sym::Dynamic("f"),
        's' => Sym::Dynamic("s"),
        'z' => Sym::Dynamic("z"),
        'π' => Sym::Dynamic("pp"),
        'ƒ' => Sym::Dynamic("ff"),
        'P' => Sym::Dynamic("mp"),
        'F' => Sym::Dynamic("mf"),
        '>' => art("accent"),
        '^' | 'v' => art("marcato"),
        '-' => art("tenuto"),
        'U' | 'u' => art("fermata"),
        '{' => Sym::Brace,
        _ => Sym::Other,
    }
}
