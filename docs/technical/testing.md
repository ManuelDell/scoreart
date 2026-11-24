# Testing and quality

## The gate

`cargo xtask ci` runs before every commit and in GitHub Actions on every push:

1. `cargo fmt --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. **assets**: every file under `assets/` and `docs/images/` has a row in `ATTRIBUTION.md`
5. **layers**: no crate depends on a crate of its own or a higher layer, and nothing below the
   UI depends on egui/eframe/winit/rfd
6. **wasm**: the web app builds and passes clippy for `wasm32-unknown-unknown`

## Never crash

People trust ScoreArt with their music, so a crash is the worst bug:

- production crates deny `unwrap`, `expect`, `panic!`, `unreachable!`, `todo!` and
  `unimplemented!`; `unsafe` is forbidden;
- every number that comes from a file, a command or the command line is treated as hostile:
  `get()` instead of indexing, checked arithmetic, clamped sizes (`model::limits`);
- `hostile_params_never_panic` runs every command with junk parameters in several states;
  `hostile_input_does_not_panic` and `hostile_musescore_files_do_not_panic` feed broken files;
- `Session::execute` restores the score when a command fails.

## What the tests cover

- **model**: note input, splitting and tying, normalisation, arranging instruments.
- **musicxml**: round trips; files in the style of Finale, Sibelius, Dorico and MuseScore
  exports (`compat_tests.rs`), MuseScore 2/3/4 files (`mscz_tests.rs`, hand-written).
- **engrave**: spacing, beams, stems, system and page breaking, spacers, page formatting.
- **engine**: every feature through its commands — layout, playback timing, macros, locks,
  sections, sounds.
- **ui-egui**: every visible text has a German translation.
- **synth**: the synthesizer, SoundFont and SFZ playback with tiny test samples written in the
  tests.

Corpus checks against real files of other programs are described in
[File formats and compatibility](file-formats.md#how-compatibility-is-tested).

## Clean room

Tests use only files written for this repository or public-domain music. Files produced by
MuseScore or other programs are never committed; their observation happens outside the
repository. See [AGENTS.md](../../AGENTS.md).
