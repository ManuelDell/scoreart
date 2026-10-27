//! [`Score`] → MusicXML 4.0 score-partwise.

use std::collections::HashMap;
use std::fmt::Write as _;

use scoreart_model::marks::XmlGroup;
use scoreart_model::{
    Accidental, Arpeggio, Barline, Clef, Event, EventKind, Jump, MeasureInfo, Pitch, Score, Spanner,
    SpannerKind, TPQ, TimeSymbol, Tremolo,
};

use crate::dom::esc;

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// The coarsest `<divisions>` that represents every duration in the score exactly.
fn divisions(score: &Score) -> u32 {
    let mut g = TPQ;
    for p in &score.parts {
        for pm in &p.measures {
            for sm in &pm.staves {
                for v in &sm.voices {
                    for e in &v.events {
                        g = gcd(g, e.ticks());
                    }
                }
            }
        }
    }
    (TPQ / g.max(1)).max(1)
}

fn clef_xml(c: Clef, number: Option<usize>) -> String {
    let (sign, line, oct) = match c {
        Clef::Treble => ("G", 2, 0),
        Clef::Treble8vb => ("G", 2, -1),
        Clef::Bass => ("F", 4, 0),
        Clef::Alto => ("C", 3, 0),
        Clef::Tenor => ("C", 4, 0),
        Clef::Percussion => ("percussion", 0, 0),
    };
    let num = number
        .map(|n| format!(" number=\"{}\"", n + 1))
        .unwrap_or_default();
    let mut s = format!("<clef{num}><sign>{sign}</sign>");
    if line > 0 {
        let _ = write!(s, "<line>{line}</line>");
    }
    if oct != 0 {
        let _ = write!(s, "<clef-octave-change>{oct}</clef-octave-change>");
    }
    s.push_str("</clef>");
    s
}

/// Lines written on notes (`<notations>`) rather than as directions.
fn on_note(k: SpannerKind) -> bool {
    matches!(
        k,
        SpannerKind::Slur
            | SpannerKind::Glissando
            | SpannerKind::WavyGlissando
            | SpannerKind::TrillLine
            | SpannerKind::Vibrato
    )
}

/// Spanner endpoints of one part keyed by (staff, voice, measure, tick).
#[derive(Default)]
struct Lines<'a> {
    starts: HashMap<(usize, usize, usize, u32), Vec<(&'a Spanner, usize)>>,
    stops: HashMap<(usize, usize, usize, u32), Vec<(&'a Spanner, usize)>>,
}

fn lines_of(score: &Score, part: usize) -> Lines<'_> {
    let mut l = Lines::default();
    let mut count: HashMap<&'static str, usize> = HashMap::new();
    for sp in score.spanners.iter().filter(|s| s.part == part) {
        let family = match sp.kind {
            SpannerKind::Slur => "slur",
            SpannerKind::Glissando | SpannerKind::WavyGlissando => "gliss",
            SpannerKind::TrillLine | SpannerKind::Vibrato => "wavy",
            SpannerKind::Crescendo | SpannerKind::Diminuendo => "wedge",
            SpannerKind::Ottava(_) => "octave",
            SpannerKind::Pedal => "pedal",
            _ => "dashes",
        };
        let c = count.entry(family).or_insert(0);
        let number = *c % 6 + 1;
        *c += 1;
        l.starts
            .entry((sp.staff, sp.voice, sp.start.measure, sp.start.tick))
            .or_default()
            .push((sp, number));
        l.stops
            .entry((sp.staff, sp.voice, sp.end.measure, sp.end.tick))
            .or_default()
            .push((sp, number));
    }
    l
}

fn direction(x: &mut String, placement: &str, body: &str, staff: Option<usize>, extra: &str) {
    let staff_el = staff.map(|s| format!("<staff>{s}</staff>")).unwrap_or_default();
    let _ = writeln!(
        x,
        "      <direction placement=\"{placement}\"{extra}><direction-type>{body}</direction-type>{staff_el}</direction>"
    );
}

fn line_start_direction(x: &mut String, sp: &Spanner, number: usize, staff: Option<usize>) {
    match sp.kind {
        SpannerKind::Crescendo => direction(
            x,
            "below",
            &format!("<wedge type=\"crescendo\" number=\"{number}\"/>"),
            staff,
            "",
        ),
        SpannerKind::Diminuendo => direction(
            x,
            "below",
            &format!("<wedge type=\"diminuendo\" number=\"{number}\"/>"),
            staff,
            "",
        ),
        SpannerKind::Ottava(o) => {
            let size = match o.abs() {
                2 => 15,
                3 => 22,
                _ => 8,
            };
            let (ty, place) = if o > 0 { ("down", "above") } else { ("up", "below") };
            direction(
                x,
                place,
                &format!("<octave-shift type=\"{ty}\" size=\"{size}\" number=\"{number}\"/>"),
                staff,
                "",
            );
        }
        SpannerKind::Pedal => direction(x, "below", "<pedal type=\"start\" line=\"no\"/>", staff, ""),
        SpannerKind::CrescendoLine
        | SpannerKind::DiminuendoLine
        | SpannerKind::LetRing
        | SpannerKind::PalmMute => {
            let (word, place) = match sp.kind {
                SpannerKind::CrescendoLine => ("cresc.", "below"),
                SpannerKind::DiminuendoLine => ("dim.", "below"),
                SpannerKind::LetRing => ("let ring", "above"),
                _ => ("P.M.", "above"),
            };
            let _ = writeln!(
                x,
                "      <direction placement=\"{place}\"><direction-type><words>{word}</words></direction-type><direction-type><dashes type=\"start\" number=\"{number}\"/></direction-type>{}</direction>",
                staff.map(|s| format!("<staff>{s}</staff>")).unwrap_or_default()
            );
        }
        _ => {}
    }
}

fn line_stop_direction(x: &mut String, sp: &Spanner, number: usize, staff: Option<usize>) {
    match sp.kind {
        SpannerKind::Crescendo | SpannerKind::Diminuendo => direction(
            x,
            "below",
            &format!("<wedge type=\"stop\" number=\"{number}\"/>"),
            staff,
            "",
        ),
        SpannerKind::Ottava(o) => {
            let size = match o.abs() {
                2 => 15,
                3 => 22,
                _ => 8,
            };
            direction(
                x,
                "above",
                &format!("<octave-shift type=\"stop\" size=\"{size}\" number=\"{number}\"/>"),
                staff,
                "",
            );
        }
        SpannerKind::Pedal => direction(x, "below", "<pedal type=\"stop\" line=\"no\"/>", staff, ""),
        SpannerKind::CrescendoLine
        | SpannerKind::DiminuendoLine
        | SpannerKind::LetRing
        | SpannerKind::PalmMute => direction(
            x,
            "below",
            &format!("<dashes type=\"stop\" number=\"{number}\"/>"),
            staff,
            "",
        ),
        _ => {}
    }
}

/// `<harmony>` from chord-symbol text such as "F#m7/C#".
fn harmony(x: &mut String, text: &str, staff: Option<usize>) {
    let mut chars = text.chars().peekable();
    let Some(root) = chars
        .next()
        .filter(|c| matches!(c.to_ascii_uppercase(), 'A'..='G'))
    else {
        // Not a parseable chord: keep it as text.
        direction(x, "above", &format!("<words>{}</words>", esc(text)), staff, "");
        return;
    };
    let mut alter = 0;
    if let Some(c) = chars.peek().copied()
        && (c == '#' || c == 'b' || c == '♯' || c == '♭')
    {
        alter = if c == '#' || c == '♯' { 1 } else { -1 };
        chars.next();
    }
    let rest: String = chars.collect();
    let (suffix, bass) = match rest.rsplit_once('/') {
        Some((s, b))
            if b.chars()
                .next()
                .is_some_and(|c| matches!(c.to_ascii_uppercase(), 'A'..='G')) =>
        {
            (s.to_string(), Some(b.to_string()))
        }
        _ => (rest.clone(), None),
    };
    let kind = match suffix.as_str() {
        "" => "major",
        "m" | "min" | "-" => "minor",
        "7" => "dominant",
        "maj7" | "M7" => "major-seventh",
        "m7" | "min7" | "-7" => "minor-seventh",
        "dim" | "o" => "diminished",
        "dim7" | "o7" => "diminished-seventh",
        "+" | "aug" => "augmented",
        "m7b5" | "ø" => "half-diminished",
        "sus4" | "sus" => "suspended-fourth",
        "sus2" => "suspended-second",
        "6" => "major-sixth",
        "m6" => "minor-sixth",
        "9" => "dominant-ninth",
        "5" => "power",
        _ => "other",
    };
    let mut h = format!(
        "      <harmony><root><root-step>{}</root-step>",
        root.to_ascii_uppercase()
    );
    if alter != 0 {
        let _ = write!(h, "<root-alter>{alter}</root-alter>");
    }
    let _ = write!(h, "</root><kind text=\"{}\">{kind}</kind>", esc(&suffix));
    if let Some(b) = bass {
        let mut bc = b.chars();
        if let Some(step) = bc.next() {
            let _ = write!(h, "<bass><bass-step>{}</bass-step>", step.to_ascii_uppercase());
            match bc.next() {
                Some('#') | Some('♯') => h.push_str("<bass-alter>1</bass-alter>"),
                Some('b') | Some('♭') => h.push_str("<bass-alter>-1</bass-alter>"),
                _ => {}
            }
            h.push_str("</bass>");
        }
    }
    if let Some(s) = staff {
        let _ = write!(h, "<staff>{s}</staff>");
    }
    h.push_str("</harmony>\n");
    x.push_str(&h);
}

const STANDARD_DYNAMICS: &[&str] = &[
    "p", "pp", "ppp", "pppp", "ppppp", "pppppp", "f", "ff", "fff", "ffff", "fffff", "ffffff", "mp", "mf",
    "sf", "sfp", "sfpp", "fp", "rf", "rfz", "sfz", "sffz", "fz", "n", "pf",
];

fn jump_words(j: Jump) -> Option<(&'static str, &'static str)> {
    let sound = match j {
        Jump::Fine => " fine=\"yes\"",
        Jump::DaCapo | Jump::DaCapoAlFine | Jump::DaCapoAlCoda => " dacapo=\"yes\"",
        Jump::DalSegno | Jump::DalSegnoAlFine | Jump::DalSegnoAlCoda => " dalsegno=\"1\"",
        Jump::ToCoda => " tocoda=\"1\"",
        _ => "",
    };
    j.text().map(|t| (t, sound))
}

pub fn write(score: &Score) -> String {
    let div = divisions(score);
    let per_tick = |t: u32| -> u32 { t * div / TPQ };
    let mut x = String::new();
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n");
    x.push_str("<!DOCTYPE score-partwise PUBLIC \"-//Recordare//DTD MusicXML 4.0 Partwise//EN\" \"http://www.musicxml.org/dtds/partwise.dtd\">\n");
    x.push_str("<score-partwise version=\"4.0\">\n");
    let m = &score.meta;
    if !m.title.is_empty() {
        let _ = writeln!(x, "  <work><work-title>{}</work-title></work>", esc(&m.title));
    }
    if !m.subtitle.is_empty() {
        let _ = writeln!(x, "  <movement-title>{}</movement-title>", esc(&m.subtitle));
    }
    x.push_str("  <identification>\n");
    if !m.composer.is_empty() {
        let _ = writeln!(x, "    <creator type=\"composer\">{}</creator>", esc(&m.composer));
    }
    if !m.lyricist.is_empty() {
        let _ = writeln!(x, "    <creator type=\"lyricist\">{}</creator>", esc(&m.lyricist));
    }
    if !m.copyright.is_empty() {
        let _ = writeln!(x, "    <rights>{}</rights>", esc(&m.copyright));
    }
    let _ = writeln!(
        x,
        "    <encoding><software>ScoreArt {}</software><supports element=\"accidental\" type=\"no\"/><supports element=\"stem\" type=\"no\"/></encoding>",
        env!("CARGO_PKG_VERSION")
    );
    x.push_str("  </identification>\n  <part-list>\n");
    for p in &score.parts {
        let _ = writeln!(x, "    <score-part id=\"{}\">", esc(&p.id));
        let _ = writeln!(x, "      <part-name>{}</part-name>", esc(&p.name));
        if !p.abbreviation.is_empty() {
            let _ = writeln!(
                x,
                "      <part-abbreviation>{}</part-abbreviation>",
                esc(&p.abbreviation)
            );
        }
        // The chosen sound library and preset travel as a virtual instrument.
        let virtual_instrument = if p.sound.library.is_empty() {
            String::new()
        } else {
            format!(
                "<virtual-instrument><virtual-library>{}</virtual-library><virtual-name>{}</virtual-name></virtual-instrument>",
                esc(&p.sound.library),
                esc(&p.sound.preset)
            )
        };
        let _ = writeln!(
            x,
            "      <score-instrument id=\"{}-I1\"><instrument-name>{}</instrument-name>{virtual_instrument}</score-instrument>",
            esc(&p.id),
            esc(&p.name)
        );
        let _ = writeln!(
            x,
            "      <midi-instrument id=\"{}-I1\"><midi-program>{}</midi-program><volume>{}</volume><pan>{}</pan></midi-instrument>",
            esc(&p.id),
            u32::from(p.program) + 1,
            p.sound.volume.min(100),
            // MusicXML pans in degrees: -90 left … 90 right.
            i32::from(p.sound.pan.clamp(-100, 100)) * 9 / 10
        );
        x.push_str("    </score-part>\n");
    }
    x.push_str("  </part-list>\n");

    for (pi, p) in score.parts.iter().enumerate() {
        let _ = writeln!(x, "  <part id=\"{}\">", esc(&p.id));
        let staves = p.staff_count();
        let lines = lines_of(score, pi);
        // Pitches tied into the next event, per (staff, voice), across barlines.
        let mut tied: HashMap<(usize, usize), Vec<Pitch>> = HashMap::new();
        for (mi, pm) in p.measures.iter().enumerate() {
            let info = score.measures.get(mi).cloned().unwrap_or_default();
            let prev_info = mi.checked_sub(1).and_then(|i| score.measures.get(i));
            let next_info = score.measures.get(mi + 1);
            let len = score.measure_ticks(mi);
            let _ = writeln!(x, "    <measure number=\"{}\">", mi + 1);
            // Left barline: forward repeat and the start of a volta.
            let volta_start = info.volta.filter(|v| prev_info.and_then(|p| p.volta) != Some(*v));
            if info.repeat_start || volta_start.is_some() {
                x.push_str("      <barline location=\"left\">");
                if info.repeat_start {
                    x.push_str("<bar-style>heavy-light</bar-style>");
                }
                if let Some(v) = volta_start {
                    let _ = write!(x, "<ending number=\"{v}\" type=\"start\">{v}.</ending>");
                }
                if info.repeat_start {
                    x.push_str("<repeat direction=\"forward\"/>");
                }
                x.push_str("</barline>\n");
            }
            // Attributes on the first measure and on changes.
            let clef_changes: Vec<(usize, Clef)> = pm
                .staves
                .iter()
                .enumerate()
                .filter_map(|(s, sm)| sm.clef.map(|c| (s, c)))
                .collect();
            let prev_pm = mi.checked_sub(1).and_then(|i| p.measures.get(i));
            let repeat_changes: Vec<(usize, bool)> = pm
                .staves
                .iter()
                .enumerate()
                .filter_map(|(s, sm)| {
                    let before = prev_pm
                        .and_then(|q| q.staves.get(s))
                        .is_some_and(|q| q.measure_repeat);
                    (sm.measure_repeat != before).then_some((s, sm.measure_repeat))
                })
                .collect();
            if mi == 0
                || info.key.is_some()
                || info.time.is_some()
                || !clef_changes.is_empty()
                || !repeat_changes.is_empty()
            {
                x.push_str("      <attributes>");
                if mi == 0 {
                    let _ = write!(x, "<divisions>{div}</divisions>");
                }
                if let Some(k) = info.key {
                    let _ = write!(x, "<key><fifths>{}</fifths></key>", k.fifths);
                }
                if let Some(t) = info.time {
                    let sym = match t.symbol {
                        TimeSymbol::Common => " symbol=\"common\"",
                        TimeSymbol::Cut => " symbol=\"cut\"",
                        TimeSymbol::Normal => "",
                    };
                    let _ = write!(
                        x,
                        "<time{sym}><beats>{}</beats><beat-type>{}</beat-type></time>",
                        t.beats, t.beat_type
                    );
                }
                if mi == 0 {
                    if staves > 1 {
                        let _ = write!(x, "<staves>{staves}</staves>");
                    }
                    for (s, c) in p.clefs.iter().enumerate() {
                        x.push_str(&clef_xml(*c, (staves > 1).then_some(s)));
                    }
                } else {
                    for (s, c) in &clef_changes {
                        x.push_str(&clef_xml(*c, (staves > 1).then_some(*s)));
                    }
                }
                for (s, on) in &repeat_changes {
                    let num = if staves > 1 {
                        format!(" number=\"{}\"", s + 1)
                    } else {
                        String::new()
                    };
                    let ty = if *on { "start" } else { "stop" };
                    let _ = write!(
                        x,
                        "<measure-style{num}><measure-repeat type=\"{ty}\">1</measure-repeat></measure-style>"
                    );
                }
                x.push_str("</attributes>\n");
            }
            if pi == 0 {
                write_measure_directions(&mut x, &info, score.tempo_at(mi));
            }
            let mut first_block = true;
            for (s, sm) in pm.staves.iter().enumerate() {
                for (v, voice) in sm.voices.iter().enumerate() {
                    if !first_block {
                        let _ = writeln!(x, "      <backup><duration>{}</duration></backup>", per_tick(len));
                    }
                    first_block = false;
                    let number = s * 4 + v + 1;
                    let staff_no = (staves > 1).then_some(s + 1);
                    for (e, on) in voice.events.iter().zip(voice.onsets()) {
                        let key = (s, v, mi, on);
                        let starts = lines.starts.get(&key).cloned().unwrap_or_default();
                        let stops = lines.stops.get(&key).cloned().unwrap_or_default();
                        for (sp, n) in starts.iter().filter(|(sp, _)| !on_note(sp.kind)) {
                            line_start_direction(&mut x, sp, *n, staff_no);
                        }
                        let tie_stops = tied.remove(&(s, v)).unwrap_or_default();
                        write_event(
                            &mut x,
                            e,
                            per_tick(e.ticks()),
                            number,
                            staff_no,
                            &tie_stops,
                            &starts,
                            &stops,
                        );
                        for (sp, n) in stops.iter().filter(|(sp, _)| !on_note(sp.kind)) {
                            line_stop_direction(&mut x, sp, *n, staff_no);
                        }
                        let tie_starts: Vec<_> =
                            e.notes().iter().filter(|n| n.tie).map(|n| n.pitch).collect();
                        if !tie_starts.is_empty() {
                            tied.insert((s, v), tie_starts);
                        }
                    }
                }
            }
            if pi == 0 {
                for j in info.jumps.iter().filter(|j| j.at_end()) {
                    if let Some((t, sound)) = jump_words(*j) {
                        let _ = writeln!(
                            x,
                            "      <direction placement=\"above\"><direction-type><words font-style=\"italic\">{t}</words></direction-type><sound{sound}/></direction>"
                        );
                    }
                }
            }
            // Right barline: style, end of a volta, backward repeat.
            let volta_end = info.volta.filter(|v| next_info.and_then(|n| n.volta) != Some(*v));
            let style = match info.barline {
                Barline::Regular => None,
                Barline::Double => Some("light-light"),
                Barline::Final | Barline::RepeatEnd => Some("light-heavy"),
            };
            if style.is_some() || volta_end.is_some() {
                x.push_str("      <barline location=\"right\">");
                if let Some(st) = style {
                    let _ = write!(x, "<bar-style>{st}</bar-style>");
                }
                if let Some(v) = volta_end {
                    // A closing hook unless it's a last ending that runs on.
                    let ty = if info.barline == Barline::RepeatEnd || v == 1 {
                        "stop"
                    } else {
                        "discontinue"
                    };
                    let _ = write!(x, "<ending number=\"{v}\" type=\"{ty}\"/>");
                }
                if info.barline == Barline::RepeatEnd {
                    x.push_str("<repeat direction=\"backward\"/>");
                }
                x.push_str("</barline>\n");
            }
            x.push_str("    </measure>\n");
        }
        x.push_str("  </part>\n");
    }
    x.push_str("</score-partwise>\n");
    x
}

fn write_measure_directions(x: &mut String, info: &MeasureInfo, effective_tempo: u16) {
    if let Some(r) = &info.rehearsal {
        direction(
            x,
            "above",
            &format!("<rehearsal enclosure=\"square\">{}</rehearsal>", esc(r)),
            None,
            "",
        );
    }
    for j in info.jumps.iter().filter(|j| !j.at_end()) {
        let body = match j {
            Jump::Segno => "<segno/>",
            Jump::Coda => "<coda/>",
            _ => "<coda smufl=\"codaSquare\"/>",
        };
        direction(x, "above", body, None, "");
    }
    if info.tempo.is_some() || info.tempo_text.is_some() {
        let mut body = String::new();
        if let Some(t) = &info.tempo_text {
            let _ = write!(
                body,
                "<words font-weight=\"bold\">{}</words></direction-type><direction-type>",
                esc(t)
            );
        }
        if let Some(t) = info.tempo {
            let _ = write!(
                body,
                "<metronome><beat-unit>quarter</beat-unit><per-minute>{}</per-minute></metronome>",
                t.bpm
            );
        } else if body.ends_with("<direction-type>") {
            body.truncate(body.len() - "</direction-type><direction-type>".len());
        }
        // A tempo word alone still carries the tempo in effect, so readers know it is a tempo mark.
        let sound = format!(
            "<sound tempo=\"{}\"/>",
            info.tempo.map(|t| t.bpm).unwrap_or(effective_tempo)
        );
        let _ = writeln!(
            x,
            "      <direction placement=\"above\"><direction-type>{body}</direction-type>{sound}</direction>"
        );
    }
    if let Some(text) = &info.text {
        direction(
            x,
            "above",
            &format!("<words font-weight=\"bold\">{}</words>", esc(text)),
            None,
            " system=\"only-top\"",
        );
    }
}

fn mark_xml(a: scoreart_model::Articulation) -> String {
    let d = a.def();
    match d.xml_attr {
        Some(("value", v)) => format!("<{}>{v}</{}>", d.xml_name, d.xml_name),
        Some((k, v)) => format!("<{} {k}=\"{v}\"/>", d.xml_name),
        None => format!("<{}/>", d.xml_name),
    }
}

fn accidental_name(a: Accidental) -> &'static str {
    match a {
        Accidental::DoubleFlat => "flat-flat",
        Accidental::Flat => "flat",
        Accidental::Natural => "natural",
        Accidental::Sharp => "sharp",
        Accidental::DoubleSharp => "double-sharp",
    }
}

#[allow(clippy::too_many_arguments)]
fn write_event(
    x: &mut String,
    e: &Event,
    dur: u32,
    voice: usize,
    staff: Option<usize>,
    tie_stops: &[Pitch],
    starts: &[(&Spanner, usize)],
    stops: &[(&Spanner, usize)],
) {
    if let Some(d) = e.dynamic {
        let body = if STANDARD_DYNAMICS.contains(&d.id()) {
            format!("<dynamics><{}/></dynamics>", d.id())
        } else {
            format!("<dynamics><other-dynamics>{}</other-dynamics></dynamics>", d.id())
        };
        direction(x, "below", &body, staff, "");
    }
    if let Some(t) = &e.text {
        direction(
            x,
            "above",
            &format!("<words font-style=\"italic\">{}</words>", esc(t)),
            staff,
            "",
        );
    }
    if let Some(c) = &e.chord_symbol {
        harmony(x, c, staff);
    }
    let staff_el = staff.map(|s| format!("<staff>{s}</staff>")).unwrap_or_default();
    // Grace notes come first.
    for g in &e.grace {
        for (i, p) in g.pitches.iter().enumerate() {
            let slash = if g.slash { " slash=\"yes\"" } else { "" };
            let chord = if i > 0 { "<chord/>" } else { "" };
            let _ = write!(
                x,
                "      <note><grace{slash}/>{chord}<pitch><step>{}</step>",
                p.step.letter()
            );
            if p.alter != 0 {
                let _ = write!(x, "<alter>{}</alter>", p.alter);
            }
            let _ = writeln!(
                x,
                "<octave>{}</octave></pitch><voice>{voice}</voice><type>{}</type>{staff_el}</note>",
                p.octave,
                g.value.xml_name()
            );
        }
    }
    let note_lines = |first: bool| -> String {
        let mut n = String::new();
        if !first {
            return n;
        }
        for (sp, num) in stops {
            match sp.kind {
                SpannerKind::Slur => {
                    let _ = write!(n, "<slur type=\"stop\" number=\"{num}\"/>");
                }
                SpannerKind::Glissando => {
                    let _ = write!(
                        n,
                        "<glissando type=\"stop\" number=\"{num}\" line-type=\"solid\"/>"
                    );
                }
                SpannerKind::WavyGlissando => {
                    let _ = write!(
                        n,
                        "<glissando type=\"stop\" number=\"{num}\" line-type=\"wavy\"/>"
                    );
                }
                _ => {}
            }
        }
        for (sp, num) in starts {
            match sp.kind {
                SpannerKind::Slur => {
                    let _ = write!(n, "<slur type=\"start\" number=\"{num}\"/>");
                }
                SpannerKind::Glissando => {
                    let _ = write!(
                        n,
                        "<glissando type=\"start\" number=\"{num}\" line-type=\"solid\"/>"
                    );
                }
                SpannerKind::WavyGlissando => {
                    let _ = write!(
                        n,
                        "<glissando type=\"start\" number=\"{num}\" line-type=\"wavy\"/>"
                    );
                }
                _ => {}
            }
        }
        n
    };
    let ornaments = |first: bool| -> String {
        let mut o = String::new();
        if first {
            for a in e
                .articulations
                .iter()
                .filter(|a| a.def().xml == XmlGroup::Ornaments)
            {
                o.push_str(&mark_xml(*a));
            }
            for (sp, num) in starts {
                match sp.kind {
                    SpannerKind::TrillLine => {
                        let _ = write!(o, "<trill-mark/><wavy-line type=\"start\" number=\"{num}\"/>");
                    }
                    SpannerKind::Vibrato => {
                        let _ = write!(
                            o,
                            "<wavy-line type=\"start\" number=\"{num}\" smufl=\"wiggleVibrato\"/>"
                        );
                    }
                    _ => {}
                }
            }
            for (sp, num) in stops {
                if matches!(sp.kind, SpannerKind::TrillLine | SpannerKind::Vibrato) {
                    let _ = write!(o, "<wavy-line type=\"stop\" number=\"{num}\"/>");
                }
            }
            match e.tremolo {
                Tremolo::None => {}
                Tremolo::Strokes(n) => {
                    let _ = write!(o, "<tremolo type=\"single\">{}</tremolo>", n.clamp(1, 4));
                }
                Tremolo::Buzz => o.push_str("<tremolo type=\"unmeasured\">0</tremolo>"),
            }
        }
        o
    };
    let tail = |x: &mut String, note: Option<&scoreart_model::Note>, tie: bool, stop: bool, first: bool| {
        let _ = write!(
            x,
            "<voice>{voice}</voice><type>{}</type>",
            e.duration.value.xml_name()
        );
        for _ in 0..e.duration.dots {
            x.push_str("<dot/>");
        }
        if let Some(n) = note
            && let Some(a) = n.forced
        {
            let _ = write!(
                x,
                "<accidental cautionary=\"yes\">{}</accidental>",
                accidental_name(a)
            );
        }
        if let Some(t) = e.duration.tuplet {
            let _ = write!(
                x,
                "<time-modification><actual-notes>{}</actual-notes><normal-notes>{}</normal-notes></time-modification>",
                t.actual, t.normal
            );
        }
        if let Some(n) = note
            && (n.head != scoreart_model::NoteHead::Normal || n.paren)
        {
            let paren = if n.paren { " parentheses=\"yes\"" } else { "" };
            let _ = write!(x, "<notehead{paren}>{}</notehead>", n.head.id());
        }
        x.push_str(&staff_el);
        let mut notations = String::new();
        if stop {
            notations.push_str("<tied type=\"stop\"/>");
        }
        if tie {
            notations.push_str("<tied type=\"start\"/>");
        }
        notations.push_str(&note_lines(first));
        let orn = ornaments(first);
        if !orn.is_empty() {
            let _ = write!(notations, "<ornaments>{orn}</ornaments>");
        }
        let mut tech = String::new();
        if first {
            for a in e
                .articulations
                .iter()
                .filter(|a| a.def().xml == XmlGroup::Technical)
            {
                tech.push_str(&mark_xml(*a));
            }
        }
        if let Some(f) = note.and_then(|n| n.fingering.as_ref()) {
            let _ = write!(tech, "<fingering>{}</fingering>", esc(f));
        }
        if !tech.is_empty() {
            let _ = write!(notations, "<technical>{tech}</technical>");
        }
        if first {
            let arts: String = e
                .articulations
                .iter()
                .filter(|a| a.def().xml == XmlGroup::Articulations)
                .map(|a| mark_xml(*a))
                .collect();
            if !arts.is_empty() {
                let _ = write!(notations, "<articulations>{arts}</articulations>");
            }
            for a in e.articulations.iter().filter(|a| a.is_fermata()) {
                let shape = a.def().xml_name;
                if shape == "normal" {
                    notations.push_str("<fermata type=\"upright\"/>");
                } else {
                    let _ = write!(notations, "<fermata type=\"upright\">{shape}</fermata>");
                }
            }
        }
        match e.arpeggio {
            Arpeggio::None => {}
            Arpeggio::Normal => notations.push_str("<arpeggiate/>"),
            Arpeggio::Up => notations.push_str("<arpeggiate direction=\"up\"/>"),
            Arpeggio::Down => notations.push_str("<arpeggiate direction=\"down\"/>"),
            Arpeggio::Bracket => notations.push_str("<non-arpeggiate type=\"bottom\"/>"),
        }
        if !notations.is_empty() {
            let _ = write!(x, "<notations>{notations}</notations>");
        }
        if first && let Some(l) = &e.lyric {
            let _ = write!(
                x,
                "<lyric number=\"1\"><syllabic>single</syllabic><text>{}</text></lyric>",
                esc(l)
            );
        }
    };
    match &e.kind {
        EventKind::Rest => {
            let _ = write!(x, "      <note><rest/><duration>{dur}</duration>");
            tail(x, None, false, false, true);
            x.push_str("</note>\n");
        }
        EventKind::Chord(notes) => {
            for (i, n) in notes.iter().enumerate() {
                x.push_str("      <note>");
                if i > 0 {
                    x.push_str("<chord/>");
                }
                let _ = write!(x, "<pitch><step>{}</step>", n.pitch.step.letter());
                if n.pitch.alter != 0 {
                    let _ = write!(x, "<alter>{}</alter>", n.pitch.alter);
                }
                let _ = write!(
                    x,
                    "<octave>{}</octave></pitch><duration>{dur}</duration>",
                    n.pitch.octave
                );
                let stop = tie_stops.contains(&n.pitch);
                if stop {
                    x.push_str("<tie type=\"stop\"/>");
                }
                if n.tie {
                    x.push_str("<tie type=\"start\"/>");
                }
                tail(x, Some(n), n.tie, stop, i == 0);
                x.push_str("</note>\n");
            }
        }
    }
}
