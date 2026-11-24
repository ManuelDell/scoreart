# Changelog

## Unreleased

- **PDF export**: every page, with Bravura and Crimson Text embedded (krilla).
- **PDFs as originals**: opening a PDF shows it in a panel beside the score (*View → PDF*).
- **PDF to score**: the music of PDFs written by notation programs — MuseScore, Sibelius,
  Finale, Dorico, ScoreArt — is read back into an editable score (staves, clefs, keys, times,
  notes, rests, voices, ties, slurs, lyrics, dynamics, titles); measures it is unsure about are
  listed. The command line converts too: `scoreart-cli convert score.pdf score.musicxml`.

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
