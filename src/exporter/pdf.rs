use crate::model::{Presentation, Slide};
use anyhow::Result;
use printpdf::*;
use printpdf::path::PaintMode;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

// 16:9 slide dimensions in mm
const SLIDE_W: f32 = 254.0;
const SLIDE_H: f32 = 143.0;

// Layout
const MARGIN_X: f32 = 14.0;
const MARGIN_TOP: f32 = 126.0; // title baseline: 17mm from top of slide

// Title
const TITLE_FONT_SIZE: f32 = 36.0;
const TITLE_RULE_OFFSET: f32 = 14.0; // mm below title baseline to the rule
const TITLE_CONTENT_GAP: f32 = 8.0;  // mm below rule to first content line

// Body text and bullets
const BODY_FONT_SIZE: f32 = 18.0;
const BODY_LINE_HEIGHT: f32 = 9.0;
const BODY_SECTION_GAP: f32 = 5.0;

// Code block
// Courier glyph advance = 0.6 × em; at 12pt: 0.6 × 12pt × (25.4mm/72pt) ≈ 2.54mm
const CODE_FONT_SIZE: f32 = 12.0;
const CODE_LINE_HEIGHT: f32 = 6.5;
const CODE_PADDING: f32 = 4.0; // padding inside the background box (top & bottom)
const CODE_BG: f32 = 0.94;     // light-gray background fill

const MM_PER_PT: f32 = 25.4 / 72.0;
const COURIER_CHAR_WIDTH_MM: f32 = 0.6 * CODE_FONT_SIZE * MM_PER_PT;

pub fn export(presentation: &Presentation, output_path: &Path) -> Result<()> {
    let title = presentation.title.as_deref().unwrap_or("Presentation");

    let (doc, page1, layer1) = PdfDocument::new(title, Mm(SLIDE_W), Mm(SLIDE_H), "Layer 1");

    let font = doc.add_builtin_font(BuiltinFont::Helvetica)?;
    let font_bold = doc.add_builtin_font(BuiltinFont::HelveticaBold)?;
    let font_mono = doc.add_builtin_font(BuiltinFont::Courier)?;

    // Load syntect syntax/theme sets once, share across all slides.
    let syntax_set = SyntaxSet::load_defaults_newlines();
    let theme_set = ThemeSet::load_defaults();
    let theme = &theme_set.themes["InspiredGitHub"];

    let slides = &presentation.slides;

    render_slide(
        doc.get_page(page1).get_layer(layer1),
        slides.first(),
        presentation.paginate.unwrap_or(false),
        1,
        slides.len(),
        &font,
        &font_bold,
        &font_mono,
        &syntax_set,
        theme,
    );

    for (i, slide) in slides.iter().enumerate().skip(1) {
        let (page, layer) = doc.add_page(Mm(SLIDE_W), Mm(SLIDE_H), "Layer 1");
        render_slide(
            doc.get_page(page).get_layer(layer),
            Some(slide),
            presentation.paginate.unwrap_or(false),
            i + 1,
            slides.len(),
            &font,
            &font_bold,
            &font_mono,
            &syntax_set,
            theme,
        );
    }

    let file = File::create(output_path)?;
    doc.save(&mut BufWriter::new(file))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_slide(
    layer: PdfLayerReference,
    slide: Option<&Slide>,
    paginate: bool,
    page_num: usize,
    total_pages: usize,
    font: &IndirectFontRef,
    font_bold: &IndirectFontRef,
    font_mono: &IndirectFontRef,
    syntax_set: &SyntaxSet,
    theme: &syntect::highlighting::Theme,
) {
    let Some(slide) = slide else { return };

    let mut cursor_y = MARGIN_TOP;

    // Title
    if let Some(title) = &slide.title {
        layer.set_fill_color(Color::Rgb(Rgb::new(0.0, 0.0, 0.0, None)));
        layer.use_text(title.as_str(), TITLE_FONT_SIZE, Mm(MARGIN_X), Mm(cursor_y), font_bold);
        cursor_y -= TITLE_RULE_OFFSET;
        layer.add_line(Line {
            points: vec![
                (Point::new(Mm(MARGIN_X), Mm(cursor_y + 2.0)), false),
                (Point::new(Mm(SLIDE_W - MARGIN_X), Mm(cursor_y + 2.0)), false),
            ],
            is_closed: false,
        });
        cursor_y -= TITLE_CONTENT_GAP;
    }

    // Body content
    if let Some(content) = &slide.content {
        layer.set_fill_color(Color::Rgb(Rgb::new(0.0, 0.0, 0.0, None)));
        for line in wrap_text(content, 60) {
            layer.use_text(line.as_str(), BODY_FONT_SIZE, Mm(MARGIN_X), Mm(cursor_y), font);
            cursor_y -= BODY_LINE_HEIGHT;
        }
        cursor_y -= BODY_SECTION_GAP;
    }

    // Bullet list
    if let Some(bullets) = &slide.bullets {
        layer.set_fill_color(Color::Rgb(Rgb::new(0.0, 0.0, 0.0, None)));
        for bullet in bullets {
            let text = format!("• {}", bullet);
            layer.use_text(text.as_str(), BODY_FONT_SIZE, Mm(MARGIN_X), Mm(cursor_y), font);
            cursor_y -= BODY_LINE_HEIGHT;
        }
        cursor_y -= BODY_SECTION_GAP;
    }

    // Syntax-highlighted code block
    if let Some(code) = &slide.code {
        render_code_block(
            &layer,
            &code.source,
            code.language.as_deref(),
            &mut cursor_y,
            font_mono,
            syntax_set,
            theme,
        );
    }

    // Page number
    if paginate {
        let page_str = format!("{} / {}", page_num, total_pages);
        layer.use_text(
            page_str.as_str(),
            9.0,
            Mm(SLIDE_W - MARGIN_X - 12.0),
            Mm(4.0),
            font,
        );
    }
}

/// Render a syntax-highlighted code block onto the PDF layer, with a gray background box.
fn render_code_block(
    layer: &PdfLayerReference,
    source: &str,
    language: Option<&str>,
    cursor_y: &mut f32,
    font_mono: &IndirectFontRef,
    syntax_set: &SyntaxSet,
    theme: &syntect::highlighting::Theme,
) {
    let line_count = LinesWithEndings::from(source).count();
    if line_count == 0 {
        return;
    }

    // Draw light-gray background box sized to fit all lines plus padding.
    let box_top = *cursor_y + CODE_PADDING;
    let box_bottom = *cursor_y - (line_count as f32 * CODE_LINE_HEIGHT) - CODE_PADDING;
    layer.set_fill_color(Color::Rgb(Rgb::new(CODE_BG, CODE_BG, CODE_BG, None)));
    layer.add_rect(
        Rect::new(
            Mm(MARGIN_X - CODE_PADDING),
            Mm(box_bottom),
            Mm(SLIDE_W - MARGIN_X + CODE_PADDING),
            Mm(box_top),
        )
        .with_mode(PaintMode::Fill),
    );

    let syntax = language
        .and_then(|lang| syntax_set.find_syntax_by_token(lang))
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

    let mut highlighter = HighlightLines::new(syntax, theme);

    for line in LinesWithEndings::from(source) {
        let ranges: Vec<(Style, &str)> = highlighter
            .highlight_line(line, syntax_set)
            .unwrap_or_default();

        let mut x = MARGIN_X + 2.0;

        for (style, token) in ranges {
            let text = token.trim_end_matches('\n').trim_end_matches('\r');
            if text.is_empty() {
                continue;
            }

            let fg = style.foreground;
            layer.set_fill_color(Color::Rgb(Rgb::new(
                fg.r as f32 / 255.0,
                fg.g as f32 / 255.0,
                fg.b as f32 / 255.0,
                None,
            )));

            layer.use_text(text, CODE_FONT_SIZE, Mm(x), Mm(*cursor_y), font_mono);

            x += text.chars().count() as f32 * COURIER_CHAR_WIDTH_MM;
        }

        layer.set_fill_color(Color::Rgb(Rgb::new(0.0, 0.0, 0.0, None)));
        *cursor_y -= CODE_LINE_HEIGHT;
    }

    // Skip past the bottom padding of the box.
    *cursor_y -= CODE_PADDING;
}

/// Naive word-wrap: splits text into lines of at most `max_chars` characters.
fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let words: Vec<&str> = paragraph.split_whitespace().collect();
        let mut current = String::new();
        for word in words {
            if current.is_empty() {
                current.push_str(word);
            } else if current.len() + 1 + word.len() <= max_chars {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(current.clone());
                current = word.to_string();
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    lines
}

