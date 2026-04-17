use anyhow::Result;

#[derive(Default)]
pub struct OutPresentation {
    pub title: Option<String>,
    pub theme: Option<String>,
    pub paginate: Option<bool>,
    pub slides: Vec<OutSlide>,
}

#[derive(Default)]
pub struct OutSlide {
    pub title: Option<String>,
    pub content_lines: Vec<String>,
    pub bullets: Vec<String>,
    pub code: Option<OutCode>,
    pub image: Option<String>,
}

pub struct OutCode {
    pub language: Option<String>,
    pub source: String,
}

pub fn import(input: &str) -> Result<String> {
    let pres = parse_marp(input);
    Ok(emit_yaml(&pres))
}

fn parse_marp(input: &str) -> OutPresentation {
    let (front_matter, body) = split_front_matter(input);
    let (fm_title, fm_theme, fm_paginate) = parse_front_matter(front_matter);

    let slide_blocks = split_slides(body);
    let slides: Vec<OutSlide> = slide_blocks.into_iter().map(parse_slide).collect();

    OutPresentation {
        title: fm_title,
        theme: fm_theme,
        paginate: fm_paginate,
        slides,
    }
}

fn split_front_matter(input: &str) -> (&str, &str) {
    let trimmed = input.trim_start_matches('\u{feff}');
    let Some(rest) = trimmed
        .strip_prefix("---\n")
        .or_else(|| trimmed.strip_prefix("---\r\n"))
    else {
        return ("", trimmed);
    };
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        let stripped = line.trim_end_matches('\n').trim_end_matches('\r');
        if stripped == "---" {
            let fm = &rest[..offset];
            let body = &rest[offset + line.len()..];
            return (fm, body);
        }
        offset += line.len();
    }
    ("", trimmed)
}

fn parse_front_matter(fm: &str) -> (Option<String>, Option<String>, Option<bool>) {
    let mut title = None;
    let mut theme = None;
    let mut paginate = None;
    for line in fm.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else { continue };
        let key = key.trim();
        let value = value.trim().trim_matches('"').trim_matches('\'');
        match key {
            "title" => title = Some(value.to_string()),
            "theme" => theme = Some(value.to_string()),
            "paginate" => {
                paginate = match value {
                    "true" => Some(true),
                    "false" => Some(false),
                    _ => None,
                }
            }
            _ => {}
        }
    }
    (title, theme, paginate)
}

fn split_slides(body: &str) -> Vec<String> {
    let mut slides: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_fence = false;
    let mut fence_marker: Option<&'static str> = None;

    for line in body.lines() {
        let trimmed = line.trim_start();
        let fence = if trimmed.starts_with("```") {
            Some("```")
        } else if trimmed.starts_with("~~~") {
            Some("~~~")
        } else {
            None
        };
        if let Some(f) = fence {
            if !in_fence {
                in_fence = true;
                fence_marker = Some(f);
            } else if fence_marker == Some(f) {
                in_fence = false;
                fence_marker = None;
            }
            current.push_str(line);
            current.push('\n');
            continue;
        }

        if !in_fence && is_slide_separator(line) {
            slides.push(std::mem::take(&mut current));
            continue;
        }

        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        slides.push(current);
    }
    slides
}

fn is_slide_separator(line: &str) -> bool {
    let t = line.trim();
    t == "---" || t == "***" || t == "___"
}

fn parse_slide(block: String) -> OutSlide {
    let mut slide = OutSlide::default();
    let mut paragraph: Vec<String> = Vec::new();

    let mut lines = block.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            flush_paragraph(&mut paragraph, &mut slide.content_lines);
            continue;
        }

        if trimmed.starts_with("<!--") {
            if !trimmed.contains("-->") {
                for inner in lines.by_ref() {
                    if inner.contains("-->") {
                        break;
                    }
                }
            }
            continue;
        }

        if slide.title.is_none() {
            if let Some(heading) = strip_heading(trimmed) {
                slide.title = Some(heading.to_string());
                continue;
            }
        }

        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            flush_paragraph(&mut paragraph, &mut slide.content_lines);
            let fence = &trimmed[..3];
            let language = trimmed[3..].trim().to_string();
            let language = if language.is_empty() { None } else { Some(language) };
            let mut source = String::new();
            for inner in lines.by_ref() {
                if inner.trim_start().starts_with(fence) {
                    break;
                }
                source.push_str(inner);
                source.push('\n');
            }
            while source.ends_with('\n') {
                source.pop();
            }
            slide.code = Some(OutCode { language, source });
            continue;
        }

        if let Some(bullet) = strip_bullet(trimmed) {
            flush_paragraph(&mut paragraph, &mut slide.content_lines);
            slide.bullets.push(bullet.to_string());
            continue;
        }

        if let Some(path) = parse_image(trimmed)
            && slide.image.is_none()
        {
            flush_paragraph(&mut paragraph, &mut slide.content_lines);
            slide.image = Some(path);
            continue;
        }

        paragraph.push(trimmed.to_string());
    }
    flush_paragraph(&mut paragraph, &mut slide.content_lines);

    slide
}

fn flush_paragraph(paragraph: &mut Vec<String>, out: &mut Vec<String>) {
    if paragraph.is_empty() {
        return;
    }
    if !out.is_empty() {
        out.push(String::new());
    }
    out.extend(paragraph.drain(..));
}

fn strip_heading(line: &str) -> Option<&str> {
    let line = line.trim_start();
    let hashes = line.bytes().take_while(|b| *b == b'#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &line[hashes..];
    if !rest.starts_with(' ') {
        return None;
    }
    Some(rest.trim())
}

fn strip_bullet(line: &str) -> Option<&str> {
    let line = line.trim_start();
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return Some(rest.trim());
        }
    }
    None
}

fn parse_image(line: &str) -> Option<String> {
    let rest = line.strip_prefix("![")?;
    let close_alt = rest.find(']')?;
    let after_alt = &rest[close_alt + 1..];
    let after_paren = after_alt.strip_prefix('(')?;
    let close = after_paren.find(')')?;
    Some(after_paren[..close].to_string())
}

// --- YAML emitter ---------------------------------------------------------

fn emit_yaml(pres: &OutPresentation) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    if let Some(title) = &pres.title {
        out.push_str(&format!("title: {}\n", scalar(title)));
    }
    if let Some(theme) = &pres.theme {
        out.push_str(&format!("theme: {}\n", scalar(theme)));
    }
    if let Some(paginate) = pres.paginate {
        out.push_str(&format!("paginate: {paginate}\n"));
    }
    out.push('\n');
    out.push_str("slides:\n");
    for (i, slide) in pres.slides.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        emit_slide(&mut out, slide);
    }
    out
}

fn emit_slide(out: &mut String, slide: &OutSlide) {
    let mut first = true;
    let prefix = |first_flag: &mut bool| {
        if *first_flag {
            *first_flag = false;
            "  - "
        } else {
            "    "
        }
    };

    if let Some(title) = &slide.title {
        out.push_str(prefix(&mut first));
        out.push_str(&format!("title: {}\n", scalar(title)));
    }
    if !slide.content_lines.is_empty() {
        out.push_str(prefix(&mut first));
        emit_content(out, &slide.content_lines);
    }
    if !slide.bullets.is_empty() {
        out.push_str(prefix(&mut first));
        out.push_str("bullets:\n");
        for b in &slide.bullets {
            out.push_str("      - ");
            out.push_str(&scalar(b));
            out.push('\n');
        }
    }
    if let Some(code) = &slide.code {
        out.push_str(prefix(&mut first));
        out.push_str("code:\n");
        if let Some(lang) = &code.language {
            out.push_str("      language: ");
            out.push_str(&scalar(lang));
            out.push('\n');
        }
        out.push_str("      source: |-\n");
        for line in code.source.lines() {
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                out.push('\n');
            } else {
                out.push_str("        ");
                out.push_str(trimmed);
                out.push('\n');
            }
        }
    }
    if let Some(image) = &slide.image {
        out.push_str(prefix(&mut first));
        out.push_str("image: ");
        out.push_str(&scalar(image));
        out.push('\n');
    }
    if first {
        out.push_str("  - {}\n");
    }
}

fn emit_content(out: &mut String, lines: &[String]) {
    if lines.len() == 1 {
        let line = &lines[0];
        if needs_block_scalar(line) {
            out.push_str("content: |-\n");
            push_block_lines(out, &[line.clone()]);
        } else {
            out.push_str("content: ");
            out.push_str(&scalar(line));
            out.push('\n');
        }
        return;
    }
    out.push_str("content: |-\n");
    push_block_lines(out, lines);
}

fn push_block_lines(out: &mut String, lines: &[String]) {
    for line in lines {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            out.push('\n');
        } else {
            out.push_str("      ");
            out.push_str(trimmed);
            out.push('\n');
        }
    }
}

fn needs_block_scalar(s: &str) -> bool {
    s.len() > 70 || s.contains('\n')
}

fn scalar(s: &str) -> String {
    if s.is_empty() {
        return "''".into();
    }
    let needs_quote = s.chars().next().is_some_and(|c| {
        matches!(c, '-' | '?' | ':' | ',' | '[' | ']' | '{' | '}' | '#' | '&' |
            '*' | '!' | '|' | '>' | '\'' | '"' | '%' | '@' | '`')
    }) || s.contains(": ")
        || s.contains(" #")
        || s.ends_with(':')
        || s == "true"
        || s == "false"
        || s == "null"
        || s == "yes"
        || s == "no"
        || s.parse::<f64>().is_ok();
    if needs_quote {
        format!("'{}'", s.replace('\'', "''"))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_front_matter_and_single_slide() {
        let md = "---\ntitle: My Talk\ntheme: default\npaginate: true\n---\n\n# Hello\n\nsome text\n";
        let pres = parse_marp(md);
        assert_eq!(pres.title.as_deref(), Some("My Talk"));
        assert_eq!(pres.theme.as_deref(), Some("default"));
        assert_eq!(pres.paginate, Some(true));
        assert_eq!(pres.slides.len(), 1);
        assert_eq!(pres.slides[0].title.as_deref(), Some("Hello"));
        assert_eq!(pres.slides[0].content_lines, vec!["some text".to_string()]);
    }

    #[test]
    fn splits_multiple_slides_on_hr() {
        let md = "# A\n\ncontent A\n\n---\n\n# B\n\ncontent B\n";
        let pres = parse_marp(md);
        assert_eq!(pres.slides.len(), 2);
        assert_eq!(pres.slides[0].title.as_deref(), Some("A"));
        assert_eq!(pres.slides[1].title.as_deref(), Some("B"));
    }

    #[test]
    fn parses_bullets_and_code() {
        let md = "# Slide\n\n- one\n- two\n\n```rust\nfn main() {}\n```\n";
        let pres = parse_marp(md);
        let s = &pres.slides[0];
        assert_eq!(s.bullets, vec!["one".to_string(), "two".to_string()]);
        let code = s.code.as_ref().unwrap();
        assert_eq!(code.language.as_deref(), Some("rust"));
        assert_eq!(code.source, "fn main() {}");
    }

    #[test]
    fn hr_inside_code_fence_is_not_a_separator() {
        let md = "# A\n\n```\n---\n```\n";
        let pres = parse_marp(md);
        assert_eq!(pres.slides.len(), 1);
    }

    #[test]
    fn parses_image() {
        let md = "# Pic\n\n![alt](diagram.svg)\n";
        let pres = parse_marp(md);
        assert_eq!(pres.slides[0].image.as_deref(), Some("diagram.svg"));
    }

    #[test]
    fn yaml_output_starts_with_doc_marker() {
        let md = "---\ntitle: T\n---\n\n# S\n\n- a\n- b\n";
        let out = import(md).unwrap();
        assert!(out.starts_with("---\n"));
        assert!(out.contains("title: T"));
        assert!(out.contains("  - title: S"));
        assert!(out.contains("      - a"));
    }

    #[test]
    fn long_content_uses_block_scalar() {
        let long = "x".repeat(200);
        let md = format!("# S\n\n{long}\n");
        let out = import(&md).unwrap();
        assert!(out.contains("content: |-"));
    }
}
