use std::collections::{BTreeSet, HashMap};

use scoreart_model::edit::EventRef;
use scoreart_model::{
    Accidental, Barline, Clef, Event, KeySig, NoteValue, Pitch, Score, SpannerKind, TPQ, TimeSig, TimeSymbol,
};

use crate::glyphs as g;
use crate::{Align, Hit, Item, Layout, Page, Shape, StaffBox, Style, Target};

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
}

impl MeasurePlan {
    fn fixed(&self) -> f32 {
        PAD_LEFT + self.cols.iter().map(|c| c.1).sum::<f32>()
    }
    fn stretch(&self) -> f32 {
        self.cols.iter().map(|c| c.2).sum()
    }
}

const PAD_LEFT: f32 = 1.2;
const STEM_LEN: f32 = 3.5;
const TITLE_SIZE: f32 = 4.4;
const NAME_SIZE: f32 = 1.9;
const LYRIC_SIZE: f32 = 1.8;
/// Distance between the lines of different verses.
const LYRIC_LINE: f32 = 2.4;

/// Estimated width of a lyric syllable (without its joining hyphen).
fn syllable_width(l: &str) -> f32 {
    l.trim_end_matches('-').chars().count() as f32 * LYRIC_SIZE * 0.5
}

struct System {
    measures: Vec<usize>,
    /// Stretch factor applied to spaces.
    stretch: f32,
    indent: f32,
    header: f32,
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
                            let half = syllable_width(l) / 2.0;
                            if let Some(Some((pon, phalf, hyphen))) = prev_syl.get(verse).copied() {
                                let gap = if hyphen { 1.1 } else { 0.5 };
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
                            let w = text_width(&chord_text(cs), 2.2) * 0.85;
                            if let Some((pon, pw)) = prev_chord {
                                lyric_need.push((pon, on, pw + 0.8));
                            }
                            prev_chord = Some((on, w));
                        }
                    }
                    if let Some((pon, pw)) = prev_chord {
                        lyric_need.push((pon, len, pw - 0.5));
                    }
                    // The last syllable needs half its width (and its hyphen) before the barline.
                    for (pon, phalf, hyphen) in prev_syl.into_iter().flatten() {
                        lyric_need.push((pon, len, phalf + if hyphen { 1.1 } else { 0.3 }));
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
                                .map(|l| syllable_width(l) / 2.0)
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
        // Widen the columns between two syllables when they are too close.
        for (from, to, need) in lyric_need {
            let have: f32 = cols.iter().filter(|c| c.0 >= from && c.0 < to).map(|c| c.2).sum();
            if have > 0.0 && have < need {
                let k = need / have;
                for c in cols.iter_mut().filter(|c| c.0 >= from && c.0 < to) {
                    c.2 *= k;
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
        plans.push(MeasurePlan { cols, len, prefix });
    }
    plans
}

/// Room needed left of the noteheads: grace notes, arpeggio, slides in, parentheses.
fn lead_extra(e: &Event) -> f32 {
    let mut w = 0.0;
    let graces: usize = e.grace.iter().map(|_| 1).sum();
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
    w
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

fn text_width(s: &str, size: f32) -> f32 {
    s.chars().count() as f32 * size * 0.52
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

fn name_indent(score: &Score, first: bool) -> f32 {
    let w = score
        .parts
        .iter()
        .enumerate()
        .filter(|(pi, _)| shown_staves(score, *pi) > 0)
        .map(|(_, p)| p)
        .map(|p| {
            let name = if first { &p.name } else { &p.abbreviation };
            if !first && shown_parts(score) < 2 {
                0.0
            } else {
                text_width(name, NAME_SIZE)
            }
        })
        .fold(0.0f32, f32::max);
    let multi_staff = (0..score.parts.len()).any(|pi| shown_staves(score, pi) > 1) || shown_parts(score) > 1;
    let bracket = if multi_staff { 1.6 } else { 0.0 };
    if w > 0.0 { w + 1.5 + bracket } else { bracket }
}

fn break_systems(score: &Score, plans: &[MeasurePlan], width: f32) -> Vec<System> {
    let mut systems = Vec::new();
    let mut m = 0usize;
    while m < plans.len() {
        let first = systems.is_empty();
        let indent = name_indent(score, first);
        let header = header_width(score, m);
        let avail = (width - indent - header).max(10.0);
        let mut ms = Vec::new();
        let mut used = 0.0f32;
        while m < plans.len() {
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
                .is_some_and(|i| i.system_break || i.page_break)
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
        let last = m >= plans.len();
        if last && used < avail * 0.75 {
            stretch = 1.0;
        }
        systems.push(System {
            measures: ms,
            stretch,
            indent,
            header,
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

struct Out<'a> {
    page: &'a mut Page,
    page_index: usize,
    hits: &'a mut Vec<Hit>,
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
            },
            None,
        );
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

pub fn layout(score: &Score, style: &Style) -> Layout {
    let (pw, ph) = style.page_sp();
    let margin = style.margin_sp().min(pw / 4.0).min(ph / 4.0);
    let width = pw - 2.0 * margin;
    let acc = accidentals(score);
    let plans = plan_measures(score, &acc, style);
    let systems = break_systems(score, &plans, width);

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
    let mut page_index = 0usize;

    // Title block.
    let mut y = margin;
    let meta = &score.meta;
    {
        let mut o = Out {
            page: &mut page,
            page_index,
            hits: &mut out.hits,
        };
        if !meta.title.is_empty() {
            y += TITLE_SIZE;
            o.text(&meta.title, pw / 2.0, y, TITLE_SIZE, Align::Center, false, false);
            y += 1.0;
        }
        if !meta.subtitle.is_empty() {
            y += 2.4;
            o.text(&meta.subtitle, pw / 2.0, y, 2.4, Align::Center, false, false);
            y += 0.6;
        }
        if !meta.composer.is_empty() || !meta.lyricist.is_empty() {
            y += 2.2;
            o.text(&meta.composer, pw - margin, y, 1.9, Align::Right, false, false);
            o.text(&meta.lyricist, margin, y, 1.9, Align::Left, false, false);
            // Keep rehearsal marks and tempo above the first system clear of this line.
            y += 1.5;
        }
        if y > margin {
            y += 3.0;
        }
    }

    for (si, sys) in systems.iter().enumerate() {
        // Vertical plan: top of every staff relative to the system top.
        let mut tops: Vec<(usize, usize, f32)> = Vec::new();
        let mut sy = 0.0f32;
        let first_in_sys = sys.measures.first().copied();
        let has_above = sys.measures.iter().any(|m| {
            score.measures.get(*m).is_some_and(|i| {
                // A rehearsal mark starting the system sits beside the clef; one moved below
                // the staff needs no row above it.
                let rehearsal_row = i.rehearsal.is_some()
                    && Some(*m) != first_in_sys
                    && !i.rehearsal_place.is_some_and(|p| p.below);
                i.tempo.is_some()
                    || i.text.is_some()
                    || i.tempo_text.is_some()
                    || rehearsal_row
                    || !i.jumps.is_empty()
            })
        });
        if has_above {
            sy += 6.0;
        }
        if sys
            .measures
            .iter()
            .any(|m| score.measures.get(*m).is_some_and(|i| i.volta.is_some()))
        {
            sy += 2.5;
        }
        for (pi, part) in score.parts.iter().enumerate() {
            let visible: Vec<usize> = (0..part.staff_count()).filter(|s| shown(score, pi, *s)).collect();
            for &s in &visible {
                // Make room for notes (with stems and marks) far above or below the staff.
                let (reach_up, reach_down) = staff_reach(score, pi, s, &sys.measures);
                sy += (reach_up - 3.0).max(0.0);
                tops.push((pi, s, sy));
                sy += (reach_down - 3.0).max(0.0);
                // One lyric line per verse used in this system.
                let verses = sys
                    .measures
                    .iter()
                    .filter_map(|m| score.staff_measure(pi, s, *m))
                    .flat_map(|sm| sm.voices.iter())
                    .flat_map(|v| v.events.iter())
                    .map(|e| e.verse_count())
                    .max()
                    .unwrap_or(0);
                let lyrics = verses > 0;
                sy += 4.0
                    + if visible.last().is_some_and(|l| s < *l) {
                        style.staff_gap
                    } else {
                        style.part_gap
                    };
                if lyrics {
                    sy += 2.6 + LYRIC_LINE * (verses.saturating_sub(1)) as f32;
                }
            }
        }
        let sys_h = (sy - style.part_gap).max(4.0);
        // A page break set by the user after the previous system's last measure.
        let forced = si > 0
            && systems
                .get(si - 1)
                .and_then(|p| p.measures.last())
                .and_then(|m| score.measures.get(*m))
                .is_some_and(|i| i.page_break);
        if (y + sys_h > ph - margin || forced) && !page.items.is_empty() && si > 0 {
            out.pages.push(std::mem::take(&mut page));
            page = Page {
                width: pw,
                height: ph,
                items: Vec::new(),
            };
            page_index += 1;
            y = margin;
            let mut o = Out {
                page: &mut page,
                page_index,
                hits: &mut out.hits,
            };
            o.text(
                &(page_index + 1).to_string(),
                pw - margin,
                margin - 1.5,
                1.6,
                Align::Right,
                false,
                false,
            );
            y += 1.0;
        }
        let top = y;
        draw_system(
            &mut ctx, &mut page, page_index, &mut out, sys, si, &plans, margin, width, top, &tops, sys_h,
        );
        y += sys_h + style.system_gap;
    }
    // Ties need both ends placed.
    draw_ties(&ctx, &mut out, &mut page, page_index);
    draw_spanners(&ctx, &mut out, &mut page, page_index);
    out.pages.push(page);
    out
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
    let x_end = margin + width;
    let first_m = sys.measures.first().copied().unwrap_or(0);
    let mut o = Out {
        page,
        page_index,
        hits: &mut out.hits,
    };

    // Staff lines.
    for &(_, _, t) in tops {
        for l in 0..5 {
            let yy = top + t + l as f32;
            o.line(x_start, yy, x_end, yy, g::STAFF_LINE, None);
        }
    }
    let sys_top = tops.first().map(|t| top + t.2).unwrap_or(top);
    let sys_bottom = tops.last().map(|t| top + t.2 + 4.0).unwrap_or(top + 4.0);
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
        let name = if si == 0 { &part.name } else { &part.abbreviation };
        if si == 0 || shown_parts(score) > 1 {
            let nx = x_start - if multi { 3.0 } else { 1.0 } - if ensemble { 1.4 } else { 0.0 };
            o.text(
                name,
                nx,
                (pt + pb) / 2.0 + 0.7,
                NAME_SIZE,
                Align::Right,
                false,
                false,
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

    // Measure number at the start of every system but the first.
    if first_m > 0 {
        o.text(
            &score.measure_number(first_m).to_string(),
            x_start,
            sys_top - 2.0,
            1.4,
            Align::Left,
            false,
            true,
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
        for &(t, lead, sp) in &plan.cols {
            cx += lead;
            cols.push((t, cx));
            cx += sp * sys.stretch;
        }
        let m_end = cx;
        cols.push((plan.len, m_end));

        // Tempo and measure text above the top staff.
        if let Some(&(_, _, t0)) = tops.first() {
            let ty = top + t0 - (row_reach + 1.4).max(2.6);
            let mut tx = cols.first().map(|c| c.1).unwrap_or(mx);
            if let Some(r) = &info.rehearsal {
                // Boxed rehearsal mark: above the barline; at the start of a system above the
                // clef; below the staff when moved there; plus the user's offset.
                let w = text_width(r, 2.2) + 1.0;
                let staff_top = top + t0;
                let place = info.rehearsal_place.unwrap_or_default();
                let (mut bx0, mut by1) = if place.below {
                    let bottom = tops.last().map_or(staff_top + 4.0, |t| top + t.2 + 4.0);
                    (m_start - 0.4, bottom + 2.0 + 2.6)
                } else if k == 0 {
                    // Clear of the measure number.
                    (x_start, staff_top - 3.6)
                } else {
                    (m_start - 0.4, ty - 2.6)
                };
                bx0 += place.dx_sp();
                by1 += place.dy_sp();
                o.text(r, bx0 + w / 2.0, by1 - 0.6, 2.2, Align::Center, true, false);
                for (a, b, c, d2) in [
                    (bx0, by1, bx0 + w, by1),
                    (bx0, by1 - 2.6, bx0 + w, by1 - 2.6),
                    (bx0, by1, bx0, by1 - 2.6),
                    (bx0 + w, by1, bx0 + w, by1 - 2.6),
                ] {
                    o.line(a, b, c, d2, 0.16, None);
                }
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
                    o.glyph(c, tx, ty + lo, None);
                    tx += w + 0.8;
                }
            }
            if let Some(tt) = &info.tempo_text {
                o.text(tt, tx, ty, 2.0, Align::Left, true, false);
                tx += text_width(tt, 2.0) + 0.8;
            }
            let mut end_x = m_end - 0.3;
            for j in info.jumps.iter().filter(|j| j.at_end()) {
                if let Some(t) = j.text() {
                    if *j == scoreart_model::Jump::ToCoda
                        && let Some((c, w, lo, _)) = crate::smufl::get("coda")
                    {
                        o.glyph(c, end_x - w, ty + lo, None);
                        end_x -= w + 0.5;
                    }
                    o.text(t, end_x, ty, 1.9, Align::Right, true, true);
                    end_x -= text_width(t, 1.9) + 1.0;
                }
            }
            if let Some(tempo) = info.tempo {
                o.push(
                    Shape::Glyph {
                        c: g::MET_QUARTER_UP,
                        x: tx,
                        y: ty,
                        scale: 0.75,
                    },
                    None,
                );
                o.text(
                    &format!("= {}", tempo.bpm),
                    tx + 1.4,
                    ty,
                    1.9,
                    Align::Left,
                    true,
                    false,
                );
                tx += 7.0;
            }
            if let Some(text) = &info.text {
                o.text(text, tx, ty, 1.9, Align::Left, true, false);
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
                o.text(
                    &format!("To {}", ch.name),
                    m_start + 0.6,
                    staff_top - 2.6,
                    1.8,
                    Align::Left,
                    false,
                    true,
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
        let vy = top + t0 - (row_reach + 1.4).max(2.6) - 3.6;
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
        // Whole-measure rest.
        if v.events.len() == 1
            && v.events.first().is_some_and(Event::is_rest)
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
            // Grace notes before the chord: small, stem up, slashed for acciaccaturas.
            let ngr = e.grace.len();
            for (gi, gr) in e.grace.iter().enumerate() {
                let gx = left_edge
                    - acc_room
                    - if e.arpeggio != scoreart_model::Arpeggio::None {
                        1.3
                    } else {
                        0.0
                    }
                    - (ngr - gi) as f32 * 1.15
                    - 0.2;
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
                    o.push(
                        Shape::Text {
                            text: f.clone(),
                            x: hx + head_w / 2.0,
                            y: fy,
                            size: 1.6,
                            align: Align::Center,
                            bold: false,
                            italic: false,
                        },
                        Some(ev),
                    );
                    fy -= 1.4;
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
        // One line for this voice's chord symbols in the measure, clear of the highest note.
        let chord_row_used = v.events.iter().any(|e| e.chord_symbol.is_some());
        let chord_row = geos
            .iter()
            .filter_map(|gg| anchors.get(&gg.ev).map(|a| a.top.min(a.stem_end)))
            .fold(d.top, f32::min)
            .min(d.top)
            - 1.6;
        let chord_row = chord_row.min(d.top - 3.0);
        for (gg, e) in geos.iter().zip(v.events.iter()) {
            if let Some(dy) = e.dynamic {
                let mut x = gg.x - 0.2;
                let low = anchors
                    .get(&gg.ev)
                    .map(|a| a.bottom.max(if a.up { a.bottom } else { a.stem_end }))
                    .unwrap_or(d.top);
                let y = (d.top + 4.0 + 3.0).max(low + 2.6);
                for ch in dy.id().chars() {
                    if let Some((c, w)) = g::dynamic_letter(ch) {
                        o.glyph(c, x, y, Some(gg.ev));
                        x += w - 0.1;
                    }
                }
            }
            if let Some(cs) = &e.chord_symbol {
                // All chord symbols of the measure sit on one line.
                let y = chord_row;
                o.push(
                    Shape::Text {
                        text: chord_text(cs),
                        x: gg.x,
                        y,
                        size: 2.2,
                        align: Align::Left,
                        bold: true,
                        italic: false,
                    },
                    Some(gg.ev),
                );
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
                let text_x = gg.x + place.dx_sp();
                o.push(
                    Shape::Text {
                        text: t.clone(),
                        x: text_x,
                        y,
                        size: 1.9,
                        align: Align::Left,
                        bold: false,
                        italic: true,
                    },
                    Some(gg.ev),
                );
            }
            for (verse, l) in e.lyrics.iter().enumerate().filter(|(_, l)| !l.is_empty()) {
                let y = d.top + 4.0 + 5.6 + LYRIC_LINE * verse as f32;
                let cx = gg.x + 0.59;
                // A pending hyphen of this verse sits between the previous syllable and this one.
                if let Some((from, py)) = hyphen_from.get_mut(verse).and_then(Option::take) {
                    let left = from;
                    let right = cx - syllable_width(l) / 2.0;
                    if right - left > 0.6 {
                        o.text(
                            "-",
                            (left + right) / 2.0,
                            py,
                            LYRIC_SIZE,
                            Align::Center,
                            false,
                            false,
                        );
                    }
                }
                let text = l.strip_suffix('-').filter(|t| !t.is_empty()).unwrap_or(l);
                o.push(
                    Shape::Text {
                        text: text.to_string(),
                        x: cx,
                        y,
                        size: LYRIC_SIZE,
                        align: Align::Center,
                        bold: false,
                        italic: false,
                    },
                    Some(gg.ev),
                );
                if l.ends_with('-') && text.len() < l.len() {
                    if hyphen_from.len() <= verse {
                        hyphen_from.resize(verse + 1, None);
                    }
                    if let Some(h) = hyphen_from.get_mut(verse) {
                        *h = Some((cx + syllable_width(text) / 2.0, y));
                    }
                }
            }
        }
        // Hyphens whose next syllable is in a later measure: right after the syllable.
        for (from, y) in hyphen_from.into_iter().flatten() {
            o.text("-", from + 1.0, y, LYRIC_SIZE, Align::Center, false, false);
        }
        let _ = d.x1;
    }
}

/// Onset of an event from its geometry list position.
fn gg_onset(geos: &[EvGeo], gg: &EvGeo) -> u32 {
    let _ = geos;
    gg.onset
}

/// Chord symbols with real sharps and flats.
fn chord_text(s: &str) -> String {
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
    let mut text_above = 0.0f32;
    for &m in measures {
        let clef = score.clef_at(part, staff, m);
        let Some(sm) = score.staff_measure(part, staff, m) else {
            continue;
        };
        for v in &sm.voices {
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
                for n in e.notes() {
                    let y = note_y(clef, n.pitch) + shift;
                    up = up
                        .max(-(y - STEM_LEN) + marks)
                        .max(-y + 1.0 + marks + above_marks + finger);
                    down = down.max(y + STEM_LEN - 4.0).max(y - 4.0 + 1.0);
                }
                up = up.max(above_marks + 1.0);
                let mut t = 0.0;
                if e.chord_symbol.is_some() {
                    t += 2.6;
                }
                if e.text.is_some() {
                    t += 2.2;
                }
                text_above = text_above.max(t);
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
    (up + text_above, down)
}

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

fn draw_spanners(ctx: &Ctx, out: &mut Layout, last_page: &mut Page, last_index: usize) {
    let score = ctx.score;
    let mut items: Vec<(usize, Shape, EventRef)> = Vec::new();
    for sp in &score.spanners {
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
                    let x1 = if s.last { b.x_right + 0.6 } else { s.x1 - 0.4 };
                    let (o0, o1) = match (cresc, s.first, s.last) {
                        (true, f, l) => (if f { 0.0 } else { 0.3 }, if l { 0.6 } else { 0.45 }),
                        (false, f, l) => (if f { 0.6 } else { 0.45 }, if l { 0.0 } else { 0.3 }),
                    };
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
                            Shape::Text {
                                text: word.into(),
                                x: a.x_left,
                                y,
                                size: 1.9,
                                align: Align::Left,
                                bold: false,
                                italic: true,
                            },
                        ));
                        x0 = a.x_left + text_width(word, 1.9) + 0.4;
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
                            Shape::Text {
                                text: word.into(),
                                x: a.x_left,
                                y,
                                size: 1.8,
                                align: Align::Left,
                                bold: false,
                                italic: true,
                            },
                        ));
                        x0 = a.x_left + text_width(word, 1.8) + 0.3;
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
