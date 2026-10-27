//! The catalogue of free sample libraries ScoreArt can fetch on request, with the licence facts
//! shown to the user before anything is downloaded.
//!
//! Nothing here is bundled: files are fetched only after the user has read and accepted the
//! licence, directly from the publisher's own public repository, one instrument at a time (the
//! `.sfz` file, its includes and its samples). Next to the files ScoreArt stores the library's
//! own licence file and a notice recording what was accepted and when.

use crate::catalog_index;
use crate::files::{Files, clean};
use crate::sfz::{References, references};

/// Name of the notice ScoreArt writes next to downloaded files.
pub const NOTICE_FILE: &str = "SCOREART-LICENCE-NOTICE.txt";

/// What a licence lets you do and asks of you, in plain words. The licence text is binding.
#[derive(Debug)]
pub struct Licence {
    pub name: &'static str,
    /// The full licence text.
    pub url: &'static str,
    pub allows: &'static [&'static str],
    pub requires: &'static [&'static str],
    pub notes: &'static [&'static str],
    /// The library's own licence or readme file (path in the repository), saved with the files.
    pub file: Option<&'static str>,
}

/// Where a library comes from.
#[derive(Debug)]
pub enum Source {
    /// Files fetched one by one from a public Git repository (`base` ends with `/`).
    Repository { base: &'static str, page: &'static str },
    /// The publisher asks people to download from their website: ScoreArt only opens it.
    Website { url: &'static str },
}

#[derive(Debug)]
pub struct Library {
    pub id: &'static str,
    /// Also the name of the library in the mixer and of its folder on disk.
    pub name: &'static str,
    pub publisher: &'static str,
    pub description: &'static str,
    pub size: &'static str,
    pub licence: Licence,
    pub source: Source,
    /// `.sfz` paths in the repository.
    pub instruments: &'static [&'static str],
}

const NOT_ADVICE: &str = "This is a summary, not legal advice. Only the licence text is binding.";

static LIBRARIES: &[Library] = &[
    Library {
        id: "vsco2ce",
        name: "VSCO 2 CE",
        publisher: "Versilian Studios (Sam Gossner, Simon Dalzell; sample editing Elan Hickler)",
        description: "Chamber orchestra: strings (solo and section), woodwinds, brass, percussion, keys and organ. SFZ instruments with WAV samples.",
        size: "about 3 GB in total; one instrument is typically 5–150 MB",
        licence: Licence {
            name: "Creative Commons Zero 1.0 (CC0, public domain dedication)",
            url: "https://creativecommons.org/publicdomain/zero/1.0/legalcode",
            allows: &[
                "Use the samples for any purpose, including commercial music and software.",
                "Copy, change and share them, with or without credit.",
            ],
            requires: &[
                "Nothing. Crediting \"VSCO 2 Community Edition by Versilian Studios\" is a courtesy, not a duty.",
            ],
            notes: &[
                "CC0 gives no warranty, and it does not cover trademarks: \"Versilian\" and \"VSCO\" stay the publisher's names.",
                NOT_ADVICE,
            ],
            file: Some("LICENSE"),
        },
        source: Source::Repository {
            base: "https://raw.githubusercontent.com/sgossner/VSCO-2-CE/SFZ/",
            page: "https://github.com/sgossner/VSCO-2-CE/tree/SFZ",
        },
        instruments: catalog_index::VSCO_2_CE,
    },
    Library {
        id: "vcsl",
        name: "VCSL",
        publisher: "Versilian Studios and contributors (Versilian Community Sample Library)",
        description: "A broad collection beyond the orchestra: recorders, flutes, reeds, brass, strings, keyboards, mallets, percussion, synths. SFZ instruments with WAV samples.",
        size: "several GB in total; one instrument is typically 20–75 MB",
        licence: Licence {
            name: "Creative Commons Zero 1.0 (CC0, public domain dedication)",
            url: "https://creativecommons.org/publicdomain/zero/1.0/legalcode",
            allows: &[
                "Use the samples for any purpose, including commercial music and software (\"no royalties, no credit, no special terms\").",
                "Copy, change and share them.",
            ],
            requires: &["Nothing. Credit is welcome, not required."],
            notes: &[
                "Individual contributors dedicated their samples under CC0; the library gives no warranty.",
                NOT_ADVICE,
            ],
            file: Some("README.md"),
        },
        source: Source::Repository {
            base: "https://raw.githubusercontent.com/sgossner/VCSL/sfz/",
            page: "https://github.com/sgossner/VCSL/tree/sfz",
        },
        instruments: catalog_index::VCSL,
    },
    Library {
        id: "salamander",
        name: "Salamander Grand Piano",
        publisher: "Alexander Holm (recording); SFZ mapping by kinwie (sfzinstruments)",
        description: "Yamaha C5 concert grand: 16 velocity layers, string resonance, hammer and pedal noise. SFZ with FLAC samples.",
        size: "about 360 MB",
        licence: Licence {
            name: "Creative Commons Attribution 3.0 Unported (CC BY 3.0)",
            url: "https://creativecommons.org/licenses/by/3.0/legalcode",
            allows: &[
                "Use the piano for any purpose, including commercial music.",
                "Copy, change and share the samples, also commercially.",
            ],
            requires: &[
                "Attribution: when you share the samples or anything built from them (e.g. a sample pack or software), credit \"Salamander Grand Piano by Alexander Holm\", link the licence and say what you changed.",
                "Do not suggest that the author endorses you or your use.",
                "For music you publish, crediting the piano in the release notes is the safe way to meet the attribution clause.",
            ],
            notes: &[
                "The licence gives no warranty. Keep the licence file that ScoreArt saves with the samples.",
                NOT_ADVICE,
            ],
            file: Some("LICENSE"),
        },
        source: Source::Repository {
            base: "https://raw.githubusercontent.com/sfzinstruments/SalamanderGrandPiano/master/",
            page: "https://github.com/sfzinstruments/SalamanderGrandPiano",
        },
        instruments: &["Salamander Grand Piano V3.sfz"],
    },
    Library {
        id: "generaluser",
        name: "GeneralUser GS",
        publisher: "S. Christian Collins",
        description: "All 128 General MIDI instruments and drum kits in one 30 MB SoundFont: the best small all-rounder.",
        size: "about 32 MB",
        licence: Licence {
            name: "GeneralUser GS License v2.0",
            url: "https://github.com/mrbumpy409/GeneralUser-GS/blob/main/documentation/LICENSE.txt",
            allows: &[
                "Use it without restriction for your own music, private or commercial.",
                "Use it in software projects, and change the SoundFont or its packaging.",
            ],
            requires: &[
                "The author asks that nobody links directly to his download files: link to his website or keep your own copy.",
            ],
            notes: &[
                "Some samples come from other free banks; the author cannot fully trace their origin (he knows of no complaint since 2000).",
                "Because of the author's request, ScoreArt does not download it: it opens his website. Download the .sf2 there and load it with \"Load sounds…\".",
                NOT_ADVICE,
            ],
            file: None,
        },
        source: Source::Website {
            url: "https://www.schristiancollins.com/generaluser.php",
        },
        instruments: &[],
    },
];

pub fn libraries() -> &'static [Library] {
    LIBRARIES
}

pub fn find(id: &str) -> Option<&'static Library> {
    LIBRARIES.iter().find(|l| l.id == id || l.name == id)
}

/// Shown name of an instrument path (`Strings/Violin Sus.sfz` → `Violin Sus`).
pub fn instrument_name(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.strip_suffix(".sfz")
        .or_else(|| file.strip_suffix(".SFZ"))
        .unwrap_or(file)
}

/// The URL of a file in a repository source; each path segment is percent-encoded.
pub fn file_url(base: &str, path: &str) -> String {
    let mut out = base.to_string();
    for (i, seg) in clean(path).split('/').enumerate() {
        if i > 0 {
            out.push('/');
        }
        for b in seg.bytes() {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                out.push(char::from(b));
            } else {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}

/// The full licence statement shown before a download and saved next to the files.
pub fn licence_text(lib: &Library) -> String {
    let l = &lib.licence;
    let mut s = format!(
        "{}\nPublisher: {}\nLicence: {}\nLicence text: {}\n",
        lib.name, lib.publisher, l.name, l.url
    );
    let page = match lib.source {
        Source::Repository { page, .. } => page,
        Source::Website { url } => url,
    };
    s.push_str(&format!("Source: {page}\n\nThe licence allows you to:\n"));
    for a in l.allows {
        s.push_str(&format!("  - {a}\n"));
    }
    s.push_str("\nThe licence requires you to:\n");
    for r in l.requires {
        s.push_str(&format!("  - {r}\n"));
    }
    s.push_str("\nPlease note:\n");
    for n in l.notes {
        s.push_str(&format!("  - {n}\n"));
    }
    s.push_str(
        "  - ScoreArt does not host, sell or change these files. They come directly from the source above; ScoreArt's own licence does not cover them.\n",
    );
    s
}

/// The notice written next to downloaded files: the licence once, then one line per accepted
/// download (`previous` is the notice already there, if any).
pub fn notice(lib: &Library, instrument: &str, accepted: &str, previous: Option<&str>) -> String {
    let line = format!("- {instrument}: licence accepted by the user — {accepted}\n");
    match previous {
        Some(p) if p.contains("Downloads:") => format!("{p}{line}"),
        _ => format!(
            "This folder was filled by ScoreArt on request of its user, from the source below.\n\n{}\nDownloads:\n{line}",
            licence_text(lib)
        ),
    }
}

/// `secs` since 1970 as a UTC date and time, e.g. `2026-10-27 12:05 UTC`.
pub fn utc_text(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Days → civil date (proleptic Gregorian calendar).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        rem / 3600,
        rem % 3600 / 60
    )
}

/// What still has to be fetched for an instrument, given the files already there: the `.sfz`
/// file itself first, then missing includes, then samples (in that order, as each step can
/// reveal the next).
pub fn next_files(lib: &Library, have: &dyn Files, instrument: &str) -> Result<Vec<String>, String> {
    let sfz = clean(instrument);
    if have.read(&sfz).is_none() {
        return Ok(vec![sfz]);
    }
    let References {
        missing_includes,
        samples,
    } = references(have, &sfz)?;
    if !missing_includes.is_empty() {
        return Ok(missing_includes);
    }
    let mut todo: Vec<String> = samples.into_iter().filter(|s| !exists(have, s)).collect();
    if let Some(f) = lib.licence.file
        && !exists(have, f)
    {
        todo.push(f.to_string());
    }
    Ok(todo)
}

fn exists(have: &dyn Files, path: &str) -> bool {
    have.list().iter().any(|p| p.eq_ignore_ascii_case(&clean(path)))
}

/// Where the desktop app and the command line keep downloaded libraries.
pub fn default_dir() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    }?;
    Some(base.join("ScoreArt").join("Sounds"))
}

/// Progress of a download.
#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub file: String,
    pub files_done: usize,
    pub files_total: usize,
    pub bytes: u64,
}

#[cfg(feature = "download")]
mod download {
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::{Library, NOTICE_FILE, Progress, Source, file_url, next_files, notice};
    use crate::files::Dir;

    /// Largest single file fetched.
    const MAX_FILE: u64 = 1024 * 1024 * 1024;

    fn agent() -> ureq::Agent {
        ureq::AgentBuilder::new()
            .try_proxy_from_env(true)
            .timeout_connect(std::time::Duration::from_secs(20))
            .timeout_read(std::time::Duration::from_secs(60))
            .user_agent(concat!("ScoreArt/", env!("CARGO_PKG_VERSION")))
            .build()
    }

    fn fetch(
        agent: &ureq::Agent,
        url: &str,
        to: &Path,
        bytes: &mut u64,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        let resp = agent.get(url).call().map_err(|e| format!("{url}: {e}"))?;
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let part = to.with_extension("part");
        let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
        let mut reader = resp.into_reader().take(MAX_FILE);
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            if cancel.load(Ordering::Relaxed) {
                drop(out);
                let _ = std::fs::remove_file(&part);
                return Err("cancelled".into());
            }
            let n = reader.read(&mut buf).map_err(|e| format!("{url}: {e}"))?;
            if n == 0 {
                break;
            }
            out.write_all(buf.get(..n).unwrap_or(&[]))
                .map_err(|e| e.to_string())?;
            *bytes += n as u64;
        }
        drop(out);
        std::fs::rename(&part, to).map_err(|e| e.to_string())
    }

    /// Downloads one instrument of `lib` into `root/<library name>/` (files already there are
    /// kept) and writes the licence notice. `accepted` records the user's acceptance (date and
    /// licence name) and must not be empty. Returns the library folder.
    pub fn install(
        lib: &Library,
        instrument: &str,
        root: &Path,
        accepted: &str,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(&Progress),
    ) -> Result<PathBuf, String> {
        if accepted.trim().is_empty() {
            return Err("the licence has to be accepted first".into());
        }
        let Source::Repository { base, .. } = lib.source else {
            return Err(format!(
                "{} is downloaded from its website, not by ScoreArt",
                lib.name
            ));
        };
        if !lib.instruments.contains(&instrument) {
            return Err(format!("{instrument} is not part of {}", lib.name));
        }
        let dir = root.join(lib.name);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let previous = std::fs::read_to_string(dir.join(NOTICE_FILE)).ok();
        std::fs::write(
            dir.join(NOTICE_FILE),
            notice(lib, instrument, accepted, previous.as_deref()),
        )
        .map_err(|e| e.to_string())?;
        let files = Dir::new(&dir);
        let agent = agent();
        let mut p = Progress::default();
        // Each round fetches what the files so far reveal: the .sfz, its includes, the samples.
        for _round in 0..32 {
            let todo = next_files(lib, &files, instrument)?;
            if todo.is_empty() {
                return Ok(dir);
            }
            p.files_total = p.files_done + todo.len();
            for f in todo {
                p.file = f.clone();
                progress(&p);
                fetch(&agent, &file_url(base, &f), &dir.join(&f), &mut p.bytes, cancel)?;
                p.files_done += 1;
                progress(&p);
            }
        }
        Err("the instrument refers to too many nested files".into())
    }
}

#[cfg(feature = "download")]
pub use download::install;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::Mem;

    #[test]
    fn every_library_states_its_licence() {
        for l in libraries() {
            assert!(!l.licence.name.is_empty() && l.licence.url.starts_with("https://"));
            assert!(!l.licence.allows.is_empty() && !l.licence.requires.is_empty());
            assert!(l.licence.notes.contains(&NOT_ADVICE), "{}", l.id);
            let text = licence_text(l);
            assert!(text.contains(l.licence.name) && text.contains("ScoreArt does not host"));
            match l.source {
                Source::Repository { base, page } => {
                    assert!(base.starts_with("https://raw.githubusercontent.com/") && base.ends_with('/'));
                    assert!(page.starts_with("https://"));
                    assert!(!l.instruments.is_empty());
                }
                Source::Website { url } => {
                    assert!(url.starts_with("https://"));
                    assert!(l.instruments.is_empty());
                }
            }
        }
        assert!(find("vsco2ce").is_some() && find("Salamander Grand Piano").is_some());
    }

    #[test]
    fn notice_records_every_acceptance() {
        let lib = find("vsco2ce").unwrap();
        let first = notice(lib, "A.sfz", "CC0 — 2026-10-27 12:00 UTC", None);
        let second = notice(lib, "B.sfz", "CC0 — 2026-10-28 09:30 UTC", Some(&first));
        assert!(second.starts_with(&first) && second.contains("- B.sfz: licence accepted"));
        assert_eq!(second.matches("Licence text:").count(), 1);
        assert_eq!(utc_text(0), "1970-01-01 00:00 UTC");
        assert_eq!(utc_text(1_793_104_766), "2026-10-27 12:39 UTC");
        assert_eq!(utc_text(951_782_400), "2000-02-29 00:00 UTC");
    }

    #[test]
    fn urls_are_encoded() {
        assert_eq!(
            file_url("https://x/", "Strings/Solo Violin/A#3 f.wav"),
            "https://x/Strings/Solo%20Violin/A%233%20f.wav"
        );
        assert_eq!(instrument_name("A/B c.sfz"), "B c");
    }

    #[test]
    fn plans_an_instrument_step_by_step() {
        let lib = find("salamander").unwrap();
        let have = Mem::default();
        let inst = lib.instruments[0];
        assert_eq!(next_files(lib, &have, inst).unwrap(), [inst]);
        have.insert(inst, b"#include \"Data/x.txt\"\n<region> sample=A0.flac".to_vec());
        assert_eq!(next_files(lib, &have, inst).unwrap(), ["Data/x.txt"]);
        have.insert("Data/x.txt", b"<region> sample=C1.flac".to_vec());
        assert_eq!(
            next_files(lib, &have, inst).unwrap(),
            ["C1.flac", "A0.flac", "LICENSE"]
        );
        for f in ["C1.flac", "A0.flac", "LICENSE"] {
            have.insert(f, vec![1]);
        }
        assert!(next_files(lib, &have, inst).unwrap().is_empty());
    }
}
