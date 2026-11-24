# Architecture

ScoreArt is a Cargo workspace of small crates in strict layers. A crate may only depend on
crates of a lower layer, and nothing below the user interface depends on egui, eframe, winit or
rfd. `cargo xtask layers` (part of `cargo xtask ci`) checks both. The layering follows the
ArtCraft apps — see [Built like the ArtCraft apps](artcraft-conventions.md).

```
apps/scoreart   apps/scoreart-cli   apps/scoreart-web            binaries
        └───────────────┬───────────────────┘
L6  crates/ui-egui     ribbon, side panel, canvas, dialogs, mixer, i18n (egui)
L4  crates/engine      Session · undo/redo · command registry · MIDI input · file dispatch
L3  crates/render      SVG from the display list (Bravura outlines via ttf-parser)
L3  crates/synth       built-in synthesizer, SoundFont and SFZ sampler, mixer, reverb, catalogue
L2  crates/engrave     layout: spacing, systems, pages → shapes, hit boxes, staff boxes
L2  crates/musicxml    MusicXML 4.0 / MXL read & write; MuseScore 2/3/4 import
L2  crates/midi        playback sequence (performance) · Standard MIDI File in and out
L1  crates/model       Score, parts, measures, staves, voices, events; edit primitives
```

## Model (`crates/model`)

- Time in ticks, `TPQ = 13440` per quarter (2⁷·3·5·7: 256ths, triplets, quintuplets and
  septuplets are exact). Note values from longa to 256th, up to four dots, tuplets.
- `Score { meta, measures: Vec<MeasureInfo>, parts, spanners, page, text styles, blocks,
  header/footer, playback style }`. `MeasureInfo` holds what is global to a measure: time, key,
  tempo, barline, repeats, voltas, jumps, breaks, section break, its own length (pickups).
- `Part → PartMeasure → StaffMeasure { voices, clef change, spacers } → Voice { events } →
  Event { Rest | Chord(notes), duration, articulations, dynamic, lyrics, grace notes, tremolo,
  arpeggio, chord symbol, text }`. Lines (slurs, hairpins, octave, pedal, trill, glissando …)
  are `Spanner`s anchored at (measure, tick) in a voice.
- **Invariant:** voice 0 of every staff measure exactly fills the measure.
  `Score::normalize` restores every invariant after an edit or import (pads with rests — tuplet
  rests where a tuplet's room is left — and cuts overflow) and clamps everything to
  `model::limits`.
- `edit.rs`: scorewriter note input (overwrite, split and tie across barlines), chords, rests,
  measure insert/delete, transpose. `arrange.rs`: splitting and merging instruments.
- `text.rs`: text kinds and styles, text blocks (with offsets and locks), headers and footers
  with their macros.

## Engraving (`crates/engrave`)

Coordinates are **staff spaces**; SMuFL glyphs are drawn with an em of 4 staff spaces. Text
widths come from the advance widths of the bundled serif (`serif.rs`).

1. **Accidentals** per staff and measure against the key and earlier alterations.
2. **Horizontal plan** per measure: the onsets of all staves become columns with lead room
   (accidentals, grace notes, arpeggios) and a duration-dependent space, plus room for dots,
   flags, displaced seconds, lyrics and graces after a note.
3. **Systems**: greedy filling of the line; a line, page or section break ahead keeps its
   measures in one system when they fit with the notes squeezed to 55 % of their room; then
   justification.
4. **Vertical**: staves, instruments and systems at the page settings' distances, more for
   notes far outside the staff, lyrics and texts, and spacers (up, down, fixed). Pages are
   filled down to the music bottom margin (vertical justification spreads staves too); fixed
   spacers keep their distance.
5. **Drawing**: stems, beams (by beat, sloped, secondary beams), flags, ledger lines, dots,
   ties (split across systems), marks, lyrics, tuplets, lines across systems, title blocks,
   headers and footers (macros filled per page).

Output: `Layout { pages: [Page { items: [Item { shape, owner }] }], hits, staves, marks }`.
`owner` is the event a shape belongs to (selection colouring); `hits` map clicks to notes,
rests, texts, lines, blocks and measures; `StaffBox` gives each staff measure's place and its
tick → x columns; `marks` are the editor-only signs for breaks and spacers.

## Engine (`crates/engine`)

- `Session { score, cursor, selection, input, undo/redo, layout cache, view, sounds, clock … }`.
- **Everything is a command**: `CommandSpec { id, label, menu, shortcut, params, enabled, run,
  undoable }` in `cmd/{edit,note,marks,score,text}.rs` — about a hundred of them. The ribbon,
  menus, palettes, shortcuts, the CLI and scripts all dispatch by id. `Session::execute(id,
  params)` snapshots the score for undoable commands, restores it when a command fails, and
  catches an escaped panic as a last resort.
- `midi_input.rs`: MIDI keyboard actions and bindings. `sample.rs`: the sample score.
- File dispatch: MusicXML, MXL, MuseScore (`.mscz`/`.mscx`), MIDI in; MusicXML, MXL, MIDI, SVG,
  WAV and stems out.

## User interface (`crates/ui-egui`)

A thin egui layer over the engine: title bar with the four layouts (ribbon, side panel, both,
focus), the ribbon (`ribbon.rs`), the side panel with palettes, instruments, properties and
sounds (`side.rs`, `panels.rs`, `instruments.rs`), the canvas (`canvas.rs`: pages, selection,
ghost note, dragging, editor-only marks), dialogs (page settings, header and footer, fonts,
texts, sections), the mixer and the sound catalogue. Every text goes through `i18n::t` /
`tf`; the German table is `i18n_de.rs`, and a test fails for any text without a translation.
Platform services (files, audio, settings, clock) come in through the `Host` trait, so the same
UI runs on the desktop and in the browser.

## Apps

- `apps/scoreart` — desktop (eframe/glow), rfd file dialogs, drag and drop, cpal audio.
- `apps/scoreart-cli` — `convert`, `info`, `commands`, `run --cmd id=json … --save`, `sounds`.
  See [Command line](command-line.md).
- `apps/scoreart-web` — WebAssembly (eframe web, WebGL2), Web Audio, Web MIDI, file picker and
  downloads, and the `?host=parent` bridge (`scoreart:ready`, `scoreart:open {name, bytes}`,
  `scoreart:save {name, bytes}`, `scoreart:dirty`) for embedding in a same-origin page.

## Playback

`midi::sequence` turns the score into a performance: repeats and jumps unrolled, ties merged,
articulation, slurs and dynamics applied per instrument family, ornaments and tremolos played
out, tempo changes, swing, fermatas, breaths and section pauses in a tempo map. `synth` renders
it with the built-in synthesizer or loaded samples, per instrument track, with reverb and a
limiter. Details: [docs/playback.md](../playback.md) and [docs/sounds.md](../sounds.md).
