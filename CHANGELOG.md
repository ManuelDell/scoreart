# Changelog

## Unreleased

- **PDF export**: every page, with Bravura and Crimson Text embedded (krilla).
- **PDFs as originals**: opening a PDF shows it in a panel beside the score (*View → PDF*).
- **PDF to score**: the music of PDFs written by notation programs — MuseScore, Sibelius,
  Finale, Dorico, ScoreArt — is read back into an editable score (staves, clefs, keys, times,
  notes, rests, voices, ties, slurs, lyrics, dynamics, titles); measures it is unsure about are
  listed. The command line converts too: `scoreart-cli convert score.pdf score.musicxml`.
- **Chord symbols sound**: from each symbol to the next, bass and chord, with an instrument and
  volume of their own per part (*Instruments* tab), as in MuseScore; MIDI export gives them a
  track of their own.
- **Songbooks from MuseScore look as there**: empty staves hidden per system, empty frames
  keeping their room, rehearsal marks, segno/coda and jumps where they were moved to, German
  chord names, instrument names as the style says, ♯ and ♭ in chord symbols of PDFs; systems
  set as close as MuseScore sets them (least distance from staff to staff, more only where
  lyrics and chord symbols would touch), chord symbols just above the staff, lyrics 3 spaces
  below it, and lines no longer squeezed so far that lyrics overlap.
- **Overlaps marked in red** on the screen (never printed), counted in the status bar,
  switchable under *View → Overlaps*: texts, lyrics, chord symbols, rehearsal marks, dynamics
  and notes that run into each other. Fixed on the way: dynamics of sung staves go above the
  staff, staves inside a system keep lyrics clear of the next staff, a clef change at the end
  of a MuseScore measure holds from the next one. Then many fewer of them: dynamics and staff
  texts move clear of notes, beams and texts; room is kept for beams (also slanted ones) and
  multi-beam stems; text blocks above the first system leave room for its tempo mark; lyric
  hyphens stay between their syllables, also on small staves. From MuseScore: dynamics and
  staff texts where their offset puts them, invisible dynamics and tempo marks (heard, not
  shown), staff texts that take no room, their size and alignment, tempo marks counted in the
  note they show, heavy-cross noteheads, beamed groups on the middle line with stems up.
- **Layout settings in ScoreArt**: instrument names (full/short/none, first and other
  systems), naming a single instrument and German chord names in the page settings; *Blank
  space…* after a system; rehearsal marks, segno/coda and jumps dragged with the hand (and
  reset); **small staves** (70 %) per instrument, also from MuseScore files.
- **Text fields for worksheets**: text blocks get a look of their own — font, size, bold,
  italic, underlined, alignment, frame, grey background, width, lines to write on (beside a
  short label like "Name:"), and fields side by side in one row. Fields take room of their own:
  the music moves aside, also when a field is dragged, and a field between two systems no
  longer runs into the one above. New command `block.look`.
- **Chord symbols move by hand** like texts; while a text, dynamic or chord symbol (or a
  rehearsal mark) is dragged, a dashed line shows the note it stays attached to and its measure
  is outlined. Saved in MusicXML (`relative-x`/`relative-y` of `<harmony>`).
- **Tied notes are held in playback** also where the tie goes on in another voice or staff of
  the instrument (as in piano music); a tie that leads nowhere no longer swallows a later note
  of the same pitch — that one is played again.
- **Opening files in the browser** goes straight to the system's file dialog; where the
  browser does not allow that, a small window in ScoreArt's look (with dropping a file onto it)
  replaces the plain, unstyled one.
- **LilyPond PDFs** are converted too: the glyph names of its music font are read from the
  PDF; octave-treble clefs with the "8" as a separate glyph, older LilyPond fonts, beams drawn
  with outlines and scores followed by their parts in the same file are handled.

## 0.1.0 — 2026-11-24

The first release: ScoreArt writes, engraves, plays and exchanges real scores.

- **Writing**: note input by keyboard, mouse (pen and hand) and MIDI keyboard; four voices;
  chords, ties, lyrics with verses; pickups and odd measures; drag notes, texts, dynamics and
  lines.
- **Notation**: the full set of everyday and rare marks — articulations, fermatas, ornaments,
  30 dynamics, slurs, hairpins, octave, pedal, trill and glissando lines, grace notes before and
  after the note, tremolos, arpeggios, noteheads, repeats and jumps, chord symbols, fingering,
  rehearsal marks, tempo; longa to 256th notes.
- **Pages**: engraving with Bravura; page settings, headers and footers with macros, line, page
  and section breaks, spacers, vertical justification — compatible with MuseScore's page
  formatting; text blocks that move freely or are locked; fonts with a live preview.
- **Playback**: built-in synthesizer, SoundFont and SFZ libraries, a mixer per instrument, free
  library catalogue with licence acceptance; everything printed is heard; WAV and stems export.
- **Files**: MusicXML 4.0 as the native format; MuseScore 2, 3 and 4 files; MusicXML of
  Finale, Sibelius, Dorico and others, checked against public corpora; MIDI in and out; SVG.
- **Everywhere**: Windows, macOS, Linux and the browser; English and German; a command line
  that drives every command.
