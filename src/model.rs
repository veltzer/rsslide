use serde::Deserialize;

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
