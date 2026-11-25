use std::collections::{BTreeSet, HashMap};

use scoreart_model::edit::EventRef;
use scoreart_model::{
    Accidental, Barline, Clef, Event, KeySig, NoteValue, Pitch, Score, SpannerKind, TPQ, TextKind,
    TextStyles, TimeSig, TimeSymbol,
};

use crate::glyphs as g;
use crate::{Align, Hit, Item, Layout, Mark, MarkKind, Page, Shape, StaffBox, Style, Target, View};
use scoreart_model::BlockRef;

/// Key into the accidental table: (part, staff, measure, voice, event, note).
type NoteKey = (usize, usize, usize, usize, usize, usize);

/// Horizontal plan of one measure (independent of the system it lands in).
#[derive(Clone, Debug, Default)]
struct MeasurePlan {
    /// (tick, lead room before the head, stretchable space after it).
    cols: Vec<(u32, f32, f32)>,
    len: u32,
    /// Width of key/time/clef changes drawn at the start when not first in a system.
    prefix: f32,
    /// Per column: the least space it keeps when a system is squeezed (what lyrics and chord
    /// symbols need; 0 = squeezable like any space).
    mins: Vec<f32>,
}

impl MeasurePlan {
    fn fixed(&self) -> f32 {
        PAD_LEFT + self.cols.iter().map(|c| c.1).sum::<f32>()
    }
    fn stretch(&self) -> f32 {
        self.cols.iter().map(|c| c.2).sum()
    }
    /// A column's space at a stretch factor: squeezing stops at what lyrics need.
    fn space(&self, i: usize, stretch: f32) -> f32 {
        let sp = self.cols.get(i).map_or(0.0, |c| c.2) * stretch;
        sp.max(
            self.mins
                .get(i)
                .copied()
                .unwrap_or(0.0)
                .min(sp / stretch.max(1e-3)),
        )
    }
    /// Width at a stretch factor (without the prefix).
    fn width_at(&self, stretch: f32) -> f32 {
        self.fixed() + (0..self.cols.len()).map(|i| self.space(i, stretch)).sum::<f32>()
    }
}

const PAD_LEFT: f32 = 1.2;
const STEM_LEN: f32 = 3.5;
const NAME_SIZE: f32 = 1.9;
const LYRIC_SIZE: f32 = 1.8;
/// Distance between the lines of different verses.
const LYRIC_LINE: f32 = 2.4;

/// A verse number in front of a syllable ("1. Mit" → "1.", "Mit"); it hangs left of the
/// syllable, which alone is centred on its note.
fn verse_number(l: &str) -> (Option<&str>, &str) {
    let digits = l.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && digits < 3 {
        let rest = &l[digits..];
        if let Some(r) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(".\u{a0}")) {
            let r = r.trim_start();
            if !r.is_empty() {
                return (Some(&l[..digits + 1]), r);
            }
        }
    }
    (None, l)
}

/// Estimated width of text in the style of `kind` drawn `nominal` sp big by default.
fn styled_width(f: &Fonts, kind: TextKind, s: &str, nominal: f32) -> f32 {
    // The serif's own widths; the sans-serif of the screen runs a little wider.
    let sans = if f.styles.get(kind).font == scoreart_model::TextFont::Sans {
        1.1
    } else {
        1.0
    };
    text_width(s, f.size(kind, nominal)) * sans
}

/// Estimated width of a lyric syllable (without its joining hyphen and verse number).
fn syllable_width(f: &Fonts, l: &str) -> f32 {
    styled_width(
        f,
        TextKind::Lyrics,
        verse_number(l).1.trim_end_matches('-'),
        LYRIC_SIZE,
    )
}

/// Baseline of the first verse below the bottom staff line, staff spaces: 3 spaces (as
/// MuseScore places lyrics), lower when notes reach down so far that the syllables would touch
/// them (1 space clear).
fn lyric_base(score: &Score, part: usize, staff: usize, measures: &[usize]) -> f32 {
    let (_, down) = staff_reach(score, part, staff, measures);
    let size = Fonts::of(score).size(TextKind::Lyrics, LYRIC_SIZE);
    (down + 1.0 + 0.75 * size).max(3.0)
}

/// Distance between the lines of different verses, for the lyrics' size.
fn lyric_line(f: &Fonts) -> f32 {
    LYRIC_LINE * f.size(TextKind::Lyrics, LYRIC_SIZE) / LYRIC_SIZE
}

struct System {
    measures: Vec<usize>,
    /// Stretch factor applied to spaces.
    stretch: f32,
    indent: f32,
    header: f32,
    /// A last system too short to fill the line ends where its music ends (width from its
    /// indent); None: it fills the line.
    ragged: Option<f32>,
    /// It shows the long instrument names (the first system, or a section's first).
    long_names: bool,
}

/// Displayed accidentals for every note, following key, measure-scoped carry and ties.
fn accidentals(score: &Score) -> HashMap<NoteKey, Accidental> {
    let mut out = HashMap::new();
    for (pi, part) in score.parts.iter().enumerate() {
        for s in 0..part.staff_count() {
            // Pitches still sounding from a tie into the next measure: (diatonic, alter).
            let mut tied_in: Vec<(i32, i8)> = Vec::new();
            for (mi, pm) in part.measures.iter().enumerate() {
                let key = score.key_at(mi);
                let Some(sm) = pm.staves.get(s) else { continue };
                // All notes of all voices on this staff, in time order.
                let mut notes: Vec<(u32, usize, usize, usize, Pitch, bool, Option<Accidental>)> = Vec::new();
                for (vi, v) in sm.voices.iter().enumerate() {
                    for (ei, (e, on)) in v.events.iter().zip(v.onsets()).enumerate() {
                        for (ni, n) in e.notes().iter().enumerate() {
                            notes.push((on, vi, ei, ni, n.pitch, n.tie, n.forced));
                        }
                    }
                }
                notes.sort_by_key(|n| (n.0, n.1, n.2, n.3));
                let mut state: HashMap<i32, i8> = HashMap::new();
                let mut next_tied: Vec<(i32, i8)> = Vec::new();
                let mut prev_tied: Vec<(i32, i8, u32)> = Vec::new();
                for (on, vi, ei, ni, p, tie, forced) in notes {
                    let d = p.diatonic();
                    let is_tied_into = (on == 0 && tied_in.contains(&(d, p.alter)))
                        || prev_tied.iter().any(|t| t.0 == d && t.1 == p.alter && t.2 < on);
                    let current = state.get(&d).copied().unwrap_or_else(|| key.alter_for(p.step));
                    if let Some(f) = forced {
                        out.insert((pi, s, mi, vi, ei, ni), f);
                    } else if !is_tied_into && current != p.alter {
                        out.insert((pi, s, mi, vi, ei, ni), Accidental::from_alter(p.alter));
                    }
                    state.insert(d, p.alter);
                    if tie {
                        prev_tied.push((d, p.alter, on));
                        next_tied.push((d, p.alter));
                    }
                }
                tied_in = next_tied;
            }
        }
    }
    out
}

fn key_width(fifths: i8) -> f32 {
    let n = f32::from(fifths.unsigned_abs());
    if n == 0.0 { 0.0 } else { n * 1.05 + 0.6 }
}

fn time_width(t: TimeSig) -> f32 {
    match t.symbol {
        TimeSymbol::Common | TimeSymbol::Cut => 2.4,
        TimeSymbol::Normal => {
            let digits = t.beats.to_string().len().max(t.beat_type.to_string().len()) as f32;
            digits * g::TIME_DIGIT_W + 0.8
        }
    }
}

fn space_for(delta: u32, spacing: f32) -> f32 {
    let q = delta as f32 / TPQ as f32;
    (3.0 * q.powf(0.55) * spacing).clamp(1.5, 12.0)
}

fn plan_measures(score: &Score, acc: &HashMap<NoteKey, Accidental>, style: &Style) -> Vec<MeasurePlan> {
    let fonts = Fonts::of(score);
    let mut plans = Vec::with_capacity(score.measures.len());
    for m in 0..score.measures.len() {
        let len = score.measure_ticks(m);
        let mut onsets: BTreeSet<u32> = BTreeSet::new();
        let mut lead: HashMap<u32, f32> = HashMap::new();
        let mut extra: HashMap<u32, f32> = HashMap::new();
        // Lyrics: between two syllables of one verse there must be room for half of each (they
        // are centred on their notes) and a gap or hyphen: (from, to, width) constraints.
        let mut lyric_need: Vec<(u32, u32, f32)> = Vec::new();
        for (pi, part) in score.parts.iter().enumerate() {
            let Some(pm) = part.measures.get(m) else { continue };
            for (si, sm) in pm.staves.iter().enumerate() {
                // Hidden staves take no room.
                if !shown(score, pi, si) {
                    continue;
                }
                for (vi, v) in sm.voices.iter().enumerate() {
                    // Per verse: the previous syllable (onset, half width, hyphen after it).
                    let mut prev_syl: Vec<Option<(u32, f32, bool)>> = Vec::new();
                    for (e, on) in v.events.iter().zip(v.onsets()) {
                        for (verse, l) in e.lyrics.iter().enumerate().filter(|(_, l)| !l.is_empty()) {
                            if prev_syl.len() <= verse {
                                prev_syl.resize(verse + 1, None);
                            }
                            let half = syllable_width(&fonts, l) / 2.0;
                            if let Some(Some((pon, phalf, hyphen))) = prev_syl.get(verse).copied() {
                                let gap = if hyphen { 1.2 } else { 0.7 };
                                lyric_need.push((pon, on, phalf + half + gap));
                            }
                            if let Some(slot) = prev_syl.get_mut(verse) {
                                *slot = Some((on, half, l.ends_with('-')));
                            }
                        }
                    }
                    // Chord symbols (left-aligned on their note) must not run into the next one.
                    let mut prev_chord: Option<(u32, f32)> = None;
                    for (e, on) in v.events.iter().zip(v.onsets()) {
                        if let Some(cs) = &e.chord_symbol {
                            let w = styled_width(
                                &fonts,
                                TextKind::ChordSymbol,
                                &chord_text(cs, fonts.german),
                                2.2,
                            );
                            if let Some((pon, pw)) = prev_chord {
                                lyric_need.push((pon, on, pw + 1.0));
                            }
                            prev_chord = Some((on, w));
                        }
                    }
                    if let Some((pon, pw)) = prev_chord {
                        lyric_need.push((pon, len, pw - 0.5));
                    }
                    // The last syllable needs half its width (and its hyphen) before the barline.
                    for (pon, phalf, hyphen) in prev_syl.into_iter().flatten() {
                        lyric_need.push((pon, len, phalf + if hyphen { 1.1 } else { 0.5 }));
                    }
                    for (ei, (e, on)) in v.events.iter().zip(v.onsets()).enumerate() {
                        onsets.insert(on);
                        let n_acc = (0..e.notes().len())
                            .filter(|ni| acc.contains_key(&(pi, si, m, vi, ei, *ni)))
                            .count();
                        let mut w = if n_acc > 0 {
                            1.25 * (n_acc.min(3) as f32) + 0.2
                        } else {
                            0.0
                        };
                        w += lead_extra(e);
                        if w > 0.0 {
                            let l = lead.entry(on).or_insert(0.0);
                            *l = l.max(w);
                        }
                        let mut x = 0.0f32;
                        if e.duration.dots > 0 {
                            x += 0.45 + 0.5 * f32::from(e.duration.dots);
                        }
                        if e.duration.value.beams() > 0 && !e.is_rest() {
                            x = x.max(0.9);
                        }
                        if has_second(e) {
                            x += 1.0;
                        }
                        x += trail_extra(e);
                        // The first syllable of a measure hangs left into the space before it.
                        if on == 0 {
                            let half = e
                                .lyrics
                                .iter()
                                .map(|l| {
                                    // A verse number hangs further left.
                                    let num = verse_number(l).0.map_or(0.0, |n| {
                                        styled_width(&fonts, TextKind::Lyrics, n, LYRIC_SIZE) + 0.5
                                    });
                                    syllable_width(&fonts, l) / 2.0 + num
                                })
                                .fold(0.0f32, f32::max);
                            if half > 1.2 {
                                let l = lead.entry(on).or_insert(0.0);
                                *l = l.max(half - 1.2);
                            }
                        }
                        let ex = extra.entry(on).or_insert(0.0);
                        *ex = ex.max(x);
                    }
                }
            }
        }
        onsets.insert(0);
        let ticks: Vec<u32> = onsets.into_iter().filter(|t| *t < len.max(1)).collect();
        let mut cols = Vec::with_capacity(ticks.len());
        for (i, &t) in ticks.iter().enumerate() {
            let next = ticks.get(i + 1).copied().unwrap_or(len.max(t + 1));
            let sp = space_for(next - t, style.spacing) + extra.get(&t).copied().unwrap_or(0.0);
            cols.push((t, lead.get(&t).copied().unwrap_or(0.0), sp));
        }
        // Widen the columns between two syllables when they are too close; that room is
        // kept when the measure is squeezed.
        let mut mins = vec![0.0f32; cols.len()];
        for (from, to, need) in lyric_need {
            let have: f32 = cols.iter().filter(|c| c.0 >= from && c.0 < to).map(|c| c.2).sum();
            if have > 0.0 {
                let k = (need / have).max(1.0);
                for (c, mn) in cols
                    .iter_mut()
                    .zip(mins.iter_mut())
                    .filter(|(c, _)| c.0 >= from && c.0 < to)
                {
                    let share = c.2 * need / have;
                    c.2 *= k;
                    *mn = mn.max(share);
                }
            }
        }
        // Changes drawn at the measure start when it is not the first of a system.
        let info = score.measures.get(m);
        let mut prefix = 0.0;
        if m > 0 {
            let clef_change = score.parts.iter().any(|p| {
                p.measures
                    .get(m)
                    .is_some_and(|pm| pm.staves.iter().any(|s| s.clef.is_some()))
            });
            if clef_change {
                prefix += 2.6;
            }
            if let Some(k) = info.and_then(|i| i.key) {
                let prev = score.key_at(m - 1);
                prefix += key_width(k.fifths.unsigned_abs().max(prev.fifths.unsigned_abs()) as i8) + 0.4;
            }
            if let Some(t) = info.and_then(|i| i.time) {
                prefix += time_width(t) + 0.4;
            }
        }
        plans.push(MeasurePlan {
            cols,
            len,
            prefix,
            mins,
        });
    }
    plans
}

/// Room needed left of the noteheads: grace notes, arpeggio, slides in, parentheses.
fn lead_extra(e: &Event) -> f32 {
    let mut w = 0.0;
    let graces = e.grace.iter().filter(|g| !g.after).count();
    if graces > 0 {
        w += graces as f32 * 1.15 + 0.5;
    }
    if e.arpeggio != scoreart_model::Arpeggio::None {
        w += 1.3;
    }
    if e.articulations
        .iter()
        .any(|a| a.def().place == scoreart_model::marks::Place::Before)
    {
        w += 1.6;
    }
    if e.notes().iter().any(|n| n.paren) {
        w += 0.5;
    }
    w
}

/// Room for the graces after a note (and its dots): (x offset of the first, width).
fn after_graces_room(e: &Event) -> f32 {
    let n = e.grace.iter().filter(|g| g.after).count();
    if n == 0 {
        0.0
    } else {
        n as f32 * 1.15 + 0.6 + f32::from(e.duration.dots) * 0.5
    }
}

/// Room needed right of the noteheads: slides out, breath marks, parentheses.
fn trail_extra(e: &Event) -> f32 {
    let mut w = 0.0;
    if e.articulations
        .iter()
        .any(|a| a.def().place == scoreart_model::marks::Place::After)
    {
        w += 1.6;
    }
    if e.articulations
        .iter()
        .any(|a| a.def().place == scoreart_model::marks::Place::Breath)
    {
        w += 1.2;
    }
    if e.notes().iter().any(|n| n.paren) {
        w += 0.5;
    }
    w + after_graces_room(e)
}

fn has_second(e: &Event) -> bool {
    let n = e.notes();
    n.windows(2).any(|w| match w {
        [a, b] => (b.pitch.diatonic() - a.pitch.diatonic()).abs() == 1,
        _ => false,
    })
}

fn header_width(score: &Score, m: usize) -> f32 {
    let mut w = 0.8 + 2.9 + 0.6;
    w += key_width(score.key_at(m).fifths);
    let show_time = m == 0 || score.measures.get(m).is_some_and(|i| i.time.is_some());
    if show_time {
        w += time_width(score.time_at(m)) + 0.2;
    }
    w
}

/// Width of a text line at `size`: the advance widths of the serif text font (Crimson Text);
/// characters outside its table count half an em.
/// ♯ ♭ ♮ in a text (chord symbols, tempo …): the text font has none, so they are drawn with
/// the music font — its glyph, width and how far above the baseline it sits (in ems).
pub fn text_accidental(c: char) -> Option<(char, f32, f32)> {
    Some(match c {
        '♯' => ('\u{E262}', 0.29, 0.33),
        '♭' => ('\u{E260}', 0.27, 0.12),
        '♮' => ('\u{E261}', 0.21, 0.33),
        _ => return None,
    })
}

pub fn text_width(s: &str, size: f32) -> f32 {
    let em: f32 = s
        .chars()
        .map(|c| {
            if let Some((_, w, _)) = text_accidental(c) {
                return w;
            }
            u32::from(c)
                .checked_sub(crate::serif::FIRST)
                .and_then(|i| crate::serif::WIDTHS.get(i as usize))
                .filter(|w| **w > 0)
                .map_or(0.52, |w| f32::from(*w) / 1000.0)
        })
        .sum();
    em * size
}

/// Whether a staff is drawn: hidden staves are left out (unless every staff is hidden).
fn shown(score: &Score, part: usize, staff: usize) -> bool {
    let any = score.parts.iter().any(|p| p.visible());
    !any || score.parts.get(part).is_some_and(|p| p.staff_visible(staff))
}

/// Number of drawn staves of a part.
fn shown_staves(score: &Score, part: usize) -> usize {
    score.parts.get(part).map_or(0, |p| {
        (0..p.staff_count()).filter(|s| shown(score, part, *s)).count()
    })
}

/// Number of parts with at least one drawn staff.
fn shown_parts(score: &Score) -> usize {
    (0..score.parts.len())
        .filter(|pi| shown_staves(score, *pi) > 0)
        .count()
}

/// The name written before a part's staves: full or short by the score's settings, none for
/// the only instrument unless it asks for one.
fn part_label(score: &Score, p: &scoreart_model::Part, first: bool) -> Option<String> {
    if shown_parts(score) < 2 && !(first && score.page.single_name) {
        return None;
    }
    let vis = if first {
        score.page.first_names
    } else {
        score.page.later_names
    };
    let name = match vis {
        0 => &p.name,
        1 => &p.abbreviation,
        _ => return None,
    };
    (!name.trim().is_empty()).then(|| name.clone())
}

fn name_indent(score: &Score, first: bool) -> f32 {
    let w = score
        .parts
        .iter()
        .enumerate()
        .filter(|(pi, _)| shown_staves(score, *pi) > 0)
        .map(|(_, p)| p)
        .map(|p| {
            part_label(score, p, first).map_or(0.0, |name| {
                text_width(&name, Fonts::of(score).size(TextKind::InstrumentName, NAME_SIZE))
            })
        })
        .fold(0.0f32, f32::max);
    let multi_staff = (0..score.parts.len()).any(|pi| shown_staves(score, pi) > 1) || shown_parts(score) > 1;
    let bracket = if multi_staff { 1.6 } else { 0.0 };
    if w > 0.0 { w + 1.5 + bracket } else { bracket }
}

/// Every measure in one system at natural spacing (continuous view).
fn one_system(score: &Score, plans: &[MeasurePlan]) -> System {
    System {
        measures: (0..plans.len()).collect(),
        stretch: 1.0,
        indent: name_indent(score, true),
        header: header_width(score, 0),
        ragged: None,
        long_names: true,
    }
}

/// The width a system takes at its stretch.
fn natural_width(sys: &System, plans: &[MeasurePlan]) -> f32 {
    let body: f32 = sys
        .measures
        .iter()
        .enumerate()
        .filter_map(|(i, m)| {
            plans
                .get(*m)
                .map(|p| p.width_at(sys.stretch) + if i == 0 { 0.0 } else { p.prefix })
        })
        .sum();
    sys.indent + sys.header + body + 0.5
}

/// Breaks the measures into systems of `width`; `fill` is the share of the line under which
/// the last system is not stretched, `first_indent` the extra indent of the first system.
/// How far a forced system may squeeze its notes together (share of their natural room).
const SQUEEZE: f32 = 0.55;

fn break_systems(
    score: &Score,
    plans: &[MeasurePlan],
    width: f32,
    fill: f32,
    first_indent: f32,
) -> Vec<System> {
    let mut systems = Vec::new();
    let mut m = 0usize;
    while m < plans.len() {
        // The first system of the score or of a section that asks for long names.
        let first = systems.is_empty() || score.section_starts_with_long_names(m);
        let indent = name_indent(score, first) + if first { first_indent } else { 0.0 };
        let header = header_width(score, m);
        let avail = (width - indent - header).max(10.0);
        let mut ms = Vec::new();
        let mut used = 0.0f32;
        // A line, page or section break ahead (the user's, or the system starts of the
        // program a file comes from): its measures stay one system when they fit with the
        // notes moved closer, down to SQUEEZE of their room, as other programs set them.
        let forced = |i: usize| {
            score
                .measures
                .get(i)
                .is_some_and(|x| x.system_break || x.page_break || x.section.is_some())
        };
        if let Some(end) = (m..plans.len()).take(64).find(|i| forced(*i)) {
            let run = |squeeze: f32| -> f32 {
                (m..=end)
                    .filter_map(|i| plans.get(i).map(|p| (i, p)))
                    .map(|(i, p)| p.width_at(squeeze) + if i == m { 0.0 } else { p.prefix })
                    .sum()
            };
            if run(1.0) > avail && run(SQUEEZE) <= avail {
                ms.extend(m..=end);
                used = run(1.0);
                m = end + 1;
            }
        }
        let squeezed = !ms.is_empty();
        while !squeezed && m < plans.len() {
            let Some(p) = plans.get(m) else { break };
            let w = p.fixed() + p.stretch() + if ms.is_empty() { 0.0 } else { p.prefix };
            // The second half of a measure written as two (hidden barline) always follows.
            let joined = m > 0
                && !ms.is_empty()
                && score
                    .measures
                    .get(m - 1)
                    .is_some_and(|i| i.barline == Barline::Hidden);
            if !ms.is_empty() && used + w > avail && !joined {
                break;
            }
            used += w;
            ms.push(m);
            m += 1;
            // A line or page break set by the user ends the system here.
            if score
                .measures
                .get(m - 1)
                .is_some_and(|i| i.system_break || i.page_break || i.section.is_some())
            {
                break;
            }
        }
        let fixed: f32 = ms
            .iter()
            .enumerate()
            .filter_map(|(i, mi)| {
                plans
                    .get(*mi)
                    .map(|p| p.fixed() + if i == 0 { 0.0 } else { p.prefix })
            })
            .sum();
        let stretchable: f32 = ms
            .iter()
            .filter_map(|mi| plans.get(*mi))
            .map(MeasurePlan::stretch)
            .sum();
        let mut stretch = if stretchable > 0.0 {
            ((avail - fixed) / stretchable).max(0.3)
        } else {
            1.0
        };
        // Squeezed: lyrics keep their room, so the rest gives a little more.
        if stretch < 1.0 {
            let width = |s: f32| -> f32 {
                ms.iter()
                    .enumerate()
                    .filter_map(|(i, mi)| {
                        plans
                            .get(*mi)
                            .map(|p| p.width_at(s) + if i == 0 { 0.0 } else { p.prefix })
                    })
                    .sum()
            };
            let (mut lo, mut hi) = (0.3f32, 1.0f32);
            for _ in 0..24 {
                let mid = (lo + hi) / 2.0;
                if width(mid) > avail {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            stretch = lo;
        }
        let last = m >= plans.len();
        let mut ragged = None;
        if last && used < avail * fill {
            stretch = 1.0;
            let repeats: f32 = ms
                .iter()
                .filter(|mi| score.measures.get(**mi).is_some_and(|i| i.repeat_start))
                .count() as f32
                * 1.6;
            ragged = Some(header + used + repeats + 0.5);
        }
        systems.push(System {
            measures: ms,
            stretch,
            indent,
            header,
            ragged,
            long_names: first,
        });
    }
    systems
}

/// Staff positions are in half spaces below the top line; y in sp.
fn note_y(clef: Clef, p: Pitch) -> f32 {
    (clef.top_line() - p.diatonic()) as f32 * 0.5
}

/// Treble-clef diatonic positions for key signature accidentals.
const SHARP_POS: [i32; 7] = [38, 35, 39, 36, 33, 37, 34];
const FLAT_POS: [i32; 7] = [34, 37, 33, 36, 32, 35, 31];

fn key_positions(clef: Clef, sharps: bool) -> [f32; 7] {
    let shift = match clef {
        Clef::Treble | Clef::Treble8vb | Clef::Percussion => 0,
        Clef::Bass => -14,
        Clef::Alto => -7,
        Clef::Tenor => -7,
    };
    let base = if sharps { SHARP_POS } else { FLAT_POS };
    let top = match clef {
        Clef::Treble8vb => Clef::Treble.top_line(),
        c => c.top_line(),
    };
    let mut out = [0.0; 7];
    for (o, d) in out.iter_mut().zip(base) {
        let mut y = (top - (d + shift)) as f32 * 0.5;
        // Tenor clef sharps would sit above the staff: drop them an octave.
        if clef == Clef::Tenor && sharps && y < -0.5 {
            y += 3.5;
        }
        *o = y;
    }
    out
}

/// The score's text styles, applied to every text that is not a note.
#[derive(Clone, Copy)]
struct Fonts<'a> {
    styles: &'a TextStyles,
    offsets: &'a scoreart_model::text::BlockOffsets,
    blocks: &'a [scoreart_model::TextBlock],
    /// Staff space in mm (text sizes are points, independent of the staff size).
    space: f32,
    /// Chord symbols with German note names.
    german: bool,
}

impl<'a> Fonts<'a> {
    fn of(score: &'a Score) -> Fonts<'a> {
        Fonts {
            styles: &score.text_styles,
            offsets: &score.block_offsets,
            blocks: &score.blocks,
            space: score.page.clamped().staff_space_mm,
            german: score.page.german_chords,
        }
    }

    /// The size in sp of a text of `kind` that ScoreArt draws `nominal` sp big by default.
    fn size(&self, kind: TextKind, nominal: f32) -> f32 {
        let st = self.styles.get(kind);
        let base = kind.reference_pt() * 0.3528 / 1.75;
        (nominal * st.size_sp(self.space) / base).clamp(0.3, 60.0)
    }

    fn shape(&self, kind: TextKind, text: &str, x: f32, y: f32, nominal: f32, align: Align) -> Shape {
        let st = self.styles.get(kind);
        Shape::Text {
            text: text.to_string(),
            x,
            y,
            size: self.size(kind, nominal),
            align,
            bold: st.bold,
            italic: st.italic,
            serif: st.font == scoreart_model::TextFont::Serif,
            kind: Some(kind),
        }
    }
}

struct Out<'a> {
    page: &'a mut Page,
    page_index: usize,
    hits: &'a mut Vec<Hit>,
    f: Fonts<'a>,
}

impl Out<'_> {
    fn push(&mut self, shape: Shape, owner: Option<EventRef>) {
        self.page.items.push(Item { shape, owner });
    }
    fn glyph(&mut self, c: char, x: f32, y: f32, owner: Option<EventRef>) {
        self.push(Shape::Glyph { c, x, y, scale: 1.0 }, owner);
    }
    fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, owner: Option<EventRef>) {
        self.push(
            Shape::Line {
                x1,
                y1,
                x2,
                y2,
                width,
            },
            owner,
        );
    }
    fn text(&mut self, text: &str, x: f32, y: f32, size: f32, align: Align, bold: bool, italic: bool) {
        if text.is_empty() {
            return;
        }
        self.push(
            Shape::Text {
                text: text.to_string(),
                x,
                y,
                size,
                align,
                bold,
                italic,
                serif: false,
                kind: None,
            },
            None,
        );
    }
    /// A styled text; returns its size in sp.
    fn styled(&mut self, kind: TextKind, text: &str, x: f32, y: f32, nominal: f32, align: Align) -> f32 {
        let sh = self.f.shape(kind, text, x, y, nominal, align);
        let size = match &sh {
            Shape::Text { size, .. } => *size,
            _ => nominal,
        };
        if !text.is_empty() {
            self.push(sh, None);
        }
        size
    }
    fn hit(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, target: Target) {
        self.hits.push(Hit {
            page: self.page_index,
            x0,
            y0,
            x1,
            y1,
            target,
        });
    }
}

fn draw_clef(o: &mut Out, x: f32, top: f32, clef: Clef, scale: f32) -> f32 {
    let (c, w, y) = g::clef(clef);
    o.push(
        Shape::Glyph {
            c,
            x,
            y: top + y,
            scale,
        },
        None,
    );
    w * scale
}

fn draw_key(o: &mut Out, x: f32, top: f32, clef: Clef, key: KeySig, prev: Option<KeySig>) -> f32 {
    if clef == Clef::Percussion {
        return 0.0;
    }
    let mut x0 = x;
    // Cancel accidentals of the previous key that the new one drops.
    if let Some(p) = prev
        && (p.fifths.signum() != key.fifths.signum() || p.fifths.unsigned_abs() > key.fifths.unsigned_abs())
    {
        let sharps = p.fifths > 0;
        let pos = key_positions(clef, sharps);
        let keep = if p.fifths.signum() == key.fifths.signum() {
            key.fifths.unsigned_abs()
        } else {
            0
        };
        for (i, y) in pos.iter().enumerate().take(p.fifths.unsigned_abs() as usize) {
            if (i as u8) < keep {
                continue;
            }
            o.glyph(g::NATURAL, x0, top + y, None);
            x0 += 0.85;
        }
        if x0 > x {
            x0 += 0.4;
        }
    }
    let sharps = key.fifths > 0;
    let pos = key_positions(clef, sharps);
    let (c, w) = if sharps {
        (g::SHARP, 0.996)
    } else {
        (g::FLAT, 0.904)
    };
    for y in pos.iter().take(key.fifths.unsigned_abs() as usize) {
        o.glyph(c, x0, top + y, None);
        x0 += w + 0.05;
    }
    x0 - x
}

fn draw_time(o: &mut Out, x: f32, top: f32, t: TimeSig) -> f32 {
    match t.symbol {
        TimeSymbol::Common => {
            o.glyph(g::TIME_COMMON, x, top + 2.0, None);
            1.7
        }
        TimeSymbol::Cut => {
            o.glyph(g::TIME_CUT, x, top + 2.0, None);
            1.7
        }
        TimeSymbol::Normal => {
            let a = t.beats.to_string();
            let b = t.beat_type.to_string();
            let w = a.len().max(b.len()) as f32 * g::TIME_DIGIT_W;
            for (s, y) in [(a, 1.0), (b, 3.0)] {
                let sw = s.len() as f32 * g::TIME_DIGIT_W;
                let mut cx = x + (w - sw) / 2.0;
                for ch in s.chars() {
                    o.glyph(
                        g::digit(ch.to_digit(10).unwrap_or(0), g::TIME_SIG_0),
                        cx,
                        top + y,
                        None,
                    );
                    cx += g::TIME_DIGIT_W;
                }
            }
            w
        }
    }
}

/// Geometry of a drawn chord or rest, kept for ties and beams.
#[derive(Clone, Debug)]
struct EvGeo {
    ev: EventRef,
    onset: u32,
    x: f32,
    value: NoteValue,
    rest: bool,
    /// (pitch, y, head x) for each note, lowest pitch first.
    heads: Vec<(Pitch, f32, f32)>,
    up: bool,
    stem_x: f32,
    stem_end: f32,
    beam_group: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
struct TieEnd {
    page: usize,
    system: usize,
    x_left: f32,
    x_right: f32,
    y: f32,
    up: bool,
}

struct Ctx<'a> {
    score: &'a Score,
    acc: &'a HashMap<NoteKey, Accidental>,
    /// Tie anchors per (event, pitch).
    heads: HashMap<(EventRef, Pitch), TieEnd>,
    events: HashMap<EventRef, EvAnchor>,
    system_ends: Vec<(usize, f32, f32)>,
}

/// One system placed on a page (before drawing): its page, top, staff tops and height, and
/// the headings above it with their tops.
struct Placed {
    si: usize,
    page: usize,
    y: f32,
    tops: Vec<(usize, usize, f32)>,
    sys_h: f32,
    blocks: Vec<(usize, f32)>,
    /// Below each staff: whether a fixed spacer sets the distance (the last: to the next system),
    /// which filling the page leaves as it is.
    fixed: Vec<bool>,
}

/// The staves of a system one below the other: top of every staff relative to the system top,
/// the system's height, and its spacers towards the systems around it: the least distance
/// above its first staff, and the least (or, fixed, the exact) distance below its last staff.
struct Stack {
    tops: Vec<(usize, usize, f32)>,
    sys_h: f32,
    above: Option<f32>,
    below: Option<(f32, bool)>,
    /// Below each staff: whether a fixed spacer sets the distance.
    fixed: Vec<bool>,
    /// How far the system's content reaches above its first staff and below its last one.
    top_reach: f32,
    bottom_reach: f32,
}

/// Size of a small staff against a normal one (MuseScore's small staves).
const SMALL_MAG: f32 = 0.7;

/// Size factor of a staff: smaller for staves set small.
fn staff_mag(score: &Score, part: usize, staff: usize) -> f32 {
    if score.parts.get(part).is_some_and(|p| p.staff_small(staff)) {
        SMALL_MAG
    } else {
        1.0
    }
}

/// Height of a staff from its top line to its bottom line.
fn staff_h(score: &Score, part: usize, staff: usize) -> f32 {
    4.0 * staff_mag(score, part, staff)
}

/// Where a point drawn for a full-size staff with its top line at `top` lands on a staff of
/// size `mag`: within 3 spaces of the staff all shrinks towards the top line; beyond, things
/// move by what the staff lost (texts and lyrics keep their size).
fn small_y(y: f32, top: f32, mag: f32) -> f32 {
    let (lo, hi) = (top - 3.0, top + 7.0);
    if y < lo {
        y + 3.0 * (1.0 - mag)
    } else if y > hi {
        y - 7.0 * (1.0 - mag)
    } else {
        top + (y - top) * mag
    }
}

/// A reach above or below a staff (in spaces) on a staff of size `mag` (see [`small_y`]):
/// near the staff it shrinks, farther out it loses what the 3 spaces near the staff lost.
fn small_reach(r: f32, mag: f32) -> f32 {
    if r > 3.0 { r - 3.0 * (1.0 - mag) } else { r * mag }
}

/// Whether a staff has nothing but rests (and no lyrics or texts) in these measures.
fn staff_empty(score: &Score, pi: usize, s: usize, measures: &[usize]) -> bool {
    measures.iter().all(|m| {
        score.staff_measure(pi, s, *m).is_none_or(|sm| {
            sm.voices
                .iter()
                .flat_map(|v| v.events.iter())
                .all(|e| matches!(e.kind, scoreart_model::EventKind::Rest))
        })
    })
}

fn stack_staves(score: &Score, style: &Style, sys: &System) -> Stack {
    let mut sy = 0.0f32;
    let first_in_sys = sys.measures.first().copied();
    let has_above = sys.measures.iter().any(|m| {
        score.measures.get(*m).is_some_and(|i| {
            // A rehearsal mark starting the system sits beside the clef; one moved below
            // the staff needs no row above it.
            // A mark placed by hand stands where it was put, not in the row.
            let rehearsal_row =
                i.rehearsal.is_some() && Some(*m) != first_in_sys && i.rehearsal_place.is_none();
            i.tempo.is_some()
                || i.text.is_some()
                || i.tempo_text.is_some()
                || rehearsal_row
                || i.jumps.iter().any(|j| {
                    if j.at_end() {
                        i.jump_place.is_none()
                    } else {
                        i.marker_place.is_none()
                    }
                })
        })
    });
    if has_above {
        // The text row (tempo, rehearsal marks, jumps): its text above what is above the staff.
        sy += 3.0;
    }
    if sys
        .measures
        .iter()
        .any(|m| score.measures.get(*m).is_some_and(|i| i.volta.is_some()))
    {
        sy += 2.5;
    }
    // Every drawn staff: (part, staff, reach up, reach down, lyrics height, gap after it,
    // spacers up, down and fixed in this system).
    struct St {
        pi: usize,
        s: usize,
        /// Height of the staff (smaller for a small staff).
        h: f32,
        up: f32,
        down: f32,
        /// How far drawn things reach above the top line and below the bottom line.
        reach_up: f32,
        reach_down: f32,
        lyrics: f32,
        gap: f32,
        sp_up: Option<f32>,
        sp_down: Option<f32>,
        sp_fixed: Option<f32>,
    }
    let mut staves: Vec<St> = Vec::new();
    // Staves set to hide when empty are left out of page systems in which they have no notes
    // (unless that would leave the system with no staff at all).
    let page = style.view == View::Page;
    let kept = |pi: usize, s: usize| {
        !page
            || !score.parts.get(pi).is_some_and(|p| p.staff_hides_empty(s))
            || !staff_empty(score, pi, s, &sys.measures)
    };
    let any_kept = score
        .parts
        .iter()
        .enumerate()
        .any(|(pi, part)| (0..part.staff_count()).any(|s| shown(score, pi, s) && kept(pi, s)));
    // When every staff would go, the top one stays.
    let top_staff = score.parts.iter().enumerate().find_map(|(pi, part)| {
        (0..part.staff_count())
            .find(|s| shown(score, pi, *s))
            .map(|s| (pi, s))
    });
    for (pi, part) in score.parts.iter().enumerate() {
        let visible: Vec<usize> = (0..part.staff_count())
            .filter(|s| {
                shown(score, pi, *s)
                    && if any_kept {
                        kept(pi, *s)
                    } else {
                        top_staff == Some((pi, *s))
                    }
            })
            .collect();
        for &s in &visible {
            // Make room for notes (with stems and marks) far above or below the staff.
            let (reach_up, reach_down) = staff_reach(score, pi, s, &sys.measures);
            let sms: Vec<&scoreart_model::StaffMeasure> = sys
                .measures
                .iter()
                .filter_map(|m| score.staff_measure(pi, s, *m))
                .collect();
            // One lyric line per verse used in this system.
            let verses = sms
                .iter()
                .flat_map(|sm| sm.voices.iter())
                .flat_map(|v| v.events.iter())
                .map(|e| e.verse_count())
                .max()
                .unwrap_or(0);
            let lyrics = if verses > 0 {
                2.6 + lyric_line(&Fonts::of(score)) * (verses.saturating_sub(1)) as f32
            } else {
                0.0
            };
            let most = |f: fn(&scoreart_model::StaffMeasure) -> Option<f32>| {
                sms.iter()
                    .filter_map(|sm| f(sm))
                    .fold(None, |a: Option<f32>, v| Some(a.map_or(v, |a| a.max(v))))
            };
            let lyric_reach = if verses > 0 {
                let size = Fonts::of(score).size(TextKind::Lyrics, LYRIC_SIZE);
                lyric_base(score, pi, s, &sys.measures)
                    + lyric_line(&Fonts::of(score)) * (verses.saturating_sub(1)) as f32
                    + 0.3 * size
            } else {
                0.0
            };
            // A small staff: everything near it shrinks (lyrics move up with its bottom line).
            let mag = staff_mag(score, pi, s);
            let (reach_up, reach_down, lyric_reach) = if mag < 1.0 {
                (
                    small_reach(reach_up, mag),
                    small_reach(reach_down, mag),
                    if lyric_reach > 0.0 {
                        lyric_reach - 3.0 * (1.0 - mag)
                    } else {
                        0.0
                    },
                )
            } else {
                (reach_up, reach_down, lyric_reach)
            };
            staves.push(St {
                pi,
                s,
                h: 4.0 * mag,
                up: (reach_up - 3.0).max(0.0),
                down: (reach_down - 3.0).max(0.0),
                reach_up,
                reach_down: reach_down.max(lyric_reach),
                lyrics,
                gap: if visible.last().is_some_and(|l| s < *l) {
                    style.staff_gap
                } else {
                    style.part_gap
                },
                sp_up: most(|sm| sm.spacer_up),
                sp_down: most(|sm| sm.spacer_down),
                sp_fixed: most(|sm| sm.spacer_fixed),
            });
        }
    }
    let mut tops: Vec<(usize, usize, f32)> = Vec::with_capacity(staves.len());
    for (k, st) in staves.iter().enumerate() {
        let top = match k.checked_sub(1).and_then(|p| staves.get(p)) {
            None => sy + st.up,
            Some(prev) => {
                let prev_bottom = tops.last().map_or(0.0, |t| t.2) + prev.h;
                // Bottom line of the staff above to the top line of this one: what the notes
                // need, at least what a spacer asks, or exactly what a fixed spacer says.
                let natural = prev.down + prev.lyrics + prev.gap + st.up;
                let d = match prev.sp_fixed {
                    Some(f) => f,
                    None => natural
                        .max(prev.sp_down.unwrap_or(0.0))
                        .max(st.sp_up.unwrap_or(0.0)),
                };
                prev_bottom + d
            }
        };
        tops.push((st.pi, st.s, top));
    }
    let sys_h = match (tops.last(), staves.last()) {
        (Some(t), Some(st)) => t.2 + st.h + st.down + st.lyrics,
        _ => 4.0,
    }
    .max(4.0);
    let fixed = staves.iter().map(|st| st.sp_fixed.is_some()).collect();
    // What stands above the first staff: its notes and chord symbols, the text row, and marks
    // placed by hand (their text rises about 2.5 spaces over the baseline).
    let placed_marks = sys
        .measures
        .iter()
        .filter_map(|m| score.measures.get(*m))
        .flat_map(|i| {
            [
                i.rehearsal.is_some().then_some(i.rehearsal_place).flatten(),
                i.jumps
                    .iter()
                    .any(|j| !j.at_end())
                    .then_some(i.marker_place)
                    .flatten(),
                i.jumps
                    .iter()
                    .any(|j| j.at_end())
                    .then_some(i.jump_place)
                    .flatten(),
            ]
        })
        .flatten()
        .filter(|p| !p.below)
        .map(|p| -p.dy_sp() + 2.5)
        .fold(0.0f32, f32::max);
    let top_reach = staves
        .first()
        .map_or(0.0, |st| st.reach_up.max(0.0) + if has_above { 3.0 } else { 0.0 })
        .max(placed_marks);
    let bottom_reach = staves.last().map_or(0.0, |st| st.reach_down.max(0.0));
    let above = staves.first().and_then(|st| st.sp_up);
    let below = staves
        .last()
        .and_then(|st| st.sp_fixed.map(|f| (f, true)).or(st.sp_down.map(|d| (d, false))));
    Stack {
        tops,
        sys_h,
        above,
        below,
        fixed,
        top_reach,
        bottom_reach,
    }
}

pub fn layout(score: &Score, style: &Style) -> Layout {
    let (mut pw, mut ph) = style.page_sp();
    let first_number = style.page.first_page_number;
    let fit = |m: [f32; 4], pw: f32, ph: f32| m.map(|v| v.min(pw / 4.0).min(ph / 4.0));
    // Margins by page index: top, bottom, left, right.
    let margins = |page: usize, pw: f32, ph: f32| {
        fit(style.margins_of(first_number.saturating_add(page as i32)), pw, ph)
    };
    let [mt, mb, ml, mr] = margins(0, pw, ph);
    // Systems are as wide as the narrower of the odd and even pages allows.
    let [_, _, el, er] = margins(1, pw, ph);
    let mut width = (pw - ml - mr).min(pw - el - er);
    let acc = accidentals(score);
    let plans = plan_measures(score, &acc, style);
    let systems = if style.view == View::Horizontal {
        // One system at natural spacing; the page grows to fit it.
        let sys = one_system(score, &plans);
        width = natural_width(&sys, &plans);
        pw = ml + width + mr;
        vec![sys]
    } else {
        break_systems(
            score,
            &plans,
            width,
            style.page.last_system_fill,
            style.page.first_indent,
        )
    };
    let endless = style.view != View::Page;

    let fonts = Fonts::of(score);
    let mut out = Layout::default();
    let mut ctx = Ctx {
        score,
        acc: &acc,
        heads: HashMap::new(),
        events: HashMap::new(),
        system_ends: Vec::new(),
    };
    let mut page = Page {
        width: pw,
        height: ph,
        items: Vec::new(),
    };

    // Title block.
    let mut y = mt;
    let meta = &score.meta;
    let mut titled = false;
    if style.view != View::Horizontal {
        let mut o = Out {
            page: &mut page,
            page_index: 0,
            hits: &mut out.hits,
            f: fonts,
        };
        let frame = Frame { ml, mr, pw };
        let fixed = |k: TextKind| BlockRef::Fixed(k);
        if !meta.title.is_empty() {
            y += draw_block(
                &mut o,
                &frame,
                TextKind::Title,
                &meta.title,
                y,
                fixed(TextKind::Title),
            ) + 0.6;
        }
        if !meta.subtitle.is_empty() {
            y += draw_block(
                &mut o,
                &frame,
                TextKind::Subtitle,
                &meta.subtitle,
                y,
                fixed(TextKind::Subtitle),
            ) + 0.4;
        }
        if !meta.composer.is_empty() || !meta.lyricist.is_empty() {
            let c = draw_block(
                &mut o,
                &frame,
                TextKind::Composer,
                &meta.composer,
                y,
                fixed(TextKind::Composer),
            );
            let l = draw_block(
                &mut o,
                &frame,
                TextKind::Lyricist,
                &meta.lyricist,
                y,
                fixed(TextKind::Lyricist),
            );
            // Keep rehearsal marks and tempo above the first system clear of this line.
            y += c.max(l) + 1.0;
        }
        if y > mt {
            y += 3.0;
            titled = true;
        }
    }

    // Pass 1: every system (with the headings above it) placed on a page.
    let mut placed: Vec<Placed> = Vec::with_capacity(systems.len());
    let mut page_index = 0usize;
    // Where the music may start on a page: below its header, and the first staff at least the
    // music top margin below the top margin.
    let page_top = |page: usize, above0: f32| -> f32 {
        let [mt, ..] = margins(page, pw, ph);
        let info = info_for(score, style, page, usize::MAX);
        let header = header_height(&fonts, score, &info, false);
        let header = if header > 0.0 {
            header + style.page.header_offset.max(0.0) + 1.0
        } else {
            0.0
        };
        (mt + style.page.music_top - above0).max(mt + header)
    };
    // The last staff line of the previous system and its spacer below (same page only).
    let mut prev_staff_end: Option<(usize, f32, Option<(f32, bool)>)> = None;
    let mut prev_reach = 0.0f32;
    for (si, sys) in systems.iter().enumerate() {
        let Stack {
            tops,
            sys_h,
            above: spacer_above,
            below: spacer_below,
            fixed,
            top_reach,
            bottom_reach,
        } = stack_staves(score, style, sys);
        let above0 = tops.first().map_or(0.0, |t| t.2);
        let last_top = tops.last().map_or(0.0, |t| t.2);
        let [_, mbp, mlp, mrp] = margins(page_index, pw, ph);
        let frame = Frame { ml: mlp, mr: mrp, pw };
        let above: Vec<(usize, &scoreart_model::TextBlock)> = if style.view == View::Horizontal {
            Vec::new()
        } else {
            score
                .blocks
                .iter()
                .enumerate()
                .filter(|(_, b)| matches!(b.kind, TextKind::Heading | TextKind::Text))
                .filter(|(_, b)| {
                    sys.measures.contains(&b.measure)
                        || (si == 0 && sys.measures.first().is_some_and(|f| b.measure < *f))
                })
                .collect()
        };
        let blocks_h: f32 = above
            .iter()
            .map(|(_, b)| block_height(&fonts, &frame, b.kind, &b.text) + BLOCK_GAP)
            .sum();
        if si == 0 {
            // The first staff keeps the music top margin (below the titles, when any).
            y = if titled {
                y
            } else {
                y.max(page_top(0, above0) - blocks_h)
            };
        }
        // A page break set by the user after the previous system's last measure.
        let forced = si > 0
            && systems
                .get(si - 1)
                .and_then(|p| p.measures.last())
                .and_then(|m| score.measures.get(*m))
                .is_some_and(|i| i.page_break);
        // Spacers between this system and the one before: at least the room they ask for, or
        // exactly that for a fixed spacer below.
        if let Some((pg, line, _)) = prev_staff_end
            && pg == page_index
        {
            // The least system distance counts from the bottom line of one system to the top
            // line of the next; more only where what hangs below the one and what stands
            // above the other would come closer than SYSTEM_CLEAR.
            let d = style.system_gap.max(prev_reach + SYSTEM_CLEAR + top_reach);
            y = line + d - above0 - blocks_h;
        }
        if let Some((pg, line, below)) = prev_staff_end
            && pg == page_index
        {
            let (need, exact) = match below {
                Some((f, true)) => (Some(f), true),
                Some((d, false)) => (Some(d.max(spacer_above.unwrap_or(0.0))), false),
                None => (spacer_above, false),
            };
            if let Some(d) = need {
                let at = line + d - above0 - blocks_h;
                y = if exact { at } else { y.max(at) };
            }
        }
        // The system fits when its last staff keeps the music bottom margin and nothing of it
        // runs into the bottom margin.
        let bottom = ph - mbp;
        let last_h = tops.last().map_or(4.0, |t| staff_h(score, t.0, t.1));
        let fits = y + blocks_h + last_top + last_h <= bottom - style.page.music_bottom
            && y + blocks_h + sys_h <= bottom;
        if !endless && (!fits || forced) && si > 0 {
            page_index += 1;
            y = page_top(page_index, above0) - blocks_h;
        }
        let [_, _, mlp, mrp] = margins(page_index, pw, ph);
        let frame = Frame { ml: mlp, mr: mrp, pw };
        let mut blocks = Vec::new();
        for (i, b) in &above {
            blocks.push((*i, y));
            y += block_height(&fonts, &frame, b.kind, &b.text) + BLOCK_GAP;
        }
        placed.push(Placed {
            si,
            page: page_index,
            y,
            tops,
            sys_h,
            blocks,
            fixed,
        });
        prev_staff_end = Some((page_index, y + last_top + last_h, spacer_below));
        prev_reach = bottom_reach;
        y += sys_h + SYSTEM_CLEAR;
        // An empty frame after the system keeps its room: the next staff starts the frame's
        // height and the distances around a frame below this system's last staff.
        if !endless
            && let Some(h) = sys
                .measures
                .last()
                .and_then(|m| score.measures.get(*m))
                .and_then(|i| i.frame_after)
            && let Some((pg, line, below)) = prev_staff_end
        {
            prev_staff_end = Some((pg, line + h + 2.0 * FRAME_GAP - style.system_gap, below));
            y = y.max(line + h + 2.0 * FRAME_GAP - style.system_gap);
        }
    }

    // Pass 2: pages filled down to the music bottom margin (not the last page): systems move
    // apart up to the largest system distance; with vertical justification the staves inside
    // the systems spread too.
    if !endless {
        let last_page = placed.last().map_or(0, |p| p.page);
        for pg in 0..=last_page {
            let on: Vec<usize> = (0..placed.len()).filter(|k| placed[*k].page == pg).collect();
            let Some(&last) = on.last() else { continue };
            let [mtp, mbp, ..] = margins(pg, pw, ph);
            let p = &placed[last];
            // The last page is filled only when its music already takes most of it.
            if pg == last_page {
                let first_y = on.first().map_or(mtp, |k| placed[*k].y);
                let used = p.y + p.sys_h - first_y;
                if !style.page.vertical_justify || used < LAST_PAGE_FILL * (ph - mbp - first_y) {
                    continue;
                }
            }
            let last_staff = p.y + p.tops.last().map_or(4.0, |t| t.2 + staff_h(score, t.0, t.1));
            // An empty frame after the page's last system fills its part of the page.
            let frame = systems
                .get(p.si)
                .and_then(|sy| sy.measures.last())
                .and_then(|m| score.measures.get(*m))
                .and_then(|i| i.frame_after)
                .map_or(0.0, |h| h + 2.0 * FRAME_GAP);
            let room = (ph - mbp - style.page.music_bottom - last_staff - frame)
                .min(ph - mbp - (p.y + p.sys_h) - frame)
                .max(0.0);
            // A forced page break leaves the rest of its page as it is.
            let forced = score
                .measures
                .get(systems[p.si].measures.last().copied().unwrap_or(0))
                .is_some_and(|i| i.page_break);
            let gaps = on.len().saturating_sub(1);
            if room <= 0.0 || forced {
                continue;
            }
            // Gaps a fixed spacer sets stay as they are: between systems (after the system
            // with the fixed spacer below its last staff) and between staves.
            let after_fixed: Vec<bool> = on
                .iter()
                .map(|k| placed[*k].fixed.last().copied().unwrap_or(false))
                .collect();
            let free_gaps = after_fixed.iter().take(gaps).filter(|x| !**x).count();
            if style.page.vertical_justify {
                let inner: usize = on
                    .iter()
                    .map(|k| {
                        let f = &placed[*k].fixed;
                        f.iter().take(f.len().saturating_sub(1)).filter(|x| !**x).count()
                    })
                    .sum();
                let weight = free_gaps as f32 * style.page.spread_factor + inner as f32;
                if weight <= 0.0 {
                    continue;
                }
                let unit = room / weight;
                let sys_extra = (unit * style.page.spread_factor)
                    .min((style.page.max_system_spread - style.system_gap).max(0.0));
                let staff_extra =
                    unit.min((style.page.max_staff_spread - style.staff_gap.min(style.part_gap)).max(0.0));
                let mut shift = 0.0;
                for (n, k) in on.iter().enumerate() {
                    if n > 0 && !after_fixed.get(n - 1).copied().unwrap_or(false) {
                        shift += sys_extra;
                    }
                    let p = &mut placed[*k];
                    for (_, by) in &mut p.blocks {
                        *by += shift;
                    }
                    p.y += shift;
                    let mut grow = 0.0;
                    for j in 0..p.tops.len() {
                        if j > 0 && !p.fixed.get(j - 1).copied().unwrap_or(false) {
                            grow += staff_extra;
                        }
                        if let Some(t) = p.tops.get_mut(j) {
                            t.2 += grow;
                        }
                    }
                    p.sys_h += grow;
                    shift += grow;
                }
            } else if free_gaps > 0 {
                let extra =
                    (room / free_gaps as f32).min((style.page.max_system_gap - style.system_gap).max(0.0));
                let mut shift = 0.0;
                for (n, k) in on.iter().enumerate() {
                    if n > 0 && !after_fixed.get(n - 1).copied().unwrap_or(false) {
                        shift += extra;
                    }
                    let p = &mut placed[*k];
                    p.y += shift;
                    for (_, by) in &mut p.blocks {
                        *by += shift;
                    }
                }
            }
        }
    }

    // Pass 3: drawing, page by page.
    let mut page_index = 0usize;
    for p in &placed {
        if p.page != page_index {
            out.pages.push(std::mem::take(&mut page));
            page = Page {
                width: pw,
                height: ph,
                items: Vec::new(),
            };
            page_index = p.page;
        }
        let [_, _, mlp, mrp] = margins(page_index, pw, ph);
        let frame = Frame { ml: mlp, mr: mrp, pw };
        if !p.blocks.is_empty() {
            let mut o = Out {
                page: &mut page,
                page_index,
                hits: &mut out.hits,
                f: fonts,
            };
            for (i, by) in &p.blocks {
                if let Some(b) = score.blocks.get(*i) {
                    draw_block(&mut o, &frame, b.kind, &b.text, *by, BlockRef::Added(*i));
                }
            }
        }
        let Some(sys) = systems.get(p.si) else { continue };
        draw_system(
            &mut ctx, &mut page, page_index, &mut out, sys, p.si, &plans, mlp, width, p.y, &p.tops, p.sys_h,
        );
        y = p.y + p.sys_h + style.system_gap;
    }
    // Breaks and spacers, for the editor.
    let mut marks = Vec::new();
    let mut last_of: HashMap<usize, (usize, f32, f32)> = HashMap::new();
    for b in &out.staves {
        let e = last_of.entry(b.measure).or_insert((b.page, b.x1, b.top));
        e.2 = e.2.min(b.top);
        let Some(sm) = score.staff_measure(b.part, b.staff, b.measure) else {
            continue;
        };
        let mark = |kind, y: f32| Mark {
            page: b.page,
            x: b.x0 + 1.0,
            y,
            kind,
            measure: b.measure,
            part: b.part,
            staff: b.staff,
        };
        if let Some(h) = sm.spacer_up {
            marks.push(mark(MarkKind::SpacerUp(h), b.top));
        }
        if let Some(h) = sm.spacer_down {
            marks.push(mark(MarkKind::SpacerDown(h), b.top + 4.0));
        }
        if let Some(h) = sm.spacer_fixed {
            marks.push(mark(MarkKind::SpacerFixed(h), b.top + 4.0));
        }
    }
    for (m, info) in score.measures.iter().enumerate() {
        let kind = if info.section.is_some() {
            MarkKind::SectionBreak
        } else if info.page_break {
            MarkKind::PageBreak
        } else if info.system_break {
            MarkKind::LineBreak
        } else {
            continue;
        };
        if let Some(&(page, x1, top)) = last_of.get(&m) {
            marks.push(Mark {
                page,
                x: x1 - 1.2,
                y: top - 2.0,
                kind,
                measure: m,
                part: 0,
                staff: 0,
            });
        }
    }
    out.marks = marks;
    // Text after the last system (a poem at the end).
    if style.view != View::Horizontal {
        let n = score.measures.len();
        for (i, b) in score.blocks.iter().enumerate().filter(|(_, b)| b.measure >= n) {
            let [_, mbp, mlp, mrp] = margins(page_index, pw, ph);
            let frame = Frame { ml: mlp, mr: mrp, pw };
            let h = block_height(&fonts, &frame, b.kind, &b.text);
            if !endless && y + h > ph - mbp && !page.items.is_empty() {
                out.pages.push(std::mem::take(&mut page));
                page = Page {
                    width: pw,
                    height: ph,
                    items: Vec::new(),
                };
                page_index += 1;
                y = margins(page_index, pw, ph)[0] + style.page.music_top;
            }
            let mut o = Out {
                page: &mut page,
                page_index,
                hits: &mut out.hits,
                f: fonts,
            };
            y += draw_block(&mut o, &frame, b.kind, &b.text, y, BlockRef::Added(i)) + BLOCK_GAP;
        }
    }
    if endless {
        // The page ends below the last system.
        ph = (y - style.system_gap + mb + 4.0).max(ph.min(40.0));
        page.width = pw;
        page.height = ph;
    }
    // Ties need both ends placed.
    draw_ties(&ctx, &mut out, &mut page, page_index);
    draw_spanners(&ctx, &mut out, &mut page, page_index);
    out.pages.push(page);
    // Headers and footers, now that the number of pages is known.
    if style.view == View::Page {
        let pages = out.pages.len();
        for (pi, pg) in out.pages.iter_mut().enumerate() {
            let [mt, mb, ml, mr] = margins(pi, pw, ph);
            let info = info_for(score, style, pi, pages);
            let mut o = Out {
                page: pg,
                page_index: pi,
                hits: &mut out.hits,
                f: fonts,
            };
            draw_header_footer(&mut o, score, style, PageBox { mt, mb, ml, mr, ph }, pw, &info);
        }
    }
    out
}

/// Space between a heading or text block and what follows, sp.
const BLOCK_GAP: f32 = 1.2;

/// The horizontal frame of a page: margins and width, sp.
struct Frame {
    ml: f32,
    mr: f32,
    pw: f32,
}

impl Frame {
    fn width(&self) -> f32 {
        (self.pw - self.ml - self.mr).max(4.0)
    }
}

/// The lines of a block's text, broken at newlines and wrapped to the page width.
fn block_lines(text: &str, size: f32, width: f32) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.lines() {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let next = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && text_width(&next, size) > width {
                out.push(std::mem::replace(&mut line, word.to_string()));
            } else {
                line = next;
            }
        }
        out.push(line);
    }
    out
}

fn line_height(size: f32) -> f32 {
    size * 1.25
}

/// Height of a block (its lines) in sp.
/// Size of segno and coda signs placed by hand, against the music font (MuseScore draws
/// them in a text font at text size).
const MARKER_SCALE: f32 = 0.6;

/// How full the last page must be before its systems are spread like the others' (MuseScore
/// spreads a last page that is well filled).
const LAST_PAGE_FILL: f32 = 0.7;

/// Least room between what hangs below one system (lyrics) and what stands above the next
/// (chord symbols), staff spaces.
const SYSTEM_CLEAR: f32 = 0.5;

/// Distance between a system and an empty frame (and the frame and the next system), staff
/// spaces (MuseScore's default).
const FRAME_GAP: f32 = 7.0;

fn block_height(f: &Fonts, frame: &Frame, kind: TextKind, text: &str) -> f32 {
    if text.trim().is_empty() {
        return 0.0;
    }
    let size = f.size(kind, kind.reference_pt() * 0.3528 / 1.75);
    let n = block_lines(text, size, frame.width()).len().max(1) as f32;
    size + (n - 1.0) * line_height(size) + size * 0.25
}

/// Draws a block of the page with its top at `y`, aligned by its style and moved by its
/// offset; returns its height. Its box can be picked up and moved.
fn draw_block(o: &mut Out, frame: &Frame, kind: TextKind, text: &str, y: f32, block: BlockRef) -> f32 {
    let h = block_height(&o.f, frame, kind, text);
    if h <= 0.0 {
        return 0.0;
    }
    let score_offset = match block {
        BlockRef::Fixed(k) => o.f.offsets.get(&k).copied().unwrap_or_default(),
        BlockRef::Added(i) => o.f.blocks.get(i).map_or((0, 0), |b| (b.dx, b.dy)),
    };
    let (dx, dy) = (f32::from(score_offset.0) / 10.0, f32::from(score_offset.1) / 10.0);
    let st = o.f.styles.get(kind);
    // `size` is the drawn size; the shapes take the nominal one and apply the style themselves.
    let nominal = kind.reference_pt() * 0.3528 / 1.75;
    let size = o.f.size(kind, nominal);
    let (x, align) = match st.align {
        scoreart_model::TextAlign::Left => (frame.ml, Align::Left),
        scoreart_model::TextAlign::Center => (frame.pw / 2.0, Align::Center),
        scoreart_model::TextAlign::Right => (frame.pw - frame.mr, Align::Right),
    };
    let (x, top) = (x + dx, y + dy);
    let lines = block_lines(text, size, frame.width());
    let mut by = top + size;
    let (mut x0, mut x1) = (f32::MAX, f32::MIN);
    for l in &lines {
        let sh = o.f.shape(kind, l, x, by, nominal, align);
        let w = text_width(l, size);
        let lx = match align {
            Align::Left => x,
            Align::Center => x - w / 2.0,
            Align::Right => x - w,
        };
        x0 = x0.min(lx);
        x1 = x1.max(lx + w);
        if !l.is_empty() {
            o.push(sh, None);
        }
        by += line_height(size);
    }
    if x1 > x0 {
        o.hit(x0 - 0.3, top - 0.2, x1 + 0.3, top + h, Target::Block { block });
    }
    h
}

/// The vertical frame of one page: margins and the page height, sp.
#[derive(Clone, Copy)]
struct PageBox {
    mt: f32,
    mb: f32,
    ml: f32,
    mr: f32,
    ph: f32,
}

/// The text style of a header or footer field: the page number's or the copyright's when it
/// holds only that, else the header's or footer's.
fn field_kind(field: &str, footer: bool) -> TextKind {
    match field.trim() {
        "$p" | "$P" => TextKind::PageNumber,
        "$C" | "$c" | "$:copyright:" => TextKind::Copyright,
        _ if footer => TextKind::Footer,
        _ => TextKind::Header,
    }
}

fn info_for<'a>(
    score: &'a Score,
    style: &'a Style,
    page_index: usize,
    pages: usize,
) -> scoreart_model::PageInfo<'a> {
    scoreart_model::PageInfo {
        meta: &score.meta,
        doc: &style.doc,
        number: style.page.first_page_number.saturating_add(page_index as i32),
        pages,
        first: page_index == 0,
        page_numbers: !score.hide_page_numbers,
        page_number_first_page: score.header_footer.page_number_first_page,
    }
}

/// The lines of the header (or footer) fields of a page, macros replaced: (field, kind, lines).
fn header_lines(
    score: &Score,
    info: &scoreart_model::PageInfo,
    footer: bool,
) -> Vec<(usize, TextKind, Vec<String>)> {
    let odd = info.number.rem_euclid(2) == 1;
    let Some(fields) = score.header_footer.fields(footer, info.first, odd) else {
        return Vec::new();
    };
    fields
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            let text = scoreart_model::expand_macros(f, info);
            let lines: Vec<String> = text.lines().map(str::to_string).collect();
            (lines.iter().any(|l| !l.trim().is_empty())).then(|| (i, field_kind(f, footer), lines))
        })
        .collect()
}

/// Height of a page's header (or footer), sp; 0 when there is none.
fn header_height(fonts: &Fonts, score: &Score, info: &scoreart_model::PageInfo, footer: bool) -> f32 {
    header_lines(score, info, footer)
        .iter()
        .map(|(_, k, lines)| {
            let size = fonts.size(*k, k.reference_pt() * 0.3528 / 1.75);
            size + (lines.len().max(1) - 1) as f32 * line_height(size) + size * 0.25
        })
        .fold(0.0, f32::max)
}

/// Draws the header and footer of a page: three fields each (left, centre, right) at the top
/// and bottom margin; they can be picked up and moved together.
fn draw_header_footer(
    o: &mut Out,
    score: &Score,
    style: &Style,
    pb: PageBox,
    pw: f32,
    info: &scoreart_model::PageInfo,
) {
    for footer in [false, true] {
        let kind = if footer {
            TextKind::Footer
        } else {
            TextKind::Header
        };
        let (dx, dy) = score
            .block_offsets
            .get(&kind)
            .map_or((0.0, 0.0), |(x, y)| (f32::from(*x) / 10.0, f32::from(*y) / 10.0));
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (i, k, lines) in header_lines(score, info, footer) {
            let nominal = k.reference_pt() * 0.3528 / 1.75;
            let size = o.f.size(k, nominal);
            let (x, align) = match i {
                0 => (pb.ml, Align::Left),
                1 => (pw / 2.0, Align::Center),
                _ => (pw - pb.mr, Align::Right),
            };
            let x = x + dx;
            let h = size + (lines.len().max(1) - 1) as f32 * line_height(size);
            // Header: hanging from the top margin; footer: standing on the bottom margin.
            let top = if footer {
                pb.ph - pb.mb + style.page.footer_offset - h + dy
            } else {
                pb.mt + style.page.header_offset + dy
            };
            let mut by = top + size;
            for l in &lines {
                let w = text_width(l, size);
                let lx = match align {
                    Align::Left => x,
                    Align::Center => x - w / 2.0,
                    Align::Right => x - w,
                };
                x0 = x0.min(lx);
                x1 = x1.max(lx + w);
                if !l.trim().is_empty() {
                    let sh = o.f.shape(k, l, x, by, nominal, align);
                    o.push(sh, None);
                }
                by += line_height(size);
            }
            y0 = y0.min(top - 0.2);
            y1 = y1.max(top + h + 0.3);
        }
        if x1 > x0 {
            o.hit(
                x0 - 0.3,
                y0,
                x1 + 0.3,
                y1,
                Target::Block {
                    block: BlockRef::Fixed(kind),
                },
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_system(
    ctx: &mut Ctx,
    page: &mut Page,
    page_index: usize,
    out: &mut Layout,
    sys: &System,
    si: usize,
    plans: &[MeasurePlan],
    margin: f32,
    width: f32,
    top: f32,
    tops: &[(usize, usize, f32)],
    sys_h: f32,
) {
    let score = ctx.score;
    let x_start = margin + sys.indent;
    let x_end = sys
        .ragged
        .map_or(margin + width, |w| (x_start + w).min(margin + width));
    let first_m = sys.measures.first().copied().unwrap_or(0);
    let mut o = Out {
        page,
        page_index,
        hits: &mut out.hits,
        f: Fonts::of(score),
    };
    // What this system draws starts here (small staves shrink it at the end).
    let (items0, hits0) = (o.page.items.len(), o.hits.len());
    let mut all_cols: Vec<f32> = Vec::new();

    // Staff lines.
    for &(_, _, t) in tops {
        for l in 0..5 {
            let yy = top + t + l as f32;
            o.line(x_start, yy, x_end, yy, g::STAFF_LINE, None);
        }
    }
    let sys_top = tops.first().map(|t| top + t.2).unwrap_or(top);
    let sys_bottom = tops
        .last()
        .map(|t| top + t.2 + staff_h(score, t.0, t.1))
        .unwrap_or(top + 4.0);
    if tops.len() > 1 {
        o.line(x_start, sys_top, x_start, sys_bottom, g::THIN_BAR, None);
    }
    // Brackets for ensembles, braces for multi-staff parts, instrument names.
    let ensemble = shown_parts(score) > 1;
    if ensemble {
        o.push(
            Shape::Rect {
                x: x_start - 1.0,
                y: sys_top - 0.4,
                w: g::THICK_BAR,
                h: sys_bottom - sys_top + 0.8,
            },
            None,
        );
        o.glyph(g::BRACKET_TOP, x_start - 1.0, sys_top - 0.4, None);
        o.glyph(g::BRACKET_BOTTOM, x_start - 1.0, sys_bottom + 0.4, None);
    }
    for (pi, part) in score.parts.iter().enumerate() {
        let staff_tops: Vec<f32> = tops.iter().filter(|t| t.0 == pi).map(|t| top + t.2).collect();
        let (Some(pt), Some(pb)) = (staff_tops.first().copied(), staff_tops.last().map(|t| t + 4.0)) else {
            continue;
        };
        let multi = staff_tops.len() > 1;
        if multi {
            let bx = x_start - if ensemble { 2.6 } else { 1.4 };
            draw_brace(&mut o, bx, pt, pb);
        }
        if let Some(name) = part_label(score, part, sys.long_names) {
            let name = &name;
            let nx = x_start - if multi { 3.0 } else { 1.0 } - if ensemble { 1.4 } else { 0.0 };
            o.styled(
                TextKind::InstrumentName,
                name,
                nx,
                (pt + pb) / 2.0 + 0.7,
                NAME_SIZE,
                Align::Right,
            );
        }
    }

    // Header: clef, key, time.
    let mut hx = x_start + 0.8;
    let key = score.key_at(first_m);
    let show_time = first_m == 0 || score.measures.get(first_m).is_some_and(|i| i.time.is_some());
    let mut clef_w = 0.0f32;
    let mut key_w = 0.0f32;
    for &(pi, s, t) in tops {
        let clef = score.clef_at(pi, s, first_m);
        clef_w = clef_w.max(draw_clef(&mut o, hx, top + t, clef, 1.0));
    }
    hx += clef_w.max(2.7) + 0.6;
    for &(pi, s, t) in tops {
        let clef = score.clef_at(pi, s, first_m);
        key_w = key_w.max(draw_key(&mut o, hx, top + t, clef, key, None));
    }
    if key_w > 0.0 {
        hx += key_w + 0.6;
    }
    if show_time {
        let ts = score.time_at(first_m);
        let mut w = 0.0f32;
        for &(_, _, t) in tops {
            w = w.max(draw_time(&mut o, hx, top + t, ts));
        }
        hx += w + 0.2;
    }
    let content_x = x_start + sys.header.max(hx - x_start);
    ctx.system_ends.push((page_index, x_end, content_x));

    // Measure number at the start of every system but the first (not a section's "1").
    if first_m > 0 && score.measure_number(first_m) > 1 {
        o.styled(
            TextKind::MeasureNumber,
            &score.measure_number(first_m).to_string(),
            x_start,
            sys_top - 2.0,
            1.4,
            Align::Left,
        );
    }

    let mut mx = content_x;
    let mut boxes: Vec<(usize, f32, f32)> = Vec::new();
    // How far the top staff's content reaches up: the text row (tempo, rehearsal, jumps) sits above it.
    let row_reach = tops
        .first()
        .map(|t| staff_reach(score, t.0, t.1, &sys.measures).0)
        .unwrap_or(0.0);
    for (k, &m) in sys.measures.iter().enumerate() {
        let Some(plan) = plans.get(m) else { continue };
        let info = score.measures.get(m).cloned().unwrap_or_default();
        let m_start = mx;
        // Changes at the start of a measure that is not first in the system.
        if k > 0 {
            let clef_change = tops
                .iter()
                .any(|&(pi, s, _)| score.staff_measure(pi, s, m).is_some_and(|sm| sm.clef.is_some()));
            if clef_change {
                for &(pi, s, t) in tops {
                    if let Some(c) = score.staff_measure(pi, s, m).and_then(|sm| sm.clef) {
                        draw_clef(&mut o, mx + 0.3, top + t, c, 0.8);
                    }
                }
                mx += 2.6;
            }
            if let Some(kk) = info.key {
                let prev = score.key_at(m.saturating_sub(1));
                let mut w = 0.0f32;
                for &(pi, s, t) in tops {
                    w = w.max(draw_key(
                        &mut o,
                        mx + 0.4,
                        top + t,
                        score.clef_at(pi, s, m),
                        kk,
                        Some(prev),
                    ));
                }
                mx += w + 0.4;
            }
            if let Some(ts) = info.time {
                let mut w = 0.0f32;
                for &(_, _, t) in tops {
                    w = w.max(draw_time(&mut o, mx + 0.4, top + t, ts));
                }
                mx += w + 0.4;
            }
        }
        // Repeat start.
        if info.repeat_start {
            for &(_, _, t) in tops {
                let y0 = top + t;
                o.push(
                    Shape::Rect {
                        x: mx,
                        y: y0,
                        w: g::THICK_BAR,
                        h: 4.0,
                    },
                    None,
                );
                o.line(mx + 0.9, y0, mx + 0.9, y0 + 4.0, g::THIN_BAR, None);
                o.glyph(g::REPEAT_DOT, mx + 1.3, y0 + 1.5, None);
                o.glyph(g::REPEAT_DOT, mx + 1.3, y0 + 2.5, None);
            }
            mx += 1.6;
        }
        // Column positions.
        let mut cols: Vec<(u32, f32)> = Vec::with_capacity(plan.cols.len() + 1);
        let mut cx = mx + PAD_LEFT;
        for (ci, &(t, lead, _)) in plan.cols.iter().enumerate() {
            cx += lead;
            cols.push((t, cx));
            cx += plan.space(ci, sys.stretch);
        }
        let m_end = cx;
        cols.push((plan.len, m_end));
        all_cols.extend(cols.iter().map(|c| c.1));

        // Tempo and measure text above the top staff.
        if let Some(&(_, _, t0)) = tops.first() {
            let ty = top + t0 - (row_reach + 1.4).max(2.6);
            let mut tx = cols.first().map(|c| c.1).unwrap_or(mx);
            if let Some(r) = &info.rehearsal {
                // Boxed rehearsal mark: above the barline; at the start of a system above the
                // clef; below the staff when moved there; plus the user's offset.
                let rs = o.f.size(TextKind::Rehearsal, 2.2);
                let bh = rs + 0.4;
                let w = text_width(r, rs) + 1.0;
                let staff_top = top + t0;
                let (bx0, by1) = match info.rehearsal_place {
                    // A mark placed by hand (as MuseScore stores it): its text centred on the
                    // measure's start plus the offset, the baseline the offset away from the
                    // top line (or, below, from the bottom line of the bottom staff).
                    Some(place) => {
                        let base = if place.below {
                            tops.last()
                                .map_or(staff_top + 4.0, |t| top + t.2 + staff_h(score, t.0, t.1))
                        } else {
                            staff_top
                        };
                        (
                            m_start + place.dx_sp() - w / 2.0,
                            base + place.dy_sp() + 0.6 * bh / 2.6,
                        )
                    }
                    // At the start of a system above the clef, clear of the measure number;
                    // elsewhere above the barline, over everything else above the staff.
                    None if k == 0 => (x_start, staff_top - 3.6),
                    None => (m_start - 0.4, ty - 2.6),
                };
                o.styled(
                    TextKind::Rehearsal,
                    r,
                    bx0 + w / 2.0,
                    by1 - 0.6 * bh / 2.6,
                    2.2,
                    Align::Center,
                );
                for (a, b, c, d2) in [
                    (bx0, by1, bx0 + w, by1),
                    (bx0, by1 - bh, bx0 + w, by1 - bh),
                    (bx0, by1, bx0, by1 - bh),
                    (bx0 + w, by1, bx0 + w, by1 - bh),
                ] {
                    o.line(a, b, c, d2, 0.16, None);
                }
                o.hit(
                    bx0,
                    by1 - bh,
                    bx0 + w,
                    by1,
                    Target::MeasureMark {
                        measure: m,
                        kind: crate::MeasureMark::Rehearsal,
                        ax: m_start,
                        ay: staff_top,
                        x: bx0 + w / 2.0,
                        base: by1 - 0.6 * bh / 2.6,
                    },
                );
                if k > 0 && info.rehearsal_place.is_none() {
                    tx = tx.max(bx0 + w + 0.8);
                }
            }
            for j in info.jumps.iter().filter(|j| !j.at_end()) {
                let name = match j {
                    scoreart_model::Jump::Segno => "segno",
                    scoreart_model::Jump::Coda => "coda",
                    _ => "codaSquare",
                };
                if let Some((c, w, lo, _)) = crate::smufl::get(name) {
                    let left = if k == 0 { x_start } else { m_start };
                    let (mx0, mbase, msize) = match info.marker_place {
                        Some(p) => (left + p.dx_sp(), top + t0 + p.dy_sp(), MARKER_SCALE),
                        None => (tx, ty, 1.0),
                    };
                    o.hit(
                        mx0,
                        mbase - 3.0 * msize,
                        mx0 + w * msize,
                        mbase + 0.3,
                        Target::MeasureMark {
                            measure: m,
                            kind: crate::MeasureMark::Marker,
                            ax: left,
                            ay: top + t0,
                            x: mx0,
                            base: mbase,
                        },
                    );
                    match info.marker_place {
                        // Placed by hand: from the measure's barline (the system's start for
                        // its first measure) and the top staff line.
                        // Drawn at text size, as MuseScore draws its markers' symbols.
                        Some(p) => {
                            o.push(
                                Shape::Glyph {
                                    c,
                                    x: left + p.dx_sp(),
                                    y: top + t0 + p.dy_sp() + lo * MARKER_SCALE,
                                    scale: MARKER_SCALE,
                                },
                                None,
                            );
                        }
                        None => {
                            o.glyph(c, tx, ty + lo, None);
                            tx += w + 0.8;
                        }
                    }
                }
            }
            if let Some(tt) = &info.tempo_text {
                let ts = o.styled(TextKind::Tempo, tt, tx, ty, 2.0, Align::Left);
                tx += text_width(tt, ts) + 0.8;
            }
            let mut end_x = m_end - 0.3;
            // Placed by hand: the right end from the measure's end, the baseline from the top
            // staff line.
            let end_y = match info.jump_place {
                Some(p) => {
                    end_x = m_end + p.dx_sp();
                    top + t0 + p.dy_sp()
                }
                None => ty,
            };
            let ty_end = end_y;
            let right_end = end_x;
            for j in info.jumps.iter().filter(|j| j.at_end()) {
                let ty = ty_end;
                if let Some(t) = j.text() {
                    if *j == scoreart_model::Jump::ToCoda
                        && let Some((c, w, lo, _)) = crate::smufl::get("coda")
                    {
                        o.glyph(c, end_x - w, ty + lo, None);
                        end_x -= w + 0.5;
                    }
                    let ts = o.styled(TextKind::SystemText, t, end_x, ty, 1.9, Align::Right);
                    end_x -= text_width(t, ts) + 1.0;
                }
            }
            if info.jumps.iter().any(|j| j.at_end()) && end_x < right_end {
                o.hit(
                    end_x + 0.5,
                    ty_end - 2.0,
                    right_end,
                    ty_end + 0.5,
                    Target::MeasureMark {
                        measure: m,
                        kind: crate::MeasureMark::Jump,
                        ax: m_end,
                        ay: top + t0,
                        x: right_end,
                        base: ty_end,
                    },
                );
            }
            if let Some(tempo) = info.tempo {
                // The beat unit: an eighth, quarter or half, perhaps dotted.
                let (unit, dotted) = tempo.unit.parts();
                let c = match unit {
                    "eighth" => g::MET_EIGHTH_UP,
                    "half" => g::MET_HALF_UP,
                    _ => g::MET_QUARTER_UP,
                };
                o.push(
                    Shape::Glyph {
                        c,
                        x: tx,
                        y: ty,
                        scale: 0.75,
                    },
                    None,
                );
                let mut lx = tx + 1.4;
                if dotted {
                    o.push(
                        Shape::Glyph {
                            c: g::MET_DOT,
                            x: tx + 1.2,
                            y: ty,
                            scale: 0.75,
                        },
                        None,
                    );
                    lx += 0.6;
                }
                let label = format!("= {}", tempo.bpm);
                let ts = o.styled(TextKind::Tempo, &label, lx, ty, 1.9, Align::Left);
                tx += (lx - tx + text_width(&label, ts) + 1.0).max(7.0);
            }
            if let Some(text) = &info.text {
                o.styled(TextKind::SystemText, text, tx, ty, 1.9, Align::Left);
            }
        }

        for &(pi, s, t) in tops {
            let staff_top = top + t;
            let clef = score.clef_at(pi, s, m);
            // An instrument change: "To Flute" above the part's first staff.
            if s == 0
                && let Some(ch) = score
                    .parts
                    .get(pi)
                    .and_then(|p| p.measures.get(m))
                    .and_then(|pm| pm.instrument_change.as_ref())
            {
                o.styled(
                    TextKind::StaffText,
                    &format!("To {}", ch.name),
                    m_start + 0.6,
                    staff_top - 2.6,
                    1.8,
                    Align::Left,
                );
            }
            out_staff_box(
                &mut o,
                &mut ctx.heads,
                &mut ctx.events,
                ctx.score,
                ctx.acc,
                StaffDraw {
                    system: si,
                    part: pi,
                    staff: s,
                    measure: m,
                    x0: m_start,
                    x1: m_end,
                    top: staff_top,
                    clef,
                    cols: &cols,
                    time: score.time_at(m),
                    sys_x1: x_end,
                    lyrics: lyric_base(score, pi, s, &sys.measures),
                },
            );
            out.staves.push(StaffBox {
                page: page_index,
                system: si,
                part: pi,
                staff: s,
                measure: m,
                x0: m_start,
                x1: m_end,
                top: staff_top,
                clef,
                columns: cols.clone(),
                mag: staff_mag(score, pi, s),
            });
        }

        // Barline at the end of the measure, per part.
        let last_in_sys = k + 1 == sys.measures.len();
        let bx = if last_in_sys { x_end } else { m_end };
        for (pi, _) in score.parts.iter().enumerate() {
            let staff_tops: Vec<f32> = tops.iter().filter(|t| t.0 == pi).map(|t| top + t.2).collect();
            let (Some(pt), Some(pb)) = (staff_tops.first().copied(), staff_tops.last().map(|t| t + 4.0))
            else {
                continue;
            };
            draw_barline(&mut o, bx, pt, pb, info.barline, &staff_tops);
        }
        boxes.push((m, m_start, m_end));
        mx = bx;
        let _ = sys_h;
    }
    // Volta brackets over the top staff.
    if let Some(&(_, _, t0)) = tops.first() {
        // Above the text row (tempo, rehearsal marks, jumps) when the system has one, else
        // right above what stands over the staff (chord symbols, high notes).
        let text_row = sys.measures.iter().any(|m| {
            score.measures.get(*m).is_some_and(|i| {
                i.tempo.is_some()
                    || i.text.is_some()
                    || i.tempo_text.is_some()
                    || i.rehearsal.is_some()
                    || !i.jumps.is_empty()
            })
        });
        let vy = if text_row {
            top + t0 - (row_reach + 1.4).max(2.6) - 3.6
        } else {
            top + t0 - row_reach.max(1.0) - 2.6
        };
        let mut k = 0;
        while k < boxes.len() {
            let Some(&(m, x0, _)) = boxes.get(k) else { break };
            let Some(n) = score.measures.get(m).and_then(|i| i.volta) else {
                k += 1;
                continue;
            };
            let mut j = k;
            while boxes
                .get(j + 1)
                .is_some_and(|b| score.measures.get(b.0).and_then(|i| i.volta) == Some(n))
            {
                j += 1;
            }
            let Some(&(lm, _, x1)) = boxes.get(j) else { break };
            let last = score.measures.get(lm);
            let continues = score.measures.get(lm + 1).and_then(|i| i.volta) == Some(n);
            let closed = !continues && (n == 1 || last.is_some_and(|i| i.barline == Barline::RepeatEnd));
            let (xa, xb) = (x0 + 0.3, x1 - 0.4);
            let starts_here = m == 0 || score.measures.get(m - 1).and_then(|i| i.volta) != Some(n);
            o.line(xa, vy, xb, vy, 0.16, None);
            if starts_here {
                o.line(xa, vy, xa, vy + 2.0, 0.16, None);
                let label = score
                    .measures
                    .get(m)
                    .and_then(|i| i.volta_text.clone())
                    .unwrap_or_else(|| format!("{n}."));
                o.text(&label, xa + 0.5, vy + 1.7, 1.7, Align::Left, true, false);
            }
            if closed {
                o.line(xb, vy, xb, vy + 2.0, 0.16, None);
            }
            k = j + 1;
        }
    }

    // Small staves: what was drawn for them at full size shrinks to their size, the noteheads
    // staying on their columns.
    let content_x = ctx.system_ends.last().map_or(x_start, |e| e.2);
    for (j, &(pi, s, t)) in tops.iter().enumerate() {
        let mag = staff_mag(score, pi, s);
        if mag >= 1.0 {
            continue;
        }
        let st = top + t;
        // What belongs to the staff: from a little above it to a little above the next one.
        let (upper, lower) = (st - 4.0, tops.get(j + 1).map_or(f32::MAX, |q| top + q.2 - 3.0));
        let col = |x: f32| -> f32 {
            if x < content_x {
                return x;
            }
            let c = all_cols
                .iter()
                .copied()
                .min_by(|a, b| (a - x).abs().total_cmp(&(b - x).abs()))
                .unwrap_or(x);
            c + (x - c) * mag
        };
        let near = |y: f32| y >= st - 3.0 && y <= st + 7.0;
        let fy = |y: f32| small_y(y, st, mag);
        for it in o.page.items.iter_mut().skip(items0) {
            let (y0, y1, x0) = match &it.shape {
                Shape::Glyph { x, y, .. } => (*y, *y, *x),
                Shape::Line { x1, y1, x2, y2, .. } => (y1.min(*y2), y1.max(*y2), x1.min(*x2)),
                Shape::Rect { x, y, h, .. } => (*y, y + h, *x),
                Shape::Quad { p } | Shape::Curve { p, .. } => (
                    p.iter().map(|q| q.1).fold(f32::MAX, f32::min),
                    p.iter().map(|q| q.1).fold(f32::MIN, f32::max),
                    p.iter().map(|q| q.0).fold(f32::MAX, f32::min),
                ),
                Shape::Text { x, y, .. } => (*y, *y, *x),
            };
            if !(y0 >= upper && y1 <= lower && x0 >= x_start - 0.2) {
                continue;
            }
            match &mut it.shape {
                Shape::Glyph { x, y, scale, .. } => {
                    if near(*y) {
                        *scale *= mag;
                    }
                    *x = col(*x);
                    *y = fy(*y);
                }
                Shape::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    width,
                } => {
                    if near(*y1) && near(*y2) && (*x1 - *x2).abs() > 1e-3 && (*y1 - *y2).abs() < 1e-3 {
                        // A staff line or ledger line keeps its length.
                    } else {
                        *x1 = col(*x1);
                        *x2 = col(*x2);
                    }
                    *y1 = fy(*y1);
                    *y2 = fy(*y2);
                    *width *= mag.max(0.85);
                }
                Shape::Rect { x, y, w, h } => {
                    let (a, b) = (fy(*y), fy(*y + *h));
                    *x = col(*x);
                    *w *= mag;
                    *y = a;
                    *h = b - a;
                }
                Shape::Quad { p } => {
                    for q in p.iter_mut() {
                        *q = (col(q.0), fy(q.1));
                    }
                }
                Shape::Curve { p, thick } => {
                    for q in p.iter_mut() {
                        *q = (col(q.0), fy(q.1));
                    }
                    *thick *= mag;
                }
                Shape::Text { x, y, .. } => {
                    *x = col(*x);
                    *y = fy(*y);
                }
            }
        }
        for h in o.hits.iter_mut().skip(hits0) {
            if h.y0 >= upper - 1.0 && h.y1 <= lower + 1.0 && h.x0 >= x_start - 0.2 && h.y1 > upper {
                h.x0 = col(h.x0);
                h.x1 = col(h.x1);
                h.y0 = fy(h.y0);
                h.y1 = fy(h.y1);
            }
        }
        // Ties, slurs and lines drawn later start from the shrunk notes.
        for (ev, a) in ctx.events.iter_mut() {
            if ev.part == pi && ev.staff == s && a.system == si && a.page == page_index {
                a.x_left = col(a.x_left);
                a.x_right = col(a.x_right);
                a.top = fy(a.top);
                a.bottom = fy(a.bottom);
                a.stem_end = fy(a.stem_end);
            }
        }
        for ((ev, _), te) in ctx.heads.iter_mut() {
            if ev.part == pi && ev.staff == s && te.system == si && te.page == page_index {
                te.x_left = col(te.x_left);
                te.x_right = col(te.x_right);
                te.y = fy(te.y);
            }
        }
    }
}

fn draw_brace(o: &mut Out, x: f32, top: f32, bottom: f32) {
    let h = bottom - top;
    let mid = (top + bottom) / 2.0;
    let w = (h * 0.06).clamp(0.6, 1.4);
    o.push(
        Shape::Curve {
            p: [
                (x + w, top),
                (x - w * 0.6, top + h * 0.12),
                (x + w * 1.4, mid - h * 0.2),
                (x, mid),
            ],
            thick: 0.45,
        },
        None,
    );
    o.push(
        Shape::Curve {
            p: [
                (x, mid),
                (x + w * 1.4, mid + h * 0.2),
                (x - w * 0.6, bottom - h * 0.12),
                (x + w, bottom),
            ],
            thick: 0.45,
        },
        None,
    );
}

fn draw_barline(o: &mut Out, x: f32, top: f32, bottom: f32, kind: Barline, staff_tops: &[f32]) {
    let h = bottom - top;
    match kind {
        Barline::Hidden => {}
        Barline::Regular => o.line(
            x - g::THIN_BAR / 2.0,
            top,
            x - g::THIN_BAR / 2.0,
            bottom,
            g::THIN_BAR,
            None,
        ),
        Barline::Double => {
            o.line(
                x - g::THIN_BAR / 2.0,
                top,
                x - g::THIN_BAR / 2.0,
                bottom,
                g::THIN_BAR,
                None,
            );
            let x2 = x - g::THIN_BAR - g::BAR_SEPARATION - g::THIN_BAR / 2.0;
            o.line(x2, top, x2, bottom, g::THIN_BAR, None);
        }
        Barline::Final | Barline::RepeatEnd => {
            o.push(
                Shape::Rect {
                    x: x - g::THICK_BAR,
                    y: top,
                    w: g::THICK_BAR,
                    h,
                },
                None,
            );
            let x2 = x - g::THICK_BAR - g::BAR_SEPARATION - g::THIN_BAR / 2.0;
            o.line(x2, top, x2, bottom, g::THIN_BAR, None);
            if kind == Barline::RepeatEnd {
                for t in staff_tops {
                    o.glyph(g::REPEAT_DOT, x2 - 0.8, t + 1.5, None);
                    o.glyph(g::REPEAT_DOT, x2 - 0.8, t + 2.5, None);
                }
            }
        }
    }
}

struct StaffDraw<'a> {
    system: usize,
    part: usize,
    staff: usize,
    measure: usize,
    x0: f32,
    x1: f32,
    top: f32,
    clef: Clef,
    cols: &'a [(u32, f32)],
    time: TimeSig,
    /// Where the system ends: texts are kept inside it.
    sys_x1: f32,
    /// Baseline of the first verse below the bottom staff line.
    lyrics: f32,
}

fn col_x(cols: &[(u32, f32)], tick: u32) -> f32 {
    cols.iter()
        .find(|c| c.0 == tick)
        .or_else(|| cols.iter().rev().find(|c| c.0 <= tick))
        .map(|c| c.1)
        .unwrap_or(0.0)
}

fn out_staff_box(
    o: &mut Out,
    heads: &mut HashMap<(EventRef, Pitch), TieEnd>,
    anchors: &mut HashMap<EventRef, EvAnchor>,
    score: &Score,
    acc: &HashMap<NoteKey, Accidental>,
    d: StaffDraw,
) {
    let Some(sm) = score.staff_measure(d.part, d.staff, d.measure) else {
        return;
    };
    o.hit(
        d.x0,
        d.top - 0.5,
        d.x1,
        d.top + 4.5,
        Target::Measure {
            part: d.part,
            staff: d.staff,
            measure: d.measure,
        },
    );
    if sm.measure_repeat {
        if let Some((c, w, _, _)) = crate::smufl::get("repeat1Bar") {
            let first = d.cols.first().map(|c| c.1).unwrap_or(d.x0);
            let last = d.cols.last().map(|c| c.1).unwrap_or(d.x1);
            o.glyph(c, (first - PAD_LEFT + last) / 2.0 - w / 2.0, d.top + 2.0, None);
        }
        return;
    }
    let multi = sm.voices.len() > 1;
    for (vi, v) in sm.voices.iter().enumerate() {
        let onsets = v.onsets();
        let mut geos: Vec<EvGeo> = Vec::with_capacity(v.events.len());
        // Whole-measure rest; in a measure of its own length (pickup, short ending) an empty
        // voice is a measure rest too, however its rests are split.
        let irregular = score.measures.get(d.measure).is_some_and(|i| i.actual.is_some());
        let plain_rest = |e: &Event| {
            e.is_rest()
                && e.dynamic.is_none()
                && e.text.is_none()
                && e.chord_symbol.is_none()
                && e.articulations.is_empty()
                && e.duration.tuplet.is_none()
        };
        if (v.events.len() == 1 && v.events.first().is_some_and(Event::is_rest)
            || irregular && !v.events.is_empty() && v.events.iter().all(plain_rest))
            && v.ticks() == score.measure_ticks(d.measure)
        {
            let ev = EventRef {
                part: d.part,
                staff: d.staff,
                voice: vi,
                measure: d.measure,
                index: 0,
            };
            let first = d.cols.first().map(|c| c.1).unwrap_or(d.x0);
            let last = d.cols.last().map(|c| c.1).unwrap_or(d.x1);
            let x = (first - PAD_LEFT + last) / 2.0 - 0.566;
            let mut y = d.top + 1.0;
            if multi {
                y += if vi % 2 == 0 { -1.0 } else { 2.0 };
            }
            let len = score.measure_ticks(d.measure);
            let (c, w) = if len >= TPQ * 8 {
                g::rest(NoteValue::Breve)
            } else {
                g::rest(NoteValue::Whole)
            };
            o.glyph(c, x, y, Some(ev));
            o.hit(x - 0.2, y - 0.3, x + w + 0.2, y + 0.8, Target::Rest { ev });
            // What stands on the rest: a chord symbol at the start of the measure, a dynamic
            // under it, a staff text over it.
            if let Some(e) = v.events.first() {
                let x0 = first;
                if let Some(cs) = &e.chord_symbol {
                    let text = chord_text(cs, o.f.german);
                    let sh = o.f.shape(
                        TextKind::ChordSymbol,
                        &text,
                        x0,
                        d.top - CHORD_ABOVE,
                        2.2,
                        Align::Left,
                    );
                    o.push(sh, Some(ev));
                }
                if let Some(dy) = e.dynamic {
                    let place = e.dynamic_place.unwrap_or_default();
                    let mut dx = x0 - 0.2 + place.dx_sp();
                    let yy = d.top + 4.0 + 3.0 + place.dy_sp();
                    for ch in dy.id().chars() {
                        if let Some((c, w)) = g::dynamic_letter(ch) {
                            o.glyph(c, dx, yy, Some(ev));
                            dx += w - 0.1;
                        }
                    }
                }
                if let Some(t) = &e.text {
                    let place = e.text_place.unwrap_or_default();
                    let yy = match e.text_place {
                        Some(p) if p.below => d.top + 4.0 + 3.0,
                        Some(_) => d.top - 1.5,
                        None => d.top - CHORD_ABOVE - if e.chord_symbol.is_some() { 2.6 } else { 0.0 },
                    } + place.dy_sp();
                    let sh =
                        o.f.shape(TextKind::StaffText, t, x0 + place.dx_sp(), yy, 1.9, Align::Left);
                    o.push(sh, Some(ev));
                }
            }
            anchors.insert(
                ev,
                EvAnchor {
                    page: o.page_index,
                    system: d.system,
                    x_left: first,
                    x_right: last - 1.0,
                    top: y - 1.0,
                    bottom: y + 1.0,
                    up: true,
                    stem_end: y - 1.0,
                    staff_top: d.top,
                },
            );
            continue;
        }
        // Pass 1: positions, stem directions, beam groups.
        let group_ticks = d.time.beat_group_ticks().max(1);
        let mut group_id = 0usize;
        let mut prev_beamable: Option<(u32, u32)> = None; // (group index, end tick)
        for (ei, (e, &on)) in v.events.iter().zip(onsets.iter()).enumerate() {
            let ev = EventRef {
                part: d.part,
                staff: d.staff,
                voice: vi,
                measure: d.measure,
                index: ei,
            };
            let x = col_x(d.cols, on);
            let heads_v: Vec<(Pitch, f32, f32)> = e
                .notes()
                .iter()
                .map(|n| {
                    (
                        n.pitch,
                        d.top
                            + note_y(d.clef, n.pitch)
                            + f32::from(score.ottava_at(d.part, d.staff, d.measure, on)) * 3.5,
                        x,
                    )
                })
                .collect();
            let beamable = !e.is_rest() && e.duration.value.beams() > 0;
            let gi = on / group_ticks;
            let end = on + e.ticks();
            let same_group = (end.saturating_sub(1)) / group_ticks == gi;
            let beam_group = if beamable && same_group {
                match prev_beamable {
                    Some((pg, pend)) if pg == gi && pend == on => Some(group_id),
                    _ => {
                        group_id += 1;
                        Some(group_id)
                    }
                }
            } else {
                None
            };
            prev_beamable = if beamable && same_group {
                Some((gi, end))
            } else {
                None
            };
            geos.push(EvGeo {
                ev,
                onset: on,
                x,
                value: e.duration.value,
                rest: e.is_rest(),
                heads: heads_v,
                up: true,
                stem_x: 0.0,
                stem_end: 0.0,
                beam_group,
            });
        }
        // Groups of one are not beamed.
        let mut counts: HashMap<usize, usize> = HashMap::new();
        for gg in geos.iter().filter_map(|g| g.beam_group) {
            *counts.entry(gg).or_insert(0) += 1;
        }
        for gg in &mut geos {
            if gg
                .beam_group
                .is_some_and(|b| counts.get(&b).copied().unwrap_or(0) < 2)
            {
                gg.beam_group = None;
            }
        }
        // Stem directions.
        let mid = d.top + 2.0;
        let dir_of = |gs: &[&EvGeo]| -> bool {
            if multi {
                return vi % 2 == 0;
            }
            let mut above = 0.0f32;
            let mut below = 0.0f32;
            for gg in gs {
                for h in &gg.heads {
                    above = above.max(mid - h.1);
                    below = below.max(h.1 - mid);
                }
            }
            below > above
        };
        let mut i = 0;
        while i < geos.len() {
            let Some(gg) = geos.get(i) else { break };
            if let Some(b) = gg.beam_group {
                let members: Vec<usize> = (i..geos.len())
                    .take_while(|j| geos.get(*j).is_some_and(|x| x.beam_group == Some(b)))
                    .collect();
                let up = dir_of(&members.iter().filter_map(|j| geos.get(*j)).collect::<Vec<_>>());
                for j in &members {
                    if let Some(x) = geos.get_mut(*j) {
                        x.up = up;
                    }
                }
                i += members.len().max(1);
            } else {
                let up = dir_of(&[gg]);
                if let Some(x) = geos.get_mut(i) {
                    x.up = up;
                }
                i += 1;
            }
        }
        // Displace seconds, compute stems.
        for gg in &mut geos {
            if gg.rest || gg.heads.is_empty() {
                continue;
            }
            let shift = g::HEAD_W - g::STEM;
            if gg.up {
                let mut prev: Option<(f32, bool)> = None;
                for h in gg.heads.iter_mut() {
                    let displaced = matches!(prev, Some((py, false)) if (py - h.1 - 0.5).abs() < 0.01);
                    if displaced {
                        h.2 = gg.x + shift;
                    }
                    prev = Some((h.1, displaced));
                }
            } else {
                let mut prev: Option<(f32, bool)> = None;
                for h in gg.heads.iter_mut().rev() {
                    let displaced = matches!(prev, Some((py, false)) if (h.1 - py - 0.5).abs() < 0.01);
                    if displaced {
                        h.2 = gg.x - shift;
                    }
                    prev = Some((h.1, displaced));
                }
            }
            let top_y = gg.heads.iter().map(|h| h.1).fold(f32::MAX, f32::min);
            let bot_y = gg.heads.iter().map(|h| h.1).fold(f32::MIN, f32::max);
            let extra = 0.5 * f32::from(gg.value.beams().saturating_sub(1));
            if gg.up {
                gg.stem_x = gg.x + g::HEAD_W - g::STEM / 2.0;
                gg.stem_end = (top_y - STEM_LEN - extra).min(mid);
            } else {
                gg.stem_x = gg.x + g::STEM / 2.0;
                gg.stem_end = (bot_y + STEM_LEN + extra).max(mid);
            }
        }
        // Beams: fit a line through the group's stems.
        let mut b = 1usize;
        while b <= group_id {
            let idx: Vec<usize> = geos
                .iter()
                .enumerate()
                .filter(|(_, x)| x.beam_group == Some(b))
                .map(|(i, _)| i)
                .collect();
            b += 1;
            if idx.len() < 2 {
                continue;
            }
            let members: Vec<EvGeo> = idx.iter().filter_map(|i| geos.get(*i).cloned()).collect();
            let (Some(f), Some(l)) = (members.first(), members.last()) else {
                continue;
            };
            let up = f.up;
            let outer = |m: &EvGeo| -> f32 {
                if up {
                    m.heads.iter().map(|h| h.1).fold(f32::MAX, f32::min)
                } else {
                    m.heads.iter().map(|h| h.1).fold(f32::MIN, f32::max)
                }
            };
            let dx = (l.stem_x - f.stem_x).max(0.1);
            let slope = ((outer(l) - outer(f)) / dx).clamp(-0.18, 0.18) * 0.7;
            let slope = if (slope * dx).abs() > 1.2 {
                1.2 * slope.signum() / dx
            } else {
                slope
            };
            let max_beams = members.iter().map(|m| m.value.beams()).max().unwrap_or(1);
            let min_len = 2.8 + 0.75 * f32::from(max_beams.saturating_sub(1));
            let mut y0 = f.stem_end;
            for m in &members {
                let at = y0 + slope * (m.stem_x - f.stem_x);
                let need = if up {
                    outer(m) - min_len
                } else {
                    outer(m) + min_len
                };
                if up && at > need {
                    y0 -= at - need;
                } else if !up && at < need {
                    y0 += need - at;
                }
            }
            // Beams sit at least at the middle line.
            let at_mid = |y: f32| if up { y.min(mid) } else { y.max(mid) };
            let first_y = at_mid(y0);
            let shift0 = first_y - y0;
            for i in &idx {
                if let Some(m) = geos.get_mut(*i) {
                    m.stem_end = y0 + shift0 + slope * (m.stem_x - f.stem_x);
                }
            }
            let beam_y = |x: f32| y0 + shift0 + slope * (x - f.stem_x);
            let dir = if up { 1.0 } else { -1.0 };
            for level in 0..max_beams {
                let off = f32::from(level) * (g::BEAM + g::BEAM_SPACING) * dir;
                // Runs of consecutive members that need this many beams.
                let mut run: Vec<&EvGeo> = Vec::new();
                let flush = |run: &mut Vec<&EvGeo>, o: &mut Out, all: &[EvGeo]| {
                    if run.is_empty() {
                        return;
                    }
                    let (Some(a), Some(z)) = (run.first(), run.last()) else {
                        return;
                    };
                    let (mut xa, mut xb) = (a.stem_x - g::STEM / 2.0, z.stem_x + g::STEM / 2.0);
                    if run.len() == 1 {
                        // A beamlet pointing into the group.
                        let last = all.last().map(|m| m.ev) == Some(a.ev);
                        if last {
                            xa = a.stem_x - 1.1;
                        } else {
                            xb = a.stem_x + 1.1;
                        }
                    }
                    let ya = beam_y(xa) + off;
                    let yb = beam_y(xb) + off;
                    let t = g::BEAM * dir;
                    o.push(
                        Shape::Quad {
                            p: [(xa, ya), (xb, yb), (xb, yb + t), (xa, ya + t)],
                        },
                        Some(a.ev),
                    );
                    run.clear();
                };
                let all: Vec<EvGeo> = idx.iter().filter_map(|i| geos.get(*i).cloned()).collect();
                for m in &all {
                    if m.value.beams() > level {
                        run.push(m);
                    } else {
                        flush(&mut run, o, &all);
                    }
                }
                flush(&mut run, o, &all);
            }
        }

        // Pass 2: draw.
        for (gg, e) in geos.iter().zip(v.events.iter()) {
            let ev = gg.ev;
            if gg.rest {
                let (c, w) = g::rest(gg.value);
                let mut y = d.top
                    + match gg.value {
                        NoteValue::Whole | NoteValue::Breve => 1.0,
                        _ => 2.0,
                    };
                if multi {
                    y += if vi % 2 == 0 { -2.0 } else { 2.0 };
                }
                o.glyph(c, gg.x, y, Some(ev));
                o.hit(gg.x - 0.1, y - 1.5, gg.x + w + 0.1, y + 1.5, Target::Rest { ev });
                let mut above = (d.top - 0.9).min(y - 2.0);
                for a in e
                    .articulations
                    .iter()
                    .filter(|a| a.def().place == scoreart_model::marks::Place::Above)
                {
                    if let Some((c, mw, lo, hi)) = g::mark(*a, true) {
                        o.glyph(c, gg.x + w / 2.0 - mw / 2.0, above + lo, Some(ev));
                        above -= hi - lo + 0.35;
                    }
                }
                anchors.insert(
                    ev,
                    EvAnchor {
                        page: o.page_index,
                        system: d.system,
                        x_left: gg.x,
                        x_right: gg.x + w,
                        top: y - 1.0,
                        bottom: y + 1.0,
                        up: true,
                        stem_end: y - 1.0,
                        staff_top: d.top,
                    },
                );
                for k in 0..e.duration.dots {
                    o.glyph(g::DOT, gg.x + w + 0.3 + 0.5 * f32::from(k), y - 0.5, Some(ev));
                }
                draw_tuplet(o, gg, e, &geos, v, d.top);
                continue;
            }
            let (_, head_w) = g::notehead(gg.value);
            // Ledger lines.
            let min_y = gg.heads.iter().map(|h| h.1).fold(f32::MAX, f32::min);
            let max_y = gg.heads.iter().map(|h| h.1).fold(f32::MIN, f32::max);
            let lx0 = gg.heads.iter().map(|h| h.2).fold(f32::MAX, f32::min) - g::LEDGER_EXT;
            let lx1 = gg.heads.iter().map(|h| h.2).fold(f32::MIN, f32::max) + head_w + g::LEDGER_EXT;
            let mut ly = d.top - 1.0;
            while ly >= min_y - 0.01 && ly > d.top - 30.0 {
                o.line(lx0, ly, lx1, ly, g::LEDGER, Some(ev));
                ly -= 1.0;
            }
            let mut ly = d.top + 5.0;
            while ly <= max_y + 0.01 && ly < d.top + 34.0 {
                o.line(lx0, ly, lx1, ly, g::LEDGER, Some(ev));
                ly += 1.0;
            }
            // Heads, accidentals, dots.
            let mut acc_cols: Vec<(f32, usize)> = Vec::new();
            let mut dot_ys: Vec<f32> = Vec::new();
            let right_edge = gg.heads.iter().map(|h| h.2).fold(f32::MIN, f32::max) + head_w;
            let left_edge = gg.heads.iter().map(|h| h.2).fold(f32::MAX, f32::min);
            let mut acc_room = 0.0f32;
            let notes = e.notes();
            for (ni, h) in gg.heads.iter().enumerate().rev() {
                let note = notes.get(ni);
                let (head, _) = g::head_for(note.map(|n| n.head).unwrap_or_default(), gg.value);
                o.glyph(head, h.2, h.1, Some(ev));
                o.hit(
                    h.2,
                    h.1 - 0.5,
                    h.2 + head_w,
                    h.1 + 0.5,
                    Target::Note { ev, note: ni },
                );
                let paren = note.is_some_and(|n| n.paren);
                if paren {
                    if let Some((c, _, _, _)) = crate::smufl::get("noteheadParenthesisLeft") {
                        o.glyph(c, h.2 - 0.45, h.1, Some(ev));
                    }
                    if let Some((c, _, _, _)) = crate::smufl::get("noteheadParenthesisRight") {
                        o.glyph(c, h.2 + head_w + 0.08, h.1, Some(ev));
                    }
                }
                if let Some(a) = acc.get(&(d.part, d.staff, d.measure, gg.ev.voice, gg.ev.index, ni)) {
                    let (c, w) = g::accidental(*a);
                    let col = (0..4)
                        .find(|c| !acc_cols.iter().any(|(y, cc)| cc == c && (y - h.1).abs() < 2.9))
                        .unwrap_or(3);
                    acc_cols.push((h.1, col));
                    let ax = left_edge - 0.2 - w - col as f32 * 1.15 - if paren { 0.45 } else { 0.0 };
                    acc_room = acc_room.max(left_edge - ax + 0.2);
                    o.glyph(c, ax, h.1, Some(ev));
                }
                if e.duration.dots > 0 {
                    let on_line = ((h.1 - d.top) * 2.0).round() as i32 % 2 == 0;
                    let dy = if on_line { h.1 - 0.5 } else { h.1 };
                    if !dot_ys.iter().any(|y| (y - dy).abs() < 0.1) {
                        dot_ys.push(dy);
                        for k in 0..e.duration.dots {
                            o.glyph(g::DOT, right_edge + 0.35 + 0.5 * f32::from(k), dy, Some(ev));
                        }
                    }
                }
                let anchor = TieEnd {
                    page: o.page_index,
                    system: d.system,
                    x_left: h.2,
                    x_right: h.2 + head_w,
                    y: h.1,
                    up: gg.up,
                };
                heads.insert((ev, h.0), anchor);
            }
            // Stem and flag.
            let stem_start = if gg.up {
                max_y - g::STEM_Y
            } else {
                min_y + g::STEM_Y
            };
            if gg.value.has_stem() {
                o.line(gg.stem_x, stem_start, gg.stem_x, gg.stem_end, g::STEM, Some(ev));
                if gg.beam_group.is_none()
                    && let Some((c, _)) = g::flag(gg.value.beams(), gg.up)
                {
                    o.glyph(c, gg.stem_x - g::STEM / 2.0, gg.stem_end, Some(ev));
                }
            }
            // A longa: a stem down from the right side of its square head.
            if gg.value == NoteValue::Long {
                let sx = gg.x + head_w - g::STEM / 2.0;
                o.line(sx, max_y, sx, max_y + 3.5, g::STEM, Some(ev));
            }
            // Tremolo strokes through the stem (above a whole note).
            let trem = match e.tremolo {
                scoreart_model::Tremolo::None => None,
                scoreart_model::Tremolo::Strokes(n) => crate::smufl::get(match n {
                    1 => "tremolo1",
                    2 => "tremolo2",
                    4 => "tremolo4",
                    _ => "tremolo3",
                }),
                scoreart_model::Tremolo::Buzz => crate::smufl::get("buzzRoll"),
            };
            if let Some((c, w, lo, hi)) = trem {
                let (cx, cy) = if gg.value.has_stem() {
                    let far = if gg.up {
                        gg.stem_end + 1.2
                    } else {
                        gg.stem_end - 1.2
                    };
                    (gg.stem_x, (far + if gg.up { min_y } else { max_y }) / 2.0)
                } else {
                    (gg.x + head_w / 2.0, min_y - 2.0)
                };
                o.glyph(c, cx - w / 2.0, cy + (lo + hi) / 2.0, Some(ev));
            }
            // Arpeggio: a wavy line (or bracket) left of the chord.
            if e.arpeggio != scoreart_model::Arpeggio::None {
                let ax = left_edge - acc_room - 0.9;
                let (y0, y1) = (min_y - 0.6, max_y + 0.6);
                if e.arpeggio == scoreart_model::Arpeggio::Bracket {
                    o.line(ax, y0, ax, y1, g::THIN_BAR, Some(ev));
                    o.line(ax, y0, ax + 0.5, y0, g::THIN_BAR, Some(ev));
                    o.line(ax, y1, ax + 0.5, y1, g::THIN_BAR, Some(ev));
                } else {
                    let mut yy = y0;
                    let mut left = true;
                    while yy < y1 {
                        let ny = (yy + 0.45).min(y1);
                        let (xa, xb) = if left {
                            (ax - 0.22, ax + 0.22)
                        } else {
                            (ax + 0.22, ax - 0.22)
                        };
                        o.line(xa, yy, xb, ny, 0.16, Some(ev));
                        left = !left;
                        yy = ny;
                    }
                    let arrow = match e.arpeggio {
                        scoreart_model::Arpeggio::Up => {
                            crate::smufl::get("arrowheadBlackUp").map(|a| (a, y0 - 0.1))
                        }
                        scoreart_model::Arpeggio::Down => {
                            crate::smufl::get("arrowheadBlackDown").map(|a| (a, y1 + 0.1))
                        }
                        _ => None,
                    };
                    if let Some(((c, w, lo, hi), at)) = arrow {
                        let base = if e.arpeggio == scoreart_model::Arpeggio::Up {
                            at + lo
                        } else {
                            at + hi
                        };
                        o.push(
                            Shape::Glyph {
                                c,
                                x: ax - w * 0.35,
                                y: base,
                                scale: 0.7,
                            },
                            Some(ev),
                        );
                    }
                }
            }
            // Grace notes before the chord: small, stem up, slashed for acciaccaturas; graces
            // after it (a trill's turn) right of the heads and dots.
            let ngr = e.grace.iter().filter(|g| !g.after).count();
            let (mut bi, mut ai) = (0usize, 0usize);
            for gr in e.grace.iter() {
                let gx = if gr.after {
                    ai += 1;
                    left_edge + head_w + 0.45 + f32::from(e.duration.dots) * 0.5 + (ai - 1) as f32 * 1.15
                } else {
                    bi += 1;
                    left_edge
                        - acc_room
                        - if e.arpeggio != scoreart_model::Arpeggio::None {
                            1.3
                        } else {
                            0.0
                        }
                        - (ngr + 1 - bi) as f32 * 1.15
                        - 0.2
                };
                const S: f32 = 0.62;
                let mut top_y = f32::MAX;
                let mut bot_y = f32::MIN;
                for p in &gr.pitches {
                    let shift =
                        f32::from(score.ottava_at(d.part, d.staff, d.measure, gg_onset(&geos, gg))) * 3.5;
                    let y = d.top + note_y(d.clef, *p) + shift;
                    top_y = top_y.min(y);
                    bot_y = bot_y.max(y);
                    o.push(
                        Shape::Glyph {
                            c: g::HEAD_BLACK,
                            x: gx,
                            y,
                            scale: S,
                        },
                        Some(ev),
                    );
                    let mut ly = d.top - 1.0;
                    while ly >= y - 0.01 && ly > d.top - 20.0 {
                        o.line(
                            gx - 0.25,
                            ly,
                            gx + g::HEAD_W * S + 0.25,
                            ly,
                            g::LEDGER * 0.8,
                            Some(ev),
                        );
                        ly -= 1.0;
                    }
                    let mut ly = d.top + 5.0;
                    while ly <= y + 0.01 && ly < d.top + 24.0 {
                        o.line(
                            gx - 0.25,
                            ly,
                            gx + g::HEAD_W * S + 0.25,
                            ly,
                            g::LEDGER * 0.8,
                            Some(ev),
                        );
                        ly += 1.0;
                    }
                }
                if top_y <= bot_y {
                    let sx = gx + g::HEAD_W * S - g::STEM * 0.4;
                    let end = top_y - 2.5;
                    o.line(sx, bot_y - 0.1, sx, end, g::STEM * 0.8, Some(ev));
                    if let Some((c, _)) = g::flag(gr.value.beams().max(1), true) {
                        o.push(
                            Shape::Glyph {
                                c,
                                x: sx - g::STEM * 0.4,
                                y: end,
                                scale: S,
                            },
                            Some(ev),
                        );
                    }
                    if gr.slash {
                        o.line(sx - 0.55, end + 1.25, sx + 0.75, end + 0.35, 0.12, Some(ev));
                    }
                }
            }
            // Marks: near the head opposite the stem, others above the staff, breaths after.
            let multi_side = multi.then_some(vi % 2 == 0);
            let near_above = multi_side.unwrap_or(!gg.up);
            let mut near: Vec<scoreart_model::Articulation> = e
                .articulations
                .iter()
                .copied()
                .filter(|a| a.def().place == scoreart_model::marks::Place::Near)
                .collect();
            near.sort_by_key(|a| {
                if matches!(a.id(), "staccato" | "tenuto" | "portato") || a.id().starts_with("staccatissimo")
                {
                    0
                } else {
                    1
                }
            });
            let mut up_cursor = (min_y - 0.75).min(if gg.up && gg.value.has_stem() {
                gg.stem_end - 0.5
            } else {
                f32::MAX
            });
            let mut down_cursor = (max_y + 0.75).max(if !gg.up && gg.value.has_stem() {
                gg.stem_end + 0.5
            } else {
                f32::MIN
            });
            for a in &near {
                let Some((c, w, lo, hi)) = g::mark(*a, near_above) else {
                    continue;
                };
                let ax = gg.x + head_w / 2.0 - w / 2.0;
                let h = hi - lo;
                if near_above {
                    let mut bottom = up_cursor;
                    if h < 0.7 {
                        bottom = snap_space(bottom - h / 2.0, d.top, -1.0) + h / 2.0;
                    }
                    o.glyph(c, ax, bottom + lo, Some(ev));
                    up_cursor = bottom - h - 0.3;
                } else {
                    let mut topy = down_cursor;
                    if h < 0.7 {
                        topy = snap_space(topy + h / 2.0, d.top, 1.0) - h / 2.0;
                    }
                    o.glyph(c, ax, topy + hi, Some(ev));
                    down_cursor = topy + h + 0.3;
                }
            }
            let mut above = up_cursor
                .min(d.top - 0.9)
                .min(if near_above { up_cursor } else { min_y - 1.0 });
            let mut above_marks: Vec<scoreart_model::Articulation> = e
                .articulations
                .iter()
                .copied()
                .filter(|a| a.def().place == scoreart_model::marks::Place::Above)
                .collect();
            // Fermatas outermost.
            above_marks.sort_by_key(|a| a.is_fermata());
            for a in &above_marks {
                let Some((c, w, lo, hi)) = g::mark(*a, true) else {
                    continue;
                };
                let ax = gg.x + head_w / 2.0 - w / 2.0;
                o.glyph(c, ax, above + lo, Some(ev));
                above -= hi - lo + 0.35;
            }
            // Fingerings above everything on the note.
            let mut fy = above - 0.2;
            for (ni, n) in notes.iter().enumerate().rev() {
                if let Some(f) = &n.fingering {
                    let hx = gg.heads.get(ni).map(|h| h.2).unwrap_or(gg.x);
                    let sh =
                        o.f.shape(TextKind::Fingering, f, hx + head_w / 2.0, fy, 1.6, Align::Center);
                    o.push(sh, Some(ev));
                    fy -= o.f.size(TextKind::Fingering, 1.4);
                }
            }
            for a in e
                .articulations
                .iter()
                .filter(|a| a.def().place == scoreart_model::marks::Place::Breath)
            {
                if let Some((c, _, lo, hi)) = g::mark(*a, true) {
                    let x = right_edge + 0.55;
                    let base = if a.id().starts_with("caesura") {
                        d.top + 1.6 + lo.min(0.0)
                    } else {
                        d.top - 0.4 + lo
                    };
                    let _ = hi;
                    o.glyph(c, x, base, Some(ev));
                }
            }
            for a in &e.articulations {
                let place = a.def().place;
                if !matches!(
                    place,
                    scoreart_model::marks::Place::Before | scoreart_model::marks::Place::After
                ) {
                    continue;
                }
                let Some((c, w, _, _)) = g::mark(*a, true) else {
                    continue;
                };
                if place == scoreart_model::marks::Place::Before {
                    o.glyph(c, left_edge - acc_room - w - 0.2, max_y, Some(ev));
                } else {
                    o.glyph(c, right_edge + 0.25, min_y, Some(ev));
                }
            }
            anchors.insert(
                ev,
                EvAnchor {
                    page: o.page_index,
                    system: d.system,
                    x_left: left_edge,
                    x_right: right_edge,
                    top: min_y,
                    bottom: max_y,
                    up: gg.up,
                    stem_end: if gg.value.has_stem() {
                        gg.stem_end
                    } else if gg.up {
                        min_y
                    } else {
                        max_y
                    },
                    staff_top: d.top,
                },
            );
            draw_tuplet(o, gg, e, &geos, v, d.top);
        }
        // Dynamics and lyrics under the staff (whole voice, also on rests).
        // Per verse: where the last hyphenated syllable ended (x, y).
        let mut hyphen_from: Vec<Option<(f32, f32)>> = Vec::new();
        // Staff texts stand over the chord symbols of the measure: above its highest note.
        let chord_row_used = v.events.iter().any(|e| e.chord_symbol.is_some());
        let chord_row = geos
            .iter()
            .filter_map(|gg| anchors.get(&gg.ev).map(|a| a.top.min(a.stem_end)))
            .fold(d.top, f32::min)
            .min(d.top)
            - 1.0;
        let chord_row = chord_row.min(d.top - CHORD_ABOVE);
        for (gg, e) in geos.iter().zip(v.events.iter()) {
            if let Some(dy) = e.dynamic {
                let place = e.dynamic_place.unwrap_or_default();
                let x0 = gg.x - 0.2 + place.dx_sp();
                let mut x = x0;
                let low = anchors
                    .get(&gg.ev)
                    .map(|a| a.bottom.max(if a.up { a.bottom } else { a.stem_end }))
                    .unwrap_or(d.top);
                let y = (d.top + 4.0 + 3.0).max(low + 2.6) + place.dy_sp();
                for ch in dy.id().chars() {
                    if let Some((c, w)) = g::dynamic_letter(ch) {
                        o.glyph(c, x, y, Some(gg.ev));
                        x += w - 0.1;
                    }
                }
                o.hit(x0 - 0.2, y - 1.6, x + 0.3, y + 0.5, Target::Dynamic { ev: gg.ev });
            }
            if let Some(cs) = &e.chord_symbol {
                // Just above the staff, or clear of the notes under the symbol (as far as it
                // reaches to the right).
                let text = chord_text(cs, o.f.german);
                let w = styled_width(&o.f, TextKind::ChordSymbol, &text, 2.2);
                let local = geos
                    .iter()
                    .filter(|h| h.x >= gg.x - 0.6 && h.x <= gg.x + w)
                    .filter_map(|h| anchors.get(&h.ev).map(|a| a.top.min(a.stem_end)))
                    .fold(d.top, f32::min);
                let y = (local - 1.0).min(d.top - CHORD_ABOVE);
                let sh = o.f.shape(TextKind::ChordSymbol, &text, gg.x, y, 2.2, Align::Left);
                o.push(sh, Some(gg.ev));
            }
            if let Some(t) = &e.text {
                let reach = anchors
                    .get(&gg.ev)
                    .map(|a| a.top.min(a.stem_end))
                    .unwrap_or(d.top)
                    - 1.4;
                // Above the chord symbols when the measure has any.
                let y = match chord_row_used {
                    true => chord_row - 2.6,
                    false => reach.min(d.top - 2.2),
                };
                // Moved by the user: below the staff and/or by an offset.
                // A moved text is measured from MuseScore's base: just above (or below) the staff.
                let place = e.text_place.unwrap_or_default();
                let y = match e.text_place {
                    Some(p) if p.below => d.top + 4.0 + 3.0,
                    Some(_) => d.top - 1.5,
                    None => y,
                } + place.dy_sp();
                let w = styled_width(&o.f, TextKind::StaffText, t, 1.9);
                // Kept inside the system (an unmoved text never runs off the page).
                let mut text_x = gg.x + place.dx_sp();
                if place.dx == 0 {
                    text_x = text_x.min(d.sys_x1 - w);
                }
                o.hit(
                    text_x - 0.2,
                    y - 1.6,
                    text_x + w + 0.2,
                    y + 0.5,
                    Target::Text { ev: gg.ev },
                );
                let sh = o.f.shape(TextKind::StaffText, t, text_x, y, 1.9, Align::Left);
                o.push(sh, Some(gg.ev));
            }
            for (verse, l) in e.lyrics.iter().enumerate().filter(|(_, l)| !l.is_empty()) {
                let y = d.top + 4.0 + d.lyrics + lyric_line(&o.f) * verse as f32;
                let cx = gg.x + 0.59;
                let (number, l) = verse_number(l);
                if let Some(n) = number {
                    let x = cx - syllable_width(&o.f, l) / 2.0 - 0.5;
                    o.styled(TextKind::Lyrics, n, x, y, LYRIC_SIZE, Align::Right);
                }
                // A pending hyphen of this verse sits between the previous syllable and this one.
                if let Some((from, py)) = hyphen_from.get_mut(verse).and_then(Option::take) {
                    let left = from;
                    let right = cx - syllable_width(&o.f, l) / 2.0;
                    if right - left > 0.6 {
                        o.styled(
                            TextKind::Lyrics,
                            "-",
                            (left + right) / 2.0,
                            py,
                            LYRIC_SIZE,
                            Align::Center,
                        );
                    }
                }
                let text = l.strip_suffix('-').filter(|t| !t.is_empty()).unwrap_or(l);
                let sh =
                    o.f.shape(TextKind::Lyrics, text, cx, y, LYRIC_SIZE, Align::Center);
                o.push(sh, Some(gg.ev));
                if l.ends_with('-') && text.len() < l.len() {
                    if hyphen_from.len() <= verse {
                        hyphen_from.resize(verse + 1, None);
                    }
                    if let Some(h) = hyphen_from.get_mut(verse) {
                        *h = Some((cx + syllable_width(&o.f, text) / 2.0, y));
                    }
                }
            }
        }
        // Hyphens whose next syllable is in a later measure: right after the syllable.
        for (from, y) in hyphen_from.into_iter().flatten() {
            o.styled(TextKind::Lyrics, "-", from + 1.0, y, LYRIC_SIZE, Align::Center);
        }
        let _ = d.x1;
    }
}

/// Onset of an event from its geometry list position.
fn gg_onset(geos: &[EvGeo], gg: &EvGeo) -> u32 {
    let _ = geos;
    gg.onset
}

/// Chord symbols with real sharps and flats; with German names a B is written H and a B flat
/// B (for the root and the bass note).
fn chord_text(s: &str, german: bool) -> String {
    let s = if german { german_chord(s) } else { s.to_string() };
    let mut out = String::with_capacity(s.len());
    let mut prev_letter = false;
    for c in s.chars() {
        let mapped = match c {
            '#' => '♯',
            'b' if prev_letter => '♭',
            c => c,
        };
        prev_letter = matches!(c, 'A'..='G');
        out.push(mapped);
    }
    out
}

fn german_chord(s: &str) -> String {
    // A note name starts the symbol and follows a slash (the bass).
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    let mut at_name = true;
    while let Some(c) = chars.next() {
        if at_name && c == 'B' {
            if chars.peek() == Some(&'b') {
                chars.next();
                out.push('B');
            } else {
                out.push('H');
            }
        } else {
            out.push(c);
        }
        at_name = c == '/';
    }
    out
}

/// Where an event was drawn, for lines (slurs, hairpins …).
#[derive(Clone, Copy, Debug)]
pub(crate) struct EvAnchor {
    page: usize,
    system: usize,
    x_left: f32,
    x_right: f32,
    top: f32,
    bottom: f32,
    up: bool,
    stem_end: f32,
    staff_top: f32,
}

/// Moves an articulation's y into a staff space (never on a line) inside the staff.
fn snap_space(y: f32, top: f32, dir: f32) -> f32 {
    let rel = y - top;
    if (0.0..=4.0).contains(&rel) {
        let r = (rel * 2.0).round() as i32;
        let half = if r % 2 == 0 {
            r as f32 / 2.0 + 0.5 * dir
        } else {
            r as f32 / 2.0
        };
        top + half.clamp(-0.5, 4.5)
    } else {
        y
    }
}

/// The tuplet groups of a voice as first and last event index. A group is full once it lasts
/// `normal` of its shortest note value (three triplet eighths last two eighths).
fn tuplet_groups(v: &scoreart_model::Voice) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut open: Option<(usize, scoreart_model::Tuplet, u32, u32)> = None;
    for (i, ev) in v.events.iter().enumerate() {
        let Some(tu) = ev.duration.tuplet else {
            if let Some((s, ..)) = open.take() {
                out.push((s, i - 1));
            }
            continue;
        };
        let (start, acc, unit) = match open {
            Some((s, t, acc, unit)) if t == tu => (s, acc, unit),
            _ => {
                if let Some((s, ..)) = open.take() {
                    out.push((s, i - 1));
                }
                (i, 0, u32::MAX)
            }
        };
        let acc = acc + ev.ticks();
        let unit = unit.min(ev.duration.value.ticks());
        if acc >= unit.saturating_mul(u32::from(tu.normal)) {
            out.push((start, i));
            open = None;
        } else {
            open = Some((start, tu, acc, unit));
        }
    }
    if let Some((s, ..)) = open {
        out.push((s, v.events.len() - 1));
    }
    out
}

/// Tuplet number centred over a tuplet group, drawn once at the group's first event.
fn draw_tuplet(o: &mut Out, gg: &EvGeo, e: &Event, geos: &[EvGeo], v: &scoreart_model::Voice, top: f32) {
    let Some(tu) = e.duration.tuplet else { return };
    let idx = gg.ev.index;
    let Some(end) = tuplet_groups(v).into_iter().find(|g| g.0 == idx).map(|g| g.1) else {
        return;
    };
    let last = geos
        .iter()
        .skip(idx)
        .take_while(|x| x.ev.index <= end)
        .last()
        .unwrap_or(gg);
    let cx = (gg.x + last.x + g::HEAD_W) / 2.0;
    let up = gg.up || gg.rest;
    let y = if up {
        let ends = gg.stem_end.min(last.stem_end).min(top);
        ends.min(gg.heads.iter().map(|h| h.1).fold(top, f32::min)) - 0.8
    } else {
        gg.stem_end.max(last.stem_end).max(top + 4.0) + 2.2
    };
    let digits = tu.actual.to_string();
    let w = digits.len() as f32 * 1.2;
    let mut x = cx - w / 2.0;
    for ch in digits.chars() {
        o.push(
            Shape::Glyph {
                c: g::digit(ch.to_digit(10).unwrap_or(3), g::TUPLET_0),
                x,
                y,
                scale: 0.8,
            },
            Some(gg.ev),
        );
        x += 1.0;
    }
}

fn draw_ties(ctx: &Ctx, out: &mut Layout, last_page: &mut Page, last_index: usize) {
    let score = ctx.score;
    let mut shapes: Vec<(usize, Shape, EventRef)> = Vec::new();
    for (pi, part) in score.parts.iter().enumerate() {
        for (mi, pm) in part.measures.iter().enumerate() {
            for (si, sm) in pm.staves.iter().enumerate() {
                for (vi, v) in sm.voices.iter().enumerate() {
                    let multi = sm.voices.len() > 1;
                    for (ei, e) in v.events.iter().enumerate() {
                        let ev = EventRef {
                            part: pi,
                            staff: si,
                            voice: vi,
                            measure: mi,
                            index: ei,
                        };
                        let notes = e.notes();
                        let n = notes.len();
                        for (ni, note) in notes.iter().enumerate() {
                            if !note.tie {
                                continue;
                            }
                            let Some(a) = ctx.heads.get(&(ev, note.pitch)) else {
                                continue;
                            };
                            let Some(next) = score.next_event(ev) else {
                                continue;
                            };
                            let target = ctx.heads.get(&(next, note.pitch));
                            // Direction: voices by number; chords split; single notes opposite the stem.
                            let up = if multi {
                                vi % 2 == 0
                            } else if n > 1 {
                                ni * 2 + 1 > n || (ni * 2 + 1 == n && !a.up)
                            } else {
                                !a.up
                            };
                            match target {
                                Some(b) if b.page == a.page && b.system == a.system => {
                                    shapes.push((
                                        a.page,
                                        tie_shape(a.x_right + 0.15, b.x_left - 0.15, a.y, up),
                                        ev,
                                    ));
                                }
                                Some(b) => {
                                    if let Some(end) = ctx.system_ends.get(a.system) {
                                        shapes.push((
                                            a.page,
                                            tie_shape(a.x_right + 0.15, end.1 - 0.3, a.y, up),
                                            ev,
                                        ));
                                    }
                                    if let Some(start) = ctx.system_ends.get(b.system) {
                                        shapes.push((
                                            b.page,
                                            tie_shape(start.2 - 1.2, b.x_left - 0.15, b.y, up),
                                            ev,
                                        ));
                                    }
                                }
                                None => {}
                            }
                        }
                    }
                }
            }
        }
    }
    for (page, shape, ev) in shapes {
        if page == last_index {
            last_page.items.push(Item {
                shape,
                owner: Some(ev),
            });
        } else if let Some(p) = out.pages.get_mut(page) {
            p.items.push(Item {
                shape,
                owner: Some(ev),
            });
        }
    }
}

fn tie_shape(xa: f32, xb: f32, y: f32, up: bool) -> Shape {
    let s = if up { -1.0 } else { 1.0 };
    let len = (xb - xa).max(0.5);
    let h = (0.3 + 0.08 * len).clamp(0.45, 1.3);
    let y0 = y + s * 0.4;
    Shape::Curve {
        p: [
            (xa, y0),
            (xa + len * 0.25, y0 + s * h),
            (xb - len * 0.25, y0 + s * h),
            (xb, y0),
        ],
        thick: g::TIE_MID,
    }
}

/// How far the notes of one staff reach above the top line and below the bottom line in the
/// given measures (sp, ≥ 0), counting stems and marks roughly.
fn staff_reach(score: &Score, part: usize, staff: usize, measures: &[usize]) -> (f32, f32) {
    let mut up = 0.0f32;
    let mut down = 0.0f32;
    let mut chords = false;
    let mut chord_up = 0.0f32;
    let mut texts = false;
    for &m in measures {
        let clef = score.clef_at(part, staff, m);
        let Some(sm) = score.staff_measure(part, staff, m) else {
            continue;
        };
        let voices = sm.voices.iter().filter(|v| !v.events.is_empty()).count();
        for (vi, v) in sm.voices.iter().enumerate() {
            for (e, on) in v.events.iter().zip(v.onsets()) {
                let shift = f32::from(score.ottava_at(part, staff, m, on)) * 3.5;
                let marks = if e.articulations.is_empty() { 0.0 } else { 1.6 };
                let above_marks = e
                    .articulations
                    .iter()
                    .filter(|a| a.def().place == scoreart_model::marks::Place::Above)
                    .count() as f32
                    * 1.7;
                let finger = if e.notes().iter().any(|n| n.fingering.is_some()) {
                    1.4
                } else {
                    0.0
                };
                // Stem direction as drawn: by voice when several sound, else away from the
                // middle line (by the note farthest from it).
                let ys: Vec<f32> = e.notes().iter().map(|n| note_y(clef, n.pitch) + shift).collect();
                let far =
                    ys.iter().copied().fold(
                        2.0f32,
                        |a, y| if (y - 2.0).abs() > (a - 2.0).abs() { y } else { a },
                    );
                let stem_up = if voices > 1 { vi % 2 == 0 } else { far >= 2.0 };
                let stem = e.duration.value.has_stem();
                for &y in &ys {
                    let stem_top = if stem && stem_up { y - STEM_LEN } else { y - 0.5 };
                    let stem_bottom = if stem && !stem_up { y + STEM_LEN } else { y + 0.5 };
                    up = up
                        .max(-stem_top + marks)
                        .max(-y + 1.0 + marks + above_marks + finger);
                    down = down.max(stem_bottom - 4.0);
                }
                up = up.max(above_marks + 1.0);
                if e.chord_symbol.is_some() {
                    // The symbol stands over this event's notes (stems, marks).
                    let local = ys
                        .iter()
                        .map(|&y| {
                            let stem_top = if stem && stem_up { y - STEM_LEN } else { y - 0.5 };
                            (-stem_top).max(0.0) + marks
                        })
                        .fold(0.0f32, f32::max);
                    chord_up = chord_up.max(local);
                }
                chords |= e.chord_symbol.is_some();
                texts |= e.text.is_some();
            }
        }
    }
    // Lines above and below the staff.
    for sp in score
        .spanners
        .iter()
        .filter(|s| s.part == part && s.staff == staff)
    {
        if !measures
            .iter()
            .any(|m| (sp.start.measure..=sp.end.measure).contains(m))
        {
            continue;
        }
        match sp.kind {
            SpannerKind::Ottava(o) if o > 0 => up = up.max(up + 2.5),
            SpannerKind::TrillLine | SpannerKind::Vibrato | SpannerKind::LetRing | SpannerKind::PalmMute => {
                up = up.max(3.5)
            }
            SpannerKind::Ottava(_) | SpannerKind::Pedal => down = down.max(down + 2.5),
            SpannerKind::Crescendo
            | SpannerKind::Diminuendo
            | SpannerKind::CrescendoLine
            | SpannerKind::DiminuendoLine => down = down.max(3.6),
            _ => {}
        }
    }
    // Chord symbols sit just above the staff, or clear of the notes (as MuseScore places
    // them), staff texts above them.
    let f = Fonts::of(score);
    let mut reach = up;
    if chords {
        reach = up.max((chord_up + 1.0).max(CHORD_ABOVE) + f.size(TextKind::ChordSymbol, 2.2) * 0.75);
    }
    if texts {
        reach += f.size(TextKind::StaffText, 1.9) * 0.8 + 0.6;
    }
    (reach, down)
}

/// Baseline of chord symbols above the top staff line when no note is in the way (staff
/// spaces; MuseScore's default puts the text's top 2.5 spaces above the staff).
const CHORD_ABOVE: f32 = 0.7;

/// One piece of a line on one system.
#[derive(Clone, Copy, Debug)]
struct Seg {
    page: usize,
    x0: f32,
    x1: f32,
    top: f32,
    first: bool,
    last: bool,
}

fn segments(ctx: &Ctx, staves: &[StaffBox], a: EvAnchor, b: EvAnchor, part: usize, staff: usize) -> Vec<Seg> {
    if a.system == b.system {
        return vec![Seg {
            page: a.page,
            x0: a.x_left,
            x1: b.x_right,
            top: a.staff_top,
            first: true,
            last: true,
        }];
    }
    let mut out = Vec::new();
    let end_of = |sys: usize| ctx.system_ends.get(sys).map(|e| e.1).unwrap_or(a.x_right);
    let start_of = |sys: usize| ctx.system_ends.get(sys).map(|e| e.2 - 0.8).unwrap_or(b.x_left);
    out.push(Seg {
        page: a.page,
        x0: a.x_left,
        x1: end_of(a.system),
        top: a.staff_top,
        first: true,
        last: false,
    });
    for k in (a.system + 1)..b.system.min(a.system + 200) {
        if let Some(sb) = staves
            .iter()
            .find(|s| s.system == k && s.part == part && s.staff == staff)
        {
            out.push(Seg {
                page: sb.page,
                x0: start_of(k),
                x1: end_of(k),
                top: sb.top,
                first: false,
                last: false,
            });
        }
    }
    out.push(Seg {
        page: b.page,
        x0: start_of(b.system),
        x1: b.x_right,
        top: b.staff_top,
        first: false,
        last: true,
    });
    out
}

fn dashed(items: &mut Vec<(usize, Shape)>, page: usize, x0: f32, x1: f32, y: f32) {
    let mut x = x0;
    let mut n = 0;
    while x < x1 && n < 2000 {
        let e = (x + 0.9).min(x1);
        items.push((
            page,
            Shape::Line {
                x1: x,
                y1: y,
                x2: e,
                y2: y,
                width: 0.16,
            },
        ));
        x = e + 0.6;
        n += 1;
    }
}

fn glyph_run(items: &mut Vec<(usize, Shape)>, page: usize, name: &str, x0: f32, x1: f32, y: f32) {
    let Some((c, w, _, _)) = crate::smufl::get(name) else {
        return;
    };
    let step = w.max(0.2);
    let mut x = x0;
    let mut n = 0;
    while x + step <= x1 + 0.01 && n < 2000 {
        items.push((page, Shape::Glyph { c, x, y, scale: 1.0 }));
        x += step;
        n += 1;
    }
}

/// Moves a shape up or down.
fn shift_y(sh: &mut Shape, dy: f32) {
    match sh {
        Shape::Glyph { y, .. } | Shape::Rect { y, .. } | Shape::Text { y, .. } => *y += dy,
        Shape::Line { y1, y2, .. } => {
            *y1 += dy;
            *y2 += dy;
        }
        Shape::Quad { p } | Shape::Curve { p, .. } => p.iter_mut().for_each(|q| q.1 += dy),
    }
}

/// A rough box around a shape (x0, y0, x1, y1).
fn bounds(sh: &Shape) -> Option<(f32, f32, f32, f32)> {
    let pts = |p: &[(f32, f32)]| {
        p.iter().fold(None, |b: Option<(f32, f32, f32, f32)>, q| {
            Some(match b {
                None => (q.0, q.1, q.0, q.1),
                Some(o) => (o.0.min(q.0), o.1.min(q.1), o.2.max(q.0), o.3.max(q.1)),
            })
        })
    };
    match sh {
        Shape::Glyph { x, y, scale, .. } => {
            Some((*x - 0.3, y - 2.0 * scale, x + 2.5 * scale, y + 1.0 * scale))
        }
        Shape::Line { x1, y1, x2, y2, .. } => pts(&[(*x1, *y1), (*x2, *y2)]),
        Shape::Rect { x, y, w, h } => Some((*x, *y, x + w, y + h)),
        Shape::Quad { p } | Shape::Curve { p, .. } => pts(p),
        Shape::Text {
            text,
            x,
            y,
            size,
            align,
            ..
        } => {
            let w = text_width(text, *size);
            let x0 = match align {
                Align::Left => *x,
                Align::Center => x - w / 2.0,
                Align::Right => x - w,
            };
            Some((x0, y - size, x0 + w, *y + 0.3))
        }
    }
}

fn draw_spanners(ctx: &Ctx, out: &mut Layout, last_page: &mut Page, last_index: usize) {
    let score = ctx.score;
    let mut items: Vec<(usize, Shape, EventRef)> = Vec::new();
    for (si, sp) in score.spanners.iter().enumerate() {
        let pos = |a: scoreart_model::Anchor| scoreart_model::edit::Pos {
            part: sp.part,
            staff: sp.staff,
            voice: sp.voice,
            measure: a.measure,
            tick: a.tick,
        };
        let (Some(ea), Some(eb)) = (pos(sp.start).event(score), pos(sp.end).event(score)) else {
            continue;
        };
        let (Some(a), Some(b)) = (ctx.events.get(&ea).copied(), ctx.events.get(&eb).copied()) else {
            continue;
        };
        let segs = segments(ctx, &out.staves, a, b, sp.part, sp.staff);
        let mut local: Vec<(usize, Shape)> = Vec::new();
        match sp.kind {
            SpannerKind::Slur => {
                let multi = score
                    .staff_measure(sp.part, sp.staff, sp.start.measure)
                    .is_some_and(|s| s.voices.len() > 1);
                let above = if multi { sp.voice % 2 == 0 } else { !a.up };
                let end_y = |x: &EvAnchor| {
                    if above {
                        if x.up { x.stem_end - 0.3 } else { x.top - 0.9 }
                    } else if !x.up {
                        x.stem_end + 0.3
                    } else {
                        x.bottom + 0.9
                    }
                };
                // Inner notes the slur must clear.
                let mut inner: Vec<EvAnchor> = Vec::new();
                let mut cur = ea;
                for _ in 0..128 {
                    let Some(n) = score.next_event(cur) else { break };
                    if n == eb {
                        break;
                    }
                    if let Some(x) = ctx.events.get(&n) {
                        inner.push(*x);
                    }
                    cur = n;
                }
                let (ya, yb) = (end_y(&a), end_y(&b));
                for s in &segs {
                    let xa = if s.first { a.x_left + 0.55 } else { s.x0 };
                    let xb = if s.last { b.x_right - 0.55 } else { s.x1 };
                    let (y0, y1) = match (s.first, s.last) {
                        (true, true) => (ya, yb),
                        (true, false) => (ya, ya),
                        (false, true) => (yb, yb),
                        _ => (
                            s.top + if above { -1.0 } else { 5.0 },
                            s.top + if above { -1.0 } else { 5.0 },
                        ),
                    };
                    let len = (xb - xa).max(0.5);
                    let mut h = (0.5 + 0.06 * len).clamp(0.7, 2.4);
                    for x in inner.iter().filter(|x| x.staff_top == s.top) {
                        let cx = (x.x_left + x.x_right) / 2.0;
                        if cx <= xa || cx >= xb {
                            continue;
                        }
                        let line = y0 + (y1 - y0) * (cx - xa) / len;
                        let obstacle = if above {
                            x.top.min(if x.up { x.stem_end } else { x.top }) - 0.8
                        } else {
                            x.bottom.max(if x.up { x.bottom } else { x.stem_end }) + 0.8
                        };
                        let need = if above { line - obstacle } else { obstacle - line };
                        if need > h * 0.8 {
                            h = (need / 0.8).min(6.0);
                        }
                    }
                    let sgn = if above { -1.0 } else { 1.0 };
                    let c = h / 0.75 * sgn;
                    local.push((
                        s.page,
                        Shape::Curve {
                            p: [
                                (xa, y0),
                                (xa + len * 0.25, y0 + c),
                                (xb - len * 0.25, y1 + c),
                                (xb, y1),
                            ],
                            thick: 0.2,
                        },
                    ));
                }
            }
            SpannerKind::Crescendo | SpannerKind::Diminuendo => {
                let cresc = sp.kind == SpannerKind::Crescendo;
                let dyn_room = |r: EventRef| {
                    score
                        .event(r)
                        .and_then(|e| e.dynamic)
                        .map(|d| d.id().len() as f32 * 1.3 + 0.4)
                        .unwrap_or(0.0)
                };
                for s in &segs {
                    let low = |x: &EvAnchor| x.bottom.max(if x.up { x.bottom } else { x.stem_end });
                    let y = (s.top + 6.6)
                        .max(if s.first { low(&a) + 2.2 } else { 0.0 })
                        .max(if s.last { low(&b) + 2.2 } else { 0.0 });
                    let x0 = if s.first { a.x_left + dyn_room(ea) } else { s.x0 };
                    // It ends before a dynamic on its last note (a hairpin "into" the f).
                    let x1 = if !s.last {
                        s.x1 - 0.4
                    } else if dyn_room(eb) > 0.0 {
                        (b.x_left - 0.8).max(x0 + 1.5)
                    } else {
                        b.x_right + 0.6
                    };
                    let (o0, o1) = match (cresc, s.first, s.last) {
                        (true, f, l) => (if f { 0.0 } else { 0.3 }, if l { 0.6 } else { 0.45 }),
                        (false, f, l) => (if f { 0.6 } else { 0.45 }, if l { 0.0 } else { 0.3 }),
                    };
                    // Opened wider or narrower by the user.
                    let k = if sp.spread > 0 {
                        f32::from(sp.spread) / 12.0
                    } else {
                        1.0
                    };
                    let (o0, o1) = (o0 * k, o1 * k);
                    local.push((
                        s.page,
                        Shape::Line {
                            x1: x0,
                            y1: y - o0,
                            x2: x1,
                            y2: y - o1,
                            width: 0.16,
                        },
                    ));
                    local.push((
                        s.page,
                        Shape::Line {
                            x1: x0,
                            y1: y + o0,
                            x2: x1,
                            y2: y + o1,
                            width: 0.16,
                        },
                    ));
                }
            }
            SpannerKind::CrescendoLine | SpannerKind::DiminuendoLine => {
                let word = if sp.kind == SpannerKind::CrescendoLine {
                    "cresc."
                } else {
                    "dim."
                };
                for s in &segs {
                    let y = (s.top + 7.0).max(if s.first { a.bottom + 2.8 } else { 0.0 });
                    let mut x0 = s.x0;
                    if s.first {
                        local.push((
                            s.page,
                            Fonts::of(score).shape(TextKind::StaffText, word, a.x_left, y, 1.9, Align::Left),
                        ));
                        x0 = a.x_left
                            + text_width(word, Fonts::of(score).size(TextKind::StaffText, 1.9))
                            + 0.4;
                    }
                    dashed(
                        &mut local,
                        s.page,
                        x0,
                        if s.last { b.x_right + 0.4 } else { s.x1 - 0.4 },
                        y - 0.5,
                    );
                }
            }
            SpannerKind::Ottava(o) => {
                let (alta, cont) = match o {
                    1 => ("ottavaAlta", "ottava"),
                    -1 => ("ottavaBassaVb", "ottava"),
                    2 => ("quindicesimaAlta", "quindicesima"),
                    -2 => ("quindicesimaBassa", "quindicesima"),
                    3 => ("ventiduesimaAlta", "ventiduesima"),
                    _ => ("ventiduesimaBassa", "ventiduesima"),
                };
                let up = o > 0;
                for s in &segs {
                    let y = if up {
                        (s.top - 3.4)
                            .min(if s.first {
                                a.top.min(a.stem_end) - 3.0
                            } else {
                                f32::MAX
                            })
                            .min(if s.last {
                                b.top.min(b.stem_end) - 3.0
                            } else {
                                f32::MAX
                            })
                    } else {
                        (s.top + 7.0)
                            .max(if s.first {
                                a.bottom.max(a.stem_end) + 2.6
                            } else {
                                0.0
                            })
                            .max(if s.last {
                                b.bottom.max(b.stem_end) + 2.6
                            } else {
                                0.0
                            })
                    };
                    let name = if s.first { alta } else { cont };
                    let Some((c, w, _, _)) = crate::smufl::get(name) else {
                        continue;
                    };
                    let x0 = if s.first { a.x_left - 0.3 } else { s.x0 };
                    local.push((
                        s.page,
                        Shape::Glyph {
                            c,
                            x: x0,
                            y,
                            scale: if s.first { 1.0 } else { 0.8 },
                        },
                    ));
                    let ly = y - 0.8;
                    let x1 = if s.last { b.x_right + 0.3 } else { s.x1 - 0.4 };
                    dashed(&mut local, s.page, x0 + w + 0.3, x1, ly);
                    if s.last {
                        let hook = if up { ly + 1.1 } else { ly - 1.1 };
                        local.push((
                            s.page,
                            Shape::Line {
                                x1,
                                y1: ly,
                                x2: x1,
                                y2: hook,
                                width: 0.16,
                            },
                        ));
                    }
                }
            }
            SpannerKind::Pedal => {
                for s in &segs {
                    let y = (s.top + 8.6).max(if s.first { a.bottom + 4.0 } else { 0.0 });
                    if s.first
                        && let Some((c, _, _, _)) = crate::smufl::get("keyboardPedalPed")
                    {
                        local.push((
                            s.page,
                            Shape::Glyph {
                                c,
                                x: a.x_left - 0.3,
                                y,
                                scale: 1.0,
                            },
                        ));
                    }
                    if s.last
                        && let Some((c, _, _, _)) = crate::smufl::get("keyboardPedalUp")
                    {
                        local.push((
                            s.page,
                            Shape::Glyph {
                                c,
                                x: b.x_right + 0.4,
                                y,
                                scale: 1.0,
                            },
                        ));
                    }
                }
            }
            SpannerKind::TrillLine | SpannerKind::Vibrato => {
                let wiggle = if sp.kind == SpannerKind::TrillLine {
                    "wiggleTrill"
                } else {
                    "wiggleVibrato"
                };
                for s in &segs {
                    let y = (s.top - 1.2).min(if s.first {
                        a.top.min(a.stem_end) - 1.3
                    } else {
                        f32::MAX
                    });
                    let mut x0 = s.x0;
                    if s.first {
                        x0 = a.x_left;
                        if sp.kind == SpannerKind::TrillLine
                            && let Some((c, w, lo, _)) = crate::smufl::get("ornamentTrill")
                        {
                            local.push((
                                s.page,
                                Shape::Glyph {
                                    c,
                                    x: x0,
                                    y: y + lo,
                                    scale: 1.0,
                                },
                            ));
                            x0 += w + 0.2;
                        }
                    }
                    let x1 = if s.last { b.x_right + 0.6 } else { s.x1 - 0.3 };
                    glyph_run(&mut local, s.page, wiggle, x0, x1, y - 0.0);
                }
            }
            SpannerKind::LetRing | SpannerKind::PalmMute => {
                let word = if sp.kind == SpannerKind::LetRing {
                    "let ring"
                } else {
                    "P.M."
                };
                for s in &segs {
                    let y = (s.top - 1.6).min(if s.first {
                        a.top.min(a.stem_end) - 1.4
                    } else {
                        f32::MAX
                    });
                    let mut x0 = s.x0;
                    if s.first {
                        local.push((
                            s.page,
                            Fonts::of(score).shape(TextKind::StaffText, word, a.x_left, y, 1.8, Align::Left),
                        ));
                        x0 = a.x_left
                            + text_width(word, Fonts::of(score).size(TextKind::StaffText, 1.8))
                            + 0.3;
                    }
                    let x1 = if s.last { b.x_right + 0.4 } else { s.x1 - 0.4 };
                    dashed(&mut local, s.page, x0, x1, y - 0.6);
                    if s.last {
                        local.push((
                            s.page,
                            Shape::Line {
                                x1,
                                y1: y - 0.6,
                                x2: x1,
                                y2: y + 0.4,
                                width: 0.16,
                            },
                        ));
                    }
                }
            }
            SpannerKind::Glissando | SpannerKind::WavyGlissando => {
                let wavy = sp.kind == SpannerKind::WavyGlissando;
                for s in &segs {
                    let (xa, ya) = if s.first {
                        (a.x_right + 0.3, a.top)
                    } else {
                        (s.x0, b.top)
                    };
                    let (xb, yb) = if s.last {
                        (b.x_left - 0.4, b.top)
                    } else {
                        (s.x1, a.top)
                    };
                    if !wavy {
                        local.push((
                            s.page,
                            Shape::Line {
                                x1: xa,
                                y1: ya,
                                x2: xb,
                                y2: yb,
                                width: 0.16,
                            },
                        ));
                        continue;
                    }
                    // Zigzag along the line.
                    let (dx, dy) = (xb - xa, yb - ya);
                    let len = (dx * dx + dy * dy).sqrt().max(0.1);
                    let (nx, ny) = (-dy / len * 0.2, dx / len * 0.2);
                    let steps = (len / 0.35).max(2.0) as usize;
                    let mut prev = (xa, ya);
                    for i in 1..=steps.min(400) {
                        let t = i as f32 / steps as f32;
                        let sgn = if i % 2 == 0 { 1.0 } else { -1.0 };
                        let p = (xa + dx * t + nx * sgn, ya + dy * t + ny * sgn);
                        local.push((
                            s.page,
                            Shape::Line {
                                x1: prev.0,
                                y1: prev.1,
                                x2: p.0,
                                y2: p.1,
                                width: 0.14,
                            },
                        ));
                        prev = p;
                    }
                }
            }
        }
        // Moved by the user.
        let dy = f32::from(sp.dy) / 10.0;
        if dy != 0.0 {
            for (_, sh) in &mut local {
                shift_y(sh, dy);
            }
        }
        // Where it can be picked up: one box per piece (system).
        for s in &segs {
            let mut b: Option<(f32, f32, f32, f32)> = None;
            for (pg, sh) in &local {
                let Some((x0, y0, x1, y1)) = bounds(sh).filter(|_| *pg == s.page) else {
                    continue;
                };
                let mx = (x0 + x1) / 2.0;
                if mx < s.x0 - 3.0 || mx > s.x1 + 3.0 || y1 < s.top - 14.0 || y0 > s.top + 18.0 {
                    continue;
                }
                b = Some(match b {
                    None => (x0, y0, x1, y1),
                    Some(o) => (o.0.min(x0), o.1.min(y0), o.2.max(x1), o.3.max(y1)),
                });
            }
            if let Some((x0, y0, x1, y1)) = b {
                out.hits.push(Hit {
                    page: s.page,
                    x0: x0 - 0.3,
                    y0: y0 - 0.5,
                    x1: x1 + 0.3,
                    y1: y1 + 0.5,
                    target: Target::Line {
                        index: si,
                        first: s.first,
                        last: s.last,
                    },
                });
            }
        }
        items.extend(local.into_iter().map(|(p, s)| (p, s, ea)));
    }
    for (page, shape, ev) in items {
        if page == last_index {
            last_page.items.push(Item {
                shape,
                owner: Some(ev),
            });
        } else if let Some(p) = out.pages.get_mut(page) {
            p.items.push(Item {
                shape,
                owner: Some(ev),
            });
        }
    }
}
