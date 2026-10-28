# Asset attribution

Every non-code asset in this repository is listed here with its author, source and licence.
`cargo xtask assets` (part of `cargo xtask ci`) fails if an asset file is missing from this table.

**Policy (mandatory):** ScoreArt contains **no MuseScore, Muse Group, Avid (Sibelius),
MakeMusic (Finale), Steinberg (Dorico), Apple, Microsoft or Adobe iconography, images,
templates, styles, sounds or soundfonts**. The only third-party assets are two fonts released under
the SIL Open Font License and bundled unmodified: the Bravura music font (Steinberg) and the
Crimson Text font (The Crimson Text Project Authors). Every other asset is
original work by ScoreArt contributors. Screenshots of other vendors' software are never committed.
**No sample library or soundfont is bundled**: users load their own (see `docs/sounds.md`); tests
use samples and SFZ files written for this repository.

Generated-in-code art is original and has no file to list: the toolbar and palette buttons
(`crates/ui-egui/src/panels.rs`), the instrument sounds of the built-in synthesizer (`crates/synth/src/lib.rs`: wavetables from
harmonic recipes and envelopes written for ScoreArt — no samples, no soundfont), the brace and tie shapes (`crates/engrave/src/layout.rs`) and
the sample score (`crates/engine/src/sample.rs`, an arrangement written for ScoreArt of the
public-domain "Ode to Joy" theme by Ludwig van Beethoven, 1824, with words by Friedrich
Schiller, 1785).

| Asset | Author | Source | Licence | Notes |
|---|---|---|---|---|
| `assets/fonts/Bravura.otf` | Steinberg Media Technologies GmbH (Daniel Spreadbury) | https://github.com/steinbergmedia/bravura (`redist/otf/Bravura.otf`) | SIL Open Font License 1.1, Reserved Font Name "Bravura" (`assets/fonts/OFL-Bravura.txt`) | SMuFL reference font; bundled unmodified. Metrics used in `crates/engrave/src/glyphs.rs` and `crates/engrave/src/smufl.rs` come from its published `bravura_metadata.json` (same licence); glyph names and code points are those of the SMuFL 1.4 specification (W3C Community Final Specification Agreement) |
| `assets/fonts/OFL-Bravura.txt` | (licence text) | https://github.com/steinbergmedia/bravura (`redist/OFL.txt`) | — | |
| `assets/fonts/CrimsonText-Regular.ttf` | The Crimson Text Project Authors (Sebastian Kosch) | https://github.com/google/fonts (`ofl/crimsontext/CrimsonText-Regular.ttf`); project https://github.com/googlefonts/Crimson | SIL Open Font License 1.1 (`assets/fonts/OFL-CrimsonText.txt`) | Serif text font for titles and texts; bundled unmodified |
| `assets/fonts/OFL-CrimsonText.txt` | (licence text) | https://github.com/google/fonts (`ofl/crimsontext/OFL.txt`) | — | |
| `assets/app-icon/scoreart.svg` | ScoreArt contributors | original artwork drawn in SVG for this repository | MIT OR Apache-2.0 | App icon: a staff with two beamed notes on a violet tile |
| `docs/images/app.png` | ScoreArt contributors | screenshot of ScoreArt itself (ribbon on the Notation tab, side panel on the palettes tab) showing its own sample score | MIT OR Apache-2.0 | Original; no third-party UI or assets |
| `docs/images/instruments.png` | ScoreArt contributors | screenshot of ScoreArt itself (its Instruments tab and instrument-change question on a test score written for this repository) | MIT OR Apache-2.0 | Original; no third-party UI or assets |
| `docs/images/midi-input.png` | ScoreArt contributors | screenshot of ScoreArt itself (its MIDI input window; the device name comes from a simulated test device) | MIT OR Apache-2.0 | Original; no third-party UI or assets |
| `docs/images/page-settings.png` | ScoreArt contributors | screenshot of ScoreArt itself (its page settings dialog over its own sample score) | MIT OR Apache-2.0 | Original; no third-party UI or assets |
| `docs/images/continuous-view.png` | ScoreArt contributors | screenshot of ScoreArt itself (its continuous view of its own sample score) | MIT OR Apache-2.0 | Original; no third-party UI or assets |
| `docs/images/focus.png` | ScoreArt contributors | screenshot of ScoreArt itself (its focus layout over its own sample score) | MIT OR Apache-2.0 | Original; no third-party UI or assets |
| `docs/images/app-de.png` | ScoreArt contributors | screenshot of ScoreArt itself (its German interface showing its own sample score) | MIT OR Apache-2.0 | Original; no third-party UI or assets |
| `docs/images/licence-dialog.png` | ScoreArt contributors | screenshot of ScoreArt itself (its licence confirmation for a catalogue download) | MIT OR Apache-2.0 | Original; shows only ScoreArt's own summary of the CC0 licence |
| `docs/images/mixer.png` | ScoreArt contributors | screenshot of ScoreArt itself (its showcase score and mixer) | MIT OR Apache-2.0 | Original; the preset name "Violin" shown comes from a user-loaded GeneralUser GS file, which is not part of the repository |
| `docs/examples/mountain-king-vsco.mxl` | ScoreArt contributors | arrangement written for this repository of Edvard Grieg's "In the Hall of the Mountain King" (Peer Gynt, 1875; public domain) | MIT OR Apache-2.0 | Original orchestration; names VSCO 2 CE presets as its sounds but contains no samples |
| `docs/images/fonts.png` | ScoreArt contributors | screenshot of ScoreArt | MIT OR Apache-2.0 | Shows the Fonts window; texts in Crimson Text (OFL), music in Bravura (OFL) |
| `docs/images/ode-to-joy.svg` | ScoreArt contributors | rendered by `scoreart-cli convert sample` | MIT OR Apache-2.0 | Engraved with Bravura (OFL) outlines |
| `docs/images/notation-showcase.svg` | ScoreArt contributors | rendered by `scoreart-cli convert` from a score written for this repository | MIT OR Apache-2.0 | Shows the notation marks; engraved with Bravura (OFL) outlines |
