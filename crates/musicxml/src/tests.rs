use scoreart_model::edit::Pos;
use scoreart_model::*;

use crate::*;

/// Written for this repository: two measures in 3/4, G major, with a chord, a dotted note,
/// a tie across the barline, a dynamic, a lyric and an articulation.
const SMALL: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="4.0">
  <work><work-title>Little &amp; Test</work-title></work>
  <identification><creator type="composer">ScoreArt contributors</creator></identification>
  <part-list><score-part id="P1"><part-name>Flute</part-name><midi-instrument id="I1"><midi-program>74</midi-program></midi-instrument></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>2</divisions><key><fifths>1</fifths></key><time><beats>3</beats><beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef></attributes>
      <direction><direction-type><dynamics><mf/></dynamics></direction-type></direction>
      <note><pitch><step>G</step><octave>4</octave></pitch><duration>3</duration><voice>1</voice><type>quarter</type><dot/><lyric><text>la</text></lyric></note>
      <note><pitch><step>A</step><octave>4</octave></pitch><duration>1</duration><voice>1</voice><type>eighth</type><notations><articulations><staccato/></articulations></notations></note>
      <note><pitch><step>B</step><octave>4</octave></pitch><duration>2</duration><voice>1</voice><type>quarter</type></note>
      <note><chord/><pitch><step>D</step><octave>5</octave></pitch><duration>2</duration><voice>1</voice><type>quarter</type><tie type="start"/></note>
    </measure>
    <measure number="2">
      <note><pitch><step>D</step><octave>5</octave></pitch><duration>2</duration><voice>1</voice><type>quarter</type><tie type="stop"/></note>
      <note><rest/><duration>4</duration><voice>1</voice><type>half</type></note>
      <barline location="right"><bar-style>light-heavy</bar-style></barline>
    </measure>
  </part>
</score-partwise>"#;

#[test]
fn imports_small_score() {
    let s = from_str(SMALL).unwrap();
    assert_eq!(s.meta.title, "Little & Test");
    assert_eq!(s.meta.composer, "ScoreArt contributors");
    assert_eq!(s.parts.len(), 1);
    assert_eq!(s.parts[0].program, 73);
    assert_eq!(s.measures.len(), 2);
    assert_eq!(s.time_at(0).beats, 3);
    assert_eq!(s.key_at(1).fifths, 1);
    let v = s.voice(0, 0, 0, 0).unwrap();
    assert_eq!(v.events.len(), 3);
    assert_eq!(v.events[0].duration.dots, 1);
    assert_eq!(v.events[0].dynamic, Dynamic::from_id("mf"));
    assert_eq!(v.events[0].lyric(0), Some("la"));
    assert_eq!(
        v.events[1].articulations,
        vec![Articulation::from_id("staccato").unwrap()]
    );
    assert_eq!(v.events[2].notes().len(), 2);
    assert!(v.events[2].notes()[1].tie);
    assert_eq!(s.measures[1].barline, Barline::Final);
}

#[test]
fn round_trip_is_stable() {
    let s = from_str(SMALL).unwrap();
    let xml = to_string(&s);
    let back = from_str(&xml).unwrap();
    assert_eq!(s, back);
    // And through .mxl.
    let mxl = to_mxl(&s).unwrap();
    let back2 = from_bytes(&mxl).unwrap();
    assert_eq!(s, back2);
}

#[test]
fn round_trip_piano_with_voices() {
    let piano = instrument("piano").unwrap();
    let mut s = Score::new("Piano", &[piano], TimeSig::default(), KeySig::new(-3), 2);
    let q = Duration::new(NoteValue::Quarter, 0);
    let note = |st, o| EventKind::Chord(vec![Note::new(Pitch::new(st, 0, o))]);
    let mut p = Pos::default();
    p = s.overwrite(p, note(Step::E, 5), q).unwrap();
    s.overwrite(p, note(Step::G, 5), Duration::new(NoteValue::Eighth, 0))
        .unwrap();
    s.overwrite(
        Pos {
            voice: 1,
            ..Pos::default()
        },
        note(Step::C, 4),
        Duration::new(NoteValue::Half, 0),
    )
    .unwrap();
    s.overwrite(
        Pos {
            staff: 1,
            ..Pos::default()
        },
        note(Step::C, 3),
        Duration::new(NoteValue::Whole, 0),
    )
    .unwrap();
    s.staff_measure_mut(0, 1, 1).unwrap().clef = Some(Clef::Treble);
    s.measures[1].tempo = Some(Tempo { bpm: 72 });
    let back = from_str(&to_string(&s)).unwrap();
    assert_eq!(s, back);
}

#[test]
fn hostile_input_does_not_panic() {
    for junk in [
        "",
        "<",
        "<score-partwise>",
        "<score-timewise/>",
        "<score-partwise><part id='x'><measure><note><duration>-5</duration></note></measure></part></score-partwise>",
        "<score-partwise><part><measure><attributes><divisions>0</divisions><time><beats>999</beats><beat-type>3</beat-type></time><staves>99</staves></attributes><note><pitch><step>Q</step><octave>99</octave><alter>9</alter></pitch><duration>99999999999</duration><type>whole</type></note><backup><duration>99999999</duration></backup></measure></part></score-partwise>",
    ] {
        let _ = from_str(junk);
        let _ = from_bytes(junk.as_bytes());
    }
    let _ = from_bytes(b"PK\x03\x04garbage");
}
