//! Texts of a score that are not notes: their styles (font, size, weight, slant, alignment) by
//! kind, and the text blocks of the page (title, headings, header, footer …), which can be moved.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The two text fonts: a book serif (Crimson Text, the default) and a clean sans-serif.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextFont {
    Sans,
    #[default]
    Serif,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
}

/// Every kind of text with its own style, in the order shown to the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextKind {
    Title,
    Subtitle,
    Composer,
    Lyricist,
    Copyright,
    Heading,
    Text,
    Header,
    Footer,
    PageNumber,
    InstrumentName,
    MeasureNumber,
    Tempo,
    SystemText,
    Rehearsal,
    StaffText,
    ChordSymbol,
    Lyrics,
    Fingering,
}

impl TextKind {
    pub const ALL: [TextKind; 19] = [
        TextKind::Title,
        TextKind::Subtitle,
        TextKind::Composer,
        TextKind::Lyricist,
        TextKind::Copyright,
        TextKind::Heading,
        TextKind::Text,
        TextKind::Header,
        TextKind::Footer,
        TextKind::PageNumber,
        TextKind::InstrumentName,
        TextKind::MeasureNumber,
        TextKind::Tempo,
        TextKind::SystemText,
        TextKind::Rehearsal,
        TextKind::StaffText,
        TextKind::ChordSymbol,
        TextKind::Lyrics,
        TextKind::Fingering,
    ];

    pub fn id(self) -> &'static str {
        match self {
            TextKind::Title => "title",
            TextKind::Subtitle => "subtitle",
            TextKind::Composer => "composer",
            TextKind::Lyricist => "lyricist",
            TextKind::Copyright => "copyright",
            TextKind::Heading => "heading",
            TextKind::Text => "text",
            TextKind::Header => "header",
            TextKind::Footer => "footer",
            TextKind::PageNumber => "page-number",
            TextKind::InstrumentName => "instrument-name",
            TextKind::MeasureNumber => "measure-number",
            TextKind::Tempo => "tempo",
            TextKind::SystemText => "system-text",
            TextKind::Rehearsal => "rehearsal",
            TextKind::StaffText => "staff-text",
            TextKind::ChordSymbol => "chord-symbol",
            TextKind::Lyrics => "lyrics",
            TextKind::Fingering => "fingering",
        }
    }

    pub fn from_id(s: &str) -> Option<TextKind> {
        TextKind::ALL.into_iter().find(|k| k.id() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            TextKind::Title => "Title",
            TextKind::Subtitle => "Subtitle",
            TextKind::Composer => "Composer",
            TextKind::Lyricist => "Lyricist",
            TextKind::Copyright => "Copyright",
            TextKind::Heading => "Heading",
            TextKind::Text => "Text block",
            TextKind::Header => "Header",
            TextKind::Footer => "Footer",
            TextKind::PageNumber => "Page number",
            TextKind::InstrumentName => "Instrument names",
            TextKind::MeasureNumber => "Measure numbers",
            TextKind::Tempo => "Tempo",
            TextKind::SystemText => "System text",
            TextKind::Rehearsal => "Rehearsal marks",
            TextKind::StaffText => "Staff text",
            TextKind::ChordSymbol => "Chord symbols",
            TextKind::Lyrics => "Lyrics",
            TextKind::Fingering => "Fingering",
        }
    }

    /// The group it is listed under.
    pub fn group(self) -> &'static str {
        match self {
            TextKind::Title
            | TextKind::Subtitle
            | TextKind::Composer
            | TextKind::Lyricist
            | TextKind::Copyright => "Title page",
            TextKind::Heading | TextKind::Text => "Between the systems",
            TextKind::Header | TextKind::Footer | TextKind::PageNumber => "Every page",
            _ => "In the music",
        }
    }

    /// Whether it is a block of the page (and so has an alignment and can be moved freely).
    pub fn is_block(self) -> bool {
        self.group() != "In the music"
    }

    /// The size in points the engraver's own measures of this kind are made for (a text drawn
    /// at its built-in size is this big); styles scale from it.
    pub fn reference_pt(self) -> f32 {
        match self {
            TextKind::Title => 22.0,
            TextKind::Subtitle => 12.0,
            TextKind::Composer | TextKind::Lyricist => 9.5,
            TextKind::Copyright | TextKind::Header | TextKind::Footer | TextKind::PageNumber => 8.0,
            TextKind::Heading => 14.0,
            TextKind::Text | TextKind::Tempo => 10.0,
            TextKind::InstrumentName | TextKind::SystemText | TextKind::StaffText => 9.5,
            TextKind::MeasureNumber => 7.0,
            TextKind::Rehearsal | TextKind::ChordSymbol => 11.0,
            TextKind::Lyrics => 9.0,
            TextKind::Fingering => 8.0,
        }
    }

    /// The style a new score starts with: as in a new MuseScore 4 score.
    pub fn default_style(self) -> TextStyle {
        let s = |size: f32, bold: bool, italic: bool, align: TextAlign| TextStyle {
            font: TextFont::Serif,
            size,
            bold,
            italic,
            align,
        };
        use TextAlign::*;
        match self {
            TextKind::Title => s(24.0, false, false, Center),
            TextKind::Subtitle => s(14.0, false, false, Center),
            TextKind::Composer => s(12.0, false, false, Right),
            TextKind::Lyricist => s(12.0, false, false, Left),
            TextKind::Copyright => s(8.0, false, false, Center),
            TextKind::Heading => s(14.0, true, false, Left),
            TextKind::Text => s(12.0, false, false, Left),
            TextKind::Header => s(8.0, false, false, Center),
            TextKind::Footer => s(8.0, false, false, Center),
            TextKind::PageNumber => s(8.0, false, false, Right),
            TextKind::InstrumentName => s(12.0, false, false, Right),
            TextKind::MeasureNumber => s(8.0, false, false, Left),
            TextKind::Tempo => s(12.0, true, false, Left),
            TextKind::SystemText => s(10.0, false, false, Left),
            TextKind::Rehearsal => s(14.0, true, false, Center),
            TextKind::StaffText => s(10.0, false, false, Left),
            TextKind::ChordSymbol => s(12.0, false, false, Left),
            TextKind::Lyrics => s(11.0, false, false, Center),
            TextKind::Fingering => s(8.0, false, false, Center),
        }
    }
}

/// How one kind of text looks. `size` is in points.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    pub font: TextFont,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub align: TextAlign,
}

impl TextStyle {
    /// Size in staff spaces for a staff space of `space_mm` (1 pt = 0.3528 mm).
    pub fn size_sp(&self, space_mm: f32) -> f32 {
        (self.size.clamp(4.0, 96.0) * 0.3528 / space_mm.max(0.1)).clamp(0.3, 60.0)
    }
}

/// The text styles of a score; kinds not set keep their defaults.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TextStyles(pub BTreeMap<TextKind, TextStyle>);

impl TextStyles {
    pub fn get(&self, k: TextKind) -> TextStyle {
        self.0.get(&k).copied().unwrap_or_else(|| k.default_style())
    }

    pub fn set(&mut self, k: TextKind, s: TextStyle) {
        if s == k.default_style() {
            self.0.remove(&k);
        } else {
            self.0.insert(k, s);
        }
    }
}

/// A block of text the user added: a heading or a text between the systems (above the system
/// that starts with `measure`), or the header or footer of every page after the first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextBlock {
    pub kind: TextKind,
    pub text: String,
    #[serde(default)]
    pub measure: usize,
    /// Moved from its place, tenths of a staff space (y down).
    #[serde(default)]
    pub dx: i16,
    #[serde(default)]
    pub dy: i16,
}

/// Which block: the title page's texts and the page number by kind, added blocks by index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlockRef {
    Fixed(TextKind),
    Added(usize),
}

/// Offsets of the fixed blocks (title, subtitle, composer, lyricist, copyright, page number).
pub type BlockOffsets = BTreeMap<TextKind, (i16, i16)>;

/// Everything about a score's texts that MusicXML has no place for, saved as one piece.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TextSettings {
    #[serde(default)]
    pub styles: TextStyles,
    #[serde(default)]
    pub blocks: Vec<TextBlock>,
    #[serde(default)]
    pub offsets: BlockOffsets,
    #[serde(default)]
    pub hide_page_numbers: bool,
    #[serde(default)]
    pub header_footer: HeaderFooter,
}

impl TextSettings {
    pub fn of(score: &crate::Score) -> TextSettings {
        TextSettings {
            styles: score.text_styles.clone(),
            blocks: score.blocks.clone(),
            offsets: score.block_offsets.clone(),
            hide_page_numbers: score.hide_page_numbers,
            header_footer: score.header_footer.clone(),
        }
    }

    pub fn apply(self, score: &mut crate::Score) {
        score.text_styles = self.styles;
        score.blocks = self.blocks;
        score.block_offsets = self.offsets;
        score.hide_page_numbers = self.hide_page_numbers;
        score.header_footer = self.header_footer.clipped();
    }
}

/// Headers and footers as MuseScore has them: shown or not, on the first page or not, odd and
/// even pages alike or different, each with a left, centre and right field. The fields may hold
/// macros: `$p` page number (not on the first page), `$P` page number on every page, `$n` number
/// of pages, `$C` copyright (first page only), `$c` copyright on every page, `$$` a dollar sign,
/// `$:tag:` a score property (workTitle, subtitle, composer, lyricist, copyright …).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeaderFooter {
    pub show_header: bool,
    pub header_first_page: bool,
    pub header_odd_even: bool,
    /// `[odd, even]` pages, each `[left, centre, right]`.
    pub header: [[String; 3]; 2],
    pub show_footer: bool,
    pub footer_first_page: bool,
    pub footer_odd_even: bool,
    pub footer: [[String; 3]; 2],
    /// `$p` also on the first page.
    pub page_number_first_page: bool,
}

impl Default for HeaderFooter {
    /// As in a new MuseScore 4 score: no header; the page number in the outer corner and the
    /// copyright in the middle of the footer.
    fn default() -> Self {
        let s = |a: &str, b: &str, c: &str| [a.to_string(), b.to_string(), c.to_string()];
        HeaderFooter {
            show_header: false,
            header_first_page: false,
            header_odd_even: true,
            header: [s("", "", ""), s("", "", "")],
            show_footer: true,
            footer_first_page: true,
            footer_odd_even: true,
            footer: [s("", "$:copyright:", "$p"), s("$p", "$:copyright:", "")],
            page_number_first_page: false,
        }
    }
}

impl HeaderFooter {
    /// The three fields of the header (`footer` false) or footer on a page; None when nothing
    /// is shown there. `first` is the first page, `odd` whether its number is odd.
    pub fn fields(&self, footer: bool, first: bool, odd: bool) -> Option<&[String; 3]> {
        let (show, on_first, odd_even, rows) = if footer {
            (
                self.show_footer,
                self.footer_first_page,
                self.footer_odd_even,
                &self.footer,
            )
        } else {
            (
                self.show_header,
                self.header_first_page,
                self.header_odd_even,
                &self.header,
            )
        };
        if !show || (first && !on_first) {
            return None;
        }
        rows.get(if odd_even && !odd { 1 } else { 0 })
            .filter(|r| r.iter().any(|f| !f.trim().is_empty()))
    }

    /// Clips every field (hostile files).
    pub fn clipped(mut self) -> HeaderFooter {
        for row in self.header.iter_mut().chain(self.footer.iter_mut()) {
            for f in row.iter_mut() {
                *f = crate::clip(f);
            }
        }
        self
    }
}

/// What headers and footers can show about the document besides the score: its file, dates
/// already written out for the reader, and the part's name (empty in the full score).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocInfo {
    pub file_name: String,
    pub file_path: String,
    /// `$d`: today; `$D`: the creation date; `$M` and `$m`: the last change's date and time.
    pub today: String,
    pub created: String,
    pub modified_date: String,
    pub modified_time: String,
    /// `$i`, `$I`, `$:partName:`.
    pub part_name: String,
}

/// How dates are written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DateStyle {
    /// 2026-11-24
    #[default]
    Iso,
    /// 24.11.2026
    German,
}

impl DateStyle {
    /// A date (year, month, day) in this style.
    pub fn date(self, (y, m, d): (i64, u32, u32)) -> String {
        match self {
            DateStyle::Iso => format!("{y:04}-{m:02}-{d:02}"),
            DateStyle::German => format!("{d:02}.{m:02}.{y:04}"),
        }
    }

    /// An ISO date ("2026-11-24", maybe with a time after it) in this style; other text as it is.
    pub fn iso(self, s: &str) -> String {
        let p: Vec<&str> = s.get(..10).unwrap_or("").split('-').collect();
        match (
            p.first().and_then(|v| v.parse().ok()),
            p.get(1).and_then(|v| v.parse().ok()),
            p.get(2).and_then(|v| v.parse().ok()),
        ) {
            (Some(y), Some(m), Some(d)) if s.len() >= 10 => self.date((y, m, d)),
            _ => s.to_string(),
        }
    }
}

/// The civil date and time (UTC) of seconds since 1970: (year, month, day), (hour, minute).
pub fn civil(secs: u64) -> ((i64, u32, u32), (u32, u32)) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Days to civil date (proleptic Gregorian calendar).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    ((y, m, d), ((rem / 3600) as u32, (rem % 3600 / 60) as u32))
}

/// What a header or footer field shows on a page: its macros replaced.
pub struct PageInfo<'a> {
    pub meta: &'a crate::Meta,
    pub doc: &'a DocInfo,
    /// The printed page number and the number of pages.
    pub number: i32,
    pub pages: usize,
    pub first: bool,
    /// Page numbers are shown at all (and also on the first page).
    pub page_numbers: bool,
    pub page_number_first_page: bool,
}

/// Replaces the macros of a header or footer field.
pub fn expand_macros(field: &str, p: &PageInfo) -> String {
    let mut out = String::with_capacity(field.len());
    let mut chars = field.chars().peekable();
    let number = |all: bool| {
        if p.number >= 1 && p.page_numbers && (all || !p.first || p.page_number_first_page) {
            p.number.to_string()
        } else {
            String::new()
        }
    };
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('p') => out.push_str(&number(false)),
            Some('P') => out.push_str(&number(true)),
            Some('n' | 'N') => out.push_str(&p.pages.to_string()),
            Some('C') if p.first => out.push_str(&p.meta.copyright),
            Some('C') => {}
            Some('c') => out.push_str(&p.meta.copyright),
            Some('d') => out.push_str(&p.doc.today),
            Some('D') if !p.doc.created.is_empty() => out.push_str(&p.doc.created),
            Some('D') => out.push_str(&p.meta.created),
            Some('M') => out.push_str(&p.doc.modified_date),
            Some('m') => out.push_str(&p.doc.modified_time),
            Some('f') => out.push_str(&p.doc.file_name),
            Some('F') if !p.doc.file_path.is_empty() => out.push_str(&p.doc.file_path),
            Some('F') => out.push_str(&p.doc.file_name),
            Some('i') if !p.first => out.push_str(&p.doc.part_name),
            Some('i') => {}
            Some('I') => out.push_str(&p.doc.part_name),
            Some('$') => out.push('$'),
            Some(':') => {
                let mut tag = String::new();
                for t in chars.by_ref() {
                    if t == ':' {
                        break;
                    }
                    tag.push(t);
                }
                let m = p.meta;
                out.push_str(match tag.as_str() {
                    "workTitle" | "movementTitle" | "title" => &m.title,
                    "subtitle" | "movementNumber" | "workNumber" => &m.subtitle,
                    "composer" => &m.composer,
                    "lyricist" | "poet" => &m.lyricist,
                    "copyright" => &m.copyright,
                    "creationDate" => &m.created,
                    "partName" => &p.doc.part_name,
                    _ => "",
                });
            }
            Some(other) => {
                out.push('$');
                out.push(other);
            }
            None => out.push('$'),
        }
    }
    out
}
