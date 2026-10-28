//! Every text the interface shows has a German translation.

use std::collections::BTreeSet;

use crate::i18n_de::DE;

/// Source files whose `t("…")` / `tf("…")` texts are collected.
const SOURCES: &[&str] = &[
    include_str!("panels.rs"),
    include_str!("instruments.rs"),
    include_str!("midi.rs"),
    include_str!("mixer.rs"),
    include_str!("page.rs"),
    include_str!("catalog.rs"),
    include_str!("lib.rs"),
    include_str!("canvas.rs"),
    include_str!("playback.rs"),
    include_str!("side.rs"),
    include_str!("ribbon.rs"),
];

/// Messages the engine and model put in the status line (exact texts and templates).
const ENGINE_MESSAGES: &[&str] = &[
    "A score needs at least one instrument",
    "A score needs at least one measure",
    "Add an instrument first",
    "Note input is off (N)",
    "Nothing to redo",
    "Nothing to undo",
    "Select a note first",
    "Select a note or rest first",
    "Select something first",
    "Too many instruments",
    "no selection",
    "Opened {}",
    "Opened {} (MuseScore) — Save writes MusicXML",
    "Opened {} (MIDI) — Save writes MusicXML",
    "Note {} · {} · Measure {} · Beat {} · {} · Voice {}",
    "Rest · {} · Measure {} · Beat {} · {} · Voice {}",
    "Measure {}",
    "Measures {}–{}",
    "bad parameter: {}",
    "unknown command: {}",
    "file: {}",
    "no such {}",
    "invalid: {}",
    "limit reached: {}",
    "Staff {}",
    "Staff {}, voice {}",
    "Key {} ({})",
    "Controller {}",
    "Controller {} (modulation)",
    "Controller {} (volume)",
    "Controller {} (pan)",
    "Controller {} (expression)",
    "Controller {} (sustain pedal)",
    "Controller {} (sostenuto pedal)",
    "Controller {} (soft pedal)",
];

/// The texts inside `t("…")` and `tf("…"` calls of a source file.
fn keys_in(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let call = if src.get(i..).is_some_and(|r| r.starts_with("t(")) {
            Some(2)
        } else if src.get(i..).is_some_and(|r| r.starts_with("tf(")) {
            Some(3)
        } else {
            None
        };
        let word_before = i > 0
            && b.get(i - 1)
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'.');
        if let (Some(n), false) = (call, word_before) {
            let mut j = i + n;
            while b.get(j).is_some_and(|c| c.is_ascii_whitespace()) {
                j += 1;
            }
            if b.get(j) == Some(&b'"') {
                let mut s = String::new();
                let mut k = j + 1;
                let mut chars = src.get(k..).unwrap_or("").chars();
                while let Some(c) = chars.next() {
                    k += c.len_utf8();
                    match c {
                        '"' => break,
                        '\\' => {
                            if let Some(e) = chars.next() {
                                k += e.len_utf8();
                                s.push(match e {
                                    'n' => '\n',
                                    other => other,
                                });
                            }
                        }
                        c => s.push(c),
                    }
                }
                out.push(s);
                i = k;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Everything that must have a German text.
pub fn required_keys() -> BTreeSet<String> {
    let mut keys: BTreeSet<String> = SOURCES.iter().flat_map(|s| keys_in(s)).collect();
    let mut add = |s: &str| {
        keys.insert(s.to_string());
    };
    for c in scoreart_engine::registry() {
        add(c.label);
    }
    for m in scoreart_model::marks::MARKS {
        add(m.label);
        add(m.group);
    }
    for k in scoreart_model::SpannerKind::ALL {
        add(k.label());
    }
    for c in scoreart_model::Clef::ALL {
        add(c.label());
    }
    for h in scoreart_model::NoteHead::ALL {
        add(h.id());
    }
    for j in scoreart_model::Jump::ALL {
        add(j.id());
    }
    for i in scoreart_model::INSTRUMENTS {
        add(i.name);
    }
    for a in scoreart_engine::midi_input::ACTIONS {
        add(a.label);
    }
    for v in scoreart_model::NoteValue::ALL {
        add(v.xml_name());
    }
    for l in scoreart_synth::catalog::libraries() {
        add(l.description);
        add(l.size);
        for s in l
            .licence
            .allows
            .iter()
            .chain(l.licence.requires)
            .chain(l.licence.notes)
        {
            add(s);
        }
    }
    for m in ENGINE_MESSAGES {
        add(m);
    }
    keys
}

#[test]
fn every_text_has_a_german_translation() {
    let de: std::collections::HashMap<&str, &str> = DE.iter().copied().collect();
    let missing: Vec<String> = required_keys()
        .into_iter()
        .filter(|k| !de.contains_key(k.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "{} texts without German translation:\n{}",
        missing.len(),
        missing.join("\n")
    );
    // Placeholders match.
    for (en, de) in DE {
        assert_eq!(en.matches("{}").count(), de.matches("{}").count(), "{en} → {de}");
        assert!(!de.trim().is_empty(), "{en}");
    }
    let mut seen = std::collections::HashSet::new();
    for (en, _) in DE {
        assert!(seen.insert(*en), "twice: {en}");
    }
}

/// Writes the keys to `target/i18n-keys.txt` (run with `--ignored` when adding texts).
#[test]
#[ignore]
fn dump_keys() {
    let de: std::collections::HashMap<&str, &str> = DE.iter().copied().collect();
    let text: String = required_keys()
        .into_iter()
        .filter(|k| !de.contains_key(k.as_str()))
        .map(|k| format!("{k:?}\n"))
        .collect();
    let _ = std::fs::write(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/i18n-keys.txt"),
        text,
    );
}
