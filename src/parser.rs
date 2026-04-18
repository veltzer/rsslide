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
    fn parse_slide_subtitle_shorthand() {
        let yaml = "slides:\n  - title: T\n    subtitle: Sub\n";
        let p = parse(yaml).unwrap();
        let subs = &p.slides[0].subtitles;
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].text, "Sub");
        assert_eq!(subs[0].level, 2);
    }

    #[test]
    fn parse_slide_subtitle_with_level() {
        let yaml = "slides:\n  - title: T\n    subtitle:\n      text: Sub\n      level: 4\n";
        let p = parse(yaml).unwrap();
        assert_eq!(p.slides[0].subtitles[0].level, 4);
    }

    #[test]
    fn parse_slide_subtitles_list() {
        let yaml = "slides:\n  - title: T\n    subtitles:\n      - A\n      - {text: B, level: 3}\n";
        let p = parse(yaml).unwrap();
        let subs = &p.slides[0].subtitles;
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0].text, "A");
        assert_eq!(subs[0].level, 2);
        assert_eq!(subs[1].text, "B");
        assert_eq!(subs[1].level, 3);
    }

    #[test]
    fn parse_slide_subtitle_rejects_bad_level() {
        let yaml = "slides:\n  - subtitle:\n      text: x\n      level: 9\n";
        assert!(parse(yaml).is_err());
    }

    #[test]
    fn parse_slide_with_table() {
        let yaml = r#"
slides:
  - title: T
    table:
      headers: [A, B]
      rows:
        - [1, 2]
        - [3, 4]
"#;
        let p = parse(yaml).unwrap();
        let t = p.slides[0].table.as_ref().unwrap();
        assert_eq!(t.headers, vec!["A".to_string(), "B".to_string()]);
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.aligns.len(), 2); // defaults to all-left
    }

    #[test]
    fn parse_table_rejects_ragged_row() {
        let yaml = r#"
slides:
  - table:
      headers: [A, B]
      rows:
        - [1, 2]
        - [3]
"#;
        let err = parse(yaml).unwrap_err();
        assert!(err.to_string().contains("cells"), "{err}");
    }

    #[test]
    fn parse_table_rejects_aligns_length_mismatch() {
        let yaml = r#"
slides:
  - table:
      headers: [A, B]
      aligns: [left]
      rows: []
"#;
        assert!(parse(yaml).is_err());
    }

    #[test]
    fn parse_invalid_yaml_returns_error() {
        let yaml = "slides: [{{{{";
        assert!(parse(yaml).is_err());
    }
}
