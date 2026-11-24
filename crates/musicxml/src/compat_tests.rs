//! What other programs write into MusicXML, and how ScoreArt reads it. Each file here was
//! written for this repository, after constructs seen in exports of Finale, Sibelius, Dorico
//! and MuseScore and in the MusicXML test suite.

use scoreart_model::*;

use crate::*;

/// A one-part score: `attrs` in the first measure, then the measures' contents.
fn score(divisions: u32, attrs: &str, measures: &[&str]) -> Score {
    let mut x = String::from(
        "<?xml version=\"1.0\"?><score-partwise version=\"4.0\"><identification><encoding><software>Other 1.0</software></encoding></identification><part-list><score-part id=\"P1\"><part-name>Piano</part-name></score-part></part-list><part id=\"P1\">",
    );
    for (i, m) in measures.iter().enumerate() {
        x += &format!("<measure number=\"{}\">", i + 1);
        if i == 0 {
            x += &format!("<attributes><divisions>{divisions}</divisions>{attrs}</attributes>");
        }
        x += m;
        x += "</measure>";
    }
    x += "</part></score-partwise>";
    from_str(&x).unwrap()
}

fn n(step: char, octave: u8, dur: u32, extra: &str) -> String {
    format!(
        "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch><duration>{dur}</duration>{extra}</note>"
    )
}

/// (onset in quarters, midi) of every note of a voice.
fn notes(v: &Voice) -> Vec<(f64, u8)> {
    let mut t = 0u32;
    let mut out = Vec::new();
    for e in &v.events {
        for note in e.notes() {
            out.push((f64::from(t) / f64::from(TPQ), note.pitch.midi()));
        }
        t += e.ticks();
    }
    out
}

const TIME_44: &str =
    "<time><beats>4</beats><beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef>";

#[test]
fn composite_time_signatures_keep_every_note() {
    // 3/8 + 2/8 + 3/4: eleven eighths.
    let eighths = (0..5)
        .map(|_| n('B', 4, 1, "<type>eighth</type>"))
        .collect::<String>()
        + &(0..3)
            .map(|_| n('B', 4, 2, "<type>quarter</type>"))
            .collect::<String>();
    let s = score(
        2,
        "<time><beats>3</beats><beat-type>8</beat-type><beats>2</beats><beat-type>8</beat-type><beats>3</beats><beat-type>4</beat-type></time>",
        &[&eighths],
    );
    assert_eq!(s.time_at(0), TimeSig::new(11, 8).unwrap());
    assert_eq!(notes(s.voice(0, 0, 0, 0).unwrap()).len(), 8);
    // "3+2" in one <beats>.
    let s = score(
        2,
        "<time><beats>3+2</beats><beat-type>8</beat-type></time>",
        &[""],
    );
    assert_eq!(s.time_at(0).beats, 5);
    // Large signatures of the test files of other programs.
    let s = score(
        1,
        "<time><beats>126</beats><beat-type>4</beat-type></time>",
        &[""],
    );
    assert_eq!(s.measure_ticks(0), 126 * TPQ);
}

#[test]
fn an_overfull_measure_grows_a_pickup_rest_does_not() {
    let five = (0..5)
        .map(|_| n('A', 4, 1, "<type>quarter</type>"))
        .collect::<String>();
    let s = score(1, TIME_44, &[&five, &n('A', 4, 4, "<type>whole</type>")]);
    assert_eq!(s.measure_ticks(0), 5 * TPQ, "no note lost");
    assert_eq!(notes(s.voice(0, 0, 0, 0).unwrap()).len(), 5);
    assert_eq!(s.measure_ticks(1), 4 * TPQ);
    // A pickup of two quarters where a second part writes a whole-measure rest.
    let x = format!(
        "<?xml version=\"1.0\"?><score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A</part-name></score-part><score-part id=\"P2\"><part-name>B</part-name></score-part></part-list><part id=\"P1\"><measure number=\"0\" implicit=\"yes\"><attributes><divisions>1</divisions>{TIME_44}</attributes>{}{}</measure></part><part id=\"P2\"><measure number=\"0\" implicit=\"yes\"><attributes><divisions>1</divisions>{TIME_44}</attributes><note><rest measure=\"yes\"/><duration>4</duration></note></measure></part></score-partwise>",
        n('D', 5, 1, ""),
        n('D', 5, 1, "")
    );
    let s = from_str(&x).unwrap();
    assert_eq!(s.measure_ticks(0), 2 * TPQ);
}

#[test]
fn a_chord_across_both_staves_keeps_its_lower_notes() {
    let m = n('C', 5, 4, "<voice>1</voice><type>whole</type><staff>1</staff>")
        + &n(
            'E',
            4,
            4,
            "<chord/><voice>1</voice><type>whole</type><staff>1</staff>",
        )
        + &n(
            'G',
            3,
            4,
            "<chord/><voice>1</voice><type>whole</type><staff>2</staff>",
        )
        + &n(
            'C',
            3,
            4,
            "<chord/><voice>1</voice><type>whole</type><staff>2</staff>",
        );
    let s = score(
        1,
        &format!("{TIME_44}<staves>2</staves><clef number=\"2\"><sign>F</sign><line>4</line></clef>"),
        &[&m],
    );
    assert_eq!(notes(s.voice(0, 0, 0, 0).unwrap()), [(0.0, 64), (0.0, 72)]);
    assert_eq!(notes(s.voice(0, 1, 0, 0).unwrap()), [(0.0, 48), (0.0, 55)]);
}

#[test]
fn cue_and_hidden_notes_take_their_time_but_show_nothing() {
    // A visible half, a hidden playback-only half (a written-out trill), then cue notes.
    let m1 = n('C', 5, 2, "<voice>1</voice><type>half</type>")
        + "<note print-object=\"no\"><pitch><step>D</step><octave>5</octave></pitch><duration>2</duration><voice>1</voice></note>";
    let m2 = "<note><cue/><pitch><step>E</step><octave>5</octave></pitch><duration>2</duration><voice>1</voice><type>half</type></note>".to_string()
        + &n('F', 5, 2, "<voice>1</voice><type>half</type>");
    let s = score(1, TIME_44, &[&m1, &m2]);
    assert_eq!(notes(s.voice(0, 0, 0, 0).unwrap()), [(0.0, 72)]);
    assert_eq!(
        notes(s.voice(0, 0, 1, 0).unwrap()),
        [(2.0, 77)],
        "after the cue's time"
    );
    assert_eq!(s.measure_ticks(0), 4 * TPQ);
}

#[test]
fn long_and_short_values_and_lengths_without_a_value() {
    // divisions 64: a longa, a 128th and a 256th; a maxima (32 quarters) has no value.
    let m1 = n('C', 5, 1024, "<type>long</type>");
    let m2 = n('C', 5, 2, "<type>128th</type>") + &n('C', 5, 1, "<type>256th</type>");
    let m3 = n('C', 5, 2048, "<type>maxima</type>");
    let s = score(
        64,
        "<time><beats>16</beats><beat-type>2</beat-type></time>",
        &[&m1, &m2, &m3],
    );
    let v = |m| s.voice(0, 0, m, 0).unwrap();
    assert_eq!(v(0).events[0].duration.value, NoteValue::Long);
    assert_eq!(v(1).events[0].duration.value, NoteValue::N128th);
    assert_eq!(v(1).events[1].duration.value, NoteValue::N256th);
    // The maxima: tied notes of 32 quarters.
    let held: Vec<_> = v(2).events.iter().filter(|e| !e.is_rest()).collect();
    assert_eq!(held.iter().map(|e| e.ticks()).sum::<u32>(), 32 * TPQ);
    assert!(held.len() > 1 && held[..held.len() - 1].iter().all(|e| e.notes()[0].tie));
    // MusicXML out and in: same values (long notes in fine divisions do not overflow).
    let back = from_str(&to_string(&s)).unwrap();
    for m in 0..3 {
        assert_eq!(back.voice(0, 0, m, 0), s.voice(0, 0, m, 0), "measure {}", m + 1);
    }
    // Four dots.
    let s = score(
        8,
        TIME_44,
        &[&n('F', 4, 31, "<type>half</type><dot/><dot/><dot/><dot/>")],
    );
    assert_eq!(s.voice(0, 0, 0, 0).unwrap().events[0].duration.dots, 4);
}

#[test]
fn rounded_triplets_of_other_programs_stay_triplets() {
    // Divisions 256: three triplet eighths written 85 + 85 + 86, a second voice entering
    // after a <forward> of 84 (a triplet eighth, rounded).
    let tm =
        "<time-modification><actual-notes>3</actual-notes><normal-notes>2</normal-notes></time-modification>";
    let m = n('B', 3, 85, &format!("<voice>1</voice><type>eighth</type>{tm}"))
        + &n('D', 4, 85, &format!("<voice>1</voice><type>eighth</type>{tm}"))
        + &n('F', 4, 85, &format!("<voice>1</voice><type>eighth</type>{tm}"))
        + &n('G', 4, 512, "<voice>1</voice><type>half</type>")
        + &n('A', 4, 256, "<voice>1</voice><type>quarter</type>")
        + "<backup><duration>1023</duration></backup><forward><duration>84</duration><voice>2</voice></forward>"
        + &n('E', 4, 86, &format!("<voice>2</voice><type>eighth</type>{tm}"));
    let s = score(256, TIME_44, &[&m]);
    assert_eq!(
        s.measure_ticks(0),
        4 * TPQ,
        "a few divisions short is the full measure"
    );
    let v0 = s.voice(0, 0, 0, 0).unwrap();
    assert_eq!(v0.ticks(), 4 * TPQ);
    assert_eq!(notes(v0).len(), 5);
    assert!(v0.events.iter().all(|e| e.duration.value <= NoteValue::Eighth));
    let v1 = s.voice(0, 0, 0, 1).unwrap();
    assert_eq!(
        notes(v1),
        [(1.0 / 3.0, 64)],
        "on the triplet, filled by a triplet rest"
    );
}

#[test]
fn graces_after_a_note_end_it() {
    // A trill's turn at the end of a measure, one taking time from the note before, and a
    // grace before the next note.
    let m1 = n('C', 5, 4, "<voice>1</voice><type>whole</type>")
        + "<note><grace/><pitch><step>B</step><octave>4</octave></pitch><voice>1</voice><type>16th</type></note>"
        + "<note><grace/><pitch><step>C</step><octave>5</octave></pitch><voice>1</voice><type>16th</type></note>";
    let m2 = n('D', 5, 2, "<voice>1</voice><type>half</type>")
        + "<note><grace steal-time-previous=\"10\"/><pitch><step>C</step><octave>5</octave></pitch><voice>1</voice><type>16th</type></note>"
        + "<note><grace slash=\"yes\"/><pitch><step>F</step><octave>5</octave></pitch><voice>1</voice><type>eighth</type></note>"
        + &n('E', 5, 2, "<voice>1</voice><type>half</type>");
    let s = score(1, TIME_44, &[&m1, &m2]);
    let e = |m: usize, i: usize| s.voice(0, 0, m, 0).unwrap().events[i].clone();
    assert_eq!(e(0, 0).grace.len(), 2);
    assert!(e(0, 0).grace.iter().all(|g| g.after));
    assert_eq!(e(1, 0).grace.len(), 1);
    assert!(e(1, 0).grace[0].after);
    assert_eq!(e(1, 1).grace.len(), 1);
    assert!(!e(1, 1).grace[0].after && e(1, 1).grace[0].slash);
    // Round trip keeps which side they are on.
    let back = from_str(&to_string(&s)).unwrap();
    assert_eq!(back.voice(0, 0, 0, 0), s.voice(0, 0, 0, 0));
    assert_eq!(back.voice(0, 0, 1, 0), s.voice(0, 0, 1, 0));
}

#[test]
fn a_hairpin_placed_by_offset_starts_at_its_note() {
    // Finale: the crescendo is written after the first note but moved back to it by <offset>.
    let m = n('B', 4, 2, "<voice>1</voice><type>half</type>")
        + "<direction><direction-type><wedge type=\"crescendo\"/></direction-type><offset>-2</offset></direction>"
        + &n('C', 5, 2, "<voice>1</voice><type>half</type>")
        + "<direction><direction-type><wedge type=\"stop\"/></direction-type><offset>-1</offset></direction>";
    let s = score(1, TIME_44, &[&m]);
    let h = s
        .spanners
        .iter()
        .find(|sp| sp.kind == SpannerKind::Crescendo)
        .unwrap();
    assert_eq!((h.start.tick, h.end.tick), (0, 2 * TPQ));
}
