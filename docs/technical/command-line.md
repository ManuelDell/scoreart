# Command line

`scoreart-cli` uses the same engine as the app, without a window.

```bash
scoreart-cli convert <in> <out>           # any readable format → .musicxml .mxl .mid .svg .wav .zip
scoreart-cli info <file>                  # what a file holds
scoreart-cli commands [--json]            # every command id, its shortcut and parameters
scoreart-cli run [--in FILE | --sample | --new] [--cmd ID[=JSON]]… [--save OUT]
scoreart-cli sounds list [LIBRARY]        # the catalogue of free sound libraries
scoreart-cli sounds licence LIBRARY
scoreart-cli sounds install LIBRARY INSTRUMENT --accept-licence [--dir DIR]
```

Every form takes `--sounds <file.sf2 | file.sfz | folder | file.zip>` (repeatable) to play with
sample libraries, for example when writing WAV.

Examples:

```bash
scoreart-cli convert song.mscz song.musicxml          # MuseScore → MusicXML
scoreart-cli convert song.musicxml song.svg           # engrave the first page
scoreart-cli convert song.musicxml stems.zip --sounds GeneralUser-GS.sf2

# Write a score from a script: every command of the app is available by id.
scoreart-cli run --new \
  --cmd 'note.enter={"step":"c"}' --cmd 'note.enter={"step":"e"}' \
  --cmd 'note.enter={"step":"g"}' --save chord.musicxml
```

`SOURCE_DATE_EPOCH` fixes the date used by the header and footer macros, for reproducible
output.

Because the CLI dispatches the same `CommandSpec`s as the ribbon and the shortcuts, anything a
user can do can be scripted or driven by an agent — the convention ScoreArt shares with the
ArtCraft apps ([why](artcraft-conventions.md)).
