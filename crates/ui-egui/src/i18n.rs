//! Interface languages. English texts are the keys; [`crate::i18n_de::DE`] holds their German
//! translations. `t` translates a fixed text, `tf` a text with `{}` placeholders, and `tr` a
//! message built elsewhere (engine status lines) by matching it against the known templates.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    De,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::De];

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::De => "de",
        }
    }

    /// The language's own name.
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::De => "Deutsch",
        }
    }

    /// The language for a locale or code like "de-AT" or "de_DE.UTF-8"; English otherwise.
    pub fn from_code(code: &str) -> Lang {
        if code.trim().to_ascii_lowercase().starts_with("de") {
            Lang::De
        } else {
            Lang::En
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(l: Lang) {
    CURRENT.store(
        match l {
            Lang::En => 0,
            Lang::De => 1,
        },
        Ordering::Relaxed,
    );
}

pub fn lang() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::De,
        _ => Lang::En,
    }
}

fn german() -> &'static HashMap<&'static str, &'static str> {
    static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| crate::i18n_de::DE.iter().copied().collect())
}

/// Translates a fixed text (unknown texts stay as they are).
pub fn t(s: &'static str) -> &'static str {
    match lang() {
        Lang::En => s,
        Lang::De => german().get(s).copied().unwrap_or(s),
    }
}

/// Translates a text with `{}` placeholders and fills them in order.
pub fn tf(template: &'static str, args: &[&dyn std::fmt::Display]) -> String {
    fill(t(template), args.iter().map(|a| a.to_string()))
}

fn fill(template: &str, args: impl Iterator<Item = String>) -> String {
    let mut out = String::new();
    let mut parts = template.split("{}");
    let mut args = args;
    if let Some(first) = parts.next() {
        out.push_str(first);
    }
    for p in parts {
        out.push_str(&args.next().unwrap_or_default());
        out.push_str(p);
    }
    out
}

/// A note or key name ("Bb", "F#4", "C") in the interface language: German uses H for B and
/// B for B flat, and -is / -es endings (Fis, Es, As).
pub fn note_name(en: &str) -> String {
    if lang() == Lang::En {
        return en.to_string();
    }
    let mut chars = en.chars();
    let Some(step) = chars.next().filter(|c| ('A'..='G').contains(c)) else {
        return en.to_string();
    };
    let rest: String = chars.collect();
    let acc_len = rest.chars().take_while(|c| *c == '#' || *c == 'b').count();
    let (acc, octave) = rest.split_at(acc_len);
    let name = match (step, acc) {
        ('B', "") => "H".to_string(),
        ('B', "b") => "B".to_string(),
        ('B', "bb") => "Heses".to_string(),
        ('E', "b") => "Es".to_string(),
        ('E', "bb") => "Eses".to_string(),
        ('A', "b") => "As".to_string(),
        ('A', "bb") => "Asas".to_string(),
        (s, "b") => format!("{s}es"),
        (s, "bb") => format!("{s}eses"),
        ('B', "#") => "His".to_string(),
        ('B', "##") => "Hisis".to_string(),
        (s, "#") => format!("{s}is"),
        (s, "##") => format!("{s}isis"),
        (s, _) => s.to_string(),
    };
    format!("{name}{octave}")
}

/// Whether `s` is one or more note names like "C4 E4" or "Bb".
fn is_note_names(s: &str) -> bool {
    !s.is_empty()
        && s.split(' ').all(|w| {
            let mut c = w.chars();
            c.next().is_some_and(|f| ('A'..='G').contains(&f))
                && c.all(|x| x == '#' || x == 'b' || x == '-' || x.is_ascii_digit())
        })
}

/// A keyboard shortcut ("Ctrl+Shift+S") with German key names.
pub fn keys(s: &str) -> String {
    if lang() == Lang::En {
        return s.to_string();
    }
    s.replace("Ctrl", "Strg")
        .replace("Shift", "Umschalt")
        .replace("Space", "Leertaste")
        .replace("Del", "Entf")
        .replace("Ins", "Einfg")
}

/// "Staff 2" / "Staff 1, voice 2" of a part's line.
pub fn line_label(part: &scoreart_model::Part, line: scoreart_model::Line) -> String {
    tr(&part.line_label(line))
}

/// Translates a message made elsewhere: an exact text, or one matching a template with `{}`
/// placeholders (whose values are translated too when they are known words).
pub fn tr(s: &str) -> String {
    if lang() == Lang::En || s.is_empty() {
        return s.to_string();
    }
    if let Some(d) = german().get(s) {
        return (*d).to_string();
    }
    // The matching template with the most fixed text wins ("Controller {} (volume)" over
    // "Controller {}").
    let best = crate::i18n_de::DE
        .iter()
        .filter(|(en, _)| en.contains("{}"))
        .filter_map(|(en, de)| capture(en, s).map(|c| (en.len() - 2 * c.len(), de, c)))
        .max_by_key(|(fixed, ..)| *fixed);
    if let Some((_, de, caps)) = best {
        {
            return fill(
                de,
                caps.into_iter().map(|c| {
                    if is_note_names(&c) {
                        c.split(' ').map(note_name).collect::<Vec<_>>().join(" ")
                    } else if let Some(base) = c.strip_suffix('.') {
                        // A dotted value: "quarter." → "Viertel."
                        let dots = c.len() - base.trim_end_matches('.').len();
                        format!("{}{}", tr(base.trim_end_matches('.')), ".".repeat(dots))
                    } else {
                        tr(&c)
                    }
                }),
            );
        }
    }
    s.to_string()
}

/// The values of a template's `{}` placeholders in `s`, if `s` has the template's form.
fn capture(template: &str, s: &str) -> Option<Vec<String>> {
    let pieces: Vec<&str> = template.split("{}").collect();
    let first = pieces.first()?;
    let mut rest = s.strip_prefix(first)?;
    let mut caps = Vec::new();
    for (i, lit) in pieces.iter().enumerate().skip(1) {
        let last = i + 1 == pieces.len();
        if last {
            if lit.is_empty() {
                caps.push(rest.to_string());
                rest = "";
            } else {
                let v = rest.strip_suffix(lit)?;
                caps.push(v.to_string());
                rest = "";
            }
        } else {
            if lit.is_empty() {
                return None;
            }
            let at = rest.find(lit)?;
            caps.push(rest.get(..at)?.to_string());
            rest = rest.get(at + lit.len()..)?;
        }
    }
    if rest.is_empty() && caps.iter().all(|c| !c.is_empty()) {
        Some(caps)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_translate_both_ways() {
        set_lang(Lang::De);
        assert_eq!(
            t("Save as…    Ctrl+Shift+S"),
            "Speichern unter…    Strg+Umschalt+S"
        );
        assert_eq!(tf("Voice {} (Ctrl+Alt+{})", &[&2, &2]), "Stimme 2 (Strg+Alt+2)");
        assert_eq!(
            tr("Note C5 · quarter · Measure 1 · Beat 4 · Piano · Voice 1"),
            "Note C5 · Viertel · Takt 1 · Schlag 4 · Klavier · Stimme 1"
        );
        assert_eq!(tr("Measures 3–5"), "Takte 3–5");
        assert_eq!(tr("something new"), "something new");
        set_lang(Lang::En);
        assert_eq!(tr("Measures 3–5"), "Measures 3–5");
        assert_eq!(Lang::from_code("de_DE.UTF-8"), Lang::De);
        assert_eq!(Lang::from_code("fr"), Lang::En);
    }
}
