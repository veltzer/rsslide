use crate::model::{Presentation, Slide};
use anyhow::Result;
use printpdf::*;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

// 16:9 slide dimensions in mm
const SLIDE_W: f32 = 254.0;
const SLIDE_H: f32 = 143.0;

// Margins
const MARGIN_X: f32 = 14.0;
const MARGIN_TOP: f32 = 120.0;

pub fn export(presentation: &Presentation, output_path: &Path) -> Result<()> {
    let title = presentation.title.as_deref().unwrap_or("Presentation");

    let (doc, page1, layer1) = PdfDocument::new(title, Mm(SLIDE_W), Mm(SLIDE_H), "Layer 1");

    let font = doc.add_builtin_font(BuiltinFont::Helvetica)?;
    let font_bold = doc.add_builtin_font(BuiltinFont::HelveticaBold)?;

    let slides = &presentation.slides;

    render_slide(
        doc.get_page(page1).get_layer(layer1),
        slides.get(0),
        presentation.paginate.unwrap_or(false),
        1,
        slides.len(),
        &font,
        &font_bold,
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
        );
    }

    let file = File::create(output_path)?;
    doc.save(&mut BufWriter::new(file))?;
    Ok(())
}

fn render_slide(
    layer: PdfLayerReference,
    slide: Option<&Slide>,
    paginate: bool,
    page_num: usize,
    total_pages: usize,
    font: &IndirectFontRef,
    font_bold: &IndirectFontRef,
) {
    let Some(slide) = slide else { return };

    let mut cursor_y = MARGIN_TOP;

    // Title
    if let Some(title) = &slide.title {
        layer.use_text(title.as_str(), 28.0, Mm(MARGIN_X), Mm(cursor_y), font_bold);
        cursor_y -= 12.0;
        // Underline rule
        layer.add_line(Line {
            points: vec![
                (Point::new(Mm(MARGIN_X), Mm(cursor_y + 2.0)), false),
                (Point::new(Mm(SLIDE_W - MARGIN_X), Mm(cursor_y + 2.0)), false),
            ],
            is_closed: false,
        });
        cursor_y -= 8.0;
    }

    // Body content
    if let Some(content) = &slide.content {
        for line in wrap_text(content, 80) {
            layer.use_text(line.as_str(), 14.0, Mm(MARGIN_X), Mm(cursor_y), font);
            cursor_y -= 7.0;
        }
        cursor_y -= 4.0;
    }

    // Bullet list
    if let Some(bullets) = &slide.bullets {
        for bullet in bullets {
            let text = format!("• {}", bullet);
            layer.use_text(text.as_str(), 14.0, Mm(MARGIN_X), Mm(cursor_y), font);
            cursor_y -= 7.0;
        }
        cursor_y -= 4.0;
    }

    // Code block
    if let Some(code) = &slide.code {
        let font_code = font; // fall back to regular; ideally Courier
        if let Some(lang) = &code.language {
            let label = format!("[{}]", lang.to_uppercase());
            layer.use_text(label.as_str(), 9.0, Mm(MARGIN_X), Mm(cursor_y), font_code);
            cursor_y -= 6.0;
        }
        for line in code.source.lines() {
            layer.use_text(line, 10.0, Mm(MARGIN_X + 2.0), Mm(cursor_y), font_code);
            cursor_y -= 5.5;
        }
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
