use anyhow::Result;
use serde::Serialize;

#[derive(Default, Serialize)]
pub struct OutPresentation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paginate: Option<bool>,
    pub slides: Vec<OutSlide>,
}

#[derive(Default, Serialize)]
pub struct OutSlide {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bullets: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<OutCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

#[derive(Serialize)]
pub struct OutCode {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub source: String,
}

pub fn import(input: &str) -> Result<String> {
    let pres = parse_marp(input);
    let yaml = serde_yaml::to_string(&pres)?;
    Ok(yaml)
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
    let mut fence_marker: Option<String> = None;

    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(marker) = fence_start(trimmed) {
            if !in_fence {
                in_fence = true;
                fence_marker = Some(marker);
            } else if fence_marker.as_deref() == Some(trimmed_fence(trimmed).as_str()) {
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

fn fence_start(line: &str) -> Option<String> {
    if line.starts_with("```") {
        Some("```".into())
    } else if line.starts_with("~~~") {
        Some("~~~".into())
    } else {
        None
    }
}

fn trimmed_fence(line: &str) -> String {
    if line.starts_with("```") {
        "```".into()
    } else if line.starts_with("~~~") {
        "~~~".into()
    } else {
        String::new()
    }
}

fn parse_slide(block: String) -> OutSlide {
    let mut title: Option<String> = None;
    let mut bullets: Vec<String> = Vec::new();
    let mut content_lines: Vec<String> = Vec::new();
    let mut code: Option<OutCode> = None;
    let mut image: Option<String> = None;

    let mut lines = block.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("<!--") {
            let mut buf = String::from(line);
            if !trimmed.contains("-->") {
                for inner in lines.by_ref() {
                    buf.push('\n');
                    buf.push_str(inner);
                    if inner.contains("-->") {
                        break;
                    }
                }
            }
            let _ = buf;
            continue;
        }

        if title.is_none() {
            if let Some(heading) = strip_heading(trimmed) {
                title = Some(heading.to_string());
                continue;
            }
        }

        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
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
            code = Some(OutCode { language, source });
            continue;
        }

        if let Some(bullet) = strip_bullet(trimmed) {
            bullets.push(bullet.to_string());
            continue;
        }

        if let Some(path) = parse_image(trimmed)
            && image.is_none()
        {
            image = Some(path);
            continue;
        }

        content_lines.push(trimmed.to_string());
    }

    let content = if content_lines.is_empty() {
        None
    } else {
        Some(content_lines.join(" "))
    };

    OutSlide {
        title,
        content,
        bullets: if bullets.is_empty() { None } else { Some(bullets) },
        code,
        image,
    }
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
        assert_eq!(pres.slides[0].content.as_deref(), Some("some text"));
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
        assert_eq!(s.bullets.as_ref().unwrap(), &["one", "two"]);
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
    fn yaml_output_is_valid() {
        let md = "---\ntitle: T\n---\n\n# S\n\n- a\n- b\n";
        let out = import(md).unwrap();
        assert!(out.contains("title: T"));
        assert!(out.contains("slides:"));
        assert!(out.contains("- a"));
    }
}
