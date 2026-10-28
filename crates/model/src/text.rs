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

    /// The style a new score starts with (sizes as ScoreArt always drew them).
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
            TextKind::Title => s(22.0, false, false, Center),
            TextKind::Subtitle => s(12.0, false, false, Center),
            TextKind::Composer => s(9.5, false, false, Right),
            TextKind::Lyricist => s(9.5, false, false, Left),
            TextKind::Copyright => s(8.0, false, false, Center),
            TextKind::Heading => s(14.0, true, false, Left),
            TextKind::Text => s(10.0, false, false, Left),
            TextKind::Header => s(8.0, false, true, Center),
            TextKind::Footer => s(8.0, false, true, Center),
            TextKind::PageNumber => s(8.0, false, false, Right),
            TextKind::InstrumentName => s(9.5, false, false, Right),
            TextKind::MeasureNumber => s(7.0, false, true, Left),
            TextKind::Tempo => s(10.0, true, false, Left),
            TextKind::SystemText => s(9.5, true, false, Left),
            TextKind::Rehearsal => s(11.0, true, false, Center),
            TextKind::StaffText => s(9.5, false, true, Left),
            TextKind::ChordSymbol => s(11.0, true, false, Left),
            TextKind::Lyrics => s(9.0, false, false, Center),
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
}

impl TextSettings {
    pub fn of(score: &crate::Score) -> TextSettings {
        TextSettings {
            styles: score.text_styles.clone(),
            blocks: score.blocks.clone(),
            offsets: score.block_offsets.clone(),
            hide_page_numbers: score.hide_page_numbers,
        }
    }

    pub fn apply(self, score: &mut crate::Score) {
        score.text_styles = self.styles;
        score.blocks = self.blocks;
        score.block_offsets = self.offsets;
        score.hide_page_numbers = self.hide_page_numbers;
    }
}
