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
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};

use scoreart_engine::Session;
use scoreart_synth::SoundBank;
use scoreart_synth::catalog::{self, Library};
use scoreart_ui_egui::{DownloadStatus, Host, ScoreApp};

type Loaded = Result<(PathBuf, SoundBank), String>;

#[derive(Default)]
struct NativeHost {
    opened: Option<(String, Vec<u8>)>,
    /// Where the open score lives on disk.
    path: Option<PathBuf>,
    audio: audio::Audio,
    audio_rate: u32,
    /// Sound libraries being loaded in the background.
    loading: Vec<Receiver<Loaded>>,
    /// Loaded libraries: name and where they came from (remembered for the next start).
    sound_paths: Vec<(String, PathBuf)>,
    /// Sound files dropped on the window, picked up on the next tick.
    dropped_sounds: Arc<Mutex<Vec<PathBuf>>>,
    /// The catalogue download in progress.
    download: Option<Download>,
    /// The open MIDI input device and the messages it sent.
    midi: Option<(String, midir::MidiInputConnection<()>)>,
    midi_queue: Arc<Mutex<Vec<Vec<u8>>>>,
}

struct Download {
    status: Arc<Mutex<DownloadStatus>>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    done: Arc<std::sync::atomic::AtomicBool>,
}

/// The file that lists the sound libraries to load at start.
fn sounds_list() -> Option<PathBuf> {
    Some(config_dir()?.join("sounds.txt"))
}

/// A setting's file (`name` is a plain word like `midi-map`).
fn setting_file(name: &str) -> Option<PathBuf> {
    let safe: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if safe.is_empty() {
        return None;
    }
    Some(config_dir()?.join(format!("{safe}.txt")))
}

/// ScoreArt's settings folder.
fn config_dir() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    }?;
    Some(base.join("ScoreArt"))
}

/// Whether a dropped file is a sound library rather than a score.
fn is_sound_file(p: &std::path::Path) -> bool {
    let ext = p
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    matches!(ext.as_str(), "sf2" | "sfz" | "zip")
}

impl NativeHost {
    /// Loads a sound library on a background thread.
    fn load_sounds(&mut self, path: PathBuf) {
        let (tx, rx) = channel();
        self.loading.push(rx);
        std::thread::spawn(move || {
            let r = std::panic::catch_unwind(|| SoundBank::from_path(&path))
                .unwrap_or_else(|_| Err("the library could not be read".into()))
                .map(|b| (path.clone(), b))
                .map_err(|e| format!("{}: {e}", path.display()));
            let _ = tx.send(r);
        });
    }

    fn save_sound_list(&self) {
        let Some(file) = sounds_list() else { return };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text: String = self
            .sound_paths
            .iter()
            .map(|(_, p)| format!("{}\n", p.display()))
            .collect();
        if let Err(e) = std::fs::write(&file, text) {
            eprintln!("scoreart: cannot write {}: {e}", file.display());
        }
    }

    /// Loads the libraries remembered from last time.
    fn load_remembered_sounds(&mut self) {
        let Some(text) = sounds_list().and_then(|f| std::fs::read_to_string(f).ok()) else {
            return;
        };
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let p = PathBuf::from(line);
            if p.exists() {
                self.load_sounds(p);
            }
        }
    }

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

/// The local time zone's offset from UTC in seconds, asked once from the system's `date`
/// command (`+0100` → 3600); zero (UTC) where it cannot be found, e.g. on Windows.
fn local_offset_secs() -> i64 {
    static OFFSET: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *OFFSET.get_or_init(|| {
        let out = std::process::Command::new("date").arg("+%z").output().ok();
        out.and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| parse_offset(s.trim()))
            .unwrap_or(0)
    })
}

/// `+0130` / `-0500` → seconds east of UTC.
fn parse_offset(s: &str) -> Option<i64> {
    let (sign, digits) = match s.as_bytes().first()? {
        b'+' => (1, s.get(1..)?),
        b'-' => (-1, s.get(1..)?),
        _ => return None,
    };
    if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let h: i64 = digits.get(..2)?.parse().ok()?;
    let m: i64 = digits.get(2..)?.parse().ok()?;
    (h <= 14 && m < 60).then_some(sign * (h * 3600 + m * 60))
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
            .add_filter(
                "Scores (MusicXML, MuseScore, MIDI)",
                &["musicxml", "mxl", "xml", "mscz", "mscx", "mid", "midi"],
            )
            .set_title("Open score")
            .pick_file()
        {
            self.load(p);
        }
    }

    fn take_opened(&mut self) -> Option<(String, Vec<u8>)> {
        self.opened.take()
    }

    fn now_secs(&self) -> Option<u64> {
        let utc = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs();
        Some(utc.saturating_add_signed(local_offset_secs()))
    }

    fn file_path(&self) -> Option<String> {
        self.path.as_ref().map(|p| p.display().to_string())
    }

    fn tick(&mut self) {
        let dropped: Vec<PathBuf> = match self.dropped_sounds.lock() {
            Ok(mut q) => std::mem::take(&mut *q),
            Err(_) => Vec::new(),
        };
        for p in dropped {
            self.load_sounds(p);
        }
    }

    fn request_sounds(&mut self, folder: bool) {
        let dlg = rfd::FileDialog::new().set_title("Load sounds");
        let picked = if folder {
            dlg.pick_folder()
        } else {
            dlg.add_filter("Sound libraries (SoundFont, SFZ, zip)", &["sf2", "sfz", "zip"])
                .pick_file()
        };
        if let Some(p) = picked {
            self.load_sounds(p);
        }
    }

    fn can_load_folders(&self) -> bool {
        true
    }

    fn take_sounds(&mut self) -> Option<Result<SoundBank, String>> {
        let mut found = None;
        self.loading.retain(|rx| {
            if found.is_some() {
                return true;
            }
            match rx.try_recv() {
                Ok(r) => {
                    found = Some(r);
                    false
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => true,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
            }
        });
        match found? {
            Ok((path, bank)) => {
                self.sound_paths.retain(|(n, p)| *p != path && *n != bank.name);
                self.sound_paths.push((bank.name.clone(), path));
                self.save_sound_list();
                Some(Ok(bank))
            }
            Err(e) => Some(Err(e)),
        }
    }

    fn can_download(&self) -> bool {
        catalog::default_dir().is_some()
    }

    fn download_sounds(&mut self, library: &'static Library, instrument: &str) {
        let Some(root) = catalog::default_dir() else {
            return;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let accepted = scoreart_ui_egui::acceptance_text(library, now);
        let status = Arc::new(Mutex::new(DownloadStatus {
            library: library.name.to_string(),
            instrument: instrument.to_string(),
            progress: Default::default(),
        }));
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (tx, rx) = channel();
        self.loading.push(rx);
        let instrument = instrument.to_string();
        let (st, c, d) = (status.clone(), cancel.clone(), done.clone());
        std::thread::spawn(move || {
            let result = catalog::install(library, &instrument, &root, &accepted, &c, &mut |p| {
                if let Ok(mut s) = st.lock() {
                    s.progress = p.clone();
                }
            })
            .and_then(|dir| SoundBank::from_path(&dir).map(|b| (dir, b)))
            .map_err(|e| format!("{}: {e}", library.name));
            d.store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = tx.send(result);
        });
        self.download = Some(Download { status, cancel, done });
    }

    fn download_status(&self) -> Option<DownloadStatus> {
        let d = self.download.as_ref()?;
        if d.done.load(std::sync::atomic::Ordering::Relaxed) {
            return None;
        }
        d.status.lock().ok().map(|s| s.clone())
    }

    fn cancel_download(&mut self) {
        if let Some(d) = &self.download {
            d.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    fn midi_ports(&mut self) -> Vec<String> {
        let Ok(input) = midir::MidiInput::new("ScoreArt") else {
            return Vec::new();
        };
        input
            .ports()
            .iter()
            .filter_map(|p| input.port_name(p).ok())
            .collect()
    }

    fn midi_open(&mut self, port: Option<&str>) -> Result<(), String> {
        // Closing the old connection first frees the device.
        self.midi = None;
        let Some(want) = port else { return Ok(()) };
        let mut input = midir::MidiInput::new("ScoreArt").map_err(|e| e.to_string())?;
        input.ignore(midir::Ignore::All);
        let ports = input.ports();
        let p = ports
            .iter()
            .find(|p| input.port_name(p).is_ok_and(|n| n == want))
            .ok_or_else(|| format!("MIDI device {want} not found"))?;
        let queue = self.midi_queue.clone();
        let conn = input
            .connect(
                p,
                "ScoreArt input",
                move |_, msg, _| {
                    if let Ok(mut q) = queue.lock()
                        && q.len() < 4096
                    {
                        q.push(msg.to_vec());
                    }
                },
                (),
            )
            .map_err(|e| e.to_string())?;
        self.midi = Some((want.to_string(), conn));
        Ok(())
    }

    fn midi_connected(&self) -> Option<String> {
        self.midi.as_ref().map(|(n, _)| n.clone())
    }

    fn take_midi(&mut self) -> Vec<Vec<u8>> {
        match self.midi_queue.lock() {
            Ok(mut q) => std::mem::take(&mut *q),
            Err(_) => Vec::new(),
        }
    }

    fn locale(&self) -> Option<String> {
        ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"]
            .iter()
            .filter_map(std::env::var_os)
            .map(|v| v.to_string_lossy().into_owned())
            .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
    }

    fn load_setting(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(setting_file(name)?).ok()
    }

    fn save_setting(&mut self, name: &str, value: &str) {
        let Some(file) = setting_file(name) else { return };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&file, value) {
            eprintln!("scoreart: cannot write {}: {e}", file.display());
        }
    }

    fn forget_sounds(&mut self, name: &str) {
        self.sound_paths.retain(|(n, _)| n != name);
        self.save_sound_list();
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
    dropped_sounds: Arc<Mutex<Vec<PathBuf>>>,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Files dropped on the window open as scores.
        let dropped = ctx.input_mut(|i| std::mem::take(&mut i.raw.dropped_files));
        for f in dropped {
            // Sound libraries dropped on the window are loaded as sounds.
            if is_sound_file(f.path()) {
                if let Ok(mut q) = self.dropped_sounds.lock() {
                    q.push(f.path().to_path_buf());
                }
                continue;
            }
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
    host.load_remembered_sounds();
    let dropped_sounds = host.dropped_sounds.clone();
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
                dropped_sounds,
            }))
        }),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn time_zone_offsets() {
        assert_eq!(super::parse_offset("+0100"), Some(3600));
        assert_eq!(super::parse_offset("-0530"), Some(-19800));
        assert_eq!(super::parse_offset("+0000"), Some(0));
        for bad in ["", "0100", "+01", "+01:00", "+9900", "CET"] {
            assert_eq!(super::parse_offset(bad), None, "{bad}");
        }
    }
}
