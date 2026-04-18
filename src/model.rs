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
    pub align: Option<String>,        // "left" (default) | "center" | "right" — applies to all elements
    pub title_align: Option<String>,   // overrides align for the title only
    pub content_align: Option<String>, // overrides align for content text and bullets
    pub valign: Option<String>,        // "top" (default) | "middle" | "bottom"
    pub columns: Option<Vec<Column>>,
    /// When true, render bullets before columns instead of after.
    #[serde(default)]
    pub bullets_first: bool,
    pub table: Option<Table>,
}

#[derive(Debug, Deserialize)]
pub struct Column {
    pub header: Option<String>,
    pub bullets: Vec<String>,
}

/// Per-column horizontal alignment for a table cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableAlign {
    Left,
    Center,
    Right,
}

impl<'de> Deserialize<'de> for TableAlign {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "left" => Ok(TableAlign::Left),
            "center" => Ok(TableAlign::Center),
            "right" => Ok(TableAlign::Right),
            other => Err(serde::de::Error::custom(format!(
                "table align must be one of left|center|right, got {other:?}"
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
    /// Per-column alignment. Length must equal `headers.len()`. Defaults to all-left.
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
            None => vec![TableAlign::Left; n],
        };
        Ok(Table { headers: raw.headers, rows: raw.rows, aligns })
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
