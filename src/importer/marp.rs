use anyhow::{Result, anyhow};

#[derive(Debug, Default)]
pub struct OutPresentation {
    pub title: Option<String>,
    pub theme: Option<String>,
    pub paginate: Option<bool>,
    pub slides: Vec<OutSlide>,
}

#[derive(Debug, Default)]
pub struct OutSlide {
    pub title: Option<String>,
    /// (text, level) — set from the first non-title `##..######` heading.
    pub subtitle: Option<(String, u8)>,
    pub content_lines: Vec<String>,
    pub bullets: Vec<String>,
    pub code: Option<OutCode>,
    pub image: Option<String>,
    pub table: Option<OutTable>,
}

#[derive(Debug)]
pub struct OutCode {
    pub language: Option<String>,
    pub source: String,
}

#[derive(Debug)]
pub struct OutTable {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// One per column: "left" | "center" | "right".
    pub aligns: Vec<&'static str>,
}

pub fn import(input: &str) -> Result<String> {
    let pres = parse_marp(input)?;
    Ok(emit_yaml(&pres))
}

fn parse_marp(input: &str) -> Result<OutPresentation> {
    let (front_matter, body) = split_front_matter(input);
    let (fm_title, fm_theme, fm_paginate) = parse_front_matter(front_matter);

    let slide_blocks = split_slides(body);
    let mut slides: Vec<OutSlide> = Vec::with_capacity(slide_blocks.len());
    for (i, block) in slide_blocks.into_iter().enumerate() {
        slides.push(parse_slide(block).map_err(|e| anyhow!("slide {}: {e}", i + 1))?);
    }

    Ok(OutPresentation {
        title: fm_title,
        theme: fm_theme,
        paginate: fm_paginate,
        slides,
    })
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

fn parse_slide(block: String) -> Result<OutSlide> {
    let mut slide = OutSlide::default();
    let mut paragraph: Vec<String> = Vec::new();

    let lines: Vec<&str> = block.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        if trimmed.is_empty() {
            flush_paragraph(&mut paragraph, &mut slide.content_lines);
            i += 1;
            continue;
        }

        if trimmed.starts_with("<!--") {
            if !trimmed.contains("-->") {
                i += 1;
                while i < lines.len() && !lines[i].contains("-->") {
                    i += 1;
                }
            }
            i += 1;
            continue;
        }

        if let Some((level, heading)) = parse_heading(trimmed) {
            if slide.title.is_none() {
                slide.title = Some(heading.to_string());
            } else if slide.subtitle.is_none() {
                if level < 2 {
                    return Err(anyhow!(
                        "second `# ` heading {:?} on a slide that already has a title — only one title is allowed",
                        heading
                    ));
                }
                slide.subtitle = Some((heading.to_string(), level));
            } else {
                return Err(anyhow!(
                    "more than one sub-heading on a slide; second is {:?}",
                    heading
                ));
            }
            i += 1;
            continue;
        }

        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            flush_paragraph(&mut paragraph, &mut slide.content_lines);
            let fence = &trimmed[..3];
            let language = trimmed[3..].trim().to_string();
            let language = if language.is_empty() { None } else { Some(language) };
            let mut source = String::new();
            i += 1;
            while i < lines.len() {
                let inner = lines[i];
                i += 1;
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

        // GFM table: header row | sep row (with --- and optional :) | body rows.
        if is_table_row(trimmed)
            && i + 1 < lines.len()
            && let Some(aligns) = parse_table_separator(lines[i + 1].trim())
        {
            flush_paragraph(&mut paragraph, &mut slide.content_lines);
            let headers = split_table_row(trimmed);
            if headers.len() == aligns.len() && slide.table.is_none() {
                let mut rows: Vec<Vec<String>> = Vec::new();
                let mut j = i + 2;
                while j < lines.len() {
                    let r = lines[j].trim();
                    if !is_table_row(r) {
                        break;
                    }
                    let cells = split_table_row(r);
                    if cells.len() != headers.len() {
                        break;
                    }
                    rows.push(cells);
                    j += 1;
                }
                slide.table = Some(OutTable { headers, rows, aligns });
                i = j;
                continue;
            }
        }

        if let Some(bullet) = strip_bullet(trimmed) {
            flush_paragraph(&mut paragraph, &mut slide.content_lines);
            slide.bullets.push(bullet.to_string());
            i += 1;
            continue;
        }

        if let Some(path) = parse_image(trimmed)
            && slide.image.is_none()
        {
            flush_paragraph(&mut paragraph, &mut slide.content_lines);
            slide.image = Some(path);
            i += 1;
            continue;
        }

        paragraph.push(trimmed.to_string());
        i += 1;
    }
    flush_paragraph(&mut paragraph, &mut slide.content_lines);

    Ok(slide)
}

fn is_table_row(line: &str) -> bool {
    line.starts_with('|') && line.ends_with('|') && line.len() >= 2
}

/// Parse a GFM table separator row (`|---|:--:|--:|`). Returns per-column
/// alignment strings, or None if the row isn't a valid separator.
fn parse_table_separator(line: &str) -> Option<Vec<&'static str>> {
    if !is_table_row(line) {
        return None;
    }
    let inner = &line[1..line.len() - 1];
    let mut aligns: Vec<&'static str> = Vec::new();
    for cell in inner.split('|') {
        let c = cell.trim();
        if c.is_empty() {
            return None;
        }
        let left = c.starts_with(':');
        let right = c.ends_with(':');
        let body = c.trim_matches(':');
        if body.is_empty() || !body.chars().all(|ch| ch == '-') {
            return None;
        }
        let align = match (left, right) {
            (true, true) => "center",
            (false, true) => "right",
            _ => "left",
        };
        aligns.push(align);
    }
    if aligns.is_empty() { None } else { Some(aligns) }
}

fn split_table_row(line: &str) -> Vec<String> {
    let inner = &line[1..line.len() - 1];
    inner.split('|').map(|c| c.trim().to_string()).collect()
}

fn flush_paragraph(paragraph: &mut Vec<String>, out: &mut Vec<String>) {
    if paragraph.is_empty() {
        return;
    }
    if !out.is_empty() {
        out.push(String::new());
    }
    out.append(paragraph);
}

/// Returns `(level, text)` for a markdown ATX heading, or None.
fn parse_heading(line: &str) -> Option<(u8, &str)> {
    let line = line.trim_start();
    let hashes = line.bytes().take_while(|b| *b == b'#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &line[hashes..];
    if !rest.starts_with(' ') {
        return None;
    }
    Some((hashes as u8, rest.trim()))
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
    if let Some((text, level)) = &slide.subtitle {
        out.push_str(prefix(&mut first));
        if *level == 2 {
            out.push_str(&format!("subtitle: {}\n", scalar(text)));
        } else {
            out.push_str("subtitle:\n");
            out.push_str(&format!("      text: {}\n", scalar(text)));
            out.push_str(&format!("      level: {}\n", level));
        }
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
    if let Some(table) = &slide.table {
        out.push_str(prefix(&mut first));
        out.push_str("table:\n");
        out.push_str("      headers:\n");
        for h in &table.headers {
            out.push_str("        - ");
            out.push_str(&scalar(h));
            out.push('\n');
        }
        if table.aligns.iter().any(|a| *a != "left") {
            out.push_str("      aligns:\n");
            for a in &table.aligns {
                out.push_str("        - ");
                out.push_str(a);
                out.push('\n');
            }
        }
        out.push_str("      rows:\n");
        for row in &table.rows {
            out.push_str("        -\n");
            for cell in row {
                out.push_str("          - ");
                out.push_str(&scalar(cell));
                out.push('\n');
            }
        }
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
            push_block_lines(out, std::slice::from_ref(line));
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
        let pres = parse_marp(md).unwrap();
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
        let pres = parse_marp(md).unwrap();
        assert_eq!(pres.slides.len(), 2);
        assert_eq!(pres.slides[0].title.as_deref(), Some("A"));
        assert_eq!(pres.slides[1].title.as_deref(), Some("B"));
    }

    #[test]
    fn parses_bullets_and_code() {
        let md = "# Slide\n\n- one\n- two\n\n```rust\nfn main() {}\n```\n";
        let pres = parse_marp(md).unwrap();
        let s = &pres.slides[0];
        assert_eq!(s.bullets, vec!["one".to_string(), "two".to_string()]);
        let code = s.code.as_ref().unwrap();
        assert_eq!(code.language.as_deref(), Some("rust"));
        assert_eq!(code.source, "fn main() {}");
    }

    #[test]
    fn hr_inside_code_fence_is_not_a_separator() {
        let md = "# A\n\n```\n---\n```\n";
        let pres = parse_marp(md).unwrap();
        assert_eq!(pres.slides.len(), 1);
    }

    #[test]
    fn parses_image() {
        let md = "# Pic\n\n![alt](diagram.svg)\n";
        let pres = parse_marp(md).unwrap();
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
    fn captures_subheading_as_subtitle() {
        let md = "## Title\n### Sub\n\nbody\n";
        let pres = parse_marp(md).unwrap();
        let s = &pres.slides[0];
        assert_eq!(s.title.as_deref(), Some("Title"));
        let (text, level) = s.subtitle.as_ref().unwrap();
        assert_eq!(text, "Sub");
        assert_eq!(*level, 3);
        // The subheading must not also leak into content.
        for line in &s.content_lines {
            assert!(!line.starts_with('#'), "heading leaked: {line}");
        }
    }

    #[test]
    fn second_subheading_is_an_error() {
        let md = "# T\n\n## A\n\n## B\n";
        let err = parse_marp(md).unwrap_err();
        assert!(err.to_string().contains("more than one"), "{err}");
    }

    #[test]
    fn yaml_emits_subtitle_shorthand_for_level_2() {
        let md = "# T\n\n## Sub\n";
        let out = import(md).unwrap();
        assert!(out.contains("subtitle: Sub"), "{out}");
    }

    #[test]
    fn yaml_emits_subtitle_block_for_higher_level() {
        let md = "# T\n\n#### Sub\n";
        let out = import(md).unwrap();
        assert!(out.contains("subtitle:\n      text: Sub\n      level: 4"), "{out}");
    }

    #[test]
    fn parses_simple_table() {
        let md = "# T\n\n| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n";
        let pres = parse_marp(md).unwrap();
        let t = pres.slides[0].table.as_ref().expect("table");
        assert_eq!(t.headers, vec!["A".to_string(), "B".to_string()]);
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.rows[0], vec!["1".to_string(), "2".to_string()]);
        assert_eq!(t.aligns, vec!["left", "left"]);
    }

    #[test]
    fn parses_table_with_alignment() {
        let md = "# T\n\n| A | B | C |\n|:--|:-:|--:|\n| 1 | 2 | 3 |\n";
        let pres = parse_marp(md).unwrap();
        let t = pres.slides[0].table.as_ref().unwrap();
        assert_eq!(t.aligns, vec!["left", "center", "right"]);
    }

    #[test]
    fn table_does_not_leak_into_content() {
        let md = "# T\n\n| A | B |\n|---|---|\n| 1 | 2 |\n";
        let pres = parse_marp(md).unwrap();
        // The cell text must not have been pushed into content_lines.
        for line in &pres.slides[0].content_lines {
            assert!(!line.contains('|'), "table row leaked: {line}");
        }
    }

    #[test]
    fn yaml_emits_table_block() {
        let md = "# T\n\n| A | B |\n|:-:|--:|\n| 1 | 2 |\n";
        let out = import(md).unwrap();
        assert!(out.contains("table:"), "{out}");
        assert!(out.contains("headers:"));
        assert!(out.contains("aligns:"));
        assert!(out.contains("- center"));
        assert!(out.contains("- right"));
    }

    #[test]
    fn long_content_uses_block_scalar() {
        let long = "x".repeat(200);
        let md = format!("# S\n\n{long}\n");
        let out = import(&md).unwrap();
        assert!(out.contains("content: |-"));
    }
}
