//! Rearranging parts: changing a part's instrument without losing music, splitting a line of a
//! part off into an instrument of its own, moving and removing parts.
//!
//! A *line* is one voice on one staff of a part, e.g. the alto on the second staff of a
//! "Women" part, or voice 2 on a shared staff. Every operation here carries the lines' notes,
//! marks and lines (slurs, hairpins …) to where they go, and drops nothing unless asked to.

use serde::{Deserialize, Serialize};

use crate::{Error, Instrument, Part, PartMeasure, PartSound, Result, Score, StaffMeasure, Voice, limits};

/// One voice on one staff of a part (both 0-based).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Line {
    pub staff: usize,
    pub voice: usize,
}

/// What happens to a part's lines when it changes to another instrument.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rearrange {
    /// Every line goes into the new instrument; lines of staves it does not have become extra
    /// voices on its last staff.
    Merge,
    /// `keep` becomes a new part with the new instrument; the old part stays with the other
    /// lines.
    Split { keep: Line },
    /// Only `keep` is kept, in the new instrument; the other lines are dropped.
    Discard { keep: Line },
}

impl Part {
    /// The lines that hold notes, staff by staff; the first voice of every staff counts even when
    /// it holds only rests.
    pub fn lines(&self) -> Vec<Line> {
        let mut out = Vec::new();
        for staff in 0..self.staff_count() {
            let voices = self
                .measures
                .iter()
                .filter_map(|m| m.staves.get(staff))
                .map(|s| s.voices.len())
                .max()
                .unwrap_or(1)
                .max(1);
            for voice in 0..voices {
                let used = voice == 0
                    || self.measures.iter().any(|m| {
                        m.staves
                            .get(staff)
                            .and_then(|s| s.voices.get(voice))
                            .is_some_and(|v| !v.is_empty())
                    });
                if used {
                    out.push(Line { staff, voice });
                }
            }
        }
        out
    }

    /// Whether a line holds any notes.
    pub fn line_has_notes(&self, line: Line) -> bool {
        self.measures.iter().any(|m| {
            m.staves
                .get(line.staff)
                .and_then(|s| s.voices.get(line.voice))
                .is_some_and(|v| !v.is_empty())
        })
    }

    /// "Staff 2", or "Staff 1, voice 2" on a staff shared by several lines.
    pub fn line_label(&self, line: Line) -> String {
        let shared = self.lines().iter().filter(|l| l.staff == line.staff).count() > 1;
        if shared {
            format!("Staff {}, voice {}", line.staff + 1, line.voice + 1)
        } else {
            format!("Staff {}", line.staff + 1)
        }
    }
}

/// Where the lines of a part go: `(from, to_staff)` in order; the voice on the target staff is
/// given by the order.
type Plan = Vec<(Line, usize)>;

/// Assigns target voices: lines going to the same staff are stacked as voices 1, 2 …
fn voices_of(plan: &Plan) -> Result<Vec<(Line, Line)>> {
    let mut next = [0usize; limits::MAX_STAVES];
    let mut out = Vec::new();
    for (from, to) in plan {
        let slot = next.get_mut(*to).ok_or(Error::Limit("too many staves"))?;
        if *slot >= limits::MAX_VOICES {
            return Err(Error::Limit("a staff holds at most four voices"));
        }
        out.push((
            *from,
            Line {
                staff: *to,
                voice: *slot,
            },
        ));
        *slot += 1;
    }
    Ok(out)
}

/// The part's measures with its lines moved as `map` says, on `staves` staves.
fn rebuild(part: &Part, map: &[(Line, Line)], staves: usize) -> Vec<PartMeasure> {
    part.measures
        .iter()
        .map(|pm| {
            let mut out = PartMeasure {
                staves: vec![StaffMeasure::default(); staves],
                instrument_change: pm.instrument_change.clone(),
            };
            for (from, to) in map {
                let Some(src) = pm.staves.get(from.staff) else {
                    continue;
                };
                let Some(dst) = out.staves.get_mut(to.staff) else {
                    continue;
                };
                if dst.voices.len() <= to.voice {
                    dst.voices.resize_with(to.voice + 1, Voice::default);
                }
                if let (Some(v), Some(slot)) = (src.voices.get(from.voice), dst.voices.get_mut(to.voice)) {
                    *slot = v.clone();
                }
                // The staff's own clef change and repeat sign go with its first line.
                if to.voice == 0 {
                    dst.clef = src.clef;
                    dst.measure_repeat = src.measure_repeat;
                }
            }
            out
        })
        .collect()
}

impl Score {
    /// Changes `part` to `inst`, keeping, splitting off or dropping its lines as `how` says.
    /// Returns the index of the part that now plays `inst`.
    pub fn rearrange_part(&mut self, part: usize, inst: &Instrument, how: Rearrange) -> Result<usize> {
        let old = self.parts.get(part).ok_or(Error::NotFound("part"))?.clone();
        let lines = old.lines();
        let n = inst.clefs.len().max(1);
        let target = |l: Line| l.staff.min(n - 1);
        match how {
            Rearrange::Merge => {
                let mut plan: Plan = lines.iter().map(|l| (*l, target(*l))).collect();
                // Lines of the instrument's own staves first, so they keep their voices.
                plan.sort_by_key(|(l, to)| (*to, l.staff != *to, l.staff, l.voice));
                let map = voices_of(&plan)?;
                let measures = rebuild(&old, &map, n);
                let p = self.parts.get_mut(part).ok_or(Error::NotFound("part"))?;
                p.measures = measures;
                become_instrument(p, inst);
                self.remap_spanners(|sp_part, l| {
                    if sp_part != part {
                        return Some((sp_part, l));
                    }
                    map.iter().find(|(f, _)| *f == l).map(|(_, t)| (part, *t))
                });
                self.normalize();
                Ok(part)
            }
            Rearrange::Discard { keep } => {
                if !lines.contains(&keep) {
                    return Err(Error::NotFound("line"));
                }
                let map = vec![(
                    keep,
                    Line {
                        staff: target(keep),
                        voice: 0,
                    },
                )];
                let measures = rebuild(&old, &map, n);
                let p = self.parts.get_mut(part).ok_or(Error::NotFound("part"))?;
                p.measures = measures;
                become_instrument(p, inst);
                self.remap_spanners(|sp_part, l| {
                    if sp_part != part {
                        return Some((sp_part, l));
                    }
                    map.iter().find(|(f, _)| *f == l).map(|(_, t)| (part, *t))
                });
                self.normalize();
                Ok(part)
            }
            Rearrange::Split { keep } => {
                if !lines.contains(&keep) {
                    return Err(Error::NotFound("line"));
                }
                if lines.len() < 2 {
                    return Err(Error::Invalid("the instrument has only one line to give".into()));
                }
                if self.parts.len() >= limits::MAX_PARTS {
                    return Err(Error::Limit("too many instruments"));
                }
                // The new part.
                let new_map = vec![(
                    keep,
                    Line {
                        staff: target(keep),
                        voice: 0,
                    },
                )];
                let mut fresh = Part {
                    id: self.free_part_id(),
                    name: String::new(),
                    abbreviation: String::new(),
                    program: 0,
                    clefs: Vec::new(),
                    measures: rebuild(&old, &new_map, n),
                    sound: PartSound::default(),
                    hidden: Vec::new(),
                };
                become_instrument(&mut fresh, inst);
                fresh.sound = PartSound {
                    library: String::new(),
                    preset: String::new(),
                    ..old.sound.clone()
                };
                // The old part keeps the other lines on the staves that still have one.
                let rest: Vec<Line> = lines.iter().copied().filter(|l| *l != keep).collect();
                let mut staves: Vec<usize> = rest.iter().map(|l| l.staff).collect();
                staves.dedup();
                let plan: Plan = rest
                    .iter()
                    .map(|l| (*l, staves.iter().position(|s| *s == l.staff).unwrap_or(0)))
                    .collect();
                let old_map = voices_of(&plan)?;
                let measures = rebuild(&old, &old_map, staves.len().max(1));
                let p = self.parts.get_mut(part).ok_or(Error::NotFound("part"))?;
                p.measures = measures;
                p.clefs = staves.iter().filter_map(|s| old.clefs.get(*s).copied()).collect();
                p.hidden = staves.iter().map(|s| !old.staff_visible(*s)).collect();
                // The new part goes above when it takes the top line, else below.
                let at = if lines.first() == Some(&keep) {
                    part
                } else {
                    part + 1
                };
                self.parts.insert(at, fresh);
                let moved = if at == part { part + 1 } else { part };
                self.remap_spanners(|sp_part, l| {
                    if sp_part != part {
                        return Some((if sp_part >= at { sp_part + 1 } else { sp_part }, l));
                    }
                    if let Some((_, t)) = new_map.iter().find(|(f, _)| *f == l) {
                        return Some((at, *t));
                    }
                    old_map.iter().find(|(f, _)| *f == l).map(|(_, t)| (moved, *t))
                });
                self.normalize();
                Ok(at)
            }
        }
    }

    /// Moves a part to position `to` in the score.
    pub fn move_part(&mut self, from: usize, to: usize) -> Result<()> {
        if from >= self.parts.len() || to >= self.parts.len() {
            return Err(Error::NotFound("part"));
        }
        let p = self.parts.remove(from);
        self.parts.insert(to, p);
        self.remap_spanners(|sp, l| {
            let moved = if sp == from {
                to
            } else if from < to && sp > from && sp <= to {
                sp - 1
            } else if to < from && sp >= to && sp < from {
                sp + 1
            } else {
                sp
            };
            Some((moved, l))
        });
        self.normalize();
        Ok(())
    }

    /// Removes a part with its lines (slurs, hairpins …).
    pub fn remove_part(&mut self, part: usize) -> Result<()> {
        if part >= self.parts.len() {
            return Err(Error::NotFound("part"));
        }
        self.parts.remove(part);
        self.remap_spanners(|sp, l| match sp.cmp(&part) {
            std::cmp::Ordering::Less => Some((sp, l)),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some((sp - 1, l)),
        });
        self.normalize();
        Ok(())
    }

    /// A part id not used yet ("P3").
    pub(crate) fn free_part_id(&self) -> String {
        (1..)
            .map(|k| format!("P{k}"))
            .find(|id| self.parts.iter().all(|p| &p.id != id))
            .unwrap_or_default()
    }

    /// Moves or drops every spanner: `f(part, line)` gives its new part and line.
    fn remap_spanners(&mut self, f: impl Fn(usize, Line) -> Option<(usize, Line)>) {
        self.spanners.retain_mut(|sp| {
            let line = Line {
                staff: sp.staff,
                voice: sp.voice,
            };
            match f(sp.part, line) {
                Some((p, l)) => {
                    sp.part = p;
                    sp.staff = l.staff;
                    sp.voice = l.voice;
                    true
                }
                None => false,
            }
        });
    }
}

/// Name, sound and staves of `inst`; the staves already hold the music.
fn become_instrument(p: &mut Part, inst: &Instrument) {
    let hidden = std::mem::take(&mut p.hidden);
    p.name = inst.name.to_string();
    p.abbreviation = inst.abbreviation.to_string();
    p.program = inst.program;
    p.clefs = inst.clefs.to_vec();
    if p.clefs.is_empty() {
        p.clefs.push(crate::Clef::Treble);
    }
    p.hidden = hidden.into_iter().take(p.clefs.len()).collect();
    p.sound.library.clear();
    p.sound.preset.clear();
}
