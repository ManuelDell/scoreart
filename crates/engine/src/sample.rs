//! The built-in sample score: the "Ode to Joy" theme (Beethoven, 1824; words by Schiller, 1785 —
//! both public domain), in an easy piano arrangement written for ScoreArt.

use scoreart_model::edit::Pos;
use scoreart_model::{
    Duration, Dynamic, EventKind, KeySig, Note, NoteValue, Pitch, Score, Step, Tempo, TimeSig, instrument,
};

fn p(step: Step, octave: i32) -> Pitch {
    // D major: F and C are sharp.
    let alter = KeySig::new(2).alter_for(step);
    Pitch::new(step, alter, octave)
}

fn chord(ps: &[Pitch]) -> EventKind {
    EventKind::Chord(ps.iter().map(|p| Note::new(*p)).collect())
}

pub fn ode_to_joy() -> Score {
    use NoteValue::*;
    use Step::*;
    let Some(piano) = instrument("piano") else {
        return Score::default();
    };
    let mut s = Score::new("Ode to Joy", &[piano], TimeSig::default(), KeySig::new(2), 8);
    s.meta.subtitle = "Theme from the Ninth Symphony".into();
    s.meta.composer = "Ludwig van Beethoven".into();
    s.meta.lyricist = "Friedrich Schiller".into();
    if let Some(m) = s.measures.first_mut() {
        m.tempo = Some(Tempo { bpm: 108 });
    }

    let q = Duration::new(Quarter, 0);
    let dq = Duration::new(Quarter, 1);
    let e = Duration::new(Eighth, 0);
    let h = Duration::new(Half, 0);
    let melody: [(Step, i32, Duration); 30] = [
        (F, 4, q),
        (F, 4, q),
        (G, 4, q),
        (A, 4, q),
        (A, 4, q),
        (G, 4, q),
        (F, 4, q),
        (E, 4, q),
        (D, 4, q),
        (D, 4, q),
        (E, 4, q),
        (F, 4, q),
        (F, 4, dq),
        (E, 4, e),
        (E, 4, h),
        (F, 4, q),
        (F, 4, q),
        (G, 4, q),
        (A, 4, q),
        (A, 4, q),
        (G, 4, q),
        (F, 4, q),
        (E, 4, q),
        (D, 4, q),
        (D, 4, q),
        (E, 4, q),
        (F, 4, q),
        (E, 4, dq),
        (D, 4, e),
        (D, 4, h),
    ];
    let words = [
        "Freu", "de,", "schö", "ner", "Göt", "ter", "fun", "ken,", "Toch", "ter", "aus", "E", "ly", "si",
        "um,", "Wir", "be", "tre", "ten", "feu", "er", "trun", "ken,", "Himm", "li", "sche,", "dein", "Hei",
        "lig", "tum!",
    ];
    let mut pos = Pos::default();
    for (i, (st, oct, d)) in melody.iter().enumerate() {
        let before = pos;
        pos = s.overwrite(pos, chord(&[p(*st, *oct)]), *d).unwrap_or(pos);
        if let Some(r) = before.event(&s)
            && let Some(ev) = s.event_mut(r)
        {
            ev.lyric = words.get(i).map(|w| w.to_string());
            if i == 0 {
                ev.dynamic = Dynamic::from_id("mp");
            }
            if i == 15 {
                ev.dynamic = Dynamic::from_id("mf");
            }
        }
    }

    let tonic = [p(D, 3), p(A, 3)];
    let dom = [p(A, 2), p(E, 3)];
    let dom7 = [p(A, 2), p(C, 3), p(G, 3)];
    let left: Vec<(Vec<Pitch>, Duration)> = vec![
        (tonic.to_vec(), h),
        (tonic.to_vec(), h),
        (tonic.to_vec(), h),
        (dom.to_vec(), h),
        (tonic.to_vec(), h),
        (tonic.to_vec(), h),
        (dom.to_vec(), h),
        (dom7.to_vec(), h),
        (tonic.to_vec(), h),
        (tonic.to_vec(), h),
        (tonic.to_vec(), h),
        (dom.to_vec(), h),
    ];
    let mut pos = Pos {
        staff: 1,
        ..Pos::default()
    };
    for (ps, d) in &left {
        pos = s.overwrite(pos, chord(ps), *d).unwrap_or(pos);
    }
    // Measure 7: broken chord in eighths.
    for st in [D, F, A, F, D, F, A, F] {
        let oct = 3;
        pos = s.overwrite(pos, chord(&[p(st, oct)]), e).unwrap_or(pos);
    }
    for (ps, d) in [(dom7.to_vec(), h), (vec![p(D, 3), p(F, 3), p(A, 3)], h)] {
        pos = s.overwrite(pos, chord(&ps), d).unwrap_or(pos);
    }
    s
}
