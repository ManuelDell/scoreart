//! Sound libraries the user loads: SoundFont 2 files (played by `rustysynth`) and SFZ sample
//! libraries (a single `.sfz` file, a folder, or a zip archive), played by [`crate::sfz`].

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rustysynth::SoundFont;
use scoreart_model::PartSound;

use crate::Family;
use crate::files::{Dir, Files, Zip, dir_of};
use crate::sfz::SfzInstrument;

/// Largest SoundFont accepted.
const MAX_SF2: usize = 2 * 1024 * 1024 * 1024 - 1;

/// One preset a library offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresetInfo {
    /// Shown and stored in the score (`PartSound::preset`).
    pub name: String,
    /// SoundFont bank (128 = percussion); 0 for SFZ.
    pub bank: u16,
    /// SoundFont program; 0 for SFZ.
    pub program: u8,
}

enum Kind {
    Sf2(Arc<SoundFont>),
    Sfz {
        files: Box<dyn Files>,
        /// `.sfz` paths inside the library.
        instruments: Vec<String>,
        cache: Mutex<HashMap<String, Arc<SfzInstrument>>>,
    },
}

/// A loaded sound library.
pub struct SoundBank {
    pub name: String,
    kind: Kind,
}

impl std::fmt::Debug for SoundBank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SoundBank({})", self.name)
    }
}

/// What actually plays a part.
#[derive(Clone)]
pub enum Resolved {
    Builtin(Family),
    Sf2 {
        font: Arc<SoundFont>,
        bank: u16,
        program: u8,
    },
    Sfz(Arc<SfzInstrument>),
}

impl std::fmt::Debug for Resolved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Resolved::Builtin(fam) => write!(f, "Builtin({fam:?})"),
            Resolved::Sf2 { bank, program, .. } => write!(f, "Sf2({bank}:{program})"),
            Resolved::Sfz(i) => write!(f, "{i:?}"),
        }
    }
}

fn sf2_from_bytes(bytes: &[u8]) -> Result<SoundFont, String> {
    if bytes.len() > MAX_SF2 {
        return Err("SoundFont too large".into());
    }
    if !(bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"sfbk")) {
        return Err("not a SoundFont 2 file".into());
    }
    // The decoder is third-party code: keep a malformed file from taking the app down.
    let mut cursor = std::io::Cursor::new(bytes);
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| SoundFont::new(&mut cursor))) {
        Ok(Ok(sf)) if !sf.get_presets().is_empty() => Ok(sf),
        Ok(Ok(_)) => Err("the SoundFont has no presets".into()),
        Ok(Err(e)) => Err(format!("SoundFont: {e:?}")),
        Err(_) => Err("the SoundFont could not be read".into()),
    }
}

fn stem(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.rsplit_once('.').map_or(file, |(s, _)| s).to_string()
}

impl SoundBank {
    fn sfz(name: String, files: Box<dyn Files>, only: Option<String>) -> Result<SoundBank, String> {
        let mut instruments: Vec<String> = match only {
            Some(p) => vec![p],
            None => files
                .list()
                .into_iter()
                .filter(|p| p.to_lowercase().ends_with(".sfz"))
                .collect(),
        };
        instruments.sort();
        if instruments.is_empty() {
            return Err("no .sfz instruments found".into());
        }
        Ok(SoundBank {
            name,
            kind: Kind::Sfz {
                files,
                instruments,
                cache: Mutex::new(HashMap::new()),
            },
        })
    }

    /// A library from file contents: a SoundFont (`.sf2`) or a zip with SFZ instruments.
    pub fn from_bytes(name: &str, bytes: Vec<u8>) -> Result<SoundBank, String> {
        let name = stem(name);
        if bytes.starts_with(b"PK") {
            let zip = Zip::new(bytes)?;
            return SoundBank::sfz(name, Box::new(zip), None);
        }
        Ok(SoundBank {
            name,
            kind: Kind::Sf2(Arc::new(sf2_from_bytes(&bytes)?)),
        })
    }

    /// A library on disk: a `.sf2` or `.zip` file, one `.sfz` file, or a folder of them.
    pub fn from_path(path: &Path) -> Result<SoundBank, String> {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Sounds".into());
        if path.is_dir() {
            return SoundBank::sfz(name, Box::new(Dir::new(path)), None);
        }
        let lower = name.to_lowercase();
        if lower.ends_with(".sfz") {
            let (dir, file) = dir_of(path);
            return SoundBank::sfz(stem(&name), Box::new(dir), Some(file));
        }
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        SoundBank::from_bytes(&name, bytes)
    }

    pub fn is_soundfont(&self) -> bool {
        matches!(self.kind, Kind::Sf2(_))
    }

    /// Presets in a stable order (SoundFonts by bank and program, SFZ by path).
    pub fn presets(&self) -> Vec<PresetInfo> {
        match &self.kind {
            Kind::Sf2(sf) => {
                let mut v: Vec<PresetInfo> = sf
                    .get_presets()
                    .iter()
                    .map(|p| PresetInfo {
                        name: p.get_name().trim().to_string(),
                        bank: p.get_bank_number().clamp(0, 128) as u16,
                        program: p.get_patch_number().clamp(0, 127) as u8,
                    })
                    .collect();
                v.sort_by_key(|p| (p.bank, p.program));
                v
            }
            Kind::Sfz { instruments, .. } => instruments
                .iter()
                .map(|p| PresetInfo {
                    name: p.trim_end_matches(".sfz").trim_end_matches(".SFZ").to_string(),
                    bank: 0,
                    program: 0,
                })
                .collect(),
        }
    }

    /// Loads (once) the SFZ instrument for a preset name.
    fn sfz_instrument(&self, preset: &str) -> Result<Arc<SfzInstrument>, String> {
        let Kind::Sfz {
            files,
            instruments,
            cache,
        } = &self.kind
        else {
            return Err("not an SFZ library".into());
        };
        let path = instruments
            .iter()
            .find(|p| p.trim_end_matches(".sfz").trim_end_matches(".SFZ") == preset)
            .or_else(|| instruments.first())
            .ok_or("empty library")?;
        if let Ok(c) = cache.lock()
            && let Some(i) = c.get(path)
        {
            return Ok(i.clone());
        }
        let inst = Arc::new(SfzInstrument::load(files.as_ref(), path)?);
        if let Ok(mut c) = cache.lock() {
            c.insert(path.clone(), inst.clone());
        }
        Ok(inst)
    }

    fn resolve_in(&self, preset: &str, program: u8) -> Result<Resolved, String> {
        match &self.kind {
            Kind::Sf2(sf) => {
                let presets = self.presets();
                let p = presets
                    .iter()
                    .find(|p| p.name == preset)
                    .or_else(|| presets.iter().find(|p| p.bank == 0 && p.program == program))
                    .or_else(|| presets.first())
                    .ok_or("empty SoundFont")?;
                Ok(Resolved::Sf2 {
                    font: sf.clone(),
                    bank: p.bank,
                    program: p.program,
                })
            }
            Kind::Sfz { .. } => Ok(Resolved::Sfz(self.sfz_instrument(preset)?)),
        }
    }
}

/// The sound for a part. Automatic (no library chosen): the first loaded SoundFont that has
/// the part's General MIDI program, else the built-in synthesizer. A library that is not
/// loaded falls back the same way; the error says why.
pub fn resolve(banks: &[Arc<SoundBank>], sound: &PartSound, program: u8) -> (Resolved, Option<String>) {
    let builtin = Resolved::Builtin(Family::of_program(program));
    if sound.library == PartSound::BUILT_IN {
        return (builtin, None);
    }
    let auto = || {
        banks
            .iter()
            .filter(|b| b.is_soundfont())
            .find(|b| b.presets().iter().any(|p| p.bank == 0 && p.program == program))
            .and_then(|b| b.resolve_in("", program).ok())
    };
    if sound.library.is_empty() {
        return (auto().unwrap_or(builtin), None);
    }
    match banks.iter().find(|b| b.name == sound.library) {
        Some(b) => match b.resolve_in(&sound.preset, program) {
            Ok(r) => (r, None),
            Err(e) => (auto().unwrap_or(builtin), Some(format!("{}: {e}", sound.library))),
        },
        None => (
            auto().unwrap_or(builtin),
            Some(format!("sound library \"{}\" is not loaded", sound.library)),
        ),
    }
}
