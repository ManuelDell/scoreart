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

mod import;
mod order;
pub mod perform;
pub mod timing;
pub use import::from_smf;
pub use order::play_order;
pub use perform::{Family, Tech, Technique};

use std::collections::HashMap;

use perform::{TechWord, profile, tech_word};
use scoreart_model::{Anchor, Arpeggio, Pitch, Score, Spanner, SpannerKind, Step, TPQ, Tremolo};

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
    /// What the notation asks of the note (staccato, pizzicato, under a slur …).
    pub tech: Tech,
    /// A measured tremolo: the note is repeated every `repeat` ticks (0 = held). A sound with
    /// real tremolo samples may hold it instead (see [`Tech::technique`]).
    pub repeat: u64,
}

impl PlayNote {
    /// The note as single strokes: a tremolo's repeated notes, or the note itself.
    pub fn strokes(&self) -> Vec<PlayNote> {
        if self.repeat == 0 || self.repeat >= self.ticks {
            return vec![*self];
        }
        let n = (self.ticks / self.repeat).min(4096);
        (0..n)
            .map(|i| PlayNote {
                start: self.start + i * self.repeat,
                ticks: (self.repeat * 9 / 10).max(1),
                repeat: 0,
                ..*self
            })
            .collect()
    }
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
    /// Hairpins: loudness changing over time, also within held notes.
    pub swells: Vec<Swell>,
    /// Seconds at each tempo change (filled by [`Sequence::prepare`]).
    tempo_secs: Vec<f64>,
}

impl Sequence {
    /// Precomputes the seconds at each tempo change (after `tempos` is set).
    pub fn prepare(&mut self) {
        let mut secs = 0.0;
        let mut last_tick = 0u64;
        let mut bpm = self.tempos.first().map(|t| t.bpm).unwrap_or(100.0);
        self.tempo_secs = self
            .tempos
            .iter()
            .map(|t| {
                secs += t.tick.saturating_sub(last_tick) as f64 / f64::from(TPQ) * 60.0 / bpm;
                last_tick = t.tick;
                bpm = t.bpm;
                secs
            })
            .collect();
    }

    /// Seconds from the start for a score tick.
    pub fn seconds_at(&self, tick: u64) -> f64 {
        if self.tempo_secs.len() == self.tempos.len() && !self.tempos.is_empty() {
            // The last change at or before `tick`.
            let i = self.tempos.partition_point(|t| t.tick <= tick);
            if i == 0 {
                let bpm = self.tempos.first().map_or(100.0, |t| t.bpm);
                return tick as f64 / f64::from(TPQ) * 60.0 / bpm;
            }
            if let (Some(t), Some(s)) = (self.tempos.get(i - 1), self.tempo_secs.get(i - 1)) {
                return s + (tick - t.tick) as f64 / f64::from(TPQ) * 60.0 / t.bpm;
            }
        }
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

/// A swell or fade (hairpin) of one part: the loudness moves from velocity level `v0` to `v1`
/// over ticks `from..to`, also on notes that are already sounding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swell {
    pub part: usize,
    pub from: u64,
    pub to: u64,
    pub v0: u8,
    pub v1: u8,
}

/// The velocity level a hairpin reaches when no dynamic marks its end.
const HAIRPIN_STEP: i32 = 24;

fn spans_of(score: &Score, part: usize, staff: usize) -> Vec<&Spanner> {
    score
        .spanners
        .iter()
        .filter(|sp| sp.part == part && sp.staff == staff)
        .collect()
}

/// The top note of the event that starts at `at` in a staff (any voice), as a MIDI key.
fn key_at(score: &Score, part: usize, staff: usize, at: Anchor) -> Option<u8> {
    let sm = score
        .parts
        .get(part)?
        .measures
        .get(at.measure)?
        .staves
        .get(staff)?;
    sm.voices.iter().find_map(|v| {
        v.events
            .iter()
            .zip(v.onsets())
            .find(|(e, on)| *on == at.tick && !e.is_rest())
            .and_then(|(e, _)| e.notes().last().map(|n| n.pitch.midi()))
    })
}

/// The dynamic written at the end of a hairpin, if any (any voice of the staff).
fn dynamic_at(score: &Score, part: usize, staff: usize, at: Anchor) -> Option<scoreart_model::Dynamic> {
    let sm = score
        .parts
        .get(part)?
        .measures
        .get(at.measure)?
        .staves
        .get(staff)?;
    sm.voices.iter().find_map(|v| {
        v.events
            .iter()
            .zip(v.onsets())
            .find(|(e, on)| *on == at.tick && e.dynamic.is_some())
            .and_then(|(e, _)| e.dynamic)
    })
}

pub fn sequence(score: &Score) -> Sequence {
    let mut seq = Sequence::default();
    let order = play_order(score);
    // Where each played measure starts.
    let mut played: Vec<(usize, u64)> = Vec::with_capacity(order.len());
    let mut tick: u64 = 0;
    for &m in &order {
        played.push((m, tick));
        tick += u64::from(score.measure_ticks(m));
    }
    let end = tick;
    seq.measure_starts = played.clone();
    let offsets: Vec<u64> = (0..score.measures.len())
        .scan(0u64, |acc, m| {
            let o = *acc;
            *acc += u64::from(score.measure_ticks(m));
            Some(o)
        })
        .collect();
    let local = |m: usize, t: u32| -> u64 { offsets.get(m).copied().unwrap_or(0) + u64::from(t) };
    let covers = |sp: &Spanner, here: u64| {
        local(sp.start.measure, sp.start.tick) <= here && here <= local(sp.end.measure, sp.end.tick)
    };
    // Open (tied) notes: (part, staff, voice, key) → index into seq.notes.
    let mut open: HashMap<(usize, usize, usize, u8), usize> = HashMap::new();
    let parts = score.parts.len();
    let mut level: Vec<u8> = vec![80; parts];
    let mut technique: Vec<Technique> = vec![Technique::Normal; parts];
    // After a hairpin without a closing dynamic: (part's tick, the level reached).
    let mut reached: Vec<Option<(u64, u8)>> = vec![None; parts];
    let mut stretches: Vec<timing::Stretch> = Vec::new();
    let mut pauses: Vec<timing::Pause> = Vec::new();
    let mut swing_on = false;
    for (k, &(m, mtick)) in played.iter().enumerate() {
        // The end of a section: silence before the next one (not after the last measure).
        // The notes that reach the end stop before it, so the silence is a real one.
        let mut section_end: Option<(u64, u64)> = None;
        if let Some(sec) = score.measures.get(m).and_then(|i| i.section)
            && sec.pause > 0.0
            && k + 1 < played.len()
        {
            let end = mtick + u64::from(score.measure_ticks(m));
            let g = (u64::from(TPQ) / 8).max(1);
            pauses.push(timing::Pause {
                from: end.saturating_sub(g),
                to: end,
                secs: f64::from(sec.pause),
            });
            section_end = Some((end, g));
        }
        // Swing words anywhere in the measure (system text, tempo text or staff text).
        let info = score.measures.get(m);
        let mut texts: Vec<&str> = info
            .map(|i| {
                [i.text.as_deref(), i.tempo_text.as_deref()]
                    .into_iter()
                    .flatten()
                    .collect()
            })
            .unwrap_or_default();
        for p in &score.parts {
            if let Some(pm) = p.measures.get(m) {
                for sm in &pm.staves {
                    for v in &sm.voices {
                        texts.extend(v.events.iter().filter_map(|e| e.text.as_deref()));
                    }
                }
            }
        }
        if let Some(s) = texts.iter().find_map(|t| timing::swing_word(t)) {
            swing_on = s;
        }
        let swing_here = swing_on && matches!(score.time_at(m).beat_type, 2 | 4);
        let key = score.key_at(m);
        let bpm = score.tempo_at(m).max(1.0);
        for (pi, part) in score.parts.iter().enumerate() {
            let Some(pm) = part.measures.get(m) else { continue };
            let program = part.program_at(m);
            let prof = profile(Family::of_program(program));
            for (si, sm0) in pm.staves.iter().enumerate() {
                // A one-measure repeat sign plays the last written measure again.
                let mut src = m;
                let mut sm = sm0;
                while sm.measure_repeat && src > 0 {
                    src -= 1;
                    match part.measures.get(src).and_then(|x| x.staves.get(si)) {
                        Some(x) => sm = x,
                        None => break,
                    }
                }
                let spans = spans_of(score, pi, si);
                for (vi, v) in sm.voices.iter().enumerate() {
                    let mut t = mtick;
                    for (e, on) in v.events.iter().zip(v.onsets()) {
                        let d = u64::from(e.ticks());
                        let here = local(src, on);
                        // Staff text: playing techniques.
                        if let Some(w) = e.text.as_deref().and_then(tech_word)
                            && let Some(tq) = technique.get_mut(pi)
                        {
                            *tq = match w {
                                TechWord::Set(x) => x,
                                TechWord::End(x) if *tq == x => Technique::Normal,
                                TechWord::End(_) => *tq,
                                TechWord::Normal => Technique::Normal,
                            };
                        }
                        // The level a hairpin reached holds from its end on.
                        if let Some(Some((at, v1))) = reached.get(pi).copied()
                            && t >= at
                        {
                            if let Some(l) = level.get_mut(pi) {
                                *l = v1;
                            }
                            if let Some(r) = reached.get_mut(pi) {
                                *r = None;
                            }
                        }
                        let mut accent_vel: Option<u8> = None;
                        if let Some(dy) = e.dynamic {
                            if dy.is_level() {
                                if let Some(l) = level.get_mut(pi) {
                                    *l = dy.velocity();
                                }
                                if let Some(r) = reached.get_mut(pi) {
                                    *r = None;
                                }
                            } else if dy.id() == "fp" || dy.id() == "sfp" || dy.id() == "sfpp" {
                                accent_vel = Some(dy.velocity());
                                if let Some(l) = level.get_mut(pi) {
                                    *l = if dy.id() == "sfpp" { 33 } else { 49 };
                                }
                            } else {
                                accent_vel = Some(dy.velocity());
                            }
                        }
                        let lv = level.get(pi).copied().unwrap_or(80);
                        // Hairpins starting here: a swell over their length (in one voice).
                        for hp in spans.iter().filter(|sp| {
                            sp.voice == vi
                                && matches!(
                                    sp.kind,
                                    SpannerKind::Crescendo
                                        | SpannerKind::Diminuendo
                                        | SpannerKind::CrescendoLine
                                        | SpannerKind::DiminuendoLine
                                )
                                && local(sp.start.measure, sp.start.tick) == here
                        }) {
                            let b = local(hp.end.measure, hp.end.tick);
                            if b <= here {
                                continue;
                            }
                            let up = matches!(hp.kind, SpannerKind::Crescendo | SpannerKind::CrescendoLine);
                            let closing = dynamic_at(score, pi, si, hp.end).filter(|d| d.is_level());
                            let v1 = closing.map_or_else(
                                || {
                                    (i32::from(lv) + if up { HAIRPIN_STEP } else { -HAIRPIN_STEP })
                                        .clamp(20, 127) as u8
                                },
                                |d| d.velocity(),
                            );
                            let to = t + (b - here);
                            seq.swells.push(Swell {
                                part: pi,
                                from: t,
                                to,
                                v0: lv,
                                v1,
                            });
                            if closing.is_none()
                                && let Some(r) = reached.get_mut(pi)
                            {
                                *r = Some((to, v1));
                            }
                        }
                        let ids: Vec<&str> = e.articulations.iter().map(|a| a.id()).collect();
                        let mut vel = i32::from(lv);
                        if let Some(a) = accent_vel {
                            vel = vel.max(i32::from(a));
                        }
                        for id in &ids {
                            vel += match *id {
                                "accent" => prof.accent,
                                "marcato" => prof.marcato,
                                "stress" => 10,
                                "soft-accent" => 8,
                                "unstress" => -12,
                                _ => 0,
                            };
                        }
                        let vel = vel.clamp(1, 127) as u8;
                        // Time: fermatas hold, breaths and caesuras pause after the note.
                        let mut cut = 0u64;
                        for id in &ids {
                            if let Some(f) = timing::fermata_stretch(id) {
                                stretches.push(timing::Stretch {
                                    from: t,
                                    to: t + d,
                                    factor: f,
                                });
                            }
                            if let Some(secs) = timing::pause_secs(id) {
                                let g = timing::pause_ticks(d);
                                cut = cut.max(g);
                                pauses.push(timing::Pause {
                                    from: t + d - g,
                                    to: t + d,
                                    secs,
                                });
                            }
                        }
                        if let Some((end, g)) = section_end
                            && t + d + g > end
                        {
                            cut = cut.max(g);
                        }
                        // How long the note sounds.
                        let short = ids.iter().any(|i| i.starts_with("staccat"));
                        let factor = if ids.iter().any(|i| i.starts_with("staccatissimo")) {
                            prof.staccatissimo
                        } else if ids.contains(&"staccato") {
                            prof.staccato
                        } else if ids.contains(&"portato") {
                            prof.portato
                        } else if ids.contains(&"tenuto") {
                            1.0
                        } else {
                            prof.plain
                        };
                        let slurred = spans.iter().any(|sp| {
                            sp.kind == SpannerKind::Slur
                                && sp.voice == vi
                                && local(sp.start.measure, sp.start.tick) <= here
                                && here < local(sp.end.measure, sp.end.tick)
                        });
                        let length = |d: u64| -> u64 {
                            if slurred && !short {
                                d + prof.overlap
                            } else {
                                ((d as f64 * factor) as u64).max(1)
                            }
                        };
                        // Swing: offbeat eighths later, onbeat ones longer.
                        let (sw_start, sw_len) = if swing_here {
                            timing::swing(u64::from(on), d)
                        } else {
                            (0, 0)
                        };
                        let mut tech = Tech {
                            technique: technique.get(pi).copied().unwrap_or_default(),
                            short,
                            legato: slurred && !short,
                            accent: ids.iter().any(|i| matches!(*i, "accent" | "marcato")),
                            ornament: false,
                            harmony: false,
                        };
                        if ids.iter().any(|i| i.ends_with("pizzicato")) {
                            tech.technique = Technique::Pizzicato;
                        }
                        let mut repeat = 0u64;
                        match e.tremolo {
                            Tremolo::Strokes(n) if n >= 3 => {
                                tech.technique = Technique::Tremolo;
                                repeat = perform::tremolo_unit(n);
                            }
                            Tremolo::Strokes(n) if n > 0 => repeat = perform::tremolo_unit(n),
                            Tremolo::Buzz => {
                                tech.technique = Technique::Tremolo;
                                repeat = perform::tremolo_unit(4);
                            }
                            _ => {}
                        }
                        // Octave lines: written lower or higher than they sound.
                        let shift: i32 = spans
                            .iter()
                            .filter_map(|sp| match sp.kind {
                                SpannerKind::Ottava(o) if covers(sp, here) => Some(12 * i32::from(o)),
                                _ => None,
                            })
                            .sum();
                        let sound = |k: u8| (i32::from(k) + shift).clamp(0, 127) as u8;
                        // Grace notes take a little time from the start of the note.
                        let mut start = t + sw_start;
                        let graces: Vec<_> = e
                            .grace
                            .iter()
                            .filter(|g| !g.pitches.is_empty() && !g.after)
                            .collect();
                        // Graces after the note (a trill's turn) take the end of it.
                        let after: Vec<_> = e
                            .grace
                            .iter()
                            .filter(|g| !g.pitches.is_empty() && g.after)
                            .collect();
                        if !after.is_empty() && !e.is_rest() {
                            let each = (u64::from(TPQ) / 8)
                                .min(d / (2 * after.len() as u64).max(1))
                                .max(1);
                            let mut at = (t + sw_start + d).saturating_sub(each * after.len() as u64);
                            for g in &after {
                                for p in &g.pitches {
                                    seq.notes.push(PlayNote {
                                        start: at,
                                        ticks: each,
                                        key: sound(p.midi()),
                                        velocity: vel,
                                        part: pi,
                                        program,
                                        tech: Tech {
                                            ornament: true,
                                            ..tech
                                        },
                                        repeat: 0,
                                    });
                                }
                                at += each;
                            }
                        }
                        if !graces.is_empty() && !e.is_rest() {
                            let each = (u64::from(TPQ) / 8)
                                .min(d / (2 * graces.len() as u64).max(1))
                                .max(1);
                            for g in &graces {
                                for p in &g.pitches {
                                    seq.notes.push(PlayNote {
                                        start,
                                        ticks: each,
                                        key: sound(p.midi()),
                                        velocity: vel,
                                        part: pi,
                                        program,
                                        tech: Tech {
                                            ornament: true,
                                            ..tech
                                        },
                                        repeat: 0,
                                    });
                                }
                                start += each;
                            }
                        }
                        // Pedal and "let ring": notes ring to the end of the line.
                        let ring_end = spans
                            .iter()
                            .filter(|sp| matches!(sp.kind, SpannerKind::Pedal | SpannerKind::LetRing))
                            .find(|sp| covers(sp, here))
                            .map(|sp| t + (local(sp.end.measure, sp.end.tick) - here) + u64::from(TPQ));
                        let lv_ring = ids.contains(&"laissez-vibrer");
                        // Ornaments: written marks, or a trill line over the note.
                        let orn_id = ids
                            .iter()
                            .copied()
                            .find(|i| perform::ornament(i, Pitch::new(Step::C, 0, 4), key, d, bpm).is_some())
                            .or_else(|| {
                                spans
                                    .iter()
                                    .any(|sp| sp.kind == SpannerKind::TrillLine && covers(sp, here))
                                    .then_some("trill")
                            });
                        let gliss_to = spans
                            .iter()
                            .find(|sp| {
                                matches!(sp.kind, SpannerKind::Glissando | SpannerKind::WavyGlissando)
                                    && local(sp.start.measure, sp.start.tick) == here
                            })
                            .and_then(|sp| key_at(score, pi, si, sp.end));
                        let n_notes = e.notes().len();
                        for (ni, n) in e.notes().iter().enumerate() {
                            let k = (pi, si, vi, n.pitch.midi());
                            let key_sounding = sound(n.pitch.midi());
                            // Rolled chords start one after another.
                            let roll = match e.arpeggio {
                                Arpeggio::Normal | Arpeggio::Up => ni as u64 * u64::from(TPQ) / 16,
                                Arpeggio::Down => (n_notes - 1 - ni) as u64 * u64::from(TPQ) / 16,
                                _ => 0,
                            };
                            if let Some(&idx) = open.get(&k)
                                && let Some(prev) = seq.notes.get_mut(idx)
                            {
                                // A tie: the note goes on.
                                prev.ticks = (t + length(d))
                                    .saturating_sub(prev.start)
                                    .saturating_sub(cut)
                                    .max(1);
                            } else {
                                let lead = start - t + roll;
                                // A tremolo fills the whole written length with its strokes.
                                let sounding = if repeat > 0 { d } else { length(d) };
                                let mut played = (sounding as i64 + sw_len).max(1) as u64;
                                played = played.saturating_sub(lead).saturating_sub(cut).max(1);
                                if let Some(re) = ring_end {
                                    played = played.max(re.saturating_sub(t));
                                }
                                if lv_ring {
                                    played = played.max(d * 3);
                                }
                                let at = start + roll;
                                let top = ni + 1 == n_notes;
                                let written: Option<Vec<perform::OrnNote>> = if let Some(g) =
                                    gliss_to.filter(|_| top)
                                {
                                    Some(perform::glissando(key_sounding, sound(g), played))
                                } else if let Some(id) = orn_id.filter(|_| top) {
                                    perform::ornament(id, n.pitch, key, played, bpm)
                                        .map(|v| v.into_iter().map(|(o, l, kk)| (o, l, sound(kk))).collect())
                                } else {
                                    None
                                };
                                match written {
                                    Some(notes) if !notes.is_empty() => {
                                        let count = notes.len();
                                        for (i, (o, l, kk)) in notes.into_iter().enumerate() {
                                            let last = i + 1 == count;
                                            seq.notes.push(PlayNote {
                                                start: at + o,
                                                ticks: l,
                                                key: kk,
                                                velocity: if i == 0 || last {
                                                    vel
                                                } else {
                                                    vel.saturating_sub(6).max(1)
                                                },
                                                part: pi,
                                                program,
                                                tech: Tech {
                                                    ornament: !last || i > 0,
                                                    ..tech
                                                },
                                                repeat: 0,
                                            });
                                        }
                                    }
                                    _ => seq.notes.push(PlayNote {
                                        start: at,
                                        ticks: played,
                                        key: key_sounding,
                                        velocity: vel,
                                        part: pi,
                                        program,
                                        tech,
                                        repeat,
                                    }),
                                }
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
    }
    harmony(score, &played, end, &mut seq.notes);
    seq.end = end;
    seq.tempos = timing::tempo_map(score, &played, end, &stretches, &pauses);
    seq.notes.sort_by_key(|n| (n.start, n.part, n.key));
    let n = seq.notes.len();
    untangle_repeated_keys(&mut seq.notes);
    debug_assert_eq!(n, seq.notes.len());
    seq.prepare();
    seq
}

/// Chord symbols sounding as their part's harmony channel (its own instrument): from each
/// symbol until the next one of the part (in the played order), the bass low and the chord
/// above it. A symbol that names no chord ("N.C.") is silence until the next.
fn harmony(score: &Score, played: &[(usize, u64)], end: u64, notes: &mut Vec<PlayNote>) {
    for (pi, part) in score.parts.iter().enumerate() {
        let h = part.harmony;
        if !h.play || h.volume == 0 {
            continue;
        }
        let mut chords: Vec<(u64, &str)> = Vec::new();
        for &(m, mtick) in played {
            let Some(pm) = part.measures.get(m) else { continue };
            let mut here: Vec<(u32, &str)> = Vec::new();
            for sm in &pm.staves {
                for v in &sm.voices {
                    for (e, on) in v.events.iter().zip(v.onsets()) {
                        if let Some(cs) = e.chord_symbol.as_deref() {
                            here.push((on, cs));
                        }
                    }
                }
            }
            here.sort_by_key(|c| c.0);
            here.dedup_by_key(|c| c.0);
            chords.extend(here.into_iter().map(|(on, cs)| (mtick + u64::from(on), cs)));
        }
        let velocity = (u32::from(h.volume) * 11 / 10).clamp(1, 127) as u8;
        for (i, &(start, cs)) in chords.iter().enumerate() {
            let stop = chords.get(i + 1).map_or(end, |c| c.0);
            let Some(tones) = scoreart_model::chord::chord_tones(cs) else {
                continue;
            };
            if stop <= start {
                continue;
            }
            for key in tones.keys() {
                notes.push(PlayNote {
                    start,
                    ticks: stop - start,
                    key,
                    velocity,
                    part: pi,
                    program: h.program,
                    tech: Tech {
                        harmony: true,
                        ..Tech::default()
                    },
                    repeat: 0,
                });
            }
        }
    }
}

/// A note that would still sound when the same key of the same part is struck again (legato
/// overlap, a long note under a short one) ends just as it is struck again, so the new note is
/// not cut off by the old one's release. `notes` is sorted by start.
fn untangle_repeated_keys(notes: &mut [PlayNote]) {
    // By sound too: a chord symbol's note and a melody note of one key are two instruments.
    let mut last: std::collections::HashMap<(usize, u8, u8), usize> = std::collections::HashMap::new();
    for i in 0..notes.len() {
        let Some(&cur) = notes.get(i) else { continue };
        if let Some(&j) = last.get(&(cur.part, cur.program, cur.key))
            && let Some(prev) = notes.get_mut(j)
            && prev.start < cur.start
            && prev.start + prev.ticks > cur.start
        {
            prev.ticks = (cur.start - prev.start)
                .saturating_sub(u64::from(TPQ) / 96)
                .max(1);
        }
        last.insert((cur.part, cur.program, cur.key), i);
    }
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
        // Mixer: channel volume (CC 7) and pan (CC 10).
        let volume = (u32::from(part.sound.volume.min(100)) * 127 / 100) as u8;
        let pan = (64 + i32::from(part.sound.pan.clamp(-100, 100)) * 63 / 100).clamp(0, 127) as u8;
        for (controller, value) in [(7u8, volume), (10, pan)] {
            abs.push((
                0,
                TrackEventKind::Midi {
                    channel: ch,
                    message: MidiMessage::Controller {
                        controller: u7::new(controller),
                        value: u7::new(value),
                    },
                },
            ));
        }
        // Instrument changes: a program change before the first note of the new instrument.
        let mut current = part.program;
        for n in seq.notes.iter().filter(|n| n.part == pi && !n.tech.harmony) {
            if n.program != current {
                current = n.program;
                abs.push((
                    scale(n.start),
                    TrackEventKind::Midi {
                        channel: ch,
                        message: MidiMessage::ProgramChange {
                            program: u7::new(current.min(127)),
                        },
                    },
                ));
            }
        }
        // Hairpins: expression (CC 11), 100 = the level the swell starts from.
        for sw in seq.swells.iter().filter(|s| s.part == pi) {
            let steps = 16u64;
            for i in 0..=steps {
                let at = sw.from + (sw.to - sw.from) * i / steps;
                let v = f64::from(sw.v0) + (f64::from(sw.v1) - f64::from(sw.v0)) * i as f64 / steps as f64;
                let cc = (100.0 * v / f64::from(sw.v0.max(1))).round().clamp(1.0, 127.0) as u8;
                abs.push((
                    scale(at),
                    TrackEventKind::Midi {
                        channel: ch,
                        message: MidiMessage::Controller {
                            controller: u7::new(11),
                            value: u7::new(cc),
                        },
                    },
                ));
            }
            abs.push((
                scale(sw.to),
                TrackEventKind::Midi {
                    channel: ch,
                    message: MidiMessage::Controller {
                        controller: u7::new(11),
                        value: u7::new(100),
                    },
                },
            ));
        }
        let strokes: Vec<PlayNote> = seq
            .notes
            .iter()
            .filter(|n| n.part == pi && !n.tech.harmony)
            .flat_map(PlayNote::strokes)
            .collect();
        for n in &strokes {
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
    // Chord symbols: a track per part that plays them, on channels after the parts'.
    let mut next_ch = score.parts.len();
    for (pi, part) in score.parts.iter().enumerate() {
        let notes: Vec<&PlayNote> = seq
            .notes
            .iter()
            .filter(|n| n.part == pi && n.tech.harmony)
            .collect();
        if notes.is_empty() {
            continue;
        }
        if next_ch == 9 {
            next_ch += 1;
        }
        let ch = u4::new((next_ch % 16) as u8);
        next_ch += 1;
        let mut abs: Vec<(u64, TrackEventKind<'static>)> = vec![(
            0,
            TrackEventKind::Midi {
                channel: ch,
                message: MidiMessage::ProgramChange {
                    program: u7::new(part.harmony.program.min(127)),
                },
            },
        )];
        for n in notes {
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
    use scoreart_model::marks::Articulation;
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
        // Tied into one note; a flute plays it slightly shorter than written.
        let len = seq.notes[0].ticks;
        assert!(
            len > u64::from(TPQ) * 18 / 10 && len <= u64::from(TPQ) * 2,
            "{len}"
        );
        assert_eq!(seq.notes[0].key, 69);
        let bytes = to_smf(&s);
        let smf = midly::Smf::parse(&bytes).unwrap();
        assert_eq!(smf.tracks.len(), 2);
        let secs = seq.seconds_at(u64::from(TPQ) * 4);
        assert!((secs - 2.4).abs() < 1e-9, "4 quarters at 100 bpm");
        assert_eq!(seq.tick_at(secs), u64::from(TPQ) * 4);
    }

    /// A flute line of quarter notes in 4/4 (C D E F | G A B C …), `measures` long.
    fn line(measures: usize) -> Score {
        let flute = instrument("flute").unwrap();
        let mut s = Score::new("T", &[flute], TimeSig::default(), KeySig::default(), measures);
        let steps = [Step::C, Step::D, Step::E, Step::F, Step::G, Step::A, Step::B];
        let mut pos = Pos::default();
        for i in 0..measures * 4 {
            let kind = EventKind::Chord(vec![Note::new(Pitch::new(steps[i % 7], 0, 5))]);
            pos = s
                .overwrite(pos, kind, Duration::new(NoteValue::Quarter, 0))
                .unwrap();
        }
        s
    }

    fn ev(m: usize, i: usize) -> edit::EventRef {
        edit::EventRef {
            part: 0,
            staff: 0,
            voice: 0,
            measure: m,
            index: i,
        }
    }

    fn mark(s: &mut Score, m: usize, i: usize, id: &str) {
        let a = Articulation::from_id(id).unwrap();
        s.event_mut(ev(m, i)).unwrap().articulations.push(a);
    }

    fn span(s: &mut Score, kind: SpannerKind, a: (usize, u32), b: (usize, u32)) {
        s.spanners.push(Spanner {
            kind,
            part: 0,
            staff: 0,
            voice: 0,
            start: Anchor {
                measure: a.0,
                tick: a.1,
            },
            end: Anchor {
                measure: b.0,
                tick: b.1,
            },
            dy: 0,
            spread: 0,
        });
    }

    #[test]
    fn a_fermata_holds_and_a_caesura_pauses() {
        let plain = sequence(&line(2));
        let mut s = line(2);
        mark(&mut s, 0, 3, "fermata");
        let held = sequence(&s);
        let q = u64::from(TPQ);
        // The fourth beat lasts twice as long; everything after moves on by one beat.
        assert!((held.seconds_at(4 * q) - plain.seconds_at(4 * q) - plain.seconds_at(q)).abs() < 1e-6);
        let mut s = line(2);
        mark(&mut s, 0, 3, "caesura");
        let paused = sequence(&s);
        let gap = paused.seconds_at(4 * q) - plain.seconds_at(4 * q);
        assert!(gap > 0.4, "a caesura adds a pause ({gap} s)");
        let n = paused.notes.iter().find(|n| n.start == 3 * q).unwrap();
        let sounding = paused.seconds_at(n.start + n.ticks) - paused.seconds_at(n.start);
        assert!(sounding < 0.6 * 0.95, "the note stops before the pause");
    }

    #[test]
    fn ritardando_slows_down_and_a_tempo_returns() {
        let mut s = line(6);
        s.measures[1].tempo_text = Some("rit.".into());
        s.measures[3].tempo_text = Some("a tempo".into());
        let seq = sequence(&s);
        let q = u64::from(TPQ);
        let beat = |m: u64| seq.seconds_at(m * 4 * q + q) - seq.seconds_at(m * 4 * q);
        assert!((beat(0) - 0.6).abs() < 1e-6);
        assert!(beat(2) > beat(1) && beat(1) > 0.6, "slower and slower");
        assert!((beat(4) - 0.6).abs() < 1e-6, "a tempo");
        // accel. without a target speeds up and then holds.
        let mut s = line(8);
        s.measures[1].tempo_text = Some("accel.".into());
        let seq = sequence(&s);
        let beat = |m: u64| seq.seconds_at(m * 4 * q + q) - seq.seconds_at(m * 4 * q);
        assert!(beat(4) < beat(1));
        assert!((beat(6) - beat(7)).abs() < 1e-9, "held after the ramp");
    }

    #[test]
    fn ornaments_are_written_out() {
        let mut s = line(1);
        mark(&mut s, 0, 0, "trill");
        mark(&mut s, 0, 1, "mordent");
        let seq = sequence(&s);
        let q = u64::from(TPQ);
        let first: Vec<u8> = seq.notes.iter().filter(|n| n.start < q).map(|n| n.key).collect();
        assert!(first.len() >= 4, "{first:?}");
        assert_eq!(&first[..2], &[72, 74], "C D C D …");
        let second: Vec<u8> = seq
            .notes
            .iter()
            .filter(|n| n.start >= q && n.start < 2 * q)
            .map(|n| n.key)
            .collect();
        assert_eq!(second, [74, 72, 74], "D C D");
    }

    #[test]
    fn octave_lines_tremolo_and_glissando() {
        let mut s = line(1);
        span(&mut s, SpannerKind::Ottava(1), (0, 0), (0, TPQ));
        s.event_mut(ev(0, 2)).unwrap().tremolo = Tremolo::Strokes(2);
        span(&mut s, SpannerKind::Glissando, (0, 3 * TPQ), (0, 3 * TPQ));
        let seq = sequence(&s);
        let keys: Vec<u8> = seq.notes.iter().map(|n| n.key).collect();
        assert_eq!(&keys[..2], &[84, 86], "8va sounds an octave higher");
        let trem = seq.notes.iter().find(|n| n.repeat > 0).unwrap();
        assert_eq!(trem.strokes().len(), 4, "sixteenths in a quarter");
        assert!(to_smf(&s).len() > 100);
    }

    #[test]
    fn slurs_bind_hairpins_swell_and_techniques_switch() {
        let mut s = line(2);
        span(&mut s, SpannerKind::Slur, (0, 0), (0, TPQ * 3));
        mark(&mut s, 0, 1, "staccato");
        span(&mut s, SpannerKind::Crescendo, (1, 0), (1, TPQ * 3));
        s.event_mut(ev(1, 0)).unwrap().text = Some("con sord.".into());
        let seq = sequence(&s);
        let q = u64::from(TPQ);
        let n0 = seq.notes[0];
        assert!(n0.tech.legato && n0.ticks > q, "slurred notes run into the next");
        assert!(seq.notes[1].tech.short && seq.notes[1].ticks < q / 2 + 1);
        let last = seq.notes.iter().find(|n| n.start == 3 * q).unwrap();
        assert!(
            !last.tech.legato && last.ticks < q,
            "the slur's last note is let go"
        );
        let sw = seq.swells[0];
        assert_eq!((sw.from, sw.to, sw.v0), (4 * q, 7 * q, 80));
        assert!(sw.v1 > sw.v0);
        let muted = seq
            .notes
            .iter()
            .filter(|n| n.start >= 4 * q)
            .all(|n| n.tech.technique == Technique::Muted);
        assert!(muted);
        // After the swell the louder level holds.
        let mut s2 = line(3);
        span(&mut s2, SpannerKind::Crescendo, (0, 0), (0, TPQ * 3));
        let seq = sequence(&s2);
        assert!(seq.notes.iter().find(|n| n.start == 8 * q).unwrap().velocity > 80);
    }

    #[test]
    fn swing_moves_offbeats() {
        let flute = instrument("flute").unwrap();
        let mut s = Score::new("T", &[flute], TimeSig::default(), KeySig::default(), 1);
        let mut pos = Pos::default();
        for _ in 0..8 {
            let kind = EventKind::Chord(vec![Note::new(Pitch::new(Step::C, 0, 5))]);
            pos = s
                .overwrite(pos, kind, Duration::new(NoteValue::Eighth, 0))
                .unwrap();
        }
        s.measures[0].text = Some("Swing".into());
        let seq = sequence(&s);
        let q = u64::from(TPQ);
        assert_eq!(seq.notes[1].start, q / 2 + q / 6);
    }
}
