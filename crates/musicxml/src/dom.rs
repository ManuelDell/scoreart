//! A tiny, bounded XML element tree on top of quick-xml.

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::Error;

const MAX_DEPTH: usize = 64;
const MAX_NODES: usize = 4_000_000;

#[derive(Debug, Default, Clone)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Element>,
    pub text: String,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    pub fn has(&self, name: &str) -> bool {
        self.child(name).is_some()
    }

    /// Trimmed text of the child `name`.
    pub fn child_text(&self, name: &str) -> Option<&str> {
        self.child(name).map(|c| c.text.trim())
    }

    pub fn child_num(&self, name: &str) -> Option<f64> {
        self.child_text(name)?
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
    }

    /// Descends a path of child names.
    pub fn path(&self, names: &[&str]) -> Option<&Element> {
        let mut e = self;
        for n in names {
            e = e.child(n)?;
        }
        Some(e)
    }
}

fn local(name: &[u8]) -> String {
    let s = String::from_utf8_lossy(name);
    match s.rsplit_once(':') {
        Some((_, l)) => l.to_string(),
        None => s.into_owned(),
    }
}

fn open(e: &quick_xml::events::BytesStart<'_>) -> Element {
    let mut el = Element {
        name: local(e.name().as_ref()),
        ..Element::default()
    };
    for a in e.attributes().flatten() {
        let key = local(a.key.as_ref());
        let value = a.unescape_value().map(|v| v.into_owned()).unwrap_or_default();
        el.attrs.push((key, value));
    }
    el
}

pub fn parse(xml: &str) -> Result<Element, Error> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut stack: Vec<Element> = vec![Element {
        name: "#document".into(),
        ..Element::default()
    }];
    let mut nodes = 0usize;
    loop {
        let ev = reader.read_event().map_err(|e| Error::Xml(e.to_string()))?;
        match ev {
            Event::Start(e) => {
                nodes += 1;
                if stack.len() > MAX_DEPTH || nodes > MAX_NODES {
                    return Err(Error::Xml("document too deep or too large".into()));
                }
                stack.push(open(&e));
            }
            Event::Empty(e) => {
                nodes += 1;
                if nodes > MAX_NODES {
                    return Err(Error::Xml("document too large".into()));
                }
                let el = open(&e);
                if let Some(top) = stack.last_mut() {
                    top.children.push(el);
                }
            }
            Event::End(_) => {
                if stack.len() <= 1 {
                    return Err(Error::Xml("unbalanced end tag".into()));
                }
                if let Some(done) = stack.pop()
                    && let Some(top) = stack.last_mut()
                {
                    top.children.push(done);
                }
            }
            Event::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text
                        .push_str(&t.decode().map_err(|e| Error::Xml(e.to_string()))?);
                }
            }
            Event::CData(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&String::from_utf8_lossy(&t));
                }
            }
            Event::GeneralRef(r) => {
                let resolved = match r.resolve_char_ref() {
                    Ok(Some(c)) => Some(c),
                    _ => match r.decode().map_err(|e| Error::Xml(e.to_string()))?.as_ref() {
                        "amp" => Some('&'),
                        "lt" => Some('<'),
                        "gt" => Some('>'),
                        "quot" => Some('"'),
                        "apos" => Some('\''),
                        _ => None,
                    },
                };
                if let (Some(c), Some(top)) = (resolved, stack.last_mut()) {
                    top.text.push(c);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    // Be lenient with truncated files: close whatever is still open.
    while stack.len() > 1 {
        if let Some(done) = stack.pop()
            && let Some(top) = stack.last_mut()
        {
            top.children.push(done);
        }
    }
    let doc = stack
        .into_iter()
        .next()
        .ok_or_else(|| Error::Xml("empty document".into()))?;
    doc.children
        .into_iter()
        .next()
        .ok_or_else(|| Error::Xml("no root element".into()))
}

/// Escapes text for element content and attribute values.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' && c != '\r' => {}
            c => out.push(c),
        }
    }
    out
}
