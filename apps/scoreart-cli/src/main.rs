//! ScoreArt command line.
//!
//! ```text
//! scoreart-cli convert <in.musicxml|in.mxl|sample> <out.musicxml|.mxl|.mid|.svg>
//! scoreart-cli commands [--json]
//! scoreart-cli run [--in FILE | --sample | --new] [--cmd 'id' | --cmd 'id={json}']… [--save OUT]
//! scoreart-cli info <file>
//! scoreart-cli sounds list [LIBRARY]
//! scoreart-cli sounds licence LIBRARY
//! scoreart-cli sounds install LIBRARY INSTRUMENT --accept-licence [--dir DIR]
//! ```
//!
//! Every form takes `--sounds PATH` (repeatable): a SoundFont (`.sf2`), an SFZ instrument, a
//! folder of SFZ instruments or a zip of them, used for `.wav` and stems (`.zip`) output.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::io::Write as _;
use std::process::ExitCode;

/// `println!` that never panics (a closed pipe, e.g. `| head`, just stops output).
macro_rules! out {
    ($($t:tt)*) => {{
        let _ = writeln!(std::io::stdout(), $($t)*);
    }};
}

use std::sync::{Arc, OnceLock};

use scoreart_engine::{Format, Session};
use scoreart_synth::SoundBank;
use serde_json::{Value, json};

/// Sound libraries given with `--sounds`.
static SOUNDS: OnceLock<Vec<Arc<SoundBank>>> = OnceLock::new();

fn new_session() -> Session {
    let mut s = Session::default();
    s.sounds = SOUNDS.get().cloned().unwrap_or_default();
    s
}

fn load(input: &str) -> Result<Session, String> {
    if input == "sample" {
        return Ok(new_session());
    }
    let bytes = std::fs::read(input).map_err(|e| format!("{input}: {e}"))?;
    let mut s = new_session();
    s.open_bytes(input, &bytes).map_err(|e| format!("{input}: {e}"))?;
    Ok(s)
}

/// Takes every `--sounds PATH` out of the arguments and loads the libraries.
fn take_sounds(args: &mut Vec<String>) -> Result<(), String> {
    let mut banks = Vec::new();
    while let Some(i) = args.iter().position(|a| a == "--sounds") {
        args.remove(i);
        if i >= args.len() {
            return Err("--sounds needs a path".into());
        }
        let path = args.remove(i);
        let bank = SoundBank::from_path(std::path::Path::new(&path)).map_err(|e| format!("{path}: {e}"))?;
        eprintln!("sounds: {} ({} presets)", bank.name, bank.presets().len());
        banks.push(Arc::new(bank));
    }
    let _ = SOUNDS.set(banks);
    Ok(())
}

fn write_out(s: &Session, out: &str) -> Result<Vec<String>, String> {
    if Format::from_name(out) == Some(Format::Svg) {
        // One file per page: out.svg, or out-1.svg, out-2.svg … for longer scores.
        let layout = scoreart_engrave::layout(&s.score, &s.style());
        let pages = scoreart_render::svg_pages(&layout, &s.style());
        let stem = out.strip_suffix(".svg").unwrap_or(out);
        let mut written = Vec::new();
        for (i, svg) in pages.iter().enumerate() {
            let name = if pages.len() == 1 {
                out.to_string()
            } else {
                format!("{stem}-{}.svg", i + 1)
            };
            std::fs::write(&name, svg).map_err(|e| format!("{name}: {e}"))?;
            written.push(name);
        }
        return Ok(written);
    }
    let bytes = s.save_bytes(out).map_err(|e| e.to_string())?;
    std::fs::write(out, bytes).map_err(|e| format!("{out}: {e}"))?;
    Ok(vec![out.to_string()])
}

fn parse_cmd(spec: &str) -> Result<(String, Value), String> {
    match spec.split_once('=') {
        Some((id, params)) => {
            let v: Value = serde_json::from_str(params).map_err(|e| format!("{id}: bad JSON params: {e}"))?;
            Ok((id.trim().to_string(), v))
        }
        None => Ok((spec.trim().to_string(), json!({}))),
    }
}

/// The free-library catalogue: list, show licences, install (only with explicit acceptance).
fn sounds(args: &[String]) -> Result<(), String> {
    use scoreart_synth::catalog;
    match args.first().map(String::as_str) {
        Some("list") => {
            match args.get(1) {
                None => {
                    for l in catalog::libraries() {
                        out!("{:<12} {:<24} {} · {}", l.id, l.name, l.licence.name, l.size);
                    }
                    out!("\nInstruments: sounds list LIBRARY · licence: sounds licence LIBRARY");
                }
                Some(id) => {
                    let l = catalog::find(id).ok_or_else(|| format!("unknown library {id}"))?;
                    for i in l.instruments {
                        out!("{i}");
                    }
                }
            }
            Ok(())
        }
        Some("licence" | "license") => {
            let id = args.get(1).ok_or("sounds licence LIBRARY")?;
            let l = catalog::find(id).ok_or_else(|| format!("unknown library {id}"))?;
            out!("{}", catalog::licence_text(l));
            Ok(())
        }
        Some("install") => {
            let (Some(id), Some(inst)) = (args.get(1), args.get(2)) else {
                return Err("sounds install LIBRARY INSTRUMENT --accept-licence [--dir DIR]".into());
            };
            let l = catalog::find(id).ok_or_else(|| format!("unknown library {id}"))?;
            let inst = l
                .instruments
                .iter()
                .find(|i| *i == inst || catalog::instrument_name(i) == inst)
                .ok_or_else(|| format!("{inst} is not an instrument of {}", l.name))?;
            // The licence is always shown; installing needs the explicit flag.
            eprintln!("{}", catalog::licence_text(l));
            if !args.iter().any(|a| a == "--accept-licence" || a == "--accept-license") {
                return Err("read the licence above; add --accept-licence to accept it and download".into());
            }
            let dir = match args.iter().position(|a| a == "--dir").and_then(|i| args.get(i + 1)) {
                Some(d) => std::path::PathBuf::from(d),
                None => catalog::default_dir().ok_or("no data folder; use --dir")?,
            };
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let accepted = format!(
                "{}, accepted on the command line (--accept-licence) on {}",
                l.licence.name,
                catalog::utc_text(now)
            );
            let cancel = std::sync::atomic::AtomicBool::new(false);
            let folder = catalog::install(l, inst, &dir, &accepted, &cancel, &mut |p| {
                eprint!(
                    "\r{}/{} files, {:.1} MB  {:<60.60}",
                    p.files_done,
                    p.files_total,
                    p.bytes as f64 / 1e6,
                    p.file
                );
            })?;
            eprintln!();
            out!("installed {} into {}", catalog::instrument_name(inst), folder.display());
            out!("use it with: --sounds \"{}\"", folder.display());
            Ok(())
        }
        _ => Err("sounds list [LIBRARY] | sounds licence LIBRARY | sounds install LIBRARY INSTRUMENT --accept-licence".into()),
    }
}

fn usage() -> ExitCode {
    eprintln!(
        "usage:\n  scoreart-cli convert <in> <out>\n  scoreart-cli commands [--json]\n  scoreart-cli run [--in FILE | --sample | --new] [--cmd ID[=JSON]]… [--save OUT]\n  scoreart-cli info <file>\n  scoreart-cli sounds list [LIBRARY] | licence LIBRARY | install LIBRARY INSTRUMENT --accept-licence [--dir DIR]\n  (all forms: --sounds <file.sf2|file.sfz|folder|file.zip>, repeatable)"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = take_sounds(&mut args) {
        eprintln!("scoreart-cli: {e}");
        return ExitCode::FAILURE;
    }
    let result: Result<(), String> = match args.first().map(String::as_str) {
        Some("convert") => match (args.get(1), args.get(2)) {
            (Some(i), Some(o)) => load(i).and_then(|s| write_out(&s, o)).map(|w| {
                for f in w {
                    out!("wrote {f}");
                }
            }),
            _ => return usage(),
        },
        Some("sounds") => sounds(args.get(1..).unwrap_or(&[])),
        Some("commands") => {
            let s = Session::default();
            let infos: Vec<_> = scoreart_engine::registry().iter().map(|c| c.info(&s)).collect();
            if args.iter().any(|a| a == "--json") {
                out!("{}", serde_json::to_string_pretty(&infos).unwrap_or_default());
            } else {
                for c in infos {
                    out!("{:<24} {:<18} {}", c.id, c.shortcut.unwrap_or(""), c.label);
                    if !c.params.is_empty() {
                        out!("{:<24} {:<18} params: {}", "", "", c.params);
                    }
                }
            }
            Ok(())
        }
        Some("info") => match args.get(1) {
            Some(f) => load(f).map(|s| {
                let sc = &s.score;
                let info = json!({
                    "title": sc.meta.title, "composer": sc.meta.composer, "measures": sc.measures.len(),
                    "parts": sc.parts.iter().map(|p| json!({ "name": p.name, "staves": p.staff_count() })).collect::<Vec<_>>(),
                    "time": format!("{}/{}", sc.time_at(0).beats, sc.time_at(0).beat_type), "key_fifths": sc.key_at(0).fifths,
                });
                out!("{}", serde_json::to_string_pretty(&info).unwrap_or_default());
            }),
            None => return usage(),
        },
        Some("run") => {
            let mut s = new_session();
            let mut save: Option<String> = None;
            let mut i = 1;
            let mut out = Ok(());
            while i < args.len() {
                let a = args.get(i).map(String::as_str).unwrap_or("");
                let next = args.get(i + 1).cloned();
                match a {
                    "--sample" => s = new_session(),
                    "--new" => {
                        let _ = s.execute("file.new", &json!({}));
                    }
                    "--in" => {
                        i += 1;
                        match next.as_deref().map(load) {
                            Some(Ok(l)) => s = l,
                            Some(Err(e)) => {
                                out = Err(e);
                                break;
                            }
                            None => return usage(),
                        }
                    }
                    "--cmd" => {
                        i += 1;
                        let Some(spec) = next else { return usage() };
                        match parse_cmd(&spec).and_then(|(id, p)| s.execute(&id, &p).map_err(|e| format!("{id}: {e}")).map(|r| (id, r))) {
                            Ok((id, r)) => out!("{}", json!({ "command": id, "ok": true, "result": r })),
                            Err(e) => {
                                out = Err(e);
                                break;
                            }
                        }
                    }
                    "--save" => {
                        i += 1;
                        save = next;
                    }
                    _ => return usage(),
                }
                i += 1;
            }
            out.and_then(|_| match save {
                Some(o) => write_out(&s, &o).map(|w| {
                    for f in w {
                        out!("wrote {f}");
                    }
                }),
                None => Ok(()),
            })
        }
        _ => return usage(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("scoreart-cli: {e}");
            ExitCode::FAILURE
        }
    }
}
