//! Real samples for what the notation asks: a library like VSCO 2 has, next to an
//! instrument's sustained sound, its staccato, pizzicato, tremolo or muted sound as separate
//! instruments ("ViolinEnsSusVib", "ViolinEnsSpic", "ViolinEnsPizz", "ViolinEnsTrem"). The
//! names are split into the instrument ("ViolinEns") and its technique words, so the matching
//! sound can be found by name.
//!
//! SoundFonts and the built-in sounds use the General MIDI programs for the same (pizzicato
//! and tremolo strings, muted trumpet).

use scoreart_midi::{Family, Tech, Technique};

/// The kind of sound a note wants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Variant {
    /// The part's own sound.
    Own,
    Short,
    Pizzicato,
    Tremolo,
    Muted,
}

impl Variant {
    pub fn of(tech: Tech) -> Variant {
        match tech.technique {
            Technique::Pizzicato => Variant::Pizzicato,
            Technique::Tremolo => Variant::Tremolo,
            Technique::Muted => Variant::Muted,
            Technique::Normal if tech.short => Variant::Short,
            Technique::Normal => Variant::Own,
        }
    }
}

/// The class of a preset by its technique words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Long,
    Short,
    Pizzicato,
    Tremolo,
    Muted,
}

/// Splits "ViolinEnsSusVib-Quiet" into ["Violin", "Ens", "Sus", "Vib", "Quiet"].
fn tokens(name: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for c in name.chars() {
        if c == '-' || c == '_' || c == ' ' || c == '.' || c == '/' {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_lower = false;
            continue;
        }
        if c.is_uppercase() && prev_lower && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        prev_lower = c.is_lowercase() || c.is_ascii_digit();
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// What a technique word says about a sound (None: part of the instrument's name).
fn word_class(w: &str) -> Option<Option<Class>> {
    let w = w.to_lowercase();
    Some(match w.as_str() {
        "pizz" | "pizzicato" => Some(Class::Pizzicato),
        "trem" | "tremolo" => Some(Class::Tremolo),
        "mute" | "muted" | "sord" | "sordino" | "straight" | "harmon" | "cup" => Some(Class::Muted),
        "stac" | "stacc" | "staccato" | "spic" | "spicc" | "spiccato" | "short" | "marc" => {
            Some(Class::Short)
        }
        "sus" | "vib" | "nv" | "vb" | "exp" | "arco" | "long" | "leg" | "legato" | "sustain" => {
            Some(Class::Long)
        }
        // Neutral words that come after the instrument's name.
        "quiet" | "loud" | "ks" | "soft" | "hard" => None,
        _ => return None,
    })
}

/// The instrument part of a name and its class (an unnamed technique counts as long).
fn split(name: &str) -> (String, Class) {
    let t = tokens(name);
    let first = t.iter().position(|w| word_class(w).is_some()).unwrap_or(t.len());
    let stem: String = t.get(..first).map(|s| s.concat()).unwrap_or_default();
    let class = t
        .iter()
        .skip(first)
        .filter_map(|w| word_class(w).flatten())
        .find(|c| *c != Class::Long)
        .unwrap_or(Class::Long);
    (stem.to_lowercase(), class)
}

/// The best preset of `presets` for variant `v` of the part's preset `own`: same instrument,
/// the wanted technique. None when the library has none (or the own sound already is it).
pub fn preset_for(presets: &[String], own: &str, v: Variant) -> Option<String> {
    let want = match v {
        Variant::Own => return None,
        Variant::Short => Class::Short,
        Variant::Pizzicato => Class::Pizzicato,
        Variant::Tremolo => Class::Tremolo,
        Variant::Muted => Class::Muted,
    };
    let (stem, own_class) = split(own);
    if stem.is_empty() || own_class == want {
        return None;
    }
    // A short variant only replaces a sustained sound the user chose, never pizzicato etc.
    if want == Class::Short && own_class != Class::Long {
        return None;
    }
    let score = |name: &str| -> i32 {
        let t = tokens(name);
        let mut s = 0;
        // Prefer plain over "-Quiet" and keyswitch instruments, and vibrato over non-vibrato.
        if t.iter().any(|w| w.eq_ignore_ascii_case("quiet")) {
            s -= 2;
        }
        if t.iter().any(|w| w.eq_ignore_ascii_case("ks")) {
            s -= 3;
        }
        if t.iter().any(|w| w.eq_ignore_ascii_case("vib")) {
            s += 1;
        }
        // "Straight" mute over others.
        if t.iter().any(|w| w.eq_ignore_ascii_case("straight")) {
            s += 1;
        }
        s
    };
    presets
        .iter()
        .filter(|p| p.as_str() != own)
        .filter(|p| {
            let (s, c) = split(p);
            s == stem && c == want
        })
        .max_by_key(|p| score(p))
        .cloned()
}

/// The General MIDI program for a variant of `program`, if General MIDI has one.
pub fn gm_program(program: u8, v: Variant) -> Option<u8> {
    let bowed = Family::of_program(program) == Family::Bowed;
    match v {
        Variant::Pizzicato if bowed => Some(45),
        Variant::Tremolo if bowed => Some(44),
        Variant::Muted if (56..=57).contains(&program) => Some(59),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vsco() -> Vec<String> {
        [
            "ViolinEnsSusVib",
            "ViolinEnsSusVib-Quiet",
            "ViolinEnsSpic",
            "ViolinEnsPizz",
            "ViolinEnsTrem",
            "ViolinEns-KS",
            "ViolaEnsSusVib",
            "ViolaEnsPizz",
            "TrumpetSus",
            "TrumpetStraightMuteSus",
            "TrumpetHarmonMuteSus",
            "FHornSus",
            "FHornMute",
            "SViolinVib",
            "SViolinPizz",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    #[test]
    fn finds_the_matching_sound_by_name() {
        let p = vsco();
        assert_eq!(
            preset_for(&p, "ViolinEnsSusVib", Variant::Pizzicato).as_deref(),
            Some("ViolinEnsPizz")
        );
        assert_eq!(
            preset_for(&p, "ViolinEnsSusVib", Variant::Short).as_deref(),
            Some("ViolinEnsSpic")
        );
        assert_eq!(
            preset_for(&p, "ViolinEnsSusVib", Variant::Tremolo).as_deref(),
            Some("ViolinEnsTrem")
        );
        assert_eq!(
            preset_for(&p, "ViolaEnsSusVib", Variant::Pizzicato).as_deref(),
            Some("ViolaEnsPizz")
        );
        assert_eq!(
            preset_for(&p, "ViolaEnsSusVib", Variant::Tremolo),
            None,
            "no viola tremolo here"
        );
        assert_eq!(
            preset_for(&p, "TrumpetSus", Variant::Muted).as_deref(),
            Some("TrumpetStraightMuteSus")
        );
        assert_eq!(
            preset_for(&p, "FHornSus", Variant::Muted).as_deref(),
            Some("FHornMute")
        );
        assert_eq!(
            preset_for(&p, "SViolinVib", Variant::Pizzicato).as_deref(),
            Some("SViolinPizz")
        );
        // The user's own choice stays: a spiccato part is not made "more short", a pizzicato
        // part does not turn into spiccato.
        assert_eq!(preset_for(&p, "ViolinEnsSpic", Variant::Short), None);
        assert_eq!(preset_for(&p, "ViolinEnsPizz", Variant::Short), None);
        assert_eq!(preset_for(&p, "ViolinEnsSusVib", Variant::Own), None);
    }

    #[test]
    fn general_midi_variants() {
        assert_eq!(gm_program(40, Variant::Pizzicato), Some(45));
        assert_eq!(gm_program(48, Variant::Tremolo), Some(44));
        assert_eq!(gm_program(56, Variant::Muted), Some(59));
        assert_eq!(gm_program(0, Variant::Pizzicato), None);
    }
}
