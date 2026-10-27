# ScoreArt roadmap

**Stage: early alpha** · target: MuseScore Studio parity for everyday engraving

> **Last updated:** 2026-10-27

## Done (M0–M3)

- Workspace, rules (`AGENTS.md`), licences, attribution check, `cargo xtask ci`
  (fmt, clippy, tests, assets, layers, wasm).
- Model, MusicXML 4.0 / MXL round trip, MIDI export, SVG export.
- Engraver: spacing, justified systems, pages, stems, beams, flags, accidentals, ledger lines,
  dots, ties (also across systems), rests, multiple voices, chords with seconds, clefs, keys,
  times, barlines, repeats, braces, brackets, articulations, dynamics, lyrics, tempo, text.
- Playback (alpha): built-in synthesizer with eight instrument families, playhead, note
  audition while editing, repeats, dynamics; cpal on desktop, Web Audio in the browser; WAV export.
- Sample playback: SoundFont 2 (`rustysynth`) and an own SFZ sampler (WAV/FLAC, layers, round
  robins, release samples, crossfades, loops); mixer with one track per instrument (sound,
  volume, pan, mute, solo, meters), saved in MusicXML; stems export (one WAV per instrument);
  libraries remembered on desktop, zip/sf2 in the browser, `--sounds` on the CLI.
- Catalogue of free libraries (VSCO 2 CE, VCSL, Salamander, GeneralUser GS): per-instrument
  download from the publishers' sources after explicit licence acceptance, with the licence and
  an acceptance record stored next to the files; desktop, browser and `scoreart-cli sounds`.
  Next for playback: articulation keyswitches driven by the notation, swing, metronome, loop,
  ritardando, reverb send, SFZ filters/LFOs, loading libraries by URL in the browser.
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
  be re-saved in MuseScore 3 or 4 first. The reader was checked black-box against files saved by
  MuseScore Studio 4.7 (ScoreArt score → MusicXML → MuseScore → `.mscz` → ScoreArt gives the same
  score back); the conventions found are covered by hand-written test files. Marks that
  MuseScore's own MusicXML import drops (shake, circled notehead, vibrato line, wavy glissando)
  cannot come back that way.
