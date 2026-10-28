//! Reading Standard MIDI Files into a score.
//!
//! MIDI stores performance, not notation, so this is an interpretation:
//! - Note times are quantised beat by beat to the simplest grid that fits: plain values down to
//!   64ths, or triplets, quintuplets and septuplets.
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
    Tuplet, clip, instrument, limits,
};

/// Grid for time signature changes and for notes after the last measure.
const GRID: u32 = TPQ / 4;
/// Beat subdivisions tried per beat, with how much each is disfavoured (in beats of timing
/// error): plain grids first, then triplets, quintuplets and septuplets.
const GRIDS: [(u32, f64); 10] = [
    (1, 0.0),
    (2, 0.0),
    (4, 0.01),
    (3, 0.015),
    (8, 0.025),
    (6, 0.025),
    (5, 0.03),
    (16, 0.04),
    (7, 0.04),
    (12, 0.04),
];
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
    let raw = |t: u64| -> u64 { t.saturating_mul(u64::from(TPQ)) / ppq };
    let to_ticks = |t: u64| -> u64 { snap(raw(t), u64::from(GRID)) };

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
                            let (s, e) = (raw(start), raw(t));
                            v.notes.push(RawNote {
                                start: s,
                                end: e.max(s + 1),
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
    let mut lens: Vec<u64> = Vec::new();
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
        let len = u64::from(cur.measure_ticks().max(GRID));
        lens.push(len);
        at += len;
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
        info.tempo = Some(Tempo::quarter(bpm));
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
            let beats = Beats::new(notes, &starts, &lens);
            let mut notes: Vec<RawNote> = notes
                .iter()
                .map(|n| {
                    let start = beats.quantise(n.start);
                    let end = beats.quantise(n.end).max(start + beats.step(start));
                    RawNote {
                        start,
                        end,
                        key: n.key,
                    }
                })
                .collect();
            notes.sort_by_key(|n| (n.start, n.key));
            notes.dedup_by_key(|n| (n.start, n.key));
            write_staff(&mut score, pi, staff, &notes, &beats, key_sig);
        }
    }
    score.normalize();
    Ok(score)
}

/// Rounds `t` to the nearest multiple of `step`.
fn snap(t: u64, step: u64) -> u64 {
    (t + step / 2) / step * step
}

/// One beat (a quarter, or the shorter rest of a measure) and the grid chosen for it.
#[derive(Clone, Copy, Debug)]
struct Beat {
    start: u64,
    len: u64,
    /// Subdivisions of a quarter: 1, 2, 4, 8, 16 are plain; 3, 6, 12 triplets; 5 and 7
    /// quintuplets and septuplets.
    div: u32,
}

impl Beat {
    fn step(&self) -> u64 {
        u64::from(TPQ / self.div)
    }

    /// The tuplet ratio its notes are written in, if any.
    fn tuplet(&self) -> Option<Tuplet> {
        let (actual, normal) = match self.div {
            3 | 12 => (3, 2),
            6 => (6, 4),
            5 => (5, 4),
            7 => (7, 4),
            _ => return None,
        };
        Some(Tuplet { actual, normal })
    }
}

/// The beats of a staff, each with the grid that best fits the notes starting in it.
struct Beats {
    beats: BTreeMap<u64, Beat>,
    /// Measure starts, and the end of the last measure.
    starts: Vec<u64>,
    end: u64,
}

impl Beats {
    fn new(notes: &[RawNote], starts: &[u64], lens: &[u64]) -> Beats {
        let mut onsets: Vec<u64> = notes.iter().map(|n| n.start).collect();
        onsets.sort_unstable();
        let mut ends: Vec<u64> = notes.iter().map(|n| n.end).collect();
        ends.sort_unstable();
        let within = |ts: &'_ [u64], at: u64, len: u64| -> std::ops::Range<usize> {
            ts.partition_point(|t| *t < at)..ts.partition_point(|t| *t < at + len)
        };
        let mut beats = BTreeMap::new();
        for (m, (&s, &len)) in starts.iter().zip(lens).enumerate() {
            let end = s + len;
            let mut at = s;
            while at < end {
                let blen = u64::from(TPQ).min(end - at);
                let on = onsets.get(within(&onsets, at, blen)).unwrap_or(&[]);
                let off = ends.get(within(&ends, at, blen)).unwrap_or(&[]);
                let div = best_grid(on, off, at, blen);
                beats.insert(
                    at,
                    Beat {
                        start: at,
                        len: blen,
                        div,
                    },
                );
                at += blen;
                if beats.len() > 64 * limits::MAX_MEASURES {
                    break;
                }
            }
            if m + 1 == starts.len() {
                break;
            }
        }
        let end = starts.last().zip(lens.last()).map_or(0, |(s, l)| s + l);
        Beats {
            beats,
            starts: starts.to_vec(),
            end,
        }
    }

    fn beat(&self, t: u64) -> Option<&Beat> {
        self.beats
            .range(..=t)
            .next_back()
            .map(|(_, b)| b)
            .filter(|b| t < b.start + b.len)
    }

    /// The measure around `t`.
    fn measure(&self, t: u64) -> usize {
        self.starts.partition_point(|s| *s <= t).saturating_sub(1)
    }

    fn quantise(&self, t: u64) -> u64 {
        match self.beat(t) {
            Some(b) => (b.start + snap(t - b.start, b.step())).min(b.start + b.len),
            None => snap(t, u64::from(GRID)),
        }
    }

    fn step(&self, t: u64) -> u64 {
        self.beat(t).map_or(u64::from(GRID), Beat::step)
    }

    /// Splits `[a, b)` at barlines and around tuplet beats, and gives each piece's durations.
    fn pieces(&self, a: u64, b: u64) -> Vec<(u64, Vec<Duration>)> {
        let mut cuts: Vec<u64> = self.starts.iter().copied().filter(|t| *t > a && *t < b).collect();
        // Where a run of beats in one tuplet ratio begins or ends.
        let first = self.beat(a).map_or(a, |x| x.start);
        let mut prev = self.beat(first.saturating_sub(1)).and_then(Beat::tuplet);
        for x in self.beats.range(first..b).map(|(_, x)| x) {
            let cur = x.tuplet();
            if cur != prev && x.start > a {
                cuts.push(x.start);
            }
            prev = cur;
        }
        cuts.push(b);
        cuts.sort_unstable();
        cuts.dedup();
        let mut out = Vec::new();
        let mut at = a;
        for cut in cuts {
            if cut <= at {
                continue;
            }
            let len = u32::try_from(cut - at).unwrap_or(u32::MAX).min(TPQ * 16);
            let plain = durations(len);
            let exact = plain.iter().map(|d| d.ticks()).sum::<u32>() == len;
            // Inside tuplet beats only whole beats are written plainly.
            let on_beats =
                self.beat(at).is_some_and(|x| x.start == at) && self.beat(cut).is_none_or(|x| x.start == cut);
            let durs = match self.beat(at).and_then(Beat::tuplet) {
                Some(tu) if !(exact && on_beats) => {
                    let scaled = len * u32::from(tu.actual) / u32::from(tu.normal);
                    let mut ds = durations(scaled);
                    if ds.iter().map(|d| d.ticks()).sum::<u32>() == scaled {
                        for d in &mut ds {
                            d.tuplet = Some(tu);
                        }
                        ds
                    } else {
                        plain
                    }
                }
                _ => plain,
            };
            out.push((at, durs));
            at = cut;
        }
        out
    }
}

/// One duration of exactly `ticks` (double dots allowed), or the fewest that add up to it.
fn durations(ticks: u32) -> Vec<Duration> {
    Duration::from_ticks(ticks).map_or_else(|| Duration::decompose(ticks), |d| vec![d])
}

/// The grid for one beat: the one with the least timing error plus its complexity cost. Note
/// ends count half as much as onsets.
fn best_grid(onsets: &[u64], ends: &[u64], start: u64, len: u64) -> u32 {
    let full = len == u64::from(TPQ);
    let mut best = (4, f64::MAX);
    for (div, cost) in GRIDS {
        let step = u64::from(TPQ / div);
        // A partial beat (the end of a 5/16 measure) only takes plain grids that fit it.
        if !full && (!len.is_multiple_of(step) || matches!(div, 3 | 5 | 6 | 7 | 12)) {
            continue;
        }
        let err = |ts: &[u64]| -> f64 {
            ts.iter()
                .map(|t| {
                    let off = t - start;
                    off.abs_diff(snap(off, step).min(len)) as f64
                })
                .sum()
        };
        let weight = onsets.len() as f64 + ends.len() as f64 / 2.0;
        let score = (err(onsets) + err(ends) / 2.0) / f64::from(TPQ) / weight.max(1.0) + cost;
        if score < best.1 {
            best = (div, score);
        }
    }
    best.0
}

/// Writes one staff's notes as chords and rests in voice 1.
fn write_staff(score: &mut Score, part: usize, staff: usize, notes: &[RawNote], beats: &Beats, key: KeySig) {
    // The staff as a sequence of chords and rests up to the end of the last measure.
    let mut spans: Vec<(u64, u64, Vec<u8>)> = Vec::new();
    let mut at = 0u64;
    let mut i = 0usize;
    while i < notes.len() {
        let Some(first) = notes.get(i) else { break };
        let onset = first.start;
        let mut j = i;
        while notes.get(j).is_some_and(|n| n.start == onset) {
            j += 1;
        }
        let group = notes.get(i..j).unwrap_or(&[]);
        i = j;
        if onset >= beats.end {
            break;
        }
        let mut end = group
            .iter()
            .map(|n| n.end)
            .min()
            .unwrap_or(onset + u64::from(GRID));
        if let Some(nx) = notes.get(j).map(|n| n.start) {
            end = end.min(nx);
        }
        // Held notes are cut after four whole notes, as hung notes in a file often are.
        end = end.min(beats.end).min(onset + u64::from(TPQ) * 16);
        if end <= onset {
            continue;
        }
        if onset > at {
            spans.push((at, onset, Vec::new()));
        }
        let mut keys: Vec<u8> = group.iter().map(|n| n.key).collect();
        keys.sort_unstable();
        keys.dedup();
        keys.truncate(limits::MAX_CHORD_NOTES);
        spans.push((onset, end, keys));
        at = end;
    }
    if at < beats.end {
        spans.push((at, beats.end, Vec::new()));
    }
    let touched: std::collections::BTreeSet<usize> = spans
        .iter()
        .filter(|(_, _, k)| !k.is_empty())
        .flat_map(|(a, b, _)| beats.measure(*a)..=beats.measure(b.saturating_sub(1)))
        .collect();
    for (a, b, keys) in spans {
        let mut pieces = beats.pieces(a, b);
        // Rests are already there; they are rewritten in measures with notes only.
        if keys.is_empty() {
            pieces.retain(|(start, _)| touched.contains(&beats.measure(*start)));
        }
        let count: usize = pieces.iter().map(|(_, d)| d.len()).sum();
        let mut k = 0usize;
        for (start, durs) in pieces {
            let m = beats.measure(start);
            let mut pos = Pos {
                part,
                staff,
                voice: 0,
                measure: m,
                tick: u32::try_from(start - beats.starts.get(m).copied().unwrap_or(0)).unwrap_or(0),
            };
            for d in durs {
                k += 1;
                let kind = if keys.is_empty() {
                    EventKind::Rest
                } else {
                    EventKind::Chord(
                        keys.iter()
                            .map(|key_no| {
                                let mut n = Note::new(Pitch::from_midi(i32::from(*key_no), key));
                                n.tie = k < count;
                                n
                            })
                            .collect(),
                    )
                };
                match score.overwrite(pos, kind, d) {
                    Ok(p) => pos = p,
                    // Past the last measure or a limit: stop this staff quietly.
                    Err(_) => return,
                }
            }
        }
    }
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

    #[test]
    fn finds_tuplets_and_fine_values() {
        // 480 ticks per quarter: triplet eighths, quintuplet 16ths, 32nds, a dotted eighth and a
        // 16th; then quarter triplets over two beats and a half note.
        let mut notes: Vec<u32> = Vec::new();
        notes.extend([160; 3]);
        notes.extend([96; 5]);
        notes.extend([60; 8]);
        notes.extend([360, 120]);
        notes.extend([320; 3]);
        notes.push(960);
        let mut track = Vec::new();
        for (i, len) in notes.iter().enumerate() {
            let key = 60 + (i % 12) as u8;
            track.push((0, on(0, key)));
            // Played a little short, as MIDI files often are.
            track.push((len - 1, off(0, key)));
            track.push((1, TrackEventKind::Meta(MetaMessage::Text(b""))));
        }
        let s = from_smf(&smf(vec![track])).unwrap();
        let v = |m: usize| s.voice(0, 0, m, 0).unwrap().events.clone();
        let m1 = v(0);
        let shape: Vec<(NoteValue, u8, Option<u8>)> = m1
            .iter()
            .map(|e| {
                (
                    e.duration.value,
                    e.duration.dots,
                    e.duration.tuplet.map(|t| t.actual),
                )
            })
            .collect();
        let mut want = vec![(NoteValue::Eighth, 0, Some(3)); 3];
        want.extend([(NoteValue::N16th, 0, Some(5)); 5]);
        want.extend([(NoteValue::N32nd, 0, None); 8]);
        want.extend([(NoteValue::Eighth, 1, None), (NoteValue::N16th, 0, None)]);
        assert_eq!(shape, want);
        assert!(
            m1.iter().all(|e| matches!(e.kind, EventKind::Chord(_))),
            "no gaps from short notes"
        );
        let m2 = v(1);
        assert_eq!(m2.len(), 4);
        assert!(
            m2[..3]
                .iter()
                .all(|e| e.duration.value == NoteValue::Quarter && e.duration.tuplet.is_some())
        );
        assert_eq!(m2[3].duration.value, NoteValue::Half);
    }
}
