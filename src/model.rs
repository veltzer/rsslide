use serde::{Deserialize, Deserializer};

#[derive(Debug, Deserialize)]
pub struct Presentation {
    pub title: Option<String>,
    #[allow(dead_code)]
    pub theme: Option<String>,
    pub paginate: Option<bool>,
    pub slides: Vec<Slide>,
}

#[derive(Debug, Deserialize)]
pub struct Slide {
    pub title: Option<String>,
    /// Markdown heading depth of the title: 1 for `#` (the default), 2 for
    /// `##`, and so on. Selects the heading style, as marp does.
    #[serde(
        default = "default_title_level",
        deserialize_with = "deserialize_level"
    )]
    pub title_level: u8,
    /// Zero or more sub-headings rendered stacked below the title rule, in
    /// declaration order. The YAML key may be `subtitle:` (one item) or
    /// `subtitles:` (list); each item is either a bare string (level 2) or
    /// `{text, level}`.
    #[serde(
        default,
        alias = "subtitle",
        deserialize_with = "deserialize_subtitles"
    )]
    pub subtitles: Vec<Subtitle>,
    pub content: Option<String>,
    pub bullets: Option<Vec<String>>,
    pub code: Option<CodeBlock>,
    /// Path to an SVG (or raster image) file on disk.
    pub image: Option<String>,
    /// Inline SVG content as a string. Takes precedence over `image` if both are set.
    pub svg: Option<String>,
    #[allow(dead_code)]
    pub class: Option<String>,
    #[allow(dead_code)]
    pub background: Option<String>,
    /// Horizontal alignment of every element. Defaults to left.
    pub align: Option<Align>,
    /// Overrides `align` for the title only.
    pub title_align: Option<Align>,
    /// Overrides `align` for subtitles, content text and bullets.
    pub content_align: Option<Align>,
    /// Vertical placement of the content block. Defaults to the configured
    /// `slide.valign` (middle, like marp).
    pub valign: Option<VAlign>,
    pub columns: Option<Vec<Column>>,
    /// When true, render bullets before columns instead of after.
    #[serde(default)]
    pub bullets_first: bool,
    pub table: Option<Table>,
}

/// Horizontal alignment of a slide element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Vertical placement of a slide's content block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}

fn default_title_level() -> u8 {
    1
}

fn deserialize_level<'de, D: Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    let level = u8::deserialize(d)?;
    if !(1..=6).contains(&level) {
        return Err(serde::de::Error::custom(format!(
            "title_level must be in 1..=6, got {level}"
        )));
    }
    Ok(level)
}

#[derive(Debug, Deserialize)]
pub struct Column {
    pub header: Option<String>,
    pub bullets: Vec<String>,
}

fn deserialize_subtitles<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Subtitle>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(Subtitle),
        Many(Vec<Subtitle>),
    }
    Ok(match OneOrMany::deserialize(d)? {
        OneOrMany::One(s) => vec![s],
        OneOrMany::Many(v) => v,
    })
}

/// A single sub-heading rendered just below the slide title.
/// `level` mirrors markdown heading depth — 2 for `##`, 3 for `###`, etc.
#[derive(Debug)]
pub struct Subtitle {
    pub text: String,
    pub level: u8,
}

impl<'de> Deserialize<'de> for Subtitle {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Plain(String),
            Full {
                text: String,
                #[serde(default = "default_subtitle_level")]
                level: u8,
            },
        }
        fn default_subtitle_level() -> u8 {
            2
        }
        let raw = Raw::deserialize(d)?;
        let (text, level) = match raw {
            Raw::Plain(s) => (s, 2u8),
            Raw::Full { text, level } => (text, level),
        };
        if !(2..=6).contains(&level) {
            return Err(serde::de::Error::custom(format!(
                "subtitle.level must be in 2..=6, got {level}"
            )));
        }
        Ok(Subtitle { text, level })
    }
}

/// Per-column horizontal alignment for a table cell. `Auto` is a GFM
/// column with no alignment colons: the header is centered (the browser
/// default for `<th>`) and body cells are left-aligned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableAlign {
    Auto,
    Left,
    Center,
    Right,
}

impl<'de> Deserialize<'de> for TableAlign {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "auto" => Ok(TableAlign::Auto),
            "left" => Ok(TableAlign::Left),
            "center" => Ok(TableAlign::Center),
            "right" => Ok(TableAlign::Right),
            other => Err(serde::de::Error::custom(format!(
                "table align must be one of auto|left|center|right, got {other:?}"
            ))),
        }
    }
}

/// A grid table with a header row, body rows, and optional per-column alignment.
/// All rows (including the header derived count) must have the same number of cells.
#[derive(Debug)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// Per-column alignment. Length must equal `headers.len()`. Defaults to all-auto.
    pub aligns: Vec<TableAlign>,
}

impl<'de> Deserialize<'de> for Table {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            headers: Vec<String>,
            rows: Vec<Vec<String>>,
            #[serde(default)]
            aligns: Option<Vec<TableAlign>>,
        }
        let raw = Raw::deserialize(d)?;
        if raw.headers.is_empty() {
            return Err(serde::de::Error::custom("table.headers must be non-empty"));
        }
        let n = raw.headers.len();
        for (i, row) in raw.rows.iter().enumerate() {
            if row.len() != n {
                return Err(serde::de::Error::custom(format!(
                    "table row {} has {} cells but headers has {}",
                    i,
                    row.len(),
                    n
                )));
            }
        }
        let aligns = match raw.aligns {
            Some(a) => {
                if a.len() != n {
                    return Err(serde::de::Error::custom(format!(
                        "table.aligns has {} entries but headers has {}",
                        a.len(),
                        n
                    )));
                }
                a
            }
            None => vec![TableAlign::Auto; n],
        };
        Ok(Table {
            headers: raw.headers,
            rows: raw.rows,
            aligns,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct CodeBlock {
    pub language: Option<String>,
    pub source: String,
    /// When true, trailing whitespace/newlines are stripped from `source`
    /// before rendering. Defaults to false — use `|-` in YAML instead.
    #[serde(default)]
    pub trim: bool,
}
