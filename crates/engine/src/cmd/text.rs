//! Texts that are not notes: the styles of every kind of text, and the blocks of the page
//! (title, subtitle, composer, lyricist, copyright, page number, headings, text blocks,
//! headers and footers), which can be added, edited, moved and removed.

use serde_json::{Value, json};

use scoreart_model::{
    BlockLook, BlockRef, MAX_BLOCK_LINES, TextAlign, TextBlock, TextFont, TextKind, clip, limits,
};

use super::{CommandSpec, always, need_str, p_str, p_usize};
use crate::{Error, Result, Session};

fn kind(p: &Value) -> Result<TextKind> {
    let k = need_str(p, "kind")?;
    TextKind::from_id(k).ok_or_else(|| Error::Param(format!("no text kind {k:?}")))
}

/// Why a locked block does not move.
pub const LOCKED: &str = "This text is locked — unlock it to move it";

/// Every block the score shows: the title page's texts that are filled in, the page number,
/// header and footer when shown, and the added blocks.
fn all_blocks(s: &Session) -> Vec<BlockRef> {
    let sc = &s.score;
    let m = &sc.meta;
    let mut out: Vec<BlockRef> = [
        (TextKind::Title, !m.title.is_empty()),
        (TextKind::Subtitle, !m.subtitle.is_empty()),
        (TextKind::Composer, !m.composer.is_empty()),
        (TextKind::Lyricist, !m.lyricist.is_empty()),
        (TextKind::Copyright, !m.copyright.is_empty()),
        (TextKind::PageNumber, !sc.hide_page_numbers),
        (TextKind::Header, sc.header_footer.show_header),
        (TextKind::Footer, sc.header_footer.show_footer),
    ]
    .into_iter()
    .filter(|(_, shown)| *shown)
    .map(|(k, _)| BlockRef::Fixed(k))
    .collect();
    out.extend((0..sc.blocks.len()).map(BlockRef::Added));
    out
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
                | TextKind::PageNumber
                | TextKind::Header
                | TextKind::Footer),
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
            params: "kind: heading|text (above the system with the measure; measure = the number of measures: after the last system) or header|footer (the middle of the header or footer), text, measure? (default: the selected one); and any look of block.look",
            enabled: always,
            run: |s, p| {
                let k = kind(p)?;
                if !matches!(
                    k,
                    TextKind::Heading | TextKind::Text | TextKind::Header | TextKind::Footer
                ) {
                    return Err(Error::Param("kind: heading, text, header or footer".into()));
                }
                let text = clip(p_str(p, "text").unwrap_or("").trim_end());
                // An answer field may be empty: lines to write on, or a frame.
                let empty_ok = matches!(k, TextKind::Heading | TextKind::Text)
                    && look(p, BlockLook::default(), k).is_ok_and(|l| l.lines > 0 || l.border || l.shade);
                if text.trim().is_empty() && !empty_ok {
                    return Err(Error::Param("the text is empty".into()));
                }
                // A header or footer text goes into the middle of the header or footer.
                if matches!(k, TextKind::Header | TextKind::Footer) {
                    let hf = &mut s.score.header_footer;
                    let (show, rows) = if k == TextKind::Header {
                        (&mut hf.show_header, &mut hf.header)
                    } else {
                        (&mut hf.show_footer, &mut hf.footer)
                    };
                    *show = true;
                    for row in rows.iter_mut() {
                        row[1] = text.clone();
                    }
                    return Ok(Value::Null);
                }
                if s.score.blocks.len() >= limits::MAX_BLOCKS {
                    return Err(Error::Param("too many text blocks".into()));
                }
                // The number of measures: after the last system.
                let last = s.score.measures.len();
                let measure = p_usize(p, "measure").unwrap_or_else(|| s.focus().2).min(last);
                let mut b = TextBlock::new(k, text, measure);
                b.look = look(p, b.look, k)?;
                s.score.blocks.push(b);
                Ok(json!({ "index": s.score.blocks.len() - 1 }))
            },
            undoable: true,
        },
        CommandSpec {
            id: "block.look",
            label: "Format text field",
            menu: &[],
            shortcut: None,
            params: LOOK_PARAMS,
            enabled: always,
            run: |s, p| {
                let i = match block(s, p)? {
                    BlockRef::Added(i) => i,
                    BlockRef::Fixed(_) => {
                        return Err(Error::Param(
                            "only added text blocks have a look of their own (title-page texts: text.style)"
                                .into(),
                        ));
                    }
                };
                let Some(b) = s.score.blocks.get(i) else {
                    return Err(Error::Param(format!("no text block {i}")));
                };
                let new = look(p, b.look, b.kind)?;
                if let Some(b) = s.score.blocks.get_mut(i) {
                    b.look = new;
                }
                Ok(look_json(&new))
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
                    BlockRef::Fixed(k @ (TextKind::Header | TextKind::Footer)) => {
                        // The middle field of every page's header or footer.
                        let hf = &mut s.score.header_footer;
                        let (show, rows) = if k == TextKind::Header {
                            (&mut hf.show_header, &mut hf.header)
                        } else {
                            (&mut hf.show_footer, &mut hf.footer)
                        };
                        *show = true;
                        for row in rows.iter_mut() {
                            row[1] = text.clone();
                        }
                    }
                    BlockRef::Fixed(k) => {
                        if let Some(f) = meta_field(s, k) {
                            *f = text;
                        }
                    }
                    BlockRef::Added(i) => {
                        let field = s
                            .score
                            .blocks
                            .get(i)
                            .is_some_and(|b| b.look.lines > 0 || b.look.border || b.look.shade);
                        if text.trim().is_empty() && !field {
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
                if s.score.block_locked(b) {
                    return Err(Error::Disabled(LOCKED.into()));
                }
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
            id: "block.lock",
            label: "Lock text position",
            menu: &[],
            shortcut: None,
            params: "block (a kind like title or page-number, the index of an added block, or all), locked? (true holds it at its place, false lets it move; default: toggle)",
            enabled: always,
            run: |s, p| {
                let want = match p.get("locked") {
                    None | Some(Value::Null) => None,
                    Some(v) => Some(
                        v.as_bool()
                            .ok_or_else(|| Error::Param("locked: true or false".into()))?,
                    ),
                };
                if p.get("block").and_then(Value::as_str) == Some("all") {
                    let all = all_blocks(s);
                    // Toggle: lock all unless all are locked already.
                    let on = want.unwrap_or_else(|| !all.iter().all(|b| s.score.block_locked(*b)));
                    for b in all {
                        s.score.set_block_locked(b, on);
                    }
                    if !on {
                        s.score.locked_blocks.clear();
                    }
                    return Ok(json!({ "locked": on }));
                }
                let b = block(s, p)?;
                let on = want.unwrap_or(!s.score.block_locked(b));
                s.score.set_block_locked(b, on);
                Ok(json!({ "locked": on }))
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
                    BlockRef::Fixed(TextKind::Header) => s.score.header_footer.show_header = false,
                    BlockRef::Fixed(TextKind::Footer) => s.score.header_footer.show_footer = false,
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
            id: "page.header-footer",
            label: "Header and footer",
            menu: &[],
            shortcut: None,
            params: "any of: show_header, header_first_page, header_odd_even, show_footer, footer_first_page, footer_odd_even, page_number_first_page (true|false); odd_header, even_header, odd_footer, even_footer ([left, centre, right] texts; macros: $p page number (not on the first page), $P page number, $n number of pages, $C copyright (first page), $c copyright, $$ dollar, $:workTitle: $:subtitle: $:composer: $:lyricist: $:copyright:); reset: true (as MuseScore 4: page number and copyright in the footer)",
            enabled: always,
            run: |s, p| {
                let mut hf = if p.get("reset").and_then(Value::as_bool) == Some(true) {
                    scoreart_model::HeaderFooter::default()
                } else {
                    s.score.header_footer.clone()
                };
                for (k, slot) in [
                    ("show_header", &mut hf.show_header),
                    ("header_first_page", &mut hf.header_first_page),
                    ("header_odd_even", &mut hf.header_odd_even),
                    ("show_footer", &mut hf.show_footer),
                    ("footer_first_page", &mut hf.footer_first_page),
                    ("footer_odd_even", &mut hf.footer_odd_even),
                    ("page_number_first_page", &mut hf.page_number_first_page),
                ] {
                    match p.get(k) {
                        None | Some(Value::Null) => {}
                        Some(Value::Bool(b)) => *slot = *b,
                        Some(_) => return Err(Error::Param(format!("{k}: true or false"))),
                    }
                }
                let [oh, eh] = &mut hf.header;
                let [of, ef] = &mut hf.footer;
                for (k, row) in [
                    ("odd_header", oh),
                    ("even_header", eh),
                    ("odd_footer", of),
                    ("even_footer", ef),
                ] {
                    match p.get(k) {
                        None | Some(Value::Null) => {}
                        Some(Value::Array(a)) if a.len() == 3 => {
                            for (slot, v) in row.iter_mut().zip(a) {
                                *slot = clip(
                                    v.as_str()
                                        .ok_or_else(|| Error::Param(format!("{k}: three texts")))?,
                                );
                            }
                        }
                        Some(_) => return Err(Error::Param(format!("{k}: [left, centre, right]"))),
                    }
                }
                s.score.header_footer = hf;
                let h = &s.score.header_footer;
                Ok(json!({
                    "show_header": h.show_header, "header_first_page": h.header_first_page, "header_odd_even": h.header_odd_even,
                    "odd_header": h.header[0], "even_header": h.header[1],
                    "show_footer": h.show_footer, "footer_first_page": h.footer_first_page, "footer_odd_even": h.footer_odd_even,
                    "odd_footer": h.footer[0], "even_footer": h.footer[1],
                    "page_number_first_page": h.page_number_first_page,
                }))
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
                                "dx": f32::from(b.dx) / 10.0, "dy": f32::from(b.dy) / 10.0, "look": look_json(&b.look) })
                    })
                    .collect();
                Ok(json!({ "blocks": added, "page_numbers": !s.score.hide_page_numbers }))
            },
            undoable: false,
        },
    ]
}

const LOOK_PARAMS: &str = "block (the index of an added block); any of: font sans|serif, size (pt, 4…96), bold, italic, underline, align left|center|right (its own formatting; style: kind goes back to the style of its kind), border, shade (light grey background), width (percent of the line, 10…100), lines (empty lines to write on below the text, 0…30), beside (stands beside the block before it)";

/// A block's look with the parameters of `p` applied; its own style starts from its kind's.
fn look(p: &Value, mut l: BlockLook, kind: TextKind) -> Result<BlockLook> {
    let flag = |k: &str| -> Result<Option<bool>> {
        match p.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            Some(_) => Err(Error::Param(format!("{k}: true or false"))),
        }
    };
    if p_str(p, "style") == Some("kind") {
        l.style = None;
    }
    let own = ["font", "size", "bold", "italic", "align"]
        .iter()
        .any(|k| p.get(*k).is_some_and(|v| !v.is_null()));
    if own {
        let mut st = l.style.unwrap_or_else(|| kind.default_style());
        if let Some(f) = p_str(p, "font") {
            st.font = match f {
                "sans" => TextFont::Sans,
                "serif" => TextFont::Serif,
                other => return Err(Error::Param(format!("font: sans or serif, not {other}"))),
            };
        }
        if let Some(v) = p.get("size").filter(|v| !v.is_null()) {
            let v = v
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or_else(|| Error::Param("size: points, 4…96".into()))?;
            st.size = ((v.clamp(4.0, 96.0) * 2.0).round() / 2.0) as f32;
        }
        if let Some(v) = flag("bold")? {
            st.bold = v;
        }
        if let Some(v) = flag("italic")? {
            st.italic = v;
        }
        if let Some(a) = p_str(p, "align") {
            st.align = match a {
                "left" => TextAlign::Left,
                "center" => TextAlign::Center,
                "right" => TextAlign::Right,
                other => return Err(Error::Param(format!("align: left, center or right, not {other}"))),
            };
        }
        l.style = Some(st);
    }
    for (k, slot) in [
        ("underline", &mut l.underline),
        ("border", &mut l.border),
        ("shade", &mut l.shade),
        ("beside", &mut l.beside),
    ] {
        if let Some(v) = flag(k)? {
            *slot = v;
        }
    }
    let num = |k: &str, lo: f64, hi: f64| -> Result<Option<u8>> {
        match p.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v
                .as_f64()
                .filter(|v| v.is_finite())
                .map(|v| Some(v.round().clamp(lo, hi) as u8))
                .ok_or_else(|| Error::Param(format!("{k}: a number, {lo}…{hi}"))),
        }
    };
    if let Some(w) = num("width", 10.0, 100.0)? {
        l.width = w;
    }
    if let Some(n) = num("lines", 0.0, f64::from(MAX_BLOCK_LINES))? {
        l.lines = n;
    }
    Ok(l.clamped())
}

fn look_json(l: &BlockLook) -> Value {
    let st = l.style.map(|s| {
        json!({
            "font": if s.font == TextFont::Sans { "sans" } else { "serif" },
            "size": s.size, "bold": s.bold, "italic": s.italic,
            "align": match s.align { TextAlign::Left => "left", TextAlign::Center => "center", TextAlign::Right => "right" },
        })
    });
    json!({
        "style": st, "underline": l.underline, "border": l.border, "shade": l.shade,
        "width": l.width, "lines": l.lines, "beside": l.beside,
    })
}
