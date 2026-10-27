//! SFZ sample instruments, written from the public SFZ format description (sfzformat.com).
//!
//! Supported: the `<control> <global> <master> <group> <region>` hierarchy, `#define` and
//! `#include`, key and velocity ranges, note names, round robins (`seq_*`, `lorand/hirand`),
//! keyswitches (the default switch is used), CC conditions, velocity and CC crossfades,
//! release triggers with `rt_decay`, choke groups (`group`/`off_by`), loops (also from the
//! sample's own loop points), the amplitude envelope, tuning, volume, pan and velocity
//! tracking. Unknown opcodes are ignored.

use std::collections::HashMap;
use std::sync::Arc;

use crate::files::{Files, clean, parent};
use crate::wav::{Sample, decode};

const MAX_INCLUDE_DEPTH: usize = 16;
const MAX_REGIONS: usize = 40_000;
const MAX_TEXT: usize = 4 * 1024 * 1024;
/// Samples kept in memory per instrument (floats), about 1.6 GB.
const MAX_SAMPLE_FLOATS: usize = 400_000_000;
const MAX_VOICES: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq)]
enum LoopMode {
    NoLoop,
    OneShot,
    Continuous,
    Sustain,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Trigger {
    Attack,
    Release,
    First,
    Legato,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum XfSource {
    Velocity,
    Cc(usize),
}

#[derive(Clone, Copy, Debug)]
struct Xfade {
    source: XfSource,
    lo: f32,
    hi: f32,
    fade_in: bool,
}

#[derive(Clone, Debug)]
struct Region {
    sample: usize,
    lokey: u8,
    hikey: u8,
    keycenter: f32,
    keytrack: f32,
    lovel: u8,
    hivel: u8,
    /// Semitones (transpose + tune).
    pitch: f32,
    gain: f32,
    pan: f32,
    veltrack: f32,
    veltrack_cc: Vec<(usize, f32)>,
    offset: usize,
    end: Option<usize>,
    loop_mode: LoopMode,
    loop_points: Option<(usize, usize)>,
    attack: f32,
    hold: f32,
    decay: f32,
    sustain: f32,
    release: f32,
    release_cc: Vec<(usize, f32)>,
    trigger: Trigger,
    rt_decay: f32,
    group: i64,
    off_by: Option<i64>,
    off_time: f32,
    seq_length: u32,
    seq_position: u32,
    lorand: f32,
    hirand: f32,
    conditions: Vec<(usize, f32, f32)>,
    amp_cc: Vec<(usize, f32)>,
    sw_last: Option<u8>,
    xfades: Vec<Xfade>,
    note_polyphony: Option<usize>,
}

/// A loaded SFZ instrument: its regions and decoded samples.
pub struct SfzInstrument {
    pub name: String,
    regions: Vec<Region>,
    samples: Vec<Arc<Sample>>,
    cc_init: [f32; 128],
    sw_range: Option<(u8, u8)>,
    sw_default: Option<u8>,
    /// Makes the loudest sample reach full scale (libraries are recorded with headroom).
    norm: f32,
    /// Problems found while loading (missing samples …), for the status line.
    pub warnings: Vec<String>,
}

impl std::fmt::Debug for SfzInstrument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SfzInstrument({}, {} regions)", self.name, self.regions.len())
    }
}

/// A MIDI note number from a number or a note name (`c4` = 60, `f#3`, `eb2`, `c-1` = 0).
fn note(v: &str) -> Option<u8> {
    let v = v.trim();
    if let Ok(n) = v.parse::<i32>() {
        return u8::try_from(n.clamp(0, 127)).ok();
    }
    let mut chars = v.chars();
    let step = match chars.next()?.to_ascii_lowercase() {
        'c' => 0,
        'd' => 2,
        'e' => 4,
        'f' => 5,
        'g' => 7,
        'a' => 9,
        'b' => 11,
        _ => return None,
    };
    let rest: String = chars.collect();
    let (alter, oct) = if let Some(r) = rest.strip_prefix('#') {
        (1, r)
    } else if let Some(r) = rest.strip_prefix('b') {
        (-1, r)
    } else {
        (0, rest.as_str())
    };
    let octave: i32 = oct.parse().ok()?;
    u8::try_from((12 * (octave + 1) + step + alter).clamp(0, 127)).ok()
}

fn num(v: &str) -> Option<f32> {
    v.trim().parse::<f32>().ok().filter(|x| x.is_finite())
}

/// Splits the opcode `name` into a base name and a trailing CC number (`locc64` → `locc`, 64).
fn cc_suffix(name: &str, base: &str) -> Option<usize> {
    name.strip_prefix(base)?
        .parse::<usize>()
        .ok()
        .filter(|n| *n < 128)
}

#[derive(Default)]
struct Parser<'a> {
    files: Option<&'a dyn Files>,
    root_dir: String,
    defines: Vec<(String, String)>,
    default_path: String,
    level: Level,
    global: Vec<(String, String)>,
    master: Vec<(String, String)>,
    group: Vec<(String, String)>,
    region: Vec<(String, String)>,
    /// Finished regions: opcodes and the sample folder prefix at the time.
    out: Vec<(Vec<(String, String)>, String)>,
    cc: Vec<(usize, f32)>,
    warnings: Vec<String>,
    chars: usize,
}

#[derive(Default, Clone, Copy, PartialEq)]
enum Level {
    #[default]
    None,
    Control,
    Global,
    Master,
    Group,
    Region,
    Ignored,
}

impl Parser<'_> {
    fn substitute(&self, s: &str) -> String {
        if !s.contains('$') {
            return s.to_string();
        }
        let mut out = s.to_string();
        // Longest names first so `$OFF1` does not eat into `$OFF10`.
        for (k, v) in &self.defines {
            if out.contains(k.as_str()) {
                out = out.replace(k.as_str(), v);
            }
        }
        out
    }

    fn finish_region(&mut self) {
        if self.level == Level::Region && self.out.len() < MAX_REGIONS {
            let mut ops = Vec::with_capacity(
                self.global.len() + self.master.len() + self.group.len() + self.region.len(),
            );
            ops.extend(self.global.iter().cloned());
            ops.extend(self.master.iter().cloned());
            ops.extend(self.group.iter().cloned());
            ops.append(&mut self.region);
            self.out.push((ops, self.default_path.clone()));
        }
        self.region.clear();
    }

    fn header(&mut self, name: &str) {
        self.finish_region();
        self.level = match name.trim() {
            "control" => Level::Control,
            "global" => {
                self.global.clear();
                self.master.clear();
                self.group.clear();
                Level::Global
            }
            "master" => {
                self.master.clear();
                self.group.clear();
                Level::Master
            }
            "group" => {
                self.group.clear();
                Level::Group
            }
            "region" => Level::Region,
            _ => Level::Ignored,
        };
    }

    fn opcode(&mut self, key: &str, value: &str) {
        let key = self.substitute(key);
        let value = self.substitute(value);
        let key = key.trim().to_string();
        if self.level == Level::Control || self.level == Level::None {
            if key == "default_path" {
                self.default_path = clean(&value);
                return;
            }
            if let Some(n) = cc_suffix(&key, "set_cc") {
                if let Some(v) = num(&value) {
                    self.cc.push((n, v));
                }
                return;
            }
            if let Some(n) = cc_suffix(&key, "set_hdcc") {
                if let Some(v) = num(&value) {
                    self.cc.push((n, v * 127.0));
                }
                return;
            }
        }
        let pair = (key, value.trim().to_string());
        match self.level {
            Level::Global => self.global.push(pair),
            Level::Master => self.master.push(pair),
            Level::Group => self.group.push(pair),
            Level::Region => self.region.push(pair),
            Level::Control | Level::None => self.global.push(pair),
            Level::Ignored => {}
        }
    }

    fn include(&mut self, path: &str, depth: usize) {
        if depth >= MAX_INCLUDE_DEPTH {
            self.warnings.push("includes nested too deeply".into());
            return;
        }
        let full = clean(&format!("{}/{}", self.root_dir, path));
        let Some(bytes) = self.files.and_then(|f| f.read(&full)) else {
            self.warnings.push(format!("missing include {path}"));
            return;
        };
        let text = String::from_utf8_lossy(&bytes).to_string();
        self.text(&text, depth + 1);
    }

    fn text(&mut self, text: &str, depth: usize) {
        self.chars = self.chars.saturating_add(text.len());
        if self.chars > MAX_TEXT * 4 {
            return;
        }
        let text = strip_comments(text);
        for line in text.lines() {
            self.line(line, depth);
        }
    }

    fn line(&mut self, line: &str, depth: usize) {
        let mut rest = line.trim();
        let mut guard = 0;
        while !rest.is_empty() && guard < 10_000 {
            guard += 1;
            if let Some(r) = rest.strip_prefix('<') {
                let (name, after) = r.split_once('>').unwrap_or((r, ""));
                self.header(name);
                rest = after.trim_start();
            } else if let Some(r) = rest.strip_prefix("#define") {
                let mut it = r.trim_start().splitn(2, char::is_whitespace);
                let name = it.next().unwrap_or("").to_string();
                let r2 = it.next().unwrap_or("").trim_start();
                let (value, after) = r2.split_once(char::is_whitespace).unwrap_or((r2, ""));
                if name.starts_with('$') {
                    let value = self.substitute(value);
                    self.defines.retain(|(k, _)| *k != name);
                    self.defines.push((name, value));
                    self.defines.sort_by_key(|d| std::cmp::Reverse(d.0.len()));
                }
                rest = after.trim_start();
            } else if let Some(r) = rest.strip_prefix("#include") {
                let r = r.trim_start();
                let (path, after) = match r.strip_prefix('"') {
                    Some(q) => q.split_once('"').unwrap_or((q, "")),
                    None => r.split_once(char::is_whitespace).unwrap_or((r, "")),
                };
                let path = self.substitute(path);
                self.include(&path, depth);
                rest = after.trim_start();
            } else if let Some(eq) = rest.find('=') {
                let key = &rest[..eq];
                if key.contains(char::is_whitespace) {
                    // A stray word: skip it.
                    let (_, after) = rest.split_once(char::is_whitespace).unwrap_or(("", ""));
                    rest = after.trim_start();
                    continue;
                }
                let after_eq = &rest[eq + 1..];
                let end = value_end(after_eq);
                let value = &after_eq[..end];
                self.opcode(key, value);
                rest = after_eq[end..].trim_start();
            } else {
                break;
            }
        }
    }
}

/// Removes `//` line comments and `/* */` block comments.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i < bytes.len() && !(bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/')) {
                if bytes[i] == b'\n' {
                    out.push('\n');
                }
                i += 1;
            }
            i += 2;
        } else {
            // Copy one whole character.
            let start = i;
            i += 1;
            while i < bytes.len() && !text.is_char_boundary(i) {
                i += 1;
            }
            out.push_str(text.get(start..i).unwrap_or(""));
        }
    }
    out
}

/// Where an opcode value ends: before whitespace followed by another opcode, a header or a
/// directive (values may contain spaces, e.g. sample paths).
fn value_end(s: &str) -> usize {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_whitespace() {
            let mut j = i;
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            let next = &s[j..];
            if next.is_empty()
                || next.starts_with('<')
                || next.starts_with("#include")
                || next.starts_with("#define")
            {
                return i;
            }
            let word_end = next
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
                .unwrap_or(next.len());
            if word_end > 0 && next[word_end..].starts_with('=') {
                return i;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    s.len()
}

fn build_region(ops: &[(String, String)], sample: usize) -> Region {
    let mut r = Region {
        sample,
        lokey: 0,
        hikey: 127,
        keycenter: 60.0,
        keytrack: 100.0,
        lovel: 1,
        hivel: 127,
        pitch: 0.0,
        gain: 1.0,
        pan: 0.0,
        veltrack: 100.0,
        veltrack_cc: Vec::new(),
        offset: 0,
        end: None,
        loop_mode: LoopMode::NoLoop,
        loop_points: None,
        attack: 0.0,
        hold: 0.0,
        decay: 0.0,
        sustain: 1.0,
        release: 0.01,
        release_cc: Vec::new(),
        trigger: Trigger::Attack,
        rt_decay: 0.0,
        group: 0,
        off_by: None,
        off_time: 0.006,
        seq_length: 1,
        seq_position: 1,
        lorand: 0.0,
        hirand: 1.0,
        conditions: Vec::new(),
        amp_cc: Vec::new(),
        sw_last: None,
        xfades: Vec::new(),
        note_polyphony: None,
    };
    let mut loop_mode = None;
    let (mut ls, mut le) = (None, None);
    let (mut transpose, mut tune, mut volume, mut amplitude) = (0.0f32, 0.0f32, 0.0f32, 100.0f32);
    let mut lo_cc: HashMap<usize, f32> = HashMap::new();
    let mut hi_cc: HashMap<usize, f32> = HashMap::new();
    let mut keycenter_set = false;
    for (k, v) in ops {
        let f = num(v);
        match k.as_str() {
            "lokey" => r.lokey = note(v).unwrap_or(r.lokey),
            "hikey" => r.hikey = note(v).unwrap_or(r.hikey),
            "key" => {
                if let Some(n) = note(v) {
                    r.lokey = n;
                    r.hikey = n;
                    if !keycenter_set {
                        r.keycenter = f32::from(n);
                    }
                }
            }
            "pitch_keycenter" => {
                if let Some(n) = note(v) {
                    r.keycenter = f32::from(n);
                    keycenter_set = true;
                }
            }
            "pitch_keytrack" => r.keytrack = f.unwrap_or(100.0),
            "lovel" => r.lovel = note(v).unwrap_or(1),
            "hivel" => r.hivel = note(v).unwrap_or(127),
            "transpose" => transpose = f.unwrap_or(0.0),
            "tune" | "pitch" => tune = f.unwrap_or(0.0),
            "volume" => volume = f.unwrap_or(0.0),
            "amplitude" => amplitude = f.unwrap_or(100.0),
            "pan" => r.pan = f.unwrap_or(0.0).clamp(-100.0, 100.0) / 100.0,
            "amp_veltrack" => r.veltrack = f.unwrap_or(100.0),
            "offset" => r.offset = f.unwrap_or(0.0).max(0.0) as usize,
            "end" => r.end = f.map(|x| x.max(0.0) as usize),
            "loop_mode" | "loopmode" => {
                loop_mode = Some(match v.as_str() {
                    "one_shot" => LoopMode::OneShot,
                    "loop_continuous" => LoopMode::Continuous,
                    "loop_sustain" => LoopMode::Sustain,
                    _ => LoopMode::NoLoop,
                })
            }
            "loop_start" | "loopstart" => ls = f.map(|x| x.max(0.0) as usize),
            "loop_end" | "loopend" => le = f.map(|x| x.max(0.0) as usize),
            "ampeg_attack" => r.attack = f.unwrap_or(0.0).clamp(0.0, 100.0),
            "ampeg_hold" => r.hold = f.unwrap_or(0.0).clamp(0.0, 100.0),
            "ampeg_decay" => r.decay = f.unwrap_or(0.0).clamp(0.0, 100.0),
            "ampeg_sustain" => r.sustain = (f.unwrap_or(100.0) / 100.0).clamp(0.0, 1.0),
            "ampeg_release" => r.release = f.unwrap_or(0.0).clamp(0.0, 100.0),
            "trigger" => {
                r.trigger = match v.as_str() {
                    "release" | "release_key" => Trigger::Release,
                    "first" => Trigger::First,
                    "legato" => Trigger::Legato,
                    _ => Trigger::Attack,
                }
            }
            "rt_decay" => r.rt_decay = f.unwrap_or(0.0).max(0.0),
            "group" => r.group = v.parse().unwrap_or(0),
            "off_by" => r.off_by = v.parse().ok(),
            "off_time" => r.off_time = f.unwrap_or(0.006).clamp(0.001, 30.0),
            "seq_length" => r.seq_length = f.unwrap_or(1.0).clamp(1.0, 100.0) as u32,
            "seq_position" => r.seq_position = f.unwrap_or(1.0).clamp(1.0, 100.0) as u32,
            "lorand" => r.lorand = f.unwrap_or(0.0),
            "hirand" => r.hirand = f.unwrap_or(1.0),
            "sw_last" => r.sw_last = note(v),
            "note_polyphony" => r.note_polyphony = f.map(|x| x.clamp(1.0, 64.0) as usize),
            "xfin_lovel" | "xfin_hivel" | "xfout_lovel" | "xfout_hivel" => xf_set(
                &mut r.xfades,
                XfSource::Velocity,
                k.starts_with("xfin"),
                k.ends_with("lovel"),
                f,
            ),
            other => {
                if let Some(n) = cc_suffix(other, "locc") {
                    lo_cc.insert(n, f.unwrap_or(0.0));
                } else if let Some(n) = cc_suffix(other, "hicc") {
                    hi_cc.insert(n, f.unwrap_or(127.0));
                } else if let Some(n) = cc_suffix(other, "amp_veltrack_oncc") {
                    r.veltrack_cc.push((n, f.unwrap_or(0.0)));
                } else if let Some(n) = cc_suffix(other, "ampeg_release_oncc") {
                    r.release_cc.push((n, f.unwrap_or(0.0)));
                } else if let Some(n) = cc_suffix(other, "amplitude_oncc") {
                    // Volume and pan stay with the mixer.
                    if n != 7 && n != 10 {
                        r.amp_cc.push((n, f.unwrap_or(100.0)));
                    }
                } else {
                    for (base, fade_in, lo) in [
                        ("xfin_locc", true, true),
                        ("xfin_hicc", true, false),
                        ("xfout_locc", false, true),
                        ("xfout_hicc", false, false),
                    ] {
                        if let Some(n) = cc_suffix(other, base) {
                            xf_set(&mut r.xfades, XfSource::Cc(n), fade_in, lo, f);
                        }
                    }
                }
            }
        }
    }
    for (n, lo) in &lo_cc {
        r.conditions
            .push((*n, *lo, hi_cc.get(n).copied().unwrap_or(127.0)));
    }
    for (n, hi) in &hi_cc {
        if !lo_cc.contains_key(n) {
            r.conditions.push((*n, 0.0, *hi));
        }
    }
    r.pitch = transpose + tune / 100.0;
    r.gain = 10f32.powf(volume.clamp(-144.0, 24.0) / 20.0) * (amplitude.clamp(0.0, 1000.0) / 100.0);
    r.loop_mode = loop_mode.unwrap_or(LoopMode::NoLoop);
    if let (Some(s), Some(e)) = (ls, le) {
        r.loop_points = Some((s, e));
    }
    // Without an explicit mode, loop points make the sample loop.
    if loop_mode.is_none() && r.loop_points.is_some() {
        r.loop_mode = LoopMode::Continuous;
    }
    if loop_mode.is_none() && r.loop_points.is_none() {
        // Marked later from the sample's own loop.
        r.loop_mode = LoopMode::NoLoop;
    }
    r
}

fn xf_set(xs: &mut Vec<Xfade>, source: XfSource, fade_in: bool, lo: bool, v: Option<f32>) {
    let Some(v) = v else { return };
    let x = match xs.iter_mut().find(|x| x.source == source && x.fade_in == fade_in) {
        Some(x) => x,
        None => {
            xs.push(Xfade {
                source,
                lo: if fade_in { 0.0 } else { 127.0 },
                hi: if fade_in { 0.0 } else { 127.0 },
                fade_in,
            });
            let Some(x) = xs.last_mut() else { return };
            x
        }
    };
    if lo {
        x.lo = v;
    } else {
        x.hi = v;
    }
}

impl SfzInstrument {
    /// Loads the instrument at `path` (relative to the library root) with its samples.
    pub fn load(files: &dyn Files, path: &str) -> Result<SfzInstrument, String> {
        let path = clean(path);
        let bytes = files.read(&path).ok_or_else(|| format!("cannot read {path}"))?;
        if bytes.len() > MAX_TEXT {
            return Err("SFZ file too large".into());
        }
        let text = String::from_utf8_lossy(&bytes).to_string();
        let mut p = Parser {
            files: Some(files),
            root_dir: parent(&path).to_string(),
            ..Parser::default()
        };
        p.text(&text, 0);
        p.finish_region();
        let mut cc_init = [0.0f32; 128];
        cc_init[7] = 100.0;
        cc_init[10] = 64.0;
        cc_init[11] = 127.0;
        for (n, v) in &p.cc {
            if let Some(c) = cc_init.get_mut(*n) {
                *c = v.clamp(0.0, 127.0);
            }
        }
        let mut warnings = p.warnings;
        let mut by_path: HashMap<String, Option<usize>> = HashMap::new();
        let mut samples: Vec<Arc<Sample>> = Vec::new();
        let mut floats = 0usize;
        let mut regions = Vec::new();
        let (mut sw_lo, mut sw_hi, mut sw_default) = (None, None, None);
        for (ops, default_path) in &p.out {
            for (k, v) in ops {
                match k.as_str() {
                    "sw_lokey" => sw_lo = note(v),
                    "sw_hikey" => sw_hi = note(v),
                    "sw_default" => sw_default = note(v),
                    _ => {}
                }
            }
            let Some(sample) = ops
                .iter()
                .rev()
                .find(|(k, _)| k == "sample")
                .map(|(_, v)| v.clone())
            else {
                continue;
            };
            if sample.starts_with('*') {
                // Built-in generators (*sine, *silence …) are not supported.
                continue;
            }
            let full = clean(&format!("{}/{}/{}", parent(&path), default_path, sample));
            let idx = match by_path.get(&full) {
                Some(i) => *i,
                None => {
                    let loaded = if floats < MAX_SAMPLE_FLOATS {
                        match files.read(&full).map(|b| decode(&b)) {
                            Some(Ok(s)) => {
                                floats += s.data.len();
                                samples.push(Arc::new(s));
                                Some(samples.len() - 1)
                            }
                            Some(Err(e)) => {
                                if warnings.len() < 20 {
                                    warnings.push(format!("{sample}: {e}"));
                                }
                                None
                            }
                            None => {
                                if warnings.len() < 20 {
                                    warnings.push(format!("missing sample {sample}"));
                                }
                                None
                            }
                        }
                    } else {
                        None
                    };
                    by_path.insert(full, loaded);
                    loaded
                }
            };
            let Some(idx) = idx else { continue };
            let mut r = build_region(ops, idx);
            let explicit_mode = ops.iter().any(|(k, _)| k == "loop_mode" || k == "loopmode");
            if let Some(s) = samples.get(idx) {
                if r.loop_points.is_none() {
                    r.loop_points = s.loop_points;
                    if !explicit_mode && s.loop_points.is_some() {
                        r.loop_mode = LoopMode::Continuous;
                    }
                }
                let frames = s.frames();
                r.loop_points = r.loop_points.filter(|(a, b)| a < b && *b < frames);
                if r.loop_points.is_none() && matches!(r.loop_mode, LoopMode::Continuous | LoopMode::Sustain)
                {
                    r.loop_mode = LoopMode::NoLoop;
                }
            }
            regions.push(r);
        }
        if regions.is_empty() {
            return Err(match warnings.first() {
                Some(w) => format!("no playable regions ({w})"),
                None => "no playable regions".into(),
            });
        }
        let sw_range = match (sw_lo, sw_hi) {
            (Some(a), Some(b)) if a <= b => Some((a, b)),
            _ => None,
        };
        let sw_default = sw_default.or_else(|| regions.iter().filter_map(|r| r.sw_last).min());
        let peak = samples
            .iter()
            .flat_map(|s| s.data.iter())
            .fold(0.0f32, |m, v| m.max(v.abs()));
        let norm = if peak > 1e-4 {
            (1.0 / peak).clamp(0.5, 8.0)
        } else {
            1.0
        };
        let name = path
            .rsplit('/')
            .next()
            .unwrap_or(&path)
            .trim_end_matches(".sfz")
            .to_string();
        Ok(SfzInstrument {
            name,
            regions,
            samples,
            cc_init,
            sw_range,
            sw_default,
            norm,
            warnings,
        })
    }
}

#[derive(Clone)]
struct Voice {
    /// Which region plays (checked by the tests).
    #[cfg_attr(not(test), allow(dead_code))]
    region: usize,
    sample: Arc<Sample>,
    key: u8,
    pos: f64,
    step: f64,
    end: f64,
    gain_l: f32,
    gain_r: f32,
    level: f32,
    age: f32,
    released: bool,
    release_mul: f32,
    looping: bool,
    loop_points: Option<(usize, usize)>,
    attack: f32,
    hold: f32,
    decay_mul: f32,
    sustain: f32,
    group: i64,
    off_by: Option<i64>,
    one_shot: bool,
    sustain_loop: bool,
}

/// Plays one [`SfzInstrument`].
pub struct SfzPlayer {
    inst: Arc<SfzInstrument>,
    voices: Vec<Voice>,
    rate: f32,
    cc: [f32; 128],
    /// Note-on velocity and time of each held key.
    held: [Option<(u8, f64)>; 128],
    seq: [u32; 128],
    rng: u32,
    time: f64,
    switch: Option<u8>,
}

impl SfzPlayer {
    pub fn new(inst: Arc<SfzInstrument>, rate: u32) -> SfzPlayer {
        SfzPlayer {
            cc: inst.cc_init,
            switch: inst.sw_default,
            inst,
            voices: Vec::new(),
            rate: rate.max(1000) as f32,
            held: [None; 128],
            seq: [0; 128],
            rng: 0x9E37_79B9,
            time: 0.0,
        }
    }

    fn random(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    fn matches(&self, r: &Region, key: u8, vel: u8, rand: f32, seq: u32, trigger: Trigger) -> bool {
        let trig_ok = matches!(
            (r.trigger, trigger),
            (Trigger::Release, Trigger::Release) | (Trigger::Attack | Trigger::First, Trigger::Attack)
        );
        trig_ok
            && (r.lokey..=r.hikey).contains(&key)
            && (r.lovel..=r.hivel).contains(&vel)
            && rand >= r.lorand
            && rand < r.hirand.max(r.lorand + 1e-6)
            && (r.seq_length <= 1 || seq % r.seq_length + 1 == r.seq_position)
            && r.sw_last.is_none_or(|s| Some(s) == self.switch)
            && r.conditions.iter().all(|(n, lo, hi)| {
                let v = self.cc.get(*n).copied().unwrap_or(0.0);
                v >= *lo && v <= *hi
            })
    }

    fn xfade_gain(&self, r: &Region, vel: u8) -> f32 {
        let mut g = 1.0f32;
        for x in &r.xfades {
            let v = match x.source {
                XfSource::Velocity => f32::from(vel),
                XfSource::Cc(n) => self.cc.get(n).copied().unwrap_or(0.0),
            };
            let span = (x.hi - x.lo).max(1e-3);
            let t = ((v - x.lo) / span).clamp(0.0, 1.0);
            // Equal-power crossfade.
            g *= if x.fade_in { t.sqrt() } else { (1.0 - t).sqrt() };
        }
        g
    }

    fn start(&mut self, key: u8, vel: u8, trigger: Trigger, extra_gain: f32) {
        let rand = self.random();
        let seq = self.seq[usize::from(key)];
        let chosen: Vec<usize> = (0..self.inst.regions.len())
            .filter(|i| {
                self.inst
                    .regions
                    .get(*i)
                    .is_some_and(|r| self.matches(r, key, vel, rand, seq, trigger))
            })
            .take(16)
            .collect();
        for ri in chosen {
            let Some(r) = self.inst.regions.get(ri).cloned() else {
                continue;
            };
            let Some(sample) = self.inst.samples.get(r.sample).cloned() else {
                continue;
            };
            // Choke groups and note polyphony.
            for v in &mut self.voices {
                let choke = v.off_by == Some(r.group) && r.group != 0;
                if choke && !v.released {
                    v.released = true;
                    v.release_mul = release_mul(r.off_time, self.rate);
                }
            }
            if let Some(n) = r.note_polyphony {
                let same: Vec<usize> = self
                    .voices
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| v.key == key && v.group == r.group && !v.released)
                    .map(|(i, _)| i)
                    .collect();
                if same.len() >= n {
                    for i in same.iter().take(same.len() + 1 - n) {
                        if let Some(v) = self.voices.get_mut(*i) {
                            v.released = true;
                            v.release_mul = release_mul(r.off_time, self.rate);
                        }
                    }
                }
            }
            let mut veltrack = r.veltrack;
            for (n, m) in &r.veltrack_cc {
                veltrack += m * self.cc.get(*n).copied().unwrap_or(0.0) / 127.0;
            }
            let vt = (veltrack / 100.0).clamp(-1.0, 1.0);
            let v2 = (f32::from(vel) / 127.0).powi(2);
            let vel_gain = if vt >= 0.0 {
                1.0 - vt + vt * v2
            } else {
                1.0 + vt * v2
            };
            let mut amp = r.gain * vel_gain * self.xfade_gain(&r, vel) * extra_gain * self.inst.norm;
            for (n, m) in &r.amp_cc {
                amp *= m / 100.0 * self.cc.get(*n).copied().unwrap_or(0.0) / 127.0;
            }
            if amp <= 1e-5 {
                continue;
            }
            let semis = (f32::from(key) - r.keycenter) * r.keytrack / 100.0 + r.pitch;
            let step = f64::from(2f32.powf(semis / 12.0)) * f64::from(sample.rate) / f64::from(self.rate);
            let frames = sample.frames();
            let end = r.end.map_or(frames, |e| (e + 1).min(frames)) as f64;
            let mut release = r.release;
            for (n, m) in &r.release_cc {
                release += m * self.cc.get(*n).copied().unwrap_or(0.0) / 127.0;
            }
            let angle = (r.pan + 1.0) * std::f32::consts::FRAC_PI_4;
            if self.voices.len() >= MAX_VOICES {
                self.voices.remove(0);
            }
            self.voices.push(Voice {
                region: ri,
                sample,
                key,
                pos: r.offset as f64,
                step: step.clamp(1e-4, 64.0),
                end,
                gain_l: amp * angle.cos() * std::f32::consts::SQRT_2,
                gain_r: amp * angle.sin() * std::f32::consts::SQRT_2,
                level: if r.attack <= 0.0 { 1.0 } else { 0.0 },
                age: 0.0,
                released: false,
                release_mul: release_mul(release.max(0.005), self.rate),
                looping: matches!(r.loop_mode, LoopMode::Continuous | LoopMode::Sustain),
                loop_points: r.loop_points,
                attack: r.attack,
                hold: r.hold,
                decay_mul: if r.decay > 0.0 {
                    release_mul(r.decay, self.rate)
                } else {
                    0.0
                },
                sustain: r.sustain,
                group: r.group,
                off_by: r.off_by,
                one_shot: r.loop_mode == LoopMode::OneShot || trigger == Trigger::Release,
                sustain_loop: r.loop_mode == LoopMode::Sustain,
            });
        }
    }

    pub fn note_on(&mut self, key: u8, vel: u8) {
        let key = key.min(127);
        let vel = vel.clamp(1, 127);
        if let Some((lo, hi)) = self.inst.sw_range
            && (lo..=hi).contains(&key)
        {
            self.switch = Some(key);
            return;
        }
        // The dynamic also drives the mod wheel, which many orchestral libraries crossfade on.
        self.cc[1] = f32::from(vel);
        self.held[usize::from(key)] = Some((vel, self.time));
        self.start(key, vel, Trigger::Attack, 1.0);
        let s = &mut self.seq[usize::from(key)];
        *s = s.wrapping_add(1);
    }

    pub fn note_off(&mut self, key: u8) {
        let key = key.min(127);
        for v in &mut self.voices {
            if v.key == key && !v.released && !v.one_shot {
                v.released = true;
            }
        }
        if let Some((vel, at)) = self.held[usize::from(key)].take() {
            let held = (self.time - at) as f32;
            let has_release = self.inst.regions.iter().any(|r| r.trigger == Trigger::Release);
            if has_release {
                let decay = self
                    .inst
                    .regions
                    .iter()
                    .find(|r| r.trigger == Trigger::Release && (r.lokey..=r.hikey).contains(&key))
                    .map_or(0.0, |r| r.rt_decay);
                let g = 10f32.powf(-decay * held / 20.0);
                self.start(key, vel, Trigger::Release, g);
            }
        }
    }

    pub fn all_off(&mut self) {
        for v in &mut self.voices {
            v.released = true;
        }
    }

    pub fn active(&self) -> bool {
        !self.voices.is_empty()
    }

    /// Adds the sound of `l.len()` frames into `l` and `r`.
    pub fn render(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len().min(r.len());
        let dt = 1.0 / self.rate;
        for v in &mut self.voices {
            let s = &v.sample;
            for i in 0..n {
                // Envelope.
                if v.released {
                    v.level *= v.release_mul;
                } else if v.age < v.attack {
                    v.level = (v.age / v.attack.max(1e-4)).min(1.0);
                } else if v.age < v.attack + v.hold {
                    v.level = 1.0;
                } else if v.decay_mul > 0.0 {
                    v.level = v.sustain + (v.level - v.sustain) * v.decay_mul;
                } else {
                    v.level = v.sustain;
                }
                v.age += dt;
                // Four-point interpolation.
                let ip = v.pos.floor();
                let frac = (v.pos - ip) as f32;
                let i0 = ip as usize;
                let (a_l, a_r) = if i0 > 0 { s.frame(i0 - 1) } else { s.frame(i0) };
                let (b_l, b_r) = s.frame(i0);
                let (c_l, c_r) = s.frame(i0 + 1);
                let (d_l, d_r) = s.frame(i0 + 2);
                let out_l = hermite(a_l, b_l, c_l, d_l, frac);
                let out_r = hermite(a_r, b_r, c_r, d_r, frac);
                if let (Some(lo), Some(ro)) = (l.get_mut(i), r.get_mut(i)) {
                    *lo += out_l * v.gain_l * v.level;
                    *ro += out_r * v.gain_r * v.level;
                }
                v.pos += v.step;
                let keep_looping = v.looping && !(v.sustain_loop && v.released);
                if keep_looping && let Some((ls, le)) = v.loop_points {
                    let len = (le + 1 - ls) as f64;
                    if v.pos >= (le + 1) as f64 && len > 0.0 {
                        v.pos -= len * ((v.pos - (le + 1) as f64) / len).floor().max(0.0) + len;
                    }
                }
                if v.pos >= v.end || (v.released && v.level < 3e-4) {
                    v.level = 0.0;
                    v.end = -1.0;
                    break;
                }
            }
        }
        self.voices.retain(|v| v.end >= 0.0);
        self.time += f64::from(n as f32 * dt);
    }
}

fn release_mul(secs: f32, rate: f32) -> f32 {
    // -60 dB after `secs`.
    (0.001f32.ln() / (secs.max(0.001) * rate)).exp()
}

#[inline]
fn hermite(a: f32, b: f32, c: f32, d: f32, t: f32) -> f32 {
    let c0 = b;
    let c1 = 0.5 * (c - a);
    let c2 = a - 2.5 * b + 2.0 * c - 0.5 * d;
    let c3 = 0.5 * (d - a) + 1.5 * (b - c);
    ((c3 * t + c2) * t + c1) * t + c0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wav::tests::wav16;

    struct Mem(HashMap<String, Vec<u8>>);
    impl Files for Mem {
        fn read(&self, path: &str) -> Option<Vec<u8>> {
            self.0.get(&clean(path)).cloned()
        }
        fn list(&self) -> Vec<String> {
            self.0.keys().cloned().collect()
        }
    }

    fn tone(len: usize) -> Vec<u8> {
        let s: Vec<i16> = (0..len)
            .map(|i| ((i as f32 * 0.1).sin() * 12000.0) as i16)
            .collect();
        wav16(&s, 1, 44_100, Some((100, len as u32 - 1)))
    }

    #[test]
    fn note_names() {
        assert_eq!(note("c4"), Some(60));
        assert_eq!(note("C#0"), Some(13));
        assert_eq!(note("eb2"), Some(39));
        assert_eq!(note("c-1"), Some(0));
        assert_eq!(note("72"), Some(72));
        assert_eq!(note("h3"), None);
    }

    #[test]
    fn parses_headers_defines_and_includes() {
        let sfz = r#"
// a comment
#define $EXT wav
<control> default_path=Samples/ set_hdcc64=0
<global> ampeg_release=0.3 sw_lokey=c0 sw_hikey=d0 sw_default=c0
<master> sw_last=c0
<group> lovel=1 hivel=64 #include "inc/regions.txt"
<group> lovel=65 hivel=127 volume=-3
<region> sample=Piano C4 loud.$EXT key=60
<master> sw_last=d0
<region> sample=other.$EXT lokey=0 hikey=127
<global> trigger=release rt_decay=3
<region> sample=rel.$EXT lokey=0 hikey=127 /* block
comment */ volume=-20
"#;
        let regions = "<region> sample=Piano C4 soft.$EXT lokey=c4 hikey=b4 pitch_keycenter=60";
        let mut m = HashMap::new();
        m.insert("lib/piano.sfz".to_string(), sfz.as_bytes().to_vec());
        m.insert("lib/inc/regions.txt".to_string(), regions.as_bytes().to_vec());
        for name in ["Piano C4 soft", "Piano C4 loud", "other", "rel"] {
            m.insert(format!("lib/Samples/{name}.wav"), tone(4000));
        }
        let files = Mem(m);
        let inst = SfzInstrument::load(&files, "lib/piano.sfz").unwrap();
        assert_eq!(inst.regions.len(), 4, "{:?}", inst.warnings);
        assert_eq!(inst.sw_range, Some((12, 14)));
        assert_eq!(inst.sw_default, Some(12));
        let soft = &inst.regions[0];
        assert_eq!((soft.lokey, soft.hikey, soft.hivel), (60, 71, 64));
        assert!((soft.release - 0.3).abs() < 1e-6);
        let loud = &inst.regions[1];
        assert_eq!((loud.lokey, loud.lovel), (60, 65));
        assert!(loud.gain < 0.75);
        assert_eq!(inst.regions[3].trigger, Trigger::Release);
        assert!(inst.regions[3].gain < 0.11);
        // The sample loop from the WAV file is used.
        assert_eq!(soft.loop_mode, LoopMode::Continuous);

        // Playing: the soft layer for a soft C4, the release sample after note off.
        let inst = Arc::new(inst);
        let mut p = SfzPlayer::new(inst, 44_100);
        p.note_on(60, 40);
        assert_eq!(p.voices.len(), 1);
        assert_eq!(p.voices[0].region, 0);
        let (mut l, mut r) = (vec![0.0f32; 512], vec![0.0f32; 512]);
        p.render(&mut l, &mut r);
        assert!(l.iter().any(|v| v.abs() > 0.01));
        p.note_off(60);
        assert!(p.voices.iter().any(|v| v.region == 3));
        // Keyswitch to D0: the other master plays.
        p.note_on(14, 100);
        p.note_on(40, 100);
        assert!(p.voices.iter().any(|v| v.region == 2));
        for _ in 0..400 {
            p.render(&mut l, &mut r);
        }
        p.all_off();
        for _ in 0..400 {
            p.render(&mut l, &mut r);
        }
        assert!(!p.active());
    }

    #[test]
    fn hostile_sfz_does_not_panic() {
        let cases: [&str; 7] = [
            "",
            "<region>",
            "<region> sample=missing.wav",
            "<region sample=x.wav",
            "#include \"self.sfz\"\n<region> sample=t.wav lokey=200 hikey=-4 pitch_keycenter=zz",
            "<region> sample=t.wav loop_start=900000 loop_end=10 offset=999999999 tune=1e30 volume=nan",
            "#define $A $A$A\n<region> sample=t.wav key=$A ampeg_release=-1 seq_length=0",
        ];
        for c in cases {
            let mut m = HashMap::new();
            m.insert("self.sfz".to_string(), c.as_bytes().to_vec());
            m.insert("t.wav".to_string(), tone(300));
            let files = Mem(m);
            if let Ok(inst) = SfzInstrument::load(&files, "self.sfz") {
                let mut p = SfzPlayer::new(Arc::new(inst), 44_100);
                for k in [0u8, 60, 127] {
                    p.note_on(k, 127);
                }
                let (mut l, mut r) = (vec![0.0f32; 4096], vec![0.0f32; 4096]);
                p.render(&mut l, &mut r);
                p.all_off();
                p.render(&mut l, &mut r);
                assert!(l.iter().all(|v| v.is_finite()));
            }
        }
    }
}
