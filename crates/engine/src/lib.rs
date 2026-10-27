//! The ScoreArt session: the open score, note-input cursor, selection, undo/redo, file I/O and
//! the **command registry**. Everything a user (or an agent) can do is a command with an id;
//! menus, toolbar, palettes, shortcuts and the CLI all dispatch through [`Session::execute`].
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod cmd;
pub mod sample;

use scoreart_engrave::{Layout, Style};
use scoreart_model::edit::{EventRef, Pos};
use scoreart_model::{NoteValue, Pitch, Score};
use scoreart_synth::{SoundBank, Synth};
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;

pub use cmd::{CommandInfo, CommandSpec, registry};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown command: {0}")]
    Unknown(String),
    #[error("{0}")]
    Disabled(String),
    #[error("bad parameter: {0}")]
    Param(String),
    #[error(transparent)]
    Model(#[from] scoreart_model::Error),
    #[error("file: {0}")]
    File(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Selection {
    /// An event (chord or rest); `note` picks one note of a chord.
    Event { ev: EventRef, note: Option<usize> },
    /// One staff of a range of measures.
    Measures {
        part: usize,
        staff: usize,
        first: usize,
        last: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Input {
    pub active: bool,
    pub value: NoteValue,
    pub dots: u8,
    pub voice: usize,
}

impl Default for Input {
    fn default() -> Self {
        Input {
            active: false,
            value: NoteValue::Quarter,
            dots: 0,
            voice: 0,
        }
    }
}

/// Supported file formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    MusicXml,
    Mxl,
    Midi,
    Svg,
    Wav,
    /// One WAV file per instrument, in a zip archive.
    Stems,
}

impl Format {
    pub fn from_name(name: &str) -> Option<Format> {
        let lower = name.to_ascii_lowercase();
        let ext = lower.rsplit('.').next().unwrap_or("");
        match ext {
            "musicxml" | "xml" => Some(Format::MusicXml),
            "mxl" => Some(Format::Mxl),
            "mid" | "midi" => Some(Format::Midi),
            "svg" => Some(Format::Svg),
            "wav" => Some(Format::Wav),
            "zip" => Some(Format::Stems),
            _ => None,
        }
    }
}

pub struct Session {
    pub score: Score,
    pub style: Style,
    /// Where the next note goes in note-input mode.
    pub cursor: Pos,
    pub selection: Option<Selection>,
    /// With an event selection: the other end of a selected range in the same voice.
    pub range_end: Option<EventRef>,
    pub input: Input,
    /// File name (no directories) of the open score.
    pub file_name: String,
    pub dirty: bool,
    /// Bumped on every score change; the UI uses it to know when to re-layout.
    pub revision: u64,
    /// The last entered or selected pitch, for choosing the octave of the next note.
    pub last_pitch: Option<Pitch>,
    /// One-line feedback for the status bar.
    pub status: String,
    /// Sound libraries loaded for playback and audio export (not part of the score).
    pub sounds: Vec<Arc<SoundBank>>,
    undo: Vec<Score>,
    redo: Vec<Score>,
    layout: Option<(u64, std::sync::Arc<Layout>)>,
}

const UNDO_LIMIT: usize = 200;

impl Default for Session {
    fn default() -> Self {
        Session::new(sample::ode_to_joy())
    }
}

impl Session {
    pub fn new(score: Score) -> Session {
        Session {
            score,
            style: Style::default(),
            cursor: Pos::default(),
            selection: None,
            range_end: None,
            input: Input::default(),
            file_name: "Untitled.musicxml".into(),
            dirty: false,
            revision: 1,
            last_pitch: None,
            status: String::new(),
            sounds: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            layout: None,
        }
    }

    /// Runs a command by id. Undoable commands snapshot the score first; a failing command
    /// leaves score and selection exactly as they were.
    pub fn execute(&mut self, id: &str, params: &Value) -> Result<Value> {
        let spec = cmd::find(id).ok_or_else(|| Error::Unknown(id.to_string()))?;
        if let Err(why) = (spec.enabled)(self) {
            return Err(Error::Disabled(why));
        }
        let before = spec.undoable.then(|| self.score.clone());
        let saved = (
            self.cursor,
            self.selection,
            self.input,
            self.last_pitch,
            self.range_end,
        );
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (spec.run)(self, params)));
        let result = match outcome {
            Ok(r) => r,
            Err(_) => Err(Error::Param(format!(
                "{id} failed unexpectedly; the score was restored"
            ))),
        };
        match result {
            Ok(v) => {
                if let Some(b) = before
                    && b != self.score
                {
                    self.undo.push(b);
                    if self.undo.len() > UNDO_LIMIT {
                        self.undo.remove(0);
                    }
                    self.redo.clear();
                    self.touch();
                }
                self.clamp_positions();
                Ok(v)
            }
            Err(e) => {
                if let Some(b) = before {
                    self.score = b;
                }
                (
                    self.cursor,
                    self.selection,
                    self.input,
                    self.last_pitch,
                    self.range_end,
                ) = saved;
                self.status = e.to_string();
                Err(e)
            }
        }
    }

    /// Marks the score changed.
    pub fn touch(&mut self) {
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(crate) fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(s) => {
                let cur = std::mem::replace(&mut self.score, s);
                self.redo.push(cur);
                self.touch();
                self.clamp_positions();
                true
            }
            None => false,
        }
    }

    pub(crate) fn redo(&mut self) -> bool {
        match self.redo.pop() {
            Some(s) => {
                let cur = std::mem::replace(&mut self.score, s);
                self.undo.push(cur);
                self.touch();
                self.clamp_positions();
                true
            }
            None => false,
        }
    }

    /// Keeps cursor and selection pointing at things that exist.
    fn clamp_positions(&mut self) {
        let s = &self.score;
        let parts = s.parts.len();
        if parts == 0 {
            self.cursor = Pos::default();
            self.selection = None;
            return;
        }
        let c = &mut self.cursor;
        c.part = c.part.min(parts - 1);
        let staves = s.parts.get(c.part).map(|p| p.staff_count()).unwrap_or(1).max(1);
        c.staff = c.staff.min(staves - 1);
        c.measure = c.measure.min(s.measures.len().saturating_sub(1));
        c.voice = c.voice.min(scoreart_model::limits::MAX_VOICES - 1);
        if c.tick >= s.measure_ticks(c.measure) {
            c.tick = 0;
        }
        match self.selection {
            Some(Selection::Event { ev, note }) => match s.event(ev) {
                Some(e) => {
                    let n = e.notes().len();
                    let note = note.filter(|i| *i < n);
                    self.selection = Some(Selection::Event { ev, note });
                }
                None => self.selection = None,
            },
            Some(Selection::Measures {
                part,
                staff,
                first,
                last,
            }) => {
                let ok = s.staff_measure(part, staff, first).is_some()
                    && s.staff_measure(part, staff, last).is_some();
                if !ok {
                    self.selection = None;
                }
            }
            None => {}
        }
    }

    /// The engraved layout of the current score (cached per revision).
    pub fn layout(&mut self) -> std::sync::Arc<Layout> {
        match &self.layout {
            Some((r, l)) if *r == self.revision => l.clone(),
            _ => {
                let l = std::sync::Arc::new(scoreart_engrave::layout(&self.score, &self.style));
                self.layout = Some((self.revision, l.clone()));
                l
            }
        }
    }

    /// Opens a score from bytes; the format is taken from the file name (MusicXML or MXL).
    pub fn open_bytes(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        let score = scoreart_musicxml::from_bytes(bytes).map_err(|e| Error::File(e.to_string()))?;
        let file_name = std::path::Path::new(name)
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled.musicxml".into());
        *self = Session {
            file_name,
            style: self.style.clone(),
            sounds: std::mem::take(&mut self.sounds),
            ..Session::new(score)
        };
        let lower = self.file_name.to_ascii_lowercase();
        self.status = if lower.ends_with(".mscz") || lower.ends_with(".mscx") {
            format!("Opened {} (MuseScore) — Save writes MusicXML", self.file_name)
        } else {
            format!("Opened {}", self.file_name)
        };
        Ok(())
    }

    /// Serialises the score in the format given by `name`'s extension.
    pub fn save_bytes(&self, name: &str) -> Result<Vec<u8>> {
        match Format::from_name(name) {
            Some(Format::MusicXml) => Ok(scoreart_musicxml::to_string(&self.score).into_bytes()),
            Some(Format::Mxl) => {
                scoreart_musicxml::to_mxl(&self.score).map_err(|e| Error::File(e.to_string()))
            }
            Some(Format::Midi) => Ok(scoreart_midi::to_smf(&self.score)),
            Some(Format::Wav) => Ok(wav_with(&self.score, &self.sounds)),
            Some(Format::Stems) => stems(&self.score, &self.sounds),
            Some(Format::Svg) => {
                let l = scoreart_engrave::layout(&self.score, &self.style);
                Ok(scoreart_render::svg_pages(&l, &self.style)
                    .into_iter()
                    .next()
                    .unwrap_or_default()
                    .into_bytes())
            }
            None => Err(Error::File(format!("unknown file type: {name}"))),
        }
    }

    /// After saving under `name`: remember it and clear the dirty flag.
    pub fn mark_saved(&mut self, name: &str) {
        if matches!(Format::from_name(name), Some(Format::MusicXml | Format::Mxl)) {
            self.file_name = std::path::Path::new(name)
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| name.to_string());
            self.dirty = false;
        }
    }

    /// First and last event of the selection, in time order (one voice).
    pub fn selected_range(&self) -> Option<(EventRef, EventRef)> {
        match self.selection {
            Some(Selection::Event { ev, .. }) => {
                let other = self
                    .range_end
                    .filter(|r| {
                        r.part == ev.part
                            && r.staff == ev.staff
                            && r.voice == ev.voice
                            && self.score.event(*r).is_some()
                    })
                    .unwrap_or(ev);
                if (other.measure, other.index) < (ev.measure, ev.index) {
                    Some((other, ev))
                } else {
                    Some((ev, other))
                }
            }
            Some(Selection::Measures {
                part,
                staff,
                first,
                last,
            }) => {
                let n = self
                    .score
                    .voice(part, staff, last, 0)
                    .map(|v| v.events.len())
                    .unwrap_or(1);
                Some((
                    EventRef {
                        part,
                        staff,
                        voice: 0,
                        measure: first,
                        index: 0,
                    },
                    EventRef {
                        part,
                        staff,
                        voice: 0,
                        measure: last,
                        index: n.saturating_sub(1),
                    },
                ))
            }
            None => None,
        }
    }

    /// Every event in the selected range.
    pub fn selected_events(&self) -> Vec<EventRef> {
        let Some((a, b)) = self.selected_range() else {
            return Vec::new();
        };
        let mut out = vec![a];
        let mut cur = a;
        while cur != b && out.len() < 4096 {
            match self.score.next_event(cur) {
                Some(n) => {
                    out.push(n);
                    cur = n;
                }
                None => break,
            }
        }
        out
    }

    /// The selected event, if any.
    pub fn selected_event(&self) -> Option<EventRef> {
        match self.selection {
            Some(Selection::Event { ev, .. }) => Some(ev),
            _ => None,
        }
    }

    /// Part, staff and measure of the selection or, failing that, the cursor.
    pub fn focus(&self) -> (usize, usize, usize) {
        match self.selection {
            Some(Selection::Event { ev, .. }) => (ev.part, ev.staff, ev.measure),
            Some(Selection::Measures {
                part, staff, first, ..
            }) => (part, staff, first),
            None => (self.cursor.part, self.cursor.staff, self.cursor.measure),
        }
    }

    /// A human-readable description of the selection for the status bar.
    pub fn describe_selection(&self) -> String {
        match self.selection {
            Some(Selection::Event { ev, note }) => {
                let Some(e) = self.score.event(ev) else {
                    return String::new();
                };
                let what = if e.is_rest() {
                    "Rest".to_string()
                } else {
                    let names: Vec<String> = match note.and_then(|i| e.notes().get(i)) {
                        Some(n) => vec![n.pitch.name()],
                        None => e.notes().iter().map(|n| n.pitch.name()).collect(),
                    };
                    format!("Note {}", names.join(" "))
                };
                let dots = ".".repeat(usize::from(e.duration.dots));
                let part = self
                    .score
                    .parts
                    .get(ev.part)
                    .map(|p| p.name.as_str())
                    .unwrap_or("");
                format!(
                    "{what} · {}{dots} · Measure {} · Beat {} · {part} · Voice {}",
                    e.duration.value.xml_name(),
                    ev.measure + 1,
                    ev.pos(&self.score)
                        .map(|p| beat_label(&self.score, p))
                        .unwrap_or_default(),
                    ev.voice + 1
                )
            }
            Some(Selection::Measures { first, last, .. }) => {
                if first == last {
                    format!("Measure {}", first + 1)
                } else {
                    format!("Measures {}–{}", first + 1, last + 1)
                }
            }
            None => String::new(),
        }
    }
}

fn beat_label(score: &Score, p: Pos) -> String {
    let ts = score.time_at(p.measure);
    let beat = scoreart_model::TPQ * 4 / u32::from(ts.beat_type.max(1));
    let b = p.tick / beat.max(1) + 1;
    let rem = p.tick % beat.max(1);
    if rem == 0 {
        b.to_string()
    } else {
        format!("{b}+{}", rem * 100 / beat.max(1))
    }
}

#[cfg(test)]
mod tests;

const WAV_RATE: u32 = 44_100;
/// Longest audio export.
const WAV_MAX_SECS: f64 = 1800.0;

/// A 16-bit WAV file from interleaved stereo samples.
fn wav_file(pcm: &[f32], rate: u32) -> Vec<u8> {
    let data_len = (pcm.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + pcm.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&2u16.to_le_bytes()); // stereo
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for v in pcm {
        out.extend_from_slice(&((v.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    out
}

/// Renders the whole score with the built-in synth as a 16-bit stereo WAV file.
pub fn wav(score: &Score) -> Vec<u8> {
    wav_with(score, &[])
}

/// Renders the whole score with each part's chosen sound as a 16-bit stereo WAV file.
pub fn wav_with(score: &Score, sounds: &[Arc<SoundBank>]) -> Vec<u8> {
    let seq = scoreart_midi::sequence(score);
    let (mut synth, _) = Synth::for_score(score, &seq, 0, WAV_RATE, sounds, None);
    let frames = (synth.duration().min(WAV_MAX_SECS) * f64::from(WAV_RATE)) as usize;
    let mut pcm = vec![0.0f32; frames * 2];
    synth.render(&mut pcm);
    wav_file(&pcm, WAV_RATE)
}

/// One WAV file per instrument ("stems") in a zip archive, numbered in score order.
pub fn stems(score: &Score, sounds: &[Arc<SoundBank>]) -> Result<Vec<u8>> {
    use std::io::Write;
    let seq = scoreart_midi::sequence(score);
    let (mut synth, _) = Synth::for_score(score, &seq, 0, WAV_RATE, sounds, None);
    let tracks = synth.render_stems(WAV_MAX_SECS);
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts =
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (part, pcm) in tracks {
            let name = score.parts.get(part).map_or("Part", |p| p.name.as_str());
            let safe: String = name
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == ' ' || c == '-' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            let file = format!("{:02} {}.wav", part + 1, safe.trim());
            z.start_file(file, opts).map_err(|e| Error::File(e.to_string()))?;
            z.write_all(&wav_file(&pcm, WAV_RATE))
                .map_err(|e| Error::File(e.to_string()))?;
        }
        z.finish().map_err(|e| Error::File(e.to_string()))?;
    }
    Ok(buf.into_inner())
}
