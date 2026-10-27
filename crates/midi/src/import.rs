//! Reading Standard MIDI Files into a score.
//!
//! MIDI stores performance, not notation, so this is an interpretation:
//! - Note times are quantised to sixteenths.
//! - Each track (or each channel of a single-track file) becomes a part.
//! - Piano-like parts are split at middle C onto two staves; other parts get the clef that fits
//!   their range.
//! - Overlapping notes are cut at the next onset, and notes starting together form chords.
//! - Drums (channel 10) are left out.
//! - Tempo, time and key signatures and track names are taken over.

use std::collections::BTreeMap;

use midly::{MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};
use scoreart_model::edit::Pos;
use scoreart_model::{
    Clef, Duration, EventKind, INSTRUMENTS, Instrument, KeySig, Note, Pitch, Score, TPQ, Tempo, TimeSig,
    clip, instrument, limits,
};

/// The finest note value kept.
const GRID: u32 = TPQ / 4;
/// Most notes read from one file.
const MAX_NOTES: usize = 200_000;

#[derive(Clone, Copy, Debug)]
struct RawNote {
    start: u64,
    end: u64,
    key: u8,
}

#[derive(Default)]
struct Voice {
    name: String,
    program: u8,
    notes: Vec<RawNote>,
}

/// A part to create: one staff per merged track.
struct PartPlan {
    name: String,
    program: u8,
    track: usize,
    staves: Vec<Vec<RawNote>>,
    hidden: bool,
}

/// An instrument template for a General MIDI program: the built-in one with that program, or
/// one of the same family.
fn template(program: u8) -> &'static Instrument {
    if let Some(i) = INSTRUMENTS.iter().find(|i| i.program == program) {
        return i;
    }
    let id = match program {
        0..=15 => "piano",
        16..=23 => "organ",
        24..=39 => "guitar",
        40 | 44..=51 => "violin",
        41 => "viola",
        42 => "cello",
        43 => "contrabass",
        52..=55 | 85 | 91 => "voice",
        56 | 59..=63 => "trombone",
        57 => "trombone",
        58 => "tuba",
        64..=69 => "oboe",
        70 | 71 => "bassoon",
        72..=79 => "flute",
        _ => "piano",
    };
    instrument(id)
        .or_else(|| INSTRUMENTS.first())
        .unwrap_or(&FALLBACK)
}

static FALLBACK: Instrument = Instrument {
    id: "piano",
    name: "Piano",
    abbreviation: "Pno.",
    program: 0,
    clefs: &[Clef::Treble, Clef::Bass],
};

/// Reads a Standard MIDI File.
pub fn from_smf(bytes: &[u8]) -> Result<Score, String> {
    let smf = Smf::parse(bytes).map_err(|e| format!("MIDI: {e}"))?;
    let ppq = match smf.header.timing {
        Timing::Metrical(t) => u64::from(t.as_int().max(1)),
        Timing::Timecode(..) => return Err("MIDI files timed in SMPTE frames are not supported".into()),
    };
    let to_ticks = |t: u64| -> u64 {
        let raw = t.saturating_mul(u64::from(TPQ)) / ppq;
        (raw + u64::from(GRID) / 2) / u64::from(GRID) * u64::from(GRID)
    };

    let mut tempo: Option<u32> = None;
    let mut times: BTreeMap<u64, (u8, u8)> = BTreeMap::new();
    let mut key: Option<i8> = None;
    let mut title: Option<String> = None;
    // Voices keyed by (track, channel).
    let mut voices: BTreeMap<(usize, u8), Voice> = BTreeMap::new();
    let mut total = 0usize;
    let single = smf.tracks.len() == 1;
    // Channel programs are shared by all tracks.
    let mut programs: [Option<u8>; 16] = [None; 16];
    let mut first_track_name: Option<String> = None;

    for (ti, track) in smf.tracks.iter().enumerate().take(limits::MAX_PARTS * 4) {
        let mut t = 0u64;
        let mut track_name: Option<String> = None;
        let mut open: BTreeMap<(u8, u8), Vec<u64>> = BTreeMap::new();
        for ev in track {
            t = t.saturating_add(u64::from(ev.delta.as_int()));
            match ev.kind {
                TrackEventKind::Meta(MetaMessage::Tempo(us)) if tempo.is_none() => {
                    tempo = Some(us.as_int().max(1));
                }
                TrackEventKind::Meta(MetaMessage::TimeSignature(n, d, _, _)) => {
                    times.entry(to_ticks(t)).or_insert((n, d));
                }
                TrackEventKind::Meta(MetaMessage::KeySignature(sf, _)) if key.is_none() => key = Some(sf),
                TrackEventKind::Meta(MetaMessage::TrackName(n)) => {
                    let n = String::from_utf8_lossy(n).trim().to_string();
                    if !n.is_empty() {
                        if ti == 0 && first_track_name.is_none() {
                            first_track_name = Some(n.clone());
                        }
                        track_name = Some(n);
                    }
                }
                TrackEventKind::Midi { channel, message } => {
                    let ch = channel.as_int();
                    match message {
                        MidiMessage::ProgramChange { program: p } => {
                            if let Some(slot) = programs.get_mut(usize::from(ch))
                                && slot.is_none()
                            {
                                *slot = Some(p.as_int());
                            }
                        }
                        MidiMessage::NoteOn { key: k, vel } if vel.as_int() > 0 => {
                            open.entry((ch, k.as_int())).or_default().push(t);
                        }
                        MidiMessage::NoteOn { key: k, .. } | MidiMessage::NoteOff { key: k, .. } => {
                            let Some(starts) = open.get_mut(&(ch, k.as_int())) else {
                                continue;
                            };
                            if starts.is_empty() || ch == 9 || total >= MAX_NOTES {
                                starts.clear();
                                continue;
                            }
                            let start = starts.remove(0);
                            let v = voices.entry((ti, ch)).or_default();
                            if v.notes.is_empty() {
                                v.program = programs.get(usize::from(ch)).copied().flatten().unwrap_or(0);
                                v.name = track_name.clone().unwrap_or_default();
                            }
                            let (s, e) = (to_ticks(start), to_ticks(t));
                            v.notes.push(RawNote {
                                start: s,
                                end: e.max(s + u64::from(GRID)),
                                key: k.as_int(),
                            });
                            total += 1;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
    // A first track without notes names the piece (a "conductor" track); otherwise its name is
    // the first instrument's.
    if !voices.keys().any(|(t, _)| *t == 0) && !single {
        title = first_track_name;
    }
    // Per track, its channels in channel order: the lowest one is the track's own instrument.
    let mut by_track: BTreeMap<usize, Vec<Voice>> = BTreeMap::new();
    for ((ti, _), v) in voices {
        if !v.notes.is_empty() {
            by_track.entry(ti).or_default().push(v);
        }
    }
    let mut plans: Vec<PartPlan> = Vec::new();
    let mut extra: Vec<PartPlan> = Vec::new();
    for (ti, vs) in by_track {
        for (k, v) in vs.into_iter().enumerate() {
            if plans.len() >= limits::MAX_PARTS {
                break;
            }
            let main = k == 0;
            // Consecutive tracks with the same name and sound are the staves of one part
            // (MuseScore writes one track per staff).
            if main
                && let Some(last) = plans.last_mut()
                && last.track + 1 == ti
                && !v.name.is_empty()
                && last.name == v.name
                && last.program == v.program
                && last.staves.len() < limits::MAX_STAVES
            {
                last.staves.push(v.notes);
                last.track = ti;
                continue;
            }
            // Further channels of a multi-track file's track (e.g. MuseScore's chord-symbol
            // playback) become their own parts, hidden but still played.
            let hidden = !main && !single;
            let name = if hidden || v.name.is_empty() {
                let inst = template(v.program).name;
                if v.name.is_empty() {
                    inst.to_string()
                } else {
                    format!("{} – {inst}", v.name)
                }
            } else {
                v.name.clone()
            };
            let plan = PartPlan {
                name,
                program: v.program,
                track: ti,
                staves: vec![v.notes],
                hidden,
            };
            if hidden {
                extra.push(plan)
            } else {
                plans.push(plan)
            }
        }
    }
    plans.extend(extra);
    plans.truncate(limits::MAX_PARTS);
    if plans.is_empty() {
        return Err("the MIDI file has no notes (drums on channel 10 are not imported)".into());
    }

    // Measures from the time signatures.
    let time_of = |(n, d): (u8, u8)| TimeSig::new(i64::from(n.max(1)), 1i64 << d.min(6)).unwrap_or_default();
    let first_time = times.values().next().copied().map(time_of).unwrap_or_default();
    let end = plans
        .iter()
        .flat_map(|p| p.staves.iter().flatten().map(|n| n.end))
        .max()
        .unwrap_or(0);
    let mut starts: Vec<u64> = Vec::new();
    let mut changes: Vec<(usize, TimeSig)> = Vec::new();
    let mut cur = first_time;
    let mut at = 0u64;
    while at < end.max(1) && starts.len() < limits::MAX_MEASURES {
        if let Some((_, ts)) = times.range(..=at).next_back()
            && !starts.is_empty()
        {
            let ts = time_of(*ts);
            if ts != cur {
                cur = ts;
                changes.push((starts.len(), ts));
            }
        }
        starts.push(at);
        at += u64::from(cur.measure_ticks().max(GRID));
    }

    let templates: Vec<&Instrument> = plans.iter().map(|p| template(p.program)).collect();
    let mut score = Score::new(
        &title.unwrap_or_default(),
        &templates,
        first_time,
        KeySig::new(i32::from(key.unwrap_or(0))),
        starts.len().max(1),
    );
    for (m, ts) in changes {
        if let Some(info) = score.measures.get_mut(m) {
            info.time = Some(ts);
        }
    }
    if let (Some(us), Some(info)) = (tempo, score.measures.first_mut()) {
        let bpm = (60_000_000.0 / f64::from(us)).round().clamp(10.0, 400.0) as u16;
        info.tempo = Some(Tempo { bpm });
    }
    score.normalize();
    let key_sig = score.key_at(0);

    let median_clef = |notes: &[RawNote]| {
        let mut keys: Vec<u8> = notes.iter().map(|n| n.key).collect();
        keys.sort_unstable();
        // Middle of the range: treble; an octave lower (tenor voices, guitar): treble 8vb; below that: bass.
        let lowest = keys.first().copied().unwrap_or(60);
        match keys.get(keys.len().saturating_sub(1) / 2).copied().unwrap_or(60) {
            62.. => Clef::Treble,
            55..=61 if lowest >= 47 => Clef::Treble8vb,
            _ => Clef::Bass,
        }
    };
    for (pi, plan) in plans.iter().enumerate() {
        // The staves to write: one per merged track, or a single track split at middle C when it
        // is a two-staff instrument using both sides.
        let all: Vec<RawNote> = plan.staves.iter().flatten().copied().collect();
        let split = plan.staves.len() == 1
            && templates.get(pi).is_some_and(|t| t.clefs.len() > 1)
            && all.iter().any(|n| n.key < 60)
            && all.iter().any(|n| n.key >= 60);
        let staves: Vec<Vec<RawNote>> = if split {
            vec![
                all.iter().copied().filter(|n| n.key >= 60).collect(),
                all.iter().copied().filter(|n| n.key < 60).collect(),
            ]
        } else {
            plan.staves.clone()
        };
        if let Some(p) = score.parts.get_mut(pi) {
            p.name = clip(&plan.name);
            p.abbreviation = clip(&plan.name.chars().take(4).collect::<String>());
            p.program = plan.program;
            p.clefs = if split {
                vec![Clef::Treble, Clef::Bass]
            } else {
                staves.iter().map(|n| median_clef(n)).collect()
            };
            let n = p.clefs.len();
            for m in &mut p.measures {
                m.staves.truncate(n);
            }
            if plan.hidden {
                p.set_visible(false);
            }
        }
        score.normalize();
        for (staff, notes) in staves.iter().enumerate() {
            let mut notes = notes.clone();
            notes.sort_by_key(|n| (n.start, n.key));
            write_staff(&mut score, pi, staff, &notes, &starts, key_sig)?;
        }
    }
    score.normalize();
    Ok(score)
}

/// Writes one staff's notes as chords and rests in voice 1.
fn write_staff(
    score: &mut Score,
    part: usize,
    staff: usize,
    notes: &[RawNote],
    starts: &[u64],
    key: KeySig,
) -> Result<(), String> {
    let mut i = 0usize;
    while i < notes.len() {
        let Some(first) = notes.get(i) else { break };
        let onset = first.start;
        let mut j = i;
        while notes.get(j).is_some_and(|n| n.start == onset) {
            j += 1;
        }
        let group = notes.get(i..j).unwrap_or(&[]);
        let next = notes.get(j).map(|n| n.start);
        let mut end = group
            .iter()
            .map(|n| n.end)
            .min()
            .unwrap_or(onset + u64::from(GRID));
        if let Some(nx) = next {
            end = end.min(nx);
        }
        i = j;
        let len = end.saturating_sub(onset);
        if len == 0 {
            continue;
        }
        // Where the onset lies: measure and tick inside it.
        let m = starts.partition_point(|s| *s <= onset).saturating_sub(1);
        let tick = onset - starts.get(m).copied().unwrap_or(0);
        let mut pos = Pos {
            part,
            staff,
            voice: 0,
            measure: m,
            tick: u32::try_from(tick).unwrap_or(0),
        };
        let mut keys: Vec<u8> = group.iter().map(|n| n.key).collect();
        keys.sort_unstable();
        keys.dedup();
        keys.truncate(limits::MAX_CHORD_NOTES);
        let pieces = Duration::decompose(u32::try_from(len).unwrap_or(u32::MAX).min(TPQ * 16));
        let count = pieces.len();
        for (k, d) in pieces.into_iter().enumerate() {
            let notes: Vec<Note> = keys
                .iter()
                .map(|key_no| {
                    let mut n = Note::new(Pitch::from_midi(i32::from(*key_no), key));
                    n.tie = k + 1 < count;
                    n
                })
                .collect();
            match score.overwrite(pos, EventKind::Chord(notes), d) {
                Ok(p) => pos = p,
                // Past the last measure or a limit: stop this staff quietly.
                Err(_) => return Ok(()),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use scoreart_model::{EventKind, NoteValue};

    fn smf(tracks: Vec<Vec<(u32, TrackEventKind<'static>)>>) -> Vec<u8> {
        use midly::num::{u15, u28};
        let tracks: Vec<Vec<midly::TrackEvent<'static>>> = tracks
            .into_iter()
            .map(|t| {
                let mut v: Vec<midly::TrackEvent<'static>> = t
                    .into_iter()
                    .map(|(d, kind)| midly::TrackEvent {
                        delta: u28::new(d),
                        kind,
                    })
                    .collect();
                v.push(midly::TrackEvent {
                    delta: u28::new(0),
                    kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
                });
                v
            })
            .collect();
        let smf = Smf {
            header: midly::Header::new(midly::Format::Parallel, Timing::Metrical(u15::new(480))),
            tracks,
        };
        let mut out = Vec::new();
        smf.write_std(&mut out).unwrap();
        out
    }

    fn on(ch: u8, key: u8) -> TrackEventKind<'static> {
        TrackEventKind::Midi {
            channel: ch.into(),
            message: MidiMessage::NoteOn {
                key: key.into(),
                vel: 90.into(),
            },
        }
    }

    fn off(ch: u8, key: u8) -> TrackEventKind<'static> {
        TrackEventKind::Midi {
            channel: ch.into(),
            message: MidiMessage::NoteOff {
                key: key.into(),
                vel: 0.into(),
            },
        }
    }

    #[test]
    fn reads_a_two_track_file() {
        let bytes = smf(vec![
            vec![
                (0, TrackEventKind::Meta(MetaMessage::TrackName(b"Song"))),
                (0, TrackEventKind::Meta(MetaMessage::Tempo(500_000.into()))),
                (0, TrackEventKind::Meta(MetaMessage::TimeSignature(3, 2, 24, 8))),
                (0, TrackEventKind::Meta(MetaMessage::KeySignature(-1, false))),
            ],
            vec![
                (0, TrackEventKind::Meta(MetaMessage::TrackName(b"Flute"))),
                (
                    0,
                    TrackEventKind::Midi {
                        channel: 0.into(),
                        message: MidiMessage::ProgramChange { program: 73.into() },
                    },
                ),
                // A slightly early quarter, then a dotted half crossing the barline.
                (3, on(0, 77)),
                (475, off(0, 77)),
                (2, on(0, 70)),
                (1440, off(0, 70)),
            ],
            vec![
                // A chord, and drums that are left out.
                (0, on(1, 48)),
                (0, on(1, 55)),
                (0, on(9, 36)),
                (960, off(1, 48)),
                (0, off(1, 55)),
                (0, off(9, 36)),
            ],
        ]);
        let s = from_smf(&bytes).unwrap();
        assert_eq!(s.meta.title, "Song");
        assert_eq!(s.parts.len(), 2);
        assert_eq!((s.parts[0].name.as_str(), s.parts[0].program), ("Flute", 73));
        assert_eq!(s.time_at(0).beats, 3);
        assert_eq!(s.key_at(0).fifths, -1);
        assert_eq!(s.measures[0].tempo.map(|t| t.bpm), Some(120));
        let v = s.voice(0, 0, 0, 0).unwrap();
        assert_eq!(v.events[0].notes()[0].pitch.name(), "F5");
        assert_eq!(v.events[0].duration.value, NoteValue::Quarter);
        assert_eq!(v.events[1].notes()[0].pitch.name(), "Bb4", "spelled in F major");
        assert!(v.events[1].notes()[0].tie, "tied over the barline");
        // The low chord gets a bass clef and stays together.
        assert_eq!(s.parts[1].clefs[0], Clef::Bass);
        let c = s.voice(1, 0, 0, 0).unwrap();
        assert!(matches!(&c.events[0].kind, EventKind::Chord(n) if n.len() == 2));
        // Round trip to MIDI and back keeps the notes.
        let again = from_smf(&crate::to_smf(&s)).unwrap();
        assert_eq!(again.voice(0, 0, 0, 0).unwrap().events.len(), v.events.len());
    }

    #[test]
    fn same_named_tracks_are_staves_and_extra_channels_hidden() {
        let prog = |ch: u8, p: u8| TrackEventKind::Midi {
            channel: ch.into(),
            message: MidiMessage::ProgramChange { program: p.into() },
        };
        let bytes = smf(vec![
            // Staff 1 of "Choir", plus chord playback on another channel.
            vec![
                (0, TrackEventKind::Meta(MetaMessage::TrackName(b"Choir"))),
                (0, prog(0, 52)),
                (0, prog(3, 0)),
                (0, on(3, 48)),
                (480, on(0, 72)),
                (0, off(3, 48)),
                (480, off(0, 72)),
            ],
            // Staff 2 of "Choir": same name, channel 0's program carries over.
            vec![
                (0, TrackEventKind::Meta(MetaMessage::TrackName(b"Choir"))),
                (480, on(0, 50)),
                (480, off(0, 50)),
            ],
        ]);
        let s = from_smf(&bytes).unwrap();
        assert_eq!(
            s.meta.title, "",
            "the first track has notes: its name is the part's"
        );
        assert_eq!(s.parts.len(), 2);
        assert_eq!(
            (
                s.parts[0].name.as_str(),
                s.parts[0].program,
                s.parts[0].staff_count()
            ),
            ("Choir", 52, 2)
        );
        assert!(s.parts[0].visible());
        assert_eq!(s.parts[1].name, "Choir – Piano");
        assert!(!s.parts[1].visible(), "chord playback stays hidden");
        assert_eq!(s.parts[0].clefs, vec![Clef::Treble, Clef::Bass]);
    }

    #[test]
    fn hostile_midi_does_not_panic() {
        for junk in [
            &b""[..],
            b"MThd",
            b"MThd\0\0\0\x06\0\x01\0\x01\x01\xe0MTrk\xff\xff\xff\xff",
            b"RIFF",
        ] {
            assert!(from_smf(junk).is_err());
        }
        let only_drums = smf(vec![vec![(0, on(9, 36)), (480, off(9, 36))]]);
        assert!(from_smf(&only_drums).is_err());
        // A note far in the future stays within the measure limit.
        let far = smf(vec![vec![
            (0, on(0, 60)),
            (u32::from(u16::MAX) * 100, off(0, 60)),
        ]]);
        let _ = from_smf(&far);
    }
}
