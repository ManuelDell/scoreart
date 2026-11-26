//! Reading MuseScore files (`.mscz`, `.mscx`) of MuseScore 3 and 4.
//!
//! Clean room: written from publicly known facts about the file format (a zip holding one XML
//! `.mscx` file whose elements are named after the notation objects) — never from MuseScore's
//! source code. Elements we don't know are skipped; the reader is lenient by design.

use std::collections::{BTreeMap, HashMap};

use scoreart_model::marks::MARKS;
use scoreart_model::{
    Anchor, Arpeggio, Articulation, Barline, BeatUnit, Clef, Duration, Dynamic, Event, Grace, HAlign, Jump,
    KeySig, MeasureInfo, Meta, Note, NoteHead, NoteValue, Part, PartMeasure, Pitch, Placement, Score,
    Spanner, SpannerKind, StaffMeasure, Step, TPQ, Tempo, TimeSig, TimeSymbol, Tremolo, Tuplet, VAlign,
    Voice, clip, limits,
};

use crate::Error;
use crate::dom::{self, Element};

/// Text of an element and its formatting children (`<b>`, `<i>`, `<font>` …), without symbols.
fn flat_text(e: &Element) -> String {
    let mut s = e.text.clone();
    for c in &e.children {
        if c.name != "sym" {
            s.push_str(&flat_text(c));
        }
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A position as a fraction "a/b" of a whole note → ticks.
fn fraction_ticks(s: &str) -> i64 {
    let (a, b) = match s.trim().split_once('/') {
        Some((a, b)) => (
            a.trim().parse::<i64>().unwrap_or(0),
            b.trim().parse::<i64>().unwrap_or(1),
        ),
        None => (s.trim().parse::<i64>().unwrap_or(0), 1),
    };
    if b == 0 {
        0
    } else {
        a.saturating_mul(i64::from(TPQ) * 4) / b
    }
}

/// A tonal pitch class (line of fifths, C = 14) → step and alteration.
fn tpc_spelling(tpc: i32) -> (Step, i8) {
    let k = tpc - 14;
    let step = Step::from_index((k * 4).rem_euclid(7));
    let alter = (k + 1).div_euclid(7).clamp(-2, 2) as i8;
    (step, alter)
}

fn pitch_of(midi: i32, tpc: Option<i32>) -> Pitch {
    let midi = midi.clamp(0, 127);
    match tpc {
        Some(t) => {
            let (step, alter) = tpc_spelling(t);
            let octave = (midi - step.semitones() - i32::from(alter)).div_euclid(12) - 1;
            Pitch::new(step, alter, octave)
        }
        None => Pitch::from_midi(midi, KeySig::default()),
    }
}

fn duration_type(s: &str) -> Option<NoteValue> {
    NoteValue::from_xml_name(s.trim())
}

fn clef_of(s: &str) -> Clef {
    match s.trim() {
        "F" | "F8vb" | "F_8va" | "F15mb" => Clef::Bass,
        "C3" => Clef::Alto,
        "C4" => Clef::Tenor,
        "G8vb" | "G_8vb" | "G8_vb" => Clef::Treble8vb,
        "PERC" | "PERC2" => Clef::Percussion,
        _ => Clef::Treble,
    }
}

fn mark_by_smufl(name: &str) -> Option<Articulation> {
    let n = name.trim();
    MARKS
        .iter()
        .find(|d| d.glyph_above == n || d.glyph_below == n)
        .and_then(|d| Articulation::from_id(d.id))
}

/// Older and descriptive subtype names some files use.
fn mark_by_legacy(name: &str) -> Option<Articulation> {
    let id = match name.trim() {
        "sforzato" | "accent" => "accent",
        "staccato" => "staccato",
        "staccatissimo" => "staccatissimo",
        "tenuto" => "tenuto",
        "portato" => "portato",
        "marcato" => "marcato",
        "fermata" | "ufermata" | "dfermata" => "fermata",
        "shortfermata" => "fermata-short",
        "longfermata" => "fermata-long",
        "trill" => "trill",
        "prall" => "short-trill",
        "mordent" => "mordent",
        "turn" => "turn",
        "reverseturn" => "inverted-turn",
        "upbow" => "up-bow",
        "downbow" => "down-bow",
        "plusstop" => "stopped",
        "ouvert" => "open",
        _ => return None,
    };
    Articulation::from_id(id)
}

fn head_of(s: &str) -> NoteHead {
    match s.trim() {
        "cross" | "x" | "heavy-cross" => NoteHead::X,
        "diamond" | "diamond-old" => NoteHead::Diamond,
        "slash" => NoteHead::Slash,
        "triangle-up" | "triangle-down" | "triangle" => NoteHead::Triangle,
        "xcircle" | "circle-x" => NoteHead::CircleX,
        "plus" => NoteHead::Cross,
        "circled" | "circled-large" => NoteHead::Circled,
        _ => NoteHead::Normal,
    }
}

/// Chord-symbol text from a `<Harmony>` element (root and bass given as tonal pitch classes).
fn harmony_text(h: &Element) -> Option<String> {
    let spell = |tpc: i32| {
        let (step, alter) = tpc_spelling(tpc);
        let acc = match alter {
            1 => "#",
            -1 => "b",
            2 => "##",
            -2 => "bb",
            _ => "",
        };
        format!("{}{acc}", step.letter())
    };
    // MuseScore 4.6+ wraps root, name and bass in <harmonyInfo>.
    let h = h.child("harmonyInfo").unwrap_or(h);
    let root = h.child_num("root").map(|r| r as i32)?;
    let name = h.child("name").map(flat_text).unwrap_or_default();
    let bass = h
        .child_num("bass")
        .or_else(|| h.child_num("base"))
        .map(|b| format!("/{}", spell(b as i32)))
        .unwrap_or_default();
    Some(clip(&format!("{}{}{}", spell(root), name, bass)))
}

/// The line kind of a `<Spanner>` (voltas, ties and glissandos are read elsewhere).
fn line_kind(c: &Element) -> Option<SpannerKind> {
    match c.attr("type").unwrap_or("") {
        "Slur" => Some(SpannerKind::Slur),
        "HairPin" => {
            let sub = c
                .path(&["HairPin", "subtype"])
                .map(|s| s.text.trim().to_string())
                .unwrap_or_default();
            Some(match sub.as_str() {
                "1" => SpannerKind::Diminuendo,
                "2" => SpannerKind::CrescendoLine,
                "3" => SpannerKind::DiminuendoLine,
                _ => SpannerKind::Crescendo,
            })
        }
        "Ottava" => {
            let sub = c
                .path(&["Ottava", "subtype"])
                .map(|s| s.text.trim().to_string())
                .unwrap_or_default();
            Some(SpannerKind::Ottava(match sub.as_str() {
                "1" | "8vb" => -1,
                "2" | "15ma" => 2,
                "3" | "15mb" => -2,
                "4" | "22ma" => 3,
                "5" | "22mb" => -3,
                _ => 1,
            }))
        }
        "Pedal" => Some(SpannerKind::Pedal),
        "Trill" => Some(SpannerKind::TrillLine),
        "Vibrato" => Some(SpannerKind::Vibrato),
        // Imported scores carry these as plain text lines.
        "TextLine" => {
            let begin = c
                .path(&["TextLine", "beginText"])
                .map(flat_text)
                .unwrap_or_default()
                .to_ascii_lowercase();
            match begin.trim_end_matches('.') {
                "cresc" | "crescendo" => Some(SpannerKind::CrescendoLine),
                "dim" | "decresc" | "diminuendo" | "decrescendo" => Some(SpannerKind::DiminuendoLine),
                "let ring" => Some(SpannerKind::LetRing),
                "p.m" | "pm" | "palm mute" => Some(SpannerKind::PalmMute),
                _ => None,
            }
        }
        "LetRing" => Some(SpannerKind::LetRing),
        "PalmMute" => Some(SpannerKind::PalmMute),
        _ => None,
    }
}

/// A text the user moved in MuseScore: `<offset x y>` (staff spaces) and `<placement>`.
fn placement_of(e: &Element) -> Option<Placement> {
    let below = e.child_text("placement").is_some_and(|p| p.trim() == "below");
    let off = e.child("offset");
    let num = |k: &str| {
        off.and_then(|o| o.attr(k))
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
    };
    let (x, y) = (num("x"), num("y"));
    (below || x != 0.0 || y != 0.0).then(|| Placement::from_sp(x, y, below))
}

/// Where MuseScore put a staff text or dynamic: its `<offset>` is the whole position (dx from
/// the note; dy to the baseline from the top line, or below the staff from the bottom line),
/// written only when it differs from the default; `above` is the default height above the
/// staff for one placed above without an offset (None: ours).
fn ms_place(e: &Element, above: Option<f64>) -> Option<Placement> {
    let placement = e.child_text("placement").map(str::trim);
    let below = placement == Some("below");
    if e.child("offset").is_some() {
        return placement_of(e).map(|p| Placement { below, ..p });
    }
    match (placement, above) {
        (Some("above"), Some(y)) => Some(Placement::from_sp(0.0, y, false)),
        _ => None,
    }
}

/// The note a tempo mark counts in, from its symbols (metNote8thUp, metAugmentationDot …).
fn tempo_unit(e: &Element) -> Option<BeatUnit> {
    let syms: Vec<String> = e
        .child("text")?
        .children_named("sym")
        .map(|x| x.text.trim().to_string())
        .collect();
    let i = syms.iter().position(|n| n.starts_with("metNote"))?;
    let dotted = syms.get(i + 1).is_some_and(|n| n == "metAugmentationDot");
    let value = match syms[i]
        .trim_start_matches("metNote")
        .trim_end_matches("Up")
        .trim_end_matches("Down")
    {
        "8th" => "eighth",
        "Quarter" => "quarter",
        "Half" => "half",
        _ => return None,
    };
    BeatUnit::from_parts(value, dotted)
}

fn jump_of_label(label: &str) -> Option<Jump> {
    Some(match label.trim().to_ascii_lowercase().as_str() {
        "segno" | "varsegno" => Jump::Segno,
        "coda" | "codab" => Jump::Coda,
        "codasquare" => Jump::VariedCoda,
        "varcoda" => Jump::VariedCoda,
        "fine" => Jump::Fine,
        "tocoda" | "tocodasym" => Jump::ToCoda,
        _ => return None,
    })
}

fn jump_of_text(text: &str) -> Option<Jump> {
    let t = text.trim().trim_end_matches('.').to_ascii_lowercase();
    Some(match t.as_str() {
        "d.c" | "da capo" => Jump::DaCapo,
        "d.c. al fine" | "da capo al fine" => Jump::DaCapoAlFine,
        "d.c. al coda" | "da capo al coda" => Jump::DaCapoAlCoda,
        "d.s" | "dal segno" => Jump::DalSegno,
        "d.s. al fine" | "dal segno al fine" => Jump::DalSegnoAlFine,
        "d.s. al coda" | "dal segno al coda" => Jump::DalSegnoAlCoda,
        "fine" => Jump::Fine,
        "to coda" => Jump::ToCoda,
        _ => return None,
    })
}

/// A line found while reading a voice; its end is given relative to its start.
struct OpenLine {
    kind: SpannerKind,
    part: usize,
    staff: usize,
    voice: usize,
    start: (usize, u32),
    /// Offset of the end: (measures, ticks).
    end: (i64, i64),
}

/// Parses the `.mscx` XML of a MuseScore file.
pub fn from_mscx(xml: &str) -> Result<Score, Error> {
    from_mscx_styled(xml, None)
}

/// Reads an `.mscx`; `mss` is the `score_style.mss` of an `.mscz`, which holds the style in
/// MuseScore 4 files.
pub fn from_mscx_styled(xml: &str, mss: Option<&str>) -> Result<Score, Error> {
    let mut root = dom::parse(xml)?;
    if root.name != "museScore" {
        return Err(Error::Unsupported(format!(
            "root element <{}> (expected a MuseScore file)",
            root.name
        )));
    }
    let version = root.attr("version").unwrap_or("4").to_string();
    if version.starts_with('1') {
        return Err(Error::Unsupported(format!(
            "MuseScore {version} files; open and save it in MuseScore 3 or 4 first, or export MusicXML"
        )));
    }
    // MuseScore 2: brought into the layout of MuseScore 3 first.
    if version.starts_with('2') {
        crate::mscz2::upgrade(&mut root);
    }
    let root = root;
    let sc = root
        .child("Score")
        .ok_or_else(|| Error::Invalid("no <Score>".into()))?;
    let mut score = Score::default();

    // Meta data.
    let mut meta = Meta::default();
    for t in sc.children_named("metaTag") {
        let v = clip(t.text.trim());
        match t.attr("name") {
            Some("workTitle") if !v.is_empty() => meta.title = v,
            Some("composer") => meta.composer = v,
            Some("lyricist") | Some("poet") => meta.lyricist = v,
            Some("copyright") => meta.copyright = v,
            Some("subtitle") => meta.subtitle = v,
            Some("creationDate") => meta.created = v,
            _ => {}
        }
    }

    // Parts and their staves: score-level <Staff id> → (part, staff index).
    let mut staff_map: HashMap<String, (usize, usize)> = HashMap::new();
    let mut next_staff_id = 0usize;
    for (pi, p) in sc.children_named("Part").enumerate().take(limits::MAX_PARTS) {
        let inst = p.child("Instrument");
        let name = inst
            .and_then(|i| i.child("longName").map(flat_text))
            .filter(|n| !n.is_empty())
            .or_else(|| p.child("trackName").map(flat_text))
            .unwrap_or_else(|| format!("Part {}", pi + 1));
        let abbreviation = inst
            .and_then(|i| i.child("shortName").map(flat_text))
            .unwrap_or_default();
        let program = inst
            .and_then(|i| i.path(&["Channel", "program"]))
            .and_then(|c| c.attr("value"))
            .and_then(|v| v.parse::<i32>().ok())
            .map(|v| v.clamp(0, 127) as u8)
            .unwrap_or(0);
        // MuseScore 4.6+ leaves out the id: staves are then numbered in order.
        let ids: Vec<String> = p
            .children_named("Staff")
            .map(|s| {
                next_staff_id += 1;
                s.attr("id")
                    .map_or_else(|| next_staff_id.to_string(), str::to_string)
            })
            .collect();
        let mut clefs = Vec::new();
        for (si, id) in ids.iter().enumerate().take(limits::MAX_STAVES) {
            staff_map.insert(id.clone(), (pi, si));
            let default = p
                .children_named("Staff")
                .nth(si)
                .and_then(|s| s.child_text("defaultClef"))
                .map(clef_of)
                .or_else(|| {
                    inst.and_then(|i| {
                        i.children_named("clef").find(|c| {
                            c.attr("staff").and_then(|n| n.parse::<usize>().ok()).unwrap_or(1) == si + 1
                        })
                    })
                    .map(|c| clef_of(&c.text))
                })
                // Without a clef of their own, MuseScore staves use the treble clef (instruments
                // such as the piano name their bass staff's clef explicitly).
                .unwrap_or(Clef::Treble);
            clefs.push(default);
        }
        if clefs.is_empty() {
            clefs.push(Clef::Treble);
        }
        // Hidden instruments (<show>0</show>) and staves (<isStaffVisible>0</isStaffVisible>).
        let part_shown = p.child_text("show").is_none_or(|v| v.trim() != "0");
        let hidden: Vec<bool> = p
            .children_named("Staff")
            .take(clefs.len())
            .map(|st| !part_shown || st.child_text("isStaffVisible").is_some_and(|v| v.trim() == "0"))
            .collect();
        let hidden = if hidden.iter().any(|h| *h) {
            hidden
        } else {
            Vec::new()
        };
        // The chord symbols' channel ("harmony") and its instrument.
        let mut harmony = scoreart_model::HarmonySound::default();
        if let Some(prog) = inst
            .into_iter()
            .flat_map(|i| i.children_named("Channel"))
            .find(|c| c.attr("name") == Some("harmony"))
            .and_then(|c| c.child("program"))
            .and_then(|pr| pr.attr("value"))
            .and_then(|v| v.trim().parse::<u8>().ok())
        {
            harmony.program = prog.min(127);
        }
        // Staves hidden in systems where they are empty, and staves drawn small.
        let per_staff = |f: &dyn Fn(&Element) -> bool| -> Vec<bool> {
            let v: Vec<bool> = p.children_named("Staff").take(clefs.len()).map(f).collect();
            if v.iter().any(|b| *b) { v } else { Vec::new() }
        };
        let hide_empty =
            per_staff(&|st: &Element| st.child_text("hideWhenEmpty").is_some_and(|v| v.trim() == "1"));
        let small = per_staff(&|st: &Element| {
            st.child("StaffType")
                .and_then(|t| t.child_text("small"))
                .is_some_and(|v| v.trim() == "1")
        });
        score.parts.push(Part {
            id: format!("P{}", pi + 1),
            name: clip(&name),
            abbreviation: clip(&abbreviation),
            program,
            clefs,
            measures: Vec::new(),
            sound: Default::default(),
            hidden,
            hide_empty,
            small,
            harmony,
        });
    }
    if score.parts.is_empty() {
        return Err(Error::Invalid("no parts".into()));
    }

    let mut lines: Vec<OpenLine> = Vec::new();
    let mut first_staff = true;
    // Breaks on a frame belong to the measure before it (a collection of songs often ends
    // each song with a section break on the frame between them).
    let mut frame_breaks: Vec<(usize, &Element)> = Vec::new();
    // Vertical frames after a measure: the height of an empty one.
    let mut frames: Vec<(usize, Option<f32>)> = Vec::new();
    for st in sc.children_named("Staff") {
        let Some(&(pi, si)) = st.attr("id").and_then(|id| staff_map.get(id)) else {
            continue;
        };
        let top_staff = first_staff;
        first_staff = false;
        // Frames (only on the first staff): the title frame before the first measure gives the
        // title page; texts in later frames (the title of a further piece, a poem at the end)
        // become headings and text blocks above the measure that follows them.
        let mut measures_before = 0usize;
        let mut title_frame_done = false;
        for c in &st.children {
            if top_staff
                && matches!(c.name.as_str(), "VBox" | "HBox" | "TBox")
                && let Some(m) = measures_before.checked_sub(1)
            {
                frame_breaks.extend(c.children_named("LayoutBreak").map(|b| (m, b)));
                // A frame between measures always ends the line; an empty one keeps its height
                // as blank space (used to push the following music onto the next page).
                if c.name == "VBox" {
                    let empty = c
                        .children_named("Text")
                        .all(|t| flat_text(t.child("text").unwrap_or(t)).trim().is_empty());
                    let h = c
                        .child_text("height")
                        .and_then(|v| v.trim().parse::<f32>().ok())
                        .filter(|h| h.is_finite() && *h > 0.0);
                    frames.push((m, if empty { h } else { None }));
                }
            }
            match c.name.as_str() {
                "Measure" => measures_before += 1,
                "VBox" | "TBox" => {
                    for t in c.children_named("Text") {
                        let v = clip(&flat_text(t.child("text").unwrap_or(t)));
                        if v.trim().is_empty() {
                            continue;
                        }
                        let style = t.child_text("style").unwrap_or("");
                        let title_page = measures_before == 0 && !title_frame_done;
                        let kind = match (title_page, style) {
                            (true, "title" | "Title") => {
                                meta.title = v;
                                continue;
                            }
                            (true, "subtitle" | "Subtitle") => {
                                meta.subtitle = v;
                                continue;
                            }
                            (true, "composer" | "Composer") => {
                                meta.composer = v;
                                continue;
                            }
                            (true, "lyricist" | "poet" | "Lyricist") => {
                                meta.lyricist = v;
                                continue;
                            }
                            (_, "title" | "Title" | "subtitle" | "Subtitle") => {
                                scoreart_model::TextKind::Heading
                            }
                            _ => scoreart_model::TextKind::Text,
                        };
                        if top_staff && score.blocks.len() < limits::MAX_BLOCKS {
                            score.blocks.push(scoreart_model::TextBlock {
                                kind,
                                text: v,
                                measure: measures_before,
                                dx: 0,
                                dy: 0,
                                locked: false,
                            });
                        }
                    }
                    if measures_before == 0 {
                        title_frame_done = true;
                    }
                }
                _ => {}
            }
        }
        let mut clef_now: Option<Clef> = None;
        // A clef later in a measure (MuseScore puts the change before the next measure at the
        // end of the one before): it holds from the next measure on.
        let mut clef_next: Option<Clef> = None;
        for (mi, m_el) in st
            .children_named("Measure")
            .enumerate()
            .take(limits::MAX_MEASURES)
        {
            if score.measures.len() <= mi {
                score.measures.push(MeasureInfo::default());
            }
            if top_staff {
                let info = score
                    .measures
                    .get_mut(mi)
                    .ok_or(Error::Invalid("measure".into()))?;
                // A measure whose real length differs from the time signature (len="3/8"), and
                // one left out of the numbering (<irregular>).
                if let Some(len) = m_el.attr("len") {
                    let t = fraction_ticks(len);
                    if t > 0 {
                        info.actual = u32::try_from(t).ok();
                    }
                }
                if m_el.child_text("irregular").is_some_and(|v| v.trim() == "1") {
                    info.uncounted = true;
                }
                if m_el.has("startRepeat") {
                    info.repeat_start = true;
                }
                // Line, page and section breaks set in MuseScore.
                for b in m_el.children_named("LayoutBreak") {
                    layout_break(info, b);
                }
                if m_el.has("endRepeat") {
                    info.barline = Barline::RepeatEnd;
                }
                for j in m_el.children_named("Jump") {
                    let t = j.child("text").map(flat_text).unwrap_or_default();
                    let jump = jump_of_text(&t).or_else(|| {
                        let to = j.child_text("jumpTo").unwrap_or("");
                        let until = j.child_text("playUntil").unwrap_or("");
                        let cont = j.child_text("continueAt").unwrap_or("");
                        Some(match (to.contains("segno"), until, cont.is_empty()) {
                            (false, "fine", _) => Jump::DaCapoAlFine,
                            (false, _, false) => Jump::DaCapoAlCoda,
                            (false, _, _) => Jump::DaCapo,
                            (true, "fine", _) => Jump::DalSegnoAlFine,
                            (true, _, false) => Jump::DalSegnoAlCoda,
                            (true, _, _) => Jump::DalSegno,
                        })
                    });
                    if let Some(jump) = jump {
                        info.jumps.push(jump);
                        if j.child("offset").is_some() {
                            info.jump_place = placement_of(j);
                        }
                    }
                }
                for mk in m_el.children_named("Marker") {
                    if let Some(jump) = mk.child_text("label").and_then(jump_of_label) {
                        info.jumps.push(jump);
                        // Placed by hand: the offset is where it stands (from MuseScore's
                        // anchor: the start, or for "Fine" and "To Coda" the end, of the
                        // measure, and the top staff line).
                        if mk.child("offset").is_some() {
                            if jump.at_end() {
                                info.jump_place = placement_of(mk);
                            } else {
                                info.marker_place = placement_of(mk);
                            }
                        }
                    }
                }
            }
            let mut pm_staff = StaffMeasure::default();
            if let Some(c) = clef_next.take() {
                pm_staff.clef = Some(c);
            }
            let mut voices_el: Vec<&Element> = m_el.children_named("voice").collect();
            // MuseScore 2-style measures put the content directly in <Measure>.
            if voices_el.is_empty() {
                voices_el.push(m_el);
            }
            for (vi, v_el) in voices_el.into_iter().enumerate().take(limits::MAX_VOICES) {
                let mut voice = Voice::default();
                let mut t: i64 = 0;
                let mut tuplet: Option<Tuplet> = None;
                let mut tuplets_by_id: HashMap<String, Tuplet> = HashMap::new();
                let mut pending_dyn: Option<Dynamic> = None;
                let mut pending_dyn_place: Option<Placement> = None;
                let mut pending_dyn_hidden = false;
                let mut pending_marks: Vec<Articulation> = Vec::new();
                let mut pending_text: Option<String> = None;
                let mut pending_place: Option<Placement> = None;
                let mut pending_text_size: Option<u8> = None;
                let mut pending_chord: Option<String> = None;
                // Pending moves of the voice position by <location>; MuseScore may step back
                // to anchor a line and forward again, so only the net move counts.
                let mut moved: i64 = 0;
                let mut graces: Vec<Grace> = Vec::new();
                let push_event = |voice: &mut Voice, t: &mut i64, e: Event| {
                    *t += i64::from(e.ticks());
                    if voice.events.len() < limits::MAX_EVENTS_PER_VOICE {
                        voice.events.push(e);
                    }
                };
                for c in &v_el.children {
                    match c.name.as_str() {
                        "Clef" => {
                            let ct = c
                                .child_text("concertClefType")
                                .or_else(|| c.child_text("subtype"))
                                .unwrap_or("G");
                            let clef = clef_of(ct);
                            if mi == 0 && t == 0 {
                                if let Some(p) = score.parts.get_mut(pi)
                                    && let Some(slot) = p.clefs.get_mut(si)
                                {
                                    *slot = clef;
                                }
                            } else if t == 0 && vi == 0 && clef_now != Some(clef) {
                                pm_staff.clef = Some(clef);
                            } else if t > 0 && vi == 0 && clef_now != Some(clef) {
                                clef_next = Some(clef);
                            }
                            clef_now = Some(clef);
                        }
                        "KeySig" if top_staff && vi == 0 => {
                            let k = c
                                .child_num("concertKey")
                                .or_else(|| c.child_num("accidental"))
                                .unwrap_or(0.0) as i32;
                            if let Some(info) = score.measures.get_mut(mi) {
                                info.key = Some(KeySig::new(k));
                            }
                        }
                        "TimeSig" if top_staff && vi == 0 => {
                            let n = c.child_num("sigN").unwrap_or(4.0) as i64;
                            let d = c.child_num("sigD").unwrap_or(4.0) as i64;
                            if let Ok(mut ts) = TimeSig::new(n, d) {
                                ts.symbol = match c.child_num("subtype").map(|s| s as i32) {
                                    Some(1) => TimeSymbol::Common,
                                    Some(2) => TimeSymbol::Cut,
                                    _ => TimeSymbol::Normal,
                                };
                                if let Some(info) = score.measures.get_mut(mi) {
                                    info.time = Some(ts);
                                }
                            }
                        }
                        "Tempo" if top_staff => {
                            let text = flat_text(c.child("text").unwrap_or(c));
                            if let Some(info) = score.measures.get_mut(mi) {
                                info.tempo_hidden = c.child_text("visible").is_some_and(|v| v.trim() == "0");
                                if let Some(qps) = c.child_num("tempo") {
                                    // Counted in the note the mark shows (♪ = 172), else in quarters.
                                    let unit = tempo_unit(c).unwrap_or(BeatUnit::Quarter);
                                    info.tempo = Some(Tempo {
                                        bpm: (qps * 60.0 / unit.quarters()).round().clamp(10.0, 400.0) as u16,
                                        unit,
                                    });
                                }
                                // The element tree keeps no order between text and child elements:
                                // prefer the formatted runs (<b>Allegro</b>) when there are any.
                                let runs: Vec<String> = c
                                    .child("text")
                                    .map(|t| {
                                        t.children
                                            .iter()
                                            .filter(|x| x.name != "sym")
                                            .map(flat_text)
                                            .filter(|x| !x.is_empty())
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                let source = if runs.is_empty() {
                                    text.clone()
                                } else {
                                    runs.join(" ")
                                };
                                let word = source
                                    .split_whitespace()
                                    .filter(|w| !w.contains('=') && !w.chars().all(|ch| ch.is_ascii_digit()))
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                if !word.is_empty() && !word.chars().all(|ch| ch.is_ascii_digit()) {
                                    info.tempo_text = Some(clip(&word));
                                }
                            }
                        }
                        "RehearsalMark" if top_staff => {
                            if let Some(info) = score.measures.get_mut(mi) {
                                info.rehearsal = Some(clip(&flat_text(c.child("text").unwrap_or(c))));
                                // The offset is where the mark sits (MuseScore's default
                                // position is only the offset of a mark not moved): below the
                                // staff without one is the default 4 spaces under it.
                                info.rehearsal_place = match placement_of(c) {
                                    Some(p) if p.below && c.child("offset").is_none() => {
                                        Some(Placement::from_sp(0.0, 4.0, true))
                                    }
                                    p => p,
                                };
                            }
                        }
                        "SystemText" if top_staff => {
                            if let Some(info) = score.measures.get_mut(mi) {
                                info.text = Some(clip(&flat_text(c.child("text").unwrap_or(c))));
                            }
                        }
                        "StaffText" | "Expression" | "PlayTechAnnotation" => {
                            let txt = flat_text(c.child("text").unwrap_or(c));
                            // Imported scores carry segno, coda and D.S./D.C. as plain staff text.
                            let sym_jump = c
                                .child("text")
                                .into_iter()
                                .flat_map(|t| t.children_named("sym"))
                                .find_map(|x| jump_of_label(x.text.trim()));
                            let jump = if txt.is_empty() {
                                sym_jump
                            } else {
                                jump_of_text(&txt)
                            };
                            if let Some(j) = jump {
                                if top_staff
                                    && let Some(info) = score.measures.get_mut(mi)
                                    && !info.jumps.contains(&j)
                                {
                                    info.jumps.push(j);
                                }
                            } else if !txt.is_empty() {
                                if pending_text.is_none() {
                                    pending_place = ms_place(c, None);
                                    // A negative least distance lets the text sit anywhere,
                                    // taking no room.
                                    if c.child_num("minDistance").is_some_and(|d| d < 0.0) {
                                        pending_place = pending_place.map(|p| Placement { free: true, ..p });
                                    }
                                    // Its alignment ("right,center"): which side sits at the place.
                                    if let Some(a) = c.child_text("align") {
                                        let mut it = a.split(',').map(str::trim);
                                        let halign = match it.next() {
                                            Some("right") => HAlign::Right,
                                            Some("center") | Some("hcenter") => HAlign::Center,
                                            _ => HAlign::Left,
                                        };
                                        let valign = match it.next() {
                                            Some("top") => VAlign::Top,
                                            Some("center") | Some("vcenter") => VAlign::Middle,
                                            Some("bottom") => VAlign::Bottom,
                                            _ => VAlign::Baseline,
                                        };
                                        pending_place =
                                            pending_place.map(|p| Placement { halign, valign, ..p });
                                    }
                                    pending_text_size = c
                                        .child_num("size")
                                        .filter(|v| v.is_finite() && *v >= 1.0)
                                        .map(|v| v.round().min(96.0) as u8);
                                }
                                pending_text = Some(match pending_text.take() {
                                    Some(p) => format!("{p} {txt}"),
                                    None => txt,
                                });
                            }
                        }
                        "Harmony" => pending_chord = harmony_text(c),
                        "Dynamic" => {
                            pending_dyn = c.child_text("subtype").and_then(Dynamic::from_id);
                            pending_dyn_place = ms_place(c, Some(-1.5));
                            pending_dyn_hidden = c.child_text("visible").is_some_and(|v| v.trim() == "0");
                        }
                        "Fermata" => {
                            if let Some(a) = c
                                .child_text("subtype")
                                .and_then(|s| mark_by_smufl(s).or_else(|| mark_by_legacy(s)))
                            {
                                pending_marks.push(a);
                            }
                        }
                        "Breath" => {
                            // A breath mark follows its note.
                            let sym = c
                                .child_text("symbol")
                                .or_else(|| c.child_text("subtype"))
                                .unwrap_or("breathMarkComma");
                            if let Some(a) = mark_by_smufl(sym).or_else(|| Articulation::from_id("breath"))
                                && let Some(last) = voice.events.last_mut()
                                && !last.articulations.contains(&a)
                            {
                                last.articulations.push(a);
                            }
                        }
                        "location" => {
                            // Moves the voice position (gaps in secondary voices).
                            let dt = c.child_text("fractions").map(fraction_ticks).unwrap_or(0);
                            moved = moved.saturating_add(dt);
                        }
                        "Tuplet" => {
                            let a = c.child_num("actualNotes").unwrap_or(3.0) as i64;
                            let n = c.child_num("normalNotes").unwrap_or(2.0) as i64;
                            if (2..=32).contains(&a) && (1..=32).contains(&n) && a != n {
                                let tu = Tuplet {
                                    actual: a as u8,
                                    normal: n as u8,
                                };
                                if let Some(id) = c.attr("id") {
                                    tuplets_by_id.insert(id.to_string(), tu);
                                } else {
                                    tuplet = Some(tu);
                                }
                            }
                        }
                        "endTuplet" => tuplet = None,
                        "Spanner" => {
                            let kind = match c.attr("type").unwrap_or("") {
                                "Volta" => {
                                    // A volta: the ending number over the measures it spans.
                                    if top_staff && let Some(next) = c.path(&["next", "location"]) {
                                        let n = c
                                            .path(&["Volta", "endings"])
                                            .and_then(|e| {
                                                e.text
                                                    .split([',', ' '])
                                                    .next()
                                                    .and_then(|x| x.trim().parse::<u8>().ok())
                                            })
                                            .unwrap_or(1)
                                            .clamp(1, 9);
                                        let span =
                                            next.child_num("measures").unwrap_or(1.0).max(1.0) as usize;
                                        let label = c
                                            .path(&["Volta", "beginText"])
                                            .map(flat_text)
                                            .filter(|t| !t.is_empty() && *t != format!("{n}."));
                                        for m in mi..(mi + span).min(mi + 64) {
                                            if score.measures.len() <= m {
                                                score.measures.push(MeasureInfo::default());
                                            }
                                            if let Some(info) = score.measures.get_mut(m) {
                                                info.volta = Some(n);
                                                info.volta_text = label.clone().map(|t| clip(&t));
                                            }
                                        }
                                    }
                                    None
                                }
                                _ => line_kind(c),
                            };
                            let at = t.saturating_add(moved);
                            if let (Some(kind), Some(next), true) = (
                                kind,
                                c.path(&["next", "location"]),
                                (0..i64::from(score.measure_ticks(mi))).contains(&at),
                            ) {
                                let dm = next.child_num("measures").unwrap_or(0.0) as i64;
                                let dt = next.child_text("fractions").map(fraction_ticks).unwrap_or(0);
                                lines.push(OpenLine {
                                    kind,
                                    part: pi,
                                    staff: si,
                                    voice: vi,
                                    start: (mi, at as u32),
                                    end: (dm, dt),
                                });
                            }
                        }
                        "Chord" | "Rest" | "MeasureRepeat" | "RepeatMeasure" => {
                            if c.name == "MeasureRepeat" || c.name == "RepeatMeasure" {
                                pm_staff.measure_repeat = true;
                            }
                            // A gap before this event (secondary voices) becomes rests.
                            let room = i64::from(score.measure_ticks(mi)).saturating_sub(t);
                            let gap = std::mem::take(&mut moved).clamp(0, room.max(0));
                            for d in Duration::fill(gap.min(i64::from(u32::MAX)) as u32) {
                                push_event(&mut voice, &mut t, Event::rest(d));
                            }
                            // Grace notes come before their main note and take no time.
                            // grace8after / grace16after / grace32after: after the main note.
                            let after = c
                                .children
                                .iter()
                                .any(|k| k.name.starts_with("grace") && k.name.ends_with("after"));
                            let grace = if c.has("acciaccatura") {
                                Some((NoteValue::Eighth, true))
                            } else if c.has("appoggiatura")
                                || c.has("grace8")
                                || c.has("grace4")
                                || c.has("grace8after")
                            {
                                Some((NoteValue::Eighth, false))
                            } else if c.has("grace16") || c.has("grace16after") {
                                Some((NoteValue::N16th, false))
                            } else if c.has("grace32") || c.has("grace32after") {
                                Some((NoteValue::N32nd, false))
                            } else {
                                None
                            };
                            if let Some((value, slash)) = grace {
                                let pitches: Vec<Pitch> = c
                                    .children_named("Note")
                                    .map(|n| {
                                        pitch_of(
                                            n.child_num("pitch").unwrap_or(60.0) as i32,
                                            n.child_num("tpc").map(|v| v as i32),
                                        )
                                    })
                                    .collect();
                                if !pitches.is_empty() && graces.len() < limits::MAX_GRACE {
                                    graces.push(Grace {
                                        pitches,
                                        value,
                                        slash,
                                        after,
                                    });
                                }
                                continue;
                            }
                            let dt = c.child_text("durationType").unwrap_or("quarter");
                            let mut dur = if dt == "measure" {
                                let ticks = c
                                    .child_text("duration")
                                    .map(fraction_ticks)
                                    .unwrap_or_else(|| i64::from(score.measure_ticks(mi)))
                                    .clamp(1, i64::from(TPQ) * 64);
                                Duration::from_ticks(ticks as u32)
                                    .unwrap_or(Duration::new(NoteValue::Whole, 0))
                            } else {
                                Duration::new(
                                    duration_type(dt).unwrap_or(NoteValue::Quarter),
                                    c.child_num("dots").unwrap_or(0.0).clamp(0.0, 3.0) as u8,
                                )
                            };
                            dur.tuplet = tuplet.or_else(|| {
                                c.child_text("Tuplet")
                                    .and_then(|id| tuplets_by_id.get(id).copied())
                            });
                            let mut e = Event::rest(dur);
                            if dt == "measure" {
                                // A whole-measure rest: fill the measure exactly.
                                let len = score.measure_ticks(mi);
                                // What stands at its start (chord symbol, text, dynamic) goes
                                // with its first rest.
                                for (k, d) in Duration::decompose(
                                    len.saturating_sub(t.clamp(0, i64::from(u32::MAX)) as u32),
                                )
                                .into_iter()
                                .enumerate()
                                {
                                    let mut r = Event::rest(d);
                                    if k == 0 {
                                        r.chord_symbol = pending_chord.take();
                                        r.text = pending_text.take();
                                        r.text_place = pending_place.take();
                                        r.text_size = pending_text_size.take();
                                        if let Some(dy) = pending_dyn.take() {
                                            r.dynamic = Some(dy);
                                            r.dynamic_place = pending_dyn_place.take();
                                            r.dynamic_hidden = std::mem::take(&mut pending_dyn_hidden);
                                        }
                                    }
                                    push_event(&mut voice, &mut t, r);
                                }
                                continue;
                            }
                            if c.name == "Chord" {
                                for sp in c.children_named("Spanner") {
                                    if let (Some(kind), Some(next)) =
                                        (line_kind(sp), sp.path(&["next", "location"]))
                                    {
                                        lines.push(OpenLine {
                                            kind,
                                            part: pi,
                                            staff: si,
                                            voice: vi,
                                            start: (mi, t.max(0).min(i64::from(u32::MAX)) as u32),
                                            end: (
                                                next.child_num("measures").unwrap_or(0.0) as i64,
                                                next.child_text("fractions").map(fraction_ticks).unwrap_or(0),
                                            ),
                                        });
                                    }
                                }
                                let mut notes = Vec::new();
                                for n in c.children_named("Note").take(limits::MAX_CHORD_NOTES) {
                                    let p = pitch_of(
                                        n.child_num("pitch").unwrap_or(60.0) as i32,
                                        n.child_num("tpc").map(|v| v as i32),
                                    );
                                    let mut note = Note::new(p);
                                    note.tie = n.has("Tie")
                                        || n.children_named("Spanner")
                                            .any(|s| s.attr("type") == Some("Tie") && s.has("next"));
                                    if let Some(h) =
                                        n.child_text("headGroup").or_else(|| n.child_text("head"))
                                    {
                                        note.head = head_of(h);
                                    }
                                    note.paren = n.child_text("hasParentheses").is_some_and(|v| v != "0")
                                        || n.children_named("Symbol").any(|s| {
                                            s.child_text("name").is_some_and(|x| x.contains("Parenthesis"))
                                        });
                                    if let Some(f) = n.child("Fingering") {
                                        let txt = flat_text(f.child("text").unwrap_or(f));
                                        if !txt.is_empty() {
                                            note.fingering = Some(clip(&txt));
                                        }
                                    }
                                    if n.path(&["Accidental", "role"])
                                        .is_some_and(|r| r.text.trim() == "1")
                                    {
                                        note.forced = Some(scoreart_model::Accidental::from_alter(p.alter));
                                    }
                                    // Glissandos start on a note.
                                    for s in n
                                        .children_named("Spanner")
                                        .filter(|s| s.attr("type") == Some("Glissando"))
                                    {
                                        if let Some(next) = s.path(&["next", "location"]) {
                                            let wavy = s
                                                .path(&["Glissando", "subtype"])
                                                .is_some_and(|x| x.text.trim() == "1");
                                            lines.push(OpenLine {
                                                kind: if wavy {
                                                    SpannerKind::WavyGlissando
                                                } else {
                                                    SpannerKind::Glissando
                                                },
                                                part: pi,
                                                staff: si,
                                                voice: vi,
                                                start: (mi, t.max(0).min(i64::from(u32::MAX)) as u32),
                                                end: (
                                                    next.child_num("measures").unwrap_or(0.0) as i64,
                                                    next.child_text("fractions")
                                                        .map(fraction_ticks)
                                                        .unwrap_or(0),
                                                ),
                                            });
                                        }
                                    }
                                    notes.push(note);
                                }
                                // MuseScore 4.6+ parenthesises notes through <NoteParenGroup>.
                                for g in c.children_named("NoteParenGroup") {
                                    for idx in g
                                        .children_named("Notes")
                                        .flat_map(|n| n.children_named("NoteIdx"))
                                    {
                                        if let Some(n) = idx
                                            .text
                                            .trim()
                                            .parse::<usize>()
                                            .ok()
                                            .and_then(|i| notes.get_mut(i))
                                        {
                                            n.paren = true;
                                        }
                                    }
                                }
                                if !notes.is_empty() {
                                    e = Event::chord(notes, dur);
                                }
                                for a in c
                                    .children_named("Articulation")
                                    .chain(c.children_named("Ornament"))
                                {
                                    if let Some(m) = a
                                        .child_text("subtype")
                                        .and_then(|s| mark_by_smufl(s).or_else(|| mark_by_legacy(s)))
                                        && !e.articulations.contains(&m)
                                    {
                                        e.articulations.push(m);
                                    }
                                }
                                for chord_line in c.children_named("ChordLine") {
                                    let sub = chord_line.child_text("subtype").unwrap_or("0");
                                    let id = match sub {
                                        "1" | "fall" => "fall",
                                        "2" | "doit" => "doit",
                                        "3" | "plop" => "plop",
                                        "4" | "scoop" => "scoop",
                                        _ => continue,
                                    };
                                    if let Some(m) = Articulation::from_id(id)
                                        && !e.articulations.contains(&m)
                                    {
                                        e.articulations.push(m);
                                    }
                                }
                                // MusicXML imports keep portato as staccato + tenuto.
                                if let (Some(st), Some(te), Some(po)) = (
                                    Articulation::from_id("staccato"),
                                    Articulation::from_id("tenuto"),
                                    Articulation::from_id("portato"),
                                ) && e.articulations.contains(&st)
                                    && e.articulations.contains(&te)
                                {
                                    e.articulations.retain(|a| *a != st && *a != te);
                                    if !e.articulations.contains(&po) {
                                        e.articulations.push(po);
                                    }
                                }
                                let trem = c.child("Tremolo").or_else(|| c.child("TremoloSingleChord"));
                                if let Some(tr) = trem.and_then(|x| x.child_text("subtype")) {
                                    e.tremolo = match tr {
                                        "r8" => Tremolo::Strokes(1),
                                        "r16" => Tremolo::Strokes(2),
                                        "r32" => Tremolo::Strokes(3),
                                        "r64" => Tremolo::Strokes(4),
                                        "buzzroll" => Tremolo::Buzz,
                                        _ => Tremolo::None,
                                    };
                                }
                                if let Some(a) = c.child("Arpeggio") {
                                    e.arpeggio = match a.child_text("subtype").unwrap_or("0") {
                                        "1" => Arpeggio::Up,
                                        "2" => Arpeggio::Down,
                                        "3" => Arpeggio::Bracket,
                                        _ => Arpeggio::Normal,
                                    };
                                }
                                // One <Lyrics> per verse; <no> counts from 0.
                                for l in c.children_named("Lyrics") {
                                    let txt = flat_text(l.child("text").unwrap_or(l));
                                    if txt.is_empty() {
                                        continue;
                                    }
                                    let verse = l.child_num("no").map_or(0, |n| n.max(0.0) as usize);
                                    let hyphen = matches!(
                                        l.child_text("syllabic").map(str::trim),
                                        Some("begin" | "middle")
                                    );
                                    let txt = if hyphen { format!("{txt}-") } else { txt };
                                    e.set_lyric(verse, Some(clip(&txt)));
                                }
                                e.grace = std::mem::take(&mut graces);
                            }
                            for m in pending_marks.drain(..) {
                                if !e.articulations.contains(&m) {
                                    e.articulations.push(m);
                                }
                            }
                            if let Some(d) = pending_dyn.take() {
                                e.dynamic = Some(d);
                                e.dynamic_place = pending_dyn_place.take();
                                e.dynamic_hidden = std::mem::take(&mut pending_dyn_hidden);
                            }
                            e.text = pending_text.take();
                            e.text_place = pending_place.take();
                            e.text_size = pending_text_size.take();
                            e.chord_symbol = pending_chord.take();
                            push_event(&mut voice, &mut t, e);
                        }
                        "BarLine" if top_staff && vi == 0 => {
                            let sub = c.child_text("subtype").unwrap_or("normal");
                            if let Some(info) = score.measures.get_mut(mi) {
                                match sub {
                                    "double" => info.barline = Barline::Double,
                                    "end" | "final" => info.barline = Barline::Final,
                                    "end-repeat" => info.barline = Barline::RepeatEnd,
                                    _ if c.child_text("visible").is_some_and(|v| v.trim() == "0") => {
                                        info.barline = Barline::Hidden
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
                pm_staff.voices.push(voice);
            }
            // Spacers of this staff (staff spaces).
            let spacer = |k: &str| {
                m_el.child_num(k)
                    .filter(|v| v.is_finite() && *v > 0.0)
                    .map(|v| v as f32)
            };
            pm_staff.spacer_up = spacer("vspacerUp");
            pm_staff.spacer_down = spacer("vspacerDown");
            pm_staff.spacer_fixed = spacer("vspacerFixed");
            // Drop trailing empty secondary voices so normalize keeps the measure tidy.
            if let Some(p) = score.parts.get_mut(pi) {
                while p.measures.len() <= mi {
                    p.measures.push(PartMeasure::default());
                }
                if let Some(pm) = p.measures.get_mut(mi) {
                    while pm.staves.len() <= si {
                        pm.staves.push(StaffMeasure::default());
                    }
                    if let Some(slot) = pm.staves.get_mut(si) {
                        *slot = pm_staff;
                    }
                }
            }
        }
    }
    for (m, b) in frame_breaks {
        if let Some(info) = score.measures.get_mut(m) {
            layout_break(info, b);
        }
    }
    for (m, h) in frames {
        if let Some(info) = score.measures.get_mut(m) {
            info.system_break = true;
            if h.is_some() {
                info.frame_after = h.map(|h| h.min(200.0));
            }
        }
    }
    score.meta = meta;
    if let Some(last) = score.measures.last_mut()
        && last.barline == Barline::Regular
    {
        last.barline = Barline::Final;
    }
    score.normalize();

    // Resolve lines now that every measure is known.
    for l in lines {
        let measure_len = |m: usize| i64::from(score.measure_ticks(m));
        let mut m = l.start.0 as i64 + l.end.0;
        let mut tick = i64::from(l.start.1) + l.end.1;
        // Normalise the end into a measure.
        let mut guard = 0;
        while tick < 0 && m > 0 && guard < 64 {
            m -= 1;
            tick += measure_len(m as usize);
            guard += 1;
        }
        while m >= 0 && (m as usize) < score.measures.len() && tick >= measure_len(m as usize) && guard < 128
        {
            tick -= measure_len(m as usize);
            m += 1;
            guard += 1;
        }
        if m < 0 {
            continue;
        }
        let (mut em, mut et) = (
            (m as usize).min(score.measures.len().saturating_sub(1)),
            tick.max(0) as u32,
        );
        let Some(v) = score.voice(l.part, l.staff, em, l.voice) else {
            continue;
        };
        let onsets = v.onsets();
        // Slurs and glissandos end on the note at the end position; other lines end before it.
        let on_note = matches!(
            l.kind,
            SpannerKind::Slur | SpannerKind::Glissando | SpannerKind::WavyGlissando
        );
        let exact = onsets.contains(&et);
        if !(on_note && exact) {
            match onsets.iter().copied().filter(|o| *o < et).max() {
                Some(o) => et = o,
                None if em > 0 && et == 0 => {
                    em -= 1;
                    et = score
                        .voice(l.part, l.staff, em, l.voice)
                        .and_then(|v| v.onsets().last().copied())
                        .unwrap_or(0);
                }
                None => et = 0,
            }
        }
        score.spanners.push(Spanner {
            kind: l.kind,
            part: l.part,
            staff: l.staff,
            voice: l.voice,
            start: Anchor {
                measure: l.start.0,
                tick: l.start.1,
            },
            end: Anchor {
                measure: em,
                tick: et,
            },
            dy: 0,
            spread: 0,
        });
    }
    // MuseScore keeps notes under an octave line at their written position; ScoreArt keeps the
    // sounding pitch and displaces them only on screen.
    let ottavas: Vec<(i8, Spanner)> = score
        .spanners
        .iter()
        .filter_map(|s| match s.kind {
            SpannerKind::Ottava(o) => Some((o, *s)),
            _ => None,
        })
        .collect();
    for (o, sp) in ottavas {
        let last = sp.end.measure.min(score.measures.len().saturating_sub(1));
        for m in sp.start.measure..=last {
            for v in 0..limits::MAX_VOICES {
                let Some(voice) = score.voice_mut(sp.part, sp.staff, m, v) else {
                    break;
                };
                let onsets = voice.onsets();
                for (e, at) in voice.events.iter_mut().zip(onsets) {
                    if (m, at) < (sp.start.measure, sp.start.tick) || (m, at) > (sp.end.measure, sp.end.tick)
                    {
                        continue;
                    }
                    let shift =
                        |p: &mut Pitch| *p = Pitch::new(p.step, p.alter, i32::from(p.octave) + i32::from(o));
                    for n in e.notes_mut() {
                        shift(&mut n.pitch);
                    }
                    for g in &mut e.grace {
                        g.pitches.iter_mut().for_each(shift);
                    }
                }
            }
        }
    }
    // The style: MuseScore's defaults for the file's version, the score's own style, and the
    // style file of an .mscz, in that order.
    // MuseScore 2 files get MuseScore 3's defaults (the closest ones known).
    let major = if version.starts_with('3') || version.starts_with('2') {
        3
    } else {
        4
    };
    let mut style: BTreeMap<String, String> = ms_defaults(major)
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    if let Some(st) = sc.child("Style") {
        collect_style(&mut style, st);
    }
    if let Some(mss) = mss
        && let Ok(r) = dom::parse(mss)
        && let Some(st) = if r.name == "Style" {
            Some(&r)
        } else {
            r.child("Style")
        }
    {
        collect_style(&mut style, st);
    }
    apply_style(&mut score, &style, major);
    score.normalize();
    Ok(score)
}

/// MuseScore's own values for the style settings ScoreArt reads, by major version (a score
/// stores only what differs from them). Observed in style files saved by MuseScore 3 and 4.
/// A `<LayoutBreak>` of MuseScore on the measure it ends: line, page or section break.
fn layout_break(info: &mut MeasureInfo, b: &Element) {
    match b.child_text("subtype").map(str::trim) {
        Some("page") => info.page_break = true,
        Some("line") => info.system_break = true,
        Some("section") => {
            let flag = |k: &str| b.child_text(k).map(|v| v.trim() != "0");
            let d = scoreart_model::SectionBreak::default();
            info.section = Some(scoreart_model::SectionBreak {
                pause: b.child_num("pause").map_or(d.pause, |v| v as f32),
                long_names: flag("startWithLongNames").unwrap_or(d.long_names),
                restart_numbers: flag("startWithMeasureOne").unwrap_or(d.restart_numbers),
            });
        }
        _ => {}
    }
}

fn ms_defaults(major: u8) -> &'static [(&'static str, &'static str)] {
    const COMMON: &[(&str, &str)] = &[
        ("pageWidth", "8.27"),
        ("pageHeight", "11.69"),
        ("staffUpperBorder", "7"),
        ("staffLowerBorder", "7"),
        ("staffDistance", "6.5"),
        ("akkoladeDistance", "6.5"),
        ("minSystemDistance", "8.5"),
        ("maxSystemDistance", "15"),
        ("spreadSystem", "2.5"),
        ("minSystemSpread", "8.5"),
        ("maxSystemSpread", "32"),
        ("lastSystemFillLimit", "0.3"),
        ("firstSystemIndentationValue", "5"),
        ("showPageNumber", "1"),
        ("showPageNumberOne", "0"),
        ("headerFirstPage", "0"),
        ("headerOddEven", "1"),
        ("footerFirstPage", "1"),
        ("footerOddEven", "1"),
        ("evenFooterC", "$:copyright:"),
        ("oddFooterC", "$:copyright:"),
        ("showFooter", "1"),
        ("titleAlign", "center,top"),
        ("subTitleAlign", "center,top"),
        ("composerAlign", "right,bottom"),
        ("lyricistAlign", "left,bottom"),
        ("frameAlign", "left,top"),
        ("tempoFontStyle", "1"),
        ("rehearsalMarkFontStyle", "1"),
        ("fingeringFontSize", "8"),
        ("measureNumberFontSize", "8"),
        ("tempoFontSize", "12"),
        ("systemFontSize", "10"),
        ("staffFontSize", "10"),
        ("rehearsalMarkFontSize", "14"),
    ];
    const V3: &[(&str, &str)] = &[
        ("pagePrintableWidth", "7.08661"),
        ("pageOddLeftMargin", "0.590551"),
        ("pageEvenLeftMargin", "0.590551"),
        ("pageOddTopMargin", "0.590551"),
        ("pageOddBottomMargin", "0.590551"),
        ("pageEvenTopMargin", "0.590551"),
        ("pageEvenBottomMargin", "0.590551"),
        ("spatium", "1.74978"),
        // Off: MuseScore 3.6 files store it only when it is switched on.
        ("enableVerticalSpread", "0"),
        ("measureSpacing", "1.2"),
        ("enableIndentationOnFirstSystem", "1"),
        ("showHeader", "1"),
        ("evenHeaderL", "$p"),
        ("oddHeaderR", "$p"),
        ("footerOffset", "0"),
        ("defaultFontFace", "Edwin"),
        ("titleFontSize", "22"),
        ("subTitleFontSize", "16"),
        ("composerFontSize", "10"),
        ("lyricistFontSize", "10"),
        ("headerFontSize", "11"),
        ("headerFontStyle", "1"),
        ("footerFontSize", "9"),
        ("longInstrumentFontSize", "10"),
        ("measureNumberFontStyle", "2"),
        ("measureNumberFontSpatiumDependent", "0"),
        ("chordSymbolAFontSize", "11"),
        ("lyricsOddFontSize", "10"),
        ("frameFontSize", "10"),
    ];
    const V4: &[(&str, &str)] = &[
        ("pagePrintableWidth", "7.4826"),
        ("pageOddLeftMargin", "0.393701"),
        ("pageEvenLeftMargin", "0.393701"),
        ("pageOddTopMargin", "0.393701"),
        ("pageOddBottomMargin", "0.787402"),
        ("pageEvenTopMargin", "0.393701"),
        ("pageEvenBottomMargin", "0.787402"),
        ("spatium", "1.76389"),
        ("enableVerticalSpread", "0"),
        ("measureSpacing", "1.5"),
        ("enableIndentationOnFirstSystem", "0"),
        ("showHeader", "0"),
        ("evenFooterL", "$p"),
        ("oddFooterR", "$p"),
        ("footerOffset", "5"),
        ("defaultFontFace", "Edwin"),
        ("titleFontSize", "24"),
        ("subTitleFontSize", "14"),
        ("composerFontSize", "12"),
        ("lyricistFontSize", "12"),
        ("headerFontSize", "8"),
        ("footerFontSize", "8"),
        ("longInstrumentFontSize", "12"),
        ("chordSymbolAFontSize", "12"),
        ("lyricsOddFontSize", "11"),
        ("frameFontSize", "12"),
    ];
    static MERGED: std::sync::OnceLock<[Vec<(&'static str, &'static str)>; 2]> = std::sync::OnceLock::new();
    let m = MERGED.get_or_init(|| {
        let mut v3 = COMMON.to_vec();
        v3.extend_from_slice(V3);
        let mut v4 = COMMON.to_vec();
        v4.extend_from_slice(V4);
        [v3, v4]
    });
    if major == 3 { &m[0] } else { &m[1] }
}

/// Adds a `<Style>`'s values to `map` (offsets by their y, as "footerOffset" → "5").
fn collect_style(map: &mut BTreeMap<String, String>, st: &Element) {
    for c in &st.children {
        let v = if c.name.ends_with("Offset") {
            c.attr("y").unwrap_or("0").to_string()
        } else {
            c.text.trim().to_string()
        };
        // MuseScore 3 wrote the staff space capitalised.
        let name = if c.name == "Spatium" {
            "spatium".to_string()
        } else {
            c.name.clone()
        };
        map.insert(name, v);
    }
}

/// Whether a font is a sans-serif one (everything else counts as serif).
fn sans_font(face: &str) -> bool {
    let f = face.to_lowercase();
    f.contains("sans")
        || [
            "arial",
            "helvetica",
            "inter",
            "roboto",
            "verdana",
            "calibri",
            "segoe",
            "tahoma",
            "lato",
        ]
        .iter()
        .any(|n| f.starts_with(n))
}

/// The page format, headers and footers and text styles of a MuseScore style (page lengths in
/// inches, the staff space in mm, distances in staff spaces, font sizes in points).
pub(crate) fn apply_style(score: &mut Score, st: &BTreeMap<String, String>, major: u8) {
    use scoreart_model::{TextAlign, TextFont, TextKind, TextStyle};
    let text = |k: &str| st.get(k).map(String::as_str);
    let num = |k: &str| {
        text(k)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
    };
    let flag = |k: &str| text(k).map(|v| v.trim() == "1" || v.trim() == "true");
    let inch = |v: f64| (v * 25.4) as f32;
    let f = &mut score.page;
    if let Some(s) = num("spatium").filter(|s| *s > 0.0) {
        f.staff_space_mm = s as f32;
    }
    let w = num("pageWidth").filter(|v| *v > 0.0);
    if let Some(w) = w {
        f.width_mm = inch(w);
    }
    if let Some(h) = num("pageHeight").filter(|v| *v > 0.0) {
        f.height_mm = inch(h);
    }
    let printable = num("pagePrintableWidth");
    if let (Some(w), Some(pw)) = (w, printable) {
        if let Some(l) = num("pageOddLeftMargin") {
            f.margin_left_mm = inch(l);
            f.margin_right_mm = inch((w - pw - l).max(0.0));
        }
        if let Some(l) = num("pageEvenLeftMargin") {
            f.even_margin_left_mm = inch(l);
            f.even_margin_right_mm = inch((w - pw - l).max(0.0));
        }
    }
    for (k, slot) in [
        ("pageOddTopMargin", &mut f.margin_top_mm),
        ("pageOddBottomMargin", &mut f.margin_bottom_mm),
        ("pageEvenTopMargin", &mut f.even_margin_top_mm),
        ("pageEvenBottomMargin", &mut f.even_margin_bottom_mm),
    ] {
        if let Some(v) = num(k).filter(|v| *v >= 0.0) {
            *slot = inch(v);
        }
    }
    // Two-sided matters only where the even pages differ.
    let differ = |a: f32, b: f32| (a - b).abs() > 0.05;
    f.two_sided = flag("pageTwosided").unwrap_or(true)
        && (differ(f.margin_left_mm, f.even_margin_left_mm)
            || differ(f.margin_right_mm, f.even_margin_right_mm)
            || differ(f.margin_top_mm, f.even_margin_top_mm)
            || differ(f.margin_bottom_mm, f.even_margin_bottom_mm));
    let sp = |k: &str, slot: &mut f32| {
        if let Some(v) = num(k).filter(|v| *v >= 0.0) {
            *slot = v as f32;
        }
    };
    sp("staffUpperBorder", &mut f.music_top);
    sp("staffLowerBorder", &mut f.music_bottom);
    sp("akkoladeDistance", &mut f.staff_gap);
    sp("staffDistance", &mut f.part_gap);
    sp("maxSystemDistance", &mut f.max_system_gap);
    sp("spreadSystem", &mut f.spread_factor);
    sp("maxSystemSpread", &mut f.max_system_spread);
    sp("maxStaffSpread", &mut f.max_staff_spread);
    sp("lastSystemFillLimit", &mut f.last_system_fill);
    f.vertical_justify = flag("enableVerticalSpread").unwrap_or(false);
    f.single_name = !flag("hideInstrumentNameIfOneInstrument").unwrap_or(true);
    let vis = |k: &str, d: u8| num(k).map_or(d, |v| v.clamp(0.0, 2.0) as u8);
    f.first_names = vis("firstSystemInstNameVisibility", 0);
    f.later_names = vis("subsSystemInstNameVisibility", 1);
    f.german_chords =
        flag("useGermanNoteNames").unwrap_or(false) || flag("useFullGermanNoteNames").unwrap_or(false);
    // With vertical justification the least system distance is a setting of its own.
    let min_key = if f.vertical_justify {
        "minSystemSpread"
    } else {
        "minSystemDistance"
    };
    sp(min_key, &mut f.system_gap);
    f.first_indent = if flag("enableIndentationOnFirstSystem").unwrap_or(false) {
        num("firstSystemIndentationValue").unwrap_or(5.0) as f32
    } else {
        0.0
    };
    // MuseScore spaces notes by a ratio whose default is ScoreArt's normal spacing.
    let normal = if major == 3 { 1.2 } else { 1.5 };
    if let Some(v) = num("measureSpacing").filter(|v| *v > 0.0) {
        f.spacing = (v / normal) as f32;
    }
    sp("headerOffset", &mut f.header_offset);
    sp("footerOffset", &mut f.footer_offset);
    score.page = score.page.clamped();
    // Hide empty staves for the whole score.
    if flag("hideEmptyStaves").unwrap_or(false) {
        for p in &mut score.parts {
            p.hide_empty = vec![true; p.staff_count()];
        }
    }

    // Headers and footers.
    let hf = &mut score.header_footer;
    let field = |k: &str| clip(text(k).unwrap_or(""));
    hf.show_header = flag("showHeader").unwrap_or(false);
    hf.header_first_page = flag("headerFirstPage").unwrap_or(false);
    hf.header_odd_even = flag("headerOddEven").unwrap_or(true);
    hf.show_footer = flag("showFooter").unwrap_or(true);
    hf.footer_first_page = flag("footerFirstPage").unwrap_or(true);
    hf.footer_odd_even = flag("footerOddEven").unwrap_or(true);
    for (row, side) in [(0usize, "odd"), (1, "even")] {
        for (i, pos) in ["L", "C", "R"].iter().enumerate() {
            hf.header[row][i] = field(&format!("{side}Header{pos}"));
            hf.footer[row][i] = field(&format!("{side}Footer{pos}"));
        }
    }
    score.hide_page_numbers = !flag("showPageNumber").unwrap_or(true);
    hf.page_number_first_page = flag("showPageNumberOne").unwrap_or(false);

    // Text styles.
    let spatium = f64::from(score.page.staff_space_mm);
    let default_face = text("defaultFontFace").unwrap_or("Edwin").to_string();
    for (prefix, kind) in [
        ("title", TextKind::Title),
        ("subTitle", TextKind::Subtitle),
        ("composer", TextKind::Composer),
        ("lyricist", TextKind::Lyricist),
        ("header", TextKind::Header),
        ("footer", TextKind::Footer),
        ("footer", TextKind::PageNumber),
        ("footer", TextKind::Copyright),
        ("frame", TextKind::Text),
        ("longInstrument", TextKind::InstrumentName),
        ("measureNumber", TextKind::MeasureNumber),
        ("tempo", TextKind::Tempo),
        ("system", TextKind::SystemText),
        ("staff", TextKind::StaffText),
        ("rehearsalMark", TextKind::Rehearsal),
        ("chordSymbolA", TextKind::ChordSymbol),
        ("lyricsOdd", TextKind::Lyrics),
        ("fingering", TextKind::Fingering),
    ] {
        let k = |what: &str| format!("{prefix}{what}");
        let face = text(&k("FontFace")).unwrap_or(&default_face).to_string();
        let Some(mut size) = num(&k("FontSize"))
            .or_else(|| num("defaultFontSize"))
            .or(Some(10.0))
        else {
            continue;
        };
        // Sizes that grow with the staff size are given for MuseScore's standard staff space.
        let dependent = flag(&k("FontSpatiumDependent")).unwrap_or(!kind.is_block());
        if dependent {
            size *= spatium / 1.764;
        }
        let bits = num(&k("FontStyle")).unwrap_or(0.0) as u32;
        let align = match text(&k("Align")).and_then(|a| a.split(',').next()).map(str::trim) {
            Some("left") => TextAlign::Left,
            Some("right") => TextAlign::Right,
            Some("center") => TextAlign::Center,
            _ => kind.default_style().align,
        };
        score.text_styles.set(
            kind,
            TextStyle {
                font: if sans_font(&face) {
                    TextFont::Sans
                } else {
                    TextFont::Serif
                },
                size: ((size.clamp(4.0, 96.0) * 2.0).round() / 2.0) as f32,
                bold: bits & 1 != 0,
                italic: bits & 2 != 0,
                align,
            },
        );
    }
}
