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
    s.measures[1].tempo = Some(Tempo::quarter(72));
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

#[test]
fn parts_without_a_program_get_one_from_their_name() {
    use crate::import::program_for_name;
    assert_eq!(program_for_name("Violins I"), Some(40));
    assert_eq!(program_for_name("Violoncello"), Some(42));
    assert_eq!(program_for_name("Kontrabass"), Some(43));
    assert_eq!(program_for_name("English Horn"), Some(68));
    assert_eq!(program_for_name("Horn in F"), Some(60));
    assert_eq!(program_for_name("Pauken"), Some(47));
    assert_eq!(program_for_name("Melody"), None);
    let xml = r#"<?xml version="1.0"?><score-partwise version="4.0"><part-list>
        <score-part id="P1"><part-name>Cellos</part-name></score-part></part-list>
        <part id="P1"><measure number="1"><attributes><divisions>1</divisions></attributes>
        <note><pitch><step>C</step><octave>3</octave></pitch><duration>4</duration><type>whole</type></note>
        </measure></part></score-partwise>"#;
    let s = crate::from_str(xml).unwrap();
    assert_eq!(s.parts[0].program, 42);
    // Saved before: piano for every part; the name wins.
    let piano = xml.replace("<part-name>Cellos</part-name>", "<part-name>Cellos</part-name><midi-instrument id=\"I\"><midi-program>1</midi-program></midi-instrument>");
    assert_eq!(crate::from_str(&piano).unwrap().parts[0].program, 42);
    let harp = xml.replace("<part-name>Cellos</part-name>", "<part-name>Cellos</part-name><midi-instrument id=\"I\"><midi-program>47</midi-program></midi-instrument>");
    assert_eq!(
        crate::from_str(&harp).unwrap().parts[0].program,
        46,
        "a written program is kept"
    );
}

/// A file written by another program (as MuseScore writes them): odd and even margins,
/// credits for the title page, and a print element starting every system.
#[test]
fn reads_layout_and_credits_of_other_programs() {
    use scoreart_model::TextKind;
    let xml = SMALL
        .replace("<work><work-title>Little &amp; Test</work-title></work>", "")
        .replace("<identification><creator type=\"composer\">ScoreArt contributors</creator></identification>", "<identification><encoding><software>Another program 4.5</software></encoding></identification>
  <defaults><scaling><millimeters>7</millimeters><tenths>40</tenths></scaling>
    <page-layout><page-height>1697</page-height><page-width>1200</page-width>
      <page-margins type=\"even\"><left-margin>60</left-margin><right-margin>100</right-margin><top-margin>57</top-margin><bottom-margin>114</bottom-margin></page-margins>
      <page-margins type=\"odd\"><left-margin>100</left-margin><right-margin>60</right-margin><top-margin>57</top-margin><bottom-margin>114</bottom-margin></page-margins>
    </page-layout></defaults>
  <credit page=\"1\"><credit-type>title</credit-type><credit-words default-x=\"600\" default-y=\"1640\" justify=\"center\" valign=\"top\" font-size=\"26\">Credit Title</credit-words></credit>
  <credit page=\"1\"><credit-type>composer</credit-type><credit-words default-x=\"1140\" default-y=\"1540\" justify=\"right\" valign=\"bottom\" font-size=\"11\">Credit Composer</credit-words></credit>
  <credit page=\"1\"><credit-words default-x=\"600\" default-y=\"60\" justify=\"center\" valign=\"bottom\">© Somebody</credit-words></credit>")
        .replace("<measure number=\"2\">", "<measure number=\"2\"><print new-system=\"yes\"/>");
    let s = from_str(&xml).unwrap();
    assert_eq!(s.meta.title, "Credit Title");
    assert_eq!(s.meta.composer, "Credit Composer");
    assert_eq!(
        s.meta.copyright, "© Somebody",
        "an untyped credit at the foot is the copyright"
    );
    assert_eq!(s.text_styles.get(TextKind::Title).size, 26.0);
    assert_eq!(s.text_styles.get(TextKind::Composer).size, 11.0);
    let p = s.page;
    assert!(p.two_sided);
    assert!((p.margin_left_mm - 17.5).abs() < 0.01 && (p.even_margin_left_mm - 10.5).abs() < 0.01);
    // The other program's line breaks are kept.
    assert!(s.measures[0].system_break);
}

/// ScoreArt writes where its systems and pages start (so other programs lay the score out
/// alike) and opens only the user's own breaks again.
#[test]
fn laid_out_systems_round_trip_without_fixing_them() {
    let mut s = from_str(SMALL).unwrap();
    let extra = s.measures[1].clone();
    for _ in 0..6 {
        s.measures.push(extra.clone());
        for p in &mut s.parts {
            let m = p.measures[1].clone();
            p.measures.push(m);
        }
    }
    s.measures[1].barline = scoreart_model::Barline::Regular;
    s.measures[4].system_break = true;
    s.page.two_sided = true;
    s.page.even_margin_left_mm = 25.0;
    s.page.music_top = 9.0;
    s.normalize();
    let xml = to_string_laid_out(&s, &[(2, false), (5, false), (7, true)]);
    assert_eq!(xml.matches("<print new-system=\"yes\"/>").count(), 2);
    assert_eq!(xml.matches("<print new-page=\"yes\"/>").count(), 1);
    assert!(xml.contains("<page-margins type=\"even\">") && xml.contains("<page-margins type=\"odd\">"));
    assert!(xml.contains("<credit page=\"1\"><credit-type>title</credit-type>"));
    let back = from_str(&xml).unwrap();
    let breaks: Vec<usize> = back
        .measures
        .iter()
        .enumerate()
        .filter(|(_, i)| i.system_break || i.page_break)
        .map(|(m, _)| m)
        .collect();
    assert_eq!(breaks, vec![4], "only the user's break");
    assert_eq!(back.page, s.page.clamped());
}
