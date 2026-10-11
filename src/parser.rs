use crate::model::Presentation;
use anyhow::Result;

pub fn parse(input: &str) -> Result<Presentation> {
    let presentation: Presentation = serde_yaml::from_str(input)?;
    Ok(presentation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Align, Element, TableAlign, VAlign};

    /// The body of the first slide of `yaml`.
    fn body(yaml: &str) -> Vec<Element> {
        parse(yaml).unwrap().slides.remove(0).body
    }

    fn kinds(body: &[Element]) -> Vec<&'static str> {
        body.iter()
            .map(|e| match e {
                Element::Text(_) => "text",
                Element::Heading(_) => "heading",
                Element::List(l) if l.ordered => "numbered",
                Element::List(_) => "bullets",
                Element::Code(_) => "code",
                Element::Table(_) => "table",
                Element::Image(_) => "image",
                Element::Svg(_) => "svg",
                Element::Columns(_) => "columns",
            })
            .collect()
    }

    #[test]
    fn parse_minimal_presentation() {
        let p = parse("slides: []").unwrap();
        assert!(p.slides.is_empty());
        assert!(p.title.is_none());
    }

    #[test]
    fn parse_title_and_paginate() {
        let p = parse("title: My Talk\npaginate: true\nslides: []").unwrap();
        assert_eq!(p.title.as_deref(), Some("My Talk"));
        assert_eq!(p.paginate, Some(true));
    }

    #[test]
    fn parse_invalid_yaml_returns_error() {
        assert!(parse("slides: [{{{{").is_err());
    }

    // ── the fixed fields ─────────────────────────────────────────────────

    #[test]
    fn fixed_fields_become_a_body_in_their_drawing_order() {
        let yaml = r#"
slides:
  - title: T
    svg: "<svg></svg>"
    code: {source: x}
    table: {headers: [A], rows: [[1]]}
    bullets: [b]
    columns: [{bullets: [c]}]
    content: words
    subtitles: [A, {text: B, level: 3}]
"#;
        let b = body(yaml);
        assert_eq!(
            kinds(&b),
            [
                "heading", "heading", "text", "columns", "bullets", "table", "code", "svg"
            ]
        );
        let Element::Heading(h) = &b[1] else {
            panic!("heading")
        };
        assert_eq!((h.text.as_str(), h.level), ("B", 3));
        let swapped = yaml.replace(
            "    content: words\n",
            "    content: words\n    bullets_first: true\n",
        );
        assert_eq!(&kinds(&body(&swapped))[3..5], ["bullets", "columns"]);
    }

    #[test]
    fn fixed_bullets_and_code_parse() {
        let b = body(
            "slides:\n  - title: Hello\n    bullets: [one, two]\n    code: {language: rust, source: 'fn main() {}'}\n",
        );
        let Element::List(l) = &b[0] else {
            panic!("bullets")
        };
        assert!(!l.ordered);
        assert_eq!(
            l.items.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(),
            ["one", "two"]
        );
        let Element::Code(code) = &b[1] else {
            panic!("code")
        };
        assert_eq!(code.language.as_deref(), Some("rust"));
        assert!(!code.trim);
    }

    #[test]
    fn code_accepts_a_list_of_blocks() {
        let b = body(
            "slides:\n  - code:\n      - {language: bash, source: echo a}\n      - {source: b}\n",
        );
        assert_eq!(kinds(&b), ["code", "code"]);
        assert!(body("slides:\n  - title: no code\n").is_empty());
    }

    #[test]
    fn code_rejects_an_empty_list_and_unknown_keys() {
        assert!(parse("slides:\n  - code: []\n").is_err());
        assert!(parse("slides:\n  - code: {langauge: rust, source: x}\n").is_err());
    }

    #[test]
    fn subtitle_shorthand_and_level_validation() {
        let b = body("slides:\n  - title: T\n    subtitle: Sub\n");
        assert!(matches!(&b[0], Element::Heading(h) if h.text == "Sub" && h.level == 2));
        assert!(parse("slides:\n  - subtitle:\n      text: x\n      level: 9\n").is_err());
    }

    #[test]
    fn image_and_svg_together_are_an_error() {
        assert!(parse("slides:\n  - image: a.svg\n    svg: '<svg/>'\n").is_err());
    }

    #[test]
    fn unknown_slide_field_is_an_error() {
        assert!(parse("slides:\n  - title: T\n    bulets: [x]\n").is_err());
    }

    // ── body ─────────────────────────────────────────────────────────────

    #[test]
    fn body_keeps_its_order() {
        let yaml = r#"
slides:
  - title: T
    body:
      - text: intro
      - code: {language: bash, source: ls}
      - numbered: [one, two]
      - heading: {text: Sub, level: 3}
      - bullets: [x]
      - table: {headers: [A], rows: [[1]]}
      - image: a.svg
"#;
        assert_eq!(
            kinds(&body(yaml)),
            [
                "text", "code", "numbered", "heading", "bullets", "table", "image"
            ]
        );
    }

    #[test]
    fn list_items_nest() {
        let yaml = r#"
slides:
  - body:
      - bullets:
          - plain
          - text: parent
            numbered:
              - child
              - text: grandchild parent
                bullets: [grandchild]
"#;
        let b = body(yaml);
        let Element::List(l) = &b[0] else {
            panic!("list")
        };
        assert!(l.items[0].children.is_none());
        let children = l.items[1].children.as_ref().unwrap();
        assert!(children.ordered);
        let grand = children.items[1].children.as_ref().unwrap();
        assert_eq!(grand.items[0].text, "grandchild");
    }

    #[test]
    fn body_rejects_mixing_with_fixed_fields() {
        let err = parse("slides:\n  - body: [{text: a}]\n    bullets: [b]\n").unwrap_err();
        assert!(format!("{err:#}").contains("cannot also use"), "{err:#}");
    }

    #[test]
    fn body_element_needs_exactly_one_known_key() {
        assert!(parse("slides:\n  - body: [{text: a, bullets: [b]}]\n").is_err());
        assert!(parse("slides:\n  - body: [{}]\n").is_err());
        assert!(parse("slides:\n  - body: [{paragraph: a}]\n").is_err());
    }

    #[test]
    fn lists_must_not_be_empty_or_doubly_nested() {
        assert!(parse("slides:\n  - body: [{bullets: []}]\n").is_err());
        assert!(
            parse("slides:\n  - body: [{bullets: [{text: a, bullets: [b], numbered: [c]}]}]\n")
                .is_err()
        );
        assert!(parse("slides:\n  - body: [{bullets: [{text: a, bullets: []}]}]\n").is_err());
        assert!(parse("slides:\n  - body: [{bullets: [{txt: a}]}]\n").is_err());
    }

    // ── alignment and title ──────────────────────────────────────────────

    #[test]
    fn parse_slide_alignment_fields() {
        let yaml = "slides:\n  - title: A\n    align: center\n    title_align: right\n    content_align: left\n    valign: middle\n";
        let s = parse(yaml).unwrap().slides.remove(0);
        assert_eq!(s.align, Some(Align::Center));
        assert_eq!(s.title_align, Some(Align::Right));
        assert_eq!(s.content_align, Some(Align::Left));
        assert_eq!(s.valign, Some(VAlign::Middle));
    }

    #[test]
    fn parse_rejects_unknown_align() {
        assert!(parse("slides:\n  - title: T\n    align: centre\n").is_err());
        assert!(parse("slides:\n  - title: T\n    valign: center\n").is_err());
    }

    #[test]
    fn parse_title_level_defaults_to_one_and_is_validated() {
        assert_eq!(
            parse("slides:\n  - title: T\n").unwrap().slides[0].title_level,
            1
        );
        assert_eq!(
            parse("slides:\n  - title: T\n    title_level: 2\n")
                .unwrap()
                .slides[0]
                .title_level,
            2
        );
        assert!(parse("slides:\n  - title: T\n    title_level: 7\n").is_err());
    }

    // ── tables ───────────────────────────────────────────────────────────

    #[test]
    fn parse_slide_with_table() {
        let b = body(
            "slides:\n  - table:\n      headers: [A, B]\n      rows:\n        - [1, 2]\n        - [3, 4]\n",
        );
        let Element::Table(t) = &b[0] else {
            panic!("table")
        };
        assert_eq!(t.headers, ["A", "B"]);
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.aligns, [TableAlign::Auto, TableAlign::Auto]);
    }

    #[test]
    fn parse_table_rejects_ragged_row_and_bad_aligns() {
        let err = parse("slides:\n  - table:\n      headers: [A, B]\n      rows:\n        - [1, 2]\n        - [3]\n").unwrap_err();
        assert!(err.to_string().contains("cells"), "{err}");
        assert!(
            parse(
                "slides:\n  - table:\n      headers: [A, B]\n      aligns: [left]\n      rows: []\n"
            )
            .is_err()
        );
    }
}
