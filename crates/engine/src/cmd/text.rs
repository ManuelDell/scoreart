//! Texts that are not notes: the styles of every kind of text, and the blocks of the page
//! (title, subtitle, composer, lyricist, copyright, page number, headings, text blocks,
//! headers and footers), which can be added, edited, moved and removed.

use serde_json::{Value, json};

use scoreart_model::{BlockRef, TextAlign, TextBlock, TextFont, TextKind, clip, limits};

use super::{CommandSpec, always, need_str, p_str, p_usize};
use crate::{Error, Result, Session};

fn kind(p: &Value) -> Result<TextKind> {
    let k = need_str(p, "kind")?;
    TextKind::from_id(k).ok_or_else(|| Error::Param(format!("no text kind {k:?}")))
}

/// `block`: a fixed block by kind ("title", "page-number" …) or an added block by index.
fn block(s: &Session, p: &Value) -> Result<BlockRef> {
    match p.get("block") {
        Some(Value::String(k)) => match TextKind::from_id(k) {
            Some(
                k @ (TextKind::Title
                | TextKind::Subtitle
                | TextKind::Composer
                | TextKind::Lyricist
                | TextKind::Copyright
                | TextKind::PageNumber),
            ) => Ok(BlockRef::Fixed(k)),
            _ => Err(Error::Param(format!("no fixed block {k:?}"))),
        },
        Some(v) => {
            let i = v
                .as_u64()
                .and_then(|i| usize::try_from(i).ok())
                .ok_or_else(|| Error::Param("block: a kind or an index".into()))?;
            if i < s.score.blocks.len() {
                Ok(BlockRef::Added(i))
            } else {
                Err(Error::Param("no such block".into()))
            }
        }
        None => Err(Error::Param("block".into())),
    }
}

fn meta_field(s: &mut Session, k: TextKind) -> Option<&mut String> {
    let m = &mut s.score.meta;
    match k {
        TextKind::Title => Some(&mut m.title),
        TextKind::Subtitle => Some(&mut m.subtitle),
        TextKind::Composer => Some(&mut m.composer),
        TextKind::Lyricist => Some(&mut m.lyricist),
        TextKind::Copyright => Some(&mut m.copyright),
        _ => None,
    }
}

/// An offset in staff spaces → tenths.
fn tenths(p: &Value, key: &str) -> Option<i16> {
    p.get(key)
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .map(|v| (v * 10.0).round().clamp(-2000.0, 2000.0) as i16)
}

fn style_json(s: &Session, k: TextKind) -> Value {
    let st = s.score.text_styles.get(k);
    json!({
        "kind": k.id(),
        "font": match st.font { TextFont::Sans => "sans", TextFont::Serif => "serif" },
        "size": st.size,
        "bold": st.bold,
        "italic": st.italic,
        "align": match st.align { TextAlign::Left => "left", TextAlign::Center => "center", TextAlign::Right => "right" },
    })
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "text.styles",
            label: "Text styles",
            menu: &[],
            shortcut: None,
            params: "",
            enabled: always,
            run: |s, _| {
                Ok(Value::Array(
                    TextKind::ALL.iter().map(|k| style_json(s, *k)).collect(),
                ))
            },
            undoable: false,
        },
        CommandSpec {
            id: "text.style",
            label: "Text style",
            menu: &[],
            shortcut: None,
            params: "kind (all — font and reset only —, title, subtitle, composer, lyricist, copyright, heading, text, header, footer, page-number, instrument-name, measure-number, tempo, system-text, rehearsal, staff-text, chord-symbol, lyrics, fingering), font? sans|serif, size? (pt, 4…96), bold?, italic?, align? left|center|right, reset? true",
            enabled: always,
            run: |s, p| {
                if p_str(p, "kind") == Some("all") {
                    // Every kind at once: the font, or back to the defaults.
                    let reset = p.get("reset").and_then(Value::as_bool) == Some(true);
                    let font = match p_str(p, "font") {
                        Some("sans") => Some(TextFont::Sans),
                        Some("serif") => Some(TextFont::Serif),
                        Some(other) => return Err(Error::Param(format!("font: sans or serif, not {other}"))),
                        None => None,
                    };
                    for k in TextKind::ALL {
                        let mut st = if reset {
                            k.default_style()
                        } else {
                            s.score.text_styles.get(k)
                        };
                        if let Some(f) = font {
                            st.font = f;
                        }
                        s.score.text_styles.set(k, st);
                    }
                    return Ok(Value::Null);
                }
                let k = kind(p)?;
                let mut st = if p.get("reset").and_then(Value::as_bool) == Some(true) {
                    k.default_style()
                } else {
                    s.score.text_styles.get(k)
                };
                if let Some(f) = p_str(p, "font") {
                    st.font = match f {
                        "sans" => TextFont::Sans,
                        "serif" => TextFont::Serif,
                        other => return Err(Error::Param(format!("font: sans or serif, not {other}"))),
                    };
                }
                if let Some(v) = p.get("size").and_then(Value::as_f64).filter(|v| v.is_finite()) {
                    st.size = ((v.clamp(4.0, 96.0) * 2.0).round() / 2.0) as f32;
                }
                if let Some(v) = p.get("bold").and_then(Value::as_bool) {
                    st.bold = v;
                }
                if let Some(v) = p.get("italic").and_then(Value::as_bool) {
                    st.italic = v;
                }
                if let Some(a) = p_str(p, "align") {
                    st.align = match a {
                        "left" => TextAlign::Left,
                        "center" => TextAlign::Center,
                        "right" => TextAlign::Right,
                        other => {
                            return Err(Error::Param(format!("align: left, center or right, not {other}")));
                        }
                    };
                }
                s.score.text_styles.set(k, st);
                Ok(style_json(s, k))
            },
            undoable: true,
        },
        CommandSpec {
            id: "block.add",
            label: "Add text block",
            menu: &[],
            shortcut: None,
            params: "kind: heading|text (above the system with the measure) or header|footer (every page after the first), text, measure? (default: the selected one)",
            enabled: always,
            run: |s, p| {
                let k = kind(p)?;
                if !matches!(
                    k,
                    TextKind::Heading | TextKind::Text | TextKind::Header | TextKind::Footer
                ) {
                    return Err(Error::Param("kind: heading, text, header or footer".into()));
                }
                let text = clip(need_str(p, "text")?.trim_end());
                if text.trim().is_empty() {
                    return Err(Error::Param("the text is empty".into()));
                }
                if s.score.blocks.len() >= limits::MAX_BLOCKS {
                    return Err(Error::Param("too many text blocks".into()));
                }
                let last = s.score.measures.len().saturating_sub(1);
                let measure = p_usize(p, "measure").unwrap_or_else(|| s.focus().2).min(last);
                s.score.blocks.push(TextBlock {
                    kind: k,
                    text,
                    measure,
                    dx: 0,
                    dy: 0,
                });
                Ok(json!({ "index": s.score.blocks.len() - 1 }))
            },
            undoable: true,
        },
        CommandSpec {
            id: "block.edit",
            label: "Edit text block",
            menu: &[],
            shortcut: None,
            params: "block (title, subtitle, composer, lyricist, copyright, or the index of an added block), text",
            enabled: always,
            run: |s, p| {
                let text = clip(need_str(p, "text")?.trim_end());
                match block(s, p)? {
                    BlockRef::Fixed(TextKind::PageNumber) => {
                        return Err(Error::Param("the page number has no text".into()));
                    }
                    BlockRef::Fixed(k) => {
                        if let Some(f) = meta_field(s, k) {
                            *f = text;
                        }
                    }
                    BlockRef::Added(i) => {
                        if text.trim().is_empty() {
                            return Err(Error::Param(
                                "the text is empty (remove the block instead)".into(),
                            ));
                        }
                        if let Some(b) = s.score.blocks.get_mut(i) {
                            b.text = text;
                        }
                    }
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "block.move",
            label: "Move text block",
            menu: &[],
            shortcut: None,
            params: "block (a kind like title or page-number, or the index of an added block), dx, dy (offset from its place in staff spaces, y down; 0, 0 = back to its place)",
            enabled: always,
            run: |s, p| {
                let b = block(s, p)?;
                let (dx, dy) = (tenths(p, "dx").unwrap_or(0), tenths(p, "dy").unwrap_or(0));
                match b {
                    BlockRef::Fixed(k) => {
                        if (dx, dy) == (0, 0) {
                            s.score.block_offsets.remove(&k);
                        } else {
                            s.score.block_offsets.insert(k, (dx, dy));
                        }
                    }
                    BlockRef::Added(i) => {
                        if let Some(x) = s.score.blocks.get_mut(i) {
                            x.dx = dx;
                            x.dy = dy;
                        }
                    }
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "block.remove",
            label: "Remove text block",
            menu: &[],
            shortcut: None,
            params: "block (a kind like title — clears that text; page-number hides the numbers — or the index of an added block)",
            enabled: always,
            run: |s, p| {
                match block(s, p)? {
                    BlockRef::Fixed(TextKind::PageNumber) => s.score.hide_page_numbers = true,
                    BlockRef::Fixed(k) => {
                        if let Some(f) = meta_field(s, k) {
                            f.clear();
                        }
                        s.score.block_offsets.remove(&k);
                    }
                    BlockRef::Added(i) => {
                        s.score.blocks.remove(i);
                    }
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "page.numbers",
            label: "Page numbers",
            menu: &[],
            shortcut: None,
            params: "on: true|false (from page 2 on)",
            enabled: always,
            run: |s, p| {
                let on = p
                    .get("on")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| Error::Param("on: true or false".into()))?;
                s.score.hide_page_numbers = !on;
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "block.list",
            label: "Text blocks",
            menu: &[],
            shortcut: None,
            params: "",
            enabled: always,
            run: |s, _| {
                let added: Vec<Value> = s
                    .score
                    .blocks
                    .iter()
                    .enumerate()
                    .map(|(i, b)| {
                        json!({ "index": i, "kind": b.kind.id(), "text": b.text, "measure": b.measure,
                                "dx": f32::from(b.dx) / 10.0, "dy": f32::from(b.dy) / 10.0 })
                    })
                    .collect();
                Ok(json!({ "blocks": added, "page_numbers": !s.score.hide_page_numbers }))
            },
            undoable: false,
        },
    ]
}
