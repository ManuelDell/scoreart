<p align="center">
  <img src="assets/app-icon/scoreart.svg" width="112" height="112" alt="ScoreArt icon: a staff with two beamed notes on a violet tile">
</p>

<h1 align="center">ScoreArt</h1>

<p align="center">
  <b>An open-source music notation editor, built in pure Rust.</b><br>
  Write, engrave and share sheet music — natively on Linux, macOS and Windows, and in the browser via WebAssembly.
</p>

<p align="center">
  <img alt="Written in Rust" src="https://img.shields.io/badge/written%20in-Rust-7048e8?style=flat-square&logo=rust&logoColor=white">
  <img alt="Runs on desktop and the web" src="https://img.shields.io/badge/runs%20on-Linux%20%C2%B7%20macOS%20%C2%B7%20Windows%20%C2%B7%20Web-7048e8?style=flat-square">
  <img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-7048e8?style=flat-square">
  <img alt="Status: early alpha" src="https://img.shields.io/badge/status-early%20alpha-orange?style=flat-square">
</p>

![ScoreArt in the browser, showing the Ode to Joy sample](docs/images/app.png)

ScoreArt is a scorewriter in the spirit of MuseScore, written from scratch. It follows the
engineering conventions of the ArtCraft apps (Rust only, egui, one command registry for UI,
CLI and agents, WebAssembly build, strict clean-room rules) — but it is an **independent
project, not affiliated with ArtCraft / storytold or with MuseScore / Muse Group.**

## What works today

- **Engraving** with the SMuFL reference font Bravura: clefs, key and time signatures, notes,
  chords (with seconds displaced), rests and whole-measure rests, accidentals (key- and
  measure-aware), dots, ledger lines, stems, flags, **beams** (grouped by beat, sloped, with
  secondary beams and beamlets), **ties** (also across systems), articulations, fermatas,
  dynamics, lyrics, tempo marks, system text, tuplet numbers, barlines and repeats, braces and
  brackets, instrument names, measure numbers, title block — with automatic spacing,
  justified system breaks and page breaks.
- **Note input** the way scorewriter users expect: `N` toggles note input, `A`–`G` enter
  pitches (octave nearest the previous note, key signature applied), `1`–`8` pick durations
  (5 = quarter), `.` dots, `0` rests, `Shift+A`–`G` add chord notes, `↑`/`↓` semitones,
  `Ctrl+↑/↓` octaves, `T` ties, `Del` deletes, notes crossing a barline are split and tied,
  four voices, mouse input with a ghost note.
- **Editing**: selection of notes, rests and measure ranges; palettes for clefs, keys, times,
  articulations, dynamics, barlines, repeats and tempo; add/insert/delete measures; add,
  rename and remove instruments (17 built-in); transpose; score properties; unlimited undo/redo.
- **Files**: **MusicXML 4.0** is the native format (`.musicxml` and compressed `.mxl`, read and
  write — exchange scores with MuseScore, Dorico, Sibelius, Finale), **MIDI** export, **SVG**
  export with real glyph outlines.
- **Web build** with a `?host=parent` `postMessage` bridge (`scoreart:ready/open/save/dirty`) so
  a same-origin page — for example a Nextcloud app — can open and save scores in it.
- **CLI** for conversion and for driving every command headlessly (agents, scripts).

Not yet: playback, PDF export, slurs, hairpins, tuplet input, multi-measure rests, part
extraction, transposing instruments, percussion and tablature — see [ROADMAP.md](ROADMAP.md).

## Build and run

Needs Rust ≥ 1.90.

```bash
cargo run --release -p scoreart -- [score.musicxml]        # desktop app
cargo run --release -p scoreart-cli -- convert sample out.svg
cargo run --release -p scoreart-cli -- commands             # every command id
cargo run --release -p scoreart-cli -- run --new \
  --cmd 'note.enter={"step":"c"}' --cmd 'note.enter={"step":"e"}' --save out.musicxml

# Web (needs `rustup target add wasm32-unknown-unknown` and `cargo install trunk`)
cd apps/scoreart-web && trunk serve        # http://127.0.0.1:8790
trunk build --release                      # → dist/web/
```

Quality gate before every commit: `cargo xtask ci` (fmt, clippy `-D warnings`, tests, asset
attribution, crate layering, wasm build).

## Keyboard

| Key | Action | Key | Action |
|---|---|---|---|
| `N` | note input on/off | `Esc` | leave note input / clear selection |
| `A`–`G` | enter note | `Shift+A`–`G` | add note to chord |
| `1`…`8` | 64th … breve (5 = quarter) | `.` | dot |
| `0` | rest | `T` | tie |
| `↑` `↓` | semitone up/down | `Ctrl+↑` `Ctrl+↓` | octave |
| `Alt+Shift+↑/↓` | diatonic step | `←` `→` | previous/next note |
| `Ctrl+←/→` | previous/next measure | `Ctrl+Alt+↑/↓` | staff above/below |
| `Ctrl+Alt+1…4` | voice | `Del` | delete |
| `Ctrl+B` | append measure | `Ins` | insert measure |
| `Ctrl+Z` / `Ctrl+Y` | undo / redo | `Ctrl+L` | lyric |
| `Ctrl+S` / `Ctrl+Shift+S` | save / save as | `Ctrl+O` / `Ctrl+N` | open / new |

## How it is built

| Layer | Crate | Job |
|---|---|---|
| L1 | `model` | score model and edit primitives |
| L2 | `musicxml`, `midi`, `engrave` | MusicXML I/O · MIDI · layout to a display list |
| L3 | `render` | SVG export with Bravura outlines |
| L4 | `engine` | session, undo, command registry |
| L6 | `ui-egui` | the egui front end |
| apps | `scoreart`, `scoreart-cli`, `scoreart-web` | desktop, CLI, browser |

Details: [docs/architecture.md](docs/architecture.md). Research and plan: [docs/plan.md](docs/plan.md).

## Legal

- Code: **MIT OR Apache-2.0** ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).
- Music font: **Bravura** © Steinberg Media Technologies GmbH, SIL Open Font License 1.1,
  bundled unmodified ([assets/fonts/OFL-Bravura.txt](assets/fonts/OFL-Bravura.txt)). Every asset
  is listed in [ATTRIBUTION.md](ATTRIBUTION.md); see also [NOTICE](NOTICE).
- **Clean room**: no MuseScore (GPL) or other copyleft notation code is read or used; file
  formats come from public specifications (MusicXML 4.0 and SMuFL 1.4 of the W3C Music Notation
  Community Group, Standard MIDI File 1.0). Rules for contributors and agents: [AGENTS.md](AGENTS.md).
- MuseScore is a trademark of Muse Group; Sibelius of Avid; Finale of MakeMusic; Dorico of
  Steinberg. These names are used only to describe file compatibility. ScoreArt is not
  affiliated with, sponsored or endorsed by any of them, nor by ArtCraft / storytold.
