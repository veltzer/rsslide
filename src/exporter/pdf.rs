use crate::config::{Color, Config};
use crate::model::{Presentation, Slide, Subtitle, Table, TableAlign};
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

// Unit conversion — hardcoded; these never change.
const MM_PER_PT: f32 = 25.4 / 72.0;
const PT_PER_MM: f32 = 72.0 / 25.4;

struct Fonts {
    title: Font,
    body: Font,
    code: Font,
}

pub fn export(presentation: &Presentation, output_path: &StdPath, cfg: &Config) -> Result<()> {
    let fontdb = build_fontdb(cfg)?;
    let fonts = load_fonts(cfg)?;

    let mut doc = Document::new();
    if let Some(title) = &presentation.title {
        doc.set_metadata(krilla::metadata::Metadata::new().title(title.clone()));
    }
    let syntax_set = SyntaxSet::load_defaults_newlines();
    let theme_set = ThemeSet::load_defaults();
    let theme = &theme_set.themes["InspiredGitHub"];
    let svg_settings = SvgSettings {
        filter_scale: cfg.svg.filter_scale,
        ..SvgSettings::default()
    };

    let slides = &presentation.slides;
    let total = slides.len();
    let paginate = presentation.paginate.unwrap_or(false);

    if slides.is_empty() {
        let page_size =
            Size::from_wh(cfg.slide.width_mm * PT_PER_MM, cfg.slide.height_mm * PT_PER_MM)
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
            cfg,
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

fn build_fontdb(cfg: &Config) -> Result<Arc<Database>> {
    let mut db = Database::new();
    // Load the body and code fonts. The title font is loaded separately
    // (only used as a krilla Font, not for usvg resolution).
    db.load_font_file(&cfg.fonts.body)
        .with_context(|| format!("failed to load body font {}", cfg.fonts.body.display()))?;
    db.load_font_file(&cfg.fonts.code)
        .with_context(|| format!("failed to load code font {}", cfg.fonts.code.display()))?;
    // Use the first loaded face's family for generic aliases. Matters only
    // for SVGs that request "sans-serif" / "monospace" by family.
    let body_family = db
        .faces()
        .next()
        .and_then(|f| f.families.first().map(|(n, _)| n.clone()))
        .unwrap_or_else(|| "DejaVu Sans".into());
    let code_family = db
        .faces()
        .find(|f| f.monospaced)
        .and_then(|f| f.families.first().map(|(n, _)| n.clone()))
        .unwrap_or_else(|| "DejaVu Sans Mono".into());
    db.set_sans_serif_family(&body_family);
    db.set_serif_family(&body_family);
    db.set_monospace_family(&code_family);
    Ok(Arc::new(db))
}

fn load_fonts(cfg: &Config) -> Result<Fonts> {
    fn read(path: &StdPath) -> Result<Font> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("failed to read font {}", path.display()))?;
        Font::new(bytes.into(), 0)
            .ok_or_else(|| anyhow::anyhow!("failed to parse font {}", path.display()))
    }
    Ok(Fonts {
        title: read(&cfg.fonts.title)?,
        body: read(&cfg.fonts.body)?,
        code: read(&cfg.fonts.code)?,
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
    cfg: &Config,
) -> Result<()> {
    let page_size =
        Size::from_wh(cfg.slide.width_mm * PT_PER_MM, cfg.slide.height_mm * PT_PER_MM)
            .context("invalid page size")?;
    let mut page = doc.start_page_with(PageSettings::new(page_size));
    let mut surface = page.surface();

    let align = slide.align.as_deref().unwrap_or("left");
    let title_align = slide.title_align.as_deref().unwrap_or(align);
    let content_align = slide.content_align.as_deref().unwrap_or(align);
    let valign = slide.valign.as_deref().unwrap_or("top");

    let content_h = content_height(slide, cfg);
    let available = cfg.slide.height_mm - cfg.slide.margin_top_mm - cfg.slide.page_bottom_reserved_mm;
    let mut cursor_y = match valign {
        "middle" => cfg.slide.margin_top_mm + ((available - content_h).max(0.0)) / 2.0,
        "bottom" => cfg.slide.margin_top_mm + (available - content_h).max(0.0),
        _ => cfg.slide.margin_top_mm,
    };

    // Title
    if let Some(title) = &slide.title {
        set_fill(&mut surface, cfg.colors.text);
        let x = text_x(title, cfg.title.font_size_pt, title_align, cfg);
        draw_text_mm(&mut surface, title, cfg.title.font_size_pt, x, cursor_y, &fonts.title);
        cursor_y += cfg.title.rule_offset_mm;
        if title_align != "center" {
            set_fill(&mut surface, cfg.colors.title_rule);
            draw_rect_mm(
                &mut surface,
                cfg.slide.margin_x_mm,
                cursor_y - 2.0 - 0.15,
                cfg.slide.width_mm - cfg.slide.margin_x_mm,
                cursor_y - 2.0 + 0.15,
            );
        }
        cursor_y += cfg.title.content_gap_mm;
    }

    // Subtitle (rendered before any other content)
    if let Some(sub) = &slide.subtitle {
        let size_pt = subtitle_font_size(sub, cfg);
        set_fill(&mut surface, cfg.colors.text);
        let x = text_x(&sub.text, size_pt, content_align, cfg);
        draw_text_mm(&mut surface, &sub.text, size_pt, x, cursor_y, &fonts.title);
        cursor_y += size_pt * MM_PER_PT + cfg.subtitle.gap_below_mm;
    }

    // Content
    if let Some(content) = &slide.content {
        set_fill(&mut surface, cfg.colors.text);
        for line in wrap_text(content, 60) {
            let x = text_x(&line, cfg.body.font_size_pt, content_align, cfg);
            draw_text_mm(&mut surface, &line, cfg.body.font_size_pt, x, cursor_y, &fonts.body);
            cursor_y += cfg.body.line_height_mm;
        }
        cursor_y += cfg.body.section_gap_mm;
    }

    // Bullets + columns, in requested order
    let render_bullets = |surface: &mut Surface<'_>, cy: &mut f32| {
        if let Some(bullets) = &slide.bullets {
            for bullet in bullets {
                let x = text_x(bullet, cfg.body.font_size_pt, content_align, cfg);
                draw_bullet_line(surface, bullet, x, *cy, &fonts.body, cfg);
                *cy += cfg.body.line_height_mm;
            }
            *cy += cfg.body.section_gap_mm;
        }
    };

    if slide.bullets_first {
        render_bullets(&mut surface, &mut cursor_y);
        if let Some(columns) = &slide.columns {
            render_columns(&mut surface, columns, &mut cursor_y, fonts, cfg);
        }
    } else {
        if let Some(columns) = &slide.columns {
            render_columns(&mut surface, columns, &mut cursor_y, fonts, cfg);
        }
        render_bullets(&mut surface, &mut cursor_y);
    }

    // Table
    if let Some(table) = &slide.table {
        render_table(&mut surface, table, &mut cursor_y, fonts, cfg)?;
    }

    // Code
    if let Some(code) = &slide.code {
        render_code_block(
            &mut surface,
            &code.source,
            code.language.as_deref(),
            code.trim,
            &mut cursor_y,
            &fonts.code,
            syntax_set,
            theme,
            fontdb,
            svg_settings,
            cfg,
        );
    }

    // SVG — inline takes precedence over file path
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
        render_svg(&mut surface, &svg_str, &mut cursor_y, fontdb, svg_settings, cfg)?;
    }

    // Page number
    if paginate {
        set_fill(&mut surface, cfg.colors.text);
        let label = format!("{} / {}", page_num, total_pages);
        let x = cfg.slide.width_mm - cfg.slide.margin_x_mm - 12.0;
        let y = cfg.slide.height_mm - 4.0;
        draw_text_mm(&mut surface, &label, 9.0, x, y, &fonts.body);
    }

    surface.finish();
    page.finish();
    Ok(())
}

/// Draw one bullet: the `•` glyph in `cfg.colors.bullet`, then the text in
/// `cfg.colors.text`. Two draw calls because each call can only use one fill.
fn draw_bullet_line(
    surface: &mut Surface<'_>,
    text: &str,
    x_mm: f32,
    y_mm: f32,
    font: &Font,
    cfg: &Config,
) {
    set_fill(surface, cfg.colors.bullet);
    draw_text_mm(surface, "•", cfg.body.font_size_pt, x_mm, y_mm, font);
    // Approximate advance of "• " at this font size. Using 0.4 × em is a
    // close match for DejaVu Sans and avoids needing real glyph metrics.
    let offset = cfg.body.font_size_pt * MM_PER_PT * 0.8;
    set_fill(surface, cfg.colors.text);
    draw_text_mm(surface, text, cfg.body.font_size_pt, x_mm + offset, y_mm, font);
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
    cfg: &Config,
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
    // Courier glyph advance = 0.6 × em at the configured size.
    let char_width_mm = 0.6 * cfg.code.font_size_pt * MM_PER_PT;

    let box_top = *cursor_y - cfg.code.padding_mm;
    let box_bottom =
        *cursor_y + ((line_count - 1) as f32 * cfg.code.line_height_mm) + cfg.code.padding_mm;
    set_fill(surface, cfg.colors.code_background);
    draw_rect_mm(
        surface,
        cfg.slide.margin_x_mm - cfg.code.padding_mm,
        box_top,
        cfg.slide.width_mm - cfg.slide.margin_x_mm + cfg.code.padding_mm,
        box_bottom,
    );

    // Language icon at the top-right corner of the background box.
    if let Some(lang) = language
        && let Some(icon_svg) = crate::assets::language_icon(lang)
    {
        let icon_right =
            cfg.slide.width_mm - cfg.slide.margin_x_mm + cfg.code.padding_mm - cfg.code.icon_inset_mm;
        let icon_left = icon_right - cfg.code.icon_size_mm;
        let icon_top = box_top + cfg.code.icon_inset_mm;
        draw_svg_fixed_box(
            surface,
            icon_svg,
            icon_left,
            icon_top,
            cfg.code.icon_size_mm,
            cfg.code.icon_size_mm,
            fontdb,
            svg_settings,
            cfg,
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
        let mut x = cfg.slide.margin_x_mm + 2.0;
        for (style, token) in ranges {
            let text = token.trim_end_matches('\n').trim_end_matches('\r');
            if text.is_empty() {
                continue;
            }
            let fg = style.foreground;
            set_fill(surface, Color(fg.r, fg.g, fg.b));
            draw_text_mm(surface, text, cfg.code.font_size_pt, x, *cursor_y, font_mono);
            x += text.chars().count() as f32 * char_width_mm;
        }
        *cursor_y += cfg.code.line_height_mm;
    }
    *cursor_y = box_bottom + cfg.body.section_gap_mm;
}

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
    cfg: &Config,
) {
    let opts = usvg::Options {
        fontdb: fontdb.clone(),
        font_family: "sans-serif".into(),
        ..Default::default()
    };
    let flattened;
    let svg_bytes: &[u8] = if cfg.svg.flatten_css_vars {
        flattened = crate::utils::css_vars::flatten(svg_str);
        flattened.as_bytes()
    } else {
        svg_str.as_bytes()
    };
    let tree = match usvg::Tree::from_data(svg_bytes, &opts) {
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
    cfg: &Config,
) -> Result<()> {
    let opts = usvg::Options {
        fontdb: fontdb.clone(),
        font_family: "sans-serif".into(),
        ..Default::default()
    };
    let flattened;
    let svg_bytes: &[u8] = if cfg.svg.flatten_css_vars {
        flattened = crate::utils::css_vars::flatten(svg_str);
        flattened.as_bytes()
    } else {
        svg_str.as_bytes()
    };
    let tree = usvg::Tree::from_data(svg_bytes, &opts)
        .map_err(|e| anyhow::anyhow!("failed to parse SVG: {e}"))?;

    let natural_w = tree.size().width();
    let natural_h = tree.size().height();
    if natural_w <= 0.0 || natural_h <= 0.0 {
        anyhow::bail!("SVG has non-positive dimensions: {natural_w}x{natural_h}");
    }

    // Push the SVG down a bit below the preceding content.
    *cursor_y += cfg.svg.top_gap_mm;

    // Fit to both remaining width (using the SVG-specific side margin) and
    // remaining height; centre horizontally.
    let available_w_pt = (cfg.slide.width_mm - 2.0 * cfg.svg.margin_x_mm) * PT_PER_MM;
    let available_h_pt =
        (cfg.slide.height_mm - cfg.slide.page_bottom_reserved_mm - *cursor_y) * PT_PER_MM;
    let scale = (available_w_pt / natural_w).min(available_h_pt / natural_h);
    let rendered_w_pt = natural_w * scale;
    let rendered_h_pt = natural_h * scale;

    let tx = (cfg.svg.margin_x_mm * PT_PER_MM) + (available_w_pt - rendered_w_pt) * 0.5;
    let ty = *cursor_y * PT_PER_MM;
    let tr = krilla::geom::Transform::from_row(scale, 0.0, 0.0, scale, tx, ty);
    surface.push_transform(&tr);
    let size = Size::from_wh(natural_w, natural_h).context("invalid svg size")?;
    surface
        .draw_svg(&tree, size, *settings)
        .ok_or_else(|| anyhow::anyhow!("krilla-svg draw_svg returned None"))?;
    surface.pop();

    *cursor_y += rendered_h_pt * MM_PER_PT + cfg.body.section_gap_mm;
    Ok(())
}

fn render_columns(
    surface: &mut Surface<'_>,
    columns: &[crate::model::Column],
    cursor_y: &mut f32,
    fonts: &Fonts,
    cfg: &Config,
) {
    let n = columns.len();
    if n == 0 {
        return;
    }
    let gutter = 4.0;
    let total_width =
        cfg.slide.width_mm - 2.0 * cfg.slide.margin_x_mm - gutter * (n - 1) as f32;
    let col_width = total_width / n as f32;

    let start_y = *cursor_y;
    let mut max_y = start_y;
    for (i, col) in columns.iter().enumerate() {
        let mut cy = start_y;
        let x = cfg.slide.margin_x_mm + i as f32 * (col_width + gutter);
        if let Some(header) = &col.header {
            set_fill(surface, cfg.colors.text);
            draw_text_mm(surface, header, cfg.body.font_size_pt + 2.0, x, cy, &fonts.title);
            cy += cfg.body.line_height_mm + 2.0;
        }
        for bullet in &col.bullets {
            draw_bullet_line(surface, bullet, x, cy, &fonts.body, cfg);
            cy += cfg.body.line_height_mm;
        }
        if cy > max_y {
            max_y = cy;
        }
    }
    *cursor_y = max_y + cfg.body.section_gap_mm;
}

fn render_table(
    surface: &mut Surface<'_>,
    table: &Table,
    cursor_y: &mut f32,
    fonts: &Fonts,
    cfg: &Config,
) -> Result<()> {
    let n = table.headers.len();
    let total_width = cfg.slide.width_mm - 2.0 * cfg.slide.margin_x_mm;
    let pad = cfg.table.cell_padding_mm;

    // Estimated glyph advance for a proportional sans font: 0.5 × em.
    let header_char_mm = 0.5 * cfg.table.header_font_size_pt * MM_PER_PT;
    let cell_char_mm = 0.5 * cfg.table.cell_font_size_pt * MM_PER_PT;

    // Per-column natural width (max of header + cells, in mm) including padding.
    let mut col_widths = vec![0.0_f32; n];
    for (c, h) in table.headers.iter().enumerate() {
        let w = h.chars().count() as f32 * header_char_mm + 2.0 * pad;
        if w > col_widths[c] {
            col_widths[c] = w;
        }
    }
    for row in &table.rows {
        for (c, cell) in row.iter().enumerate() {
            let w = cell.chars().count() as f32 * cell_char_mm + 2.0 * pad;
            if w > col_widths[c] {
                col_widths[c] = w;
            }
        }
    }
    let natural_total: f32 = col_widths.iter().sum();
    if natural_total > total_width {
        anyhow::bail!(
            "table is too wide for the slide: natural width {natural_total:.1}mm > available {total_width:.1}mm \
             (headers: {:?})",
            table.headers
        );
    }
    // Distribute leftover space proportionally so the table spans the full
    // content width — keeps borders flush with the slide margins.
    let leftover = total_width - natural_total;
    if natural_total > 0.0 {
        for w in &mut col_widths {
            *w += leftover * (*w / natural_total);
        }
    }

    let row_h = cfg.table.row_height_mm;
    let total_rows = 1 + table.rows.len();
    let table_top = *cursor_y;
    let table_bottom = table_top + total_rows as f32 * row_h;
    let table_left = cfg.slide.margin_x_mm;

    // Column x-offsets (left edge of each column).
    let mut col_lefts = Vec::with_capacity(n + 1);
    col_lefts.push(table_left);
    for w in &col_widths {
        col_lefts.push(col_lefts.last().copied().unwrap() + *w);
    }

    // Borders: horizontal rules between rows, vertical rules between columns.
    set_fill(surface, cfg.colors.title_rule);
    let bw = cfg.table.border_width_mm;
    for r in 0..=total_rows {
        let y = table_top + r as f32 * row_h;
        draw_rect_mm(
            surface,
            table_left,
            y - bw / 2.0,
            *col_lefts.last().unwrap(),
            y + bw / 2.0,
        );
    }
    for x in &col_lefts {
        draw_rect_mm(
            surface,
            *x - bw / 2.0,
            table_top,
            *x + bw / 2.0,
            table_bottom,
        );
    }

    // Header row.
    for (c, h) in table.headers.iter().enumerate() {
        let cell_w = col_widths[c];
        let text_w = h.chars().count() as f32 * header_char_mm;
        if text_w > cell_w - 2.0 * pad {
            anyhow::bail!(
                "table header cell {:?} (col {}) width {:.1}mm exceeds column width {:.1}mm",
                h, c, text_w, cell_w - 2.0 * pad
            );
        }
        let x = align_x(col_lefts[c], cell_w, text_w, pad, table.aligns[c]);
        let y = table_top + (row_h - cfg.table.header_font_size_pt * MM_PER_PT) / 2.0;
        set_fill(surface, cfg.colors.text);
        draw_text_mm(surface, h, cfg.table.header_font_size_pt, x, y, &fonts.title);
    }

    // Body rows.
    for (r, row) in table.rows.iter().enumerate() {
        let y_top = table_top + (r + 1) as f32 * row_h;
        for (c, cell) in row.iter().enumerate() {
            let cell_w = col_widths[c];
            let text_w = cell.chars().count() as f32 * cell_char_mm;
            if text_w > cell_w - 2.0 * pad {
                anyhow::bail!(
                    "table cell {:?} at row {} col {} width {:.1}mm exceeds column width {:.1}mm",
                    cell, r, c, text_w, cell_w - 2.0 * pad
                );
            }
            let x = align_x(col_lefts[c], cell_w, text_w, pad, table.aligns[c]);
            let y = y_top + (row_h - cfg.table.cell_font_size_pt * MM_PER_PT) / 2.0;
            set_fill(surface, cfg.colors.text);
            draw_text_mm(surface, cell, cfg.table.cell_font_size_pt, x, y, &fonts.body);
        }
    }

    *cursor_y = table_bottom + cfg.body.section_gap_mm;
    Ok(())
}

fn align_x(col_left: f32, cell_w: f32, text_w: f32, pad: f32, align: TableAlign) -> f32 {
    match align {
        TableAlign::Left => col_left + pad,
        TableAlign::Right => col_left + cell_w - pad - text_w,
        TableAlign::Center => col_left + (cell_w - text_w) / 2.0,
    }
}

// ── primitives ────────────────────────────────────────────────────────────

fn set_fill(surface: &mut Surface<'_>, c: Color) {
    let (r, g, b) = c.rgb();
    surface.set_fill(Some(Fill {
        paint: rgb::Color::new(r, g, b).into(),
        opacity: NormalizedF32::ONE,
        rule: FillRule::default(),
    }));
}

fn draw_text_mm(surface: &mut Surface<'_>, text: &str, size: f32, x_mm: f32, y_mm: f32, font: &Font) {
    let x = x_mm * PT_PER_MM;
    let y = y_mm * PT_PER_MM;
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
    let Some(rect) = Rect::from_ltrb(
        left * PT_PER_MM,
        top * PT_PER_MM,
        right * PT_PER_MM,
        bottom * PT_PER_MM,
    ) else {
        return;
    };
    let mut pb = PathBuilder::new();
    pb.push_rect(rect);
    if let Some(path) = pb.finish() {
        surface.draw_path(&path);
    }
}

// ── layout helpers ────────────────────────────────────────────────────────

fn content_height(slide: &Slide, cfg: &Config) -> f32 {
    let mut h = 0.0;
    if slide.title.is_some() {
        h += cfg.title.rule_offset_mm + cfg.title.content_gap_mm;
    }
    if let Some(sub) = &slide.subtitle {
        h += subtitle_font_size(sub, cfg) * MM_PER_PT + cfg.subtitle.gap_below_mm;
    }
    if let Some(content) = &slide.content {
        let n = wrap_text(content, 60).len() as f32;
        h += n * cfg.body.line_height_mm + cfg.body.section_gap_mm;
    }
    if let Some(columns) = &slide.columns {
        let max_lines = columns
            .iter()
            .map(|col| {
                let header_lines = if col.header.is_some() { 1.0 } else { 0.0 };
                header_lines + col.bullets.len() as f32
            })
            .fold(0.0_f32, f32::max);
        h += max_lines * cfg.body.line_height_mm + cfg.body.section_gap_mm;
    }
    if let Some(bullets) = &slide.bullets {
        h += bullets.len() as f32 * cfg.body.line_height_mm + cfg.body.section_gap_mm;
    }
    if let Some(code) = &slide.code {
        let src = if code.trim {
            code.source.trim_end_matches('\n').trim_end_matches('\r')
        } else {
            code.source.as_str()
        };
        let n = LinesWithEndings::from(src).count() as f32;
        h += (n - 1.0).max(0.0) * cfg.code.line_height_mm + 2.0 * cfg.code.padding_mm;
    }
    if let Some(table) = &slide.table {
        let rows = 1 + table.rows.len();
        h += rows as f32 * cfg.table.row_height_mm + cfg.body.section_gap_mm;
    }
    h
}

fn subtitle_font_size(sub: &Subtitle, cfg: &Config) -> f32 {
    // Validated to 2..=6 by the deserializer.
    cfg.subtitle.font_sizes_pt[(sub.level - 2) as usize]
}

fn text_x(text: &str, font_size: f32, align: &str, cfg: &Config) -> f32 {
    let margin = cfg.slide.margin_x_mm;
    let w_mm = text.chars().count() as f32 * 0.5 * font_size * MM_PER_PT;
    match align {
        "center" => (cfg.slide.width_mm / 2.0 - w_mm / 2.0).max(margin),
        "right" => (cfg.slide.width_mm - margin - w_mm).max(margin),
        _ => margin,
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
        let cfg = Config::default();
        assert_eq!(text_x("anything", 18.0, "left", &cfg), cfg.slide.margin_x_mm);
        assert_eq!(text_x("anything", 18.0, "unknown", &cfg), cfg.slide.margin_x_mm);
    }

    #[test]
    fn text_x_center_is_between_margins() {
        let cfg = Config::default();
        let x = text_x("Hello", 18.0, "center", &cfg);
        assert!(x >= cfg.slide.margin_x_mm);
        assert!(x < cfg.slide.width_mm / 2.0 + 1.0);
    }

    #[test]
    fn text_x_right_is_greater_than_center() {
        let cfg = Config::default();
        let center = text_x("Hello", 18.0, "center", &cfg);
        let right = text_x("Hello", 18.0, "right", &cfg);
        assert!(right > center);
    }

    #[test]
    fn text_x_never_below_margin() {
        let cfg = Config::default();
        let long: String = "a".repeat(200);
        for align in &["left", "center", "right"] {
            assert!(text_x(&long, 18.0, align, &cfg) >= cfg.slide.margin_x_mm);
        }
    }

    fn empty_slide() -> Slide {
        Slide {
            title: None,
            subtitle: None,
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
            table: None,
        }
    }

    #[test]
    fn content_height_empty_slide_is_zero() {
        let cfg = Config::default();
        assert_eq!(content_height(&empty_slide(), &cfg), 0.0);
    }

    #[test]
    fn content_height_title_only() {
        let cfg = Config::default();
        let mut s = empty_slide();
        s.title = Some("Hello".into());
        assert_eq!(
            content_height(&s, &cfg),
            cfg.title.rule_offset_mm + cfg.title.content_gap_mm
        );
    }

    #[test]
    fn content_height_table_adds_rows() {
        let cfg = Config::default();
        let mut s = empty_slide();
        s.table = Some(Table {
            headers: vec!["A".into(), "B".into()],
            rows: vec![vec!["1".into(), "2".into()], vec!["3".into(), "4".into()]],
            aligns: vec![TableAlign::Left, TableAlign::Left],
        });
        let expected = 3.0 * cfg.table.row_height_mm + cfg.body.section_gap_mm;
        assert!((content_height(&s, &cfg) - expected).abs() < 0.001);
    }

    #[test]
    fn content_height_bullets_add_lines() {
        let cfg = Config::default();
        let mut s = empty_slide();
        s.bullets = Some(vec!["a".into(), "b".into(), "c".into()]);
        let expected = 3.0 * cfg.body.line_height_mm + cfg.body.section_gap_mm;
        assert!((content_height(&s, &cfg) - expected).abs() < 0.001);
    }
}
