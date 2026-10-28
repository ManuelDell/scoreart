//! The order measures are played in: repeats, voltas (1st/2nd endings) and the jumps D.C.,
//! D.S., al Fine, al Coda, To Coda.
//!
//! Conventions (as players read them):
//! - `|: … :|` is played twice, or as often as its endings ask for (endings "1." "2." "3.").
//! - An ending is played only on its own pass.
//! - D.C. goes back to the start, D.S. to the segno; each is taken once. After such a jump
//!   repeats are not taken again and only the last ending of each repeat is played.
//! - "al Fine" stops at Fine, "al Coda" goes from To Coda to the coda — both only after the
//!   jump.

use scoreart_model::{Barline, Jump, Score};

/// The passes an ending is played on: its number, or the numbers in its label ("1., 3.").
fn ending_passes(score: &Score, m: usize) -> Vec<u8> {
    let Some(info) = score.measures.get(m) else {
        return Vec::new();
    };
    let Some(n) = info.volta else {
        return Vec::new();
    };
    let from_text: Vec<u8> = info
        .volta_text
        .as_deref()
        .unwrap_or("")
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|d| d.parse::<u8>().ok())
        .filter(|d| (1..=9).contains(d))
        .collect();
    if from_text.is_empty() { vec![n] } else { from_text }
}

/// The measures of the endings that follow (or contain) the repeat ending at `m`.
fn endings_around(score: &Score, m: usize) -> std::ops::Range<usize> {
    let has = |i: usize| score.measures.get(i).is_some_and(|x| x.volta.is_some());
    let mut a = m;
    while a > 0 && has(a) && has(a - 1) {
        a -= 1;
    }
    if !has(a) {
        a = m + 1;
    }
    let mut b = a;
    while has(b) {
        b += 1;
    }
    a..b
}

/// How many passes the repeat from `start` ending at `m` makes: two, or as many as the endings
/// inside it or right after it name (a song whose refrain closes the repeat counts the passes
/// of the verse endings: "1.+3." … "2.4.5.").
fn passes(score: &Score, start: usize, m: usize) -> u8 {
    let around = endings_around(score, m);
    (start.min(m)..=m)
        .chain(around)
        .flat_map(|i| ending_passes(score, i))
        .max()
        .unwrap_or(2)
        .clamp(2, 9)
}

/// The highest pass any ending of the group containing `m` is played on.
fn last_ending(score: &Score, m: usize) -> u8 {
    let has = |i: usize| score.measures.get(i).is_some_and(|x| x.volta.is_some());
    let mut a = m;
    while a > 0 && has(a - 1) {
        a -= 1;
    }
    let mut b = m;
    while has(b) {
        b += 1;
    }
    (a..b).flat_map(|i| ending_passes(score, i)).max().unwrap_or(1)
}

fn find(score: &Score, from: usize, which: impl Fn(&Jump) -> bool) -> Option<usize> {
    (from..score.measures.len())
        .chain(0..from)
        .find(|&i| score.measures.get(i).is_some_and(|x| x.jumps.iter().any(&which)))
}

#[derive(Clone, Copy, PartialEq)]
enum After {
    /// No D.C./D.S. taken yet.
    First,
    /// Jumped back; play to the end (or Fine / To Coda as the jump says).
    Plain,
    Fine,
    Coda,
}

pub fn play_order(score: &Score) -> Vec<usize> {
    let n = score.measures.len();
    let limit = scoreart_model::limits::MAX_MEASURES * 4;
    let mut order = Vec::new();
    let mut after = After::First;
    // The measure the current repeat goes back to, and which pass it is on.
    let mut start = 0usize;
    let mut pass: u8 = 1;
    let mut m = 0usize;
    while m < n && order.len() < limit {
        let Some(info) = score.measures.get(m) else { break };
        // Entering a repeat (coming back to it keeps the pass count).
        if info.repeat_start && start != m {
            start = m;
            pass = 1;
        }
        // Endings: only on their own pass (after a jump: only the last ending).
        let endings = ending_passes(score, m);
        let wanted = if after == After::First {
            pass
        } else {
            last_ending(score, m)
        };
        if !endings.is_empty() && !endings.contains(&wanted) {
            m += 1;
            continue;
        }
        order.push(m);
        // Jumps at the end of the measure.
        if after != After::First {
            if after == After::Fine && info.jumps.contains(&Jump::Fine) {
                break;
            }
            if after == After::Coda && info.jumps.contains(&Jump::ToCoda) {
                match find(score, m + 1, |j| matches!(j, Jump::Coda | Jump::VariedCoda)) {
                    Some(c) if c > m => {
                        m = c;
                        after = After::Plain;
                        continue;
                    }
                    _ => break,
                }
            }
        }
        if info.barline == Barline::RepeatEnd && after == After::First {
            if pass < passes(score, start, m) {
                pass += 1;
                m = start;
                continue;
            }
            // Done with this repeat; the next one starts fresh.
            pass = 1;
            start = m + 1;
        }
        if after == After::First
            && let Some(j) = info.jumps.iter().find(|j| {
                matches!(
                    j,
                    Jump::DaCapo
                        | Jump::DaCapoAlFine
                        | Jump::DaCapoAlCoda
                        | Jump::DalSegno
                        | Jump::DalSegnoAlFine
                        | Jump::DalSegnoAlCoda
                )
            })
        {
            after = match j {
                Jump::DaCapoAlFine | Jump::DalSegnoAlFine => After::Fine,
                Jump::DaCapoAlCoda | Jump::DalSegnoAlCoda => After::Coda,
                _ => After::Plain,
            };
            let to = match j {
                Jump::DalSegno | Jump::DalSegnoAlFine | Jump::DalSegnoAlCoda => {
                    find(score, 0, |j| *j == Jump::Segno).unwrap_or(0)
                }
                _ => 0,
            };
            m = to;
            start = to;
            pass = 1;
            continue;
        }
        m += 1;
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use scoreart_model::*;

    fn score(n: usize) -> Score {
        let flute = instrument("flute").unwrap();
        Score::new("T", &[flute], TimeSig::default(), KeySig::default(), n)
    }

    #[test]
    fn verse_endings_and_a_refrain_inside_the_repeat() {
        // Verse | 1.+3. :| 2.4.5. | Refrain :|  — five verses, the refrain after 2, 4 and 5.
        let mut s = score(5);
        s.measures[1].volta = Some(1);
        s.measures[1].volta_text = Some("1.+3.".into());
        s.measures[1].barline = Barline::RepeatEnd;
        s.measures[2].volta = Some(2);
        s.measures[2].volta_text = Some("2.4.5.".into());
        s.measures[4].barline = Barline::RepeatEnd;
        assert_eq!(
            play_order(&s),
            vec![0, 1, 0, 2, 3, 4, 0, 1, 0, 2, 3, 4, 0, 2, 3, 4]
        );
    }

    #[test]
    fn repeats_unroll_once() {
        let mut s = score(3);
        s.measures[1].barline = Barline::RepeatEnd;
        assert_eq!(play_order(&s), vec![0, 1, 0, 1, 2]);
    }

    #[test]
    fn first_and_second_endings() {
        // A |: B | 1. C :| 2. D | E
        let mut s = score(5);
        s.measures[1].repeat_start = true;
        s.measures[2].volta = Some(1);
        s.measures[2].barline = Barline::RepeatEnd;
        s.measures[3].volta = Some(2);
        assert_eq!(play_order(&s), vec![0, 1, 2, 1, 3, 4]);
    }

    #[test]
    fn three_endings_and_shared_label() {
        // |: A | 1.2. B :| 3. C
        let mut s = score(3);
        s.measures[1].volta = Some(1);
        s.measures[1].volta_text = Some("1., 2.".into());
        s.measures[1].barline = Barline::RepeatEnd;
        s.measures[2].volta = Some(3);
        assert_eq!(play_order(&s), vec![0, 1, 0, 1, 0, 2]);
    }

    #[test]
    fn da_capo_al_fine() {
        // A | B (Fine) | C | D (D.C. al Fine)
        let mut s = score(4);
        s.measures[1].jumps.push(Jump::Fine);
        s.measures[3].jumps.push(Jump::DaCapoAlFine);
        assert_eq!(play_order(&s), vec![0, 1, 2, 3, 0, 1]);
    }

    #[test]
    fn dal_segno_al_coda_skips_repeats() {
        // A | %B :| C (To Coda) | D (D.S. al Coda) | ⊕E
        let mut s = score(5);
        s.measures[1].jumps.push(Jump::Segno);
        s.measures[1].barline = Barline::RepeatEnd;
        s.measures[2].jumps.push(Jump::ToCoda);
        s.measures[3].jumps.push(Jump::DalSegnoAlCoda);
        s.measures[4].jumps.push(Jump::Coda);
        assert_eq!(play_order(&s), vec![0, 1, 0, 1, 2, 3, 1, 2, 4]);
    }

    #[test]
    fn after_da_capo_only_the_last_ending() {
        // |: A | 1. B :| 2. C (D.C.)
        let mut s = score(3);
        s.measures[1].volta = Some(1);
        s.measures[1].barline = Barline::RepeatEnd;
        s.measures[2].volta = Some(2);
        s.measures[2].jumps.push(Jump::DaCapo);
        assert_eq!(play_order(&s), vec![0, 1, 0, 2, 0, 2]);
    }

    #[test]
    fn a_jump_without_its_target_still_ends() {
        let mut s = score(2);
        s.measures[0].jumps.push(Jump::ToCoda);
        s.measures[1].jumps.push(Jump::DaCapoAlCoda);
        assert_eq!(play_order(&s), vec![0, 1, 0]);
    }
}
