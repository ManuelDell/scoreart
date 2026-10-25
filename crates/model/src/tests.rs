use crate::edit::Pos;
use crate::*;

fn piano_score(measures: usize) -> Score {
    let piano = instrument("piano").unwrap();
    Score::new("Test", &[piano], TimeSig::default(), KeySig::default(), measures)
}

fn q() -> Duration {
    Duration::new(NoteValue::Quarter, 0)
}

#[test]
fn new_score_is_filled_with_rests() {
    let s = piano_score(4);
    assert_eq!(s.measures.len(), 4);
    assert_eq!(s.parts[0].staff_count(), 2);
    for m in 0..4 {
        let v = s.voice(0, 0, m, 0).unwrap();
        assert_eq!(v.ticks(), TPQ * 4);
        assert!(v.is_empty());
    }
    assert_eq!(s.measures[3].barline, Barline::Final);
}

#[test]
fn pitch_midi_round_trip() {
    let c4 = Pitch::new(Step::C, 0, 4);
    assert_eq!(c4.midi(), 60);
    assert_eq!(c4.diatonic(), 28);
    for m in 21..=108 {
        assert_eq!(Pitch::from_midi(m, KeySig::new(0)).midi() as i32, m);
        assert_eq!(Pitch::from_midi(m, KeySig::new(-5)).midi() as i32, m);
        assert_eq!(Pitch::from_midi(m, KeySig::new(7)).midi() as i32, m);
    }
    // In C# major, C (midi 60) is spelled B#3.
    let p = Pitch::from_midi(60, KeySig::new(7));
    assert_eq!((p.step, p.alter, p.octave), (Step::B, 1, 3));
}

#[test]
fn key_alterations() {
    let d = KeySig::new(2);
    assert_eq!(d.alter_for(Step::F), 1);
    assert_eq!(d.alter_for(Step::C), 1);
    assert_eq!(d.alter_for(Step::G), 0);
    let bb = KeySig::new(-2);
    assert_eq!(bb.alter_for(Step::B), -1);
    assert_eq!(bb.alter_for(Step::E), -1);
    assert_eq!(bb.alter_for(Step::A), 0);
}

#[test]
fn durations() {
    assert_eq!(Duration::new(NoteValue::Quarter, 1).ticks(), TPQ * 3 / 2);
    assert_eq!(Duration::new(NoteValue::Half, 2).ticks(), TPQ * 7 / 2);
    let trip = Duration {
        value: NoteValue::Eighth,
        dots: 0,
        tuplet: Some(Tuplet { actual: 3, normal: 2 }),
    };
    assert_eq!(trip.ticks() * 3, TPQ);
    let parts = Duration::decompose(TPQ * 5);
    assert_eq!(parts.iter().map(|d| d.ticks()).sum::<u32>(), TPQ * 5);
    assert_eq!(parts.len(), 2);
}

#[test]
fn overwrite_writes_notes_and_advances() {
    let mut s = piano_score(2);
    let mut pos = Pos::default();
    for step in [Step::C, Step::D, Step::E, Step::F, Step::G] {
        let kind = EventKind::Chord(vec![Note::new(Pitch::new(step, 0, 4))]);
        pos = s.overwrite(pos, kind, q()).unwrap();
    }
    assert_eq!((pos.measure, pos.tick), (1, TPQ));
    let v = s.voice(0, 0, 0, 0).unwrap();
    assert_eq!(v.events.len(), 4);
    assert!(v.events.iter().all(|e| !e.is_rest()));
    let v1 = s.voice(0, 0, 1, 0).unwrap();
    assert_eq!(v1.ticks(), TPQ * 4);
    assert!(!v1.events[0].is_rest());
}

#[test]
fn overwrite_across_barline_ties() {
    let mut s = piano_score(1);
    let pos = Pos {
        tick: TPQ * 3,
        ..Pos::default()
    };
    let kind = EventKind::Chord(vec![Note::new(Pitch::new(Step::A, 0, 4))]);
    let after = s.overwrite(pos, kind, Duration::new(NoteValue::Half, 0)).unwrap();
    assert_eq!(s.measures.len(), 2, "a measure is appended");
    let last = s.voice(0, 0, 0, 0).unwrap().events.last().unwrap();
    assert!(last.notes()[0].tie);
    let next = &s.voice(0, 0, 1, 0).unwrap().events[0];
    assert_eq!(next.ticks(), TPQ);
    assert!(!next.notes()[0].tie);
    assert_eq!((after.measure, after.tick), (1, TPQ));
    assert_eq!(s.measures[1].barline, Barline::Final);
    assert_eq!(s.measures[0].barline, Barline::Regular);
}

#[test]
fn overwrite_cuts_long_event() {
    let mut s = piano_score(1);
    let kind = EventKind::Chord(vec![Note::new(Pitch::new(Step::C, 0, 5))]);
    s.overwrite(Pos::default(), kind.clone(), Duration::new(NoteValue::Whole, 0))
        .unwrap();
    s.overwrite(
        Pos {
            tick: TPQ,
            ..Pos::default()
        },
        EventKind::Rest,
        q(),
    )
    .unwrap();
    let v = s.voice(0, 0, 0, 0).unwrap();
    assert_eq!(v.ticks(), TPQ * 4);
    assert!(!v.events[0].is_rest());
    assert_eq!(v.events[0].ticks(), TPQ);
    assert!(v.events[1].is_rest());
}

#[test]
fn time_change_refits_measures() {
    let mut s = piano_score(2);
    s.measures[0].time = Some(TimeSig::new(3, 4).unwrap());
    s.normalize();
    assert_eq!(s.voice(0, 0, 1, 0).unwrap().ticks(), TPQ * 3);
    assert_eq!(s.time_at(1).beats, 3);
}

#[test]
fn delete_and_insert_measures() {
    let mut s = piano_score(3);
    s.insert_measures(1, 2).unwrap();
    assert_eq!(s.measures.len(), 5);
    assert_eq!(s.parts[0].measures.len(), 5);
    s.delete_measures(0, 4).unwrap();
    assert_eq!(s.measures.len(), 1);
    assert!(s.measures[0].time.is_some());
    assert_eq!(s.measures[0].barline, Barline::Final);
    assert!(s.delete_measures(0, 1).is_err());
}

#[test]
fn secondary_voice_created_on_demand() {
    let mut s = piano_score(1);
    let pos = Pos {
        voice: 1,
        ..Pos::default()
    };
    let kind = EventKind::Chord(vec![Note::new(Pitch::new(Step::E, 0, 4))]);
    s.overwrite(pos, kind, q()).unwrap();
    assert_eq!(s.staff_measure(0, 0, 0).unwrap().voices.len(), 2);
    assert_eq!(s.voice(0, 0, 0, 1).unwrap().ticks(), TPQ * 4);
}
