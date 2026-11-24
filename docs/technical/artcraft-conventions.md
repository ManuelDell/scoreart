# Built like the ArtCraft apps

ScoreArt is organised the way the [ArtCraft apps](https://getartcraft.com/apps) are built —
WordCraft, GridCraft, DeckCraft, PdfCraft and their siblings, whose public repositories
(`storytold/wordcraft`, `storytold/gridcraft`, MIT OR Apache-2.0) show a clear recipe for a
desktop-and-web office application in Rust. ScoreArt applies that recipe to music notation.

> ScoreArt is **not** an ArtCraft product and is not affiliated with ArtCraft / storytold. It
> uses none of their names, logos, icons or code — only the publicly described engineering
> conventions below.

| Topic | ArtCraft convention | In ScoreArt |
|---|---|---|
| Language | Rust only, no hand-written JavaScript or TypeScript | same; the web app is the same Rust code compiled to WebAssembly |
| User interface | egui/eframe, a thin layer over the engine, swappable | `crates/ui-egui` (L6); nothing below it knows egui |
| Targets | native macOS, Windows, Linux and the browser | `apps/scoreart`, `apps/scoreart-web` |
| Layers | small crates in strict layers L1–L6, checked by `cargo xtask layers` | model → formats/engraving → render/synth → engine → UI ([architecture](architecture.md)) |
| Commands | *everything is a command*: a `CommandSpec` with id, label, menu, shortcut, params, enabled and run; UI, shortcuts, CLI and agents dispatch by id | `crates/engine/src/cmd`; `scoreart-cli commands` lists them, `scoreart-cli run` drives them |
| Ribbon | office-style ribbon of grouped commands, folded with a double-click | `crates/ui-egui/src/ribbon.rs`, plus a MuseScore-like side panel and a focus layout |
| Native format | the open standard of the domain (OOXML for WordCraft, XLSX for GridCraft) | **MusicXML 4.0** (`.musicxml`, `.mxl`) |
| Robustness | never crash: no `unwrap`/`expect`/`panic` in non-test code, no `unsafe`, hostile input clamped, a test runs every command with junk parameters | same ([testing](testing.md)) |
| Embedding | `?host=parent` `postMessage` bridge: `<app>:ready` → `<app>:open` → `<app>:save`, same-origin only | `scoreart:ready/open/save/dirty` |
| Quality gate | `cargo xtask ci`: fmt, clippy `-D warnings`, tests, assets, layers, wasm | same |
| Licences | code MIT OR Apache-2.0; every asset with a row in `ATTRIBUTION.md`, checked in CI | same |
| Clean room | competitors are only observed from the outside; their code and assets are never used | same, with MuseScore's GPL code as the hard line ([AGENTS.md](../../AGENTS.md)) |

## Why this structure

- **One engine, many fronts.** Because every action is a command with an id, the ribbon, the
  side panel, keyboard shortcuts, the command line and automated agents all do exactly the
  same thing. A feature is written once, tested once, and is scriptable from day one.
- **The UI can be replaced.** egui sits on top; the engine, engraver and file formats do not
  depend on it. The SVG export, the CLI and the tests use the same layout code as the screen.
- **The web is not a second product.** The browser build is the same crates compiled to
  WebAssembly, with a `Host` implementation for files, audio and MIDI.
- **Open format first.** Saving MusicXML means a score from ScoreArt opens in every other
  notation program, and nothing is ever locked in.
- **Safe to depend on.** Never-crash rules, clamped input and the clean-room discipline keep
  the code trustworthy and the licence clean.
