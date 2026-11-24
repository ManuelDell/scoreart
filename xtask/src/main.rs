//! `cargo xtask <task>`:
//! - `ci`      fmt check, clippy -D warnings, tests, assets, layers, wasm
//! - `assets`  every asset file has a row in ATTRIBUTION.md
//! - `layers`  crates below the UI don't depend on egui/eframe/rfd; layer order holds
//! - `wasm`    the web app builds for wasm32
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

fn run(cmd: &str, args: &[&str]) -> Result<(), String> {
    eprintln!("==> {cmd} {}", args.join(" "));
    let status = Command::new(cmd)
        .args(args)
        .current_dir(root())
        .status()
        .map_err(|e| format!("{cmd}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{cmd} {} failed", args.join(" ")))
    }
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            files_under(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// Asset files (anything under assets/ that isn't a licence text) must be listed in ATTRIBUTION.md.
fn assets() -> Result<(), String> {
    let r = root();
    let table =
        std::fs::read_to_string(r.join("ATTRIBUTION.md")).map_err(|e| format!("ATTRIBUTION.md: {e}"))?;
    let mut files = Vec::new();
    files_under(&r.join("assets"), &mut files);
    files_under(&r.join("docs/images"), &mut files);
    let mut missing = Vec::new();
    for f in files {
        let rel = f
            .strip_prefix(&r)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if !table.contains(&format!("`{rel}`")) {
            missing.push(rel);
        }
    }
    if missing.is_empty() {
        eprintln!("==> assets: every asset is attributed");
        Ok(())
    } else {
        Err(format!(
            "assets missing from ATTRIBUTION.md:\n  {}",
            missing.join("\n  ")
        ))
    }
}

/// Layer of each crate; a crate may only depend on crates of a lower layer.
const LAYERS: &[(&str, u8)] = &[
    ("scoreart-model", 1),
    ("scoreart-musicxml", 2),
    ("scoreart-midi", 2),
    ("scoreart-synth", 3),
    ("scoreart-engrave", 2),
    ("scoreart-pdf", 3),
    ("scoreart-render", 3),
    ("scoreart-engine", 4),
    ("scoreart-ui-egui", 6),
];
const UI_ONLY: &[&str] = &["egui", "eframe", "rfd", "winit"];

fn layers() -> Result<(), String> {
    let r = root();
    let mut errors = Vec::new();
    for (name, layer) in LAYERS {
        let dir = name.trim_start_matches("scoreart-");
        let manifest = std::fs::read_to_string(r.join("crates").join(dir).join("Cargo.toml"))
            .map_err(|e| format!("{name}: {e}"))?;
        let deps = manifest
            .split("[dependencies]")
            .nth(1)
            .unwrap_or("")
            .split("\n[")
            .next()
            .unwrap_or("");
        for line in deps.lines() {
            let dep = line.split(['=', ' ']).next().unwrap_or("").trim();
            if dep.is_empty() {
                continue;
            }
            if *layer < 6 && UI_ONLY.contains(&dep) {
                errors.push(format!("{name} (L{layer}) depends on UI crate {dep}"));
            }
            if let Some((_, dl)) = LAYERS.iter().find(|(n, _)| *n == dep)
                && dl >= layer
            {
                errors.push(format!("{name} (L{layer}) depends on {dep} (L{dl})"));
            }
        }
    }
    if errors.is_empty() {
        eprintln!("==> layers: ok");
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn wasm() -> Result<(), String> {
    let target = ["-p", "scoreart-web", "--target", "wasm32-unknown-unknown"];
    run(
        "cargo",
        &[&["clippy"][..], &target, &["--", "-D", "warnings"]].concat(),
    )?;
    run(
        "cargo",
        &[
            "build",
            "-p",
            "scoreart-web",
            "--target",
            "wasm32-unknown-unknown",
        ],
    )
}

fn ci() -> Result<(), String> {
    run("cargo", &["fmt", "--all", "--check"])?;
    run(
        "cargo",
        &["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    )?;
    run("cargo", &["test", "--workspace"])?;
    assets()?;
    layers()?;
    wasm()
}

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_default();
    let r = match task.as_str() {
        "ci" => ci(),
        "assets" => assets(),
        "layers" => layers(),
        "wasm" => wasm(),
        _ => Err("usage: cargo xtask ci|assets|layers|wasm".into()),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}
