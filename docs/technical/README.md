# ScoreArt technical documentation

How ScoreArt is built, for contributors and the curious. The rules every change must follow are
in [AGENTS.md](../../AGENTS.md).

1. [Architecture](architecture.md) — crates and layers, the score model, engraving, commands,
   the apps
2. [Built like the ArtCraft apps](artcraft-conventions.md) — the structure ScoreArt shares with
   WordCraft and GridCraft, and why
3. [File formats and compatibility](file-formats.md) — MusicXML, MuseScore 2/3/4, MIDI, SVG,
   WAV, and how they are tested against other programs
4. [Testing and quality](testing.md) — the CI gate, never-crash rules, corpus checks
5. [Command line](command-line.md) — converting files and driving every command
6. [Playback](../playback.md) and [sounds](../sounds.md) — how a score is performed; sample
   libraries, the catalogue and the mixer
7. [Research and plan](../plan.md) — the first plan, including the legal situation of a
   MuseScore alternative

The user guide is in [../guide](../guide/README.md).
