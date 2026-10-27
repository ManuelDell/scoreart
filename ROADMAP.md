# ScoreArt roadmap

**Stage: early alpha** · target: MuseScore Studio parity for everyday engraving

> **Last updated:** 2026-10-25 · first public state

## Done (M0–M3)

- Workspace, rules (`AGENTS.md`), licences, attribution check, `cargo xtask ci`
  (fmt, clippy, tests, assets, layers, wasm).
- Model, MusicXML 4.0 / MXL round trip, MIDI export, SVG export.
- Engraver: spacing, justified systems, pages, stems, beams, flags, accidentals, ledger lines,
  dots, ties (also across systems), rests, multiple voices, chords with seconds, clefs, keys,
  times, barlines, repeats, braces, brackets, articulations, dynamics, lyrics, tempo, text.
- Playback (alpha): built-in synthesizer with eight instrument families, playhead, note
  audition while editing, repeats, dynamics; cpal on desktop, Web Audio in the browser; WAV export.
  Next for playback: mute/solo and volume per instrument, swing, metronome, loop, ritardando,
  better instrument sounds (optional permissively licensed soundfonts).
- The full catalogue of notation marks (articulations, fermatas, breaths, ornaments, techniques,
  slides, 30 dynamics, all lines incl. slurs/hairpins/8va/pedal/trill/glissando, grace notes,
  tremolos, arpeggios, noteheads, voltas, jumps, rehearsal marks, chord symbols, fingerings,
  texts) — engraved, editable, round-tripped through MusicXML, mostly played back.
- Note input and editing with undo/redo; palettes; properties; desktop, CLI and web apps;
  `?host=parent` bridge for embedding.

## Next (ranked)

| # | Gap | Why it matters | Estimate |
|---|---|---|---|
| 2 | **PDF export** (krilla, MIT/Apache) and printing | Sheet music is printed | 4–6 h |
| 4 | **Tuplet input** (`Ctrl+3` …) and tuplet brackets | Common rhythm | 4–6 h |
| 5 | **Copy/paste, range selection across staves** | Everyday editing | 6–10 h |
| 6 | **Nextcloud app** (reuse the `?host=parent` bridge, like the ArtCraft-Office integration) | Use it where the files are | 3–5 h |
| 7 | Multi-measure rests, part extraction, transposing instruments | Ensemble work | 12–20 h |
| 8 | Collision avoidance (lyrics, dynamics, articulations), better beam slopes, cross-staff beaming | Engraving quality | ongoing |
| 9 | MIDI import; MusicXML corpus tests against files from other scorewriters | File exchange | 8–12 h |
| 10 | Chord symbols, guitar tablature, percussion notation | Popular use | 15–25 h |
| 11 | MCP server (agents drive the same commands) | Agent control | 3–5 h |
| 12 | Interface translations (German first) | Reach | 4–6 h |

## Known limitations

- Ornaments (trills, turns, mordents) and tremolos are shown but not played as figures yet.
- No automatic collision avoidance between marks of neighbouring notes beyond stacking per note.

- A time signature change cuts or pads the following measures instead of re-barring.
- Only the first part's key/time/tempo is read from MusicXML (scores with per-part keys are
  imported with the first part's key).
- MuseScore files (`.mscz`/`.mscx`, versions 3 and 4) open read-only; layout settings, beaming
  overrides, multi-staff spans and some text styles are not taken over. MuseScore 1/2 files must
  be re-saved in MuseScore 3 or 4 first. The reader is tested with hand-written files; a test
  against real MuseScore-made files is still open.
