# Files

## Saving

ScoreArt saves **MusicXML** (`.musicxml`, or compressed `.mxl`) — the open format of music
notation. MuseScore, Sibelius, Finale, Dorico and most other programs open it. Your page
settings, texts, fonts, sounds and mixer are saved in it too, and ScoreArt writes where its
lines and pages start, so other programs show your score with the same lines.

## Opening

*File → Open* (`Ctrl+O`), or drop a file on the window. ScoreArt opens:

| File | From |
|---|---|
| `.musicxml`, `.mxl`, `.xml` | MusicXML from any program — Finale, Sibelius, Dorico, MuseScore, Noteflight … |
| `.mscz`, `.mscx` | **MuseScore 2, 3 and 4** directly, with page layout, texts and styles |
| `.mid`, `.midi` | MIDI files; rhythms are tidied to the simplest fitting notes, triplets included |

A MuseScore file is saved as MusicXML when you press *Save*; the original stays untouched.

ScoreArt was checked note by note against several hundred files from these programs — see
[File formats and compatibility](../technical/file-formats.md) for what that covered.

## Exporting

*File → Export*:

| Format | For |
|---|---|
| MusicXML / compressed MusicXML | other notation programs |
| MIDI | sequencers and DAWs |
| SVG (first page) | pictures for web pages and documents |
| WAV | listening, sharing |
| WAV per instrument (zip) | mixing in an audio program |

PDF export is on the [roadmap](../../ROADMAP.md).
