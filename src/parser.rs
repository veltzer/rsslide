use crate::model::Presentation;
use anyhow::Result;

pub fn parse(input: &str) -> Result<Presentation> {
    let presentation: Presentation = serde_yaml::from_str(input)?;
    Ok(presentation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_presentation() {
        let yaml = "slides: []";
        let p = parse(yaml).unwrap();
        assert!(p.slides.is_empty());
        assert!(p.title.is_none());
    }

    #[test]
    fn parse_title_and_paginate() {
        let yaml = "title: My Talk\npaginate: true\nslides: []";
        let p = parse(yaml).unwrap();
        assert_eq!(p.title.as_deref(), Some("My Talk"));
        assert_eq!(p.paginate, Some(true));
    }

    #[test]
    fn parse_slide_with_bullets() {
        let yaml = r#"
slides:
  - title: Hello
    bullets:
      - one
      - two
"#;
        let p = parse(yaml).unwrap();
        assert_eq!(p.slides.len(), 1);
        let slide = &p.slides[0];
        assert_eq!(slide.title.as_deref(), Some("Hello"));
        let bullets = slide.bullets.as_ref().unwrap();
        assert_eq!(bullets, &["one", "two"]);
    }

    #[test]
    fn parse_slide_with_code_block() {
        let yaml = r#"
slides:
  - title: Code
    code:
      language: rust
      source: "fn main() {}"
"#;
        let p = parse(yaml).unwrap();
        let code = p.slides[0].code.as_ref().unwrap();
        assert_eq!(code.language.as_deref(), Some("rust"));
        assert_eq!(code.source, "fn main() {}");
        assert!(!code.trim); // default false
    }

    #[test]
    fn parse_slide_alignment_fields() {
        let yaml = r#"
slides:
  - title: Aligned
    align: center
    title_align: right
    content_align: left
    valign: middle
"#;
        let p = parse(yaml).unwrap();
        let s = &p.slides[0];
        assert_eq!(s.align.as_deref(), Some("center"));
        assert_eq!(s.title_align.as_deref(), Some("right"));
        assert_eq!(s.content_align.as_deref(), Some("left"));
        assert_eq!(s.valign.as_deref(), Some("middle"));
    }

    #[test]
    fn parse_slide_inline_svg() {
        let yaml = r#"
slides:
  - title: Diagram
    svg: "<svg></svg>"
"#;
        let p = parse(yaml).unwrap();
        assert_eq!(p.slides[0].svg.as_deref(), Some("<svg></svg>"));
    }

    #[test]
    fn parse_invalid_yaml_returns_error() {
        let yaml = "slides: [{{{{";
        assert!(parse(yaml).is_err());
    }
}
