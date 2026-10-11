//! PDF export, laid out like marp's `default` theme.
//!
//! Each slide is turned into a vertical stack of blocks (headings,
//! paragraphs, lists, columns, a table, a code block), every block measured
//! first and drawn second. Margins between blocks collapse as in CSS, and
//! the whole stack is placed inside the slide padding according to the
//! slide's vertical alignment (middle by default, as in marp). An SVG, if
//! any, fills the space left below the stack.
//!
//! All coordinates are PDF points derived from the page size: the base
//! font size and paddings are fractions of the page, and every other size
//! is a multiple of the base font size (see `config.rs`).

use crate::config::{Color, Config};
use crate::exporter::text::{
    FontBytes, FontKind, FontSet, Line, Span, TextMetrics, parse_inline, wrap,
};
use crate::model::{
    Align, Column, Element, ListItem, Presentation, Slide, Table, TableAlign, VAlign,
};
use anyhow::{Context, Result, bail};
use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Size, Transform};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule};
use krilla::surface::Surface;
use krilla::text::TextDirection;
use krilla_svg::{SurfaceExt, SvgSettings};
use std::path::Path as StdPath;
use std::sync::Arc;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use usvg::fontdb::Database;

const PT_PER_MM: f32 = 72.0 / 25.4;

/// Bezier handle length for a quarter circle of radius 1.
const KAPPA: f32 = 0.552_284_8;

/// Everything the layout code needs that is shared by all slides.
struct Ctx<'a> {
    cfg: &'a Config,
    fonts: &'a FontSet<'a>,
    fontdb: Arc<Database>,
    svg_settings: SvgSettings,
    syntax_set: SyntaxSet,
    theme: Theme,
    /// Page size in points.
    width: f32,
    height: f32,
    /// Base font size (1 em) in points.
    em: f32,
}

impl Ctx<'_> {
    fn pad_x(&self) -> f32 {
        self.width * self.cfg.slide.padding_x_ratio
    }
    fn pad_y(&self) -> f32 {
        self.height * self.cfg.slide.padding_y_ratio
    }
    fn content_width(&self) -> f32 {
        self.width - 2.0 * self.pad_x()
    }
    fn body_metrics(&self) -> TextMetrics {
        self.metrics(self.em)
    }
    fn metrics(&self, size: f32) -> TextMetrics {
        let code_size = size * self.cfg.inline_code.font_size_em;
        TextMetrics {
            size,
            code_size,
            code_pad: code_size * self.cfg.inline_code.padding_x_em,
        }
    }
}

pub fn export(presentation: &Presentation, output_path: &StdPath, cfg: &Config) -> Result<()> {
    let pdf = render(presentation, cfg)?;
    std::fs::write(output_path, pdf)
        .with_context(|| format!("failed to write {}", output_path.display()))?;
    Ok(())
}

/// Build the shared layout context over already-loaded fonts.
fn make_ctx<'a>(cfg: &'a Config, bytes: &FontBytes, fonts: &'a FontSet<'a>) -> Result<Ctx<'a>> {
    let mut theme_set = ThemeSet::load_defaults();
    let github = ThemeSet::load_from_reader(&mut std::io::Cursor::new(include_str!(
        "../../assets/themes/github.tmTheme"
    )))
    .context("parsing the bundled GitHub code theme")?;
    theme_set.themes.insert("GitHub".into(), github);
    let theme = theme_set.themes.remove(&cfg.code.theme).with_context(|| {
        let mut known: Vec<&String> = theme_set.themes.keys().collect();
        known.sort();
        format!("unknown code theme {:?} (known: {known:?})", cfg.code.theme)
    })?;

    let width = cfg.slide.width_mm * PT_PER_MM;
    let height = cfg.slide.height_mm * PT_PER_MM;
    Ok(Ctx {
        cfg,
        fonts,
        fontdb: build_fontdb(bytes.regular(), bytes.mono(), &bytes.others())?,
        svg_settings: SvgSettings {
            filter_scale: cfg.svg.filter_scale,
            ..SvgSettings::default()
        },
        syntax_set: SyntaxSet::load_defaults_newlines(),
        theme,
        width,
        height,
        em: width * cfg.slide.font_size_ratio,
    })
}

/// Render `presentation` to the bytes of a PDF.
fn render(presentation: &Presentation, cfg: &Config) -> Result<Vec<u8>> {
    let bytes = FontBytes::read(&cfg.fonts)?;
    let fonts = bytes.load()?;
    let ctx = make_ctx(cfg, &bytes, &fonts)?;
    let page_size = Size::from_wh(ctx.width, ctx.height).context("invalid page size")?;

    let mut doc = Document::new();
    if let Some(title) = &presentation.title {
        doc.set_metadata(krilla::metadata::Metadata::new().title(title.clone()));
    }

    let slides = &presentation.slides;
    let total = slides.len();
    let paginate = presentation.paginate.unwrap_or(false);

    if slides.is_empty() {
        let mut page = doc.start_page_with(PageSettings::new(page_size));
        page.surface().finish();
        page.finish();
    }

    // Lay out every slide before drawing any, so one run reports all the
    // slides with problems rather than only the first.
    let mut layouts = Vec::with_capacity(total);
    let mut errors = Vec::new();
    for (i, slide) in slides.iter().enumerate() {
        match layout_slide(&ctx, slide, paginate.then_some(i + 1)) {
            Ok(ops) => layouts.push(ops),
            Err(e) => errors.push(format!("slide {}/{}: {e:#}", i + 1, total)),
        }
    }
    if !errors.is_empty() {
        bail!(
            "{} slide(s) cannot be laid out:\n  {}",
            errors.len(),
            errors.join("\n  ")
        );
    }

    for (i, ops) in layouts.into_iter().enumerate() {
        let mut page = doc.start_page_with(PageSettings::new(page_size));
        let mut surface = page.surface();
        draw_ops(&mut surface, &ctx, ops).with_context(|| format!("slide {}/{}", i + 1, total))?;
        surface.finish();
        page.finish();
    }

    doc.finish()
        .map_err(|e| anyhow::anyhow!("PDF finalisation failed: {e:?}"))
}

/// The font database usvg resolves SVG text against: the configured fonts,
/// with the generic families pointing at the regular and mono faces.
fn build_fontdb(regular: &[u8], mono: &[u8], others: &[&[u8]]) -> Result<Arc<Database>> {
    let family = |bytes: &[u8], what: &str| -> Result<String> {
        let mut one = Database::new();
        one.load_font_data(bytes.to_vec());
        one.faces()
            .next()
            .and_then(|f| f.families.first().map(|(n, _)| n.clone()))
            .with_context(|| format!("{what} font has no family name"))
    };
    let sans = family(regular, "regular")?;
    let mono_family = family(mono, "mono")?;
    let mut db = Database::new();
    for bytes in [regular, mono].iter().chain(others) {
        db.load_font_data(bytes.to_vec());
    }
    db.set_sans_serif_family(&sans);
    db.set_serif_family(&sans);
    db.set_monospace_family(&mono_family);
    Ok(Arc::new(db))
}

// ── display list ──────────────────────────────────────────────────────────

/// One drawing operation. `y` of text is the baseline; `y` of a box is its
/// top edge.
enum Op {
    Text {
        x: f32,
        y: f32,
        font: FontKind,
        size: f32,
        color: Color,
        text: String,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        color: Color,
    },
    /// A circle outline: `x`, `y` is the centre, `radius` the middle of the
    /// stroke.
    Ring {
        x: f32,
        y: f32,
        radius: f32,
        stroke: f32,
        color: Color,
    },
    Svg {
        tree: Box<usvg::Tree>,
        x: f32,
        y: f32,
        scale: f32,
    },
}

impl Op {
    fn shift(&mut self, dx: f32, dy: f32) {
        match self {
            Op::Text { x, y, .. }
            | Op::Rect { x, y, .. }
            | Op::Ring { x, y, .. }
            | Op::Svg { x, y, .. } => {
                *x += dx;
                *y += dy;
            }
        }
    }
}

/// A laid-out piece of the slide's vertical stack. Op coordinates are
/// relative to the block's top edge.
struct Block {
    ops: Vec<Op>,
    height: f32,
    margin_top: f32,
    margin_bottom: f32,
}

/// Total height of `blocks` stacked with collapsed margins. The first
/// block's top margin and the last block's bottom margin do not count
/// (marp's `section > :first-child` / `:last-child` rules).
fn stack_height(blocks: &[Block]) -> f32 {
    let mut h = 0.0;
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            h += blocks[i - 1].margin_bottom.max(b.margin_top);
        }
        h += b.height;
    }
    h
}

/// Move `blocks` into absolute position starting at `top`, appending
/// their ops to `out`. Returns the bottom edge of the last block.
fn place(blocks: Vec<Block>, top: f32, out: &mut Vec<Op>) -> f32 {
    let mut y = top;
    let mut prev_margin: Option<f32> = None;
    for b in blocks {
        if let Some(m) = prev_margin {
            y += m.max(b.margin_top);
        }
        for mut op in b.ops {
            op.shift(0.0, y);
            out.push(op);
        }
        y += b.height;
        prev_margin = Some(b.margin_bottom);
    }
    y
}

// ── slide layout ──────────────────────────────────────────────────────────

fn layout_slide(ctx: &Ctx, slide: &Slide, page_number: Option<usize>) -> Result<Vec<Op>> {
    let cfg = ctx.cfg;
    let align = slide.align.unwrap_or(Align::Left);
    let title_align = slide.title_align.unwrap_or(align);
    let content_align = slide.content_align.unwrap_or(align);
    let x0 = ctx.pad_x();
    let width = ctx.content_width();
    let Split {
        above: body,
        picture,
        below: after,
    } = split_picture(&slide.body)?;

    // Every block of the slide above its picture (all of them, without a
    // picture), with its code shrunk by `scale`.
    let build = |scale: f32| -> Result<Vec<Block>> {
        let mut blocks = Vec::new();
        if let Some(title) = &slide.title {
            blocks.push(heading_block(
                ctx,
                title,
                slide.title_level,
                x0,
                width,
                title_align,
            ));
        }
        for element in body {
            blocks.extend(element_blocks(
                ctx,
                element,
                x0,
                width,
                content_align,
                scale,
            )?);
        }
        Ok(blocks)
    };

    let top = ctx.pad_y();
    let available = ctx.height - 2.0 * top;
    let sources: Vec<&str> = body
        .iter()
        .chain(after)
        .filter_map(|e| match e {
            Element::Code(code) => Some(code_source(code)),
            _ => None,
        })
        .collect();
    // Code first shrinks to fit the width, like marp's auto-scaling of
    // `<pre>`; all of a slide's code shares one factor so it reads at one
    // size.
    let mut scale = code_width_scale(ctx, &sources, width);
    let mut blocks = build(scale)?;
    if picture.is_none() && !sources.is_empty() && stack_height(&blocks) > available {
        // Then, if the slide is too tall, it shrinks to fit the height. The
        // stack height is linear in the scale (only the code's text and
        // padding scale), so two layouts give the exact factor.
        let full = stack_height(&blocks);
        let half = stack_height(&build(scale / 2.0)?);
        let per_unit = 2.0 * (full - half) / scale;
        let fixed = full - per_unit * scale;
        let fit = (available - fixed) / per_unit;
        if fit <= 0.0 {
            bail!("no room left for the code below the slide's other content");
        }
        scale = fit.min(scale);
        blocks = build(scale)?;
    }
    let stack = stack_height(&blocks);
    let mut ops = Vec::new();

    if let Some(svg) = picture {
        // The picture takes all the room the other elements leave, so they
        // sit at the top and vertical alignment does not apply.
        let mut below = Vec::new();
        for element in after {
            below.extend(element_blocks(
                ctx,
                element,
                x0,
                width,
                content_align,
                scale,
            )?);
        }
        let gap = cfg.svg.top_gap_em * ctx.em;
        let picture_top = if blocks.is_empty() {
            top
        } else {
            place(blocks, top, &mut ops) + gap
        };
        let reserved = if below.is_empty() {
            0.0
        } else {
            stack_height(&below) + gap
        };
        let (op, height) = svg_op(ctx, &svg, picture_top, reserved)?;
        ops.push(op);
        if !below.is_empty() {
            place(below, picture_top + height + gap, &mut ops);
        }
    } else {
        // Content taller than the padded area starts at the top and runs
        // into the bottom padding, like marp's `safe center`. Content whose
        // ink would run off the page (marp would cut it) is an error; the
        // empty leading under the last line may hang past the edge.
        let slack = (available - stack).max(0.0);
        let y = match slide.valign.unwrap_or(cfg.slide.valign) {
            VAlign::Top => top,
            VAlign::Middle => top + slack / 2.0,
            VAlign::Bottom => top + slack,
        };
        place(blocks, y, &mut ops);
        if ink_bottom(ctx, &ops) > ctx.height {
            bail!(
                "content does not fit on the slide: it is {:.0}% of the page height \
                 below the top padding; split the slide or shorten it",
                stack / (ctx.height - top) * 100.0
            );
        }
    }

    if let Some(n) = page_number {
        ops.push(page_number_op(ctx, n));
    }
    Ok(ops)
}

/// The lowest point anything in `ops` draws: text descenders, box bottoms,
/// rings, pictures.
fn ink_bottom(ctx: &Ctx, ops: &[Op]) -> f32 {
    ops.iter()
        .map(|op| match op {
            Op::Text { y, font, size, .. } => y - ctx.fonts.get(*font).descent * size,
            Op::Rect { y, h, .. } => y + h,
            Op::Ring {
                y, radius, stroke, ..
            } => y + radius + stroke / 2.0,
            Op::Svg { tree, y, scale, .. } => y + tree.size().height() * scale,
        })
        .fold(f32::MIN, f32::max)
}

/// The blocks one body element lays out as. Images are handled by the
/// caller (see `split_picture`).
fn element_blocks(
    ctx: &Ctx,
    element: &Element,
    x0: f32,
    width: f32,
    align: Align,
    code_scale: f32,
) -> Result<Vec<Block>> {
    Ok(match element {
        Element::Text(text) => paragraphs(text)
            .iter()
            .map(|p| paragraph_block(ctx, p, x0, width, align))
            .collect(),
        Element::Heading(h) => vec![heading_block(ctx, &h.text, h.level, x0, width, align)],
        Element::List(list) => vec![list_block(ctx, list.ordered, &list.items, x0, width, align)],
        Element::Code(code) => vec![code_block(
            ctx,
            code_source(code),
            code.language.as_deref(),
            x0,
            width,
            code_scale,
        )?],
        Element::Table(table) => vec![table_block(ctx, table, x0, width, align)?],
        Element::Columns(columns) => vec![columns_block(ctx, columns, x0, width, align)?],
        Element::Image(_) | Element::Svg(_) => {
            unreachable!("split_picture removes the image")
        }
    })
}

fn code_source(code: &crate::model::CodeBlock) -> &str {
    if code.trim {
        code.source.trim_end_matches(['\n', '\r'])
    } else {
        &code.source
    }
}

/// Separate the slide's picture (an `image` file or inline `svg`) from the
/// rest of its body: the elements above it, its SVG source, and the
/// elements below it. A slide has at most one picture; it fills the space
/// the other elements leave.
fn split_picture(body: &[Element]) -> Result<Split<'_>> {
    let mut at = None;
    for (i, e) in body.iter().enumerate() {
        if matches!(e, Element::Image(_) | Element::Svg(_)) {
            if at.is_some() {
                bail!("a slide can have one image or svg; split the slide");
            }
            at = Some(i);
        }
    }
    let Some(i) = at else {
        return Ok(Split {
            above: body,
            picture: None,
            below: &[],
        });
    };
    let svg = match &body[i] {
        Element::Svg(svg) => svg.clone(),
        Element::Image(path) => {
            if !path.to_lowercase().ends_with(".svg") {
                bail!("image path {path:?} is not an .svg (raster images not yet supported)");
            }
            std::fs::read_to_string(path)
                .with_context(|| format!("failed to read SVG file {path:?}"))?
        }
        _ => unreachable!("index of a picture"),
    };
    Ok(Split {
        above: &body[..i],
        picture: Some(svg),
        below: &body[i + 1..],
    })
}

/// A slide body split around its picture (see `split_picture`).
struct Split<'a> {
    above: &'a [Element],
    picture: Option<String>,
    below: &'a [Element],
}

/// Split slide content into paragraphs on blank lines. Within a paragraph
/// every newline is a line break (marp renders with `breaks: true`).
fn paragraphs(content: &str) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        } else {
            current.push(line.trim().to_string());
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Style of one run of laid-out text lines.
struct TextStyle {
    metrics: TextMetrics,
    line_height: f32,
    color: Color,
    /// Font of the text's ascent/descent, which places the baseline.
    font: FontKind,
}

/// Distance from the top of a line box to its baseline: half the leading,
/// then the ascent (CSS inline layout).
fn baseline_offset(ctx: &Ctx, font: FontKind, size: f32, line_height: f32) -> f32 {
    let f = ctx.fonts.get(font);
    let content = (f.ascent - f.descent) * size;
    (line_height * size - content) / 2.0 + f.ascent * size
}

fn line_x(x0: f32, width: f32, line_width: f32, align: Align) -> f32 {
    match align {
        Align::Left => x0,
        Align::Center => x0 + (width - line_width) / 2.0,
        Align::Right => x0 + width - line_width,
    }
}

/// Draw one wrapped line with its left edge at `x` and its baseline at `y`.
fn line_ops(ctx: &Ctx, line: &Line, x: f32, y: f32, style: &TextStyle, ops: &mut Vec<Op>) {
    let cfg = ctx.cfg;
    let m = style.metrics;
    // One background box per run of adjacent code pieces, so a code span
    // split into words still gets a single rounded box.
    let mut i = 0;
    while i < line.pieces.len() {
        if !line.pieces[i].span.code {
            i += 1;
            continue;
        }
        let start = &line.pieces[i];
        while i + 1 < line.pieces.len() && line.pieces[i + 1].span.code {
            i += 1;
        }
        let end = &line.pieces[i];
        let mono = &ctx.fonts.mono;
        let pad_y = m.code_size * cfg.inline_code.padding_y_em;
        let box_top = y - mono.ascent * m.code_size - pad_y;
        let box_bottom = y - mono.descent * m.code_size + pad_y;
        ops.push(Op::Rect {
            x: x + start.x,
            y: box_top,
            w: end.x + end.width - start.x,
            h: box_bottom - box_top,
            radius: cfg.inline_code.radius_em * ctx.em,
            color: cfg.colors.inline_code_background,
        });
        i += 1;
    }
    for piece in &line.pieces {
        let span = &piece.span;
        if span.text.is_empty() {
            // A zero-width break opportunity (after a hyphen).
            continue;
        }
        let color = if span.link {
            cfg.colors.link
        } else {
            style.color
        };
        let (font, size) = if span.code {
            (FontKind::Mono, m.code_size)
        } else {
            (span.font(), m.size)
        };
        ops.push(Op::Text {
            x: x + piece.x + piece.pad_left,
            y,
            font,
            size,
            color,
            text: span.text.clone(),
        });
    }
}

/// Lay out `spans` wrapped to `width`, one line box per line. Returns the
/// ops (relative to the top of the first line) and the total height.
fn text_lines(
    ctx: &Ctx,
    spans: &[Span],
    x0: f32,
    width: f32,
    align: Align,
    style: &TextStyle,
) -> (Vec<Op>, f32) {
    let lh = style.line_height * style.metrics.size;
    let base = baseline_offset(ctx, style.font, style.metrics.size, style.line_height);
    let mut ops = Vec::new();
    let lines = wrap(spans, width, ctx.fonts, style.metrics);
    for (i, line) in lines.iter().enumerate() {
        let x = line_x(x0, width, line.width, align);
        line_ops(ctx, line, x, i as f32 * lh + base, style, &mut ops);
    }
    (ops, lines.len() as f32 * lh)
}

fn heading_block(ctx: &Ctx, text: &str, level: u8, x0: f32, width: f32, align: Align) -> Block {
    let cfg = ctx.cfg;
    let size = ctx.em * cfg.heading.font_sizes_em[usize::from(level - 1)];
    let color = match level {
        1 => cfg.colors.h1,
        6 => cfg.colors.h6,
        _ => cfg.colors.text,
    };
    // Headings are set in the bold face throughout.
    let spans: Vec<Span> = parse_inline(text)
        .into_iter()
        .map(|s| Span { bold: true, ..s })
        .collect();
    let style = TextStyle {
        metrics: ctx.metrics(size),
        line_height: cfg.heading.line_height,
        color,
        font: FontKind::Bold,
    };
    let (ops, height) = text_lines(ctx, &spans, x0, width, align, &style);
    Block {
        ops,
        height: height + size * cfg.heading.padding_bottom_em[usize::from(level - 1)],
        margin_top: cfg.heading.margin_top_em * ctx.em,
        margin_bottom: cfg.heading.margin_bottom_em * ctx.em,
    }
}

fn body_style(ctx: &Ctx) -> TextStyle {
    TextStyle {
        metrics: ctx.body_metrics(),
        line_height: ctx.cfg.slide.line_height,
        color: ctx.cfg.colors.text,
        font: FontKind::Regular,
    }
}

fn paragraph_block(ctx: &Ctx, lines: &[String], x0: f32, width: f32, align: Align) -> Block {
    let style = body_style(ctx);
    let mut ops = Vec::new();
    let mut height = 0.0;
    for line in lines {
        let (mut line_ops, h) = text_lines(ctx, &parse_inline(line), x0, width, align, &style);
        line_ops.iter_mut().for_each(|op| op.shift(0.0, height));
        ops.extend(line_ops);
        height += h;
    }
    Block {
        ops,
        height,
        margin_top: 0.0,
        margin_bottom: ctx.cfg.block.margin_bottom_em * ctx.em,
    }
}

/// A bulleted or numbered list, with nested lists under their items.
fn list_block(
    ctx: &Ctx,
    ordered: bool,
    items: &[ListItem],
    x0: f32,
    width: f32,
    align: Align,
) -> Block {
    let mut ops = Vec::new();
    let area = Area { x0, width, align };
    let height = list_ops(ctx, ordered, items, 1, area, 0.0, &mut ops);
    Block {
        ops,
        height,
        margin_top: 0.0,
        margin_bottom: ctx.cfg.block.margin_bottom_em * ctx.em,
    }
}

/// A horizontal band of the slide to lay text out in.
#[derive(Clone, Copy)]
struct Area {
    x0: f32,
    width: f32,
    align: Align,
}

/// Lay out a list at nesting `depth` (1 for the outermost) in `area` from
/// `y`, and return where it ends. Text is indented by `list.indent_em` per level and
/// each item's marker hangs to the left of its first line, as a CSS outside
/// marker. Items are `list.item_gap_em` apart; a nested list follows its
/// item's text directly.
fn list_ops(
    ctx: &Ctx,
    ordered: bool,
    items: &[ListItem],
    depth: usize,
    area: Area,
    mut y: f32,
    ops: &mut Vec<Op>,
) -> f32 {
    let Area { x0, width, align } = area;
    let cfg = ctx.cfg;
    let style = body_style(ctx);
    let indent = cfg.list.indent_em * ctx.em;
    let text_x0 = x0 + indent;
    let text_w = width - indent;
    let lh = style.line_height * ctx.em;
    let base = baseline_offset(ctx, FontKind::Regular, ctx.em, style.line_height);
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            y += cfg.list.item_gap_em * ctx.em;
        }
        let lines = wrap(&parse_inline(&item.text), text_w, ctx.fonts, style.metrics);
        let first_x = lines
            .first()
            .map_or(text_x0, |l| line_x(text_x0, text_w, l.width, align));
        marker_ops(ctx, ordered, depth, i, first_x, y + base, ops);
        for line in &lines {
            let x = line_x(text_x0, text_w, line.width, align);
            line_ops(ctx, line, x, y + base, &style, ops);
            y += lh;
        }
        if lines.is_empty() {
            // An empty item still takes a line, like an empty `<li>`.
            y += lh;
        }
        if let Some(children) = &item.children {
            let inner = Area {
                x0: text_x0,
                width: text_w,
                align,
            };
            y = list_ops(
                ctx,
                children.ordered,
                &children.items,
                depth + 1,
                inner,
                y,
                ops,
            );
        }
    }
    y
}

/// The marker of item `index` of a list at `depth`, for a first line that
/// starts at `x` with its baseline at `baseline`. As in browsers: bullets
/// are a disc, then a ring, then a square; numbers are decimal, then lower
/// roman, then lower alpha (GitHub's `ol ol` / `ol ol ol` styles).
fn marker_ops(
    ctx: &Ctx,
    ordered: bool,
    depth: usize,
    index: usize,
    x: f32,
    baseline: f32,
    ops: &mut Vec<Op>,
) {
    let cfg = ctx.cfg;
    let color = cfg.colors.text;
    if ordered {
        let number = index + 1;
        let label = match depth {
            1 => number.to_string(),
            2 => roman(number),
            _ => alpha(number),
        };
        // The marker is "N. ", right-aligned against the text.
        let text = format!("{label}.");
        let w = ctx.fonts.regular.width(&format!("{text} "), ctx.em);
        ops.push(Op::Text {
            x: x - w,
            y: baseline,
            font: FontKind::Regular,
            size: ctx.em,
            color,
            text,
        });
        return;
    }
    let size = cfg.list.disc_size_em * ctx.em;
    let cx = x - cfg.list.disc_offset_em * ctx.em;
    let cy = baseline - cfg.list.disc_rise_em * ctx.em;
    match depth {
        1 => ops.push(Op::Rect {
            x: cx - size / 2.0,
            y: cy - size / 2.0,
            w: size,
            h: size,
            radius: size / 2.0,
            color,
        }),
        2 => ops.push(Op::Ring {
            x: cx,
            y: cy,
            radius: size / 2.0,
            stroke: cfg.list.ring_stroke_em * ctx.em,
            color,
        }),
        _ => ops.push(Op::Rect {
            x: cx - size / 2.0,
            y: cy - size / 2.0,
            w: size,
            h: size,
            radius: 0.0,
            color,
        }),
    }
}

/// Lower-case roman numeral (1 -> i, 4 -> iv, 9 -> ix, ...).
fn roman(mut n: usize) -> String {
    const DIGITS: [(usize, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut out = String::new();
    for (value, digits) in DIGITS {
        while n >= value {
            out.push_str(digits);
            n -= value;
        }
    }
    out
}

/// Lower-case letters as CSS `lower-alpha` counts: a..z, aa, ab, ...
fn alpha(mut n: usize) -> String {
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push(b'a' + (n % 26) as u8);
        n /= 26;
    }
    out.reverse();
    String::from_utf8(out).expect("ASCII letters")
}

/// Side-by-side columns of equal width, each an optional level-3 header
/// over a bulleted list.
fn columns_block(
    ctx: &Ctx,
    columns: &[Column],
    x0: f32,
    width: f32,
    align: Align,
) -> Result<Block> {
    if columns.is_empty() {
        bail!("columns: must contain at least one column");
    }
    let cfg = ctx.cfg;
    let n = columns.len() as f32;
    let gap = cfg.columns.gap_em * ctx.em;
    let col_w = (width - gap * (n - 1.0)) / n;
    let mut ops = Vec::new();
    let mut height: f32 = 0.0;
    for (i, col) in columns.iter().enumerate() {
        let cx = x0 + i as f32 * (col_w + gap);
        let mut blocks = Vec::new();
        if let Some(header) = &col.header {
            // marp decks write column headers as `###`.
            blocks.push(heading_block(ctx, header, 3, cx, col_w, align));
        }
        blocks.push(list_block(ctx, false, &col.bullets, cx, col_w, align));
        height = height.max(stack_height(&blocks));
        place(blocks, 0.0, &mut ops);
    }
    Ok(Block {
        ops,
        height,
        margin_top: 0.0,
        margin_bottom: cfg.block.margin_bottom_em * ctx.em,
    })
}

/// A GFM table, sized to its content like marp's `width: max-content`.
/// When that is wider than the slide, columns shrink as in the CSS
/// automatic table layout: each keeps its longest word and shares the
/// remaining width in proportion to how much it would need to fit
/// unwrapped. Cells then wrap.
fn table_block(ctx: &Ctx, table: &Table, x0: f32, width: f32, align: Align) -> Result<Block> {
    let cfg = ctx.cfg;
    let style = body_style(ctx);
    let pad_x = cfg.table.cell_padding_x_em * ctx.em;
    let pad_y = cfg.table.cell_padding_y_em * ctx.em;
    let border = cfg.table.border_em * ctx.em;

    let header_row: Vec<Vec<Span>> = table
        .headers
        .iter()
        .map(|h| {
            parse_inline(h)
                .into_iter()
                .map(|s| Span { bold: true, ..s })
                .collect()
        })
        .collect();
    let cells: Vec<Vec<Vec<Span>>> = std::iter::once(header_row)
        .chain(
            table
                .rows
                .iter()
                .map(|r| r.iter().map(|c| parse_inline(c)).collect()),
        )
        .collect();

    let widest = |lines: Vec<Line>| lines.iter().map(|l| l.width).fold(0.0, f32::max);
    let n = table.headers.len();
    let mut max_w = vec![0.0_f32; n];
    let mut min_w = vec![0.0_f32; n];
    for row in &cells {
        for (c, spans) in row.iter().enumerate() {
            let full = widest(wrap(spans, f32::INFINITY, ctx.fonts, style.metrics));
            // A zero width puts every unbreakable word on its own line.
            let word = widest(wrap(spans, 0.0, ctx.fonts, style.metrics));
            max_w[c] = max_w[c].max(full + 2.0 * pad_x);
            min_w[c] = min_w[c].max(word + 2.0 * pad_x);
        }
    }
    let avail = width - border;
    let sum_max: f32 = max_w.iter().sum();
    let sum_min: f32 = min_w.iter().sum();
    let col_w: Vec<f32> = if sum_max <= avail {
        max_w
    } else if sum_min > avail {
        bail!(
            "table is too wide for the slide even with every cell wrapped: needs {:.0}% \
             of the content width (headers: {:?})",
            (sum_min + border) / width * 100.0,
            table.headers
        );
    } else {
        let spare = avail - sum_min;
        let flex = sum_max - sum_min;
        (0..n)
            .map(|c| min_w[c] + spare * (max_w[c] - min_w[c]) / flex)
            .collect()
    };
    let table_w = col_w.iter().sum::<f32>() + border;

    let lh = style.line_height * ctx.em;
    let base = baseline_offset(ctx, FontKind::Regular, ctx.em, style.line_height);
    let wrapped: Vec<Vec<Vec<Line>>> = cells
        .iter()
        .map(|row| {
            row.iter()
                .enumerate()
                .map(|(c, spans)| wrap(spans, col_w[c] - 2.0 * pad_x, ctx.fonts, style.metrics))
                .collect()
        })
        .collect();
    let row_h: Vec<f32> = wrapped
        .iter()
        .map(|row| {
            let lines = row.iter().map(Vec::len).max().unwrap_or(0).max(1);
            lines as f32 * lh + 2.0 * pad_y + border
        })
        .collect();
    let table_h = row_h.iter().sum::<f32>() + border;
    let left = line_x(x0, width, table_w, align);

    let mut ops = Vec::new();
    // Grid: the whole table in the border color, then each cell's
    // background inset by the border width.
    ops.push(Op::Rect {
        x: left,
        y: 0.0,
        w: table_w,
        h: table_h,
        radius: 0.0,
        color: cfg.colors.table_border,
    });
    let mut top = 0.0;
    for (r, row) in wrapped.iter().enumerate() {
        // Body rows alternate, starting plain (CSS tbody tr:nth-child(2n)).
        let fill = if r >= 2 && r % 2 == 0 {
            cfg.colors.table_stripe
        } else {
            Color(0xff, 0xff, 0xff)
        };
        let mut cell_left = left;
        for (c, lines) in row.iter().enumerate() {
            ops.push(Op::Rect {
                x: cell_left + border,
                y: top + border,
                w: col_w[c] - border,
                h: row_h[r] - border,
                radius: 0.0,
                color: fill,
            });
            let cell_align = match (table.aligns[c], r) {
                (TableAlign::Auto, 0) | (TableAlign::Center, _) => Align::Center,
                (TableAlign::Auto, _) | (TableAlign::Left, _) => Align::Left,
                (TableAlign::Right, _) => Align::Right,
            };
            let inner_x = cell_left + border / 2.0 + pad_x;
            let inner_w = col_w[c] - 2.0 * pad_x;
            // Cells are vertically centered in their row (CSS `<td>` default).
            let slack = row_h[r] - border - 2.0 * pad_y - lines.len() as f32 * lh;
            for (k, line) in lines.iter().enumerate() {
                let tx = line_x(inner_x, inner_w, line.width, cell_align);
                let y = top + border + pad_y + slack / 2.0 + k as f32 * lh + base;
                line_ops(ctx, line, tx, y, &style, &mut ops);
            }
            cell_left += col_w[c];
        }
        top += row_h[r];
    }
    Ok(Block {
        ops,
        height: table_h,
        margin_top: 0.0,
        margin_bottom: cfg.block.margin_bottom_em * ctx.em,
    })
}

/// A fenced code block: a bordered, rounded box spanning the content width,
/// syntax-highlighted text, and the language's icon in the top-right corner.
/// Everything inside the box is shrunk by `scale` (see `code_scale`).
fn code_block(
    ctx: &Ctx,
    source: &str,
    language: Option<&str>,
    x0: f32,
    width: f32,
    scale: f32,
) -> Result<Block> {
    let cfg = ctx.cfg;
    let em = ctx.em * scale;
    let size = em * cfg.code.font_size_em;
    let lh = size * cfg.code.line_height;
    let pad = cfg.code.padding_em * em;
    let border = cfg.code.border_em * ctx.em;
    let radius = cfg.code.radius_em * ctx.em;
    let line_count = LinesWithEndings::from(source).count();
    if line_count == 0 {
        bail!("code block is empty");
    }
    let height = 2.0 * (border + pad) + line_count as f32 * lh;

    let mut ops = vec![
        Op::Rect {
            x: x0,
            y: 0.0,
            w: width,
            h: height,
            radius,
            color: cfg.colors.code_border,
        },
        Op::Rect {
            x: x0 + border,
            y: border,
            w: width - 2.0 * border,
            h: height - 2.0 * border,
            radius: (radius - border).max(0.0),
            color: cfg.colors.code_background,
        },
    ];

    if let Some(lang) = language
        && let Some(icon) = crate::assets::language_icon(lang)
    {
        let side = cfg.code.icon_size_em * em;
        let inset = cfg.code.icon_inset_em * em;
        let tree = parse_svg(ctx, icon).with_context(|| format!("{lang} icon"))?;
        let (scale, w, h) = fit(&tree, side, side)?;
        ops.push(Op::Svg {
            tree: Box::new(tree),
            x: x0 + width - border - inset - side + (side - w) / 2.0,
            y: border + inset + (side - h) / 2.0,
            scale,
        });
    }

    // Unknown languages render as plain text, as highlight.js does in marp.
    let syntax = language
        .and_then(|lang| ctx.syntax_set.find_syntax_by_token(lang))
        .unwrap_or_else(|| ctx.syntax_set.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, &ctx.theme);
    let base = baseline_offset(ctx, FontKind::Mono, size, cfg.code.line_height);
    let mono = &ctx.fonts.mono;
    for (i, line) in LinesWithEndings::from(source).enumerate() {
        let ranges: Vec<(Style, &str)> = highlighter
            .highlight_line(line, &ctx.syntax_set)
            .with_context(|| format!("highlighting code line {}: {line:?}", i + 1))?;
        let mut x = x0 + border + pad;
        let y = border + pad + i as f32 * lh + base;
        for (style, token) in ranges {
            let text = token.trim_end_matches(['\n', '\r']);
            if text.is_empty() {
                continue;
            }
            let fg = style.foreground;
            ops.push(Op::Text {
                x,
                y,
                font: FontKind::Mono,
                size,
                color: Color(fg.r, fg.g, fg.b),
                text: text.to_string(),
            });
            x += mono.width(text, size);
        }
    }
    Ok(Block {
        ops,
        height,
        margin_top: 0.0,
        margin_bottom: cfg.block.margin_bottom_em * ctx.em,
    })
}

/// How much a slide's code must shrink so that the widest line of every
/// block fits `width`. Never enlarges. The border does not scale; the
/// padding and the text inside it do.
fn code_width_scale(ctx: &Ctx, sources: &[&str], width: f32) -> f32 {
    let cfg = ctx.cfg;
    let border = 2.0 * cfg.code.border_em * ctx.em;
    let pad = 2.0 * cfg.code.padding_em * ctx.em;
    let size = ctx.em * cfg.code.font_size_em;
    sources
        .iter()
        .map(|source| {
            let widest = LinesWithEndings::from(source)
                .map(|l| ctx.fonts.mono.width(l.trim_end_matches(['\n', '\r']), size))
                .fold(0.0, f32::max);
            (width - border) / (widest + pad)
        })
        .fold(1.0, f32::min)
}

fn parse_svg(ctx: &Ctx, svg: &str) -> Result<usvg::Tree> {
    let opts = usvg::Options {
        fontdb: ctx.fontdb.clone(),
        font_family: "sans-serif".into(),
        ..Default::default()
    };
    let flattened;
    let bytes: &[u8] = if ctx.cfg.svg.flatten_css_vars {
        flattened = crate::utils::css_vars::flatten(svg);
        flattened.as_bytes()
    } else {
        svg.as_bytes()
    };
    usvg::Tree::from_data(bytes, &opts).map_err(|e| anyhow::anyhow!("failed to parse SVG: {e}"))
}

/// Scale that fits `tree` inside `w` x `h`, and the resulting size.
fn fit(tree: &usvg::Tree, w: f32, h: f32) -> Result<(f32, f32, f32)> {
    let (nw, nh) = (tree.size().width(), tree.size().height());
    if nw <= 0.0 || nh <= 0.0 {
        bail!("SVG has non-positive dimensions: {nw}x{nh}");
    }
    let scale = (w / nw).min(h / nh);
    Ok((scale, nw * scale, nh * scale))
}

/// The slide's SVG, filling the area from `top` down to the SVG bottom
/// margin less `reserved` (room kept for elements below it), centred
/// horizontally. Returns the op and the SVG's drawn height.
fn svg_op(ctx: &Ctx, svg: &str, top: f32, reserved: f32) -> Result<(Op, f32)> {
    let cfg = ctx.cfg;
    let margin_x = ctx.width * cfg.svg.margin_x_ratio;
    let bottom = ctx.height * (1.0 - cfg.svg.margin_bottom_ratio) - reserved;
    let avail_w = ctx.width - 2.0 * margin_x;
    let avail_h = bottom - top;
    if avail_h <= 0.0 {
        bail!("no room left for the SVG between the slide's other content");
    }
    let tree = parse_svg(ctx, svg)?;
    let (scale, w, h) = fit(&tree, avail_w, avail_h)?;
    let op = Op::Svg {
        tree: Box::new(tree),
        x: margin_x + (avail_w - w) / 2.0,
        y: top,
        scale,
    };
    Ok((op, h))
}

/// The page number, right-aligned at marp's pagination position.
fn page_number_op(ctx: &Ctx, n: usize) -> Op {
    let cfg = ctx.cfg;
    let size = ctx.em * cfg.pagination.font_size_em;
    let text = n.to_string();
    let right = ctx.width * (1.0 - cfg.pagination.right_ratio);
    let box_bottom = ctx.height * (1.0 - cfg.pagination.bottom_ratio);
    let lh = cfg.slide.line_height;
    let baseline = box_bottom - lh * size + baseline_offset(ctx, FontKind::Regular, size, lh);
    Op::Text {
        x: right - ctx.fonts.regular.width(&text, size),
        y: baseline,
        font: FontKind::Regular,
        size,
        color: cfg.colors.pagination,
        text,
    }
}

// ── drawing ───────────────────────────────────────────────────────────────

fn draw_ops(surface: &mut Surface<'_>, ctx: &Ctx, ops: Vec<Op>) -> Result<()> {
    for op in ops {
        match op {
            Op::Text {
                x,
                y,
                font,
                size,
                color,
                text,
            } => {
                set_fill(surface, color);
                surface.draw_text(
                    Point::from_xy(x, y),
                    ctx.fonts.get(font).krilla.clone(),
                    size,
                    &text,
                    false,
                    TextDirection::Auto,
                );
            }
            Op::Rect {
                x,
                y,
                w,
                h,
                radius,
                color,
            } => {
                set_fill(surface, color);
                let path = rounded_rect(x, y, w, h, radius)
                    .with_context(|| format!("degenerate rectangle {w}x{h} at ({x}, {y})"))?;
                surface.draw_path(&path);
            }
            Op::Ring {
                x,
                y,
                radius,
                stroke,
                color,
            } => {
                surface.set_fill(Some(Fill {
                    paint: {
                        let (r, g, b) = color.rgb();
                        rgb::Color::new(r, g, b).into()
                    },
                    opacity: NormalizedF32::ONE,
                    rule: FillRule::EvenOdd,
                }));
                let mut pb = PathBuilder::new();
                push_circle(&mut pb, x, y, radius + stroke / 2.0);
                push_circle(&mut pb, x, y, radius - stroke / 2.0);
                let path = pb
                    .finish()
                    .with_context(|| format!("degenerate ring of radius {radius}"))?;
                surface.draw_path(&path);
            }
            Op::Svg { tree, x, y, scale } => {
                let size = Size::from_wh(tree.size().width(), tree.size().height())
                    .context("invalid SVG size")?;
                surface.push_transform(&Transform::from_row(scale, 0.0, 0.0, scale, x, y));
                surface
                    .draw_svg(&tree, size, ctx.svg_settings)
                    .context("krilla-svg failed to draw the SVG")?;
                surface.pop();
            }
        }
    }
    Ok(())
}

fn set_fill(surface: &mut Surface<'_>, c: Color) {
    let (r, g, b) = c.rgb();
    surface.set_fill(Some(Fill {
        paint: rgb::Color::new(r, g, b).into(),
        opacity: NormalizedF32::ONE,
        rule: FillRule::default(),
    }));
}

/// Append a closed circle to `pb`, as four cubic arcs.
fn push_circle(pb: &mut PathBuilder, cx: f32, cy: f32, r: f32) {
    let k = r * KAPPA;
    pb.move_to(cx + r, cy);
    pb.cubic_to(cx + r, cy + k, cx + k, cy + r, cx, cy + r);
    pb.cubic_to(cx - k, cy + r, cx - r, cy + k, cx - r, cy);
    pb.cubic_to(cx - r, cy - k, cx - k, cy - r, cx, cy - r);
    pb.cubic_to(cx + k, cy - r, cx + r, cy - k, cx + r, cy);
    pb.close();
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<krilla::geom::Path> {
    let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
    let k = r * KAPPA;
    let (l, t, rt, b) = (x, y, x + w, y + h);
    let mut pb = PathBuilder::new();
    pb.move_to(l + r, t);
    pb.line_to(rt - r, t);
    pb.cubic_to(rt - r + k, t, rt, t + r - k, rt, t + r);
    pb.line_to(rt, b - r);
    pb.cubic_to(rt, b - r + k, rt - r + k, b, rt - r, b);
    pb.line_to(l + r, b);
    pb.cubic_to(l + r - k, b, l, b - r + k, l, b - r);
    pb.line_to(l, t + r);
    pb.cubic_to(l, t + r - k, l + r - k, t, l + r, t);
    pb.close();
    pb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(height: f32, margin_top: f32, margin_bottom: f32) -> Block {
        Block {
            ops: Vec::new(),
            height,
            margin_top,
            margin_bottom,
        }
    }

    #[test]
    fn stack_collapses_margins_and_drops_outer_ones() {
        let blocks = vec![
            block(10.0, 5.0, 3.0),
            block(20.0, 4.0, 2.0),
            block(5.0, 1.0, 9.0),
        ];
        // 10 + max(3,4) + 20 + max(2,1) + 5; the first top and last bottom
        // margins are ignored.
        assert_eq!(stack_height(&blocks), 10.0 + 4.0 + 20.0 + 2.0 + 5.0);
        assert_eq!(stack_height(&[]), 0.0);
    }

    #[test]
    fn place_matches_stack_height() {
        let blocks = vec![block(10.0, 5.0, 3.0), block(20.0, 4.0, 2.0)];
        let h = stack_height(&blocks);
        let mut ops = Vec::new();
        assert_eq!(place(blocks, 7.0, &mut ops), 7.0 + h);
    }

    #[test]
    fn paragraphs_split_on_blank_lines_and_keep_line_breaks() {
        assert_eq!(
            paragraphs("a\nb\n\n  \nc"),
            vec![
                vec!["a".to_string(), "b".to_string()],
                vec!["c".to_string()]
            ]
        );
        assert!(paragraphs("").is_empty());
    }

    #[test]
    fn line_x_aligns_within_box() {
        assert_eq!(line_x(10.0, 100.0, 40.0, Align::Left), 10.0);
        assert_eq!(line_x(10.0, 100.0, 40.0, Align::Center), 40.0);
        assert_eq!(line_x(10.0, 100.0, 40.0, Align::Right), 70.0);
    }

    #[test]
    fn rounded_rect_handles_zero_radius() {
        assert!(rounded_rect(0.0, 0.0, 10.0, 5.0, 0.0).is_some());
        assert!(rounded_rect(0.0, 0.0, 10.0, 5.0, 100.0).is_some());
    }

    // ── layout, with the configured fonts ─────────────────────────────────

    fn with_ctx<R>(cfg: &Config, f: impl FnOnce(&Ctx) -> R) -> R {
        let bytes = FontBytes::read(&cfg.fonts)
            .expect("the default fonts must be installed (see scripts/ci-install-tools.repo.sh)");
        let fonts = bytes.load().unwrap();
        let ctx = make_ctx(cfg, &bytes, &fonts).unwrap();
        f(&ctx)
    }

    fn deck(yaml: &str) -> Presentation {
        crate::parser::parse(yaml).expect("test YAML parses")
    }

    /// One slide, given as the YAML of a single list item.
    fn one_slide(item: &str) -> Slide {
        deck(&format!("slides:\n  - {}", item.replace('\n', "\n    ")))
            .slides
            .remove(0)
    }

    fn bullets_yaml(n: usize) -> String {
        let items: String = (1..=n).map(|i| format!("\n  - Bullet {i}")).collect();
        format!("title: T\nbullets:{items}")
    }

    /// (text, x, baseline) of every text op.
    fn texts(ops: &[Op]) -> Vec<(&str, f32, f32)> {
        ops.iter()
            .filter_map(|op| match op {
                Op::Text { text, x, y, .. } => Some((text.as_str(), *x, *y)),
                _ => None,
            })
            .collect()
    }

    fn first_baseline(ops: &[Op]) -> f32 {
        texts(ops).first().expect("slide has text").2
    }

    fn table_slide(yaml_table: &str) -> Table {
        match one_slide(&format!("table:\n{yaml_table}")).body.remove(0) {
            Element::Table(t) => t,
            _ => panic!("a table"),
        }
    }

    /// Text ops in the mono font, as (text, size).
    fn mono_sizes(ops: &[Op]) -> Vec<f32> {
        ops.iter()
            .filter_map(|op| match op {
                Op::Text {
                    font: FontKind::Mono,
                    size,
                    ..
                } => Some(*size),
                _ => None,
            })
            .collect()
    }

    const FULL_SLIDE: &str = "title: Scaling **check**
subtitle: {text: A subtitle, level: 3}
content: Body with `inline code` and https://example.org
bullets:
  - text: two **bold**
    bullets:
      - text: nested
        numbered: [deep]
table:
  headers: [Name, Value]
  aligns: [auto, right]
  rows:
    - [alpha, '1']
    - [beta, '22']
code:
  language: rust
  source: |-
    fn main() {
        println!(\"hi\");
    }";

    const SVG_SLIDE: &str = "title: Columns and a diagram
columns:
  - header: Left
    bullets: [a, b]
  - bullets: [c]
svg: <svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 40 20'><rect width='40' height='20'/></svg>";

    #[test]
    fn layout_scales_exactly_with_the_page_size() {
        // CLAUDE.md: changing the page size must rescale every element.
        // Lay the same slides out on two page sizes and require every
        // coordinate, size and radius to scale by exactly the page factor.
        let factor = 1.5;
        let small = Config::default();
        let mut large = Config::default();
        large.slide.width_mm *= factor;
        large.slide.height_mm *= factor;
        for item in [FULL_SLIDE, SVG_SLIDE] {
            let a = with_ctx(&small, |ctx| {
                layout_slide(ctx, &one_slide(item), Some(3)).unwrap()
            });
            let b = with_ctx(&large, |ctx| {
                layout_slide(ctx, &one_slide(item), Some(3)).unwrap()
            });
            assert_eq!(a.len(), b.len(), "op count differs between page sizes");
            let close = |x: f32, y: f32, what: &str| {
                assert!(
                    (y - factor * x).abs() <= 1e-3 * y.abs().max(1.0),
                    "{what}: {y} is not {factor} x {x}"
                );
            };
            for (oa, ob) in a.iter().zip(&b) {
                match (oa, ob) {
                    (
                        Op::Text {
                            x,
                            y,
                            size,
                            text,
                            font,
                            color,
                        },
                        Op::Text {
                            x: x2,
                            y: y2,
                            size: s2,
                            text: t2,
                            font: f2,
                            color: c2,
                        },
                    ) => {
                        assert_eq!((text, font, color), (t2, f2, c2));
                        close(*x, *x2, text);
                        close(*y, *y2, text);
                        close(*size, *s2, text);
                    }
                    (
                        Op::Rect {
                            x,
                            y,
                            w,
                            h,
                            radius,
                            color,
                        },
                        Op::Rect {
                            x: x2,
                            y: y2,
                            w: w2,
                            h: h2,
                            radius: r2,
                            color: c2,
                        },
                    ) => {
                        assert_eq!(color, c2);
                        for (v, v2) in [(x, x2), (y, y2), (w, w2), (h, h2), (radius, r2)] {
                            close(*v, *v2, "rect");
                        }
                    }
                    (
                        Op::Ring {
                            x,
                            y,
                            radius,
                            stroke,
                            color,
                        },
                        Op::Ring {
                            x: x2,
                            y: y2,
                            radius: r2,
                            stroke: s2,
                            color: c2,
                        },
                    ) => {
                        assert_eq!(color, c2);
                        for (v, v2) in [(x, x2), (y, y2), (radius, r2), (stroke, s2)] {
                            close(*v, *v2, "ring");
                        }
                    }
                    (
                        Op::Svg { x, y, scale, .. },
                        Op::Svg {
                            x: x2,
                            y: y2,
                            scale: s2,
                            ..
                        },
                    ) => {
                        for (v, v2) in [(x, x2), (y, y2), (scale, s2)] {
                            close(*v, *v2, "svg");
                        }
                    }
                    _ => panic!("op kinds differ between page sizes"),
                }
            }
        }
    }

    #[test]
    fn valign_places_the_stack_top_middle_or_bottom() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let y = |valign: &str| {
                let s = one_slide(&format!("title: T\nvalign: {valign}\nbullets: [a, b]"));
                first_baseline(&layout_slide(ctx, &s, None).unwrap())
            };
            let (top, middle, bottom) = (y("top"), y("middle"), y("bottom"));
            let slack = bottom - top;
            assert!(slack > 0.0);
            assert!(
                (middle - top - slack / 2.0).abs() < 1e-3,
                "{top} {middle} {bottom}"
            );
            // The configured default is middle.
            let default = first_baseline(
                &layout_slide(ctx, &one_slide("title: T\nbullets: [a, b]"), None).unwrap(),
            );
            assert!((default - middle).abs() < 1e-3);
        });
    }

    #[test]
    fn content_taller_than_the_padding_box_starts_at_the_top() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let placed = |n: usize, valign: &str| {
                let s = one_slide(&format!("{}\nvalign: {valign}", bullets_yaml(n)));
                layout_slide(ctx, &s, None).unwrap()
            };
            // The first count at which middle placement equals top placement
            // is the first stack that no longer fits the padded area.
            let n = (1..40)
                .find(|&n| {
                    let top = first_baseline(&placed(n, "top"));
                    (first_baseline(&placed(n, "middle")) - top).abs() < 1e-3
                })
                .expect("some count of bullets overflows the padded area");
            let ops = placed(n, "middle");
            let last = texts(&ops).last().unwrap().2;
            let padded_bottom = ctx.height - ctx.pad_y();
            assert!(
                last > padded_bottom,
                "the stack runs into the bottom padding"
            );
            assert!(last < ctx.height, "but stays on the page");
        });
    }

    #[test]
    fn ink_bottom_counts_descenders_and_boxes() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let text = Op::Text {
                x: 0.0,
                y: 100.0,
                font: FontKind::Regular,
                size: 20.0,
                color: cfg.colors.text,
                text: "g".into(),
            };
            let descent = -ctx.fonts.regular.descent * 20.0;
            assert!(descent > 0.0);
            assert!(
                (ink_bottom(ctx, std::slice::from_ref(&text)) - (100.0 + descent)).abs() < 1e-4
            );
            let rect = Op::Rect {
                x: 0.0,
                y: 90.0,
                w: 5.0,
                h: 30.0,
                radius: 0.0,
                color: cfg.colors.text,
            };
            assert_eq!(ink_bottom(ctx, &[text, rect]), 120.0);
        });
    }

    #[test]
    fn the_last_lines_leading_may_hang_past_the_page() {
        // Find the largest bullet count that lays out; its last line box
        // may end below the page as long as the text itself does not.
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let fits = |n: usize| layout_slide(ctx, &one_slide(&bullets_yaml(n)), None).is_ok();
            let n = (1..60).take_while(|&n| fits(n)).last().unwrap();
            let ops = layout_slide(ctx, &one_slide(&bullets_yaml(n)), None).unwrap();
            assert!(ink_bottom(ctx, &ops) <= ctx.height);
            let next = layout_slide(ctx, &one_slide(&bullets_yaml(n + 1)), None);
            assert!(next.is_err(), "one more bullet runs off the page");
        });
    }

    #[test]
    fn content_running_off_the_page_is_an_error() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let err = layout_slide(ctx, &one_slide(&bullets_yaml(40)), None)
                .err()
                .expect("must fail");
            assert!(err.to_string().contains("does not fit"), "{err}");
        });
    }

    #[test]
    fn table_keeps_its_natural_width_when_it_fits() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let t = table_slide("  headers: [A, B]\n  rows:\n    - [x, y]");
            let width = ctx.content_width();
            let block = table_block(ctx, &t, 0.0, width, Align::Left).unwrap();
            let Op::Rect { w, .. } = &block.ops[0] else {
                panic!("the table starts with its border rectangle")
            };
            assert!(
                *w < width / 2.0,
                "a tiny table must not stretch: {w} of {width}"
            );
            assert_eq!(texts(&block.ops).len(), 4, "one line per cell");
        });
    }

    #[test]
    fn cells_of_a_table_that_fits_stay_on_one_line() {
        // Columns get exactly their natural width, so re-wrapping a cell to
        // that width must not break it (a float rounding hazard).
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            for row in [
                "['-o', Show timer information]",
                "[a b c, the quick brown fox jumps]",
                "['x', All sockets (listening + non-listening)]",
            ] {
                let t = table_slide(&format!("  headers: [Flag, Meaning]\n  rows:\n    - {row}"));
                let block = table_block(ctx, &t, 0.0, ctx.content_width(), Align::Left).unwrap();
                let baselines: Vec<f32> = texts(&block.ops)
                    .iter()
                    .skip(2) // the header row
                    .map(|(_, _, y)| *y)
                    .collect();
                assert!(
                    baselines.iter().all(|y| (y - baselines[0]).abs() < 1e-3),
                    "{row}: a body cell wrapped although the table fits"
                );
            }
        });
    }

    #[test]
    fn too_wide_table_fills_the_width_and_wraps_cells() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let long = "word ".repeat(60);
            let t = table_slide(&format!(
                "  headers: [Name, Description]\n  rows:\n    - [short, '{long}']"
            ));
            let narrow = table_slide("  headers: [Name, Description]\n  rows:\n    - [short, d]");
            let width = ctx.content_width();
            let wide = table_block(ctx, &t, 0.0, width, Align::Left).unwrap();
            let base = table_block(ctx, &narrow, 0.0, width, Align::Left).unwrap();
            let Op::Rect { w, .. } = &wide.ops[0] else {
                panic!("the table starts with its border rectangle")
            };
            assert!(
                (w - width).abs() < 1e-2,
                "shrunk to the content width: {w} vs {width}"
            );
            assert!(
                wide.height > 2.0 * base.height,
                "the long cell wraps onto several lines"
            );
            // Every word is still drawn, inside the table.
            let words = texts(&wide.ops)
                .iter()
                .filter(|(t, ..)| *t == "word")
                .count();
            assert_eq!(words, 60);
            assert!(texts(&wide.ops).iter().all(|(_, x, _)| *x < width));
        });
    }

    #[test]
    fn table_too_wide_even_when_wrapped_is_an_error() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let word = "x".repeat(300);
            let t = table_slide(&format!("  headers: [A]\n  rows:\n    - ['{word}']"));
            let err = table_block(ctx, &t, 0.0, ctx.content_width(), Align::Left)
                .err()
                .expect("must fail");
            assert!(
                err.to_string().contains("even with every cell wrapped"),
                "{err}"
            );
        });
    }

    #[test]
    fn auto_alignment_centers_the_header_and_left_aligns_the_body() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let t = table_slide("  headers: [H]\n  rows:\n    - [a much longer body cell]");
            let block = table_block(ctx, &t, 0.0, ctx.content_width(), Align::Left).unwrap();
            let Op::Rect { w: table_w, .. } = &block.ops[0] else {
                panic!("the table starts with its border rectangle")
            };
            let border = cfg.table.border_em * ctx.em;
            let pad = cfg.table.cell_padding_x_em * ctx.em;
            let found = texts(&block.ops);
            let header = found.iter().find(|(t, ..)| *t == "H").unwrap();
            let body = found.iter().find(|(t, ..)| *t == "a").unwrap();
            assert!(
                (body.1 - (border / 2.0 + pad)).abs() < 1e-3,
                "body starts at the padding"
            );
            let header_center = header.1 + ctx.fonts.bold.width("H", ctx.em) / 2.0;
            let col_center = border / 2.0 + (table_w - border) / 2.0;
            assert!(
                (header_center - col_center).abs() < 1e-2,
                "{header_center} vs {col_center}"
            );
        });
    }

    #[test]
    fn code_that_fits_is_not_scaled() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let s = code_width_scale(ctx, &["fn main() {}\n"], ctx.content_width());
            assert_eq!(s, 1.0, "code is never enlarged");
        });
    }

    #[test]
    fn code_wider_than_the_slide_shrinks_to_fit_its_widest_line() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let line = "x".repeat(400);
            let width = ctx.content_width();
            let s = code_width_scale(ctx, &["short", &line], width);
            assert!(s < 1.0);
            let text = ctx
                .fonts
                .mono
                .width(&line, ctx.em * cfg.code.font_size_em * s);
            let chrome = 2.0 * (cfg.code.padding_em * ctx.em * s + cfg.code.border_em * ctx.em);
            assert!(
                (text + chrome - width).abs() < 1e-2,
                "{} vs {width}",
                text + chrome
            );
        });
    }

    /// The lowest point any op of `ops` reaches: text baselines, box bottoms.
    fn lowest(ops: &[Op]) -> f32 {
        ops.iter()
            .map(|op| match op {
                Op::Text { y, .. } => *y,
                Op::Rect { y, h, .. } => y + h,
                Op::Ring { y, radius, .. } => y + radius,
                Op::Svg { y, .. } => *y,
            })
            .fold(f32::MIN, f32::max)
    }

    #[test]
    fn code_too_tall_for_the_slide_shrinks_to_fill_the_padded_area() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let source = "let x = 1;\n".repeat(200);
            let s = one_slide(&format!(
                "title: T\nbody:\n  - text: before\n  - code: {{source: {source:?}}}\n  - text: after"
            ));
            let ops = layout_slide(ctx, &s, None).unwrap();
            let sizes = mono_sizes(&ops);
            assert!(sizes[0] < ctx.em * cfg.code.font_size_em, "the code shrank");
            let bottom = ctx.height - ctx.pad_y();
            let low = lowest(&ops);
            assert!(
                low <= bottom + 1e-2 && low > bottom - ctx.em,
                "{low} vs {bottom}"
            );
        });
    }

    #[test]
    fn code_blocks_share_one_scale() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let a = "let a = 1;\n".repeat(30);
            let b = "let b = 2;\n".repeat(30);
            let s = one_slide(&format!(
                "title: T\nbody:\n  - code: {{source: {a:?}}}\n  - text: between\n  - code: {{source: {b:?}}}"
            ));
            let ops = layout_slide(ctx, &s, None).unwrap();
            let sizes = mono_sizes(&ops);
            assert_eq!(sizes.len(), 60);
            assert!(
                sizes.iter().all(|&z| (z - sizes[0]).abs() < 1e-4),
                "one size"
            );
            assert!(sizes[0] < ctx.em * cfg.code.font_size_em);
            assert!(lowest(&ops) <= ctx.height - ctx.pad_y() + 1e-2);
        });
    }

    #[test]
    fn every_code_block_is_drawn_in_order() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let s = one_slide(
                "title: T\ncode:\n  - {language: bash, source: echo first}\n  - {source: echo second}",
            );
            let ops = layout_slide(ctx, &s, None).unwrap();
            let found = texts(&ops);
            let y = |word: &str| found.iter().find(|(t, ..)| t.contains(word)).unwrap().2;
            assert!(y("first") < y("second"), "the blocks stack in source order");
        });
    }

    #[test]
    fn code_with_no_room_left_is_an_error() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let yaml = format!("{}\ncode: {{source: x}}", bullets_yaml(40));
            let err = layout_slide(ctx, &one_slide(&yaml), None)
                .err()
                .expect("must fail");
            assert!(err.to_string().contains("no room left"), "{err}");
        });
    }

    // ── source order and lists ────────────────────────────────────────────

    #[test]
    fn body_is_drawn_top_to_bottom_in_order() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let s = one_slide(
                "title: T\nbody:\n  - text: alpha\n  - code: {source: beta}\n  - bullets: [gamma]\n  - text: delta",
            );
            let ops = layout_slide(ctx, &s, None).unwrap();
            let found = texts(&ops);
            let y = |w: &str| found.iter().find(|(t, ..)| t.contains(w)).unwrap().2;
            assert!(y("alpha") < y("beta") && y("beta") < y("gamma") && y("gamma") < y("delta"));
        });
    }

    #[test]
    fn nested_items_indent_and_follow_their_parent_without_a_gap() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let s = one_slide(
                "body:\n  - bullets:\n      - text: parent\n        bullets: [child]\n      - sibling",
            );
            let ops = layout_slide(ctx, &s, None).unwrap();
            let found = texts(&ops);
            let at = |w: &str| *found.iter().find(|(t, ..)| *t == w).unwrap();
            let (parent, child, sibling) = (at("parent"), at("child"), at("sibling"));
            let indent = cfg.list.indent_em * ctx.em;
            let lh = cfg.slide.line_height * ctx.em;
            let gap = cfg.list.item_gap_em * ctx.em;
            assert!(
                (child.1 - parent.1 - indent).abs() < 1e-3,
                "one more indent"
            );
            assert!(
                (child.2 - parent.2 - lh).abs() < 1e-3,
                "no gap before a nested list"
            );
            assert!(
                (sibling.2 - child.2 - lh - gap).abs() < 1e-3,
                "item gap between siblings"
            );
            assert!((sibling.1 - parent.1).abs() < 1e-3);
        });
    }

    #[test]
    fn markers_follow_the_nesting_depth() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let numbered = one_slide(
                "body:\n  - numbered:\n      - one\n      - text: two\n        numbered:\n          - text: x\n            numbered: [y]\n      - three",
            );
            let ops = layout_slide(ctx, &numbered, None).unwrap();
            let labels: Vec<&str> = texts(&ops)
                .iter()
                .map(|t| t.0)
                .filter(|t| t.ends_with('.'))
                .collect();
            assert_eq!(labels, ["1.", "2.", "i.", "a.", "3."]);

            let bullets = one_slide(
                "body:\n  - bullets:\n      - text: a\n        bullets:\n          - text: b\n            bullets: [c]",
            );
            let ops = layout_slide(ctx, &bullets, None).unwrap();
            let size = cfg.list.disc_size_em * ctx.em;
            let shapes: Vec<&str> = ops
                .iter()
                .filter_map(|op| match op {
                    Op::Rect { radius, w, .. } if (*w - size).abs() < 1e-3 => {
                        Some(if *radius > 0.0 { "disc" } else { "square" })
                    }
                    Op::Ring { .. } => Some("ring"),
                    _ => None,
                })
                .collect();
            assert_eq!(shapes, ["disc", "ring", "square"]);
        });
    }

    #[test]
    fn numbered_marker_ends_one_space_before_the_text() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let ops = layout_slide(ctx, &one_slide("body:\n  - numbered: [word]"), None).unwrap();
            let found = texts(&ops);
            let marker = found.iter().find(|t| t.0 == "1.").unwrap();
            let text = found.iter().find(|t| t.0 == "word").unwrap();
            let end = marker.1 + ctx.fonts.regular.width("1. ", ctx.em);
            assert!((end - text.1).abs() < 1e-3, "{end} vs {}", text.1);
        });
    }

    #[test]
    fn roman_and_alpha_counters() {
        let r: Vec<String> = [1, 2, 3, 4, 9, 14, 40, 1994]
            .iter()
            .map(|&n| roman(n))
            .collect();
        assert_eq!(r, ["i", "ii", "iii", "iv", "ix", "xiv", "xl", "mcmxciv"]);
        let a: Vec<String> = [1, 2, 26, 27, 52, 53].iter().map(|&n| alpha(n)).collect();
        assert_eq!(a, ["a", "b", "z", "aa", "az", "ba"]);
    }

    #[test]
    fn content_below_a_picture_follows_it() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let svg = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 4 2'><rect width='4' height='2'/></svg>";
            let s = one_slide(&format!(
                "title: T\nbody:\n  - text: above\n  - svg: \"{svg}\"\n  - bullets: [below]"
            ));
            let ops = layout_slide(ctx, &s, None).unwrap();
            let found = texts(&ops);
            let y = |w: &str| found.iter().find(|(t, ..)| *t == w).unwrap().2;
            let (pic_top, pic_bottom) = ops
                .iter()
                .find_map(|op| match op {
                    Op::Svg { tree, y, scale, .. } => Some((*y, y + tree.size().height() * scale)),
                    _ => None,
                })
                .unwrap();
            assert!(
                y("above") < pic_top && pic_bottom < y("below"),
                "above, picture, below"
            );
            assert!(
                ink_bottom(ctx, &ops) <= ctx.height * (1.0 - cfg.svg.margin_bottom_ratio) + 1e-2
            );

            let two = one_slide(&format!("body:\n  - svg: \"{svg}\"\n  - svg: \"{svg}\""));
            let err = layout_slide(ctx, &two, None).err().expect("must fail");
            assert!(err.to_string().contains("one image or svg"), "{err}");
        });
    }

    #[test]
    fn page_number_is_right_aligned_at_the_configured_position() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let Op::Text { x, text, size, .. } = page_number_op(ctx, 12) else {
                panic!("the page number is text")
            };
            assert_eq!(text, "12");
            let right = x + ctx.fonts.regular.width("12", size);
            let expected = ctx.width * (1.0 - cfg.pagination.right_ratio);
            assert!((right - expected).abs() < 1e-3, "{right} vs {expected}");
        });
    }

    #[test]
    fn render_produces_a_pdf_for_every_feature() {
        let yaml = format!(
            "title: Deck\npaginate: true\nslides:\n  - {}\n  - {}\n",
            FULL_SLIDE.replace('\n', "\n    "),
            SVG_SLIDE.replace('\n', "\n    ")
        );
        let pdf = render(&deck(&yaml), &Config::default()).unwrap();
        assert!(pdf.starts_with(b"%PDF-"), "not a PDF");
    }

    #[test]
    fn render_reports_every_slide_that_does_not_fit() {
        let bad = bullets_yaml(40).replace('\n', "\n    ");
        let yaml = format!("slides:\n  - title: ok\n  - {bad}\n  - title: also ok\n  - {bad}\n");
        let err = format!(
            "{:#}",
            render(&deck(&yaml), &Config::default()).unwrap_err()
        );
        assert!(err.contains("2 slide(s)"), "{err}");
        assert!(
            err.contains("slide 2/4") && err.contains("slide 4/4"),
            "{err}"
        );
        assert!(
            !err.contains("slide 1/4") && !err.contains("slide 3/4"),
            "{err}"
        );
    }

    #[test]
    fn unknown_code_theme_is_an_error() {
        let mut cfg = Config::default();
        cfg.code.theme = "NoSuchTheme".into();
        let bytes = FontBytes::read(&cfg.fonts).unwrap();
        let fonts = bytes.load().unwrap();
        let err = make_ctx(&cfg, &bytes, &fonts).err().expect("must fail");
        assert!(err.to_string().contains("NoSuchTheme"), "{err}");
    }
}
