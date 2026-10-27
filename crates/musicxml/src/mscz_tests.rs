//! MuseScore reading, tested with small files written by hand for this repository.

use scoreart_model::*;

use crate::*;

/// MuseScore 4 style: a piano part (two staves), title frame, signatures, tempo, tuplets,
/// voices, ties, lines and marks.
const MS4: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<museScore version="4.40">
  <Score>
    <Division>480</Division>
    <metaTag name="composer">Test Composer</metaTag>
    <metaTag name="workTitle">Work</metaTag>
    <Part id="1">
      <Staff id="1"><StaffType group="pitched"/></Staff>
      <Staff id="2"><StaffType group="pitched"/><defaultClef>F</defaultClef></Staff>
      <trackName>Piano</trackName>
      <Instrument id="piano"><longName>Piano</longName><shortName>Pno.</shortName><Channel><program value="0"/></Channel></Instrument>
    </Part>
    <Staff id="1">
      <VBox><Text><style>title</style><text>Hand <b>Made</b></text></Text><Text><style>composer</style><text>Somebody</text></Text></VBox>
      <Measure>
        <voice>
          <Clef><concertClefType>G</concertClefType><transposingClefType>G</transposingClefType></Clef>
          <KeySig><concertKey>2</concertKey></KeySig>
          <TimeSig><sigN>3</sigN><sigD>4</sigD></TimeSig>
          <Tempo><tempo>2</tempo><followText>1</followText><text>Allegro <sym>metNoteQuarterUp</sym> = 120</text></Tempo>
          <RehearsalMark><text>A</text></RehearsalMark>
          <Dynamic><subtype>mf</subtype><velocity>80</velocity></Dynamic>
          <StaffText><text>dolce</text></StaffText>
          <Harmony><root>20</root><name>m7</name></Harmony>
          <Spanner type="HairPin"><HairPin><subtype>0</subtype></HairPin><next><location><fractions>3/4</fractions></location></next></Spanner>
          <Spanner type="Slur"><Slur/><next><location><fractions>1/3</fractions></location></next></Spanner>
          <Fermata><subtype>fermataAbove</subtype></Fermata>
          <Chord>
            <durationType>quarter</durationType>
            <Lyrics><syllabic>single</syllabic><text>la</text></Lyrics>
            <Articulation><subtype>articStaccatoAbove</subtype></Articulation>
            <Ornament><subtype>ornamentTrill</subtype></Ornament>
            <Note><Fingering><text>3</text></Fingering><pitch>66</pitch><tpc>20</tpc></Note>
          </Chord>
          <Chord><acciaccatura/><durationType>eighth</durationType><Note><pitch>69</pitch><tpc>17</tpc></Note></Chord>
          <Tuplet><normalNotes>2</normalNotes><actualNotes>3</actualNotes><baseNote>eighth</baseNote></Tuplet>
          <Chord><durationType>eighth</durationType><Note><pitch>67</pitch><tpc>15</tpc></Note></Chord>
          <Chord><durationType>eighth</durationType><Note><pitch>69</pitch><tpc>17</tpc></Note></Chord>
          <Spanner type="Slur"><prev><location><fractions>-1/3</fractions></location></prev></Spanner>
          <Chord><durationType>eighth</durationType><Note><pitch>71</pitch><tpc>19</tpc></Note></Chord>
          <endTuplet/>
          <Chord>
            <durationType>quarter</durationType>
            <Arpeggio><subtype>1</subtype></Arpeggio>
            <TremoloSingleChord><subtype>r16</subtype></TremoloSingleChord>
            <Note><pitch>62</pitch><tpc>16</tpc><headGroup>cross</headGroup></Note>
            <Note><pitch>74</pitch><tpc>16</tpc>
              <Spanner type="Tie"><Tie/><next><location><measures>1</measures><fractions>-3/4</fractions></location></next></Spanner>
            </Note>
          </Chord>
          <Breath><symbol>breathMarkComma</symbol></Breath>
          <Spanner type="HairPin"><prev><location><fractions>-3/4</fractions></location></prev></Spanner>
        </voice>
        <voice>
          <location><fractions>1/4</fractions></location>
          <Chord><durationType>half</durationType><Note><pitch>57</pitch><tpc>17</tpc></Note></Chord>
        </voice>
      </Measure>
      <Measure>
        <startRepeat/>
        <endRepeat>2</endRepeat>
        <voice>
          <Spanner type="Volta"><Volta><endings>1</endings><text>1.</text></Volta><next><location><measures>1</measures></location></next></Spanner>
          <Spanner type="Ottava"><Ottava><subtype>8va</subtype></Ottava><next><location><fractions>1/2</fractions></location></next></Spanner>
          <Chord><durationType>half</durationType><dots>1</dots><Note><pitch>74</pitch><tpc>16</tpc></Note></Chord>
          <Spanner type="Ottava"><prev><location><fractions>-1/2</fractions></location></prev></Spanner>
        </voice>
      </Measure>
      <Measure>
        <Marker><style>Repeat Text Right</style><text>Fine</text><label>fine</label></Marker>
        <Jump><text>D.C. al Fine</text><jumpTo>start</jumpTo><playUntil>fine</playUntil></Jump>
        <voice>
          <Rest><durationType>measure</durationType><duration>3/4</duration></Rest>
          <BarLine><subtype>end</subtype></BarLine>
        </voice>
      </Measure>
    </Staff>
    <Staff id="2">
      <Measure>
        <voice>
          <Clef><concertClefType>F</concertClefType></Clef>
          <KeySig><concertKey>2</concertKey></KeySig>
          <TimeSig><sigN>3</sigN><sigD>4</sigD></TimeSig>
          <Rest><durationType>measure</durationType><duration>3/4</duration></Rest>
        </voice>
      </Measure>
      <Measure><voice><MeasureRepeat><subtype>1</subtype><durationType>measure</durationType><duration>3/4</duration></MeasureRepeat></voice></Measure>
      <Measure><voice><Rest><durationType>measure</durationType><duration>3/4</duration></Rest></voice></Measure>
    </Staff>
  </Score>
</museScore>
"#;

/// MuseScore 3 style: old key signature tag, tuplets by id, tie elements.
const MS3: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<museScore version="3.02">
  <Score>
    <Part><Staff id="1"/><trackName>Flute</trackName><Instrument><longName>Flute</longName><Channel><program value="73"/></Channel></Instrument></Part>
    <Staff id="1">
      <Measure>
        <voice>
          <KeySig><accidental>-3</accidental></KeySig>
          <TimeSig><sigN>4</sigN><sigD>4</sigD><subtype>1</subtype></TimeSig>
          <Tuplet id="1"><normalNotes>2</normalNotes><actualNotes>3</actualNotes></Tuplet>
          <Chord><Tuplet>1</Tuplet><durationType>quarter</durationType><Note><pitch>63</pitch><tpc>11</tpc></Note></Chord>
          <Chord><Tuplet>1</Tuplet><durationType>quarter</durationType><Note><pitch>65</pitch><tpc>13</tpc></Note></Chord>
          <Chord><Tuplet>1</Tuplet><durationType>quarter</durationType><Note><pitch>67</pitch><tpc>15</tpc><Tie id="2"/></Note></Chord>
          <Chord><durationType>half</durationType><Note><endSpanner id="2"/><pitch>67</pitch><tpc>15</tpc></Note></Chord>
        </voice>
      </Measure>
    </Staff>
  </Score>
</museScore>
"#;

fn mscz(xml: &str) -> Vec<u8> {
    use std::io::Write;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let o = zip::write::SimpleFileOptions::default();
        z.start_file("score.mscx", o).unwrap();
        z.write_all(xml.as_bytes()).unwrap();
        z.finish().unwrap();
    }
    buf.into_inner()
}

#[test]
fn reads_musescore_4() {
    let s = from_bytes(&mscz(MS4)).unwrap();
    assert_eq!(s.meta.title, "Hand Made");
    assert_eq!(s.meta.composer, "Somebody");
    assert_eq!(s.parts.len(), 1);
    assert_eq!(s.parts[0].clefs, vec![Clef::Treble, Clef::Bass]);
    assert_eq!(s.parts[0].abbreviation, "Pno.");
    assert_eq!(s.measures.len(), 3);
    assert_eq!(s.key_at(0).fifths, 2);
    assert_eq!(s.time_at(0).beats, 3);
    let m0 = &s.measures[0];
    assert_eq!(m0.tempo.map(|t| t.bpm), Some(120));
    assert_eq!(m0.tempo_text.as_deref(), Some("Allegro"));
    assert_eq!(m0.rehearsal.as_deref(), Some("A"));

    let v = s.voice(0, 0, 0, 0).unwrap();
    let first = &v.events[0];
    let p = first.notes()[0].pitch;
    assert_eq!(
        (p.step, p.alter, p.octave),
        (Step::F, 1, 4),
        "F#4 spelled from the tpc"
    );
    assert_eq!(first.dynamic.map(|d| d.id()), Some("mf"));
    assert_eq!(first.text.as_deref(), Some("dolce"));
    assert_eq!(first.chord_symbol.as_deref(), Some("F#m7"));
    assert_eq!(first.lyric(0), Some("la"));
    let ids: Vec<&str> = first.articulations.iter().map(|a| a.id()).collect();
    assert!(
        ids.contains(&"fermata") && ids.contains(&"staccato") && ids.contains(&"trill"),
        "{ids:?}"
    );
    assert_eq!(first.notes()[0].fingering.as_deref(), Some("3"));
    // Triplet with a grace note before it.
    let trip = &v.events[1];
    assert_eq!(trip.duration.tuplet, Some(Tuplet { actual: 3, normal: 2 }));
    assert_eq!(trip.grace.len(), 1);
    assert!(trip.grace[0].slash);
    let chord = &v.events[4];
    assert_eq!(chord.arpeggio, Arpeggio::Up);
    assert_eq!(chord.tremolo, Tremolo::Strokes(2));
    assert_eq!(chord.notes()[0].head, NoteHead::X);
    assert!(chord.notes()[1].tie);
    assert!(chord.articulations.iter().any(|a| a.id() == "breath"));
    assert_eq!(v.ticks(), TPQ * 3);
    // Second voice starts after a quarter rest.
    let v2 = s.voice(0, 0, 0, 1).unwrap();
    assert!(v2.events[0].is_rest());
    assert_eq!(v2.events[1].notes()[0].pitch.name(), "A3");
    // Lines.
    let kinds: Vec<&str> = s.spanners.iter().map(|x| x.kind.id()).collect();
    assert!(
        kinds.contains(&"slur") && kinds.contains(&"crescendo") && kinds.contains(&"8va"),
        "{kinds:?}"
    );
    let slur = s.spanners.iter().find(|x| x.kind == SpannerKind::Slur).unwrap();
    assert_eq!(
        (slur.start.tick, slur.end.tick),
        (0, TPQ + TPQ / 3),
        "slur ends on the second triplet note"
    );
    // Measure marks.
    assert!(s.measures[1].repeat_start);
    assert_eq!(s.measures[1].barline, Barline::RepeatEnd);
    assert_eq!(s.measures[1].volta, Some(1));
    assert!(s.measures[2].jumps.contains(&Jump::Fine));
    assert!(s.measures[2].jumps.contains(&Jump::DaCapoAlFine));
    assert_eq!(s.measures[2].barline, Barline::Final);
    assert!(s.staff_measure(0, 1, 1).unwrap().measure_repeat);
    // And it saves as MusicXML.
    let back = from_str(&to_string(&s)).unwrap();
    assert_eq!(back, s);
}

#[test]
fn reads_musescore_3_uncompressed() {
    let s = from_bytes(MS3.as_bytes()).unwrap();
    assert_eq!(s.parts[0].name, "Flute");
    assert_eq!(s.parts[0].program, 73);
    assert_eq!(s.key_at(0).fifths, -3);
    assert_eq!(s.time_at(0).symbol, TimeSymbol::Common);
    let v = s.voice(0, 0, 0, 0).unwrap();
    assert_eq!(v.events.len(), 4);
    assert_eq!(v.events[0].notes()[0].pitch.name(), "Eb4");
    assert_eq!(v.events[0].duration.tuplet, Some(Tuplet { actual: 3, normal: 2 }));
    assert!(v.events[2].notes()[0].tie);
    assert!(v.events[3].duration.tuplet.is_none());
}

#[test]
fn hostile_musescore_files_do_not_panic() {
    for junk in [
        "<museScore/>",
        "<museScore version=\"2.06\"><Score/></museScore>",
        "<museScore version=\"4\"><Score><Part/><Staff id=\"1\"><Measure><voice><Chord><durationType>measure</durationType><duration>9/0</duration><Note><pitch>999</pitch><tpc>-500</tpc></Note></Chord><location><fractions>99999999/1</fractions></location><Spanner type=\"Slur\"><next><location><measures>-99</measures><fractions>-7/3</fractions></location></next></Spanner></voice></Measure></Staff></Score></museScore>",
    ] {
        let _ = from_bytes(junk.as_bytes());
        let _ = from_bytes(&mscz(junk));
    }
}

/// Conventions of newer MuseScore 4 files (4.6 and later) and of scores MuseScore imported
/// from MusicXML: staves without ids, `<harmonyInfo>`, `<NoteParenGroup>`, lines that start
/// inside a chord, numbered chord lines, text lines, jumps as staff text, back-and-forth
/// `<location>` moves and notes under an octave line kept at their written position.
const MS46: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<museScore version="4.70">
  <Score>
    <Division>480</Division>
    <Part id="1">
      <Staff><StaffType group="pitched"><name>stdNormal</name></StaffType></Staff>
      <trackName>Flute</trackName>
      <Instrument id="flute"><longName>Flute</longName></Instrument>
    </Part>
    <Staff id="1">
      <Measure>
        <voice>
          <KeySig><concertKey>-2</concertKey></KeySig>
          <TimeSig><sigN>4</sigN><sigD>4</sigD></TimeSig>
          <Tempo><tempo>1.2</tempo><followText>1</followText><text><b>Andante</b> <sym>metNoteQuarterUp</sym> = 72</text></Tempo>
          <StaffText><text><sym>segno</sym></text></StaffText>
          <Harmony><harmonyInfo><name>maj7</name><root>12</root><bass>14</bass></harmonyInfo></Harmony>
          <Chord>
            <durationType>quarter</durationType>
            <Spanner type="Slur"><Slur/><next><location><fractions>1/2</fractions></location></next></Spanner>
            <Articulation><subtype>articStaccatoAbove</subtype></Articulation>
            <Articulation><subtype>articTenutoAbove</subtype></Articulation>
            <Note><pitch>70</pitch><tpc>12</tpc></Note>
            <NoteParenGroup><Notes><NoteIdx>0</NoteIdx></Notes></NoteParenGroup>
            <ChordLine><subtype>4</subtype></ChordLine>
          </Chord>
          <Chord>
            <durationType>quarter</durationType>
            <Note><pitch>72</pitch><tpc>14</tpc></Note>
            <ChordLine><subtype>1</subtype></ChordLine>
          </Chord>
          <Chord>
            <durationType>quarter</durationType>
            <Spanner type="Slur"><prev><location><fractions>-1/2</fractions></location></prev></Spanner>
            <Note><pitch>74</pitch><tpc>16</tpc></Note>
          </Chord>
          <location><fractions>-40/1</fractions></location>
          <location><fractions>40/1</fractions></location>
          <Spanner type="TextLine"><TextLine><beginText>cresc.</beginText></TextLine><next><location><measures>1</measures><fractions>-1/4</fractions></location></next></Spanner>
          <Chord><durationType>quarter</durationType><Note><pitch>74</pitch><tpc>16</tpc></Note></Chord>
          <StaffText><text><i>D.S. al Coda</i></text></StaffText>
        </voice>
      </Measure>
      <Measure>
        <voice>
          <Spanner type="TextLine"><prev><location><measures>-1</measures><fractions>1/4</fractions></location></prev></Spanner>
          <Spanner type="Ottava"><Ottava><subtype>8va</subtype></Ottava><next><location><fractions>1/2</fractions></location></next></Spanner>
          <Chord><durationType>quarter</durationType><Note><pitch>62</pitch><tpc>16</tpc></Note></Chord>
          <Chord><durationType>quarter</durationType><Note><pitch>60</pitch><tpc>14</tpc></Note></Chord>
          <Spanner type="Ottava"><prev><location><fractions>-1/2</fractions></location></prev></Spanner>
          <Chord><durationType>half</durationType><Note><pitch>60</pitch><tpc>14</tpc></Note></Chord>
        </voice>
      </Measure>
    </Staff>
  </Score>
</museScore>
"#;

#[test]
fn reads_musescore_4_6_conventions() {
    let s = from_bytes(MS46.as_bytes()).unwrap();
    assert_eq!(s.parts.len(), 1);
    assert_eq!(s.measures.len(), 2);
    assert_eq!(s.key_at(0).fifths, -2);
    let m0 = &s.measures[0];
    assert_eq!(m0.tempo.map(|t| t.bpm), Some(72));
    assert_eq!(m0.tempo_text.as_deref(), Some("Andante"));
    assert!(m0.jumps.contains(&Jump::Segno), "{:?}", m0.jumps);
    assert!(m0.jumps.contains(&Jump::DalSegnoAlCoda), "{:?}", m0.jumps);

    let v = s.voice(0, 0, 0, 0).unwrap();
    assert_eq!(v.events.len(), 4, "the location moves cancel out");
    let first = &v.events[0];
    assert_eq!(first.chord_symbol.as_deref(), Some("Bbmaj7/C"));
    assert!(first.notes()[0].paren);
    let ids: Vec<&str> = first.articulations.iter().map(|a| a.id()).collect();
    assert!(ids.contains(&"portato") && !ids.contains(&"staccato"), "{ids:?}");
    assert!(ids.contains(&"scoop"), "{ids:?}");
    assert!(v.events[1].articulations.iter().any(|a| a.id() == "fall"));
    assert!(v.events[3].text.is_none());

    let slur = s.spanners.iter().find(|x| x.kind == SpannerKind::Slur).unwrap();
    assert_eq!((slur.start.tick, slur.end.tick), (0, 2 * TPQ));
    assert!(s.spanners.iter().any(|x| x.kind == SpannerKind::CrescendoLine));
    assert!(s.spanners.iter().any(|x| x.kind == SpannerKind::Ottava(1)));

    // Under the 8va the notes sound an octave above their written position.
    let v1 = s.voice(0, 0, 1, 0).unwrap();
    let names: Vec<String> = v1.events.iter().map(|e| e.notes()[0].pitch.name()).collect();
    assert_eq!(names, ["D5", "C5", "C4"]);
}

/// Hidden instruments and staves, several lyric verses with hyphens, staves without a clef of
/// their own (treble) and a line break — as MuseScore 4 writes them.
const MS_LAYOUT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<museScore version="4.50">
  <Score>
    <Division>480</Division>
    <Part id="1">
      <Staff id="1"><StaffType group="pitched"/></Staff>
      <Staff id="2"><StaffType group="pitched"/></Staff>
      <trackName>Women</trackName>
      <Instrument id="women"><longName>Women</longName></Instrument>
    </Part>
    <Part id="2">
      <Staff id="3"><StaffType group="pitched"/><isStaffVisible>0</isStaffVisible></Staff>
      <Staff id="4"><StaffType group="pitched"/><isStaffVisible>0</isStaffVisible></Staff>
      <show>0</show>
      <trackName>Piano</trackName>
      <Instrument id="piano"><longName>Piano</longName><clef staff="2">F</clef></Instrument>
    </Part>
    <Staff id="1">
      <Measure>
        <voice>
          <TimeSig><sigN>2</sigN><sigD>4</sigD></TimeSig>
          <Chord><durationType>quarter</durationType>
            <Lyrics><syllabic>begin</syllabic><text>1. Freu</text></Lyrics>
            <Lyrics><no>1</no><text>2. Wir</text></Lyrics>
            <Note><pitch>67</pitch><tpc>15</tpc></Note></Chord>
          <Chord><durationType>quarter</durationType>
            <Lyrics><syllabic>end</syllabic><text>de</text></Lyrics>
            <Lyrics><no>1</no><text>be</text><syllabic>begin</syllabic></Lyrics>
            <Note><pitch>69</pitch><tpc>17</tpc></Note></Chord>
        </voice>
      </Measure>
      <Measure>
        <LayoutBreak><subtype>line</subtype></LayoutBreak>
        <voice><Rest><durationType>measure</durationType><duration>2/4</duration></Rest></voice>
      </Measure>
      <Measure>
        <voice><Rest><durationType>measure</durationType><duration>2/4</duration></Rest></voice>
      </Measure>
    </Staff>
    <Staff id="2"><Measure><voice><Rest><durationType>measure</durationType><duration>2/4</duration></Rest></voice></Measure></Staff>
    <Staff id="3"><Measure><voice><Rest><durationType>measure</durationType><duration>2/4</duration></Rest></voice></Measure></Staff>
    <Staff id="4"><Measure><voice><Rest><durationType>measure</durationType><duration>2/4</duration></Rest></voice></Measure></Staff>
  </Score>
</museScore>
"#;

#[test]
fn reads_hidden_staves_verses_and_breaks() {
    let s = from_bytes(MS_LAYOUT.as_bytes()).unwrap();
    assert_eq!(
        s.parts[0].clefs,
        vec![Clef::Treble, Clef::Treble],
        "no clef given: treble"
    );
    assert_eq!(s.parts[1].clefs, vec![Clef::Treble, Clef::Bass]);
    assert!(s.parts[0].visible() && !s.parts[1].visible());
    let v = s.voice(0, 0, 0, 0).unwrap();
    assert_eq!(
        v.events[0].lyrics,
        vec!["1. Freu-".to_string(), "2. Wir".to_string()]
    );
    assert_eq!(v.events[1].lyric(0), Some("de"));
    assert_eq!(v.events[1].lyric(1), Some("be-"));
    assert!(s.measures[1].system_break && !s.measures[0].system_break);
    // All of it survives MusicXML.
    let back = from_str(&to_string(&s)).unwrap();
    assert_eq!(back.parts[1].hidden, vec![true, true]);
    assert_eq!(
        back.voice(0, 0, 0, 0).unwrap().events[0].lyrics,
        v.events[0].lyrics
    );
    assert_eq!(
        back.voice(0, 0, 0, 0).unwrap().events[1].lyrics,
        v.events[1].lyrics
    );
    assert!(back.measures[1].system_break);
}
