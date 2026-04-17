use crate::model::{Presentation, Slide};
use anyhow::{Context, Result};
use fontdb::Database;
use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Rect, Size};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule};
use krilla::surface::Surface;
use krilla::text::{Font, TextDirection};
use krilla_svg::{SurfaceExt, SvgSettings};
use std::path::Path as StdPath;
use std::sync::Arc;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

// 16:9 slide dimensions in mm
const SLIDE_W_MM: f32 = 254.0;
const SLIDE_H_MM: f32 = 143.0;

// Layout (all in mm, measured from top of slide)
const MARGIN_X: f32 = 14.0;
const MARGIN_TOP: f32 = 17.0;
const PAGE_BOTTOM_RESERVED: f32 = 10.0; // mm reserved at bottom for page numbers

// Title
const TITLE_FONT_SIZE: f32 = 36.0;
const TITLE_RULE_OFFSET: f32 = 14.0; // mm below title baseline to the rule
const TITLE_CONTENT_GAP: f32 = 8.0;

// Body
const BODY_FONT_SIZE: f32 = 18.0;
const BODY_LINE_HEIGHT: f32 = 9.0;
const BODY_SECTION_GAP: f32 = 5.0;

// Code
const CODE_FONT_SIZE: f32 = 12.0;
const CODE_LINE_HEIGHT: f32 = 6.5;
const CODE_PADDING: f32 = 4.0;
const CODE_BG: u8 = 240; // light gray

// Language icon placed at the top-right corner of the code box
const ICON_SIZE_MM: f32 = 16.0;
const ICON_INSET_MM: f32 = 2.0;

// Conversion
const MM_PER_PT: f32 = 25.4 / 72.0;
const PT_PER_MM: f32 = 72.0 / 25.4;

// Courier glyph advance = 0.6 × em
const COURIER_CHAR_WIDTH_MM: f32 = 0.6 * CODE_FONT_SIZE * MM_PER_PT;

struct Fonts {
    sans: Font,
    sans_bold: Font,
    mono: Font,
}

pub fn export(presentation: &Presentation, output_path: &StdPath) -> Result<()> {
    let fontdb = build_fontdb()?;
    let fonts = load_fonts()?;

    let mut doc = Document::new();
    if let Some(title) = &presentation.title {
        doc.set_metadata(krilla::metadata::Metadata::new().title(title.clone()));
    }
    let syntax_set = SyntaxSet::load_defaults_newlines();
    let theme_set = ThemeSet::load_defaults();
    let theme = &theme_set.themes["InspiredGitHub"];
    let svg_settings = SvgSettings {
        filter_scale: 2.0,
        ..SvgSettings::default()
    };

    let slides = &presentation.slides;
    let total = slides.len();
    let paginate = presentation.paginate.unwrap_or(false);

    if slides.is_empty() {
        // Emit a single blank page so the output file still exists.
        let page_size = Size::from_wh(SLIDE_W_MM * PT_PER_MM, SLIDE_H_MM * PT_PER_MM)
            .context("invalid page size")?;
        let mut page = doc.start_page_with(PageSettings::new(page_size));
        page.surface().finish();
        page.finish();
    }

    for (i, slide) in slides.iter().enumerate() {
        render_slide(
            &mut doc,
            slide,
            paginate,
            i + 1,
            total,
            &fonts,
            &fontdb,
            &svg_settings,
            &syntax_set,
            theme,
        )
        .with_context(|| format!("slide {}/{}", i + 1, total))?;
    }

    let pdf = doc
        .finish()
        .map_err(|e| anyhow::anyhow!("PDF finalisation failed: {e:?}"))?;
    std::fs::write(output_path, pdf)
        .with_context(|| format!("failed to write {}", output_path.display()))?;
    Ok(())
}

/// Load a minimal fontdb — one sans + one mono — and set generic family
/// aliases so SVGs that reference `Arial, sans-serif` / `Courier New,
/// monospace` resolve to the loaded faces.
fn build_fontdb() -> Result<Arc<Database>> {
    let mut db = Database::new();
    let sans = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";
    let mono = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf";
    db.load_font_file(sans)
        .with_context(|| format!("failed to load {sans}"))?;
    db.load_font_file(mono)
        .with_context(|| format!("failed to load {mono}"))?;
    db.set_sans_serif_family("DejaVu Sans");
    db.set_monospace_family("DejaVu Sans Mono");
    db.set_serif_family("DejaVu Sans");
    Ok(Arc::new(db))
}

/// Load the three krilla `Font` values we use directly (for title / body /
/// code text rendering). These are separate from the `fontdb::Database`,
/// which is used only for SVG text resolution.
fn load_fonts() -> Result<Fonts> {
    fn read(path: &str) -> Result<Font> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("failed to read font {path}"))?;
        Font::new(bytes.into(), 0)
            .ok_or_else(|| anyhow::anyhow!("failed to parse font {path}"))
    }
    Ok(Fonts {
        sans: read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")?,
        sans_bold: read("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf")?,
        mono: read("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf")?,
    })
}

#[allow(clippy::too_many_arguments)]
fn render_slide(
    doc: &mut Document,
    slide: &Slide,
    paginate: bool,
    page_num: usize,
    total_pages: usize,
    fonts: &Fonts,
    fontdb: &Arc<Database>,
    svg_settings: &SvgSettings,
    syntax_set: &SyntaxSet,
    theme: &syntect::highlighting::Theme,
) -> Result<()> {
    let page_size = Size::from_wh(SLIDE_W_MM * PT_PER_MM, SLIDE_H_MM * PT_PER_MM)
        .context("invalid page size")?;
    let mut page = doc.start_page_with(PageSettings::new(page_size));
    let mut surface = page.surface();

    let align = slide.align.as_deref().unwrap_or("left");
    let title_align = slide.title_align.as_deref().unwrap_or(align);
    let content_align = slide.content_align.as_deref().unwrap_or(align);
    let valign = slide.valign.as_deref().unwrap_or("top");

    let content_h = content_height(slide);
    let available = SLIDE_H_MM - MARGIN_TOP - PAGE_BOTTOM_RESERVED;
    let mut cursor_y = match valign {
        "middle" => MARGIN_TOP + ((available - content_h).max(0.0)) / 2.0,
        "bottom" => MARGIN_TOP + (available - content_h).max(0.0),
        _ => MARGIN_TOP,
    };

    // Title
    if let Some(title) = &slide.title {
        set_fill(&mut surface, 0, 0, 0);
        let x = text_x(title, TITLE_FONT_SIZE, title_align);
        draw_text_mm(&mut surface, title, TITLE_FONT_SIZE, x, cursor_y, &fonts.sans_bold);
        cursor_y += TITLE_RULE_OFFSET;
        if title_align != "center" {
            draw_hline_mm(&mut surface, MARGIN_X, SLIDE_W_MM - MARGIN_X, cursor_y - 2.0);
        }
        cursor_y += TITLE_CONTENT_GAP;
    }

    // Content
    if let Some(content) = &slide.content {
        set_fill(&mut surface, 0, 0, 0);
        for line in wrap_text(content, 60) {
            let x = text_x(&line, BODY_FONT_SIZE, content_align);
            draw_text_mm(&mut surface, &line, BODY_FONT_SIZE, x, cursor_y, &fonts.sans);
            cursor_y += BODY_LINE_HEIGHT;
        }
        cursor_y += BODY_SECTION_GAP;
    }

    // Bullets + columns, in requested order
    let render_bullets = |surface: &mut Surface<'_>, cy: &mut f32| {
        if let Some(bullets) = &slide.bullets {
            set_fill(surface, 0, 0, 0);
            for bullet in bullets {
                let line = format!("• {}", bullet);
                let x = text_x(&line, BODY_FONT_SIZE, content_align);
                draw_text_mm(surface, &line, BODY_FONT_SIZE, x, *cy, &fonts.sans);
                *cy += BODY_LINE_HEIGHT;
            }
            *cy += BODY_SECTION_GAP;
        }
    };

    if slide.bullets_first {
        render_bullets(&mut surface, &mut cursor_y);
        if let Some(columns) = &slide.columns {
            render_columns(&mut surface, columns, &mut cursor_y, &fonts.sans, &fonts.sans_bold);
        }
    } else {
        if let Some(columns) = &slide.columns {
            render_columns(&mut surface, columns, &mut cursor_y, &fonts.sans, &fonts.sans_bold);
        }
        render_bullets(&mut surface, &mut cursor_y);
    }

    // Code
    if let Some(code) = &slide.code {
        render_code_block(
            &mut surface,
            &code.source,
            code.language.as_deref(),
            code.trim,
            &mut cursor_y,
            &fonts.mono,
            syntax_set,
            theme,
            fontdb,
            svg_settings,
        );
    }

    // SVG — inline `svg` takes precedence over `image`
    let svg_source: Option<String> = if let Some(inline) = &slide.svg {
        Some(inline.clone())
    } else if let Some(path) = &slide.image {
        if !path.to_lowercase().ends_with(".svg") {
            anyhow::bail!(
                "image path {:?} is not an .svg (raster images not yet supported)",
                path
            );
        }
        Some(
            std::fs::read_to_string(path)
                .with_context(|| format!("failed to read SVG file {:?}", path))?,
        )
    } else {
        None
    };

    if let Some(svg_str) = svg_source {
        render_svg(&mut surface, &svg_str, &mut cursor_y, fontdb, svg_settings)?;
    }

    // Page number
    if paginate {
        set_fill(&mut surface, 0, 0, 0);
        let label = format!("{} / {}", page_num, total_pages);
        let x = SLIDE_W_MM - MARGIN_X - 12.0;
        let y = SLIDE_H_MM - 4.0;
        draw_text_mm(&mut surface, &label, 9.0, x, y, &fonts.sans);
    }

    surface.finish();
    page.finish();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_code_block(
    surface: &mut Surface<'_>,
    source: &str,
    language: Option<&str>,
    trim: bool,
    cursor_y: &mut f32,
    font_mono: &Font,
    syntax_set: &SyntaxSet,
    theme: &syntect::highlighting::Theme,
    fontdb: &Arc<Database>,
    svg_settings: &SvgSettings,
) {
    let source: &str = if trim {
        source.trim_end_matches('\n').trim_end_matches('\r')
    } else {
        source
    };
    let line_count = LinesWithEndings::from(source).count();
    if line_count == 0 {
        return;
    }

    // Background rectangle. Top = first line baseline - padding (above first line);
    // bottom = last line baseline + padding.
    let box_top = *cursor_y - CODE_PADDING;
    let box_bottom = *cursor_y + ((line_count - 1) as f32 * CODE_LINE_HEIGHT) + CODE_PADDING;
    set_fill(surface, CODE_BG, CODE_BG, CODE_BG);
    draw_rect_mm(
        surface,
        MARGIN_X - CODE_PADDING,
        box_top,
        SLIDE_W_MM - MARGIN_X + CODE_PADDING,
        box_bottom,
    );

    // Language icon at the top-right corner of the background box.
    if let Some(lang) = language
        && let Some(icon_svg) = crate::assets::language_icon(lang) {
            let icon_right = SLIDE_W_MM - MARGIN_X + CODE_PADDING - ICON_INSET_MM;
            let icon_left = icon_right - ICON_SIZE_MM;
            let icon_top = box_top + ICON_INSET_MM;
            draw_svg_fixed_box(
                surface,
                icon_svg,
                icon_left,
                icon_top,
                ICON_SIZE_MM,
                ICON_SIZE_MM,
                fontdb,
                svg_settings,
            );
        }

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
            set_fill(surface, fg.r, fg.g, fg.b);
            draw_text_mm(surface, text, CODE_FONT_SIZE, x, *cursor_y, font_mono);
            x += text.chars().count() as f32 * COURIER_CHAR_WIDTH_MM;
        }
        *cursor_y += CODE_LINE_HEIGHT;
    }
    *cursor_y = box_bottom + BODY_SECTION_GAP;
}

/// Draw an SVG into a fixed (x, y, width, height) mm box, preserving aspect
/// ratio and centering within the box. Used for small fixed-size glyphs like
/// language icons — never fails; on parse error the icon is silently skipped
/// (bundled icons are static and should never fail, but we don't want an
/// icon bug to block an otherwise-valid slide).
#[allow(clippy::too_many_arguments)]
fn draw_svg_fixed_box(
    surface: &mut Surface<'_>,
    svg_str: &str,
    x_mm: f32,
    y_mm: f32,
    w_mm: f32,
    h_mm: f32,
    fontdb: &Arc<Database>,
    settings: &SvgSettings,
) {
    let opts = usvg::Options {
        fontdb: fontdb.clone(),
        font_family: "sans-serif".into(),
        ..Default::default()
    };
    let tree = match usvg::Tree::from_data(svg_str.as_bytes(), &opts) {
        Ok(t) => t,
        Err(_) => return,
    };
    let natural_w = tree.size().width();
    let natural_h = tree.size().height();
    if natural_w <= 0.0 || natural_h <= 0.0 {
        return;
    }

    let box_w_pt = w_mm * PT_PER_MM;
    let box_h_pt = h_mm * PT_PER_MM;
    let scale = (box_w_pt / natural_w).min(box_h_pt / natural_h);
    let rendered_w_pt = natural_w * scale;
    let rendered_h_pt = natural_h * scale;

    let tx = x_mm * PT_PER_MM + (box_w_pt - rendered_w_pt) * 0.5;
    let ty = y_mm * PT_PER_MM + (box_h_pt - rendered_h_pt) * 0.5;
    let tr = krilla::geom::Transform::from_row(scale, 0.0, 0.0, scale, tx, ty);
    surface.push_transform(&tr);
    if let Some(size) = Size::from_wh(natural_w, natural_h) {
        let _ = surface.draw_svg(&tree, size, *settings);
    }
    surface.pop();
}

fn render_svg(
    surface: &mut Surface<'_>,
    svg_str: &str,
    cursor_y: &mut f32,
    fontdb: &Arc<Database>,
    settings: &SvgSettings,
) -> Result<()> {
    let opts = usvg::Options {
        fontdb: fontdb.clone(),
        font_family: "sans-serif".into(),
        ..Default::default()
    };
    let tree = usvg::Tree::from_data(svg_str.as_bytes(), &opts)
        .map_err(|e| anyhow::anyhow!("failed to parse SVG: {e}"))?;

    let natural_w = tree.size().width();
    let natural_h = tree.size().height();
    if natural_w <= 0.0 || natural_h <= 0.0 {
        anyhow::bail!("SVG has non-positive dimensions: {natural_w}x{natural_h}");
    }

    // Fit to available slide width, preserving aspect; never upscale.
    let available_w_pt = (SLIDE_W_MM - 2.0 * MARGIN_X) * PT_PER_MM;
    let scale = (available_w_pt / natural_w).min(1.0);
    let rendered_w_pt = natural_w * scale;
    let rendered_h_pt = natural_h * scale;

    // Translate to (MARGIN_X, cursor_y) in page coords (top-left of SVG).
    let tx = MARGIN_X * PT_PER_MM;
    let ty = *cursor_y * PT_PER_MM;
    let tr = krilla::geom::Transform::from_row(scale, 0.0, 0.0, scale, tx, ty);
    surface.push_transform(&tr);
    let size = Size::from_wh(natural_w, natural_h).context("invalid svg size")?;
    surface
        .draw_svg(&tree, size, *settings)
        .ok_or_else(|| anyhow::anyhow!("krilla-svg draw_svg returned None"))?;
    surface.pop();

    *cursor_y += rendered_h_pt * MM_PER_PT + BODY_SECTION_GAP;
    // Silence unused — kept for future overlay logic.
    let _ = rendered_w_pt;
    Ok(())
}

fn render_columns(
    surface: &mut Surface<'_>,
    columns: &[crate::model::Column],
    cursor_y: &mut f32,
    font: &Font,
    font_bold: &Font,
) {
    let n = columns.len();
    if n == 0 {
        return;
    }
    let gutter = 4.0;
    let total_width = SLIDE_W_MM - 2.0 * MARGIN_X - gutter * (n - 1) as f32;
    let col_width = total_width / n as f32;

    let start_y = *cursor_y;
    let mut max_y = start_y;
    for (i, col) in columns.iter().enumerate() {
        let mut cy = start_y;
        let x = MARGIN_X + i as f32 * (col_width + gutter);
        if let Some(header) = &col.header {
            set_fill(surface, 0, 0, 0);
            draw_text_mm(surface, header, BODY_FONT_SIZE + 2.0, x, cy, font_bold);
            cy += BODY_LINE_HEIGHT + 2.0;
        }
        set_fill(surface, 0, 0, 0);
        for bullet in &col.bullets {
            let line = format!("• {}", bullet);
            draw_text_mm(surface, &line, BODY_FONT_SIZE, x, cy, font);
            cy += BODY_LINE_HEIGHT;
        }
        if cy > max_y {
            max_y = cy;
        }
    }
    *cursor_y = max_y + BODY_SECTION_GAP;
}

// ── primitives ────────────────────────────────────────────────────────────

fn set_fill(surface: &mut Surface<'_>, r: u8, g: u8, b: u8) {
    surface.set_fill(Some(Fill {
        paint: rgb::Color::new(r, g, b).into(),
        opacity: NormalizedF32::ONE,
        rule: FillRule::default(),
    }));
}

fn draw_text_mm(surface: &mut Surface<'_>, text: &str, size: f32, x_mm: f32, y_mm: f32, font: &Font) {
    let x = x_mm * PT_PER_MM;
    let y = y_mm * PT_PER_MM;
    // y_mm is measured from the TOP of the slide down to the text baseline,
    // which matches krilla's native coord system (y grows downward). We need
    // to offset by the font ascent so the text sits visually at y_mm.
    surface.draw_text(
        Point::from_xy(x, y + size * 0.8),
        font.clone(),
        size,
        text,
        false,
        TextDirection::Auto,
    );
}

fn draw_rect_mm(surface: &mut Surface<'_>, left: f32, top: f32, right: f32, bottom: f32) {
    let rect = Rect::from_ltrb(
        left * PT_PER_MM,
        top * PT_PER_MM,
        right * PT_PER_MM,
        bottom * PT_PER_MM,
    );
    let Some(rect) = rect else {
        return;
    };
    let mut pb = PathBuilder::new();
    pb.push_rect(rect);
    if let Some(path) = pb.finish() {
        surface.draw_path(&path);
    }
}

fn draw_hline_mm(surface: &mut Surface<'_>, x1: f32, x2: f32, y: f32) {
    // Draw a thin filled rectangle as a line (krilla requires a Stroke with
    // explicit width, simpler to use a filled rect of height 0.3mm).
    set_fill(surface, 0, 0, 0);
    draw_rect_mm(surface, x1, y - 0.15, x2, y + 0.15);
}

// ── layout helpers ────────────────────────────────────────────────────────

fn content_height(slide: &Slide) -> f32 {
    let mut h = 0.0;
    if slide.title.is_some() {
        h += TITLE_RULE_OFFSET + TITLE_CONTENT_GAP;
    }
    if let Some(content) = &slide.content {
        let n = wrap_text(content, 60).len() as f32;
        h += n * BODY_LINE_HEIGHT + BODY_SECTION_GAP;
    }
    if let Some(columns) = &slide.columns {
        let max_lines = columns
            .iter()
            .map(|col| {
                let header_lines = if col.header.is_some() { 1.0 } else { 0.0 };
                header_lines + col.bullets.len() as f32
            })
            .fold(0.0_f32, f32::max);
        h += max_lines * BODY_LINE_HEIGHT + BODY_SECTION_GAP;
    }
    if let Some(bullets) = &slide.bullets {
        h += bullets.len() as f32 * BODY_LINE_HEIGHT + BODY_SECTION_GAP;
    }
    if let Some(code) = &slide.code {
        let src = if code.trim {
            code.source.trim_end_matches('\n').trim_end_matches('\r')
        } else {
            code.source.as_str()
        };
        let n = LinesWithEndings::from(src).count() as f32;
        h += (n - 1.0).max(0.0) * CODE_LINE_HEIGHT + 2.0 * CODE_PADDING;
    }
    h
}

fn text_x(text: &str, font_size: f32, align: &str) -> f32 {
    match align {
        "center" => {
            let w = text.chars().count() as f32 * 0.5 * font_size * MM_PER_PT;
            (SLIDE_W_MM / 2.0 - w / 2.0).max(MARGIN_X)
        }
        "right" => {
            let w = text.chars().count() as f32 * 0.5 * font_size * MM_PER_PT;
            (SLIDE_W_MM - MARGIN_X - w).max(MARGIN_X)
        }
        _ => MARGIN_X,
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_text_short_fits_one_line() {
        assert_eq!(wrap_text("hello world", 20), vec!["hello world"]);
    }

    #[test]
    fn wrap_text_breaks_at_max_chars() {
        assert_eq!(
            wrap_text("one two three four five", 10),
            vec!["one two", "three four", "five"]
        );
    }

    #[test]
    fn wrap_text_empty_input() {
        assert!(wrap_text("", 20).is_empty());
    }

    #[test]
    fn wrap_text_single_word_longer_than_limit() {
        assert_eq!(
            wrap_text("superlongwordthatexceedslimit", 10),
            vec!["superlongwordthatexceedslimit"]
        );
    }

    #[test]
    fn wrap_text_preserves_paragraph_breaks() {
        assert_eq!(
            wrap_text("line one\nline two", 40),
            vec!["line one", "line two"]
        );
    }

    #[test]
    fn text_x_left_returns_margin() {
        assert_eq!(text_x("anything", 18.0, "left"), MARGIN_X);
        assert_eq!(text_x("anything", 18.0, "unknown"), MARGIN_X);
    }

    #[test]
    fn text_x_center_is_between_margins() {
        let x = text_x("Hello", 18.0, "center");
        assert!(x >= MARGIN_X);
        assert!(x < SLIDE_W_MM / 2.0 + 1.0);
    }

    #[test]
    fn text_x_right_is_greater_than_center() {
        let center = text_x("Hello", 18.0, "center");
        let right = text_x("Hello", 18.0, "right");
        assert!(right > center);
    }

    #[test]
    fn text_x_never_below_margin() {
        let long: String = "a".repeat(200);
        for align in &["left", "center", "right"] {
            assert!(text_x(&long, 18.0, align) >= MARGIN_X);
        }
    }

    fn empty_slide() -> Slide {
        Slide {
            title: None,
            content: None,
            bullets: None,
            code: None,
            image: None,
            svg: None,
            class: None,
            background: None,
            align: None,
            title_align: None,
            content_align: None,
            valign: None,
            columns: None,
            bullets_first: false,
        }
    }

    #[test]
    fn content_height_empty_slide_is_zero() {
        assert_eq!(content_height(&empty_slide()), 0.0);
    }

    #[test]
    fn content_height_title_only() {
        let mut s = empty_slide();
        s.title = Some("Hello".into());
        assert_eq!(content_height(&s), TITLE_RULE_OFFSET + TITLE_CONTENT_GAP);
    }

    #[test]
    fn content_height_bullets_add_lines() {
        let mut s = empty_slide();
        s.bullets = Some(vec!["a".into(), "b".into(), "c".into()]);
        let expected = 3.0 * BODY_LINE_HEIGHT + BODY_SECTION_GAP;
        assert!((content_height(&s) - expected).abs() < 0.001);
    }
}
