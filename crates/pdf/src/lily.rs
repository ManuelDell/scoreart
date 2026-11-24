//! LilyPond's PDFs: its music font (Emmentaler, formerly Feta) is embedded as a Type 1 / CFF
//! font without Unicode values, but the PDF's font dictionary names every glyph it uses
//! (`noteheads.s2`, `accidentals.flat`, `clefs.G` …) in the encoding's `/Differences`; older
//! versions keep digits and dynamic letters in a font of their own and braces in another. The
//! names and what they stand for are documented publicly in the LilyPond manual's list of
//! Emmentaler glyphs; no LilyPond code or font is used. A drawn glyph only tells its character
//! code and advance width, so each glyph is found again by that pair — which is unique across
//! the fonts of the files observed; a pair two fonts share is dropped.

use std::collections::HashMap;

use hayro::hayro_syntax::object::{Array, Dict, Name, Object};

use crate::PdfDoc;

/// Character code and advance width (tenths of a font unit) → the SMuFL code point the glyph
/// stands for (`None`: the pair is ambiguous, or a glyph of no interest).
#[derive(Default)]
pub(crate) struct LilyGlyphs(HashMap<(u32, i32), Option<char>>);

impl LilyGlyphs {
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The SMuFL character of a drawn glyph of a LilyPond music font.
    pub(crate) fn get(&self, code: u32, width: Option<f32>) -> Option<char> {
        let w = width.filter(|w| w.is_finite())?;
        self.0.get(&(code, (w * 10.0).round() as i32)).copied().flatten()
    }
}

/// Most fonts looked at (hostile files).
const MAX_FONTS: usize = 2000;

/// Reads the glyph names of the LilyPond music fonts of every page.
pub(crate) fn glyphs(doc: &PdfDoc) -> LilyGlyphs {
    let mut map: HashMap<(u32, i32), Option<char>> = HashMap::new();
    // Only files with a LilyPond music font are read this way.
    let mut music = false;
    let mut seen = std::collections::HashSet::new();
    for page in doc.pdf().pages().iter().take(crate::MAX_PAGES) {
        let fonts = &page.resources().fonts;
        for key in fonts.keys().collect::<Vec<Name<'_>>>() {
            if seen.len() >= MAX_FONTS {
                break;
            }
            let Some(font) = page.resources().get_font(&key) else {
                continue;
            };
            let id = font
                .obj_id()
                .map(|i| format!("{i:?}"))
                .unwrap_or_else(|| key.as_str().to_string());
            if !seen.insert(id) {
                continue;
            }
            let base = font
                .get::<Name<'_>>("BaseFont")
                .map(|n| n.as_str().to_string())
                .unwrap_or_default();
            let lower = base.to_ascii_lowercase();
            // Older versions draw braces with a font of their own, Aybabtu.
            let is_music =
                lower.contains("emmentaler") || lower.contains("feta") || lower.contains("aybabtu");
            music |= is_music;
            let mut named = differences(&font);
            // Older LilyPond keeps the digits and dynamic letters in a font of their own
            // ("feta-alphabet") with a plain encoding: the code is the character.
            if named.is_empty() && is_music {
                named = plain(&font);
            }
            for (code, name, width) in named {
                let sym = if is_music {
                    smufl(&name, lower.contains("brace") || lower.contains("aybabtu"))
                } else {
                    None
                };
                let k = (code, (width * 10.0).round() as i32);
                match map.get(&k) {
                    Some(old) if *old != sym => {
                        map.insert(k, None);
                    }
                    Some(_) => {}
                    None => {
                        map.insert(k, sym);
                    }
                }
            }
        }
    }
    if !music {
        map.clear();
    }
    LilyGlyphs(map)
}

/// (character code, glyph name, advance width) of every glyph a font's encoding names.
fn differences(font: &Dict<'_>) -> Vec<(u32, String, f32)> {
    let Some(enc) = font.get::<Dict<'_>>("Encoding") else {
        return Vec::new();
    };
    let Some(diffs) = enc.get::<Array<'_>>("Differences") else {
        return Vec::new();
    };
    let first = font.get::<f32>("FirstChar").unwrap_or(0.0).max(0.0) as u32;
    let widths: Vec<f32> = font
        .get::<Array<'_>>("Widths")
        .map(|a| a.iter::<f32>().take(4096).collect())
        .unwrap_or_default();
    let mut out = Vec::new();
    let mut code: u32 = 0;
    for o in diffs.iter::<Object<'_>>().take(4096) {
        match o {
            Object::Number(n) => code = n.as_f64().clamp(0.0, 65535.0) as u32,
            Object::Name(n) => {
                if let Some(w) = code.checked_sub(first).and_then(|i| widths.get(i as usize)) {
                    out.push((code, n.as_str().to_string(), *w));
                }
                code = code.saturating_add(1);
            }
            _ => {}
        }
    }
    out
}

/// (code, the character's glyph name, width) of a font with a plain (ASCII) encoding.
fn plain(font: &Dict<'_>) -> Vec<(u32, String, f32)> {
    let first = font.get::<f32>("FirstChar").unwrap_or(0.0).max(0.0) as u32;
    let widths: Vec<f32> = font
        .get::<Array<'_>>("Widths")
        .map(|a| a.iter::<f32>().take(256).collect())
        .unwrap_or_default();
    const DIGITS: [&str; 10] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
    ];
    let mut out = Vec::new();
    for (i, w) in widths.iter().enumerate() {
        let code = first.saturating_add(i as u32);
        let Some(c) = char::from_u32(code) else { continue };
        let name = match c {
            '0'..='9' => DIGITS
                .get((c as u32 - '0' as u32) as usize)
                .map(|d| d.to_string()),
            'p' | 'm' | 'f' | 'r' | 's' | 'z' | 'n' => Some(c.to_string()),
            _ => None,
        };
        if let Some(name) = name {
            out.push((code, name, *w));
        }
    }
    out
}

/// What an Emmentaler glyph name stands for, as a SMuFL code point.
fn smufl(name: &str, brace_font: bool) -> Option<char> {
    if brace_font || name.starts_with("brace") {
        return char::from_u32(0xE000);
    }
    let u: u32 = match name {
        // Noteheads: s0 whole, s1 half, s2 black, sM1 breve; shaped heads by their duration.
        "noteheads.sM1" | "noteheads.sM1double" => 0xE0A0,
        "noteheads.s0" => 0xE0A2,
        "noteheads.s1" => 0xE0A3,
        "noteheads.s2" => 0xE0A4,
        n if n.starts_with("noteheads.s0") => 0xE0A2,
        n if n.starts_with("noteheads.s1") => 0xE0A3,
        n if n.starts_with("noteheads.s2") => 0xE0A9,
        "rests.M3" => 0xE4E1,
        "rests.M1" | "rests.M1o" => 0xE4E2,
        "rests.0" | "rests.0o" => 0xE4E3,
        "rests.1" | "rests.1o" => 0xE4E4,
        "rests.2" | "rests.2classical" => 0xE4E5,
        "rests.3" => 0xE4E6,
        "rests.4" => 0xE4E7,
        "rests.5" => 0xE4E8,
        "rests.6" => 0xE4E9,
        "rests.7" => 0xE4EA,
        // Older names count quarter tones: 2 sharp, 0 natural, M2 flat.
        "accidentals.flat" | "accidentals.M2" | "accidentals.-1" => 0xE260,
        "accidentals.natural" | "accidentals.0" => 0xE261,
        "accidentals.sharp" | "accidentals.2" => 0xE262,
        "accidentals.doublesharp" | "accidentals.4" => 0xE263,
        "accidentals.flatflat" | "accidentals.M4" | "accidentals.-2" => 0xE264,
        "clefs.G" | "clefs.G_change" => 0xE050,
        "clefs.F" | "clefs.F_change" => 0xE062,
        "clefs.C" | "clefs.C_change" => 0xE05C,
        "clefs.percussion" | "clefs.percussion_change" => 0xE069,
        "dots.dot" => 0xE1E7,
        "flags.u3" => 0xE240,
        "flags.d3" => 0xE241,
        "flags.u4" => 0xE242,
        "flags.d4" => 0xE243,
        "flags.u5" => 0xE244,
        "flags.d5" => 0xE245,
        "flags.u6" => 0xE246,
        "flags.d6" => 0xE247,
        "flags.u7" => 0xE248,
        "flags.d7" => 0xE249,
        "timesig.C44" => 0xE08A,
        "timesig.C22" => 0xE08B,
        "zero" => 0xE080,
        "one" => 0xE081,
        "two" => 0xE082,
        "three" => 0xE083,
        "four" => 0xE084,
        "five" => 0xE085,
        "six" => 0xE086,
        "seven" => 0xE087,
        "eight" => 0xE088,
        "nine" => 0xE089,
        // Dynamic letters.
        "p" => 0xE520,
        "m" => 0xE521,
        "f" => 0xE522,
        "r" => 0xE523,
        "s" => 0xE524,
        "z" => 0xE525,
        "n" => 0xE526,
        // Articulations and ornaments.
        "scripts.sforzato" => 0xE4A0,
        "scripts.staccato" => 0xE4A2,
        "scripts.tenuto" => 0xE4A4,
        "scripts.ustaccatissimo" => 0xE4A6,
        "scripts.dstaccatissimo" => 0xE4A7,
        "scripts.umarcato" => 0xE4AC,
        "scripts.dmarcato" => 0xE4AD,
        "scripts.ufermata" => 0xE4C0,
        "scripts.dfermata" => 0xE4C1,
        "scripts.trill" => 0xE566,
        "scripts.turn" => 0xE567,
        "scripts.prall" => 0xE56C,
        "scripts.mordent" => 0xE56D,
        "scripts.downbow" => 0xE610,
        "scripts.upbow" => 0xE612,
        _ => return None,
    };
    char::from_u32(u)
}

#[cfg(test)]
mod tests {
    use super::smufl;

    #[test]
    fn names_map_to_smufl() {
        assert_eq!(smufl("noteheads.s2", false), Some('\u{E0A4}'));
        assert_eq!(smufl("noteheads.s1diamond", false), Some('\u{E0A3}'));
        assert_eq!(smufl("accidentals.flat", false), Some('\u{E260}'));
        assert_eq!(smufl("brace205", true), Some('\u{E000}'));
        assert_eq!(smufl("accidentals.leftparen", false), None);
    }
}
