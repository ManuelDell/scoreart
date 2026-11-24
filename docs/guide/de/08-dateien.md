# Dateien

## Speichern

ScoreArt speichert **MusicXML** (`.musicxml` oder komprimiert `.mxl`) — das offene Format für
Notensatz. MuseScore, Sibelius, Finale, Dorico und die meisten anderen Programme öffnen es.
Seiteneinstellungen, Texte, Schriften, Klänge und Mixer werden darin mitgespeichert, und
ScoreArt schreibt hinein, wo seine Zeilen und Seiten beginnen — so zeigen andere Programme
deine Partitur mit denselben Zeilen.

## Öffnen

*Datei → Öffnen…* (`Strg+O`) oder eine Datei aufs Fenster ziehen. ScoreArt öffnet:

| Datei | Von |
|---|---|
| `.musicxml`, `.mxl`, `.xml` | MusicXML aus jedem Programm — Finale, Sibelius, Dorico, MuseScore, Noteflight … |
| `.mscz`, `.mscx` | **MuseScore 2, 3 und 4** direkt, mit Seitenlayout, Texten und Stilen |
| `.mid`, `.midi` | MIDI-Dateien; Rhythmen werden auf die einfachsten passenden Notenwerte gebracht, Triolen eingeschlossen |

Eine MuseScore-Datei wird beim *Speichern* als MusicXML gesichert; das Original bleibt
unverändert.

ScoreArt wurde Note für Note an mehreren hundert Dateien dieser Programme geprüft — Details
(englisch) unter [Dateiformate und Kompatibilität](../../technical/file-formats.md).

## Exportieren

*Datei → Exportieren*:

| Format | Wofür |
|---|---|
| MusicXML / komprimiertes MusicXML | andere Notensatzprogramme |
| MIDI | Sequenzer und DAWs |
| SVG (erste Seite) | Bilder für Webseiten und Dokumente |
| WAV | anhören, weitergeben |
| WAV pro Instrument (Zip) | Abmischen in einem Audioprogramm |

PDF-Export steht auf der [Roadmap](../../../ROADMAP.md).
