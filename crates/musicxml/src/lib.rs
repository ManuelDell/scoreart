//! MusicXML 4.0 import and export, written from the W3C Music Notation Community Group
//! specification. Reads `.musicxml`/`.xml` (score-partwise) and compressed `.mxl`; writes both.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod dom;
mod export;
mod import;
pub mod mscz;

use std::io::{Cursor, Read, Write};

use scoreart_model::Score;

/// Largest file we accept (compressed or not).
pub const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("XML error: {0}")]
    Xml(String),
    #[error("not supported: {0}")]
    Unsupported(String),
    #[error("invalid MusicXML: {0}")]
    Invalid(String),
    #[error("MXL container: {0}")]
    Zip(String),
}

/// Parses MusicXML text.
pub fn from_str(xml: &str) -> Result<Score, Error> {
    if xml.len() > MAX_FILE_BYTES {
        return Err(Error::Invalid("file too large".into()));
    }
    let root = dom::parse(xml)?;
    import::read(&root)
}

/// Parses `.musicxml` bytes or an `.mxl` (zip) container.
pub fn from_bytes(bytes: &[u8]) -> Result<Score, Error> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(Error::Invalid("file too large".into()));
    }
    if bytes.starts_with(b"PK") {
        // A MuseScore .mscz holds an .mscx; a compressed MusicXML (.mxl) a container.xml.
        if let Some((mscx, style)) = read_mscz(bytes)? {
            let mut score = mscz::from_mscx(&mscx)?;
            if let Some(mss) = style {
                mscz::apply_style_file(&mut score, &mss);
            }
            return Ok(score);
        }
        return from_str(&read_mxl(bytes)?);
    }
    let text = decode_text(bytes);
    let head: String = text.chars().take(2000).collect();
    if head.contains("<museScore") {
        return mscz::from_mscx(&text);
    }
    from_str(&text)
}

/// UTF-8 (with or without BOM) or UTF-16 with BOM.
fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    let utf16 = |le: bool, b: &[u8]| -> String {
        let (pairs, _) = b.as_chunks::<2>();
        let units: Vec<u16> = pairs
            .iter()
            .map(|c| {
                if le {
                    u16::from_le_bytes(*c)
                } else {
                    u16::from_be_bytes(*c)
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(true, rest);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(false, rest);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

/// The `.mscx` inside a MuseScore `.mscz`, if this zip is one, and its style file.
fn read_mscz(bytes: &[u8]) -> Result<Option<(String, Option<String>)>, Error> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| Error::Zip(e.to_string()))?;
    let Some(name) = zip
        .file_names()
        .find(|n| n.to_ascii_lowercase().ends_with(".mscx") && !n.contains('/'))
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let style_name = zip
        .file_names()
        .find(|n| n.to_ascii_lowercase().ends_with(".mss") && !n.contains('/'))
        .map(str::to_string);
    let mut read = |name: &str| -> Result<String, Error> {
        let f = zip
            .by_name(name)
            .map_err(|e| Error::Zip(format!("{name}: {e}")))?;
        let mut out = Vec::new();
        f.take(MAX_FILE_BYTES as u64 + 1)
            .read_to_end(&mut out)
            .map_err(|e| Error::Zip(e.to_string()))?;
        if out.len() > MAX_FILE_BYTES {
            return Err(Error::Invalid("file too large".into()));
        }
        Ok(decode_text(&out))
    };
    let mscx = read(&name)?;
    // A broken style file only costs the page settings.
    let style = style_name.and_then(|n| read(&n).ok());
    Ok(Some((mscx, style)))
}

fn read_mxl(bytes: &[u8]) -> Result<String, Error> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| Error::Zip(e.to_string()))?;
    let mut read_entry = |name: &str| -> Result<Vec<u8>, Error> {
        let f = zip
            .by_name(name)
            .map_err(|e| Error::Zip(format!("{name}: {e}")))?;
        let mut out = Vec::new();
        f.take(MAX_FILE_BYTES as u64 + 1)
            .read_to_end(&mut out)
            .map_err(|e| Error::Zip(e.to_string()))?;
        if out.len() > MAX_FILE_BYTES {
            return Err(Error::Invalid("file too large".into()));
        }
        Ok(out)
    };
    let container = read_entry("META-INF/container.xml")?;
    let root = dom::parse(&decode_text(&container))?;
    let path = root
        .child("rootfiles")
        .and_then(|r| r.children_named("rootfile").next())
        .and_then(|r| r.attr("full-path"))
        .map(str::to_string)
        .ok_or_else(|| Error::Zip("container.xml has no rootfile".into()))?;
    Ok(decode_text(&read_entry(&path)?))
}

/// Writes MusicXML 4.0 (uncompressed).
pub fn to_string(score: &Score) -> String {
    export::write(score)
}

/// Writes a compressed `.mxl` container.
pub fn to_mxl(score: &Score) -> Result<Vec<u8>, Error> {
    let xml = to_string(score);
    let mut buf = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let stored =
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflated =
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let z = |e: zip::result::ZipError| Error::Zip(e.to_string());
        let io = |e: std::io::Error| Error::Zip(e.to_string());
        zip.start_file("mimetype", stored).map_err(z)?;
        zip.write_all(b"application/vnd.recordare.musicxml").map_err(io)?;
        zip.start_file("META-INF/container.xml", deflated).map_err(z)?;
        zip.write_all(
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<container><rootfiles><rootfile full-path=\"score.musicxml\" media-type=\"application/vnd.recordare.musicxml+xml\"/></rootfiles></container>\n",
        )
        .map_err(io)?;
        zip.start_file("score.musicxml", deflated).map_err(z)?;
        zip.write_all(xml.as_bytes()).map_err(io)?;
        zip.finish().map_err(z)?;
    }
    Ok(buf.into_inner())
}

#[cfg(test)]
mod mscz_tests;
#[cfg(test)]
mod tests;
