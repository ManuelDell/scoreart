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

/// A 6/8 measure written as two 3/8 halves with a hidden barline, an uncounted pickup with a
/// line break, a volta labelled "1." for the second pass, and moved rehearsal marks and text.
const MS_IRREGULAR: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<museScore version="4.60">
  <Score>
    <Division>480</Division>
    <Part id="1"><Staff><StaffType group="pitched"/></Staff><trackName>Voice</trackName>
      <Instrument id="voice"><longName>Voice</longName></Instrument></Part>
    <Staff id="1">
      <Measure>
        <voice>
          <TimeSig><sigN>6</sigN><sigD>8</sigD></TimeSig>
          <RehearsalMark><placement>below</placement><offset x="-9.4" y="5.7"/><text>Vers</text></RehearsalMark>
          <Chord><dots>1</dots><durationType>quarter</durationType><Note><pitch>67</pitch><tpc>15</tpc></Note></Chord>
          <Chord><dots>1</dots><durationType>quarter</durationType><Note><pitch>69</pitch><tpc>17</tpc></Note></Chord>
        </voice>
      </Measure>
      <Measure len="3/8">
        <voice>
          <Chord><dots>1</dots><durationType>quarter</durationType><Note><pitch>71</pitch><tpc>19</tpc></Note></Chord>
          <BarLine><visible>0</visible></BarLine>
        </voice>
      </Measure>
      <Measure len="3/8">
        <voice>
          <Rest><durationType>quarter</durationType></Rest>
          <Chord><durationType>eighth</durationType><Note><pitch>67</pitch><tpc>15</tpc></Note></Chord>
        </voice>
      </Measure>
      <Measure len="3/8">
        <irregular>1</irregular>
        <LayoutBreak><subtype>line</subtype></LayoutBreak>
        <voice>
          <PlayTechAnnotation><offset x="6" y="-4.7"/><text>Instrumente fallen ein</text></PlayTechAnnotation>
          <Chord><dots>1</dots><durationType>quarter</durationType><Note><pitch>67</pitch><tpc>15</tpc></Note></Chord>
        </voice>
      </Measure>
      <Measure>
        <voice>
          <Spanner type="Volta"><Volta><beginText>1.</beginText><endings>2</endings></Volta>
            <next><location><measures>1</measures></location></next></Spanner>
          <Rest><durationType>measure</durationType><duration>6/8</duration></Rest>
        </voice>
      </Measure>
    </Staff>
  </Score>
</museScore>
"#;

#[test]
fn reads_irregular_measures_and_placements() {
    let s = from_bytes(MS_IRREGULAR.as_bytes()).unwrap();
    assert_eq!(s.measures.len(), 5);
    assert_eq!(s.measure_ticks(0), TPQ * 3);
    assert_eq!(s.measure_ticks(1), TPQ * 3 / 2, "len=\"3/8\"");
    let half = s.voice(0, 0, 2, 0).unwrap();
    assert_eq!(half.events.len(), 2, "no padding rests: {:?}", half.events);
    assert_eq!(s.measures[1].barline, Barline::Hidden);
    assert!(s.measures[3].uncounted && s.measures[3].system_break);
    assert_eq!(s.measure_number(4), 4, "the pickup is not counted");
    assert_eq!(s.measures[4].volta, Some(2));
    assert_eq!(s.measures[4].volta_text.as_deref(), Some("1."));
    let p = s.measures[0].rehearsal_place.unwrap();
    assert!(p.below && p.dx == -94 && p.dy == 57);
    let t = &s.voice(0, 0, 3, 0).unwrap().events[0];
    assert_eq!(t.text.as_deref(), Some("Instrumente fallen ein"));
    assert_eq!(t.text_place.map(|p| (p.dx, p.dy)), Some((60, -47)));
    // All of it survives MusicXML.
    let back = from_str(&to_string(&s)).unwrap();
    assert_eq!(back.measure_ticks(1), TPQ * 3 / 2);
    assert_eq!(back.measures[1].barline, Barline::Hidden);
    assert!(back.measures[3].uncounted && back.measures[3].system_break);
    assert_eq!(back.measures[4].volta_text.as_deref(), Some("1."));
    assert_eq!(back.measures[0].rehearsal_place, s.measures[0].rehearsal_place);
    assert_eq!(back.voice(0, 0, 3, 0).unwrap().events[0].text_place, t.text_place);
}

/// Odd meters and fine rhythms: 7/8, a change to 5/16, quintuplets, septuplets, double dots and
/// 64ths.
const MS_RHYTHMS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<museScore version="4.60">
  <Score>
    <Division>480</Division>
    <Part id="1">
      <Staff id="1"/>
      <trackName>Flute</trackName>
      <Instrument id="flute"><longName>Flute</longName><Channel><program value="73"/></Channel></Instrument>
    </Part>
    <Staff id="1">
      <Measure>
        <voice>
          <TimeSig><sigN>7</sigN><sigD>8</sigD></TimeSig>
          <Chord><durationType>quarter</durationType><Note><pitch>64</pitch><tpc>18</tpc></Note></Chord>
          <Chord><durationType>quarter</durationType><Note><pitch>65</pitch><tpc>13</tpc></Note></Chord>
          <Chord><dots>1</dots><durationType>quarter</durationType><Note><pitch>67</pitch><tpc>15</tpc></Note></Chord>
        </voice>
      </Measure>
      <Measure>
        <voice>
          <TimeSig><sigN>5</sigN><sigD>16</sigD></TimeSig>
          <Chord><durationType>16th</durationType><Note><pitch>67</pitch><tpc>15</tpc></Note></Chord>
          <Chord><durationType>16th</durationType><Note><pitch>69</pitch><tpc>17</tpc></Note></Chord>
          <Chord><dots>1</dots><durationType>eighth</durationType><Note><pitch>71</pitch><tpc>19</tpc></Note></Chord>
        </voice>
      </Measure>
      <Measure>
        <voice>
          <TimeSig><sigN>4</sigN><sigD>4</sigD></TimeSig>
          <Tuplet><normalNotes>4</normalNotes><actualNotes>5</actualNotes><baseNote>16th</baseNote></Tuplet>
          <Chord><durationType>16th</durationType><Note><pitch>72</pitch><tpc>14</tpc></Note></Chord>
          <Chord><durationType>16th</durationType><Note><pitch>74</pitch><tpc>16</tpc></Note></Chord>
          <Chord><durationType>16th</durationType><Note><pitch>76</pitch><tpc>18</tpc></Note></Chord>
          <Chord><durationType>16th</durationType><Note><pitch>77</pitch><tpc>13</tpc></Note></Chord>
          <Chord><durationType>16th</durationType><Note><pitch>79</pitch><tpc>15</tpc></Note></Chord>
          <endTuplet/>
          <Tuplet><normalNotes>4</normalNotes><actualNotes>7</actualNotes><baseNote>16th</baseNote></Tuplet>
          <Chord><durationType>16th</durationType><Note><pitch>72</pitch><tpc>14</tpc></Note></Chord>
          <Chord><durationType>16th</durationType><Note><pitch>74</pitch><tpc>16</tpc></Note></Chord>
          <Chord><durationType>16th</durationType><Note><pitch>76</pitch><tpc>18</tpc></Note></Chord>
          <Chord><durationType>16th</durationType><Note><pitch>77</pitch><tpc>13</tpc></Note></Chord>
          <Chord><durationType>16th</durationType><Note><pitch>79</pitch><tpc>15</tpc></Note></Chord>
          <Chord><durationType>16th</durationType><Note><pitch>81</pitch><tpc>17</tpc></Note></Chord>
          <Chord><durationType>16th</durationType><Note><pitch>83</pitch><tpc>19</tpc></Note></Chord>
          <endTuplet/>
          <Chord><durationType>half</durationType><Note><pitch>84</pitch><tpc>14</tpc></Note></Chord>
        </voice>
      </Measure>
      <Measure>
        <voice>
          <Chord><dots>2</dots><durationType>eighth</durationType><Note><pitch>72</pitch><tpc>14</tpc></Note></Chord>
          <Chord><durationType>32nd</durationType><Note><pitch>74</pitch><tpc>16</tpc></Note></Chord>
          <Chord><durationType>64th</durationType><Note><pitch>76</pitch><tpc>18</tpc></Note></Chord>
          <Chord><durationType>64th</durationType><Note><pitch>74</pitch><tpc>16</tpc></Note></Chord>
          <Chord><durationType>64th</durationType><Note><pitch>72</pitch><tpc>14</tpc></Note></Chord>
          <Chord><durationType>64th</durationType><Note><pitch>71</pitch><tpc>19</tpc></Note></Chord>
          <Rest><dots>1</dots><durationType>eighth</durationType></Rest>
          <Rest><durationType>half</durationType></Rest>
        </voice>
      </Measure>
    </Staff>
  </Score>
</museScore>
"#;

#[test]
fn reads_odd_meters_and_fine_rhythms() {
    let s = from_bytes(MS_RHYTHMS.as_bytes()).unwrap();
    let lens: Vec<u32> = (0..4).map(|m| s.measure_ticks(m)).collect();
    assert_eq!(lens, [TPQ * 7 / 2, TPQ * 5 / 4, TPQ * 4, TPQ * 4]);
    let counts = |s: &Score| -> Vec<usize> {
        (0..4)
            .map(|m| s.voice(0, 0, m, 0).unwrap().events.len())
            .collect()
    };
    assert_eq!(counts(&s), [3, 3, 13, 8], "nothing padded or swallowed");
    for m in 0..4 {
        let sum: u32 = s
            .voice(0, 0, m, 0)
            .unwrap()
            .events
            .iter()
            .map(|e| e.ticks())
            .sum();
        assert_eq!(sum, s.measure_ticks(m), "measure {} is exactly full", m + 1);
    }
    let tuplets = &s.voice(0, 0, 2, 0).unwrap().events;
    assert_eq!(tuplets[0].duration.tuplet, Some(Tuplet { actual: 5, normal: 4 }));
    assert_eq!(tuplets[5].duration.tuplet, Some(Tuplet { actual: 7, normal: 4 }));
    assert_eq!(tuplets[5].ticks() * 7, TPQ, "septuplets are exact");
    let fine = &s.voice(0, 0, 3, 0).unwrap().events;
    assert_eq!(
        (fine[0].duration.value, fine[0].duration.dots),
        (NoteValue::Eighth, 2)
    );
    assert_eq!(fine[2].duration.value, NoteValue::N64th);
    // MusicXML keeps all of it.
    let back = from_str(&to_string(&s)).unwrap();
    assert_eq!(counts(&back), counts(&s));
    assert_eq!(back.voice(0, 0, 2, 0).unwrap().events, *tuplets);
    assert_eq!(back.measures[1].time, s.measures[1].time);
}

#[test]
fn reads_page_settings_from_the_style() {
    let xml = MS_RHYTHMS.replace(
        "<Score>",
        "<Score><Style><pageWidth>11.69</pageWidth><pageHeight>8.27</pageHeight><pagePrintableWidth>10.5</pagePrintableWidth><pageOddLeftMargin>0.5</pageOddLeftMargin><pageOddTopMargin>0.4</pageOddTopMargin><pageOddBottomMargin>0.6</pageOddBottomMargin><spatium>2</spatium><staffDistance>8</staffDistance><akkoladeDistance>5</akkoladeDistance><minSystemDistance>10</minSystemDistance></Style>",
    );
    let s = from_bytes(xml.as_bytes()).unwrap();
    let p = s.page;
    assert!(
        (p.width_mm - 296.9).abs() < 0.1 && (p.height_mm - 210.0).abs() < 0.1,
        "A4 landscape"
    );
    assert!((p.margin_left_mm - 12.7).abs() < 0.01 && (p.margin_right_mm - 17.5).abs() < 0.1);
    assert!((p.margin_top_mm - 10.16).abs() < 0.01 && (p.margin_bottom_mm - 15.24).abs() < 0.01);
    assert_eq!(
        (p.staff_space_mm, p.part_gap, p.staff_gap, p.system_gap),
        (2.0, 8.0, 5.0, 10.0)
    );
}

#[test]
fn page_style_defaults_of_musescore_3_and_4() {
    use scoreart_model::{TextFont, TextKind};
    let v = |version: &str| {
        MS_RHYTHMS.replacen(
            &MS_RHYTHMS[MS_RHYTHMS.find("<museScore").unwrap()..MS_RHYTHMS.find("<Score>").unwrap()],
            &format!("<museScore version=\"{version}\">\n  "),
            1,
        )
    };
    // MuseScore 4: margins 10 mm (bottom 20 mm), page number and copyright in the footer.
    let s4 = from_bytes(v("4.20").as_bytes()).unwrap();
    assert!((s4.page.margin_top_mm - 10.0).abs() < 0.01 && (s4.page.margin_bottom_mm - 20.0).abs() < 0.01);
    assert!((s4.page.staff_space_mm - 1.764).abs() < 0.001);
    assert!(!s4.header_footer.show_header);
    assert_eq!(s4.header_footer.footer[0][2], "$p");
    assert_eq!(s4.header_footer.footer[1][0], "$p");
    assert_eq!(s4.text_styles.get(TextKind::Title).size, 24.0);
    assert!(
        !s4.text_styles.get(TextKind::StaffText).italic,
        "MuseScore's staff text is upright"
    );
    assert_eq!(s4.page.music_top, 7.0);
    assert_eq!((s4.page.system_gap, s4.page.max_system_gap), (8.5, 15.0));
    // MuseScore 3: margins 15 mm, page numbers in the header, vertical justification.
    let s3 = from_bytes(v("3.02").as_bytes()).unwrap();
    assert!((s3.page.margin_top_mm - 15.0).abs() < 0.01 && (s3.page.margin_left_mm - 15.0).abs() < 0.01);
    assert!(s3.header_footer.show_header && s3.page.vertical_justify);
    assert_eq!(s3.header_footer.header[0][2], "$p");
    assert_eq!(s3.text_styles.get(TextKind::Title).size, 22.0);
    assert!(s3.page.first_indent > 0.0);
    // A style overrides them: two-sided, music margins, fill threshold, header, fonts.
    let styled = v("4.20").replace(
        "<Score>",
        "<Score><Style><pageWidth>8.27</pageWidth><pagePrintableWidth>7.0</pagePrintableWidth><pageOddLeftMargin>0.8</pageOddLeftMargin><pageEvenLeftMargin>0.47</pageEvenLeftMargin><pageTwosided>1</pageTwosided><staffUpperBorder>9</staffUpperBorder><lastSystemFillLimit>0.5</lastSystemFillLimit><showHeader>1</showHeader><oddHeaderC>$:workTitle:</oddHeaderC><titleFontSize>30</titleFontSize><titleFontStyle>1</titleFontStyle><staffFontFace>Arial</staffFontFace><staffFontStyle>2</staffFontStyle><measureSpacing>1.8</measureSpacing><footerOffset x=\"0\" y=\"3\"/></Style>",
    );
    let s = from_bytes(styled.as_bytes()).unwrap();
    let p = s.page;
    assert!(p.two_sided);
    assert!((p.margin_left_mm - 20.32).abs() < 0.01 && (p.even_margin_left_mm - 11.94).abs() < 0.01);
    assert!((p.margin_right_mm - (8.27 - 7.0 - 0.8) * 25.4).abs() < 0.01);
    assert_eq!(
        (p.music_top, p.last_system_fill, p.footer_offset),
        (9.0, 0.5, 3.0)
    );
    assert!((p.spacing - 1.2).abs() < 0.001, "1.8 against MuseScore 4's 1.5");
    assert!(s.header_footer.show_header && s.header_footer.header[0][1] == "$:workTitle:");
    let title = s.text_styles.get(TextKind::Title);
    assert_eq!((title.size, title.bold), (30.0, true));
    let staff = s.text_styles.get(TextKind::StaffText);
    assert_eq!((staff.font, staff.italic), (TextFont::Sans, true));
}

#[test]
fn texts_of_later_frames_become_headings_and_text_blocks() {
    let xml = MS_RHYTHMS.replacen(
        "<Measure>",
        "<VBox><Text><style>title</style><text>The title</text></Text></VBox><Measure>",
        1,
    );
    // A frame before the second measure and one after the last.
    let mut parts = xml.splitn(3, "<Measure>");
    let (a, b, c) = (
        parts.next().unwrap(),
        parts.next().unwrap(),
        parts.next().unwrap(),
    );
    let xml = format!(
        "{a}<Measure>{b}<VBox><Text><style>Title</style><text>Second piece</text></Text></VBox><Measure>{c}"
    );
    let end = xml.rfind("</Staff>").unwrap();
    let xml = format!(
        "{}<VBox><Text><style>Frame</style><text>A poem\nin two lines</text></Text></VBox>{}",
        &xml[..end],
        &xml[end..]
    );
    let s = from_bytes(xml.as_bytes()).unwrap();
    assert_eq!(s.meta.title, "The title");
    let n = s.measures.len();
    assert!(
        s.blocks
            .iter()
            .any(|b| b.kind == TextKind::Heading && b.text == "Second piece" && b.measure == 1)
    );
    assert!(
        s.blocks
            .iter()
            .any(|b| b.kind == TextKind::Text && b.text.starts_with("A poem") && b.measure == n)
    );
}
