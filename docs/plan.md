# ScoreArt — research and plan

> **Status:** 2026-10-25 · first plan, written before the first line of code.

ScoreArt is a clean-room, open-source, Rust-native **music notation editor** (a scorewriter),
targeting MuseScore Studio parity for everyday engraving work — and built the way the ArtCraft
apps (WordCraft, GridCraft, DeckCraft, PdfCraft …) are built. It is **not** an ArtCraft product
and is not affiliated with ArtCraft / storytold or with MuseScore / Muse Group.

## 1. What we learned from the ArtCraft apps

Source: `storytold/wordcraft`, `storytold/gridcraft` (MIT OR Apache-2.0) and
<https://getartcraft.com/apps>.

| Topic | ArtCraft convention | ScoreArt |
|---|---|---|
| Language | Rust only, no handwritten JS/TS | same |
| UI | egui/eframe, thin layer over the engine, swappable | same (`crates/ui-egui`) |
| Targets | native macOS/Windows/Linux/BSD + WebAssembly in the browser | same (`apps/scoreart`, `apps/scoreart-web`) |
| Architecture | strict layers L0–L6, enforced by `cargo xtask layers`; nothing below the UI depends on egui | same |
| Commands | *everything is a command*: a `CommandSpec` (id, label, menu, shortcut, params, enabled, run); UI, shortcuts, CLI, control channel and MCP all dispatch by id | same (`crates/engine/src/cmd`) |
| Native format | the open standard (OOXML for WordCraft, XLSX for GridCraft) | **MusicXML** (`.musicxml`, `.mxl`) |
| Agents | CLI + MCP server drive the same commands | CLI now, MCP next |
| Robustness | "never crash": no `unwrap/expect/panic` in non-test code, `unsafe` forbidden, hostile input clamped, a test that runs every command with junk params | same |
| Web embedding | `?host=parent` `postMessage` bridge: `<app>:ready` → `<app>:open {name, bytes}` → `<app>:save {name, bytes}`, same-origin only | same: `scoreart:ready/open/save` — drop-in for the existing Nextcloud integration |
| Quality gate | `cargo xtask ci` = fmt, clippy `-D warnings`, tests, assets, layers, wasm | same |
| Licence | code MIT OR Apache-2.0; per-file `ATTRIBUTION.md` table checked by `cargo xtask assets`; `NOTICE` | same |

### How ArtCraft stays legally safe (and what ScoreArt copies from that)

1. **Clean room.** The target app may only be *observed* black-box (run it, use its UI,
   screenshot its window into a git-ignored folder). Never read, disassemble or copy anything
   inside it; never commit files it produced; never copy its wording beyond feature names.
2. **No copyleft code.** File formats come from **public specifications**; GPL/LGPL/AGPL code
   (LibreOffice, pandoc …) is never read or copied.
3. **No vendor assets.** No icons, images, templates, themes, fonts, sounds from Microsoft, Adobe,
   Avid, Autodesk. Icons are drawn in code; every asset file has an `ATTRIBUTION.md` row
   (author, source, licence) and CI fails without it.
4. **Proprietary fonts are never bundled;** documents asking for them get open substitutes.
5. **Trademarks.** Target-app names appear only descriptively ("opens MusicXML exported from
   MuseScore"). The ArtCraft marks themselves are *not* open source: their brand licence forbids
   using the ArtCraft name/logo for anything other than the official app, and forks must remove
   them. → ScoreArt uses **none** of the ArtCraft marks, icons or names, and has its own brand.

## 2. The legal situation for a MuseScore alternative

| Item | Facts | Consequence for ScoreArt |
|---|---|---|
| MuseScore Studio code | GPL (v3 for MuseScore 4, v2 for older versions), with a CLA | **Never read or copy** MuseScore source, including its engraving/layout code, `instruments.xml`, styles, templates, palettes, translations. Our code is MIT/Apache, so any GPL fragment would contaminate it. |
| MuseScore assets | icons, UI fonts, Leland/MuseJazz fonts, MuseSounds, the MuseScore look | not used. (Leland is OFL, but we pick the SMuFL reference font instead so nothing is MuseScore-branded.) |
| "MuseScore" name / logo | trademark of Muse Group | only descriptive use; never in our product name, icon, store listing or marketing. |
| `.mscz` format | no formal spec; a zip with one XML `.mscx` file; file formats are not protected by copyright, reading them for interoperability is lawful | **read-only import** of MuseScore 3/4 files, written from publicly known facts about the XML structure and from black-box observation of files — never from MuseScore's GPL source. Test files are hand-written. Checked by running MuseScore Studio as a black box (convert a ScoreArt MusicXML file to `.mscz`, read it back, compare); those generated files stay out of the repository. Saving goes to MusicXML. |
| MusicXML 4.0 | W3C Community Group report under the W3C Community **Final Specification Agreement** (royalty-free commitments; XSD/DTD freely redistributable) | native file format. |
| SMuFL 1.4 | W3C Community Group spec (same FSA) | glyph code points and engraving metrics. |
| Bravura | SMuFL reference font by Steinberg, **SIL OFL 1.1**, reserved font name "Bravura" | bundled unmodified in `assets/fonts/` with `OFL-Bravura.txt`; if we ever subset or modify it, the result must be renamed. |
| Standard MIDI File 1.0 | spec published by the MIDI Association | MIDI export (crate `midly`, Unlicense). |
| Engraving rules (e.g. from *Behind Bars*) | facts and conventions are not copyrightable; the book's text and examples are | rules implemented from general knowledge; never copy text or example scores. |
| Keyboard shortcuts / workflow (N, A–G, 1–8 durations …) | functional, not copyrightable | MuseScore-like note input so users feel at home; own UI design, own icons. |
| Sample scores | must be our own or public domain | only scores written for this repo or public-domain melodies (e.g. folk tunes), recorded in `ATTRIBUTION.md`. |
| Sound | sample libraries carry their own licences; Muse Sounds are proprietary (licence limited to Muse products) | built-in synthesis by default; **user-loaded** SoundFont (`rustysynth`, MIT) and SFZ (own sampler from the public sfzformat.com description; FLAC via `claxon`, Apache-2.0). Nothing bundled; recommended free libraries (VSCO 2 CE / VCSL CC0, Salamander CC BY, GeneralUser GS) in `docs/sounds.md`. Bundling one later needs a permissive licence and an `ATTRIBUTION.md` row. |

**Name check:** a web search for "ScoreArt" as music software found no product. ("ScoreCloud",
"ScoreWriter" exist.) The name contains neither "MuseScore" nor "ArtCraft". Before a public
release, do a real trademark search (EUIPO/DPMA/USPTO).

## 3. Architecture

| Layer | Crate | Job |
|---|---|---|
| L1 | `model` | score model: parts, staves, measures, voices, events, pitches, durations; edit primitives (overwrite note input, fill with rests, ties) |
| L2 | `musicxml`, `midi` | MusicXML 4.0 import/export (`.musicxml`, `.mxl`); Standard MIDI File export |
| L2 | `engrave` | layout: spacing, system and page breaking, stems, beams, accidentals, ledger lines, ties, dots, rests, clefs/keys/times, text; outputs a renderer-neutral display list plus hit-test boxes |
| L3 | `render` | SVG export of the display list with real Bravura outlines (via `ttf-parser`) |
| L4 | `engine` | `Session`: score, cursor, selection, note-input state, undo/redo, **command registry**, samples, I/O dispatch |
| L6 | `ui-egui` | egui front end: menu, toolbar, palettes, score canvas, properties, status bar |
| apps | `scoreart`, `scoreart-cli`, `scoreart-web` | native app, CLI (convert / commands / run), browser build with `?host=parent` bridge |

Units: layout works in **staff spaces** (sp); SMuFL fonts are designed so that 1 em = 4 sp.

## 4. Milestones

- **M0 — foundation (this session):** workspace, rules (`AGENTS.md`), licences, attribution
  check, xtask (ci, assets, layers, wasm).
- **M1 — core notation:** model, MusicXML round trip, engraving of single-voice and multi-voice
  staves (clefs, keys, times, notes, rests, chords, accidentals, dots, ties, beams, flags,
  ledger lines, articulations, dynamics, lyrics, tempo), page layout, SVG and MIDI export.
- **M2 — editing:** MuseScore-like note input (N, A–G, 1–8, `.`, 0, arrows, Ctrl+arrows, T,
  Del), selection, undo/redo, palettes, add/remove parts and measures, score properties.
- **M3 — apps:** native app, CLI, web app with host bridge.
- **M4 — next:** playback (built-in synth, web audio), PDF export (`krilla`), MCP server, slurs,
  tuplets input, multi-measure rests, parts extraction, transposing instruments, percussion,
  guitar tab, chord symbols, `.mid` import, Nextcloud app integration, i18n (German first).
