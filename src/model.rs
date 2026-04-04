use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Presentation {
    pub title: Option<String>,
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
    pub image: Option<String>,
    pub class: Option<String>,
    pub background: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CodeBlock {
    pub language: Option<String>,
    pub source: String,
}
