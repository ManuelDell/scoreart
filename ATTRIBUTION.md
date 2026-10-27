# Asset attribution

Every non-code asset in this repository is listed here with its author, source and licence.
`cargo xtask assets` (part of `cargo xtask ci`) fails if an asset file is missing from this table.

**Policy (mandatory):** ScoreArt contains **no MuseScore, Muse Group, Avid (Sibelius),
MakeMusic (Finale), Steinberg (Dorico), Apple, Microsoft or Adobe iconography, images,
templates, styles, sounds or soundfonts**. The only third-party asset is the Bravura music font,
released by Steinberg under the SIL Open Font License and bundled unmodified. Every other asset is
original work by ScoreArt contributors. Screenshots of other vendors' software are never committed.

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
| `assets/app-icon/scoreart.svg` | ScoreArt contributors | original artwork drawn in SVG for this repository | MIT OR Apache-2.0 | App icon: a staff with two beamed notes on a violet tile |
| `docs/images/app.png` | ScoreArt contributors | screenshot of ScoreArt itself showing its own sample score | MIT OR Apache-2.0 | Original; no third-party UI or assets |
| `docs/images/ode-to-joy.svg` | ScoreArt contributors | rendered by `scoreart-cli convert sample` | MIT OR Apache-2.0 | Engraved with Bravura (OFL) outlines |
| `docs/images/notation-showcase.svg` | ScoreArt contributors | rendered by `scoreart-cli convert` from a score written for this repository | MIT OR Apache-2.0 | Shows the notation marks; engraved with Bravura (OFL) outlines |
