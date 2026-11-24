//! MuseScore 2 files (`.mscx` version 2.x) brought into the layout of MuseScore 3 files, which
//! the reader in `mscz.rs` understands. Built from observing files written by MuseScore 2
//! (the OpenScore corpus), not from MuseScore's source.
//!
//! What differs in MuseScore 2:
//! - a measure holds its content directly, not in `<voice>` elements; each chord or rest names
//!   its voice by `<track>` (staff × 4 + voice), and `<tick>` (absolute, in `<Division>`s per
//!   quarter) moves the position back for the next voice;
//! - lines are declared where they start (`<HairPin id="7">`, `<Pedal>`, `<Ottava>` …) and end
//!   at `<endSpanner id="7"/>`; slurs are declared in the measure (`<Slur id="3">`) and named by
//!   the chords where they start and stop (`<Slur type="start" id="3"/>`);
//! - a measure rest gives its length as `<duration z="3" n="4"/>`.

use std::collections::HashMap;

use scoreart_model::TPQ;

use crate::dom::Element;

/// Lines declared in a measure and ended by `<endSpanner>`.
const LINES: &[&str] = &[
    "HairPin", "Pedal", "Ottava", "Trill", "TextLine", "Volta", "LetRing", "Vibrato", "PalmMute",
];

/// Kept on the measure itself in the MuseScore 3 layout.
const MEASURE_LEVEL: &[&str] = &[
    "LayoutBreak",
    "vspacerUp",
    "vspacerDown",
    "vspacerFixed",
    "irregular",
    "startRepeat",
    "endRepeat",
    "Jump",
    "Marker",
    "stretch",
];

fn el(name: &str, children: Vec<Element>) -> Element {
    Element {
        name: name.into(),
        attrs: Vec::new(),
        children,
        text: String::new(),
    }
}

fn text_el(name: &str, text: String) -> Element {
    Element {
        text,
        ..el(name, Vec::new())
    }
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a.abs().max(1) } else { gcd(b, a % b) }
}

/// Ticks (TPQ per quarter) as a fraction of a whole note: "3/8".
fn fraction(ticks: i64) -> String {
    let whole = i64::from(TPQ) * 4;
    let g = gcd(ticks, whole);
    format!("{}/{}", ticks / g, whole / g)
}

fn location(ticks: i64) -> Element {
    el("location", vec![text_el("fractions", fraction(ticks))])
}

/// "3/4" → ticks.
fn fraction_ticks(s: &str) -> i64 {
    match s.trim().split_once('/') {
        Some((a, b)) => {
            let (a, b) = (
                a.trim().parse::<i64>().unwrap_or(0),
                b.trim().parse::<i64>().unwrap_or(1),
            );
            if b == 0 {
                0
            } else {
                a.saturating_mul(i64::from(TPQ) * 4) / b
            }
        }
        None => 0,
    }
}

fn value_ticks(name: &str) -> Option<i64> {
    let q = i64::from(TPQ);
    Some(match name {
        "long" => q * 16,
        "breve" => q * 8,
        "whole" => q * 4,
        "half" => q * 2,
        "quarter" => q,
        "eighth" => q / 2,
        "16th" => q / 4,
        "32nd" => q / 8,
        "64th" => q / 16,
        "128th" => q / 32,
        "256th" => q / 64,
        _ => return None,
    })
}

fn is_grace(c: &Element) -> bool {
    c.children.iter().any(|k| {
        matches!(k.name.as_str(), "acciaccatura" | "appoggiatura")
            || (k.name.starts_with("grace") && k.name.len() > 5)
    })
}

/// One thing of a measure, placed in time and voice.
struct Item {
    time: i64,
    voice: usize,
    el: Element,
    /// How far it moves its voice on (chords and rests).
    len: i64,
}

/// A line from where it starts to where it ends.
struct Line {
    measure: usize,
    time: i64,
    voice: usize,
    kind: String,
    def: Element,
    end: Option<(usize, i64)>,
}

/// Rewrites a MuseScore 2 `<museScore>` tree in place into the MuseScore 3 layout.
pub(crate) fn upgrade(root: &mut Element) {
    let Some(score) = root.children.iter_mut().find(|c| c.name == "Score") else {
        return;
    };
    let division = score
        .child_num("Division")
        .filter(|d| d.is_finite() && *d > 0.0)
        .unwrap_or(480.0) as i64;
    let to_ticks = |d: i64| d.saturating_mul(i64::from(TPQ)) / division.max(1);
    text_styles(score);
    // Staves of the music (after the parts): read all first, since a slur may end in the
    // other staff of a piano.
    let mut found: Vec<Found> = score
        .children
        .iter()
        .filter(|c| c.name == "Staff")
        .map(|st| analyse(st, &to_ticks))
        .collect();
    let mut slur_end: HashMap<String, i64> = HashMap::new();
    for f in &found {
        slur_end.extend(f.slur_end.iter().map(|(k, v)| (k.clone(), *v)));
    }
    for (staff, f) in score
        .children
        .iter_mut()
        .filter(|c| c.name == "Staff")
        .zip(found.iter_mut())
    {
        build(staff, f, &slur_end);
    }
    // Title frames: OpenScore's MuseScore 2 files name the title texts after the work.
    for staff in score.children.iter_mut().filter(|c| c.name == "Staff") {
        for frame in staff.children.iter_mut().filter(|c| c.name == "VBox") {
            for t in frame.children.iter_mut().filter(|c| c.name == "Text") {
                for s in t.children.iter_mut().filter(|c| c.name == "style") {
                    // (Their movement title is the big one, the work title the small one.)
                    let new = match s.text.trim() {
                        "Movement Title" => "Title",
                        "Work Title" => "Subtitle",
                        "Poet" => "Lyricist",
                        _ => continue,
                    };
                    s.text = new.into();
                }
            }
        }
    }
}

/// MuseScore 2 writes every text style as a `<TextStyle>` block (name, family, size, bold,
/// italic); MuseScore 3 has one style entry each (`titleFontSize` …).
fn text_styles(score: &mut Element) {
    const NAMES: &[(&str, &str)] = &[
        ("Title", "title"),
        ("Subtitle", "subTitle"),
        ("Composer", "composer"),
        ("Lyricist", "lyricist"),
        ("Lyrics Odd Lines", "lyricsOdd"),
        ("Lyrics Even Lines", "lyricsEven"),
        ("Fingering", "fingering"),
        ("Instrument Name (Long)", "longInstrument"),
        ("Instrument Name (Short)", "shortInstrument"),
        ("Dynamics", "dynamics"),
        ("Tempo", "tempo"),
        ("Measure Number", "measureNumber"),
        ("System", "system"),
        ("Staff", "staff"),
        ("Chord Symbol", "chordSymbolA"),
        ("Rehearsal Mark", "rehearsalMark"),
        ("Header", "header"),
        ("Footer", "footer"),
        ("Frame", "frame"),
        // The title texts of their frames (see above) come last and win.
        ("Movement Title", "title"),
        ("Work Title", "subTitle"),
    ];
    let Some(style) = score.children.iter_mut().find(|c| c.name == "Style") else {
        return;
    };
    let blocks: Vec<Element> = style
        .children
        .iter()
        .filter(|c| c.name == "TextStyle")
        .cloned()
        .collect();
    let mut add: Vec<(String, String)> = Vec::new();
    for (name, key) in NAMES {
        let Some(b) = blocks
            .iter()
            .find(|b| b.child_text("name").map(str::trim) == Some(*name))
        else {
            continue;
        };
        if let Some(size) = b.child_text("size") {
            add.push((format!("{key}FontSize"), size.trim().to_string()));
        }
        if let Some(face) = b.child_text("family") {
            add.push((format!("{key}FontFace"), face.trim().to_string()));
        }
        let on = |k: &str| b.child_text(k).is_some_and(|v| v.trim() == "1");
        let bits = u8::from(on("bold")) | (u8::from(on("italic")) << 1);
        add.push((format!("{key}FontStyle"), bits.to_string()));
    }
    for (k, v) in add {
        style.children.retain(|c| c.name != k);
        style.children.push(text_el(&k, v));
    }
}

/// What pass 1 found in a staff.
struct Found {
    items: Vec<Vec<Item>>,
    kept: Vec<Vec<Element>>,
    tuplets: Vec<Vec<Element>>,
    starts: Vec<i64>,
    lines: HashMap<String, Line>,
    slur_defs: HashMap<String, Element>,
    /// Slur id → (measure, item, time) where it starts.
    slur_start: HashMap<String, (usize, usize, i64)>,
    /// Slur id → time where it stops.
    slur_end: HashMap<String, i64>,
}

fn analyse(staff: &Element, to_ticks: &dyn Fn(i64) -> i64) -> Found {
    // Pass 1: every element of every measure with its time and voice; lines from start to end.
    let mut items: Vec<Vec<Item>> = Vec::new();
    let mut kept: Vec<Vec<Element>> = Vec::new();
    let mut tuplets: Vec<Vec<Element>> = Vec::new();
    let mut starts: Vec<i64> = Vec::new();
    let mut lines: HashMap<String, Line> = HashMap::new();
    let mut slur_defs: HashMap<String, Element> = HashMap::new();
    let mut measure_start = 0i64;
    let mut sig = (4i64, 4i64);
    for (mi, m) in staff.children.iter().filter(|c| c.name == "Measure").enumerate() {
        let mut here: Vec<Item> = Vec::new();
        let mut keep: Vec<Element> = Vec::new();
        let mut tup_defs: Vec<Element> = Vec::new();
        let mut tup_ratio: HashMap<String, (i64, i64)> = HashMap::new();
        if let Some(ts) = m.children.iter().find(|c| c.name == "TimeSig") {
            sig = (
                ts.child_num("sigN").unwrap_or(4.0).max(1.0) as i64,
                ts.child_num("sigD").unwrap_or(4.0).max(1.0) as i64,
            );
        }
        let len = m
            .attr("len")
            .map(fraction_ticks)
            .filter(|t| *t > 0)
            .unwrap_or(sig.0 * i64::from(TPQ) * 4 / sig.1);
        let mut cur = measure_start;
        starts.push(measure_start);
        for c in &m.children {
            let voice_of = |c: &Element| {
                c.child_num("track")
                    .map(|t| (t.max(0.0) as usize) % 4)
                    .unwrap_or(0)
            };
            match c.name.as_str() {
                "tick" => {
                    if let Ok(t) = c.text.trim().parse::<i64>() {
                        cur = to_ticks(t);
                    }
                }
                "Beam" | "Segment" => {}
                "Tuplet" => {
                    if let Some(id) = c.attr("id") {
                        let a = c.child_num("actualNotes").unwrap_or(3.0).max(1.0) as i64;
                        let n = c.child_num("normalNotes").unwrap_or(2.0).max(1.0) as i64;
                        tup_ratio.insert(id.to_string(), (a, n));
                    }
                    tup_defs.push(c.clone());
                }
                "Slur" => {
                    if let Some(id) = c.attr("id") {
                        slur_defs.insert(id.to_string(), c.clone());
                    }
                }
                "endSpanner" => {
                    if let Some(l) = c.attr("id").and_then(|id| lines.get_mut(id)) {
                        l.end = Some((mi, cur));
                    }
                }
                name if LINES.contains(&name) => {
                    if let Some(id) = c.attr("id") {
                        lines.insert(
                            id.to_string(),
                            Line {
                                measure: mi,
                                time: cur,
                                voice: voice_of(c),
                                kind: name.to_string(),
                                def: c.clone(),
                                end: None,
                            },
                        );
                    }
                }
                name if MEASURE_LEVEL.contains(&name) => keep.push(c.clone()),
                "Chord" | "Rest" => {
                    let mut e = c.clone();
                    let mut len_here = 0;
                    if !is_grace(c) {
                        let dt = c.child_text("durationType").unwrap_or("quarter");
                        let mut d = if dt == "measure" {
                            c.child("duration")
                                .map(|d| match (d.attr("z"), d.attr("n")) {
                                    (Some(z), Some(n)) => fraction_ticks(&format!("{z}/{n}")),
                                    _ => fraction_ticks(&d.text),
                                })
                                .filter(|t| *t > 0)
                                .unwrap_or(len)
                        } else {
                            let base = value_ticks(dt).unwrap_or(i64::from(TPQ));
                            let dots = c.child_num("dots").unwrap_or(0.0).clamp(0.0, 4.0) as u32;
                            let mut add = base;
                            let mut t = base;
                            for _ in 0..dots {
                                add /= 2;
                                t += add;
                            }
                            t
                        };
                        if let Some((a, n)) = c.child_text("Tuplet").and_then(|id| tup_ratio.get(id.trim())) {
                            d = d * n / a.max(&1);
                        }
                        len_here = d;
                    }
                    // A measure rest's length as text, as MuseScore 3 writes it.
                    for k in e.children.iter_mut().filter(|k| k.name == "duration") {
                        if let (Some(z), Some(n)) = (k.attr("z"), k.attr("n")) {
                            k.text = format!("{z}/{n}");
                        }
                    }
                    here.push(Item {
                        time: cur,
                        voice: voice_of(c),
                        el: e,
                        len: len_here,
                    });
                    cur += len_here;
                }
                _ => here.push(Item {
                    time: cur,
                    voice: voice_of(c),
                    el: c.clone(),
                    len: 0,
                }),
            }
        }
        items.push(here);
        kept.push(keep);
        tuplets.push(tup_defs);
        measure_start += len;
    }

    // Slurs: from the chord that starts them to the chord that stops them.
    let mut slur_start: HashMap<String, (usize, usize, i64)> = HashMap::new();
    let mut slur_end: HashMap<String, i64> = HashMap::new();
    for (m, list) in items.iter().enumerate() {
        for (i, it) in list.iter().enumerate() {
            for s in it.el.children.iter().filter(|k| k.name == "Slur") {
                let Some(id) = s.attr("id") else { continue };
                match s.attr("type") {
                    Some("start") => {
                        slur_start.insert(id.to_string(), (m, i, it.time));
                    }
                    Some("stop") => {
                        slur_end.insert(id.to_string(), it.time);
                    }
                    _ => {}
                }
            }
        }
    }
    Found {
        items,
        kept,
        tuplets,
        starts,
        lines,
        slur_defs,
        slur_start,
        slur_end,
    }
}

fn build(staff: &mut Element, f: &mut Found, slur_end: &HashMap<String, i64>) {
    let Found {
        items,
        kept,
        tuplets,
        starts,
        lines,
        slur_defs,
        slur_start,
        ..
    } = f;
    // Spanner elements of the MuseScore 3 layout, by (measure, item index) for slurs and by
    // measure for the other lines.
    let spanner = |kind: &str, def: Option<&Element>, len: i64, measures: Option<usize>| {
        let mut inner = el(kind, def.map(|d| d.children.clone()).unwrap_or_default());
        if kind == "HairPin" {
            // A hairpin written as "cresc." / "dim." text: the dashed-line kinds.
            let text = inner.has("beginText");
            for s in inner.children.iter_mut().filter(|k| k.name == "subtype") {
                if text {
                    s.text = match s.text.trim() {
                        "1" => "3".into(),
                        _ => "2".into(),
                    };
                }
            }
        }
        let mut loc = vec![text_el("fractions", fraction(len))];
        if let Some(n) = measures {
            loc.insert(0, text_el("measures", n.to_string()));
        }
        let mut sp = el("Spanner", vec![inner, el("next", vec![el("location", loc)])]);
        sp.attrs.push(("type".into(), kind.to_string()));
        sp
    };
    let mut slur_at: HashMap<(usize, usize), Vec<Element>> = HashMap::new();
    for (id, (m, i, t)) in slur_start.iter() {
        if let Some(end) = slur_end.get(id) {
            slur_at
                .entry((*m, *i))
                .or_default()
                .push(spanner("Slur", slur_defs.get(id), end - t, None));
        }
    }
    let mut line_at: HashMap<usize, Vec<(i64, usize, Element)>> = HashMap::new();
    for l in lines.values() {
        let Some((em, et)) = l.end else { continue };
        let sp = if l.kind == "Volta" {
            spanner(
                &l.kind,
                Some(&l.def),
                0,
                Some(em.saturating_sub(l.measure).max(1)),
            )
        } else {
            spanner(&l.kind, Some(&l.def), et - l.time, None)
        };
        line_at.entry(l.measure).or_default().push((l.time, l.voice, sp));
    }

    // Pass 2: the measures in the MuseScore 3 layout.
    for (mi, m) in staff
        .children
        .iter_mut()
        .filter(|c| c.name == "Measure")
        .enumerate()
    {
        let measure_start = starts.get(mi).copied().unwrap_or(0);
        let list = items.get_mut(mi).map(std::mem::take).unwrap_or_default();
        let mut voices: Vec<Vec<(i64, usize, Element, i64)>> = vec![Vec::new(); 4];
        for (i, it) in list.into_iter().enumerate() {
            let v = it.voice.min(3);
            if let Some(sps) = slur_at.remove(&(mi, i)) {
                for sp in sps {
                    voices[v].push((it.time, 0, sp, 0));
                }
            }
            let mut e = it.el;
            e.children.retain(|k| k.name != "track" && k.name != "Slur");
            voices[v].push((it.time, 1, e, it.len));
        }
        for (t, v, sp) in line_at.remove(&mi).unwrap_or_default() {
            voices[v.min(3)].push((t, 0, sp, 0));
        }
        let used = voices.iter().rposition(|v| !v.is_empty()).unwrap_or(0);
        let mut out: Vec<Element> = kept.get(mi).cloned().unwrap_or_default();
        for mut list in voices.into_iter().take(used + 1) {
            // Stable by time: lines before the chord they start at.
            list.sort_by_key(|(t, order, _, _)| (*t, *order));
            let mut children: Vec<Element> = tuplets.get(mi).cloned().unwrap_or_default();
            let mut pos = measure_start;
            for (t, _, e, len) in list {
                if t != pos {
                    children.push(location(t - pos));
                    pos = t;
                }
                children.push(e);
                pos += len;
            }
            out.push(el("voice", children));
        }
        m.children = out;
    }
}
