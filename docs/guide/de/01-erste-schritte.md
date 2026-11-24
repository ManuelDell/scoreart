# Erste Schritte

## Installieren

Lade ScoreArt von der [Release-Seite](https://github.com/ManuelDell/scoreart/releases)
herunter:

| Dein Computer | Download | Dann |
|---|---|---|
| **Windows** | `scoreart-…-windows.zip` | Entpacken und `scoreart.exe` starten. Windows warnt evtl. vor einem unbekannten Herausgeber: *Weitere Informationen → Trotzdem ausführen*. |
| **macOS** | `scoreart-…-macos.tar.gz` | Entpacken, dann beim ersten Mal Rechtsklick auf `scoreart` → *Öffnen* (die App ist noch nicht signiert). |
| **Linux** | `scoreart-…-linux.tar.gz` | Entpacken und `./scoreart` starten. |
| **Browser** | `scoreart-…-web.zip` | Eine Web-Version für jeden Webserver; sie läuft komplett im Browser. |

Du kannst ScoreArt auch selbst aus dem Quellcode bauen — siehe die
[README](../../../README.md#for-developers).

## Das Fenster

![Das ScoreArt-Fenster](../../images/app-de.png)

- **Titelleiste** (oben): Speichern, Rückgängig, Wiederherstellen, Abspielen; der Dateiname;
  und der Wechsel zwischen den vier **Ansichten** — *Ribbon*, *Seitenleiste*, *Beides*
  (Standard) und *Fokus*.
- **Ribbon**: Reiter mit Knöpfen, nach Aufgaben gruppiert. *Start* enthält, was du ständig
  brauchst (Noteneingabe, Notenwerte, Vorzeichen, Stimmen, Wiedergabe); *Notation* und *Weitere
  Zeichen* enthalten die Zeichen; dann folgen *Takte*, *Text*, *Instrumente*, *Format* und
  *Ansicht*. Ein ausgegrauter Knopf kann mit der aktuellen Auswahl nichts anfangen — sein
  Tooltip sagt dir, warum. Ein Doppelklick auf einen Reiter (oder `Strg+F1`) klappt das Ribbon
  ein.
- **Seitenleiste** (links): *Paletten* mit allen Zeichen, *Instrumente*, *Eigenschaften* der
  Auswahl und *Klänge*. Ein Klick auf den offenen Reiter klappt die Leiste ein.
- **Die Partitur** in der Mitte, so wie sie gedruckt wird.
- **Statuszeile** (unten): was ausgewählt ist, der Eingabemodus, die Größe der Partitur.

**Fokus** zeigt nur die Partitur und eine kleine schwebende Leiste — gut zum Schreiben ohne
Ablenkung:

![Die Fokus-Ansicht](../../images/de/focus.png)

## Deine erste Partitur

1. *Datei → Neue Partitur…* (`Strg+N`).
2. Gib Titel und Komponist ein und wähle die **Instrumente** (Klavier, Gesang, Chor,
   Streicher, Bläser …), die **Taktart**, die **Tonart**, die Zahl der Takte und das Tempo.
3. Klicke auf **Erstellen**. Die leere Partitur erscheint und wartet auf Noten — weiter geht
   es mit [Noten schreiben](02-noten-schreiben.md).

Zum Umschauen öffnet *Datei → Beispiele → Ode an die Freude* das Beispiellied. Drücke die
**Leertaste**, um es zu hören.

## Rückgängig, speichern, zoomen

- **Rückgängig / Wiederherstellen**: `Strg+Z` / `Strg+Y` — so oft du willst.
- **Speichern**: `Strg+S`. ScoreArt speichert MusicXML (`.musicxml`), das jedes
  Notensatzprogramm öffnen kann. Siehe [Dateien](08-dateien.md).
- **Zoom**: `Strg +` / `Strg −` oder *Ansicht → Zoom*. Unter *Ansicht* wechselst du außerdem
  zwischen **Seitenansicht**, **durchgehender Ansicht** (eine endlose Zeile) und einer
  vertikalen Ansicht.
