//! The music of a PDF written by a notation program, read back from what it draws: staves from
//! five evenly spaced lines, systems from the lines and braces that join them, measures from
//! barlines, and notes from noteheads, stems, flags, beams and dots — with clefs, keys, time
//! signatures, accidentals, rests, voices, ties, slurs, lyrics, dynamics, articulations, titles
//! and tempo. Measures whose rhythm does not add up are reported as uncertain.

use std::collections::BTreeMap;

use scoreart_model::{
    Anchor, Articulation, Barline, Clef, Duration, Dynamic, Event, EventKind, Grace, KeySig, MeasureInfo,
    Note, NoteValue, Part, PartMeasure, PartSound, Pitch, Score, Spanner, SpannerKind, StaffMeasure, TPQ,
    Tempo, TimeSig, Tuplet, Voice, clip, limits,
};

use crate::PdfDoc;
use crate::collect::{Glyph, PageContent, Seg, collect};
use crate::fonts::{ClefSym, HeadSym, Sym, classify, is_music};

/// The score read from a PDF, with what is doubtful.
pub struct Recognition {
    pub score: Score,
    /// Measures (0-based) whose voices do not add up to the measure's length.
    pub uncertain: Vec<usize>,
    /// Remarks for the user (in English; the interface translates them).
    pub notes: Vec<String>,
}

/// Reads the music of a PDF.
pub fn recognize(doc: &PdfDoc) -> Result<Recognition, String> {
    let pages = collect(doc);
    let mut r = Reader::default();
    for page in &pages {
        r.page(page);
    }
    if r.systems.is_empty() {
        return Err(
            "no staves found: the PDF holds no music drawn as lines and music-font glyphs \
             (a scan needs optical recognition, which ScoreArt does not do yet)"
                .into(),
        );
    }
    let title = pages.first().map(titles).unwrap_or_default();
    Ok(r.build(title))
}

// ------------------------------------------------------------------ staves and systems

#[derive(Clone, Copy, Debug)]
struct Staff {
    top: f32,
    sp: f32,
    x0: f32,
    x1: f32,
}

impl Staff {
    fn bottom(&self) -> f32 {
        self.top + 4.0 * self.sp
    }
    fn mid(&self) -> f32 {
        self.top + 2.0 * self.sp
    }
}

/// Horizontal lines: (y, x0, x1), collinear pieces joined.
fn lines(segs: &[Seg]) -> Vec<(f32, f32, f32)> {
    let mut hs: Vec<(f32, f32, f32)> = segs
        .iter()
        .filter(|s| s.is_horizontal() && s.w < 1.6)
        .map(|s| ((s.y0 + s.y1) / 2.0, s.left(), s.right()))
        .collect();
    hs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut out: Vec<(f32, f32, f32)> = Vec::new();
    for h in hs {
        // Pieces of one line (sorted by height): look back over all lines at this height.
        if let Some(last) = out
            .iter_mut()
            .rev()
            .take_while(|l| h.0 - l.0 < 0.3)
            .find(|l| (l.0 - h.0).abs() < 0.3 && h.1 <= l.2 + 3.0 && h.2 >= l.1 - 3.0)
        {
            last.1 = last.1.min(h.1);
            last.2 = last.2.max(h.2);
        } else {
            out.push(h);
        }
    }
    out.retain(|l| l.2 - l.1 > 40.0);
    out
}

/// Five evenly spaced lines that overlap: a staff.
fn staves(lines: &[(f32, f32, f32)]) -> Vec<Staff> {
    let mut used = vec![false; lines.len()];
    let mut out = Vec::new();
    for i in 0..lines.len() {
        if used[i] {
            continue;
        }
        let a = lines[i];
        let overlaps = |b: &(f32, f32, f32)| {
            let ov = a.2.min(b.2) - a.1.max(b.1);
            ov > 0.8 * (a.2 - a.1).min(b.2 - b.1)
        };
        let below: Vec<usize> = (i + 1..lines.len())
            .filter(|j| !used[*j] && overlaps(&lines[*j]) && lines[*j].0 - a.0 < 100.0)
            .collect();
        'cand: for &j in below.iter().take(3) {
            let d = lines[j].0 - a.0;
            if !(2.0..=20.0).contains(&d) {
                continue;
            }
            let mut picked = vec![i, j];
            for k in 2..5 {
                let want = a.0 + d * k as f32;
                match below
                    .iter()
                    .copied()
                    .filter(|x| !picked.contains(x))
                    .min_by(|x, y| (lines[*x].0 - want).abs().total_cmp(&(lines[*y].0 - want).abs()))
                {
                    Some(x) if (lines[x].0 - want).abs() < d * 0.3 => picked.push(x),
                    _ => continue 'cand,
                }
            }
            // Evenly spaced: a line drawn a little off (rounding) is fine, a stray line is not.
            let ys: Vec<f32> = picked.iter().map(|p| lines[*p].0).collect();
            let gaps: Vec<f32> = ys.windows(2).map(|w| w[1] - w[0]).collect();
            let (lo, hi) = gaps
                .iter()
                .fold((f32::MAX, f32::MIN), |(l, h), g| (l.min(*g), h.max(*g)));
            if !(lo > 0.0 && hi < lo * 1.3) {
                continue 'cand;
            }
            for p in &picked {
                used[*p] = true;
            }
            let x0 = picked.iter().map(|p| lines[*p].1).fold(f32::MIN, f32::max);
            let x1 = picked.iter().map(|p| lines[*p].2).fold(f32::MAX, f32::min);
            let top = a.0;
            let sp = (lines[picked[4]].0 - top) / 4.0;
            out.push(Staff { top, sp, x0, x1 });
            break;
        }
    }
    out.sort_by(|a, b| a.top.total_cmp(&b.top));
    out
}

/// Braces left of the staves (x0): brace glyphs, or curved shapes (a brace drawn as outlines,
/// often in two halves) — their vertical spans.
fn braces_of(page: &PageContent, elems: &[Elem], x0: f32, sp: f32) -> Vec<(f32, f32)> {
    let mut spans: Vec<(f32, f32)> = elems
        .iter()
        .filter(|e| e.sym == Sym::Brace && e.g.bx1 <= x0 + sp && e.g.bx0 >= x0 - 5.0 * sp)
        .map(|e| (e.g.by0, e.g.by1))
        .collect();
    spans.extend(
        page.curves
            .iter()
            .filter(|c| {
                let (l, r) = (c.a.0.min(c.b.0), c.a.0.max(c.b.0));
                r <= x0 + 0.2 * sp && l >= x0 - 5.0 * sp && c.y1 - c.y0 > 3.0 * sp
            })
            .map(|c| (c.y0, c.y1)),
    );
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(f32, f32)> = Vec::new();
    for s in spans {
        match out.last_mut() {
            Some(l) if s.0 <= l.1 + sp => l.1 = l.1.max(s.1),
            _ => out.push(s),
        }
    }
    out
}

// ------------------------------------------------------------------ elements

/// A glyph with what it is.
#[derive(Clone, Debug)]
struct Elem {
    g: Glyph,
    sym: Sym,
}

/// A chord or rest found in one staff and measure.
#[derive(Clone, Debug)]
struct Item {
    x: f32,
    /// Pitches (diatonic, alteration as written or None) — empty for a rest.
    heads: Vec<(i32, Option<i8>, f32)>,
    value: NoteValue,
    dots: u8,
    up: Option<bool>,
    /// Vertical place of a rest (staff positions below the top line).
    rest_pos: f32,
    measure_rest: bool,
    tuplet: Option<Tuplet>,
    grace: Vec<Grace>,
    marks: Vec<Articulation>,
    dynamic: Option<Dynamic>,
    lyrics: BTreeMap<usize, String>,
    /// Tie on into the next item (by head index).
    ties: Vec<bool>,
    /// Stem end (for beams) and head x span (for curves).
    hx0: f32,
    hx1: f32,
    hy0: f32,
    hy1: f32,
}

/// One staff of one measure.
#[derive(Clone, Debug, Default)]
struct Cell {
    items: Vec<Item>,
    clef: Option<Clef>,
    /// The key the staff is in here.
    key: KeySig,
}

/// A system: its staves (index in system → staff) and measures.
struct System {
    staves: Vec<Staff>,
    /// Per measure: the staff cells.
    measures: Vec<Vec<Cell>>,
    infos: Vec<MeasureInfo>,
    names: Vec<String>,
    /// Staff ranges joined by braces (first, last).
    braces: Vec<(usize, usize)>,
    /// Slurs: (staff, measure a, item a, measure b, item b).
    slurs: Vec<(usize, usize, usize, usize, usize)>,
}

#[derive(Default)]
struct Reader {
    systems: Vec<System>,
    clefs: Vec<Clef>,
    /// The key of each staff (instruments may differ).
    keys: Vec<KeySig>,
    key: KeySig,
    time: Option<TimeSig>,
    tempo: Option<Tempo>,
    /// Staves per system of the score (from its first system).
    score_staves: Option<usize>,
    /// Pages after the score hold the instruments' parts: reading stopped there.
    parts_follow: bool,
}

fn near(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

impl Reader {
    fn page(&mut self, page: &PageContent) {
        if self.parts_follow {
            return;
        }
        let all = staves(&lines(&page.segs));
        if all.is_empty() {
            return;
        }
        let elems: Vec<Elem> = page
            .glyphs
            .iter()
            .filter(|g| is_music(g))
            .map(|g| Elem {
                sym: classify(g),
                g: g.clone(),
            })
            .collect();
        let mut elems = elems;
        // A small "8" in a text font right under a treble clef (LilyPond) belongs to the clef.
        let clefs: Vec<Glyph> = elems
            .iter()
            .filter(|e| e.sym == Sym::Clef(crate::fonts::ClefSym::G))
            .map(|e| e.g.clone())
            .collect();
        let under_clef = |g: &Glyph| {
            g.ch == Some('8')
                && clefs.iter().any(|c| {
                    let sp = (c.by1 - c.by0) / 7.0;
                    g.cx() > c.bx0 && g.cx() < c.bx1 && g.by0 > c.by1 - 1.5 * sp && g.by0 < c.by1 + sp
                })
        };
        for g in page.glyphs.iter().filter(|g| !is_music(g) && under_clef(g)) {
            elems.push(Elem {
                sym: Sym::TimeDigit(8),
                g: g.clone(),
            });
        }
        let text: Vec<&Glyph> = page
            .glyphs
            .iter()
            .filter(|g| !is_music(g) && !under_clef(g))
            .collect();
        // Systems: neighbouring staves joined by a vertical line or a brace.
        let joined =
            |a: &Staff, b: &Staff| {
                let sp = a.sp.max(b.sp);
                page.segs.iter().any(|s| {
                    s.is_vertical()
                        && s.top() <= a.bottom() + 0.5
                        && s.bottom() >= b.top - 0.5
                        && s.xm() >= a.x0.min(b.x0) - 3.0 * sp
                        && s.xm() <= a.x1.max(b.x1) + 1.0
                }) || page.polys.iter().any(|p| {
                    p.w() < sp && p.y0 <= a.bottom() + 0.5 && p.y1 >= b.top - 0.5 && p.x0 <= a.x0 + sp
                }) || braces_of(page, &elems, a.x0.min(b.x0), sp)
                    .iter()
                    .any(|(t, u)| *t <= a.top + sp && *u >= b.bottom() - sp)
            };
        let mut groups: Vec<Vec<Staff>> = Vec::new();
        for s in all {
            match groups.last_mut() {
                Some(g) if g.last().is_some_and(|l| joined(l, &s)) => g.push(s),
                _ => groups.push(vec![s]),
            }
        }
        // A score followed by its parts in one file: once a whole page has only single staves
        // (and more than one) after a score of several, the parts begin.
        match self.score_staves {
            Some(k) if k > 1 && groups.len() > 1 && groups.iter().all(|g| g.len() == 1) => {
                self.parts_follow = true;
                return;
            }
            None => self.score_staves = groups.first().map(Vec::len),
            _ => {}
        }
        for g in groups {
            self.system(g, page, &elems, &text);
        }
    }

    fn system(&mut self, staves: Vec<Staff>, page: &PageContent, elems: &[Elem], text: &[&Glyph]) {
        let n = staves.len();
        let sp = staves.iter().map(|s| s.sp).sum::<f32>() / n.max(1) as f32;
        let (x0, x1) = (
            staves.iter().map(|s| s.x0).fold(f32::MAX, f32::min),
            staves.iter().map(|s| s.x1).fold(f32::MIN, f32::max),
        );
        let (y0, y1) = (
            staves.first().map_or(0.0, |s| s.top) - 8.0 * sp,
            staves.last().map_or(0.0, |s| s.bottom()) + 8.0 * sp,
        );
        // Which staff a vertical position belongs to.
        let staff_of = |y: f32| -> usize {
            staves
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    let da = if y < a.top {
                        a.top - y
                    } else {
                        (y - a.bottom()).max(0.0)
                    };
                    let db = if y < b.top {
                        b.top - y
                    } else {
                        (y - b.bottom()).max(0.0)
                    };
                    da.total_cmp(&db)
                })
                .map_or(0, |(i, _)| i)
        };
        let mine: Vec<&Elem> = elems
            .iter()
            .filter(|e| e.g.cy() >= y0 && e.g.cy() <= y1 && e.g.cx() >= x0 - 4.0 * sp && e.g.cx() <= x1 + sp)
            .collect();
        let heads: Vec<&Elem> = mine
            .iter()
            .copied()
            .filter(|e| matches!(e.sym, Sym::Head(_)))
            .collect();
        // Vertical strokes: stems and barlines.
        let verts: Vec<&Seg> = page
            .segs
            .iter()
            .filter(|s| {
                s.is_vertical()
                    && s.xm() >= x0 - 1.0
                    && s.xm() <= x1 + 1.0
                    && s.bottom() >= y0
                    && s.top() <= y1
            })
            .collect();
        let touches_head = |s: &Seg| {
            heads.iter().any(|h| {
                (near(s.xm(), h.g.bx0, 0.4 * sp) || near(s.xm(), h.g.bx1, 0.4 * sp))
                    && (near(s.top(), h.g.cy(), 0.7 * sp) || near(s.bottom(), h.g.cy(), 0.7 * sp))
            })
        };
        // Barlines per staff: spanning the staff, touching no notehead.
        let mut bar_x: Vec<Vec<(f32, f32)>> = vec![Vec::new(); n];
        for (si, st) in staves.iter().enumerate() {
            for s in &verts {
                if s.top() <= st.top + 0.3 * st.sp
                    && s.bottom() >= st.bottom() - 0.3 * st.sp
                    && !touches_head(s)
                {
                    bar_x[si].push((s.xm(), s.w));
                }
            }
            for p in &page.polys {
                if p.w() < st.sp
                    && p.h() > 3.0 * st.sp
                    && p.y0 <= st.top + 0.3 * st.sp
                    && p.y1 >= st.bottom() - 0.3 * st.sp
                {
                    bar_x[si].push(((p.x0 + p.x1) / 2.0, p.w()));
                }
            }
        }
        // Barline positions of the system: in most staves.
        let mut cand: Vec<(f32, f32)> = bar_x.iter().flatten().copied().collect();
        cand.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut clusters: Vec<Vec<(f32, f32)>> = Vec::new();
        for c in cand {
            match clusters.last_mut() {
                Some(cl) if cl.last().is_some_and(|l| c.0 - l.0 < 1.2 * sp) => cl.push(c),
                _ => clusters.push(vec![c]),
            }
        }
        let mut bounds: Vec<(f32, Barline, bool)> = Vec::new(); // x, barline, thick-first (repeat start)
        for cl in &clusters {
            let staves_with = (0..n)
                .filter(|si| bar_x[*si].iter().any(|b| cl.iter().any(|c| near(c.0, b.0, 0.1))))
                .count();
            if staves_with * 2 < n.max(1) && n > 1 {
                continue;
            }
            let xs: Vec<f32> = cl.iter().map(|c| c.0).collect();
            let xm = xs.iter().sum::<f32>() / xs.len() as f32;
            let thick = cl.iter().any(|c| c.1 > 0.3 * sp);
            let distinct = {
                let mut d = xs.clone();
                d.dedup_by(|a, b| (*a - *b).abs() < 0.2 * sp);
                d.len()
            };
            let left_thick = cl.first().is_some_and(|c| c.1 > 0.3 * sp) && distinct > 1;
            let kind = match (distinct > 1, thick) {
                (true, true) => Barline::Final,
                (true, false) => Barline::Double,
                _ => Barline::Regular,
            };
            bounds.push((xm, kind, left_thick));
        }
        // The line at the system's start is no measure boundary.
        bounds.retain(|b| b.0 > x0 + 1.5 * sp);
        if bounds.last().is_none_or(|b| b.0 < x1 - 2.0 * sp) {
            bounds.push((x1, Barline::Regular, false));
        }
        // Repeat dots beside a barline.
        let dots_at = |x: f32, left: bool| {
            mine.iter().any(|e| {
                e.sym == Sym::RepeatDots
                    && if left {
                        e.g.bx1 <= x + 0.3 * sp && e.g.bx1 >= x - 2.0 * sp
                    } else {
                        e.g.bx0 >= x - 0.3 * sp && e.g.bx0 <= x + 2.0 * sp
                    }
            }) || {
                // Two augmentation-dot glyphs in the spaces around the middle line.
                let ds: Vec<&&Elem> = mine
                    .iter()
                    .filter(|e| e.sym == Sym::Dot)
                    .filter(|e| {
                        if left {
                            e.g.bx1 <= x && e.g.bx1 >= x - 1.5 * sp
                        } else {
                            e.g.bx0 >= x && e.g.bx0 <= x + 1.5 * sp
                        }
                    })
                    .collect();
                staves.iter().any(|st| {
                    ds.iter()
                        .any(|d| near(d.g.cy(), st.mid() - 0.5 * st.sp, 0.3 * st.sp))
                        && ds
                            .iter()
                            .any(|d| near(d.g.cy(), st.mid() + 0.5 * st.sp, 0.3 * st.sp))
                })
            }
        };
        let mut cells: Vec<Vec<Vec<&Elem>>> = vec![vec![Vec::new(); n]; bounds.len()];
        let mut lefts = vec![x0];
        lefts.extend(bounds.iter().map(|b| b.0));
        for e in &mine {
            let cx = e.g.cx();
            let mi = bounds
                .iter()
                .position(|b| cx < b.0)
                .unwrap_or(bounds.len().saturating_sub(1));
            let si = staff_of(e.g.cy());
            if let Some(c) = cells.get_mut(mi).and_then(|m| m.get_mut(si)) {
                c.push(e);
            }
        }
        let first_system = self.systems.is_empty();
        if self.clefs.len() < n {
            self.clefs.resize(n, Clef::Treble);
        }
        if self.keys.len() < n {
            self.keys.resize(n, self.key);
        }
        let mut sys = System {
            staves: staves.clone(),
            measures: Vec::new(),
            infos: Vec::new(),
            names: vec![String::new(); n],
            braces: Vec::new(),
            slurs: Vec::new(),
        };
        // Braces join piano staves into one instrument.
        for (t, u) in braces_of(page, elems, x0, sp) {
            let a = staff_of(t + sp);
            let b = staff_of(u - sp);
            if b > a {
                sys.braces.push((a, b));
            }
        }
        // Instrument names: text left of the staves, for the staff (or brace) beside it.
        let mut name_glyphs: Vec<&&Glyph> = text
            .iter()
            .filter(|t| {
                t.bx1 < x0 - 0.5 * sp
                    && t.cy() >= staves.first().map_or(y0, |s| s.top - 2.0 * sp)
                    && t.cy() <= y1 - 6.0 * sp
            })
            .collect();
        name_glyphs.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.bx0.total_cmp(&b.bx0)));
        for t in name_glyphs {
            let mut si = staff_of(t.cy());
            if let Some(b) = sys.braces.iter().find(|b| si >= b.0 && si <= b.1) {
                si = b.0;
            }
            if let (Some(name), Some(c)) = (sys.names.get_mut(si), t.ch) {
                if let Some(last) = name.chars().last()
                    && last != ' '
                    && c.is_alphabetic() != last.is_alphabetic()
                {
                    name.push(' ');
                }
                name.push(c);
            }
        }
        let _ = first_system;
        // Measure by measure.
        let all_verts: Vec<Seg> = verts.iter().map(|s| **s).collect();
        for (mi, b) in bounds.iter().enumerate() {
            let left = lefts.get(mi).copied().unwrap_or(x0);
            let mut info = MeasureInfo {
                barline: b.1,
                ..MeasureInfo::default()
            };
            if dots_at(b.0, true) {
                info.barline = Barline::RepeatEnd;
            }
            if dots_at(left, false) {
                info.repeat_start = true;
            }
            let mut row = Vec::with_capacity(n);
            for (si, &st) in staves.iter().enumerate().take(n) {
                let es: Vec<&Elem> = cells.get(mi).and_then(|m| m.get(si)).cloned().unwrap_or_default();
                let (cell, key, time) = self.cell(
                    si,
                    &st,
                    &es,
                    &all_verts,
                    &page.polys,
                    &page.curves,
                    (left, b.0),
                    mi == 0,
                );
                if si == 0 {
                    if let Some(k) = key
                        && k != self.key
                    {
                        self.key = k;
                        info.key = Some(k);
                    }
                    if let Some(t) = time
                        && Some(t) != self.time
                    {
                        self.time = Some(t);
                        info.time = Some(t);
                    }
                }
                row.push(cell);
            }
            // Lyrics and dynamics under each staff.
            for si in 0..n {
                let st = staves[si];
                let below = staves.get(si + 1).map_or(st.bottom() + 10.0 * st.sp, |s| s.top);
                attach_lyrics(&mut row[si].items, text, &st, below, left, b.0);
                attach_dynamics(&mut row[si].items, &mine, &st, left, b.0);
            }
            sys.measures.push(row);
            sys.infos.push(info);
        }
        // Tempo above the first staff of the first system.
        if self.tempo.is_none()
            && let Some(st) = staves.first()
        {
            let words: String = text
                .iter()
                .filter(|t| t.cy() < st.top && t.cy() > st.top - 8.0 * st.sp)
                .filter_map(|t| t.ch)
                .collect();
            if let Some((_, after)) = words.split_once('=') {
                let digits: String = after
                    .chars()
                    .skip_while(|c| !c.is_ascii_digit())
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if let Ok(bpm) = digits.parse::<u16>()
                    && (10..=400).contains(&bpm)
                {
                    self.tempo = Some(Tempo::quarter(bpm));
                    if let Some(i) = sys.infos.first_mut() {
                        i.tempo = self.tempo;
                    }
                }
            }
        }
        // Ties and slurs from the curves.
        curves(&mut sys, &page.curves);
        self.systems.push(sys);
    }

    /// One staff of one measure: its items, and a key or time signature found in it.
    #[allow(clippy::too_many_arguments)]
    fn cell(
        &mut self,
        si: usize,
        st: &Staff,
        es: &[&Elem],
        verts: &[Seg],
        polys: &[crate::collect::Poly],
        _curves: &[crate::collect::Curve],
        (left, right): (f32, f32),
        system_start: bool,
    ) -> (Cell, Option<KeySig>, Option<TimeSig>) {
        let sp = st.sp;
        let mut cell = Cell::default();
        let pos = |y: f32| ((y - st.top) / (sp / 2.0)).round() as i32;
        let heads: Vec<&&Elem> = es.iter().filter(|e| matches!(e.sym, Sym::Head(_))).collect();
        let first_note_x = es
            .iter()
            .filter(|e| matches!(e.sym, Sym::Head(_) | Sym::Rest(_)))
            .map(|e| e.g.bx0)
            .fold(f32::MAX, f32::min);
        let last_note_x = es
            .iter()
            .filter(|e| matches!(e.sym, Sym::Head(_) | Sym::Rest(_)))
            .map(|e| e.g.bx1)
            .fold(f32::MIN, f32::max);
        // Clef.
        for e in es.iter().filter(|e| matches!(e.sym, Sym::Clef(_))) {
            if e.g.cx() > last_note_x && last_note_x > f32::MIN {
                continue; // a courtesy clef before the next system or measure
            }
            if let Sym::Clef(c) = e.sym {
                // A small "8" drawn under the clef as a glyph of its own (LilyPond) makes an
                // octave clef too.
                let eight_below = es.iter().any(|d| {
                    d.sym == Sym::TimeDigit(8)
                        && d.g.cx() > e.g.bx0
                        && d.g.cx() < e.g.bx1
                        && d.g.cy() > e.g.by1 - 0.5 * sp
                        && d.g.cy() < e.g.by1 + 2.0 * sp
                });
                let clef = match c {
                    ClefSym::G if eight_below => Clef::Treble8vb,
                    ClefSym::G | ClefSym::G8va => Clef::Treble,
                    ClefSym::G8vb => Clef::Treble8vb,
                    ClefSym::F | ClefSym::F8vb => Clef::Bass,
                    ClefSym::Percussion => Clef::Percussion,
                    ClefSym::C => {
                        if near(e.g.cy(), st.top + sp, 0.5 * sp) {
                            Clef::Tenor
                        } else {
                            Clef::Alto
                        }
                    }
                };
                if let Some(cur) = self.clefs.get_mut(si) {
                    if *cur != clef {
                        cell.clef = Some(clef);
                    }
                    *cur = clef;
                }
            }
        }
        let clef = self.clefs.get(si).copied().unwrap_or_default();
        // Accidentals: of a note (just left of a head at its height) or of a key signature.
        let mut note_acc: Vec<(f32, f32, i8)> = Vec::new();
        let mut key_acc: Vec<(f32, i32, i8)> = Vec::new(); // x, step letter, sign
        for e in es.iter() {
            if let Sym::Accidental(a) = e.sym {
                // A flat's bowl, not the middle of its bounding box, sits on the note's line.
                let ay = if a < 0 { e.g.by1 - 0.7 * sp } else { e.g.cy() };
                let of_head = heads.iter().any(|h| {
                    near(h.g.cy(), ay, 0.45 * sp)
                        && h.g.bx0 >= e.g.bx1 - 0.3 * sp
                        && h.g.bx0 - e.g.bx1 < 3.5 * sp
                });
                if of_head {
                    note_acc.push((e.g.bx1, ay, a));
                } else if e.g.cx() < first_note_x {
                    let d = clef.top_line() - pos(ay);
                    key_acc.push((e.g.cx(), d.rem_euclid(7), a));
                }
            }
        }
        // A key signature: sharps in the order F C G D A E B, or flats B E A D G C F
        // (naturals cancelling the old key come first and say C major or the new key follows).
        key_acc.sort_by(|a, b| a.0.total_cmp(&b.0));
        let signs: Vec<&(f32, i32, i8)> = key_acc.iter().filter(|a| a.2 != 0).collect();
        const SHARPS: [i32; 7] = [3, 0, 4, 1, 5, 2, 6]; // F C G D A E B (C = 0)
        const FLATS: [i32; 7] = [6, 2, 5, 1, 4, 0, 3]; // B E A D G C F
        let has_clef = es
            .iter()
            .any(|e| matches!(e.sym, Sym::Clef(_)) && e.g.cx() < first_note_x);
        let key = if signs.is_empty() {
            // Every system repeats the key after its clef: none there is C major.
            (!key_acc.is_empty() || (system_start && has_clef)).then(|| KeySig::new(0))
        } else if signs.len() <= 7
            && signs.iter().all(|a| a.2 == 1)
            && signs.iter().zip(SHARPS).all(|(a, s)| a.1 == s)
        {
            Some(KeySig::new(signs.len() as i32))
        } else if signs.len() <= 7
            && signs.iter().all(|a| a.2 == -1)
            && signs.iter().zip(FLATS).all(|(a, s)| a.1 == s)
        {
            Some(KeySig::new(-(signs.len() as i32)))
        } else {
            None
        };
        if let Some(k) = key
            && let Some(slot) = self.keys.get_mut(si)
        {
            *slot = k;
        }
        let staff_key = self.keys.get(si).copied().unwrap_or(self.key);
        // Time signature: digits stacked above and below the middle line.
        let mut time = None;
        let digits: Vec<&&Elem> = es
            .iter()
            .filter(|e| matches!(e.sym, Sym::TimeDigit(_)) && e.g.cx() < first_note_x)
            // Inside the staff: an "8" under a clef is no time signature.
            .filter(|e| e.g.cy() > st.top - 0.3 * sp && e.g.cy() < st.bottom() + 0.3 * sp)
            .collect();
        if let Some(e) = es
            .iter()
            .find(|e| matches!(e.sym, Sym::TimeCommon | Sym::TimeCut) && e.g.cx() < first_note_x)
        {
            time = TimeSig::new(
                if e.sym == Sym::TimeCommon { 4 } else { 2 },
                if e.sym == Sym::TimeCommon { 4 } else { 2 },
            )
            .ok();
        } else if !digits.is_empty() {
            let mut up: Vec<&&&Elem> = digits.iter().filter(|e| e.g.cy() < st.mid()).collect();
            let mut down: Vec<&&&Elem> = digits.iter().filter(|e| e.g.cy() >= st.mid()).collect();
            up.sort_by(|a, b| a.g.bx0.total_cmp(&b.g.bx0));
            down.sort_by(|a, b| a.g.bx0.total_cmp(&b.g.bx0));
            let num = |v: &[&&&Elem]| -> i64 {
                v.iter().fold(0i64, |n, e| match e.sym {
                    Sym::TimeDigit(d) => n.saturating_mul(10).saturating_add(i64::from(d)),
                    _ => n,
                })
            };
            if !up.is_empty() && !down.is_empty() {
                time = TimeSig::new(num(&up), num(&down)).ok();
            }
        }
        // Stems: vertical strokes touching a head at one end.
        let stems: Vec<&Seg> = verts
            .iter()
            .filter(|s| {
                s.xm() > left - sp
                    && s.xm() < right + sp
                    && s.bottom() - s.top() > 1.5 * sp
                    && s.w < 0.35 * sp
            })
            .collect();
        let normal = {
            let mut sizes: Vec<f32> = heads.iter().map(|h| h.g.size).collect();
            sizes.sort_by(f32::total_cmp);
            sizes.get(sizes.len() / 2).copied().unwrap_or(0.0)
        };
        // Group heads into chords: by the stem they touch, stemless ones by place.
        let mut groups: Vec<(Option<usize>, Vec<&&Elem>)> = Vec::new();
        for h in &heads {
            let stem = stems.iter().position(|s| {
                (near(s.xm(), h.g.bx1, 0.45 * sp) || near(s.xm(), h.g.bx0, 0.45 * sp))
                    && h.g.cy() >= s.top() - 0.7 * sp
                    && h.g.cy() <= s.bottom() + 0.7 * sp
            });
            let found = groups.iter_mut().find(|(s, g)| match (stem, s) {
                (Some(a), Some(b)) => a == *b,
                (None, None) => g.iter().any(|o| near(o.g.cx(), h.g.cx(), 1.3 * sp)),
                _ => false,
            });
            match found {
                Some((_, g)) => g.push(h),
                None => groups.push((stem, vec![h])),
            }
        }
        let used_acc = &mut vec![false; note_acc.len()];
        let mut alters: BTreeMap<i32, i8> = BTreeMap::new();
        let mut items: Vec<Item> = Vec::new();
        let mut graces: Vec<(f32, Grace)> = Vec::new();
        let mut order: Vec<usize> = (0..groups.len()).collect();
        order.sort_by(|a, b| {
            let xa = groups[*a].1.iter().map(|h| h.g.bx0).fold(f32::MAX, f32::min);
            let xb = groups[*b].1.iter().map(|h| h.g.bx0).fold(f32::MAX, f32::min);
            xa.total_cmp(&xb)
        });
        for gi in order {
            let (stem, hs) = &groups[gi];
            let stem = stem.and_then(|i| stems.get(i)).copied();
            let small = normal > 0.0 && hs.iter().all(|h| h.g.size < normal * 0.8);
            let hx0 = hs.iter().map(|h| h.g.bx0).fold(f32::MAX, f32::min);
            let hx1 = hs.iter().map(|h| h.g.bx1).fold(f32::MIN, f32::max);
            let hy0 = hs.iter().map(|h| h.g.cy()).fold(f32::MAX, f32::min);
            let hy1 = hs.iter().map(|h| h.g.cy()).fold(f32::MIN, f32::max);
            // Pitches, with accidentals written and those carried in the measure.
            let mut pitches: Vec<(i32, Option<i8>, f32)> = Vec::new();
            for h in hs {
                let d = clef.top_line() - pos(h.g.cy());
                let acc = note_acc.iter().enumerate().find(|(i, a)| {
                    !used_acc[*i]
                        && near(a.1, h.g.cy(), 0.45 * sp)
                        && a.0 <= h.g.bx0 + 0.3 * sp
                        && h.g.bx0 - a.0 < 3.5 * sp
                });
                let written = acc.map(|(i, a)| {
                    used_acc[i] = true;
                    a.2
                });
                if let Some(a) = written
                    && !small
                {
                    alters.insert(d, a);
                }
                pitches.push((d, written.or_else(|| alters.get(&d).copied()), h.g.cx()));
            }
            pitches.sort_by_key(|p| p.0);
            pitches.dedup_by_key(|p| p.0);
            let kind = hs
                .iter()
                .map(|h| match h.sym {
                    Sym::Head(k) => k,
                    _ => HeadSym::Black,
                })
                .fold(HeadSym::Whole, |a, k| match (a, k) {
                    (HeadSym::Black, _) | (_, HeadSym::Black) | (_, HeadSym::Other) | (HeadSym::Other, _) => {
                        HeadSym::Black
                    }
                    (HeadSym::Half, _) | (_, HeadSym::Half) => HeadSym::Half,
                    (HeadSym::Breve, _) | (_, HeadSym::Breve) => HeadSym::Breve,
                    _ => HeadSym::Whole,
                });
            let up = stem.map(|s| s.top() < hy0 - 0.8 * sp);
            let mut value = match kind {
                HeadSym::Breve => NoteValue::Breve,
                HeadSym::Whole => NoteValue::Whole,
                HeadSym::Half => NoteValue::Half,
                _ => NoteValue::Quarter,
            };
            if kind == HeadSym::Black
                && let Some(s) = stem
            {
                let tip = if up == Some(true) { s.top() } else { s.bottom() };
                let flags = es
                    .iter()
                    .filter_map(|e| match e.sym {
                        Sym::Flag(n)
                            if near(e.g.bx0, s.xm(), 0.6 * sp)
                                && e.g.by0 <= tip + 1.5 * sp
                                && e.g.by1 >= tip - 1.5 * sp =>
                        {
                            Some(n)
                        }
                        _ => None,
                    })
                    .max()
                    .unwrap_or(0);
                let beams = polys
                    .iter()
                    .filter(|p| p.h() < 3.0 * sp && p.w() > 0.8 * sp)
                    .filter(|p| {
                        p.mid_at(s.xm()).is_some_and(|y| {
                            p.x0 <= s.xm() + 0.3 * sp
                                && p.x1 >= s.xm() - 0.3 * sp
                                && y >= s.top() - 0.5 * sp
                                && y <= s.bottom() + 0.5 * sp
                                && (y - tip).abs() < 3.0 * sp
                        })
                    })
                    .count() as u8;
                let n = flags.max(beams);
                value = match n {
                    0 => NoteValue::Quarter,
                    1 => NoteValue::Eighth,
                    2 => NoteValue::N16th,
                    3 => NoteValue::N32nd,
                    4 => NoteValue::N64th,
                    5 => NoteValue::N128th,
                    _ => NoteValue::N256th,
                };
            }
            if small {
                let ps: Vec<Pitch> = pitches.iter().map(|p| pitch(p.0, p.1, staff_key)).collect();
                if !ps.is_empty() {
                    graces.push((
                        hx0,
                        Grace {
                            pitches: ps,
                            value: if value >= NoteValue::Eighth {
                                value
                            } else {
                                NoteValue::Eighth
                            },
                            slash: false,
                            after: false,
                        },
                    ));
                }
                continue;
            }
            let dots = dots_right(
                es,
                hx1,
                &pitches.iter().map(|p| p.0).collect::<Vec<_>>(),
                st,
                clef,
            );
            items.push(Item {
                x: hx0,
                ties: vec![false; pitches.len()],
                heads: pitches,
                value,
                dots,
                up,
                rest_pos: 0.0,
                measure_rest: false,
                tuplet: None,
                grace: Vec::new(),
                marks: Vec::new(),
                dynamic: None,
                lyrics: BTreeMap::new(),
                hx0,
                hx1,
                hy0,
                hy1,
            });
        }
        // Rests.
        let rests: Vec<&&Elem> = es.iter().filter(|e| matches!(e.sym, Sym::Rest(_))).collect();
        let lone = rests.len() == 1 && items.is_empty();
        for e in rests {
            let Sym::Rest(v) = e.sym else { continue };
            let dots = dots_right(es, e.g.bx1, &[clef.top_line() - pos(e.g.cy())], st, clef);
            let measure_rest = lone && v == NoteValue::Whole;
            items.push(Item {
                x: e.g.bx0,
                heads: Vec::new(),
                value: v,
                dots,
                up: None,
                rest_pos: (e.g.cy() - st.top) / (sp / 2.0),
                measure_rest,
                tuplet: None,
                grace: Vec::new(),
                marks: Vec::new(),
                dynamic: None,
                lyrics: BTreeMap::new(),
                ties: Vec::new(),
                hx0: e.g.bx0,
                hx1: e.g.bx1,
                hy0: e.g.cy(),
                hy1: e.g.cy(),
            });
        }
        items.sort_by(|a, b| a.x.total_cmp(&b.x));
        // Grace notes go to the next item.
        for (gx, g) in graces {
            if let Some(it) = items.iter_mut().find(|i| i.x > gx) {
                it.grace.push(g);
            }
        }
        // Tuplet numbers: the items under their beam, or the nearest ones.
        for e in es.iter().filter(|e| matches!(e.sym, Sym::Tuplet(_))) {
            let Sym::Tuplet(k) = e.sym else { continue };
            let k = k.clamp(2, 9);
            let normal = match k {
                2 => 3,
                3 => 2,
                4 => 3,
                5..=7 => 4,
                _ => 8,
            };
            let mut near_items: Vec<usize> =
                (0..items.len()).filter(|i| items[*i].tuplet.is_none()).collect();
            near_items.sort_by(|a, b| {
                (items[*a].x - e.g.cx())
                    .abs()
                    .total_cmp(&(items[*b].x - e.g.cx()).abs())
            });
            near_items.truncate(usize::from(k));
            for i in near_items {
                if let Some(it) = items.get_mut(i) {
                    it.tuplet = Some(Tuplet { actual: k, normal });
                }
            }
        }
        // Articulations above or below an item.
        for e in es.iter() {
            let a = match e.sym {
                Sym::Articulation(a) => Some(a),
                Sym::Dot => None,
                _ => continue,
            };
            let Some(it) = items
                .iter_mut()
                .filter(|i| !i.heads.is_empty())
                .min_by(|x, y| (x.hx0 - e.g.cx()).abs().total_cmp(&(y.hx0 - e.g.cx()).abs()))
            else {
                continue;
            };
            let over = e.g.cx() >= it.hx0 - 0.3 * sp && e.g.cx() <= it.hx1 + 0.3 * sp;
            let off = e.g.cy() < it.hy0 - 0.6 * sp || e.g.cy() > it.hy1 + 0.6 * sp;
            if !(over && off) {
                continue;
            }
            let a = a.or_else(|| Articulation::from_id("staccato"));
            if let Some(a) = a
                && !it.marks.contains(&a)
            {
                it.marks.push(a);
            }
        }
        cell.items = items;
        cell.key = staff_key;
        (cell, key, time)
    }

    /// The score from the systems.
    fn build(self, title: Titles) -> Recognition {
        let mut notes = Vec::new();
        if self.parts_follow {
            notes.push(
                "The pages after the score hold the instruments' parts; only the score was read.".into(),
            );
        }
        let n = self
            .systems
            .iter()
            .map(|s| s.staves.len())
            .fold(BTreeMap::<usize, usize>::new(), |mut m, k| {
                *m.entry(k).or_default() += 1;
                m
            })
            .into_iter()
            .max_by_key(|(k, c)| (*c, *k))
            .map_or(1, |(k, _)| k)
            .clamp(1, limits::MAX_PARTS * 4);
        if self.systems.iter().any(|s| s.staves.len() != n) {
            notes.push(
                "Some systems have a different number of staves; they were matched from the top.".into(),
            );
        }
        // Instruments from the first system with all staves: braces join staves.
        let first = self.systems.iter().find(|s| s.staves.len() == n);
        let mut part_of: Vec<(usize, usize)> = Vec::with_capacity(n); // staff → (part, staff in part)
        let mut parts: Vec<(Vec<usize>, String)> = Vec::new();
        let mut si = 0;
        while si < n {
            let span = first
                .and_then(|s| s.braces.iter().find(|b| b.0 == si))
                .map_or(si, |b| b.1.min(n - 1));
            let name: String = first
                .map(|s| {
                    (si..=span)
                        .filter_map(|k| s.names.get(k))
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            let pi = parts.len();
            for (k, staff) in (si..=span).enumerate() {
                let _ = staff;
                part_of.push((pi, k));
            }
            parts.push((
                (si..=span).collect(),
                clip(name.split_whitespace().collect::<Vec<_>>().join(" ").as_str()),
            ));
            si = span + 1;
        }
        let mut score = Score::default();
        score.meta.title = title.title;
        score.meta.subtitle = title.subtitle;
        score.meta.composer = title.composer;
        score.meta.lyricist = title.lyricist;
        score.meta.copyright = title.copyright;
        for (staves, name) in &parts {
            let name = if name.is_empty() {
                format!("Part {}", score.parts.len() + 1)
            } else {
                name.clone()
            };
            let program = scoreart_musicxml::program_for_name(&name).unwrap_or(0);
            score.parts.push(Part {
                id: format!("P{}", score.parts.len() + 1),
                name: clip(&name),
                abbreviation: String::new(),
                program,
                clefs: vec![Clef::Treble; staves.len()],
                measures: Vec::new(),
                sound: PartSound::default(),
                hidden: Vec::new(),
            });
        }
        let mut uncertain = Vec::new();
        let mut time = TimeSig::new(4, 4).unwrap_or_default();
        let mut first_clefs: Vec<Option<Clef>> = vec![None; n];
        let mut slurs: Vec<Spanner> = Vec::new();
        for sys in &self.systems {
            let base = score.measures.len();
            for (mi, (row, info)) in sys.measures.iter().zip(&sys.infos).enumerate() {
                if score.measures.len() >= limits::MAX_MEASURES {
                    break;
                }
                if let Some(t) = info.time {
                    time = t;
                }
                let mut info = info.clone();
                let len = time.measure_ticks();
                let mut longest = 0u32;
                let mut doubtful = false;
                for p in &mut score.parts {
                    p.measures.push(PartMeasure {
                        staves: vec![StaffMeasure::default(); p.clefs.len()],
                        ..PartMeasure::default()
                    });
                }
                for (k, cell) in row.iter().enumerate().take(n) {
                    let Some(&(pi, ps)) = part_of.get(k) else { continue };
                    if let Some(c) = cell.clef
                        && score.measures.is_empty()
                        && let Some(slot @ None) = first_clefs.get_mut(k)
                    {
                        *slot = Some(c);
                    }
                    let (voices, total, bad) = voices_of(cell, len, cell.key);
                    longest = longest.max(total);
                    doubtful |= bad;
                    if let Some(sm) = score
                        .parts
                        .get_mut(pi)
                        .and_then(|p| p.measures.last_mut())
                        .and_then(|m| m.staves.get_mut(ps))
                    {
                        sm.voices = voices;
                        if !score.measures.is_empty() || base > 0 || mi > 0 {
                            sm.clef = cell.clef;
                        }
                    }
                }
                // A short first measure is a pickup.
                if score.measures.is_empty() && longest > 0 && longest < len {
                    info.actual = Some(longest);
                    info.uncounted = true;
                } else if doubtful {
                    uncertain.push(score.measures.len());
                }
                score.measures.push(info);
            }
            for &(k, ma, ia, mb, ib) in &sys.slurs {
                let Some(&(pi, ps)) = part_of.get(k) else { continue };
                let at = |m: usize, i: usize| -> Option<Anchor> {
                    let measure = base + m;
                    let v = score
                        .parts
                        .get(pi)?
                        .measures
                        .get(measure)?
                        .staves
                        .get(ps)?
                        .voices
                        .first()?;
                    let onsets = v.onsets();
                    // The i-th sounding item of voice 0 (rests included).
                    let tick = *onsets.get(i.min(onsets.len().saturating_sub(1)))?;
                    Some(Anchor { measure, tick })
                };
                if let (Some(start), Some(end)) = (at(ma, ia), at(mb, ib))
                    && (end.measure, end.tick) > (start.measure, start.tick)
                {
                    slurs.push(Spanner {
                        kind: SpannerKind::Slur,
                        part: pi,
                        staff: ps,
                        voice: 0,
                        start,
                        end,
                        dy: 0,
                        spread: 0,
                    });
                }
            }
        }
        // Clefs of the first measure are the staves' clefs.
        for (k, c) in first_clefs.iter().enumerate() {
            if let (Some(c), Some(&(pi, ps))) = (c, part_of.get(k))
                && let Some(slot) = score.parts.get_mut(pi).and_then(|p| p.clefs.get_mut(ps))
            {
                *slot = *c;
            }
        }
        if let Some(m) = score.measures.first_mut() {
            m.time = m.time.or(Some(time));
            m.key = m.key.or(Some(KeySig::new(0)));
        }
        if let Some(m) = score.measures.last_mut()
            && m.barline == Barline::Regular
        {
            m.barline = Barline::Final;
        }
        score.spanners = slurs;
        score.normalize();
        if !uncertain.is_empty() {
            notes.push(format!(
                "{} measures need a look: their rhythm does not add up.",
                uncertain.len()
            ));
        }
        Recognition {
            score,
            uncertain,
            notes,
        }
    }
}

fn pitch(d: i32, written: Option<i8>, key: KeySig) -> Pitch {
    let step = scoreart_model::Step::from_index(d.rem_euclid(7));
    let alter = written.unwrap_or_else(|| key.alter_for(step));
    Pitch::from_diatonic(d, alter)
}

/// Augmentation dots right of a note or rest (in the spaces of its pitches).
fn dots_right(es: &[&Elem], x1: f32, ds: &[i32], st: &Staff, clef: Clef) -> u8 {
    let sp = st.sp;
    let mut xs: Vec<f32> = es
        .iter()
        .filter(|e| e.sym == Sym::Dot)
        .filter(|e| e.g.bx0 > x1 - 0.1 * sp && e.g.bx0 < x1 + 2.0 * sp)
        .filter(|e| {
            ds.iter().any(|d| {
                let y = st.top + (clef.top_line() - d) as f32 * sp / 2.0;
                e.g.cy() > y - 0.8 * sp && e.g.cy() < y + 0.4 * sp
            })
        })
        .map(|e| e.g.cx())
        .collect();
    xs.sort_by(f32::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 0.3 * sp);
    // Dots follow each other closely.
    let mut n = 0u8;
    let mut last = x1;
    for x in xs {
        if x - last < 1.6 * sp {
            n += 1;
            last = x;
        }
    }
    n.min(4)
}

fn duration(it: &Item) -> Duration {
    let mut d = Duration::new(it.value, it.dots);
    d.tuplet = it.tuplet;
    d
}

/// The voices of a cell: one, or two when stems point both ways at the same time.
fn voices_of(cell: &Cell, len: u32, key: KeySig) -> (Vec<Voice>, u32, bool) {
    let items = &cell.items;
    let total = |v: &[&Item]| -> u32 {
        v.iter()
            .map(|i| if i.measure_rest { len } else { duration(i).ticks() })
            .sum()
    };
    let all: Vec<&Item> = items.iter().collect();
    let one = total(&all);
    let ups = items.iter().filter(|i| i.up == Some(true)).count();
    let downs = items.iter().filter(|i| i.up == Some(false)).count();
    let split = one > len + TPQ / 64 && ups > 0 && downs > 0;
    let groups: Vec<Vec<&Item>> = if split {
        let upper: Vec<&Item> = items
            .iter()
            .filter(|i| i.up == Some(true) || (i.heads.is_empty() && i.rest_pos < 3.0))
            .collect();
        let lower: Vec<&Item> = items
            .iter()
            .filter(|i| i.up == Some(false) || (i.heads.is_empty() && i.rest_pos >= 3.0))
            .collect();
        vec![upper, lower]
    } else {
        vec![all]
    };
    let mut voices = Vec::new();
    let mut longest = 0;
    let mut bad = false;
    for g in groups {
        let mut v = Voice::default();
        let mut t = 0u32;
        for it in &g {
            let d = duration(it);
            let mut e = if it.heads.is_empty() {
                if it.measure_rest {
                    let mut e = Event::rest(Duration::new(NoteValue::Whole, 0));
                    e.duration = Duration::fill(len).first().copied().unwrap_or(e.duration);
                    e
                } else {
                    Event::rest(d)
                }
            } else {
                let notes: Vec<Note> = it
                    .heads
                    .iter()
                    .zip(&it.ties)
                    .map(|(h, tie)| {
                        let mut n = Note::new(pitch(h.0, h.1, key));
                        n.tie = *tie;
                        n
                    })
                    .collect();
                Event::chord(notes, d)
            };
            if it.measure_rest {
                // As long as the measure: one or more rests.
                t += len;
                for r in Duration::fill(len) {
                    v.events.push(Event::rest(r));
                }
                continue;
            }
            e.grace = it.grace.clone();
            e.articulations = it.marks.clone();
            e.dynamic = it.dynamic;
            for (verse, text) in &it.lyrics {
                e.set_lyric(*verse, Some(clip(text)));
            }
            t += e.ticks();
            v.events.push(e);
        }
        if t > len + TPQ / 64 || (t < len && t > 0 && !g.is_empty()) {
            bad = true;
        }
        longest = longest.max(t);
        voices.push(v);
    }
    if voices.is_empty() {
        voices.push(Voice::default());
    }
    // A short voice is fine when it is the only one and the measure is a pickup; the caller
    // decides.
    if voices.len() == 1 && longest < len {
        bad = false;
    }
    let _ = EventKind::Rest;
    (voices, longest, bad)
}

/// Words of text under a staff become the lyrics of the nearest notes.
fn attach_lyrics(items: &mut [Item], text: &[&Glyph], st: &Staff, below: f32, left: f32, right: f32) {
    let sp = st.sp;
    let mut gl: Vec<&&Glyph> = text
        .iter()
        .filter(|t| {
            t.cy() > st.bottom() + 0.8 * sp && t.cy() < below - 0.5 * sp && t.cx() >= left && t.cx() < right
        })
        .filter(|t| t.ch.is_some_and(|c| !c.is_whitespace()))
        .collect();
    if gl.is_empty() {
        return;
    }
    gl.sort_by(|a, b| a.y.total_cmp(&b.y));
    // Lines by baseline.
    let mut rows: Vec<Vec<&&Glyph>> = Vec::new();
    for g in gl {
        match rows.last_mut() {
            Some(r) if r.first().is_some_and(|f| near(f.y, g.y, 0.4 * sp)) => r.push(g),
            _ => rows.push(vec![g]),
        }
    }
    for (verse, mut row) in rows.into_iter().enumerate().take(scoreart_model::MAX_VERSES) {
        row.sort_by(|a, b| a.bx0.total_cmp(&b.bx0));
        // Words: glyphs closer than a space.
        let mut words: Vec<(f32, f32, String)> = Vec::new();
        for g in row {
            let c = g.ch.unwrap_or(' ');
            match words.last_mut() {
                Some(w) if g.bx0 - w.1 < g.size * 0.18 => {
                    w.1 = g.bx1;
                    w.2.push(c);
                }
                _ => words.push((g.bx0, g.bx1, c.to_string())),
            }
        }
        let mut last: Option<usize> = None;
        for (x0, x1, w) in words {
            if w.chars().all(|c| c == '-' || c == '–' || c == '_') {
                if let Some(i) = last
                    && let Some(text) = items.get_mut(i).and_then(|it| it.lyrics.get_mut(&verse))
                    && !text.ends_with('-')
                {
                    text.push('-');
                }
                continue;
            }
            let cx = (x0 + x1) / 2.0;
            let best = items
                .iter()
                .enumerate()
                .filter(|(_, i)| !i.heads.is_empty() && i.hx0 < x1 + sp && i.hx1 > x0 - sp)
                .min_by(|(_, a), (_, b)| {
                    ((a.hx0 + a.hx1) / 2.0 - cx)
                        .abs()
                        .total_cmp(&((b.hx0 + b.hx1) / 2.0 - cx).abs())
                })
                .map(|(i, _)| i);
            if let Some(i) = best
                && let Some(it) = items.get_mut(i)
            {
                it.lyrics.entry(verse).or_insert(w);
                last = Some(i);
            }
        }
    }
}

/// Dynamics under (or over) a staff go to the nearest note or rest.
fn attach_dynamics(items: &mut [Item], es: &[&Elem], st: &Staff, left: f32, right: f32) {
    let sp = st.sp;
    let mut ds: Vec<(&Glyph, &str)> = es
        .iter()
        .filter_map(|e| match e.sym {
            Sym::Dynamic(s) => Some((&e.g, s)),
            _ => None,
        })
        .filter(|(g, _)| {
            g.cx() >= left && g.cx() < right && g.cy() > st.top - 6.0 * sp && g.cy() < st.bottom() + 6.0 * sp
        })
        .filter(|(g, _)| g.cy() > st.bottom() || g.cy() < st.top)
        .collect();
    ds.sort_by(|a, b| a.0.bx0.total_cmp(&b.0.bx0));
    let mut groups: Vec<(f32, f32, String)> = Vec::new();
    for (g, s) in ds {
        match groups.last_mut() {
            Some(gr) if g.bx0 - gr.1 < 0.6 * sp => {
                gr.1 = g.bx1;
                gr.2.push_str(s);
            }
            _ => groups.push((g.bx0, g.bx1, s.to_string())),
        }
    }
    for (x0, x1, s) in groups {
        let Some(d) = Dynamic::from_id(&s) else { continue };
        let cx = (x0 + x1) / 2.0;
        if let Some(it) = items.iter_mut().min_by(|a, b| {
            ((a.hx0 + a.hx1) / 2.0 - cx)
                .abs()
                .total_cmp(&((b.hx0 + b.hx1) / 2.0 - cx).abs())
        }) {
            it.dynamic = Some(d);
        }
    }
}

/// Curves between notes: ties (same pitch, next note) or slurs.
fn curves(sys: &mut System, all: &[crate::collect::Curve]) {
    let n = sys.staves.len();
    // Every item of every staff, in order: (staff, measure, index, x0, x1, ys).
    for c in all {
        let (l, r) = if c.a.0 <= c.b.0 { (c.a, c.b) } else { (c.b, c.a) };
        let Some(si) = (0..n).min_by(|a, b| {
            let da = (sys.staves[*a].mid() - (c.y0 + c.y1) / 2.0).abs();
            let db = (sys.staves[*b].mid() - (c.y0 + c.y1) / 2.0).abs();
            da.total_cmp(&db)
        }) else {
            continue;
        };
        let st = sys.staves[si];
        let sp = st.sp;
        if (c.y1 - c.y0) > 8.0 * sp || r.0 - l.0 < 0.8 * sp {
            continue;
        }
        let pick = |x: f32, y: f32, from_left: bool| -> Option<(usize, usize)> {
            let mut best: Option<((usize, usize), f32)> = None;
            for (mi, row) in sys.measures.iter().enumerate() {
                let Some(cell) = row.get(si) else { continue };
                for (ii, it) in cell.items.iter().enumerate() {
                    if it.heads.is_empty() {
                        continue;
                    }
                    let hx = if from_left { it.hx1 } else { it.hx0 };
                    let dx = (hx - x).abs();
                    let dy = if y < it.hy0 {
                        it.hy0 - y
                    } else {
                        (y - it.hy1).max(0.0)
                    };
                    if dx < 2.5 * sp && dy < 4.5 * sp {
                        let d = dx + dy;
                        if best.is_none_or(|b| d < b.1) {
                            best = Some(((mi, ii), d));
                        }
                    }
                }
            }
            best.map(|b| b.0)
        };
        let (Some(a), b) = (pick(l.0, l.1, true), pick(r.0, r.1, false)) else {
            continue;
        };
        match b {
            Some(b) if b != a => {
                let (ia, ib) = (
                    &sys.measures[a.0][si].items[a.1],
                    &sys.measures[b.0][si].items[b.1],
                );
                // Next item with a common pitch near the curve: a tie.
                let next = (a.0, a.1 + 1) == b
                    || (b.0 == a.0 + 1 && b.1 == 0 && a.1 + 1 == sys.measures[a.0][si].items.len());
                let common: Vec<usize> = ia
                    .heads
                    .iter()
                    .enumerate()
                    .filter(|(_, h)| ib.heads.iter().any(|g| g.0 == h.0))
                    .map(|(i, _)| i)
                    .collect();
                if next && !common.is_empty() && (r.0 - l.0) < 12.0 * sp {
                    // Every pitch the two have in common is tied.
                    if let Some(it) = sys
                        .measures
                        .get_mut(a.0)
                        .and_then(|r| r.get_mut(si))
                        .and_then(|c| c.items.get_mut(a.1))
                    {
                        for i in common {
                            if let Some(t) = it.ties.get_mut(i) {
                                *t = true;
                            }
                        }
                    }
                } else {
                    sys.slurs.push((si, a.0, a.1, b.0, b.1));
                }
            }
            _ => {
                // To the end of the system: a tie into the next system (same pitch decides
                // when the score is put together — kept on the first common head).
                if r.0 > st.x1 - 2.5 * sp
                    && let Some(it) = sys
                        .measures
                        .get_mut(a.0)
                        .and_then(|r| r.get_mut(si))
                        .and_then(|c| c.items.get_mut(a.1))
                    && it.heads.len() == 1
                    && let Some(t) = it.ties.first_mut()
                {
                    *t = true;
                }
            }
        }
    }
}

// ------------------------------------------------------------------ titles

#[derive(Default)]
struct Titles {
    title: String,
    subtitle: String,
    composer: String,
    lyricist: String,
    copyright: String,
}

/// Title, subtitle, composer, lyricist and copyright from the text of the first page.
fn titles(page: &PageContent) -> Titles {
    let ls = lines(&page.segs);
    let first_staff = staves(&ls).first().map_or(page.height * 0.3, |s| s.top);
    let text: Vec<&Glyph> = page
        .glyphs
        .iter()
        .filter(|g| !is_music(g) && g.ch.is_some())
        .collect();
    // Lines of text by baseline (within the top area and the foot).
    let mut rows: Vec<(f32, f32, f32, f32, String)> = Vec::new(); // y, x0, x1, size, text
    let mut sorted: Vec<&&Glyph> = text
        .iter()
        .filter(|g| g.y < first_staff - 6.0 || g.y > page.height * 0.9)
        .collect();
    sorted.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.bx0.total_cmp(&b.bx0)));
    for g in sorted {
        let c = g.ch.unwrap_or(' ');
        match rows
            .iter_mut()
            .find(|r| near(r.0, g.y, g.size * 0.3) && g.bx0 - r.2 < g.size * 1.5 && g.bx0 >= r.1 - 1.0)
        {
            Some(r) => {
                if g.bx0 - r.2 > g.size * 0.15 {
                    r.4.push(' ');
                }
                r.2 = g.bx1;
                r.3 = r.3.max(g.size);
                r.4.push(c);
            }
            None => rows.push((g.y, g.bx0, g.bx1, g.size, c.to_string())),
        }
    }
    let w = page.width;
    let mut t = Titles::default();
    let centred = |r: &(f32, f32, f32, f32, String)| near((r.1 + r.2) / 2.0, w / 2.0, w * 0.08);
    let mut top: Vec<&(f32, f32, f32, f32, String)> = rows.iter().filter(|r| r.0 < first_staff).collect();
    top.sort_by(|a, b| b.3.total_cmp(&a.3));
    if let Some(r) = top.iter().find(|r| centred(r)) {
        t.title = clip(r.4.trim());
    }
    if let Some(r) = top
        .iter()
        .filter(|r| centred(r) && r.4.trim() != t.title)
        .max_by(|a, b| a.3.total_cmp(&b.3))
    {
        t.subtitle = clip(r.4.trim());
    }
    if let Some(r) = top
        .iter()
        .filter(|r| r.1 > w * 0.5)
        .filter(|r| !r.4.contains('=') && r.4.chars().any(char::is_alphabetic))
        .max_by(|a, b| a.0.total_cmp(&b.0))
    {
        t.composer = clip(r.4.trim());
    }
    // (Not the tempo mark: "♩ = 108".)
    let words =
        |r: &&&(f32, f32, f32, f32, String)| !r.4.contains('=') && r.4.chars().any(char::is_alphabetic);
    if let Some(r) = top
        .iter()
        .filter(|r| r.2 < w * 0.5)
        .filter(words)
        .max_by(|a, b| a.0.total_cmp(&b.0))
    {
        t.lyricist = clip(r.4.trim());
    }
    if let Some(r) = rows
        .iter()
        .filter(|r| r.0 > page.height * 0.9 && centred(r))
        .max_by(|a, b| a.0.total_cmp(&b.0))
    {
        t.copyright = clip(r.4.trim());
    }
    t
}
