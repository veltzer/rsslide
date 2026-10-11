use serde::{Deserialize, Deserializer};

#[derive(Debug, Deserialize)]
pub struct Presentation {
    pub title: Option<String>,
    #[allow(dead_code)]
    pub theme: Option<String>,
    pub paginate: Option<bool>,
    pub slides: Vec<Slide>,
}

/// One slide: its title and everything below it, in drawing order.
#[derive(Debug)]
pub struct Slide {
    pub title: Option<String>,
    /// Markdown heading depth of the title: 1 for `#` (the default), 2 for
    /// `##`, and so on. Selects the heading style, as marp does.
    pub title_level: u8,
    /// Everything below the title, top to bottom.
    pub body: Vec<Element>,
    #[allow(dead_code)]
    pub class: Option<String>,
    /// A picture filling one side of the slide, the content laid out in
    /// the other side (marp's `![bg left]`).
    pub background: Option<Background>,
    /// Horizontal alignment of every element. Defaults to left.
    pub align: Option<Align>,
    /// Overrides `align` for the title only.
    pub title_align: Option<Align>,
    /// Overrides `align` for everything below the title.
    pub content_align: Option<Align>,
    /// Vertical placement of the content block. Defaults to the configured
    /// `slide.valign` (middle, like marp).
    pub valign: Option<VAlign>,
}

/// One element of a slide's body.
#[derive(Debug)]
pub enum Element {
    /// A paragraph; each newline is a line break.
    Text(String),
    Heading(Subtitle),
    List(List),
    Code(CodeBlock),
    Table(Table),
    /// Path to an SVG file.
    Image(String),
    /// Inline SVG markup.
    Svg(String),
    Columns(Vec<Column>),
    /// A blockquote, holding its own elements.
    Quote(Vec<Element>),
}

/// A bulleted (`ordered: false`) or numbered list.
#[derive(Debug)]
pub struct List {
    pub ordered: bool,
    pub items: Vec<ListItem>,
}

#[derive(Debug)]
pub struct ListItem {
    pub text: String,
    /// A list nested under this item.
    pub children: Option<List>,
}

/// The YAML form of a slide. A slide lists its content either in `body`,
/// in source order, or in the older fixed fields (`subtitle(s)`,
/// `content`, `bullets`, `columns`, `table`, `code`, `image`, `svg`),
/// which are drawn in that order (with `bullets_first` swapping bullets and
/// columns). Mixing the two is an error.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSlide {
    title: Option<String>,
    #[serde(
        default = "default_title_level",
        deserialize_with = "deserialize_level"
    )]
    title_level: u8,
    body: Option<Vec<Element>>,
    #[serde(
        default,
        alias = "subtitle",
        deserialize_with = "deserialize_subtitles"
    )]
    subtitles: Vec<Subtitle>,
    content: Option<String>,
    bullets: Option<Vec<ListItem>>,
    #[serde(default, deserialize_with = "deserialize_code")]
    code: Vec<CodeBlock>,
    image: Option<String>,
    svg: Option<String>,
    columns: Option<Vec<Column>>,
    #[serde(default)]
    bullets_first: bool,
    table: Option<Table>,
    class: Option<String>,
    background: Option<Background>,
    align: Option<Align>,
    title_align: Option<Align>,
    content_align: Option<Align>,
    valign: Option<VAlign>,
}

impl<'de> Deserialize<'de> for Slide {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let raw = RawSlide::deserialize(d)?;
        let has_fields = !raw.subtitles.is_empty()
            || raw.content.is_some()
            || raw.bullets.is_some()
            || !raw.code.is_empty()
            || raw.image.is_some()
            || raw.svg.is_some()
            || raw.columns.is_some()
            || raw.bullets_first
            || raw.table.is_some();
        let body = match raw.body {
            Some(body) => {
                if has_fields {
                    return Err(D::Error::custom(
                        "a slide with `body` cannot also use subtitle(s), content, bullets, \
                         columns, bullets_first, table, code, image or svg",
                    ));
                }
                body
            }
            None => {
                if raw.image.is_some() && raw.svg.is_some() {
                    return Err(D::Error::custom(
                        "a slide cannot have both `image` and `svg`",
                    ));
                }
                let mut body: Vec<Element> =
                    raw.subtitles.into_iter().map(Element::Heading).collect();
                body.extend(raw.content.map(Element::Text));
                let bullets = raw.bullets.map(|items| {
                    Element::List(List {
                        ordered: false,
                        items,
                    })
                });
                let columns = raw.columns.map(Element::Columns);
                if raw.bullets_first {
                    body.extend(bullets);
                    body.extend(columns);
                } else {
                    body.extend(columns);
                    body.extend(bullets);
                }
                body.extend(raw.table.map(Element::Table));
                body.extend(raw.code.into_iter().map(Element::Code));
                body.extend(raw.image.map(Element::Image));
                body.extend(raw.svg.map(Element::Svg));
                body
            }
        };
        Ok(Slide {
            title: raw.title,
            title_level: raw.title_level,
            body,
            class: raw.class,
            background: raw.background,
            align: raw.align,
            title_align: raw.title_align,
            content_align: raw.content_align,
            valign: raw.valign,
        })
    }
}

impl<'de> Deserialize<'de> for Element {
    /// A single-key map naming the element: `text`, `heading`, `bullets`,
    /// `numbered`, `code`, `table`, `image`, `svg` or `columns`.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            text: Option<String>,
            heading: Option<Subtitle>,
            bullets: Option<Vec<ListItem>>,
            numbered: Option<Vec<ListItem>>,
            code: Option<CodeBlock>,
            table: Option<Table>,
            image: Option<String>,
            svg: Option<String>,
            columns: Option<Vec<Column>>,
            quote: Option<Vec<Element>>,
        }
        let raw = Raw::deserialize(d)?;
        let mut found: Vec<Element> = Vec::new();
        found.extend(raw.text.map(Element::Text));
        found.extend(raw.heading.map(Element::Heading));
        found.extend(raw.bullets.map(|items| {
            Element::List(List {
                ordered: false,
                items,
            })
        }));
        found.extend(raw.numbered.map(|items| {
            Element::List(List {
                ordered: true,
                items,
            })
        }));
        found.extend(raw.code.map(Element::Code));
        found.extend(raw.table.map(Element::Table));
        found.extend(raw.image.map(Element::Image));
        found.extend(raw.svg.map(Element::Svg));
        found.extend(raw.columns.map(Element::Columns));
        found.extend(raw.quote.map(Element::Quote));
        if found.len() != 1 {
            return Err(D::Error::custom(format!(
                "a body element must have exactly one of text, heading, bullets, numbered, \
                 code, table, image, svg, columns, quote; found {}",
                found.len()
            )));
        }
        if let Some(Element::List(list)) = found.first()
            && list.items.is_empty()
        {
            return Err(D::Error::custom("a list must have at least one item"));
        }
        Ok(found.remove(0))
    }
}

impl<'de> Deserialize<'de> for ListItem {
    /// A plain string, or `{text, bullets|numbered}` for an item with a
    /// nested list.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Plain(String),
            Full(Full),
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Full {
            text: String,
            bullets: Option<Vec<ListItem>>,
            numbered: Option<Vec<ListItem>>,
        }
        Ok(match Raw::deserialize(d)? {
            Raw::Plain(text) => ListItem {
                text,
                children: None,
            },
            Raw::Full(full) => {
                let children = match (full.bullets, full.numbered) {
                    (Some(_), Some(_)) => {
                        return Err(D::Error::custom(
                            "a list item can nest `bullets` or `numbered`, not both",
                        ));
                    }
                    (Some(items), None) => Some(List {
                        ordered: false,
                        items,
                    }),
                    (None, Some(items)) => Some(List {
                        ordered: true,
                        items,
                    }),
                    (None, None) => None,
                };
                if children.as_ref().is_some_and(|l| l.items.is_empty()) {
                    return Err(D::Error::custom(
                        "a nested list must have at least one item",
                    ));
                }
                ListItem {
                    text: full.text,
                    children,
                }
            }
        })
    }
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

/// A split background: a picture filling one side of the slide.
#[derive(Debug)]
pub struct Background {
    /// Path to the picture: SVG, PNG or JPEG.
    pub image: String,
    pub side: Side,
    /// The picture's share of the slide width, 0 < `split` < 1.
    pub split: f32,
    pub size: BackgroundSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
}

/// How the picture fills its side, as CSS `background-size`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BackgroundSize {
    /// Fill the side, cropping what overflows (the default).
    Cover,
    /// Fit inside the side whole.
    Contain,
    /// This share of the side's width, keeping the aspect ratio.
    Width(f32),
}

impl<'de> Deserialize<'de> for Background {
    /// `{image, split: left|right, split_size?: "N%", size?: cover|contain|"N%"}`,
    /// marp's `![bg left:N% size]` keywords.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            image: String,
            split: Side,
            split_size: Option<String>,
            size: Option<String>,
        }
        let raw = Raw::deserialize(d)?;
        let split = match &raw.split_size {
            None => 0.5,
            Some(s) => match parse_percent(s) {
                Some(p) if p > 0.0 && p < 1.0 => p,
                _ => {
                    return Err(D::Error::custom(format!(
                        "background split_size {s:?} must be a percentage between 0% and 100%"
                    )));
                }
            },
        };
        let size = match raw.size.as_deref() {
            None | Some("cover") => BackgroundSize::Cover,
            Some("contain") => BackgroundSize::Contain,
            Some(s) => match parse_percent(s) {
                Some(p) if p > 0.0 => BackgroundSize::Width(p),
                _ => {
                    return Err(D::Error::custom(format!(
                        "background size {s:?} must be cover, contain or a percentage"
                    )));
                }
            },
        };
        Ok(Background {
            image: raw.image,
            side: raw.split,
            split,
            size,
        })
    }
}

/// `"40%"` as 0.4.
pub fn parse_percent(s: &str) -> Option<f32> {
    let number = s.strip_suffix('%')?;
    if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    number.parse::<f32>().ok().map(|p| p / 100.0)
}

/// One of a slide's side-by-side columns.
#[derive(Debug)]
pub struct Column {
    /// The column's content, top to bottom.
    pub body: Vec<Element>,
}

impl<'de> Deserialize<'de> for Column {
    /// `{body}`, or the older `{header?, bullets}` (a level-3 heading over a
    /// bulleted list).
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            body: Option<Vec<Element>>,
            header: Option<String>,
            bullets: Option<Vec<ListItem>>,
        }
        let raw = Raw::deserialize(d)?;
        match (raw.body, raw.header, raw.bullets) {
            (Some(body), None, None) => Ok(Column { body }),
            (Some(_), ..) => Err(D::Error::custom(
                "a column has either `body` or `header`/`bullets`, not both",
            )),
            (None, header, bullets) => {
                let mut body = Vec::new();
                body.extend(header.map(|text| Element::Heading(Subtitle { text, level: 3 })));
                body.extend(bullets.map(|items| {
                    Element::List(List {
                        ordered: false,
                        items,
                    })
                }));
                if body.is_empty() {
                    return Err(D::Error::custom(
                        "a column needs `body`, `header` or `bullets`",
                    ));
                }
                Ok(Column { body })
            }
        }
    }
}

fn deserialize_code<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<CodeBlock>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(CodeBlock),
        Many(Vec<CodeBlock>),
    }
    let blocks = match OneOrMany::deserialize(d)? {
        OneOrMany::One(c) => vec![c],
        OneOrMany::Many(v) => v,
    };
    if blocks.is_empty() {
        return Err(serde::de::Error::custom(
            "code: an empty list; omit the field instead",
        ));
    }
    Ok(blocks)
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
#[serde(deny_unknown_fields)]
pub struct CodeBlock {
    pub language: Option<String>,
    pub source: String,
    /// When true, trailing whitespace/newlines are stripped from `source`
    /// before rendering. Defaults to false — use `|-` in YAML instead.
    #[serde(default)]
    pub trim: bool,
}
