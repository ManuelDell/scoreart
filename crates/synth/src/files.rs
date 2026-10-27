//! Where the files of a sample library come from: a folder on disk or a zip archive in memory.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Largest single file read from a library (samples included).
const MAX_FILE: u64 = 512 * 1024 * 1024;
/// Most files listed when scanning a library.
const MAX_LIST: usize = 50_000;

/// Read access to the files of a library, by `/`-separated path relative to its root.
pub trait Files: Send + Sync {
    fn read(&self, path: &str) -> Option<Vec<u8>>;
    /// Every file path in the library.
    fn list(&self) -> Vec<String>;
}

/// Normalises a path: `\` → `/`, `.` and `..` resolved, no leading `/`.
pub fn clean(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            p => out.push(p),
        }
    }
    out.join("/")
}

/// The folder part of a path ("" for a file at the root).
pub fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

/// A folder on disk.
pub struct Dir {
    root: PathBuf,
}

impl Dir {
    pub fn new(root: impl Into<PathBuf>) -> Dir {
        Dir { root: root.into() }
    }

    /// Resolves a relative path, matching names case-insensitively when the exact name is
    /// missing (libraries made on Windows or macOS).
    fn resolve(&self, path: &str) -> Option<PathBuf> {
        let exact = self.root.join(path);
        if exact.is_file() {
            return Some(exact);
        }
        let mut cur = self.root.clone();
        for part in path.split('/') {
            let direct = cur.join(part);
            if direct.exists() {
                cur = direct;
                continue;
            }
            let lower = part.to_lowercase();
            let found = std::fs::read_dir(&cur)
                .ok()?
                .filter_map(|e| e.ok())
                .find(|e| e.file_name().to_string_lossy().to_lowercase() == lower)?;
            cur = found.path();
        }
        cur.is_file().then_some(cur)
    }
}

impl Files for Dir {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        let p = self.resolve(&clean(path))?;
        let f = std::fs::File::open(p).ok()?;
        let mut buf = Vec::new();
        f.take(MAX_FILE).read_to_end(&mut buf).ok()?;
        Some(buf)
    }

    fn list(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![(self.root.clone(), String::new())];
        while let Some((dir, rel)) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.filter_map(|e| e.ok()) {
                if out.len() >= MAX_LIST {
                    return out;
                }
                let name = e.file_name().to_string_lossy().to_string();
                let r = if rel.is_empty() {
                    name
                } else {
                    format!("{rel}/{name}")
                };
                let path = e.path();
                if path.is_dir() {
                    if stack.len() < 10_000 {
                        stack.push((path, r));
                    }
                } else {
                    out.push(r);
                }
            }
        }
        out.sort();
        out
    }
}

/// A zip archive held in memory (used in the browser, where there are no folders).
pub struct Zip {
    bytes: Arc<Vec<u8>>,
    /// Lower-case clean path → name inside the archive.
    names: HashMap<String, String>,
}

impl Zip {
    pub fn new(bytes: Vec<u8>) -> Result<Zip, String> {
        let bytes = Arc::new(bytes);
        let archive =
            zip::ZipArchive::new(std::io::Cursor::new(bytes.as_slice())).map_err(|e| e.to_string())?;
        let names = archive
            .file_names()
            .take(MAX_LIST)
            .filter(|n| !n.ends_with('/'))
            .map(|n| (clean(n).to_lowercase(), n.to_string()))
            .collect();
        Ok(Zip { bytes, names })
    }
}

impl Files for Zip {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        let name = self.names.get(&clean(path).to_lowercase())?;
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(self.bytes.as_slice())).ok()?;
        let f = archive.by_name(name).ok()?;
        if f.size() > MAX_FILE {
            return None;
        }
        let mut buf = Vec::new();
        f.take(MAX_FILE).read_to_end(&mut buf).ok()?;
        Some(buf)
    }

    fn list(&self) -> Vec<String> {
        let mut v: Vec<String> = self.names.values().map(|n| clean(n)).collect();
        v.sort();
        v
    }
}

/// Files of one directory on disk when only a single `.sfz` file was chosen.
pub fn dir_of(file: &Path) -> (Dir, String) {
    let root = file.parent().map(Path::to_path_buf).unwrap_or_default();
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    (Dir::new(root), name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_paths() {
        assert_eq!(clean("a\\b/../c/./d.wav"), "a/c/d.wav");
        assert_eq!(clean("/x//y"), "x/y");
        assert_eq!(clean("../../z"), "z");
        assert_eq!(parent("a/b/c.sfz"), "a/b");
        assert_eq!(parent("c.sfz"), "");
    }
}
