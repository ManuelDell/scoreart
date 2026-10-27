//! The ScoreArt egui front end. Thin: it reads [`Session`] state and runs commands by id.
//! The platform (native or web) plugs in through [`Host`] for file dialogs and saving.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod canvas;
mod catalog;
pub mod glyph;
pub mod i18n;
mod i18n_de;
#[cfg(test)]
mod i18n_tests;
mod instruments;
mod midi;
mod mixer;
mod page;
mod panels;
mod playback;
pub mod theme;

use crate::i18n::{t, tf, tr};
use egui::{Key, Modifiers};
use serde_json::{Value, json};

use scoreart_engine::Session;

/// Platform services.
pub trait Host {
    /// Ask the user for a score to open; the result arrives through [`Host::take_opened`].
    fn request_open(&mut self);
    /// A file the user picked (or the host page sent), if any arrived.
    fn take_opened(&mut self) -> Option<(String, Vec<u8>)>;
    /// Store `bytes`. `name` is a file name; `save_as` asks for a location. Returns the name it
    /// was saved under once saving finished (synchronously), or `None` if pending/cancelled.
    fn save(&mut self, name: &str, bytes: Vec<u8>, save_as: bool) -> Option<String>;
    /// Whether the app runs inside a host page that owns files (no New/Open in that case).
    fn embedded(&self) -> bool {
        false
    }
    /// Output sample rate for [`scoreart_synth::Synth`].
    fn audio_rate(&self) -> u32 {
        44_100
    }
    /// Starts playing `synth`, replacing whatever plays.
    fn audio_start(&mut self, _synth: scoreart_synth::Synth) -> Result<(), String> {
        Err("no audio output".into())
    }
    fn audio_stop(&mut self) {}
    /// Called once per frame (e.g. to feed an audio queue).
    fn tick(&mut self) {}
    /// Seconds of the current synth played so far; `None` when nothing is playing.
    fn audio_position(&mut self) -> Option<f64> {
        None
    }
    /// Ask the user for a sound library (a file, or with `folder` a folder of SFZ
    /// instruments); it arrives through [`Host::take_sounds`].
    fn request_sounds(&mut self, _folder: bool) {}
    /// Whether [`Host::request_sounds`] can pick folders.
    fn can_load_folders(&self) -> bool {
        false
    }
    /// A sound library that finished loading (or why it failed), if any.
    fn take_sounds(&mut self) -> Option<Result<scoreart_synth::SoundBank, String>> {
        None
    }
    /// The user removed a library: do not load it again next time.
    fn forget_sounds(&mut self, _name: &str) {}
    /// Whether [`Host::download_sounds`] works here.
    fn can_download(&self) -> bool {
        false
    }
    /// Fetches one instrument of a catalogue library. Called only after the user ticked the
    /// licence acceptance; the library arrives through [`Host::take_sounds`].
    fn download_sounds(&mut self, _library: &'static scoreart_synth::catalog::Library, _instrument: &str) {}
    /// The download in progress, if any.
    fn download_status(&self) -> Option<DownloadStatus> {
        None
    }
    fn cancel_download(&mut self) {}
    /// MIDI input devices that can be opened (may ask the user for permission first).
    fn midi_ports(&mut self) -> Vec<String> {
        Vec::new()
    }
    /// Opens a MIDI input device, or closes the open one with `None`.
    fn midi_open(&mut self, _port: Option<&str>) -> Result<(), String> {
        Err("MIDI input is not available here".into())
    }
    /// The open MIDI device's name.
    fn midi_connected(&self) -> Option<String> {
        None
    }
    /// MIDI messages received since the last call.
    fn take_midi(&mut self) -> Vec<Vec<u8>> {
        Vec::new()
    }
    /// A small setting kept between sessions (e.g. the MIDI bindings).
    fn load_setting(&self, _name: &str) -> Option<String> {
        None
    }
    fn save_setting(&mut self, _name: &str, _value: &str) {}
    /// The language the host page asks for (e.g. the Nextcloud user's); it wins over the saved
    /// choice.
    fn host_language(&self) -> Option<String> {
        None
    }
    /// Whether a catalogue library may be offered (an administrator can hide libraries).
    fn library_allowed(&self, _library: &str) -> bool {
        true
    }
    /// The instruments of a catalogue library the host keeps a copy of (fetched from there
    /// instead of the internet), if it keeps one.
    fn provided_instruments(&self, _library: &str) -> Option<Vec<String>> {
        None
    }
    /// Whether catalogue libraries may be fetched from the internet.
    fn internet_downloads(&self) -> bool {
        true
    }
    /// Sound libraries the host offers (by name), e.g. files an administrator provides.
    fn host_sounds(&self) -> Vec<String> {
        Vec::new()
    }
    /// Loads one of [`Host::host_sounds`]; it arrives through [`Host::take_sounds`].
    fn load_host_sounds(&mut self, _name: &str) {}
    /// The system's or browser's language ("de-DE", "en_US.UTF-8" …).
    fn locale(&self) -> Option<String> {
        None
    }
}

pub use catalog::DownloadStatus;

/// The acceptance record saved with downloaded files.
pub fn acceptance_text(library: &scoreart_synth::catalog::Library, unix_secs: u64) -> String {
    format!(
        "{}, accepted in the ScoreArt app (licence shown in full, box ticked) on {}",
        library.licence.name,
        scoreart_synth::catalog::utc_text(unix_secs)
    )
}

/// Commands after which the selected notes are auditioned.
const PREVIEW_AFTER: &[&str] = &[
    "note.enter",
    "note.add",
    "note.midi",
    "note.pitch-up",
    "note.pitch-down",
    "note.step-up",
    "note.step-down",
    "note.octave-up",
    "note.octave-down",
    "note.accidental",
];

/// A host that does nothing (tests, screenshots).
#[derive(Default)]
pub struct NoHost;

impl Host for NoHost {
    fn request_open(&mut self) {}
    fn take_opened(&mut self) -> Option<(String, Vec<u8>)> {
        None
    }
    fn save(&mut self, _: &str, _: Vec<u8>, _: bool) -> Option<String> {
        None
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Dialog {
    NewScore(panels::NewScore),
    Shortcuts,
    About,
    Text {
        title: &'static str,
        command: &'static str,
        key: &'static str,
        value: String,
    },
}

pub struct ScoreApp {
    pub session: Session,
    pub host: Box<dyn Host>,
    /// Pixels per staff space.
    pub zoom: f32,
    pub(crate) dialog: Option<Dialog>,
    pub(crate) scroll_to_cursor: bool,
    pub(crate) fields: panels::Fields,
    fonts_ready: bool,
    pub show_palette: bool,
    pub show_properties: bool,
    pub(crate) playback: Option<playback::Playback>,
    /// Play notes as they are entered or changed.
    pub sound_on_input: bool,
    pub show_mixer: bool,
    pub(crate) mixer: mixer::Mixer,
    /// The "Free sound libraries" window, when open.
    pub(crate) catalog: Option<catalog::CatalogState>,
    /// The "Instruments and sounds" window and a pending instrument change.
    pub(crate) instruments: instruments::InstrumentsState,
    /// The page settings being edited, while the dialog is open.
    pub(crate) page_settings: Option<scoreart_model::PageFormat>,
    /// MIDI keyboard input and its bindings.
    pub(crate) midi: midi::MidiUi,
}

impl ScoreApp {
    /// Switches the interface language and remembers it.
    pub fn set_language(&mut self, l: i18n::Lang) {
        i18n::set_lang(l);
        self.host.save_setting("language", l.code());
        self.fields = panels::Fields::default();
    }

    /// Opens the "MIDI input" window.
    pub fn open_midi(&mut self) {
        self.midi.open = true;
    }

    /// Opens the "Instruments and sounds" window.
    pub fn open_instruments(&mut self) {
        self.instruments.open = true;
    }

    /// Opens the "Free sound libraries" window.
    pub fn open_catalog(&mut self) {
        if self.catalog.is_none() {
            self.catalog = Some(catalog::CatalogState::default());
        }
    }

    pub fn new(ctx: &egui::Context, session: Session, host: Box<dyn Host>) -> ScoreApp {
        let lang = host
            .host_language()
            .or_else(|| host.load_setting("language"))
            .or_else(|| host.locale())
            .map_or(i18n::Lang::En, |c| i18n::Lang::from_code(&c));
        i18n::set_lang(lang);
        glyph::install_fonts(ctx);
        ScoreApp {
            session,
            host,
            zoom: 7.5,
            dialog: None,
            scroll_to_cursor: false,
            fields: panels::Fields::default(),
            fonts_ready: true,
            show_palette: true,
            show_properties: true,
            playback: None,
            sound_on_input: true,
            show_mixer: false,
            mixer: mixer::Mixer::default(),
            catalog: None,
            instruments: Default::default(),
            page_settings: None,
            midi: Default::default(),
        }
    }

    /// Runs a command and reports failures in the status bar.
    pub fn run(&mut self, id: &str, params: Value) {
        match self.session.execute(id, &params) {
            Ok(_) => {
                self.scroll_to_cursor = true;
                if PREVIEW_AFTER.contains(&id) {
                    self.preview_selection();
                }
                if !id.starts_with("select.") && !id.starts_with("cursor.") {
                    self.session.status.clear();
                }
            }
            Err(e) => self.session.status = e.to_string(),
        }
    }

    /// Title for the window: file name, dirty marker, app name.
    pub fn title(&self) -> String {
        format!(
            "{}{} — ScoreArt",
            if self.session.dirty { "• " } else { "" },
            self.session.file_name
        )
    }

    pub fn open_bytes(&mut self, name: &str, bytes: &[u8]) {
        self.stop_playback();
        match self.session.open_bytes(name, bytes) {
            Ok(()) => {
                self.fields = panels::Fields::default();
                self.scroll_to_cursor = true;
            }
            Err(e) => self.session.status = e.to_string(),
        }
    }

    pub fn save(&mut self, save_as: bool) {
        let mut name = self.session.file_name.clone();
        if scoreart_engine::Format::from_name(&name).is_none_or(|f| {
            !matches!(
                f,
                scoreart_engine::Format::MusicXml | scoreart_engine::Format::Mxl
            )
        }) {
            name = format!(
                "{}.musicxml",
                name.rsplit_once('.').map(|(a, _)| a).unwrap_or(&name)
            );
        }
        self.export(&name, save_as);
    }

    pub fn export(&mut self, name: &str, save_as: bool) {
        match self.session.save_bytes(name) {
            Ok(bytes) => {
                if let Some(saved) = self.host.save(name, bytes, save_as) {
                    self.session.mark_saved(&saved);
                    self.session.status = tf("Saved {}", &[&saved]);
                }
            }
            Err(e) => self.session.status = e.to_string(),
        }
    }

    /// The whole UI for one frame.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if !self.fonts_ready {
            return;
        }
        let ctx = ui.ctx().clone();
        if let Some((name, bytes)) = self.host.take_opened() {
            self.open_bytes(&name, &bytes);
        }
        while let Some(loaded) = self.host.take_sounds() {
            match loaded {
                Ok(bank) => self.add_sounds(bank),
                Err(e) => self.session.status = tf("Sounds: {}", &[&tr(&e)]),
            }
        }
        self.keyboard(&ctx);
        self.host.tick();
        midi::poll(self);
        self.follow_playback(&ctx);
        panels::menu_bar(self, ui);
        panels::toolbar(self, ui);
        panels::status_bar(self, ui);
        if self.show_mixer {
            mixer::panel(self, ui);
        }
        if self.show_palette {
            panels::palette(self, ui);
        }
        if self.show_properties {
            panels::properties(self, ui);
        }
        let tk = theme::Tokens::get(&ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(tk.canvas))
            .show(ui, |ui| {
                canvas::show(self, ui);
            });
        panels::dialogs(self, &ctx);
        catalog::window(self, &ctx);
        instruments::window(self, &ctx);
        page::dialog(self, &ctx);
        midi::window(self, &ctx);
        // Ctrl + mouse wheel zooms.
        let (zoom_delta, cmd) = ctx.input(|i| (i.zoom_delta(), i.modifiers.command));
        if (zoom_delta - 1.0).abs() > 1e-3 {
            self.set_zoom(self.zoom * zoom_delta);
        }
        let _ = cmd;
    }

    pub fn set_zoom(&mut self, z: f32) {
        if z.is_finite() {
            self.zoom = z.clamp(2.0, 40.0);
        }
    }

    fn keyboard(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input()
            || self.dialog.is_some()
            || self.instruments.has_pending()
            || self.page_settings.is_some()
        {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            match ev {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers: m,
                    ..
                } => self.key(key, m),
                // Characters without a key of their own: hairpins.
                egui::Event::Text(t) if t == "<" => self.run("line.add", json!({ "type": "crescendo" })),
                egui::Event::Text(t) if t == ">" => self.run("line.add", json!({ "type": "diminuendo" })),
                _ => {}
            }
        }
    }

    /// Keyboard shortcuts → commands.
    pub fn key(&mut self, key: Key, m: Modifiers) {
        let plain = !m.command && !m.alt && !m.shift;
        let shift_only = m.shift && !m.command && !m.alt;
        let cmd_only = m.command && !m.alt && !m.shift;
        let letter = match key {
            Key::A => Some("a"),
            Key::B => Some("b"),
            Key::C => Some("c"),
            Key::D => Some("d"),
            Key::E => Some("e"),
            Key::F => Some("f"),
            Key::G => Some("g"),
            _ => None,
        };
        let digit = match key {
            Key::Num1 => Some(1),
            Key::Num2 => Some(2),
            Key::Num3 => Some(3),
            Key::Num4 => Some(4),
            Key::Num5 => Some(5),
            Key::Num6 => Some(6),
            Key::Num7 => Some(7),
            Key::Num8 => Some(8),
            _ => None,
        };
        match (key, letter, digit) {
            (_, Some(l), _) if plain => self.run("note.enter", json!({ "step": l })),
            (_, Some(l), _) if shift_only => self.run("note.add", json!({ "step": l })),
            (Key::B, _, _) if cmd_only => self.run("measure.append", json!({})),
            (Key::A, _, _) if cmd_only => self.run("select.all", json!({})),
            (_, _, Some(d)) if plain => self.run("input.duration", json!({ "value": d })),
            (_, _, Some(d)) if m.command && m.alt && d <= 4 => self.run("input.voice", json!({ "voice": d })),
            (Key::Num0, _, _) if plain => self.run("note.rest", json!({})),
            (Key::Period, _, _) if plain => self.run("input.dot", json!({})),
            (Key::N, _, _) if plain => self.run("input.toggle", json!({})),
            (Key::N, _, _) if cmd_only => self.dialog = Some(Dialog::NewScore(panels::NewScore::default())),
            (Key::T, _, _) if plain => self.run("note.tie", json!({})),
            (Key::Space, _, _) if plain => self.toggle_playback(),
            (Key::F10, _, _) if plain => self.show_mixer = !self.show_mixer,
            (Key::I, _, _) if plain => self.instruments.open = !self.instruments.open,
            (Key::Escape, _, _) if self.is_playing() => self.stop_playback(),
            (Key::Escape, _, _) => self.run("edit.escape", json!({})),
            (Key::ArrowUp, _, _) if plain => self.run("note.pitch-up", json!({})),
            (Key::ArrowDown, _, _) if plain => self.run("note.pitch-down", json!({})),
            (Key::ArrowUp, _, _) if cmd_only => self.run("note.octave-up", json!({})),
            (Key::ArrowDown, _, _) if cmd_only => self.run("note.octave-down", json!({})),
            (Key::ArrowUp, _, _) if m.alt && m.shift && !m.command => self.run("note.step-up", json!({})),
            (Key::ArrowDown, _, _) if m.alt && m.shift && !m.command => self.run("note.step-down", json!({})),
            (Key::ArrowUp, _, _) if m.command && m.alt => self.run("select.staff-up", json!({})),
            (Key::ArrowDown, _, _) if m.command && m.alt => self.run("select.staff-down", json!({})),
            (Key::ArrowRight, _, _) if plain => self.run("select.next", json!({})),
            (Key::ArrowRight, _, _) if shift_only => self.run("select.extend-next", json!({})),
            (Key::ArrowLeft, _, _) if shift_only => self.run("select.extend-prev", json!({})),
            (Key::S, _, _) if plain => self.run("line.add", json!({ "type": "slur" })),
            (Key::Slash, _, _) if plain => self.run("note.grace", json!({ "type": "acciaccatura" })),
            (Key::K, _, _) if cmd_only => {
                self.dialog = Some(Dialog::Text {
                    title: t("Chord symbol (e.g. Cmaj7, F#m7/C#)"),
                    command: "note.chord-symbol",
                    key: "text",
                    value: String::new(),
                })
            }
            (Key::T, _, _) if cmd_only => {
                self.dialog = Some(Dialog::Text {
                    title: t("Staff text"),
                    command: "note.text",
                    key: "text",
                    value: String::new(),
                })
            }
            (Key::M, _, _) if cmd_only => self.run("measure.rehearsal", json!({})),
            (Key::ArrowLeft, _, _) if plain => self.run("select.prev", json!({})),
            (Key::ArrowRight, _, _) if cmd_only => self.run("select.next-measure", json!({})),
            (Key::ArrowLeft, _, _) if cmd_only => self.run("select.prev-measure", json!({})),
            (Key::Delete | Key::Backspace, _, _) if plain => self.run("note.delete", json!({})),
            (Key::Delete, _, _) if cmd_only => self.run("measure.delete", json!({})),
            (Key::Insert, _, _) if plain => self.run("measure.insert", json!({})),
            (Key::Z, _, _) if cmd_only => self.run("edit.undo", json!({})),
            (Key::Z, _, _) if m.command && m.shift => self.run("edit.redo", json!({})),
            (Key::Y, _, _) if cmd_only => self.run("edit.redo", json!({})),
            (Key::S, _, _) if cmd_only => self.save(false),
            (Key::S, _, _) if m.command && m.shift => self.save(true),
            (Key::O, _, _) if cmd_only && !self.host.embedded() => self.host.request_open(),
            (Key::L, _, _) if cmd_only => panels::open_lyric_dialog(self),
            (Key::Equals | Key::Plus, _, _) if m.command => self.set_zoom(self.zoom * 1.15),
            (Key::Minus, _, _) if m.command => self.set_zoom(self.zoom / 1.15),
            (Key::Num0, _, _) if cmd_only => self.set_zoom(7.5),
            _ => {}
        }
    }
}
