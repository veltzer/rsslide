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
    #[allow(dead_code)]
    pub image: Option<String>,
    #[allow(dead_code)]
    pub class: Option<String>,
    #[allow(dead_code)]
    pub background: Option<String>,
    pub align: Option<String>,  // "left" (default) | "center" | "right"
    pub valign: Option<String>, // "top" (default) | "middle" | "bottom"
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
