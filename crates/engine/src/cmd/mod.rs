//! The command registry. Each command is a [`CommandSpec`]; ids are stable API for the UI,
//! the CLI and agents.

mod edit;
mod marks;
mod note;
mod score;
mod text;
pub use text::LOCKED as TEXT_LOCKED;

use std::sync::OnceLock;

use serde::Serialize;
use serde_json::Value;

use crate::{Error, Result, Session};

pub type Enabled = fn(&Session) -> std::result::Result<(), String>;
pub type Run = fn(&mut Session, &Value) -> Result<Value>;

pub struct CommandSpec {
    pub id: &'static str,
    pub label: &'static str,
    /// Menu placement, e.g. `["Add", "Notes"]`. Empty = not in menus.
    pub menu: &'static [&'static str],
    pub shortcut: Option<&'static str>,
    /// Parameter documentation (JSON object keys).
    pub params: &'static str,
    pub enabled: Enabled,
    pub run: Run,
    pub undoable: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CommandInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub menu: Vec<&'static str>,
    pub shortcut: Option<&'static str>,
    pub params: &'static str,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
}

impl CommandSpec {
    pub fn info(&self, s: &Session) -> CommandInfo {
        let e = (self.enabled)(s);
        CommandInfo {
            id: self.id,
            label: self.label,
            menu: self.menu.to_vec(),
            shortcut: self.shortcut,
            params: self.params,
            enabled: e.is_ok(),
            disabled_reason: e.err(),
        }
    }
}

pub fn always(_: &Session) -> std::result::Result<(), String> {
    Ok(())
}

pub fn registry() -> &'static [CommandSpec] {
    static REG: OnceLock<Vec<CommandSpec>> = OnceLock::new();
    REG.get_or_init(|| {
        let mut v = Vec::new();
        v.extend(edit::commands());
        v.extend(note::commands());
        v.extend(marks::commands());
        v.extend(score::commands());
        v.extend(text::commands());
        v
    })
}

pub fn find(id: &str) -> Option<&'static CommandSpec> {
    registry().iter().find(|c| c.id == id)
}

// ---- parameter helpers ----

pub(crate) fn p_str<'a>(p: &'a Value, key: &str) -> Option<&'a str> {
    p.get(key).and_then(Value::as_str)
}

pub(crate) fn p_i64(p: &Value, key: &str) -> Option<i64> {
    p.get(key).and_then(|v| {
        v.as_i64()
            .or_else(|| v.as_f64().filter(|f| f.is_finite()).map(|f| f as i64))
            .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
    })
}

pub(crate) fn p_usize(p: &Value, key: &str) -> Option<usize> {
    p_i64(p, key).and_then(|v| usize::try_from(v).ok())
}

pub(crate) fn need_str<'a>(p: &'a Value, key: &str) -> Result<&'a str> {
    p_str(p, key).ok_or_else(|| Error::Param(format!("`{key}` (string) is required")))
}

pub(crate) fn need_i64(p: &Value, key: &str) -> Result<i64> {
    p_i64(p, key).ok_or_else(|| Error::Param(format!("`{key}` (number) is required")))
}

pub(crate) fn need_usize(p: &Value, key: &str) -> Result<usize> {
    p_usize(p, key).ok_or_else(|| Error::Param(format!("`{key}` (non-negative number) is required")))
}
