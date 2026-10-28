//! Edit primitives. All of them keep the score's invariants ([`Score::normalize`]).

use serde::{Deserialize, Serialize};

use crate::{Duration, Error, Event, EventKind, MeasureInfo, Note, Pitch, Result, Score, limits};

/// A position in a voice: measure and tick within it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Pos {
    pub part: usize,
    pub staff: usize,
    pub voice: usize,
    pub measure: usize,
    pub tick: u32,
}

/// One event of a voice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct EventRef {
    pub part: usize,
    pub staff: usize,
    pub voice: usize,
    pub measure: usize,
    pub index: usize,
}

impl EventRef {
    pub fn pos(self, score: &Score) -> Option<Pos> {
        let v = score.voice(self.part, self.staff, self.measure, self.voice)?;
        let tick = *v.onsets().get(self.index)?;
        Some(Pos {
            part: self.part,
            staff: self.staff,
            voice: self.voice,
            measure: self.measure,
            tick,
        })
    }
}

impl Pos {
    pub fn event(self, score: &Score) -> Option<EventRef> {
        let v = score.voice(self.part, self.staff, self.measure, self.voice)?;
        let (index, _) = v.event_at(self.tick)?;
        Some(EventRef {
            part: self.part,
            staff: self.staff,
            voice: self.voice,
            measure: self.measure,
            index,
        })
    }
}

impl Score {
    pub fn event(&self, r: EventRef) -> Option<&Event> {
        self.voice(r.part, r.staff, r.measure, r.voice)?
            .events
            .get(r.index)
    }

    pub fn event_mut(&mut self, r: EventRef) -> Option<&mut Event> {
        self.voice_mut(r.part, r.staff, r.measure, r.voice)?
            .events
            .get_mut(r.index)
    }

    /// Checks that `pos` addresses an existing staff and measure; creates voices 1–3 on demand.
    fn ensure_voice(&mut self, pos: Pos) -> Result<()> {
        if pos.voice >= limits::MAX_VOICES {
            return Err(Error::Invalid(format!(
                "voice {} (0–{})",
                pos.voice,
                limits::MAX_VOICES - 1
            )));
        }
        let len = self.measure_ticks(pos.measure);
        let sm = self
            .staff_measure_mut(pos.part, pos.staff, pos.measure)
            .ok_or(Error::NotFound("staff or measure"))?;
        while sm.voices.len() <= pos.voice {
            let mut v = crate::Voice::default();
            for d in Duration::decompose(len) {
                v.events.push(Event::rest(d));
            }
            sm.voices.push(v);
        }
        Ok(())
    }

    /// Note input in overwrite mode: writes `kind` lasting `duration` at `pos`, replacing what was
    /// there. A note that runs past the barline is split and tied into the following measure(s)
    /// (measures are appended when needed). Returns the position right after the new event.
    pub fn overwrite(&mut self, pos: Pos, kind: EventKind, duration: Duration) -> Result<Pos> {
        if let EventKind::Chord(n) = &kind
            && n.is_empty()
        {
            return Err(Error::Invalid("empty chord".into()));
        }
        let mut left = duration.ticks();
        if left == 0 {
            return Err(Error::Invalid("zero duration".into()));
        }
        let mut pos = pos;
        let mut first = true;
        let mut guard = 0;
        while left > 0 {
            guard += 1;
            if guard > 64 {
                return Err(Error::Limit("note too long"));
            }
            self.ensure_voice(pos)?;
            let len = self.measure_ticks(pos.measure);
            if pos.tick >= len {
                return Err(Error::Invalid(format!(
                    "tick {} past measure end {len}",
                    pos.tick
                )));
            }
            let room = len - pos.tick;
            let here = left.min(room);
            left -= here;
            let pieces: Vec<Duration> = if here == duration.ticks() && first {
                vec![duration]
            } else {
                Duration::decompose(here)
            };
            let continues = left > 0;
            let mut events = Vec::new();
            let count = pieces.len();
            for (i, d) in pieces.into_iter().enumerate() {
                let mut kind = kind.clone();
                let tie = continues || i + 1 < count;
                if let EventKind::Chord(notes) = &mut kind {
                    for n in notes.iter_mut() {
                        n.tie = tie || n.tie && !continues && i + 1 == count;
                    }
                }
                let mut ev = Event::rest(d);
                ev.kind = kind;
                events.push(ev);
            }
            self.replace_span(pos, here, events)?;
            first = false;
            if continues {
                if pos.measure + 1 >= self.measures.len() {
                    self.insert_measures(self.measures.len(), 1)?;
                }
                pos = Pos {
                    measure: pos.measure + 1,
                    tick: 0,
                    ..pos
                };
            } else {
                pos.tick += here;
            }
        }
        // Normalise the "after" position to the next measure start when we hit the barline.
        if pos.tick >= self.measure_ticks(pos.measure) && pos.measure + 1 < self.measures.len() {
            pos = Pos {
                measure: pos.measure + 1,
                tick: 0,
                ..pos
            };
        }
        Ok(pos)
    }

    /// Replaces ticks `[pos.tick, pos.tick + span)` of one voice with `events` (which must last
    /// exactly `span`). Events cut at the front keep their head (re-split, tied); events cut at the
    /// back leave rests.
    fn replace_span(&mut self, pos: Pos, span: u32, events: Vec<Event>) -> Result<()> {
        let t0 = pos.tick;
        let t1 = pos.tick.saturating_add(span);
        let v = self
            .voice_mut(pos.part, pos.staff, pos.measure, pos.voice)
            .ok_or(Error::NotFound("voice"))?;
        let mut out: Vec<Event> = Vec::new();
        let mut t = 0u32;
        let mut inserted = false;
        let old = std::mem::take(&mut v.events);
        for e in old {
            let s = t;
            let end = t.saturating_add(e.ticks());
            t = end;
            if end <= t0 || s >= t1 {
                if s >= t1 && !inserted {
                    out.extend(events.iter().cloned());
                    inserted = true;
                }
                out.push(e);
                continue;
            }
            if s < t0 {
                // Head of an event that starts before the span.
                let pieces = Duration::decompose(t0 - s);
                let n = pieces.len();
                for (i, d) in pieces.into_iter().enumerate() {
                    let mut head = e.clone();
                    head.duration = d;
                    for note in head.notes_mut() {
                        note.tie = i + 1 < n;
                    }
                    if i > 0 {
                        head.clear_marks();
                    }
                    out.push(head);
                }
            }
            if !inserted {
                out.extend(events.iter().cloned());
                inserted = true;
            }
            if end > t1 {
                for d in Duration::decompose(end - t1) {
                    out.push(Event::rest(d));
                }
            }
        }
        if !inserted {
            out.extend(events);
        }
        v.events = out;
        self.normalize();
        Ok(())
    }

    /// Adds `pitch` to the chord at `r` (a rest becomes a single note). Duplicates are ignored.
    pub fn add_to_chord(&mut self, r: EventRef, pitch: Pitch) -> Result<()> {
        let e = self.event_mut(r).ok_or(Error::NotFound("event"))?;
        match &mut e.kind {
            EventKind::Rest => e.kind = EventKind::Chord(vec![Note::new(pitch)]),
            EventKind::Chord(notes) => {
                if notes.len() >= limits::MAX_CHORD_NOTES {
                    return Err(Error::Limit("chord notes"));
                }
                if !notes.iter().any(|n| n.pitch == pitch) {
                    notes.push(Note::new(pitch));
                    notes.sort_by_key(|n| (n.pitch.diatonic(), n.pitch.alter));
                }
            }
        }
        Ok(())
    }

    /// Turns the event at `r` into a rest of the same length (MuseScore-style delete).
    pub fn make_rest(&mut self, r: EventRef) -> Result<()> {
        let e = self.event_mut(r).ok_or(Error::NotFound("event"))?;
        e.kind = EventKind::Rest;
        e.clear_marks();
        self.merge_rests(r.part, r.staff, r.measure, r.voice);
        Ok(())
    }

    /// Merges runs of plain rests into the fewest durations.
    fn merge_rests(&mut self, part: usize, staff: usize, measure: usize, voice: usize) {
        let Some(v) = self.voice_mut(part, staff, measure, voice) else {
            return;
        };
        // Only merge when every rest in the run is plain; keep tuplets as they are.
        let mut out: Vec<Event> = Vec::new();
        let mut run = 0u32;
        let flush = |out: &mut Vec<Event>, run: &mut u32| {
            if *run > 0 {
                for d in Duration::decompose(*run) {
                    out.push(Event::rest(d));
                }
                *run = 0;
            }
        };
        for e in std::mem::take(&mut v.events) {
            let plain = e.is_rest()
                && e.duration.tuplet.is_none()
                && e.dynamic.is_none()
                && e.articulations.is_empty()
                && e.text.is_none()
                && e.chord_symbol.is_none();
            if plain {
                run += e.ticks();
            } else {
                flush(&mut out, &mut run);
                out.push(e);
            }
        }
        flush(&mut out, &mut run);
        v.events = out;
    }

    /// Inserts `count` empty measures before measure `at` (`at == len` appends).
    pub fn insert_measures(&mut self, at: usize, count: usize) -> Result<()> {
        let at = at.min(self.measures.len());
        if self.measures.len().saturating_add(count) > limits::MAX_MEASURES {
            return Err(Error::Limit("measures"));
        }
        let appending = at == self.measures.len();
        for sp in &mut self.spanners {
            for a in [&mut sp.start, &mut sp.end] {
                if a.measure >= at && !appending {
                    a.measure += count;
                }
            }
        }
        // Headings and texts stay with their music.
        for b in &mut self.blocks {
            if b.measure >= at && !appending {
                b.measure += count;
            }
        }
        for _ in 0..count {
            self.measures.insert(at, MeasureInfo::default());
            for p in &mut self.parts {
                if at <= p.measures.len() {
                    p.measures.insert(at, crate::PartMeasure::default());
                }
            }
        }
        if appending && at > 0 {
            // The final barline moves to the new last measure.
            if let Some(prev) = self.measures.get_mut(at - 1)
                && prev.barline == crate::Barline::Final
            {
                prev.barline = crate::Barline::Regular;
                if let Some(last) = self.measures.last_mut() {
                    last.barline = crate::Barline::Final;
                }
            }
        }
        self.normalize();
        Ok(())
    }

    /// Deletes measures `[at, at + count)`; the score keeps at least one measure.
    pub fn delete_measures(&mut self, at: usize, count: usize) -> Result<()> {
        if at >= self.measures.len() {
            return Err(Error::NotFound("measure"));
        }
        let end = at.saturating_add(count).min(self.measures.len());
        if end - at >= self.measures.len() {
            return Err(Error::Invalid("cannot delete every measure".into()));
        }
        let first = at == 0;
        let removed_final = self.measures.get(end - 1).map(|m| m.barline) == Some(crate::Barline::Final);
        let carried: Option<MeasureInfo> = if first {
            self.measures.first().cloned()
        } else {
            None
        };
        let removed = end - at;
        self.spanners
            .retain(|sp| !(sp.start.measure >= at && sp.end.measure < end));
        for sp in &mut self.spanners {
            if sp.start.measure >= end {
                sp.start.measure -= removed;
            } else if sp.start.measure >= at {
                sp.start = crate::Anchor { measure: at, tick: 0 };
            }
            if sp.end.measure >= end {
                sp.end.measure -= removed;
            } else if sp.end.measure >= at {
                sp.end = crate::Anchor {
                    measure: at.saturating_sub(1),
                    tick: u32::MAX,
                };
            }
        }
        for b in &mut self.blocks {
            if b.measure >= end {
                b.measure -= removed;
            } else if b.measure >= at {
                b.measure = at;
            }
        }
        self.measures.drain(at..end);
        for p in &mut self.parts {
            let e = end.min(p.measures.len());
            if at < e {
                p.measures.drain(at..e);
            }
        }
        if let (Some(c), Some(m0)) = (carried, self.measures.first_mut()) {
            // Keep the opening time/key/tempo when measure 1 goes away.
            m0.time = m0.time.or(c.time);
            m0.key = m0.key.or(c.key);
            m0.tempo = m0.tempo.or(c.tempo);
        }
        if removed_final && let Some(last) = self.measures.last_mut() {
            last.barline = crate::Barline::Final;
        }
        self.normalize();
        Ok(())
    }

    /// Transposes every note of the whole score by `semitones`, re-spelled for each measure's key.
    pub fn transpose(&mut self, semitones: i32) {
        let keys: Vec<_> = (0..self.measures.len()).map(|m| self.key_at(m)).collect();
        for p in &mut self.parts {
            for (m, pm) in p.measures.iter_mut().enumerate() {
                let key = keys.get(m).copied().unwrap_or_default();
                for sm in &mut pm.staves {
                    for v in &mut sm.voices {
                        for e in &mut v.events {
                            for n in e.notes_mut() {
                                n.pitch = n.pitch.transpose_chromatic(semitones, key);
                            }
                        }
                    }
                }
            }
        }
    }

    /// The next event position after `r` in the same voice, crossing barlines.
    pub fn next_event(&self, r: EventRef) -> Option<EventRef> {
        let v = self.voice(r.part, r.staff, r.measure, r.voice)?;
        if r.index + 1 < v.events.len() {
            return Some(EventRef {
                index: r.index + 1,
                ..r
            });
        }
        let mut m = r.measure + 1;
        while m < self.measures.len() {
            if self
                .voice(r.part, r.staff, m, r.voice)
                .is_some_and(|v| !v.events.is_empty())
            {
                return Some(EventRef {
                    measure: m,
                    index: 0,
                    ..r
                });
            }
            m += 1;
        }
        None
    }

    pub fn prev_event(&self, r: EventRef) -> Option<EventRef> {
        if r.index > 0 {
            return Some(EventRef {
                index: r.index - 1,
                ..r
            });
        }
        let mut m = r.measure;
        while m > 0 {
            m -= 1;
            if let Some(v) = self.voice(r.part, r.staff, m, r.voice)
                && !v.events.is_empty()
            {
                return Some(EventRef {
                    measure: m,
                    index: v.events.len() - 1,
                    ..r
                });
            }
        }
        None
    }
}
