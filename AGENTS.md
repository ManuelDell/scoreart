# ScoreArt — instructions for agents

ScoreArt is a clean-room, open-source, Rust-native music notation editor (scorewriter) targeting
MuseScore Studio parity for everyday engraving — and going further on speed, openness and agent
control. It runs natively (macOS, Windows, Linux, BSD) and in the browser via WASM. It follows the
conventions of the ArtCraft apps (WordCraft, GridCraft …) but is **not** affiliated with ArtCraft
/ storytold or with MuseScore / Muse Group. Research and plan: [`docs/plan.md`](docs/plan.md).

## Never crash
People trust ScoreArt with their music; a crash loses their work. This outranks feature work.
- **No panics in non-test code:** no `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!`,
  `unimplemented!`; no `unsafe` (`unsafe_code = "forbid"`).
- **Errors are `Result<T, E>`** through the crate's error type and `?`. An unfinished feature
  returns an error or is disabled; it never panics.
- **Input-derived numbers are hostile** (files, commands, CLI params): `get()` instead of `[i]`,
  checked/saturating arithmetic, clamp sizes, cap allocations, bound recursion.
- **Prove it:** `crates/engine/src/tests.rs::hostile_params_never_panic` runs every command with
  junk params — keep it green. Every crash fix lands with a regression test.
- Production crate roots carry `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic,
  clippy::unimplemented, clippy::todo, clippy::unreachable)]`; new crates start with it.

## Clean room — MuseScore, Sibelius, Finale, Dorico
**These rules are non-negotiable.** Breaking them is the worst mistake you can make in this repo.
- MuseScore is **GPL**. Never read, copy, port or paraphrase MuseScore source code — not its
  layout/engraving code, not `instruments.xml`, styles (`.mss`), templates, palettes, workspaces,
  translations, test scores or tours. The same holds for any GPL/LGPL/AGPL notation code
  (LilyPond, Frescobaldi, Audiveris, MuseScore plugins …).
- Other scorewriters may be **observed** black-box only: run them, use their UI with synthetic
  scores, screenshot their window into `plan/screenshots/` (git-ignored, never committed). Never
  disassemble or copy anything inside an installed application; never commit files they produced.
- File formats come from **public specifications**: MusicXML 4.0 (W3C Community Group), SMuFL 1.4,
  Standard MIDI File 1.0. The MuseScore reader (`crates/musicxml/src/mscz.rs`) is built only from
  publicly known facts about `.mscz`/`.mscx` and from observing files — never from MuseScore source.
  Test files for it are written by hand for this repo; never commit files MuseScore produced.
- Engraving rules come from general music-engraving knowledge. Never copy text or music examples
  from books (*Behind Bars*, *The Art of Music Engraving* …).
- Never copy vendor wording beyond feature names.

## Assets: no vendor iconography, images or sounds
- No icons, images, templates, styles, fonts, cursors, sounds or soundfonts from MuseScore,
  Muse Group, Avid (Sibelius), MakeMusic (Finale), Steinberg (Dorico), Apple, Microsoft or Adobe —
  except fonts released under an open licence (Bravura, OFL) used unmodified.
- Every asset must be **original** (drawn in code — prefer this), **public domain / CC0**, **OSI
  open source**, or **Creative Commons that allows redistribution**.
- **Every asset file has a row in [`ATTRIBUTION.md`](ATTRIBUTION.md)**; `cargo xtask assets`
  (part of `cargo xtask ci`) fails otherwise. Add the row in the same commit as the file.
- Sample scores are written for this repo or are public-domain melodies; record the source.
- **Sample libraries are never committed** (no `.sf2`, `.sfz` libraries, sample WAV/FLAC files);
  users load their own. Muse Sounds are proprietary and must not be used, imitated or loaded by
  default. Sampler tests use tiny samples and SFZ text written in the test. The SFZ reader is
  written from the public format description (sfzformat.com) only, never from other samplers'
  source code.
- **Catalogue downloads** (`crates/synth/src/catalog.rs`): only libraries whose licence allows
  free use, fetched from the publisher's own public source; the licence summary (allows /
  requires / notes, "not legal advice") is shown in full and must be explicitly accepted before
  anything is fetched; the licence file and an acceptance record are saved with the files.
  Respect publishers who ask not to be hot-linked (open their website instead).
- Never use the ArtCraft or MuseScore names, logos or icons for ScoreArt (names may appear only
  descriptively, e.g. "opens MusicXML exported from MuseScore").

## Architecture (see [`docs/architecture.md`](docs/architecture.md))
| Layer | Crate | Job |
|---|---|---|
| L1 | `model` | score model + edit primitives |
| L2 | `musicxml`, `midi`, `engrave` | MusicXML I/O; MIDI export; layout to a display list |
| L3 | `render` | SVG export (Bravura outlines) |
| L4 | `engine` | `Session`, undo, **command registry**, samples, I/O dispatch |
| L6 | `ui-egui` | egui front end (swappable) |
| apps | `apps/scoreart`, `apps/scoreart-cli`, `apps/scoreart-web` | binaries |

- **Everything is a command.** User-visible behaviour = a `CommandSpec` in
  `crates/engine/src/cmd/*.rs` (id, label, menu, shortcut, params doc, `enabled`, `run`) + tests.
  Menus, toolbar, palettes, shortcuts, CLI and (later) MCP dispatch by id. Programmatic calls
  never open dialogs.
- **Layering** is enforced by `cargo xtask layers`: nothing below L6 depends on egui/eframe/rfd.
  The UI is thin and reads `Session` state.
- **Rust only** (no handwritten JS/TS). **Never break wasm** (`cargo xtask wasm`).

## Quality gates
Before every commit: `cargo xtask ci` (fmt, clippy -D warnings, tests, assets, layers, wasm).

## Running
- Native: `cargo run --release -p scoreart -- [file.musicxml]`
- Web: `cd apps/scoreart-web && trunk serve` (or `trunk build --release` → `dist/web/`)
- CLI: `scoreart-cli convert in.musicxml out.svg|out.mid|out.musicxml|out.mxl`,
  `scoreart-cli commands`, `scoreart-cli run --sample --cmd 'note.input.toggle' --cmd 'note.enter={"step":"c"}' --save out.musicxml`
- Headless screenshot of the UI: `cargo run -p scoreart-ui-egui --example ui_shot -- out.png`
