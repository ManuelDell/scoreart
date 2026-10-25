# Architecture

> **Status:** 2026-10-25 · first version.

ScoreArt is a Cargo workspace of small crates in strict layers (checked by `cargo xtask layers`):
a crate may only depend on crates of a lower layer, and nothing below the UI depends on egui,
eframe or rfd.

```
apps/scoreart  apps/scoreart-cli  apps/scoreart-web        binaries
        └──────────┬──────────────────┘
L6  crates/ui-egui        menus, toolbar, palettes, properties, canvas, keyboard
L4  crates/engine         Session · undo/redo · command registry · sample · file I/O
L3  crates/render         SVG from the display list (Bravura outlines via ttf-parser)
L3  crates/synth          built-in synthesizer: Sequence → stereo f32 frames
L2  crates/engrave        layout → pages of shapes + hit boxes + staff boxes
L2  crates/musicxml       MusicXML 4.0 / MXL read & write
L2  crates/midi           tie-merged playback sequence · Standard MIDI File
L1  crates/model          Score, parts, staves, measures, voices, events; edit primitives
```

## Model (`crates/model`)

- Time in ticks, `TPQ = 1920` per quarter (2⁷·3·5: 64ths, triplets and quintuplets are exact).
- `Score { meta, measures: Vec<MeasureInfo>, parts: Vec<Part> }`. `MeasureInfo` holds global
  changes (time, key, tempo, barline, repeat start, system text).
- `Part { clefs (one per staff), measures: Vec<PartMeasure> }` → `StaffMeasure { voices, clef change }`
  → `Voice { events }` → `Event { kind: Rest | Chord(notes), duration, articulations, dynamic, lyric }`.
- **Invariant:** voice 0 of every staff measure exactly fills the measure; `Score::normalize`
  restores all invariants after any edit or import (pads with rests, cuts overflow).
- `edit.rs`: `overwrite` implements scorewriter note input (replace what is there, split and
  tie across barlines, append measures), plus chords, rests, measure insert/delete, transpose.
- Hard limits (`model::limits`) cap hostile input.

## Engraving (`crates/engrave`)

Coordinates are **staff spaces** (sp); SMuFL glyphs are drawn with an em of 4 sp.

1. **Accidentals**: per staff and measure, all voices in time order, against the key and the
   measure's running alterations; tied-over notes carry no accidental.
2. **Horizontal plan** per measure: the union of onsets across all staves becomes columns;
   each column gets lead room (accidentals) and a duration-dependent space
   (≈ 3 sp × (Δ/quarter)^0.55) plus room for dots, flags, displaced seconds and lyrics.
3. **System breaking**: greedy fill of the line width minus indent (instrument names) and the
   system header (clef, key, time); then justification by stretching the spaces (the last
   system is left natural when it is less than ¾ full).
4. **Vertical**: staves of a part 6 sp apart, parts 7 sp, systems 9 sp; extra room for lyrics
   and text above; pages break when the next system does not fit.
5. **Drawing** per staff measure and voice: stem directions (by the farthest note from the middle
   line; by voice when voices are shared), seconds displaced, stems ≥ 3.5 sp reaching the middle
   line, beam groups by beat (compound meters by dotted beat), sloped beams (limited slope,
   minimum stem lengths, secondary beams and beamlets), flags, ledger lines, dots in spaces,
   stacked accidentals, articulations opposite the stem (fermata above the staff), dynamics and
   lyrics under the staff, tuplet numbers.
6. **Ties** are drawn last, when both ends are known; a tie that crosses a system break is
   split into an outgoing and an incoming half.

Output: `Layout { pages: [Page { items: [Item { shape, owner }] }], hits, staves }`. `owner` is the
event a shape belongs to (selection colouring); `hits` map clicks to notes, rests and measures;
`StaffBox` gives each staff/measure's position, clef and tick→x columns (cursor, mouse input).

## Engine (`crates/engine`)

- `Session { score, style, cursor, selection, input, undo/redo, layout cache }`.
- **Everything is a command**: `CommandSpec { id, label, menu, shortcut, params, enabled, run,
  undoable }` in `cmd/{edit,note,score}.rs`. `Session::execute(id, params)` snapshots the score
  for undoable commands, restores it if the command fails, and catches an escaped panic as a
  last resort. `cargo run -p scoreart-cli -- commands` lists all of them.
- `hostile_params_never_panic` runs every command with junk parameters in several states.

## UI (`crates/ui-egui`)

Thin egui layer: menu bar, toolbar (durations, accidentals, voices — glyphs from Bravura), left
palettes, right properties, status bar, canvas (pages, selection, cursor, ghost note), dialogs
(new score, text, shortcuts, about). Keys map to command ids. Platform services (open, save,
embedded mode) come in through the `Host` trait.

## Apps

- `apps/scoreart`: eframe (glow) desktop app; rfd file dialogs; drag-and-drop.
- `apps/scoreart-cli`: `convert`, `info`, `commands`, `run --cmd id=json … --save`.
- `apps/scoreart-web`: eframe web runner (WebGL2), file picker and downloads, `?host=parent`
  bridge (`scoreart:ready`, `scoreart:open {name, bytes}`, `scoreart:save {name, bytes}`,
  `scoreart:dirty {dirty}`), same-origin parent only.

## Playback

`midi::sequence` flattens the score (repeats unrolled, ties merged, dynamics → velocity,
staccato shortened, accents louder) into notes with score ticks plus a tempo map.
`synth::Synth` turns that into audio: per note a voice reading two single-cycle wavetables
(bright/mellow, crossfaded over time and by pitch to avoid aliasing) of one of eight
instrument families chosen by the General MIDI program, with an attack/decay/release envelope,
vibrato for strings, voice and winds, breath noise for flutes, stereo panning per part and a
soft limiter. The UI asks the `Host` for audio: desktop pulls blocks in a cpal callback; the
browser renders 0.2 s chunks scheduled 0.6 s ahead on an `AudioContext`. The playhead follows
the audio clock (`Host::audio_position`) through the tempo map back to measure and tick.
