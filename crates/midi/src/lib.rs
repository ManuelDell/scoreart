//! Turns a [`Score`] into a flat, tie-merged note sequence (for playback) and into a Standard
//! MIDI File (format 1, one track per part).
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::collections::HashMap;

use scoreart_model::{Articulation, Barline, Score, TPQ};

/// MIDI ticks per quarter in exported files.
pub const MIDI_TPQ: u16 = 480;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayNote {
    /// Score tick (TPQ per quarter) where the note starts, after unrolling repeats.
    pub start: u64,
    pub ticks: u64,
    pub key: u8,
    pub velocity: u8,
    pub part: usize,
    pub program: u8,
}

/// Tempo change at a score tick, in quarter notes per minute.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempoChange {
    pub tick: u64,
    pub bpm: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Sequence {
    pub notes: Vec<PlayNote>,
    pub tempos: Vec<TempoChange>,
    pub end: u64,
    /// For every played measure (in order): (measure index, start tick).
    pub measure_starts: Vec<(usize, u64)>,
}

impl Sequence {
    /// Seconds from the start for a score tick.
    pub fn seconds_at(&self, tick: u64) -> f64 {
        let mut secs = 0.0;
        let mut last_tick = 0u64;
        let mut bpm = self.tempos.first().map(|t| t.bpm).unwrap_or(100.0);
        for t in &self.tempos {
            if t.tick >= tick {
                break;
            }
            secs += (t.tick.saturating_sub(last_tick)) as f64 / f64::from(TPQ) * 60.0 / bpm;
            last_tick = t.tick;
            bpm = t.bpm;
        }
        secs + tick.saturating_sub(last_tick) as f64 / f64::from(TPQ) * 60.0 / bpm
    }

    /// The score tick at `secs` seconds (inverse of [`Sequence::seconds_at`]).
    pub fn tick_at(&self, secs: f64) -> u64 {
        let mut acc = 0.0;
        let mut last_tick = 0u64;
        let mut bpm = self.tempos.first().map(|t| t.bpm).unwrap_or(100.0);
        for t in &self.tempos {
            let seg = (t.tick.saturating_sub(last_tick)) as f64 / f64::from(TPQ) * 60.0 / bpm;
            if acc + seg > secs {
                break;
            }
            acc += seg;
            last_tick = t.tick;
            bpm = t.bpm;
        }
        last_tick + ((secs - acc).max(0.0) * bpm / 60.0 * f64::from(TPQ)) as u64
    }

    /// The measure playing at `tick`.
    pub fn measure_at(&self, tick: u64) -> Option<usize> {
        self.measure_starts
            .iter()
            .rev()
            .find(|(_, s)| *s <= tick)
            .map(|(m, _)| *m)
    }
}

/// The order measures are played in: repeats (`|:` … `:|`) are taken once.
pub fn play_order(score: &Score) -> Vec<usize> {
    let mut order = Vec::new();
    let mut start = 0usize;
    let mut repeated: Vec<usize> = Vec::new();
    let mut m = 0usize;
    while m < score.measures.len() && order.len() < scoreart_model::limits::MAX_MEASURES * 2 {
        let info = &score.measures[m];
        if info.repeat_start {
            start = m;
        }
        order.push(m);
        if info.barline == Barline::RepeatEnd && !repeated.contains(&m) {
            repeated.push(m);
            m = start;
            continue;
        }
        m += 1;
    }
    order
}

pub fn sequence(score: &Score) -> Sequence {
    let mut seq = Sequence::default();
    let order = play_order(score);
    let mut tick: u64 = 0;
    let mut last_bpm: Option<u16> = None;
    // Open (tied) notes: (part, staff, voice, key) → index into seq.notes.
    let mut open: HashMap<(usize, usize, usize, u8), usize> = HashMap::new();
    let mut velocity: Vec<u8> = vec![80; score.parts.len()];
    for &m in &order {
        seq.measure_starts.push((m, tick));
        let bpm = score.tempo_at(m);
        if last_bpm != Some(bpm) {
            seq.tempos.push(TempoChange {
                tick,
                bpm: f64::from(bpm.max(1)),
            });
            last_bpm = Some(bpm);
        }
        for (pi, part) in score.parts.iter().enumerate() {
            let Some(pm) = part.measures.get(m) else { continue };
            for (si, sm) in pm.staves.iter().enumerate() {
                for (vi, v) in sm.voices.iter().enumerate() {
                    let mut t = tick;
                    for e in &v.events {
                        let d = u64::from(e.ticks());
                        if let Some(dy) = e.dynamic
                            && let Some(vel) = velocity.get_mut(pi)
                        {
                            *vel = dy.velocity();
                        }
                        let vel = velocity.get(pi).copied().unwrap_or(80);
                        let accent = e
                            .articulations
                            .iter()
                            .any(|a| matches!(a, Articulation::Accent | Articulation::Marcato));
                        let vel = if accent {
                            vel.saturating_add(20).min(127)
                        } else {
                            vel
                        };
                        let short = e
                            .articulations
                            .iter()
                            .any(|a| matches!(a, Articulation::Staccato | Articulation::Staccatissimo));
                        for n in e.notes() {
                            let key = n.pitch.midi();
                            let k = (pi, si, vi, key);
                            if let Some(&idx) = open.get(&k)
                                && let Some(prev) = seq.notes.get_mut(idx)
                                && prev.start + prev.ticks == t
                            {
                                prev.ticks += d;
                            } else {
                                let played = if short { (d / 2).max(1) } else { d };
                                seq.notes.push(PlayNote {
                                    start: t,
                                    ticks: played,
                                    key,
                                    velocity: vel,
                                    part: pi,
                                    program: part.program,
                                });
                                open.insert(k, seq.notes.len() - 1);
                            }
                            if !n.tie {
                                open.remove(&k);
                            }
                        }
                        // Rests and untied notes close ties in this voice.
                        if e.is_rest() {
                            open.retain(|o, _| !(o.0 == pi && o.1 == si && o.2 == vi));
                        }
                        t += d;
                    }
                }
            }
        }
        tick += u64::from(score.measure_ticks(m));
    }
    seq.end = tick;
    seq.notes.sort_by_key(|n| (n.start, n.part, n.key));
    seq
}

/// Writes a format-1 Standard MIDI File: a tempo track plus one track per part.
pub fn to_smf(score: &Score) -> Vec<u8> {
    use midly::num::{u4, u7, u15, u24, u28};
    use midly::{Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind};

    let seq = sequence(score);
    let scale = |t: u64| -> u64 { t * u64::from(MIDI_TPQ) / u64::from(TPQ) };
    let mut tracks: Vec<Vec<TrackEvent<'static>>> = Vec::new();

    // Track 0: tempo map and time signature.
    let mut abs: Vec<(u64, TrackEventKind<'static>)> = Vec::new();
    for t in &seq.tempos {
        let us = (60_000_000.0 / t.bpm.max(1.0)) as u32;
        abs.push((
            scale(t.tick),
            TrackEventKind::Meta(MetaMessage::Tempo(u24::new(us.min(0xFF_FFFF)))),
        ));
    }
    let ts = score.time_at(0);
    let denom_pow = (ts.beat_type.max(1) as f32).log2() as u8;
    abs.push((
        0,
        TrackEventKind::Meta(MetaMessage::TimeSignature(ts.beats, denom_pow, 24, 8)),
    ));
    tracks.push(to_track(abs, scale(seq.end)));

    for (pi, part) in score.parts.iter().enumerate() {
        let ch = u4::new(if pi >= 9 { ((pi + 1) % 16) as u8 } else { pi as u8 });
        let mut abs: Vec<(u64, TrackEventKind<'static>)> = Vec::new();
        abs.push((
            0,
            TrackEventKind::Midi {
                channel: ch,
                message: MidiMessage::ProgramChange {
                    program: u7::new(part.program.min(127)),
                },
            },
        ));
        for n in seq.notes.iter().filter(|n| n.part == pi) {
            let key = u7::new(n.key.min(127));
            abs.push((
                scale(n.start + n.ticks),
                TrackEventKind::Midi {
                    channel: ch,
                    message: MidiMessage::NoteOff { key, vel: u7::new(0) },
                },
            ));
            abs.push((
                scale(n.start),
                TrackEventKind::Midi {
                    channel: ch,
                    message: MidiMessage::NoteOn {
                        key,
                        vel: u7::new(n.velocity.clamp(1, 127)),
                    },
                },
            ));
        }
        tracks.push(to_track(abs, scale(seq.end)));
    }

    fn to_track(mut abs: Vec<(u64, TrackEventKind<'static>)>, end: u64) -> Vec<TrackEvent<'static>> {
        // Note-offs sort before note-ons at the same tick (stable sort keeps insertion order).
        abs.sort_by_key(|(t, k)| {
            (
                *t,
                !matches!(
                    k,
                    TrackEventKind::Midi {
                        message: MidiMessage::NoteOff { .. },
                        ..
                    }
                ),
            )
        });
        let mut out = Vec::with_capacity(abs.len() + 1);
        let mut last = 0u64;
        for (t, kind) in abs {
            let delta = t.saturating_sub(last).min(0x0FFF_FFFF) as u32;
            last = t.max(last);
            out.push(TrackEvent {
                delta: u28::new(delta),
                kind,
            });
        }
        out.push(TrackEvent {
            delta: u28::new(end.saturating_sub(last).min(0x0FFF_FFFF) as u32),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });
        out
    }

    let smf = Smf {
        header: Header::new(Format::Parallel, Timing::Metrical(u15::new(MIDI_TPQ))),
        tracks,
    };
    let mut buf = Vec::new();
    if smf.write_std(&mut buf).is_err() {
        return Vec::new();
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use scoreart_model::edit::Pos;
    use scoreart_model::*;

    #[test]
    fn ties_merge_and_smf_parses() {
        let flute = instrument("flute").unwrap();
        let mut s = Score::new("T", &[flute], TimeSig::default(), KeySig::default(), 2);
        let kind = EventKind::Chord(vec![Note::new(Pitch::new(Step::A, 0, 4))]);
        s.overwrite(
            Pos {
                tick: TPQ * 3,
                ..Pos::default()
            },
            kind,
            Duration::new(NoteValue::Half, 0),
        )
        .unwrap();
        let seq = sequence(&s);
        assert_eq!(seq.notes.len(), 1);
        assert_eq!(seq.notes[0].ticks, u64::from(TPQ) * 2);
        assert_eq!(seq.notes[0].key, 69);
        let bytes = to_smf(&s);
        let smf = midly::Smf::parse(&bytes).unwrap();
        assert_eq!(smf.tracks.len(), 2);
        let secs = seq.seconds_at(u64::from(TPQ) * 4);
        assert!((secs - 2.4).abs() < 1e-9, "4 quarters at 100 bpm");
        assert_eq!(seq.tick_at(secs), u64::from(TPQ) * 4);
    }

    #[test]
    fn repeats_unroll_once() {
        let flute = instrument("flute").unwrap();
        let mut s = Score::new("T", &[flute], TimeSig::default(), KeySig::default(), 3);
        s.measures[1].barline = Barline::RepeatEnd;
        assert_eq!(play_order(&s), vec![0, 1, 0, 1, 2]);
    }
}
