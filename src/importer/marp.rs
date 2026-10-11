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
    /// Heading depth of the title (`#` = 1, `##` = 2, ...).
    pub title_level: u8,
    /// One per non-title `##..######` heading, in source order.
    pub subtitles: Vec<(String, u8)>,
    pub content_lines: Vec<String>,
    pub bullets: Vec<String>,
    /// Every fenced code block, in source order.
    pub code: Vec<OutCode>,
    pub image: Option<String>,
    /// Inline `<svg>...</svg>` markup.
    pub svg: Option<String>,
    /// From a `<div class="columns">` block: one per `### header`.
    pub columns: Vec<OutColumn>,
    /// True when the slide's bullets come before its columns.
    pub bullets_first: bool,
    pub table: Option<OutTable>,
}

#[derive(Debug, Default)]
pub struct OutColumn {
    pub header: Option<String>,
    pub bullets: Vec<String>,
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
    /// One per column: "auto" | "left" | "center" | "right".
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
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
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

        // Raw HTML. marp decks use three idioms, each mapped onto the
        // model; anything else is rejected rather than leaked as text.
        if trimmed.starts_with('<') {
            flush_paragraph(&mut paragraph, &mut slide.content_lines);
            if trimmed.starts_with("<style") {
                // The stylesheet only defines the `.columns` grid, which
                // the `columns` field below replaces; rsslide applies no
                // custom CSS.
                i = skip_past(&lines, i, "</style>")?;
            } else if trimmed.starts_with("<svg") {
                if slide.svg.is_some() || slide.image.is_some() {
                    return Err(anyhow!("more than one image or <svg> on a slide"));
                }
                let end = skip_past(&lines, i, "</svg>")?;
                slide.svg = Some(lines[i..end].join("\n"));
                i = end;
            } else if trimmed == "<div class=\"columns\">" {
                if !slide.columns.is_empty() {
                    return Err(anyhow!("more than one <div class=\"columns\"> on a slide"));
                }
                let end = skip_past_div(&lines, i)?;
                slide.columns = parse_columns(&lines[i + 1..end - 1])?;
                slide.bullets_first = !slide.bullets.is_empty();
                i = end;
            } else {
                return Err(anyhow!("unsupported raw HTML: {trimmed:?}"));
            }
            continue;
        }

        if let Some((level, heading)) = parse_heading(trimmed) {
            if slide.title.is_none() {
                slide.title = Some(heading.to_string());
                slide.title_level = level;
            } else if level >= 2 {
                slide.subtitles.push((heading.to_string(), level));
            } else {
                return Err(anyhow!(
                    "second `# ` heading {:?} on a slide that already has a title — only one title is allowed",
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
            let language = if language.is_empty() {
                None
            } else {
                Some(language)
            };
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
            slide.code.push(OutCode { language, source });
            continue;
        }

        // GFM table: header row | sep row (with --- and optional :) | body rows.
        if is_table_row(trimmed)
            && i + 1 < lines.len()
            && let Some(aligns) = parse_table_separator(lines[i + 1].trim())
        {
            flush_paragraph(&mut paragraph, &mut slide.content_lines);
            let headers = split_table_row(trimmed);
            if headers.len() != aligns.len() {
                return Err(anyhow!(
                    "table header {trimmed:?} has {} cells but its separator row has {}",
                    headers.len(),
                    aligns.len()
                ));
            }
            if slide.table.is_none() {
                let mut rows: Vec<Vec<String>> = Vec::new();
                let mut j = i + 2;
                while j < lines.len() {
                    let r = lines[j].trim();
                    if !is_table_row(r) {
                        break;
                    }
                    let cells = split_table_row(r);
                    if cells.len() != headers.len() {
                        return Err(anyhow!(
                            "table row {r:?} has {} cells but the header has {}",
                            cells.len(),
                            headers.len()
                        ));
                    }
                    rows.push(cells);
                    j += 1;
                }
                slide.table = Some(OutTable {
                    headers,
                    rows,
                    aligns,
                });
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

/// Index of the line after the first line (from `start`) containing
/// `closing`.
fn skip_past(lines: &[&str], start: usize, closing: &str) -> Result<usize> {
    (start..lines.len())
        .find(|&j| lines[j].contains(closing))
        .map(|j| j + 1)
        .ok_or_else(|| anyhow!("{:?} is never closed by {closing}", lines[start].trim()))
}

/// Index of the line after the `</div>` that closes the `<div` on line
/// `start`, counting nested divs.
fn skip_past_div(lines: &[&str], start: usize) -> Result<usize> {
    let mut depth = 0usize;
    for (j, line) in lines.iter().enumerate().skip(start) {
        depth += line.matches("<div").count();
        let closes = line.matches("</div>").count();
        if closes > depth {
            return Err(anyhow!("unbalanced </div> on line {:?}", line.trim()));
        }
        depth -= closes;
        if depth == 0 {
            return Ok(j + 1);
        }
    }
    Err(anyhow!(
        "{:?} is never closed by </div>",
        lines[start].trim()
    ))
}

/// The inside of a `<div class="columns">`. Each column is either a
/// `### header` line followed by bullets, or a bare `<div>` ... `</div>`
/// holding bullets (a column without a header).
fn parse_columns(lines: &[&str]) -> Result<Vec<OutColumn>> {
    let mut columns: Vec<OutColumn> = Vec::new();
    let mut in_bare_div = false;
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "<div>" && !in_bare_div {
            in_bare_div = true;
            columns.push(OutColumn::default());
        } else if trimmed == "</div>" && in_bare_div {
            in_bare_div = false;
        } else if let Some((_, heading)) = parse_heading(trimmed)
            && !in_bare_div
        {
            columns.push(OutColumn {
                header: Some(heading.to_string()),
                bullets: Vec::new(),
            });
        } else if let Some(bullet) = strip_bullet(trimmed) {
            let column = columns
                .last_mut()
                .ok_or_else(|| anyhow!("bullet {bullet:?} before the first column heading"))?;
            column.bullets.push(bullet.to_string());
        } else {
            return Err(anyhow!(
                "unsupported line inside <div class=\"columns\">: {trimmed:?}"
            ));
        }
    }
    if in_bare_div {
        return Err(anyhow!("unclosed <div> inside <div class=\"columns\">"));
    }
    if columns.is_empty() {
        return Err(anyhow!("<div class=\"columns\"> has no columns"));
    }
    Ok(columns)
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
    let mut aligns: Vec<&'static str> = Vec::new();
    for cell in split_table_row(line) {
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
            (true, false) => "left",
            (false, false) => "auto",
        };
        aligns.push(align);
    }
    if aligns.is_empty() {
        None
    } else {
        Some(aligns)
    }
}

/// The cells of a `| a | b |` row, trimmed. A `\|` is a literal pipe
/// inside a cell (GFM), not a cell boundary.
fn split_table_row(line: &str) -> Vec<String> {
    let inner = &line[1..line.len() - 1];
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                cell.push('|');
                chars.next();
            }
            '|' => cells.push(std::mem::take(&mut cell).trim().to_string()),
            _ => cell.push(c),
        }
    }
    cells.push(cell.trim().to_string());
    cells
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
        if slide.title_level != 1 {
            out.push_str(&format!("    title_level: {}\n", slide.title_level));
        }
    }
    if !slide.subtitles.is_empty() {
        out.push_str(prefix(&mut first));
        if slide.subtitles.len() == 1 {
            let (text, level) = &slide.subtitles[0];
            if *level == 2 {
                out.push_str(&format!("subtitle: {}\n", scalar(text)));
            } else {
                out.push_str("subtitle:\n");
                out.push_str(&format!("      text: {}\n", scalar(text)));
                out.push_str(&format!("      level: {}\n", level));
            }
        } else {
            out.push_str("subtitles:\n");
            for (text, level) in &slide.subtitles {
                if *level == 2 {
                    out.push_str(&format!("      - {}\n", scalar(text)));
                } else {
                    out.push_str("      -\n");
                    out.push_str(&format!("        text: {}\n", scalar(text)));
                    out.push_str(&format!("        level: {}\n", level));
                }
            }
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
    match slide.code.as_slice() {
        [] => {}
        [code] => {
            out.push_str(prefix(&mut first));
            out.push_str("code:\n");
            emit_code(out, code, "      ", "      ");
        }
        blocks => {
            out.push_str(prefix(&mut first));
            out.push_str("code:\n");
            for code in blocks {
                emit_code(out, code, "      - ", "        ");
            }
        }
    }
    if let Some(image) = &slide.image {
        out.push_str(prefix(&mut first));
        out.push_str("image: ");
        out.push_str(&scalar(image));
        out.push('\n');
    }
    if let Some(svg) = &slide.svg {
        out.push_str(prefix(&mut first));
        out.push_str(&format!("svg: {}\n", block_header(svg)));
        push_block_lines(out, &svg.lines().map(str::to_string).collect::<Vec<_>>());
    }
    if !slide.columns.is_empty() {
        out.push_str(prefix(&mut first));
        out.push_str("columns:\n");
        for col in &slide.columns {
            match &col.header {
                Some(h) => out.push_str(&format!(
                    "      - header: {}\n        bullets:\n",
                    scalar(h)
                )),
                None => out.push_str("      - bullets:\n"),
            }
            for b in &col.bullets {
                out.push_str(&format!("          - {}\n", scalar(b)));
            }
        }
        if slide.bullets_first {
            out.push_str("    bullets_first: true\n");
        }
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
        if table.aligns.iter().any(|a| *a != "auto") {
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

/// One code block's fields. `first` prefixes its first key (it carries the
/// `- ` of a list item); `indent` prefixes the others.
fn emit_code(out: &mut String, code: &OutCode, first: &str, indent: &str) {
    let mut lead = first;
    if let Some(lang) = &code.language {
        out.push_str(lead);
        out.push_str("language: ");
        out.push_str(&scalar(lang));
        out.push('\n');
        lead = indent;
    }
    out.push_str(lead);
    out.push_str(&format!("source: {}\n", block_header(&code.source)));
    for line in code.source.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            out.push('\n');
        } else {
            out.push_str(indent);
            out.push_str("  ");
            out.push_str(trimmed);
            out.push('\n');
        }
    }
}

/// The header of a stripped literal block scalar holding `text`. YAML
/// takes a block's indentation from its first non-empty line, so when that
/// line starts with whitespace (a code snippet cut from inside a function)
/// a less-indented later line would end the block early. An explicit
/// indentation indicator avoids that; every block this emitter writes is
/// indented two columns past its key.
fn block_header(text: &str) -> &'static str {
    let first = text.lines().find(|l| !l.trim().is_empty());
    if first.is_some_and(|l| l.starts_with([' ', '\t'])) {
        "|2-"
    } else {
        "|-"
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

/// Plain scalars that YAML (1.1, as yamllint reads it) takes for a bool or
/// null rather than a string, in any case.
fn is_yaml_keyword(s: &str) -> bool {
    const WORDS: [&str; 12] = [
        "true", "false", "yes", "no", "on", "off", "y", "n", "null", "~", "none", "nil",
    ];
    WORDS.iter().any(|w| s.eq_ignore_ascii_case(w))
}

fn scalar(s: &str) -> String {
    if s.is_empty() {
        return "''".into();
    }
    let needs_quote = s.chars().next().is_some_and(|c| {
        matches!(
            c,
            '-' | '?'
                | ':'
                | ','
                | '['
                | ']'
                | '{'
                | '}'
                | '#'
                | '&'
                | '*'
                | '!'
                | '|'
                | '>'
                | '\''
                | '"'
                | '%'
                | '@'
                | '`'
        )
    }) || s.contains(": ")
        || s.contains(" #")
        || s.ends_with(':')
        || is_yaml_keyword(s)
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
        let md =
            "---\ntitle: My Talk\ntheme: default\npaginate: true\n---\n\n# Hello\n\nsome text\n";
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
        assert_eq!(s.code.len(), 1);
        let code = &s.code[0];
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
        assert_eq!(s.subtitles.len(), 1);
        let (text, level) = &s.subtitles[0];
        assert_eq!(text, "Sub");
        assert_eq!(*level, 3);
        for line in &s.content_lines {
            assert!(!line.starts_with('#'), "heading leaked: {line}");
        }
    }

    #[test]
    fn captures_multiple_subheadings_in_order() {
        let md = "# T\n## Author\n## email@x\n## Affiliation\n";
        let pres = parse_marp(md).unwrap();
        let subs = &pres.slides[0].subtitles;
        assert_eq!(subs.len(), 3);
        assert_eq!(subs[0].0, "Author");
        assert_eq!(subs[1].0, "email@x");
        assert_eq!(subs[2].0, "Affiliation");
    }

    #[test]
    fn second_top_level_heading_is_an_error() {
        let md = "# T\n\n# AlsoTop\n";
        let err = parse_marp(md).unwrap_err();
        assert!(err.to_string().contains("only one title"), "{err}");
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
        assert!(
            out.contains("subtitle:\n      text: Sub\n      level: 4"),
            "{out}"
        );
    }

    #[test]
    fn yaml_emits_subtitles_list_for_multiple() {
        let md = "# T\n## A\n## B\n";
        let out = import(md).unwrap();
        assert!(out.contains("subtitles:"), "{out}");
        assert!(out.contains("- A"));
        assert!(out.contains("- B"));
    }

    #[test]
    fn parses_simple_table() {
        let md = "# T\n\n| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n";
        let pres = parse_marp(md).unwrap();
        let t = pres.slides[0].table.as_ref().expect("table");
        assert_eq!(t.headers, vec!["A".to_string(), "B".to_string()]);
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.rows[0], vec!["1".to_string(), "2".to_string()]);
        assert_eq!(t.aligns, vec!["auto", "auto"]);
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
    fn yaml_emits_title_level_only_when_not_one() {
        let out = import("## Sub-level title\n").unwrap();
        assert!(out.contains("title_level: 2"), "{out}");
        let out = import("# Top-level title\n").unwrap();
        assert!(!out.contains("title_level"), "{out}");
    }

    #[test]
    fn escaped_pipe_stays_inside_its_cell() {
        let md = "# T\n\n| Problem | Command |\n|---|---|\n| gateway? | `ip route \\| grep default` |\n| ip? | `ip addr` |\n";
        let pres = parse_marp(md).unwrap();
        let t = pres.slides[0].table.as_ref().unwrap();
        assert_eq!(t.rows.len(), 2, "both rows stay in the table");
        assert_eq!(t.rows[0][1], "`ip route | grep default`");
        assert!(pres.slides[0].content_lines.is_empty());
    }

    #[test]
    fn ragged_table_row_is_an_error() {
        let md = "# T\n\n| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 | 5 |\n";
        let err = parse_marp(md).unwrap_err().to_string();
        assert!(err.contains("has 3 cells but the header has 2"), "{err}");
    }

    #[test]
    fn every_code_block_is_kept_in_order() {
        let md = "# T\n\nfirst:\n\n```bash\necho one\n```\n\nsecond:\n\n```\nplain two\n```\n";
        let pres = parse_marp(md).unwrap();
        let code = &pres.slides[0].code;
        assert_eq!(code.len(), 2);
        assert_eq!(code[0].source, "echo one");
        assert_eq!(code[1].source, "plain two");
        assert!(code[1].language.is_none());

        let yaml = import(md).unwrap();
        assert!(
            yaml.contains(
                "    code:\n      - language: bash\n        source: |-\n          echo one\n"
            ),
            "{yaml}"
        );
        assert!(
            yaml.contains("      - source: |-\n          plain two\n"),
            "{yaml}"
        );
        let parsed = crate::parser::parse(&yaml).expect("emitted YAML parses");
        let sources: Vec<&str> = parsed.slides[0]
            .code
            .iter()
            .map(|c| c.source.as_str())
            .collect();
        assert_eq!(sources, ["echo one", "plain two"]);
    }

    #[test]
    fn single_code_block_keeps_the_mapping_form() {
        let yaml = import("# T\n\n```rust\nfn main() {}\n```\n").unwrap();
        assert!(
            yaml.contains(
                "    code:\n      language: rust\n      source: |-\n        fn main() {}\n"
            ),
            "{yaml}"
        );
    }

    #[test]
    fn code_whose_first_line_is_indented_round_trips() {
        // A snippet cut from the middle of a function: the first line is
        // indented deeper than a later one.
        let md = "# T\n\n```python\n        x = 1\n    return x\n```\n";
        let yaml = import(md).unwrap();
        let parsed = crate::parser::parse(&yaml).unwrap_or_else(|e| panic!("{e}\n{yaml}"));
        assert_eq!(
            parsed.slides[0].code[0].source,
            "        x = 1\n    return x"
        );
        // The same inside a list of blocks.
        let md = "# T\n\n```\n  a\nb\n```\n\n```\n    c\nd\n```\n";
        let yaml = import(md).unwrap();
        let parsed = crate::parser::parse(&yaml).unwrap_or_else(|e| panic!("{e}\n{yaml}"));
        let sources: Vec<&str> = parsed.slides[0]
            .code
            .iter()
            .map(|c| c.source.as_str())
            .collect();
        assert_eq!(sources, ["  a\nb", "    c\nd"]);
    }

    #[test]
    fn indented_svg_round_trips() {
        let md = "# T\n\n<svg viewBox=\"0 0 1 1\">\n    <rect/>\n</svg>\n";
        let md = md.replace("<svg", "  <svg");
        let yaml = import(&md).unwrap();
        let parsed = crate::parser::parse(&yaml).unwrap_or_else(|e| panic!("{e}\n{yaml}"));
        assert!(
            parsed.slides[0]
                .svg
                .as_deref()
                .unwrap()
                .starts_with("  <svg")
        );
    }

    #[test]
    fn style_block_is_skipped() {
        let md = "<style>\n.columns { display: grid; }\n</style>\n\n# T\n";
        let pres = parse_marp(md).unwrap();
        assert!(pres.slides[0].content_lines.is_empty());
        assert_eq!(pres.slides[0].title.as_deref(), Some("T"));
    }

    #[test]
    fn inline_svg_becomes_svg_field() {
        let md = "# T\n\n<svg viewBox=\"0 0 1 1\">\n<rect/>\n</svg>\n";
        let out = import(md).unwrap();
        assert!(out.contains("svg: |-\n      <svg viewBox"), "{out}");
        assert!(!out.contains("content"), "{out}");
    }

    #[test]
    fn columns_div_becomes_columns() {
        let md = "# T\n\n- first\n\n<div class=\"columns\">\n\n### A\n\n- a1\n- a2\n\n### B\n\n- b1\n\n</div>\n";
        let pres = parse_marp(md).unwrap();
        let s = &pres.slides[0];
        assert_eq!(s.columns.len(), 2);
        assert_eq!(s.columns[0].header.as_deref(), Some("A"));
        assert_eq!(s.columns[0].bullets, vec!["a1", "a2"]);
        assert_eq!(s.columns[1].bullets, vec!["b1"]);
        assert!(s.bullets_first);
        let out = import(md).unwrap();
        assert!(
            out.contains("columns:\n      - header: A\n        bullets:\n          - a1\n"),
            "{out}"
        );
        assert!(out.contains("bullets_first: true"), "{out}");
        crate::parser::parse(&out).expect("emitted YAML parses");
    }

    #[test]
    fn bare_div_columns_have_no_header() {
        let md = "# T\n\n<div class=\"columns\">\n\n<div>\n\n- a\n\n</div>\n\n<div>\n\n- b\n\n</div>\n\n</div>\n\n- after\n";
        let pres = parse_marp(md).unwrap();
        let s = &pres.slides[0];
        assert_eq!(s.columns.len(), 2);
        assert!(s.columns[0].header.is_none());
        assert_eq!(s.columns[1].bullets, vec!["b"]);
        assert_eq!(s.bullets, vec!["after"]);
        assert!(!s.bullets_first);
        crate::parser::parse(&import(md).unwrap()).expect("emitted YAML parses");
    }

    #[test]
    fn unsupported_html_is_an_error() {
        assert!(parse_marp("# T\n\n<div class=\"other\">\n</div>\n").is_err());
        assert!(parse_marp("# T\n\n<svg>\n").is_err());
    }

    #[test]
    fn yaml_keywords_are_quoted() {
        for word in ["Yes", "no", "ON", "Off", "True", "null", "~"] {
            assert_eq!(scalar(word), format!("'{word}'"));
        }
        assert_eq!(scalar("Yesterday"), "Yesterday");
    }

    #[test]
    fn long_content_uses_block_scalar() {
        let long = "x".repeat(200);
        let md = format!("# S\n\n{long}\n");
        let out = import(&md).unwrap();
        assert!(out.contains("content: |-"));
    }
}
