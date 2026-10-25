//! [`Score`] → MusicXML 4.0 score-partwise.

use std::fmt::Write as _;

use scoreart_model::{Articulation, Barline, Clef, Event, EventKind, Score, TPQ, TimeSymbol};

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
        let _ = writeln!(
            x,
            "      <score-instrument id=\"{}-I1\"><instrument-name>{}</instrument-name></score-instrument>",
            esc(&p.id),
            esc(&p.name)
        );
        let _ = writeln!(
            x,
            "      <midi-instrument id=\"{}-I1\"><midi-program>{}</midi-program></midi-instrument>",
            esc(&p.id),
            u32::from(p.program) + 1
        );
        x.push_str("    </score-part>\n");
    }
    x.push_str("  </part-list>\n");

    for (pi, p) in score.parts.iter().enumerate() {
        let _ = writeln!(x, "  <part id=\"{}\">", esc(&p.id));
        let staves = p.staff_count();
        // Pitches tied into the next event, per (staff, voice), across barlines.
        let mut tied: std::collections::HashMap<(usize, usize), Vec<scoreart_model::Pitch>> =
            Default::default();
        for (mi, pm) in p.measures.iter().enumerate() {
            let info = score.measures.get(mi).cloned().unwrap_or_default();
            let len = score.measure_ticks(mi);
            let _ = writeln!(x, "    <measure number=\"{}\">", mi + 1);
            if info.repeat_start {
                x.push_str("      <barline location=\"left\"><bar-style>heavy-light</bar-style><repeat direction=\"forward\"/></barline>\n");
            }
            // Attributes on the first measure and on changes.
            let clef_changes: Vec<(usize, Clef)> = pm
                .staves
                .iter()
                .enumerate()
                .filter_map(|(s, sm)| sm.clef.map(|c| (s, c)))
                .collect();
            if mi == 0 || info.key.is_some() || info.time.is_some() || !clef_changes.is_empty() {
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
                x.push_str("</attributes>\n");
            }
            if pi == 0 {
                if let Some(t) = info.tempo {
                    let _ = writeln!(
                        x,
                        "      <direction placement=\"above\"><direction-type><metronome><beat-unit>quarter</beat-unit><per-minute>{}</per-minute></metronome></direction-type><sound tempo=\"{}\"/></direction>",
                        t.bpm, t.bpm
                    );
                }
                if let Some(text) = &info.text {
                    let _ = writeln!(
                        x,
                        "      <direction placement=\"above\"><direction-type><words>{}</words></direction-type></direction>",
                        esc(text)
                    );
                }
            }
            let mut first_block = true;
            for (s, sm) in pm.staves.iter().enumerate() {
                for (v, voice) in sm.voices.iter().enumerate() {
                    if !first_block {
                        let _ = writeln!(x, "      <backup><duration>{}</duration></backup>", per_tick(len));
                    }
                    first_block = false;
                    let number = s * 4 + v + 1;
                    for e in &voice.events {
                        let stops = tied.remove(&(s, v)).unwrap_or_default();
                        write_event(
                            &mut x,
                            e,
                            per_tick(e.ticks()),
                            number,
                            (staves > 1).then_some(s + 1),
                            &stops,
                        );
                        let starts: Vec<_> = e.notes().iter().filter(|n| n.tie).map(|n| n.pitch).collect();
                        if !starts.is_empty() {
                            tied.insert((s, v), starts);
                        }
                    }
                }
            }
            match info.barline {
                Barline::Regular => {}
                Barline::Double => x.push_str("      <barline location=\"right\"><bar-style>light-light</bar-style></barline>\n"),
                Barline::Final => x.push_str("      <barline location=\"right\"><bar-style>light-heavy</bar-style></barline>\n"),
                Barline::RepeatEnd => x.push_str(
                    "      <barline location=\"right\"><bar-style>light-heavy</bar-style><repeat direction=\"backward\"/></barline>\n",
                ),
            }
            x.push_str("    </measure>\n");
        }
        x.push_str("  </part>\n");
    }
    x.push_str("</score-partwise>\n");
    x
}

fn write_event(
    x: &mut String,
    e: &Event,
    dur: u32,
    voice: usize,
    staff: Option<usize>,
    stops: &[scoreart_model::Pitch],
) {
    if let Some(d) = e.dynamic {
        let staff_el = staff.map(|s| format!("<staff>{s}</staff>")).unwrap_or_default();
        let _ = writeln!(
            x,
            "      <direction placement=\"below\"><direction-type><dynamics><{}/></dynamics></direction-type>{staff_el}</direction>",
            d.id()
        );
    }
    let common = |x: &mut String| {
        let _ = write!(x, "<duration>{dur}</duration>");
    };
    let tail = |x: &mut String, tie: bool, stop: bool, first: bool| {
        let _ = write!(
            x,
            "<voice>{voice}</voice><type>{}</type>",
            e.duration.value.xml_name()
        );
        for _ in 0..e.duration.dots {
            x.push_str("<dot/>");
        }
        if let Some(t) = e.duration.tuplet {
            let _ = write!(
                x,
                "<time-modification><actual-notes>{}</actual-notes><normal-notes>{}</normal-notes></time-modification>",
                t.actual, t.normal
            );
        }
        if let Some(s) = staff {
            let _ = write!(x, "<staff>{s}</staff>");
        }
        let arts: Vec<&Articulation> = if first {
            e.articulations.iter().collect()
        } else {
            Vec::new()
        };
        if tie || stop || !arts.is_empty() {
            x.push_str("<notations>");
            if stop {
                x.push_str("<tied type=\"stop\"/>");
            }
            if tie {
                x.push_str("<tied type=\"start\"/>");
            }
            let normal: Vec<&str> = arts
                .iter()
                .filter_map(|a| match a {
                    Articulation::Staccato => Some("<staccato/>"),
                    Articulation::Staccatissimo => Some("<staccatissimo/>"),
                    Articulation::Accent => Some("<accent/>"),
                    Articulation::Tenuto => Some("<tenuto/>"),
                    Articulation::Marcato => Some("<strong-accent/>"),
                    Articulation::Fermata => None,
                })
                .collect();
            if !normal.is_empty() {
                let _ = write!(x, "<articulations>{}</articulations>", normal.concat());
            }
            if arts.contains(&&Articulation::Fermata) {
                x.push_str("<fermata type=\"upright\"/>");
            }
            x.push_str("</notations>");
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
            x.push_str("      <note><rest/>");
            common(x);
            tail(x, false, false, true);
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
                let _ = write!(x, "<octave>{}</octave></pitch>", n.pitch.octave);
                common(x);
                let stop = stops.contains(&n.pitch);
                if stop {
                    x.push_str("<tie type=\"stop\"/>");
                }
                if n.tie {
                    x.push_str("<tie type=\"start\"/>");
                }
                tail(x, n.tie, stop, i == 0);
                x.push_str("</note>\n");
            }
        }
    }
}
