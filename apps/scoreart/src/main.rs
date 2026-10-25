//! ScoreArt desktop app.
//!
//! Usage: `scoreart [score.musicxml|score.mxl]`
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod audio;

use std::path::PathBuf;

use scoreart_engine::Session;
use scoreart_ui_egui::{Host, ScoreApp};

#[derive(Default)]
struct NativeHost {
    opened: Option<(String, Vec<u8>)>,
    /// Where the open score lives on disk.
    path: Option<PathBuf>,
    audio: audio::Audio,
    audio_rate: u32,
}

impl NativeHost {
    fn load(&mut self, path: PathBuf) {
        match std::fs::read(&path) {
            Ok(bytes) => {
                let name = path.to_string_lossy().into_owned();
                self.path = Some(path);
                self.opened = Some((name, bytes));
            }
            Err(e) => eprintln!("scoreart: cannot read {}: {e}", path.display()),
        }
    }
}

impl Host for NativeHost {
    fn audio_rate(&self) -> u32 {
        self.audio_rate
    }

    fn audio_start(&mut self, synth: scoreart_synth::Synth) -> Result<(), String> {
        self.audio.start(synth)
    }

    fn audio_stop(&mut self) {
        self.audio.stop();
    }

    fn audio_position(&mut self) -> Option<f64> {
        self.audio.position()
    }

    fn request_open(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("MusicXML", &["musicxml", "mxl", "xml"])
            .set_title("Open score")
            .pick_file()
        {
            self.load(p);
        }
    }

    fn take_opened(&mut self) -> Option<(String, Vec<u8>)> {
        self.opened.take()
    }

    fn save(&mut self, name: &str, bytes: Vec<u8>, save_as: bool) -> Option<String> {
        let same_kind = |p: &PathBuf| {
            p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase())
                == std::path::Path::new(name)
                    .extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase())
        };
        let target = match &self.path {
            Some(p) if !save_as && same_kind(p) => Some(p.clone()),
            _ => {
                let ext = std::path::Path::new(name)
                    .extension()
                    .map(|e| e.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let mut dlg = rfd::FileDialog::new().set_file_name(name).set_title("Save");
                if !ext.is_empty() {
                    dlg = dlg.add_filter(ext.to_uppercase(), &[ext.as_str()]);
                }
                dlg.save_file()
            }
        }?;
        match std::fs::write(&target, bytes) {
            Ok(()) => {
                let is_score = matches!(
                    scoreart_engine::Format::from_name(name),
                    Some(scoreart_engine::Format::MusicXml | scoreart_engine::Format::Mxl)
                );
                if is_score {
                    self.path = Some(target.clone());
                }
                Some(
                    target
                        .file_name()
                        .map(|f| f.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                )
            }
            Err(e) => {
                eprintln!("scoreart: cannot write {}: {e}", target.display());
                None
            }
        }
    }
}

struct App {
    ui: ScoreApp,
    title: String,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Files dropped on the window open as scores.
        let dropped = ctx.input_mut(|i| std::mem::take(&mut i.raw.dropped_files));
        for f in dropped {
            let name = f.path().to_string_lossy().into_owned();
            match f.bytes() {
                Ok(bytes) => self.ui.open_bytes(&name, &bytes),
                Err(e) => self.ui.session.status = e,
            }
        }
        self.ui.ui(ui);
        let title = self.ui.title();
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

fn main() -> eframe::Result<()> {
    let mut host = NativeHost::default();
    // Opening the device tells us its sample rate (44.1 kHz if there is none).
    host.audio_rate = host.audio.rate();
    if let Some(arg) = std::env::args().nth(1) {
        host.load(PathBuf::from(arg));
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1360.0, 880.0])
            .with_title("ScoreArt")
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "ScoreArt",
        options,
        Box::new(move |cc| {
            let ui = ScoreApp::new(&cc.egui_ctx, Session::default(), Box::new(host));
            Ok(Box::new(App {
                ui,
                title: String::new(),
            }))
        }),
    )
}
