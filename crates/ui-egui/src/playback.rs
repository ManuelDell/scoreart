//! Playback: builds the sequence, hands a [`Synth`] to the platform's audio output and follows
//! the audio clock with a playhead.

use std::sync::Arc;

use scoreart_engine::Selection;
use scoreart_midi::Sequence;
use scoreart_synth::Synth;

use crate::ScoreApp;

pub struct Playback {
    seq: Arc<Sequence>,
    /// Seconds from the start of the (unrolled) score where playback began.
    from_secs: f64,
    /// Measure and tick inside it currently sounding.
    pub at: Option<(usize, u32)>,
}

impl ScoreApp {
    pub fn is_playing(&self) -> bool {
        self.playback.is_some()
    }

    /// Space: start from the selection (or the input cursor, or the beginning), or stop.
    pub fn toggle_playback(&mut self) {
        if self.playback.is_some() {
            self.stop_playback();
            return;
        }
        let seq = Arc::new(scoreart_midi::sequence(&self.session.score));
        let start_measure = match self.session.selection {
            Some(Selection::Event { ev, .. }) => Some(ev.measure),
            Some(Selection::Measures { first, .. }) => Some(first),
            None if self.session.input.active => Some(self.session.cursor.measure),
            None => None,
        };
        let from_tick = start_measure
            .and_then(|m| {
                seq.measure_starts
                    .iter()
                    .find(|(mm, _)| *mm == m)
                    .map(|(_, t)| *t)
            })
            .unwrap_or(0);
        self.mixer.sync(&self.session.score, self.session.revision);
        let (synth, warnings) = Synth::for_score(
            &self.session.score,
            &seq,
            from_tick,
            self.host.audio_rate(),
            &self.session.sounds,
            Some(self.mixer.controls.clone()),
        );
        match self.host.audio_start(synth) {
            Ok(()) => {
                let from_secs = seq.seconds_at(from_tick);
                self.playback = Some(Playback {
                    seq,
                    from_secs,
                    at: None,
                });
                self.session.status = match warnings.first() {
                    Some(w) => crate::i18n::tf("Playing — {}", &[&crate::i18n::tr(w)]),
                    None => crate::i18n::t("Playing — Space stops").into(),
                };
            }
            Err(e) => self.session.status = crate::i18n::tf("Playback: {}", &[&crate::i18n::tr(&e)]),
        }
    }

    pub fn stop_playback(&mut self) {
        self.host.audio_stop();
        if self.playback.take().is_some() {
            self.session.status.clear();
        }
    }

    /// Per frame: advance the playhead from the audio clock; stop at the end.
    pub(crate) fn follow_playback(&mut self, ctx: &egui::Context) {
        let Some(pb) = &mut self.playback else { return };
        match self.host.audio_position() {
            Some(secs) => {
                let tick = pb.seq.tick_at(pb.from_secs + secs);
                pb.at = pb
                    .seq
                    .measure_starts
                    .iter()
                    .rev()
                    .find(|(_, s)| *s <= tick)
                    .map(|(m, s)| (*m, u32::try_from(tick - s).unwrap_or(0)));
                if tick > pb.seq.end {
                    pb.at = None;
                }
                ctx.request_repaint();
            }
            None => {
                self.playback = None;
                self.session.status.clear();
            }
        }
    }

    /// Plays MIDI keys briefly with the sound of the selected (or first) instrument.
    pub(crate) fn preview_keys(&mut self, keys: &[u8]) {
        if self.playback.is_some() || keys.is_empty() {
            return;
        }
        let part = self.session.focus().0;
        let Some(p) = self
            .session
            .score
            .parts
            .get(part)
            .or(self.session.score.parts.first())
        else {
            return;
        };
        let (sound, _) = scoreart_synth::resolve(&self.session.sounds, &p.sound, p.program);
        let synth = Synth::preview_sound(keys, &sound, self.host.audio_rate());
        let _ = self.host.audio_start(synth);
    }

    /// Plays the selected chord briefly after a pitch was entered or changed.
    pub(crate) fn preview_selection(&mut self) {
        if self.playback.is_some() || !self.sound_on_input {
            return;
        }
        let Some(Selection::Event { ev, note }) = self.session.selection else {
            return;
        };
        let Some(e) = self.session.score.event(ev) else {
            return;
        };
        let keys: Vec<u8> = match note.and_then(|n| e.notes().get(n)) {
            Some(n) => vec![n.pitch.midi()],
            None => e.notes().iter().map(|n| n.pitch.midi()).collect(),
        };
        if keys.is_empty() {
            return;
        }
        let Some(part) = self.session.score.parts.get(ev.part) else {
            return;
        };
        let (sound, _) = scoreart_synth::resolve(&self.session.sounds, &part.sound, part.program);
        let synth = Synth::preview_sound(&keys, &sound, self.host.audio_rate());
        // A failed preview is not worth a message: playback reports audio problems.
        let _ = self.host.audio_start(synth);
    }
}
