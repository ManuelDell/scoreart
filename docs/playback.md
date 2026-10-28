# How ScoreArt plays a score

Playback follows a few principles that open-source notation editors arrived at in public (the
MuseScore 4 handbook, its developers' blog posts and release notes describe them; ScoreArt's
implementation is its own):

1. **Pass on what the notation means, not a flattened MIDI note.** Every played note carries
   its marks (staccato, accent, under a slur, pizzicato, tremolo, muted) so that the sound can
   decide how to play it.
2. **Each instrument family plays its own way.**
3. **Dynamics are a curve, not only a velocity.**
4. **Playing techniques written as text switch the sound.**
5. **Everything printed is heard.**
6. **Balanced sound out of the box.**
7. **A human touch.**
8. **Every note lives**: it grows into its sound, rounds and lets go, like a spoken syllable.

## 1 and 4: marks and techniques reach the sound

`scoreart_midi::sequence` turns the score into played notes. Each has a `Tech`:

| In the score | `Tech` | With VSCO 2 CE (by name) | SoundFont / built-in |
|---|---|---|---|
| staccato, staccatissimo, spiccato | `short` | `…Spic` / `…Stac` instead of a sustained sound | the note is shortened |
| staff text "pizz." (until "arco"), snap / left-hand pizzicato | `Pizzicato` | `…Pizz` | General MIDI 45 (strings) |
| tremolo with 3+ strokes, buzz roll, "trem." | `Tremolo` | `…Trem`, held | General MIDI 44, or repeated notes |
| "con sord.", "mute" (until "senza sord.", "open") | `Muted` | `…Mute`, `…StraightMuteSus` | General MIDI 59 (trumpet) |
| "ord.", "nat." | back to normal | | |

Library sound names are split into the instrument and its technique words
("ViolinEnsSusVib" → "ViolinEns" + sustained), so the matching sound of the same instrument is
found in the loaded library (`crates/synth/src/variants.rs`). The user's choice stays the
default: a part set to a spiccato or pizzicato sound is never switched to another short sound.

## 2: families

The General MIDI program decides the family (`crates/midi/src/perform.rs`):

| Family | plain note | staccato | under a slur | accent / marcato |
|---|---|---|---|---|
| keys | 97 % | 50 % | as written | +18 / +26 |
| plucked, percussion | rings | 55 % | as written | +18 / +26 |
| bowed strings | 93 % | 45 % | runs 1/24 beat into the next note | +16 / +24 |
| winds, brass, voices | 90 % | 40 % | runs 1/32 beat into the next note | +16 / +24 |

Tenuto plays the full length, portato 70–85 %. The last note under a slur is let go.

## 3: dynamics

- Dynamic marks set a level per part.
- A hairpin is a swell: the loudness of the part moves smoothly from the level at its start to
  the dynamic at its end (or one step up or down when none is written). This also applies to
  notes already sounding, so a crescendo on a held note grows. Without a closing dynamic, the
  reached level stays.
- MIDI export writes hairpins as expression (CC 11).
- The level is turned into a velocity on a gentler curve, so that pp is still heard and ff is
  not overwhelming.

## 5: everything printed is heard

- **Repeats and endings**: `|: :|` with first, second, third endings ("1., 2."), D.C., D.S., al
  Fine, al Coda, To Coda. After a D.C./D.S. repeats are not taken again, and only the last ending
  is played (`crates/midi/src/order.rs`).
- **Fermatas** hold their note: ×2, short ×1.5, very short ×1.3, long ×2.5, very long ×3.
- **Breath marks and caesuras** stop the note early and add a pause (0.25 s; caesura 0.75 s).
- **accel., stringendo / rit., rall., allargando …** (tempo text, system text or staff text)
  change the tempo gradually: up to the next tempo mark or "a tempo" when one follows within
  four measures, otherwise by a quarter over four measures, and then the new tempo holds.
- **Swing**: "Swing" / "Shuffle" plays off-beat eighths 2:1 until "Straight".
- **Ornaments** are written out in the key: trills (also trill lines), mordents, short trills,
  turns, inverted turns, the schleifer; jazz scoops, plops, doits and falls; glissandos as
  chromatic runs; measured tremolos as repeated notes.
- **Octave lines** (8va, 8vb, 15ma …) sound where they say.
- **Pedal** and **let ring** hold notes to the end of the line; **l.v.** lets a note ring.

## 6: balance and room

- Every sampled sound is measured once (the loudest 50 ms of a mezzo-forte note at the part's
  middle key) and brought to the loudness of the built-in sound of its family. Samples recorded
  quieter or louder, a quickly dying pizzicato against a slow bow stroke, all sit at a common
  level before the mixer.
- A room (reverb, 25 % by default) is added; each side has its own, so placement in the stereo
  field stays. *Mixer → Room*.
- Files (WAV, stems) are rendered without a limiter and turned down as a whole until the loudest
  moment sits just under full scale.

## 7: human touch

Notes start a few milliseconds early or late (the same nudge for notes that start together, a
little more per note) and vary a few velocity steps. The variation is the same every time the
piece is played. *Mixer → Human touch* turns it off.

Room and human touch are saved in the score (MusicXML `<miscellaneous-field>`s).

## 8: the life of a note

A played note is not a box: it grows into its sound, rounds a little and lets go, like a
spoken syllable. Research on performance says the same:
- articulation is the ratio of sounding length to the time to the next note, staccato about
  0.4–0.6 (Bresin & Battel; the KTH performance rules);
- note amplitudes "hardly ever have sustained plateaux" (Clynes);
- players move the peak of a note earlier with a steep attack or later with a slow one to shape
  expression (studies of clarinettists by Schubert and colleagues).

ScoreArt gives every note of bowed strings, winds and voices a loudness curve on top of its
samples (`crates/synth/src/shape.rs`):

| Note | start | growing into the tone | arch | end |
|---|---|---|---|---|
| plain | 55 % | 35 ms (winds), 60 ms (bows), 70 ms (voices); louder is quicker | peaks at about a third, 1 dB deep | tapers to 65 % over the last 0.15 s |
| staccato | 40 % | 10 ms | early peak, falls steeply | down to 35 % over its second half |
| accent | 60 % | 8 ms, with a push of +30 % that falls back in about 0.1 s | early peak | as plain |
| legato (under a slur) | 75 % | 1.5 × slower | peaks in the middle | carries on into the next note |
| long (over 1.2 s) | | | a slow, wider arch (messa di voce) | tapers over up to 0.25 s |

A note leaning towards a higher next note peaks a little later, one falling to a lower note a
little earlier. Keys, plucked strings and percussion keep their natural shape.

- **SFZ and built-in sounds**: every voice follows its note's curve.
- **SoundFonts**: notes are spread over the MIDI channels so that each note's curve can be sent
  as its channel's expression.

Parts without a General MIDI program in the file (or with "piano" for a part whose name clearly
says otherwise, as older files were saved) take it from their name, in English or German:
"Violins I", "Violoncello", "Kontrabass", "Horn in F", "Pauken" …

