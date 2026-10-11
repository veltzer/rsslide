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
    /// Everything after the title, in source order.
    pub body: Vec<OutElement>,
}

#[derive(Debug, PartialEq)]
pub enum OutElement {
    /// A paragraph, one entry per source line (marp renders line breaks).
    Text(Vec<String>),
    Heading(String, u8),
    List(OutList),
    Code(OutCode),
    Table(OutTable),
    Image(String),
    Svg(String),
    Columns(Vec<OutColumn>),
    Quote(Vec<OutElement>),
}

#[derive(Debug, Default, PartialEq)]
pub struct OutList {
    pub ordered: bool,
    pub items: Vec<OutItem>,
}

#[derive(Debug, PartialEq)]
pub struct OutItem {
    pub text: String,
    pub children: Option<OutList>,
}

#[derive(Debug, Default, PartialEq)]
pub struct OutColumn {
    pub body: Vec<OutElement>,
}

#[derive(Debug, PartialEq)]
pub struct OutCode {
    pub language: Option<String>,
    pub source: String,
}

#[derive(Debug, PartialEq)]
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
    let lines: Vec<&str> = block.lines().collect();
    let mut title = None;
    let body = parse_body(&lines, Some(&mut title))?;
    let (title, title_level) = title.unzip();
    Ok(OutSlide {
        title,
        title_level: title_level.unwrap_or(0),
        body,
    })
}

/// The elements of `lines`, in order. With `title`, the first heading
/// becomes the slide's title (and a second `#` heading is an error);
/// without it (inside a column or a quote), every heading is an element.
fn parse_body(
    lines: &[&str],
    mut title: Option<&mut Option<(String, u8)>>,
) -> Result<Vec<OutElement>> {
    let mut body: Vec<OutElement> = Vec::new();
    let mut paragraph: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        if trimmed.is_empty() {
            flush_paragraph(&mut paragraph, &mut body);
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
            flush_paragraph(&mut paragraph, &mut body);
            if trimmed.starts_with("<style") {
                // The stylesheet only defines the `.columns` grid, which
                // the `columns` element below replaces; rsslide applies no
                // custom CSS.
                i = skip_past(lines, i, "</style>")?;
            } else if trimmed.starts_with("<svg") {
                let end = skip_past(lines, i, "</svg>")?;
                body.push(OutElement::Svg(lines[i..end].join("\n")));
                i = end;
            } else if trimmed == "<div class=\"columns\">" {
                let end = skip_past_div(lines, i)?;
                body.push(OutElement::Columns(parse_columns(&lines[i + 1..end - 1])?));
                i = end;
            } else {
                return Err(anyhow!("unsupported raw HTML: {trimmed:?}"));
            }
            continue;
        }

        // A blockquote: the run of `>` lines, parsed again with one `>`
        // stripped (so `>>` nests).
        if trimmed.starts_with('>') {
            flush_paragraph(&mut paragraph, &mut body);
            let end = (i..lines.len())
                .find(|&j| !lines[j].trim_start().starts_with('>'))
                .unwrap_or(lines.len());
            let inner: Vec<&str> = lines[i..end]
                .iter()
                .map(|l| {
                    let l = l.trim_start().strip_prefix('>').expect("a `>` line");
                    l.strip_prefix(' ').unwrap_or(l)
                })
                .collect();
            body.push(OutElement::Quote(parse_body(&inner, None)?));
            i = end;
            continue;
        }

        if let Some((level, heading)) = parse_heading(trimmed) {
            flush_paragraph(&mut paragraph, &mut body);
            if let Some(title) = title.as_deref_mut()
                && title.is_none()
            {
                *title = Some((heading.to_string(), level));
            } else if level >= 2 {
                body.push(OutElement::Heading(heading.to_string(), level));
            } else {
                return Err(anyhow!(
                    "second `# ` heading {:?} on a slide that already has a title — only one title is allowed",
                    heading
                ));
            }
            i += 1;
            continue;
        }

        if is_fence(trimmed) {
            flush_paragraph(&mut paragraph, &mut body);
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
            body.push(OutElement::Code(OutCode { language, source }));
            continue;
        }

        // GFM table: header row | sep row (with --- and optional :) | body rows.
        if is_table_row(trimmed)
            && i + 1 < lines.len()
            && let Some(aligns) = parse_table_separator(lines[i + 1].trim())
        {
            flush_paragraph(&mut paragraph, &mut body);
            let headers = split_table_row(trimmed);
            if headers.len() != aligns.len() {
                return Err(anyhow!(
                    "table header {trimmed:?} has {} cells but its separator row has {}",
                    headers.len(),
                    aligns.len()
                ));
            }
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
            body.push(OutElement::Table(OutTable {
                headers,
                rows,
                aligns,
            }));
            i = j;
            continue;
        }

        if list_item(line).is_some() {
            flush_paragraph(&mut paragraph, &mut body);
            let (list, next) = parse_list(lines, i)?;
            body.push(OutElement::List(list));
            i = next;
            continue;
        }

        if let Some(path) = parse_image(trimmed) {
            flush_paragraph(&mut paragraph, &mut body);
            body.push(OutElement::Image(path));
            i += 1;
            continue;
        }

        paragraph.push(trimmed.to_string());
        i += 1;
    }
    flush_paragraph(&mut paragraph, &mut body);
    Ok(body)
}

fn is_fence(trimmed: &str) -> bool {
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

/// `(indent, ordered, text)` of a list item line: `- x`, `* x`, `+ x`,
/// `1. x` or `1) x`, indented by any amount (a tab counts as four spaces).
fn list_item(line: &str) -> Option<(usize, bool, &str)> {
    let body = line.trim_start_matches([' ', '\t']);
    let lead = &line[..line.len() - body.len()];
    let indent = lead.chars().map(|c| if c == '\t' { 4 } else { 1 }).sum();
    for marker in ["- ", "* ", "+ "] {
        if let Some(text) = body.strip_prefix(marker) {
            return Some((indent, false, text.trim()));
        }
    }
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    if (1..=9).contains(&digits) {
        let rest = &body[digits..];
        for marker in [". ", ") "] {
            if let Some(text) = rest.strip_prefix(marker) {
                return Some((indent, true, text.trim()));
            }
        }
    }
    None
}

/// Parse the list that starts at `lines[start]`. Returns the list and the
/// index of the first line after it. Deeper-indented items nest under the
/// item before them. Blank lines between items do not end the list; a
/// plain line straight after an item continues that item's text, as in
/// CommonMark. A top-level item of the other kind (bullets after numbers or
/// the reverse) starts a new list, so it ends this one.
fn parse_list(lines: &[&str], start: usize) -> Result<(OutList, usize)> {
    // Open lists, outermost first, each with the indentation of its items.
    let mut stack: Vec<(usize, OutList)> = Vec::new();
    let mut i = start;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            let next = (i + 1..lines.len()).find(|&j| !lines[j].trim().is_empty());
            match next.and_then(|j| list_item(lines[j]).map(|item| (j, item))) {
                Some((j, (indent, ordered, _)))
                    if indent > stack[0].0 || ordered == stack[0].1.ordered =>
                {
                    i = j;
                    continue;
                }
                _ => break,
            }
        }
        let Some((indent, ordered, text)) = list_item(line) else {
            let interrupts = is_fence(trimmed)
                || parse_heading(trimmed).is_some()
                || is_table_row(trimmed)
                || trimmed.starts_with('<')
                || trimmed.starts_with('>')
                || is_slide_separator(trimmed);
            if interrupts {
                break;
            }
            let (_, list) = stack.last_mut().expect("a list item came first");
            let item = list.items.last_mut().expect("an open list has an item");
            item.text.push(' ');
            item.text.push_str(trimmed);
            i += 1;
            continue;
        };
        if stack.is_empty() {
            stack.push((
                indent,
                OutList {
                    ordered,
                    items: Vec::new(),
                },
            ));
        } else {
            while stack.len() > 1 && indent < stack[stack.len() - 1].0 {
                close_level(&mut stack);
            }
            let (top_indent, top) = stack.last().expect("stack is not empty");
            if indent > *top_indent {
                stack.push((
                    indent,
                    OutList {
                        ordered,
                        items: Vec::new(),
                    },
                ));
            } else if ordered != top.ordered {
                if stack.len() == 1 {
                    break;
                }
                return Err(anyhow!(
                    "list item {trimmed:?} switches between bullets and numbers inside a nested list"
                ));
            }
        }
        let (_, list) = stack.last_mut().expect("stack is not empty");
        list.items.push(OutItem {
            text: text.to_string(),
            children: None,
        });
        i += 1;
    }
    while stack.len() > 1 {
        close_level(&mut stack);
    }
    let (_, list) = stack.pop().expect("parse_list starts on a list item");
    Ok((list, i))
}

/// Pop the innermost open list and attach it to the last item of its parent.
fn close_level(stack: &mut Vec<(usize, OutList)>) {
    let (_, child) = stack.pop().expect("a level to close");
    let (_, parent) = stack.last_mut().expect("a parent level");
    let item = parent
        .items
        .last_mut()
        .expect("a nested list follows an item");
    item.children = Some(child);
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

/// The inside of a `<div class="columns">`. Each column is either a bare
/// `<div>` ... `</div>`, or a heading line and everything after it up to
/// the next heading of the same level (headings inside code fences do not
/// count). A column's content is parsed like a slide body.
fn parse_columns(lines: &[&str]) -> Result<Vec<OutColumn>> {
    let mut columns: Vec<Vec<&str>> = Vec::new();
    // The heading level that starts header columns, once one is seen.
    let mut header_level: Option<u8> = None;
    let mut in_fence = false;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if !in_fence && trimmed == "<div>" {
            let end = skip_past_div(lines, i)?;
            columns.push(lines[i + 1..end - 1].to_vec());
            header_level = None;
            i = end;
            continue;
        }
        let heading = if in_fence {
            None
        } else {
            parse_heading(trimmed)
        };
        if let Some((level, _)) = heading
            && header_level.is_none_or(|h| h == level)
        {
            header_level = Some(level);
            columns.push(vec![line]);
        } else if trimmed.is_empty() && columns.is_empty() {
            // Blank lines before the first column.
        } else if header_level.is_none() {
            return Err(anyhow!(
                "unsupported line inside <div class=\"columns\"> outside any column: {trimmed:?}"
            ));
        } else {
            columns.last_mut().expect("a header column").push(line);
        }
        if is_fence(trimmed) {
            in_fence = !in_fence;
        }
        i += 1;
    }
    if columns.is_empty() {
        return Err(anyhow!("<div class=\"columns\"> has no columns"));
    }
    columns
        .iter()
        .map(|lines| {
            Ok(OutColumn {
                body: parse_body(lines, None)?,
            })
        })
        .collect()
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

fn flush_paragraph(paragraph: &mut Vec<String>, body: &mut Vec<OutElement>) {
    if !paragraph.is_empty() {
        body.push(OutElement::Text(std::mem::take(paragraph)));
    }
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
    if slide.title.is_none() && slide.body.is_empty() {
        out.push_str("  - {}\n");
        return;
    }
    let mut lead = "  - ";
    if let Some(title) = &slide.title {
        out.push_str(&format!("{lead}title: {}\n", scalar(title)));
        if slide.title_level != 1 {
            out.push_str(&format!("    title_level: {}\n", slide.title_level));
        }
        lead = "    ";
    }
    if slide.body.is_empty() {
        return;
    }
    out.push_str(&format!("{lead}body:\n"));
    for element in &slide.body {
        emit_element(out, element, 6);
    }
}

/// One body element, as a list item at column `col`; its value is
/// indented to column `col + 4`.
fn emit_element(out: &mut String, element: &OutElement, col: usize) {
    let pad = " ".repeat(col);
    let value = " ".repeat(col + 4);
    match element {
        OutElement::Text(lines) => {
            if lines.len() == 1 && !needs_block_scalar(&lines[0]) {
                out.push_str(&format!("{pad}- text: {}\n", scalar(&lines[0])));
            } else {
                out.push_str(&format!("{pad}- text: |-\n"));
                push_block_lines(out, lines, &value);
            }
        }
        OutElement::Heading(text, level) => {
            out.push_str(&format!("{pad}- heading:\n"));
            out.push_str(&format!("{value}text: {}\n", scalar(text)));
            out.push_str(&format!("{value}level: {level}\n"));
        }
        OutElement::List(list) => {
            let key = if list.ordered { "numbered" } else { "bullets" };
            out.push_str(&format!("{pad}- {key}:\n"));
            emit_items(out, &list.items, col + 4);
        }
        OutElement::Code(code) => {
            out.push_str(&format!("{pad}- code:\n"));
            emit_code(out, code, &value, &value);
        }
        OutElement::Table(table) => {
            out.push_str(&format!("{pad}- table:\n"));
            emit_table(out, table, col + 4);
        }
        OutElement::Image(path) => {
            out.push_str(&format!("{pad}- image: {}\n", scalar(path)));
        }
        OutElement::Svg(svg) => {
            out.push_str(&format!("{pad}- svg: {}\n", block_header(svg)));
            let lines: Vec<String> = svg.lines().map(str::to_string).collect();
            push_block_lines(out, &lines, &value);
        }
        OutElement::Columns(columns) => {
            out.push_str(&format!("{pad}- columns:\n"));
            for column in columns {
                if column.body.is_empty() {
                    out.push_str(&format!("{value}- body: []\n"));
                    continue;
                }
                out.push_str(&format!("{value}- body:\n"));
                for element in &column.body {
                    emit_element(out, element, col + 8);
                }
            }
        }
        OutElement::Quote(body) => {
            if body.is_empty() {
                out.push_str(&format!("{pad}- quote: []\n"));
                return;
            }
            out.push_str(&format!("{pad}- quote:\n"));
            for element in body {
                emit_element(out, element, col + 4);
            }
        }
    }
}

/// List items as a YAML sequence at column `col`; an item with a nested
/// list becomes `{text, bullets|numbered}`.
fn emit_items(out: &mut String, items: &[OutItem], col: usize) {
    let pad = " ".repeat(col);
    for item in items {
        match &item.children {
            None => out.push_str(&format!("{pad}- {}\n", scalar(&item.text))),
            Some(children) => {
                let key = if children.ordered {
                    "numbered"
                } else {
                    "bullets"
                };
                out.push_str(&format!("{pad}- text: {}\n", scalar(&item.text)));
                out.push_str(&format!("{pad}  {key}:\n"));
                emit_items(out, &children.items, col + 4);
            }
        }
    }
}

/// A table's fields as a mapping at column `col`.
fn emit_table(out: &mut String, table: &OutTable, col: usize) {
    let pad = " ".repeat(col);
    out.push_str(&format!("{pad}headers:\n"));
    for h in &table.headers {
        out.push_str(&format!("{pad}  - {}\n", scalar(h)));
    }
    if table.aligns.iter().any(|a| *a != "auto") {
        out.push_str(&format!("{pad}aligns:\n"));
        for a in &table.aligns {
            out.push_str(&format!("{pad}  - {a}\n"));
        }
    }
    out.push_str(&format!("{pad}rows:\n"));
    for row in &table.rows {
        out.push_str(&format!("{pad}  -\n"));
        for cell in row {
            out.push_str(&format!("{pad}    - {}\n", scalar(cell)));
        }
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

/// Block scalar lines, each prefixed with `indent`; blank lines stay empty.
fn push_block_lines(out: &mut String, lines: &[String], indent: &str) {
    for line in lines {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            out.push('\n');
        } else {
            out.push_str(indent);
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
    use crate::model::{Element, Presentation};

    fn slide(md: &str) -> OutSlide {
        parse_marp(md).unwrap().slides.remove(0)
    }

    /// Import `md`, then parse the YAML with the real parser.
    fn round_trip(md: &str) -> Presentation {
        let yaml = import(md).unwrap();
        crate::parser::parse(&yaml).unwrap_or_else(|e| panic!("{e}\n---\n{yaml}"))
    }

    fn text(lines: &[&str]) -> OutElement {
        OutElement::Text(lines.iter().map(|l| l.to_string()).collect())
    }

    fn item(text: &str) -> OutItem {
        OutItem {
            text: text.into(),
            children: None,
        }
    }

    fn list(ordered: bool, items: Vec<OutItem>) -> OutList {
        OutList { ordered, items }
    }

    // ── slides and front matter ───────────────────────────────────────────

    #[test]
    fn imports_front_matter_and_single_slide() {
        let md = "---\ntitle: Deck\ntheme: default\npaginate: true\n---\n\n# Hello\n\nworld\n";
        let pres = parse_marp(md).unwrap();
        assert_eq!(pres.title.as_deref(), Some("Deck"));
        assert_eq!(pres.paginate, Some(true));
        assert_eq!(pres.slides.len(), 1);
        assert_eq!(pres.slides[0].title.as_deref(), Some("Hello"));
        assert_eq!(pres.slides[0].body, vec![text(&["world"])]);
    }

    #[test]
    fn splits_multiple_slides_on_hr() {
        let pres = parse_marp("# A\n\n---\n\n# B\n\n---\n\n# C\n").unwrap();
        let titles: Vec<_> = pres.slides.iter().map(|s| s.title.as_deref()).collect();
        assert_eq!(titles, [Some("A"), Some("B"), Some("C")]);
    }

    #[test]
    fn hr_inside_code_fence_is_not_a_separator() {
        let pres = parse_marp("# A\n\n```\nx\n---\ny\n```\n").unwrap();
        assert_eq!(pres.slides.len(), 1);
    }

    #[test]
    fn yaml_output_starts_with_doc_marker() {
        assert!(import("# T\n").unwrap().starts_with("---\n"));
    }

    #[test]
    fn title_keeps_its_level_and_a_second_h1_is_an_error() {
        let s = slide("## Sub-level title\n");
        assert_eq!(
            (s.title.as_deref(), s.title_level),
            (Some("Sub-level title"), 2)
        );
        assert!(import("## X\n").unwrap().contains("title_level: 2"));
        assert!(!import("# X\n").unwrap().contains("title_level"));
        assert!(parse_marp("# A\n\n# B\n").is_err());
    }

    // ── source order ──────────────────────────────────────────────────────

    #[test]
    fn body_keeps_source_order() {
        let md = "## T\n\nintro\n\n```bash\none\n```\n\n**Label**:\n- a\n\n### Sub\n\n| H |\n|---|\n| c |\n\n```\ntwo\n```\n";
        let body = slide(md).body;
        let kinds: Vec<&str> = body
            .iter()
            .map(|e| match e {
                OutElement::Text(_) => "text",
                OutElement::Heading(..) => "heading",
                OutElement::List(_) => "list",
                OutElement::Code(_) => "code",
                OutElement::Table(_) => "table",
                OutElement::Image(_) => "image",
                OutElement::Svg(_) => "svg",
                OutElement::Columns(_) => "columns",
                OutElement::Quote(_) => "quote",
            })
            .collect();
        assert_eq!(
            kinds,
            ["text", "code", "text", "list", "heading", "table", "code"]
        );
        let pres = round_trip(md);
        assert_eq!(pres.slides[0].body.len(), 7);
        assert!(matches!(pres.slides[0].body[4], Element::Heading(ref h) if h.level == 3));
    }

    #[test]
    fn paragraph_lines_stay_one_text_element() {
        let body = slide("# T\n\nline one\nline two\n\nnext paragraph\n").body;
        assert_eq!(
            body,
            vec![text(&["line one", "line two"]), text(&["next paragraph"])]
        );
    }

    #[test]
    fn long_text_uses_a_block_scalar() {
        let long = "x".repeat(200);
        let yaml = import(&format!("# S\n\n{long}\n")).unwrap();
        assert!(yaml.contains("      - text: |-\n"), "{yaml}");
        round_trip(&format!("# S\n\n{long}\n"));
    }

    // ── lists ─────────────────────────────────────────────────────────────

    #[test]
    fn numbered_list_is_ordered() {
        let body = slide("# T\n\n1. one\n1. two\n").body;
        assert_eq!(
            body,
            vec![OutElement::List(list(true, vec![item("one"), item("two")]))]
        );
        let pres = round_trip("# T\n\n1. one\n1. two\n");
        assert!(
            matches!(&pres.slides[0].body[0], Element::List(l) if l.ordered && l.items.len() == 2)
        );
    }

    #[test]
    fn nested_lists_follow_indentation() {
        let md = "# T\n\n- a\n    - a1\n        - a1x\n    - a2\n- b\n    1. b1\n";
        let mut a = item("a");
        let mut a1 = item("a1");
        a1.children = Some(list(false, vec![item("a1x")]));
        a.children = Some(list(false, vec![a1, item("a2")]));
        let mut b = item("b");
        b.children = Some(list(true, vec![item("b1")]));
        assert_eq!(
            slide(md).body,
            vec![OutElement::List(list(false, vec![a, b]))]
        );

        let pres = round_trip(md);
        let Element::List(l) = &pres.slides[0].body[0] else {
            panic!("a list")
        };
        let a = &l.items[0];
        let a1 = &a.children.as_ref().unwrap().items[0];
        assert_eq!(a1.children.as_ref().unwrap().items[0].text, "a1x");
        assert!(l.items[1].children.as_ref().unwrap().ordered);
    }

    #[test]
    fn blank_lines_between_items_do_not_split_the_list() {
        let body = slide("# T\n\n- a\n\n- b\n").body;
        assert_eq!(
            body,
            vec![OutElement::List(list(false, vec![item("a"), item("b")]))]
        );
    }

    #[test]
    fn switching_kind_at_top_level_starts_a_new_list() {
        let body = slide("# T\n\n- a\n1. b\n").body;
        assert_eq!(
            body,
            vec![
                OutElement::List(list(false, vec![item("a")])),
                OutElement::List(list(true, vec![item("b")])),
            ]
        );
    }

    #[test]
    fn switching_kind_inside_a_nested_list_is_an_error() {
        assert!(parse_marp("# T\n\n- a\n    - b\n    1. c\n").is_err());
    }

    #[test]
    fn a_plain_line_after_an_item_continues_it() {
        let body = slide("# T\n\n- first\n  more of first\n- second\n").body;
        assert_eq!(
            body,
            vec![OutElement::List(list(
                false,
                vec![item("first more of first"), item("second")]
            ))]
        );
    }

    #[test]
    fn a_fence_ends_the_list() {
        let body = slide("# T\n\n- a\n    ```bash\n    echo\n    ```\n- b\n").body;
        assert!(matches!(body[0], OutElement::List(ref l) if l.items.len() == 1));
        assert!(matches!(body[1], OutElement::Code(_)));
        assert!(matches!(body[2], OutElement::List(_)));
    }

    #[test]
    fn bold_label_line_is_not_a_bullet() {
        let body = slide("# T\n\n**Pros**:\n- fast\n").body;
        assert_eq!(
            body,
            vec![
                text(&["**Pros**:"]),
                OutElement::List(list(false, vec![item("fast")]))
            ]
        );
    }

    // ── code ──────────────────────────────────────────────────────────────

    #[test]
    fn every_code_block_is_kept_in_order() {
        let md = "# T\n\nfirst:\n\n```bash\necho one\n```\n\nsecond:\n\n```\nplain two\n```\n";
        let body = slide(md).body;
        let code: Vec<&OutCode> = body
            .iter()
            .filter_map(|e| match e {
                OutElement::Code(c) => Some(c),
                _ => None,
            })
            .collect();
        assert_eq!(code[0].source, "echo one");
        assert_eq!(code[0].language.as_deref(), Some("bash"));
        assert_eq!(code[1].source, "plain two");
        assert!(code[1].language.is_none());
        let yaml = import(md).unwrap();
        assert!(
            yaml.contains("      - code:\n          language: bash\n          source: |-\n            echo one\n"),
            "{yaml}"
        );
        round_trip(md);
    }

    #[test]
    fn code_whose_first_line_is_indented_round_trips() {
        // A snippet cut from the middle of a function.
        let pres = round_trip("# T\n\n```python\n        x = 1\n    return x\n```\n");
        let Element::Code(code) = &pres.slides[0].body[0] else {
            panic!("code")
        };
        assert_eq!(code.source, "        x = 1\n    return x");
    }

    // ── tables ────────────────────────────────────────────────────────────

    #[test]
    fn parses_tables_with_alignment() {
        let body = slide("# T\n\n| A | B | C | D |\n|---|:--|:-:|--:|\n| 1 | 2 | 3 | 4 |\n").body;
        let OutElement::Table(t) = &body[0] else {
            panic!("table")
        };
        assert_eq!(t.headers, ["A", "B", "C", "D"]);
        assert_eq!(t.rows, vec![vec!["1", "2", "3", "4"]]);
        assert_eq!(t.aligns, ["auto", "left", "center", "right"]);
        round_trip("# T\n\n| A | B |\n|:-:|--:|\n| 1 | 2 |\n");
    }

    #[test]
    fn every_table_is_kept() {
        let md = "# T\n\n| A |\n|---|\n| 1 |\n\ntext\n\n| B |\n|---|\n| 2 |\n";
        let body = slide(md).body;
        assert_eq!(body.len(), 3);
        assert!(matches!(body[2], OutElement::Table(ref t) if t.headers == ["B"]));
        round_trip(md);
    }

    #[test]
    fn escaped_pipe_stays_inside_its_cell() {
        let md = "# T\n\n| Problem | Command |\n|---|---|\n| gw? | `ip route \\| grep default` |\n| ip? | `ip addr` |\n";
        let OutElement::Table(t) = &slide(md).body[0] else {
            panic!("table")
        };
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.rows[0][1], "`ip route | grep default`");
    }

    #[test]
    fn ragged_table_row_is_an_error() {
        let err = parse_marp("# T\n\n| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 | 5 |\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("has 3 cells but the header has 2"), "{err}");
    }

    // ── images, html ──────────────────────────────────────────────────────

    #[test]
    fn parses_image() {
        assert_eq!(
            slide("# T\n\n![alt](svg/x.svg)\n").body,
            vec![OutElement::Image("svg/x.svg".into())]
        );
        round_trip("# T\n\n![alt](svg/x.svg)\n");
    }

    #[test]
    fn inline_svg_becomes_an_svg_element() {
        let md = "# T\n\n  <svg viewBox=\"0 0 1 1\">\n    <rect/>\n  </svg>\n";
        let pres = round_trip(md);
        assert!(matches!(&pres.slides[0].body[0], Element::Svg(svg) if svg.starts_with("  <svg")));
    }

    #[test]
    fn style_block_is_skipped() {
        let s = slide("<style>\n.columns { display: grid; }\n</style>\n\n# T\n");
        assert_eq!(s.title.as_deref(), Some("T"));
        assert!(s.body.is_empty());
    }

    fn heading(text: &str, level: u8) -> OutElement {
        OutElement::Heading(text.into(), level)
    }

    #[test]
    fn columns_div_becomes_columns() {
        let md = "# T\n\n- first\n\n<div class=\"columns\">\n\n### A\n\n- a1\n- a2\n\n### B\n\n- b1\n\n</div>\n";
        let body = slide(md).body;
        let OutElement::Columns(cols) = &body[1] else {
            panic!("columns after the list")
        };
        assert_eq!(
            cols[0].body,
            vec![
                heading("A", 3),
                OutElement::List(list(false, vec![item("a1"), item("a2")]))
            ]
        );
        assert_eq!(
            cols[1].body[1],
            OutElement::List(list(false, vec![item("b1")]))
        );
        round_trip(md);
    }

    #[test]
    fn bare_div_columns_hold_any_content() {
        let md = "# T\n\n<div class=\"columns\">\n<div>\n\n### Unix\n\n```bash\ncurl x | sh\n```\n\n</div>\n<div>\n\n**Windows**:\n- run the installer\n\n</div>\n</div>\n";
        let OutElement::Columns(cols) = &slide(md).body[0] else {
            panic!("columns")
        };
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0].body[0], heading("Unix", 3));
        assert!(matches!(&cols[0].body[1], OutElement::Code(c) if c.source == "curl x | sh"));
        assert_eq!(cols[1].body[0], text(&["**Windows**:"]));
        let pres = round_trip(md);
        let Element::Columns(cols) = &pres.slides[0].body[0] else {
            panic!("columns")
        };
        assert!(matches!(cols[0].body[1], Element::Code(_)));
    }

    #[test]
    fn header_columns_split_only_on_their_own_level_outside_fences() {
        let md = "# T\n\n<div class=\"columns\">\n\n### A\n\n#### detail\n\n```\n### not a column\n```\n\n### B\n\ntext\n\n</div>\n";
        let OutElement::Columns(cols) = &slide(md).body[0] else {
            panic!("columns")
        };
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0].body[1], heading("detail", 4));
        assert!(matches!(&cols[0].body[2], OutElement::Code(c) if c.source == "### not a column"));
        round_trip(md);
    }

    #[test]
    fn content_outside_any_column_is_an_error() {
        assert!(parse_marp("# T\n\n<div class=\"columns\">\nstray\n</div>\n").is_err());
    }

    #[test]
    fn blockquote_becomes_a_quote() {
        let md = "# T\n\n> No algorithm can guarantee consensus.\n> Second line\n>\n> Next paragraph\n\nafter\n";
        let body = slide(md).body;
        assert_eq!(
            body,
            vec![
                OutElement::Quote(vec![
                    text(&["No algorithm can guarantee consensus.", "Second line"]),
                    text(&["Next paragraph"]),
                ]),
                text(&["after"]),
            ]
        );
        let pres = round_trip(md);
        assert!(matches!(&pres.slides[0].body[0], Element::Quote(q) if q.len() == 2));
    }

    #[test]
    fn nested_quotes_and_lists_in_quotes() {
        let md = "# T\n\n> outer\n>\n> > inner\n>\n> - item\n";
        let OutElement::Quote(q) = &slide(md).body[0] else {
            panic!("quote")
        };
        assert_eq!(q[0], text(&["outer"]));
        assert_eq!(q[1], OutElement::Quote(vec![text(&["inner"])]));
        assert_eq!(q[2], OutElement::List(list(false, vec![item("item")])));
        round_trip(md);
    }

    #[test]
    fn a_quote_line_ends_a_list() {
        let body = slide("# T\n\n- a\n> quoted\n").body;
        assert!(matches!(body[0], OutElement::List(_)));
        assert!(matches!(body[1], OutElement::Quote(_)));
    }

    #[test]
    fn unsupported_html_is_an_error() {
        assert!(parse_marp("# T\n\n<div class=\"other\">\n</div>\n").is_err());
        assert!(parse_marp("# T\n\n<svg>\n").is_err());
    }

    // ── YAML scalars ──────────────────────────────────────────────────────

    #[test]
    fn yaml_keywords_are_quoted() {
        for word in ["Yes", "no", "ON", "Off", "True", "null", "~"] {
            assert_eq!(scalar(word), format!("'{word}'"));
        }
        assert_eq!(scalar("Yesterday"), "Yesterday");
        let pres = round_trip("# T\n\n- Yes\n- no\n");
        let Element::List(l) = &pres.slides[0].body[0] else {
            panic!("list")
        };
        assert_eq!(l.items[0].text, "Yes");
    }
}
