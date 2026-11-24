# File formats and compatibility

| Format | Read | Write | Code |
|---|---|---|---|
| MusicXML 4.0 (`.musicxml`, `.xml`) | ✓ | ✓ (native format) | `crates/musicxml/src/{import,export}.rs` |
| Compressed MusicXML (`.mxl`) | ✓ | ✓ | `crates/musicxml/src/lib.rs` |
| MuseScore 3 and 4 (`.mscz`, `.mscx`) | ✓ | — (saved as MusicXML) | `crates/musicxml/src/mscz.rs` |
| MuseScore 2 (`.mscz`, `.mscx`) | ✓ | — | `crates/musicxml/src/mscz2.rs` → `mscz.rs` |
| Standard MIDI File (`.mid`) | ✓ | ✓ | `crates/midi/src/{import,lib}.rs` |
| SVG (first page) | — | ✓ | `crates/render` |
| WAV (whole score, or one per instrument) | — | ✓ | `crates/synth/src/wav.rs` |

All formats are implemented from **public specifications** (MusicXML 4.0 and SMuFL 1.4 of the
W3C Music Notation Community Group, Standard MIDI File 1.0) and, for MuseScore files, from
publicly known facts about the format and observation of files — never from MuseScore's GPL
source code.

## MusicXML

ScoreArt's own data that MusicXML has no element for — page settings, text styles and blocks,
locks, headers and footers, the user's breaks, sections and spacers, sounds and mixer — is
stored in `<miscellaneous-field name="scoreart-…">` entries, so a round trip through ScoreArt
loses nothing and other programs simply ignore it. Page margins (odd/even), the title page as
`<credit>`s and the start of every system and page (`<print new-system/new-page>`) are written
as standard MusicXML, so other programs show the same lines and pages.

Reading MusicXML from other programs takes care of what they really write:

- **Time**: composite signatures (`3+2`, or 3/8 + 2/8 + 3/4 as one signature of the same
  length), signatures up to 128 beats; durations rounded by the exporting program (a triplet in
  256ths of a quarter) snap to the sixteenth or triplet grid; overfull measures grow instead of
  losing notes (rests never grow a measure — Finale writes whole-measure rests in pickups);
  gaps of a tuplet's length are filled with tuplet rests.
- **Notes**: longa to 256th, up to four dots; lengths no single value has (a maxima) become
  tied notes; chords reaching into the other staff of a piano; cue notes and hidden playback
  notes (`print-object="no"`) take their time but show nothing; grace notes after a note (at
  the end of a measure, or `steal-time-previous`).
- **Directions**: `<offset>` moves hairpins, texts and dynamics to where the program shows them.
- **Layout**: `<print new-system/new-page>` of other programs become line and page breaks (as
  MuseScore does when it imports layout); ScoreArt keeps only the user's own breaks when it
  reads its own files back. Measure numbers that start at 1 again become section breaks.

## MuseScore files

`mscz.rs` reads MuseScore 3 and 4 files: parts and staves, voices, notes, chords, marks, lines,
chord symbols, repeats, jumps, frames and their texts, page layout and text styles from the
file's `<Style>` and the `.mss` style inside the `.mscz` — with the defaults of the MuseScore
version that wrote it for everything the file does not store (MuseScore writes only what
differs from its defaults; those defaults were read off third-party style files and observed
files).

MuseScore 2 files are first rewritten into the layout of MuseScore 3 files (`mscz2.rs`), then
read by the same code:

- voices by `<track>` (staff × 4 + voice), with `<tick>` moving the position back, become
  `<voice>` elements with `<location>` moves;
- lines declared where they start (`<HairPin id>`, `<Pedal>`, `<Ottava>`, `<Trill>`,
  `<TextLine>`, `<Volta>`) and ended by `<endSpanner>` become `<Spanner>` elements with the
  length to their end; slurs from the chord that starts them to the one that stops them, also
  into the other staff of a piano;
- measure rests `<duration z n>`, tuplets by id, text styles as `<TextStyle>` blocks.

MuseScore 1 files are refused with a message.

## How compatibility is tested

Besides the hand-written tests in the repository (`crates/musicxml/src/{tests,compat_tests,
mscz_tests}.rs`), ScoreArt was checked against public corpora. These files are not part of the
repository — files produced by other programs are never committed — but the method is simple to
repeat: for every file, a script lists what it contains, and ScoreArt's reading is compared
with it.

| Corpus | Files | Programs | Compared |
|---|---|---|---|
| [OpenScore Lieder](https://github.com/OpenScore/Lieder) (CC0) | 234 | MuseScore 2 and 3 | section breaks, spacers, measure numbers, playback pauses, header macros, text locks, MusicXML round trip; for MuseScore 2 also every note, grace note, lyric, tie, slur, hairpin and pedal |
| [music21 corpus](https://github.com/cuthbertLab/music21) | 302 | Finale, Sibelius, MuseScore, Noteflight, music21 | every note (part, measure, position, pitch), measure lengths, grace notes, ties, slurs, hairpins, lyrics, chord symbols, line and page breaks, round trip |
| [scorewriter comparison](https://github.com/eNote-GmbH/scorewriter-comparison) (CC BY 4.0) | 113 | Dorico, Finale, Sibelius, MuseScore | the same, plus the layout against each program's own PDF |
| [MusicXML test suite](https://github.com/w3c-cg/musicxmlTestSuite) (MIT) | ~170 | hand-written | the same |

Results at version 0.1:

- All 234 OpenScore songs open and pass every check; the MuseScore 2 songs match their files
  note for note.
- In the MusicXML corpora, the notes of every file match, except for nested tuplets
  (approximated), files with contradictory durations, and doubled hairpins or slurs (merged).
- System breaks: compared with the systems the exporting program wrote, ScoreArt now differs
  in 7 of 38 MuseScore files and 6 of 67 Sibelius files (Finale often writes only some system
  starts). Page images match Sibelius' and Dorico's own PDFs line for line in the test scores.

Known gaps: slurs from one staff into the other are drawn in the staff where they start; several
chord symbols on one note keep one; lyrics on single notes of a chord are read for the chord;
a file with no parts at all is refused.
