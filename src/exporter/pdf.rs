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

use crate::config::{Color, Config, SvgSize};
use crate::exporter::highlight::Highlighter;
use crate::exporter::text::{
    FontBytes, FontKind, FontSet, Line, Span, TextMetrics, parse_inline, wrap, wrap_within,
};
use crate::model::{
    Align, Background, BackgroundSize, Column, Element, List, Presentation, Side, Slide, Table,
    TableAlign, VAlign,
};
use crate::theme::{Palette, SharedThemes};
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
use std::cell::RefCell;
use std::path::Path as StdPath;
use std::sync::Arc;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::util::LinesWithEndings;
use usvg::fontdb::Database;

const PT_PER_MM: f32 = 72.0 / 25.4;

/// Bezier handle length for a quarter circle of radius 1.
const KAPPA: f32 = 0.552_284_8;

/// What content running off the page is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    /// An error naming the slide (the default).
    Error,
    /// A warning naming the slide; the slide is drawn anyway, cut at the
    /// page edge as marp cuts it.
    Warn,
}

/// Everything the layout code needs that is shared by all slides.
struct Ctx<'a> {
    cfg: &'a Config,
    /// The deck's page palette, when its theme names one: diagrams are
    /// rethemed with it (the colours are already in `cfg.colors`).
    palette: Option<&'a Palette>,
    fonts: &'a FontSet<'a>,
    fontdb: Arc<Database>,
    svg_settings: SvgSettings,
    highlighter: &'a Highlighter,
    theme: &'a Theme,
    /// Page size in points.
    width: f32,
    height: f32,
    /// Base font size (1 em) in points.
    em: f32,
    overflow: Overflow,
    /// Overflow warnings of the slide being laid out (see `Overflow::Warn`).
    warnings: RefCell<Vec<String>>,
}

impl Ctx<'_> {
    fn pad_x(&self) -> f32 {
        self.width * self.cfg.slide.padding_x_ratio
    }
    fn pad_y(&self) -> f32 {
        self.height * self.cfg.slide.padding_y_ratio
    }
    /// The content width of a slide without a background.
    #[cfg(test)]
    fn content_width(&self) -> f32 {
        self.content_width_in(self.width)
    }
    /// The width inside the paddings of a region `region_w` wide.
    fn content_width_in(&self, region_w: f32) -> f32 {
        region_w - 2.0 * self.pad_x()
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

/// Write `presentation` as a PDF to `output_path`. Returns the overflow
/// warnings, each naming its slide (only with `Overflow::Warn`).
/// What every deck of a run shares, built once: the configured font files,
/// the font database SVG text is set with, the syntax highlighter and the
/// code theme. Decks may be rendered from it on several threads at once.
pub struct Engine<'c> {
    cfg: &'c Config,
    bytes: FontBytes,
    fontdb: Arc<Database>,
    highlighter: Highlighter,
    theme: Theme,
    /// The page palettes, when `[theme] shared_themes` is configured.
    shared: Option<SharedThemes>,
}

impl<'c> Engine<'c> {
    pub fn new(cfg: &'c Config) -> Result<Self> {
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
        let bytes = FontBytes::read(&cfg.fonts)?;
        let fontdb = build_fontdb(bytes.regular(), bytes.mono(), &bytes.others())?;
        let shared = match &cfg.theme.shared_themes {
            Some(dir) => Some(
                SharedThemes::load(dir)
                    .with_context(|| format!("loading shared themes from {}", dir.display()))?,
            ),
            None => None,
        };
        Ok(Self {
            cfg,
            bytes,
            fontdb,
            highlighter: Highlighter::new()?,
            theme,
            shared,
        })
    }

    /// Write `presentation` as a PDF to `output_path`. Returns the overflow
    /// warnings, each naming its slide (only with `Overflow::Warn`).
    pub fn export(
        &self,
        presentation: &Presentation,
        output_path: &StdPath,
        overflow: Overflow,
    ) -> Result<Vec<String>> {
        let (pdf, warnings) = self.render(presentation, overflow)?;
        std::fs::write(output_path, pdf)
            .with_context(|| format!("failed to write {}", output_path.display()))?;
        Ok(warnings)
    }

    /// The layout context over this deck's font handles, for `cfg` (the
    /// engine's configuration, or a copy coloured by the deck's palette).
    fn ctx<'a>(
        &'a self,
        cfg: &'a Config,
        palette: Option<&'a Palette>,
        fonts: &'a FontSet<'a>,
    ) -> Ctx<'a> {
        let width = cfg.slide.width_mm * PT_PER_MM;
        let height = cfg.slide.height_mm * PT_PER_MM;
        Ctx {
            cfg,
            palette,
            fonts,
            fontdb: Arc::clone(&self.fontdb),
            svg_settings: SvgSettings {
                filter_scale: cfg.svg.filter_scale,
                ..SvgSettings::default()
            },
            highlighter: &self.highlighter,
            theme: &self.theme,
            width,
            height,
            em: width * cfg.slide.font_size_ratio,
            overflow: Overflow::Error,
            warnings: RefCell::new(Vec::new()),
        }
    }

    /// Render `presentation` to the bytes of a PDF.
    fn render(
        &self,
        presentation: &Presentation,
        overflow: Overflow,
    ) -> Result<(Vec<u8>, Vec<String>)> {
        let fonts = self.bytes.load()?;
        // A deck in a page palette renders with that palette's colours: a
        // copy of the configuration carries them, the engine's stays as is.
        let palette = crate::theme::resolve(
            self.cfg,
            self.shared.as_ref(),
            presentation.theme.as_deref(),
        )?;
        let coloured;
        let cfg = match palette {
            Some(p) => {
                coloured = Config {
                    colors: p.colors.clone(),
                    ..self.cfg.clone()
                };
                &coloured
            }
            None => self.cfg,
        };
        let mut ctx = self.ctx(cfg, palette, &fonts);
        ctx.overflow = overflow;
        render_with(&ctx, presentation)
    }
}

/// Render `presentation` to the bytes of a PDF, with the overflow
/// warnings, in `ctx`.
fn render_with(ctx: &Ctx, presentation: &Presentation) -> Result<(Vec<u8>, Vec<String>)> {
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
    let mut warnings = Vec::new();
    for (i, slide) in slides.iter().enumerate() {
        let laid_out = layout_slide(ctx, slide, paginate.then_some(i + 1)).and_then(|mut ops| {
            if i == 0 && ctx.cfg.credit.enabled {
                ops.push(credit_op(ctx)?);
            }
            Ok(ops)
        });
        match laid_out {
            Ok(ops) => layouts.push(ops),
            Err(e) => errors.push(format!("slide {}/{}: {e:#}", i + 1, total)),
        }
        for w in ctx.warnings.borrow_mut().drain(..) {
            warnings.push(format!("slide {}/{}: {w}", i + 1, total));
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
        // The page background, when it is not the paper's own white. Drawn
        // here rather than as an op: it is not content, so it must not count
        // for the overflow checks that read the ops.
        if ctx.cfg.colors.background != Color(0xff, 0xff, 0xff) {
            draw_ops(
                &mut surface,
                ctx,
                vec![Op::Rect {
                    x: 0.0,
                    y: 0.0,
                    w: ctx.width,
                    h: ctx.height,
                    radius: 0.0,
                    color: ctx.cfg.colors.background,
                }],
            )
            .with_context(|| format!("slide {}/{} background", i + 1, total))?;
        }
        draw_ops(&mut surface, ctx, ops).with_context(|| format!("slide {}/{}", i + 1, total))?;
        surface.finish();
        page.finish();
    }

    let pdf = doc
        .finish()
        .map_err(|e| anyhow::anyhow!("PDF finalisation failed: {e:?}"))?;
    Ok((pdf, warnings))
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
    // For a character an SVG's font lacks, usvg takes the first face that
    // has it, so the monospace face goes last: a sans fallback (as a
    // browser would pick) comes first.
    let mut db = Database::new();
    for bytes in std::iter::once(&regular).chain(others).chain([&mono]) {
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
    /// A background picture drawn at `x`, `y` at `w` x `h`, cut to the
    /// rectangle `clip` (x, y, w, h).
    Background {
        picture: Picture,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        clip: [f32; 4],
    },
}

/// A picture file's contents, ready to draw.
enum Picture {
    Raster(krilla::image::Image),
    Svg(Box<usvg::Tree>),
}

impl Picture {
    /// Natural size: pixels for a raster, the SVG's size for an SVG.
    fn size(&self) -> (f32, f32) {
        match self {
            Picture::Raster(image) => {
                let (w, h) = image.size();
                (w as f32, h as f32)
            }
            Picture::Svg(tree) => (tree.size().width(), tree.size().height()),
        }
    }
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
            Op::Background { x, y, clip, .. } => {
                *x += dx;
                *y += dy;
                clip[0] += dx;
                clip[1] += dy;
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
    // The slide's content runs across the page, or beside its background.
    let (region_x, region_w) = match &slide.background {
        None => (0.0, ctx.width),
        Some(bg) => {
            let content_w = ctx.width * (1.0 - bg.split);
            match bg.side {
                Side::Left => (ctx.width - content_w, content_w),
                Side::Right => (0.0, content_w),
            }
        }
    };
    let x0 = region_x + ctx.pad_x();
    let width = ctx.content_width_in(region_w);
    if width <= 0.0 {
        bail!("no room for the content beside the background");
    }
    // In "fill" mode the picture is laid out separately, in the space the
    // rest leaves; in "width" mode it is an ordinary block of the body.
    let Split {
        above: body,
        picture,
        below: after,
    } = if cfg.svg.size == SvgSize::Fill {
        split_picture(&slide.body)?
    } else {
        Split {
            above: &slide.body,
            picture: None,
            below: &[],
        }
    };

    let area = Area {
        x0,
        width,
        align: content_align,
    };
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
            )?);
        }
        for element in body {
            blocks.extend(element_blocks(ctx, element, area, scale, false)?);
        }
        Ok(blocks)
    };

    let top = ctx.pad_y();
    let available = ctx.height - 2.0 * top;
    let mut sources: Vec<(&str, f32)> = Vec::new();
    code_sources(ctx, body, width, &mut sources);
    code_sources(ctx, after, width, &mut sources);
    // Code first shrinks to fit the width, like marp's auto-scaling of
    // `<pre>`; all of a slide's code shares one factor so it reads at one
    // size.
    let mut scale = code_width_scale(ctx, &sources);
    let mut blocks = build(scale)?;
    // Content taller than the padded area may run into the bottom padding,
    // as below; only content that would run off the page shrinks its code.
    let runs_off = |blocks: Vec<Block>| {
        let mut placed = Vec::new();
        place(blocks, top, &mut placed);
        ink_bottom(ctx, &placed) > ctx.height
    };
    if picture.is_none() && !sources.is_empty() && runs_off(build(scale)?) {
        // Then the code shrinks to fit the padded area. The stack height is
        // linear in the scale (only the code's text and padding scale), so
        // two layouts give the exact factor.
        let full = stack_height(&blocks);
        let half = stack_height(&build(scale / 2.0)?);
        let per_unit = 2.0 * (full - half) / scale;
        let fixed = full - per_unit * scale;
        let fit = (available - fixed) / per_unit;
        if fit > 0.0 {
            scale = fit.min(scale);
            blocks = build(scale)?;
        } else if ctx.overflow == Overflow::Error {
            bail!("no room left for the code below the slide's other content");
        }
        // With `Overflow::Warn`, the code keeps its size and the content
        // runs off the page, which the check below reports.
    }
    let stack = stack_height(&blocks);
    let mut ops = Vec::new();

    if let Some(svg) = picture {
        // The picture takes all the room the other elements leave, so they
        // sit at the top and vertical alignment does not apply.
        let mut below = Vec::new();
        for element in after {
            below.extend(element_blocks(ctx, element, area, scale, false)?);
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
        let (op, height) = svg_op(ctx, &svg, picture_top, reserved, region_x, region_w)?;
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
            let message = format!(
                "content does not fit on the slide: it is {:.0}% of the page height \
                 below the top padding; split the slide or shorten it",
                stack / (ctx.height - top) * 100.0
            );
            match ctx.overflow {
                Overflow::Error => bail!(message),
                Overflow::Warn => ctx.warnings.borrow_mut().push(message),
            }
        }
    }

    if let Some(n) = page_number {
        ops.push(page_number_op(ctx, n, region_x + region_w));
    }
    if let Some(bg) = &slide.background {
        ops.insert(0, background_op(ctx, bg)?);
    }
    // Every character must be in one of its role's fonts.
    for op in &ops {
        if let Op::Text { font, text, .. } = op {
            ctx.fonts.get(*font).runs(text)?;
        }
    }
    Ok(ops)
}

/// A split background: the picture placed in its side of the slide as
/// CSS `background-size` places it (centred), and cut to that side.
fn background_op(ctx: &Ctx, bg: &Background) -> Result<Op> {
    let picture = load_picture(ctx, &bg.image)?;
    let area_w = ctx.width * bg.split;
    let area_x = match bg.side {
        Side::Left => 0.0,
        Side::Right => ctx.width - area_w,
    };
    let (nw, nh) = picture.size();
    if nw <= 0.0 || nh <= 0.0 {
        bail!("background {:?} has no size: {nw}x{nh}", bg.image);
    }
    let scale = match bg.size {
        BackgroundSize::Cover => (area_w / nw).max(ctx.height / nh),
        BackgroundSize::Contain => (area_w / nw).min(ctx.height / nh),
        BackgroundSize::Width(share) => area_w * share / nw,
    };
    let (w, h) = (nw * scale, nh * scale);
    Ok(Op::Background {
        picture,
        x: area_x + (area_w - w) / 2.0,
        y: (ctx.height - h) / 2.0,
        w,
        h,
        clip: [area_x, 0.0, area_w, ctx.height],
    })
}

/// The picture file at `path`: SVG, PNG or JPEG, by its extension.
fn load_picture(ctx: &Ctx, path: &str) -> Result<Picture> {
    let lower = path.to_lowercase();
    if lower.ends_with(".svg") {
        let svg = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read SVG file {path:?}"))?;
        return Ok(Picture::Svg(Box::new(parse_svg(ctx, &svg)?)));
    }
    let bytes = std::fs::read(path).with_context(|| format!("failed to read image {path:?}"))?;
    let data = krilla::Data::from(bytes);
    let image = if lower.ends_with(".png") {
        krilla::image::Image::from_png(data, true)
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        krilla::image::Image::from_jpeg(data, true)
    } else {
        bail!("image {path:?} is not an SVG, PNG or JPEG file");
    };
    image
        .map(Picture::Raster)
        .map_err(|e| anyhow::anyhow!("failed to decode image {path:?}: {e}"))
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
            // The drawing, not its canvas: an SVG's empty canvas may run off
            // the page (marp sizes 1280x720 diagrams to the content width).
            // The stroke box, not the layer box: a filter's region (a drop
            // shadow allocates 10% beyond its shape) is not drawn ink. And
            // only what lies on the canvas: a browser clips an SVG image to
            // it, and so does `draw_ops`.
            Op::Svg { tree, y, scale, .. } => {
                y + canvas_ink_bottom(tree.root(), tree.size().height()) * scale
            }
            // A background is not content: it fills its side by design.
            Op::Background { .. } => f32::MIN,
        })
        .fold(f32::MIN, f32::max)
}

/// The lowest point of what `group` draws on a canvas `height` tall, in
/// the SVG's units: an element wholly below the canvas is not drawn (a
/// browser clips an SVG image to its canvas), one that crosses its edge is
/// cut there.
fn canvas_ink_bottom(group: &usvg::Group, height: f32) -> f32 {
    group
        .children()
        .iter()
        .map(|node| match node {
            usvg::Node::Group(g) => canvas_ink_bottom(g, height),
            leaf => {
                let b = leaf.abs_stroke_bounding_box();
                if b.top() >= height {
                    f32::MIN
                } else {
                    b.bottom().min(height)
                }
            }
        })
        .fold(f32::MIN, f32::max)
}

/// The blocks one body element lays out as. A top-level picture in "fill"
/// mode is handled by the caller (see `split_picture`); a `nested` one (in
/// a column or a quote) is always an ordinary block.
fn element_blocks(
    ctx: &Ctx,
    element: &Element,
    area: Area,
    code_scale: f32,
    nested: bool,
) -> Result<Vec<Block>> {
    let Area { x0, width, align } = area;
    Ok(match element {
        Element::Text(text) => paragraphs(text)
            .iter()
            .map(|p| paragraph_block(ctx, p, x0, width, align))
            .collect::<Result<_>>()?,
        Element::Heading(h) => vec![heading_block(ctx, &h.text, h.level, x0, width, align)?],
        Element::List(list) => vec![list_block(ctx, list, x0, width, align)?],
        Element::Code(code) => vec![code_block(
            ctx,
            code_source(code),
            code.language.as_deref(),
            x0,
            width,
            code_scale,
        )?],
        Element::Table(table) => vec![table_block(ctx, table, x0, width, align)?],
        Element::Columns(columns) => vec![columns_block(ctx, columns, area, code_scale)?],
        Element::Quote(body) => vec![quote_block(ctx, body, area, code_scale)?],
        Element::Image(_) | Element::Svg(_) => {
            if ctx.cfg.svg.size == SvgSize::Fill && !nested {
                unreachable!("split_picture removes the picture in fill mode")
            }
            vec![picture_block(
                ctx,
                &picture_source(element)?,
                x0,
                width,
                align,
            )?]
        }
    })
}

/// Every code block of `elements` (also those in columns and quotes) with
/// the width it is drawn at.
fn code_sources<'a>(ctx: &Ctx, elements: &'a [Element], width: f32, out: &mut Vec<(&'a str, f32)>) {
    for element in elements {
        match element {
            Element::Code(code) => out.push((code_source(code), width)),
            Element::Columns(columns) => {
                let col_w = column_width(ctx, columns.len(), width);
                for col in columns {
                    code_sources(ctx, &col.body, col_w, out);
                }
            }
            Element::Quote(body) => code_sources(ctx, body, quote_inner_width(ctx, width), out),
            _ => {}
        }
    }
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
    let svg = picture_source(&body[i])?;
    Ok(Split {
        above: &body[..i],
        picture: Some(svg),
        below: &body[i + 1..],
    })
}

/// The SVG source of an `image` (read from its file) or `svg` element.
fn picture_source(element: &Element) -> Result<String> {
    match element {
        Element::Svg(svg) => Ok(svg.clone()),
        Element::Image(path) => {
            if !path.to_lowercase().ends_with(".svg") {
                bail!("image path {path:?} is not an .svg (raster images not yet supported)");
            }
            std::fs::read_to_string(path)
                .with_context(|| format!("failed to read SVG file {path:?}"))
        }
        _ => unreachable!("only pictures have an SVG source"),
    }
}

/// A picture as a block in the flow, sized as marp sizes an `<img>` in a
/// paragraph: its own width (its SVG size, in marp's CSS px) but no wider
/// than the content area (`max-width: 100%`), keeping its aspect ratio.
fn picture_block(ctx: &Ctx, svg: &str, x0: f32, width: f32, align: Align) -> Result<Block> {
    let cfg = ctx.cfg;
    let tree = parse_svg(ctx, svg)?;
    let pt_per_px = ctx.width / cfg.svg.slide_width_px;
    let natural = tree.size().width() * pt_per_px;
    let (scale, w, h) = fit(&tree, natural.min(width), f32::INFINITY)?;
    // The image sits on the baseline of its paragraph's only line, which
    // reaches below it by the text's descent and half-leading.
    let style = body_style(ctx);
    let line = style.line_height * ctx.em;
    let below = line - baseline_offset(ctx, FontKind::Regular, ctx.em, style.line_height);
    Ok(Block {
        ops: vec![Op::Svg {
            tree: Box::new(tree),
            x: line_x(x0, width, w, align),
            y: 0.0,
            scale,
        }],
        height: h + below,
        margin_top: 0.0,
        margin_bottom: cfg.block.margin_bottom_em * ctx.em,
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
) -> Result<(Vec<Op>, f32)> {
    let lh = style.line_height * style.metrics.size;
    let base = baseline_offset(ctx, style.font, style.metrics.size, style.line_height);
    let mut ops = Vec::new();
    let lines = wrap_within(spans, width, ctx.fonts, style.metrics)?;
    for (i, line) in lines.iter().enumerate() {
        let x = line_x(x0, width, line.width, align);
        line_ops(ctx, line, x, i as f32 * lh + base, style, &mut ops);
    }
    Ok((ops, lines.len() as f32 * lh))
}

fn heading_block(
    ctx: &Ctx,
    text: &str,
    level: u8,
    x0: f32,
    width: f32,
    align: Align,
) -> Result<Block> {
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
    let (ops, height) = text_lines(ctx, &spans, x0, width, align, &style)?;
    Ok(Block {
        ops,
        height: height + size * cfg.heading.padding_bottom_em[usize::from(level - 1)],
        margin_top: cfg.heading.margin_top_em * ctx.em,
        margin_bottom: cfg.heading.margin_bottom_em * ctx.em,
    })
}

fn body_style(ctx: &Ctx) -> TextStyle {
    TextStyle {
        metrics: ctx.body_metrics(),
        line_height: ctx.cfg.slide.line_height,
        color: ctx.cfg.colors.text,
        font: FontKind::Regular,
    }
}

fn paragraph_block(
    ctx: &Ctx,
    lines: &[String],
    x0: f32,
    width: f32,
    align: Align,
) -> Result<Block> {
    let style = body_style(ctx);
    let mut ops = Vec::new();
    let mut height = 0.0;
    for line in lines {
        let (mut line_ops, h) = text_lines(ctx, &parse_inline(line), x0, width, align, &style)?;
        line_ops.iter_mut().for_each(|op| op.shift(0.0, height));
        ops.extend(line_ops);
        height += h;
    }
    Ok(Block {
        ops,
        height,
        margin_top: 0.0,
        margin_bottom: ctx.cfg.block.margin_bottom_em * ctx.em,
    })
}

/// A bulleted or numbered list, with nested lists under their items.
fn list_block(ctx: &Ctx, list: &List, x0: f32, width: f32, align: Align) -> Result<Block> {
    let mut ops = Vec::new();
    let area = Area { x0, width, align };
    let height = list_ops(ctx, list, 1, area, 0.0, &mut ops)?;
    Ok(Block {
        ops,
        height,
        margin_top: 0.0,
        margin_bottom: ctx.cfg.block.margin_bottom_em * ctx.em,
    })
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
    list: &List,
    depth: usize,
    area: Area,
    mut y: f32,
    ops: &mut Vec<Op>,
) -> Result<f32> {
    let Area { x0, width, align } = area;
    let cfg = ctx.cfg;
    let style = body_style(ctx);
    let indent = cfg.list.indent_em * ctx.em;
    let text_x0 = x0 + indent;
    let text_w = width - indent;
    let lh = style.line_height * ctx.em;
    let base = baseline_offset(ctx, FontKind::Regular, ctx.em, style.line_height);
    for (i, item) in list.items.iter().enumerate() {
        if i > 0 {
            y += cfg.list.item_gap_em * ctx.em;
        }
        let lines = wrap_within(&parse_inline(&item.text), text_w, ctx.fonts, style.metrics)?;
        let first_x = lines
            .first()
            .map_or(text_x0, |l| line_x(text_x0, text_w, l.width, align));
        let number = list.start as usize + i;
        marker_ops(ctx, list.ordered, depth, number, first_x, y + base, ops);
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
            y = list_ops(ctx, children, depth + 1, inner, y, ops)?;
        }
    }
    Ok(y)
}

/// The marker of the item numbered `number` (counting from the list's
/// start) of a list at `depth`, for a first line that starts at `x` with
/// its baseline at `baseline`. As in browsers: bullets
/// are a disc, then a ring, then a square; numbers are decimal, then lower
/// roman, then lower alpha (GitHub's `ol ol` / `ol ol ol` styles).
fn marker_ops(
    ctx: &Ctx,
    ordered: bool,
    depth: usize,
    number: usize,
    x: f32,
    baseline: f32,
    ops: &mut Vec<Op>,
) {
    let cfg = ctx.cfg;
    let color = cfg.colors.text;
    if ordered {
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

/// The width of each of `n` columns sharing `width`.
fn column_width(ctx: &Ctx, n: usize, width: f32) -> f32 {
    let gap = ctx.cfg.columns.gap_em * ctx.em;
    (width - gap * (n as f32 - 1.0)) / n as f32
}

/// Side-by-side columns of equal width, each laying out its own body. As
/// in a CSS grid (marp's `.columns`), a column keeps the margins of its
/// first and last elements inside it instead of collapsing them with the
/// slide's other elements, and the columns block has no margins of its own.
fn columns_block(ctx: &Ctx, columns: &[Column], area: Area, code_scale: f32) -> Result<Block> {
    if columns.is_empty() {
        bail!("columns: must contain at least one column");
    }
    let cfg = ctx.cfg;
    let gap = cfg.columns.gap_em * ctx.em;
    let col_w = column_width(ctx, columns.len(), area.width);
    let mut ops = Vec::new();
    let mut height: f32 = 0.0;
    for (i, col) in columns.iter().enumerate() {
        let inner = Area {
            x0: area.x0 + i as f32 * (col_w + gap),
            width: col_w,
            align: area.align,
        };
        let mut blocks = Vec::new();
        for element in &col.body {
            blocks.extend(element_blocks(ctx, element, inner, code_scale, true)?);
        }
        let top = blocks.first().map_or(0.0, |b| b.margin_top);
        let bottom = blocks.last().map_or(0.0, |b| b.margin_bottom);
        height = height.max(top + stack_height(&blocks) + bottom);
        place(blocks, top, &mut ops);
    }
    Ok(Block {
        ops,
        height,
        margin_top: 0.0,
        margin_bottom: 0.0,
    })
}

/// The width left for a quote's content inside `width`: its border and
/// the padding on both sides.
fn quote_inner_width(ctx: &Ctx, width: f32) -> f32 {
    let q = &ctx.cfg.quote;
    width - (q.border_em + 2.0 * q.padding_em) * ctx.em
}

/// A blockquote as marp draws it: its content in the muted text color,
/// padded on both sides, with a bar down its left edge.
fn quote_block(ctx: &Ctx, body: &[Element], area: Area, code_scale: f32) -> Result<Block> {
    let cfg = ctx.cfg;
    let border = cfg.quote.border_em * ctx.em;
    let inner = Area {
        x0: area.x0 + border + cfg.quote.padding_em * ctx.em,
        width: quote_inner_width(ctx, area.width),
        align: area.align,
    };
    let mut blocks = Vec::new();
    for element in body {
        blocks.extend(element_blocks(ctx, element, inner, code_scale, true)?);
    }
    let height = stack_height(&blocks);
    let mut ops = vec![Op::Rect {
        x: area.x0,
        y: 0.0,
        w: border,
        h: height,
        radius: 0.0,
        color: cfg.colors.quote_border,
    }];
    let mut content = Vec::new();
    place(blocks, 0.0, &mut content);
    // Text in the body color becomes muted (CSS `color` inherits); links,
    // highlighted code and other colors keep theirs.
    for mut op in content {
        if let Op::Text { color, .. } | Op::Rect { color, .. } | Op::Ring { color, .. } = &mut op
            && *color == cfg.colors.text
        {
            *color = cfg.colors.quote_text;
        }
        ops.push(op);
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

    let base = baseline_offset(ctx, FontKind::Mono, size, cfg.code.line_height);
    let mono = &ctx.fonts.mono;
    let lines = ctx.highlighter.highlight(source, language, ctx.theme)?;
    for (i, runs) in lines.into_iter().enumerate() {
        let mut x = x0 + border + pad;
        let y = border + pad + i as f32 * lh + base;
        for (color, text) in runs {
            let width = mono.width(&text, size);
            ops.push(Op::Text {
                x,
                y,
                font: FontKind::Mono,
                size,
                color,
                text,
            });
            x += width;
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
/// block fits the width it is drawn at. Never enlarges. The border does
/// not scale; the padding and the text inside it do.
fn code_width_scale(ctx: &Ctx, sources: &[(&str, f32)]) -> f32 {
    let cfg = ctx.cfg;
    let border = 2.0 * cfg.code.border_em * ctx.em;
    let pad = 2.0 * cfg.code.padding_em * ctx.em;
    let size = ctx.em * cfg.code.font_size_em;
    sources
        .iter()
        .map(|(source, width)| {
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
    // In a page palette, a diagram installed from a diagram theme takes the
    // palette's variant of it before its variables are resolved.
    let rethemed;
    let svg = match ctx.palette {
        Some(palette) => {
            rethemed = crate::theme::retheme_svg(svg, palette);
            rethemed.as_str()
        }
        None => svg,
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
fn svg_op(
    ctx: &Ctx,
    svg: &str,
    top: f32,
    reserved: f32,
    region_x: f32,
    region_w: f32,
) -> Result<(Op, f32)> {
    let cfg = ctx.cfg;
    let margin_x = ctx.width * cfg.svg.margin_x_ratio;
    let bottom = ctx.height * (1.0 - cfg.svg.margin_bottom_ratio) - reserved;
    let avail_w = region_w - 2.0 * margin_x;
    let avail_h = bottom - top;
    if avail_h <= 0.0 {
        bail!("no room left for the SVG between the slide's other content");
    }
    let tree = parse_svg(ctx, svg)?;
    let (scale, w, h) = fit(&tree, avail_w, avail_h)?;
    let op = Op::Svg {
        tree: Box::new(tree),
        x: region_x + margin_x + (avail_w - w) / 2.0,
        y: top,
        scale,
    };
    Ok((op, h))
}

/// The page number, right-aligned at marp's pagination position in the
/// content region that ends at `region_right`.
fn page_number_op(ctx: &Ctx, n: usize, region_right: f32) -> Op {
    let cfg = ctx.cfg;
    let size = ctx.em * cfg.pagination.font_size_em;
    let text = n.to_string();
    let right = region_right - ctx.width * cfg.pagination.right_ratio;
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

/// The line crediting rsslide, at the bottom-left of the page, mirroring
/// the page number's position at the bottom-right.
fn credit_op(ctx: &Ctx) -> Result<Op> {
    let cfg = ctx.cfg;
    let size = ctx.em * cfg.credit.font_size_em;
    let text = format!(
        "Created by rsslide {} by Mark Veltzer <mark.veltzer@gmail.com>",
        env!("CARGO_PKG_VERSION")
    );
    ctx.fonts.get(FontKind::Regular).runs(&text)?;
    let box_bottom = ctx.height * (1.0 - cfg.credit.bottom_ratio);
    let lh = cfg.slide.line_height;
    let baseline = box_bottom - lh * size + baseline_offset(ctx, FontKind::Regular, size, lh);
    Ok(Op::Text {
        x: ctx.width * cfg.credit.left_ratio,
        y: baseline,
        font: FontKind::Regular,
        size,
        color: cfg.colors.credit,
        text,
    })
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
                let chain = ctx.fonts.get(font);
                let mut x = x;
                for (face, run) in chain.runs(&text)? {
                    surface.draw_text(
                        Point::from_xy(x, y),
                        chain.krilla(face),
                        size,
                        run,
                        false,
                        TextDirection::Auto,
                    );
                    x += chain.run_width(face, run, size);
                }
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
                let (w, h) = (tree.size().width(), tree.size().height());
                let size = Size::from_wh(w, h).context("invalid SVG size")?;
                surface.push_transform(&Transform::from_row(scale, 0.0, 0.0, scale, x, y));
                // Clip to the canvas, as a browser clips an SVG image: an
                // element placed outside it is not shown.
                let canvas =
                    krilla::geom::Rect::from_xywh(0.0, 0.0, w, h).context("invalid SVG size")?;
                let mut pb = PathBuilder::new();
                pb.push_rect(canvas);
                let clip = pb.finish().context("SVG clip path")?;
                surface.push_clip_path(&clip, &FillRule::NonZero);
                surface
                    .draw_svg(&tree, size, ctx.svg_settings)
                    .context("krilla-svg failed to draw the SVG")?;
                surface.pop();
                surface.pop();
            }
            Op::Background {
                picture,
                x,
                y,
                w,
                h,
                clip,
            } => {
                let [cx, cy, cw, ch] = clip;
                let rect = krilla::geom::Rect::from_xywh(cx, cy, cw, ch)
                    .context("invalid background area")?;
                let mut pb = PathBuilder::new();
                pb.push_rect(rect);
                let path = pb.finish().context("background clip path")?;
                surface.push_clip_path(&path, &FillRule::NonZero);
                match picture {
                    Picture::Raster(image) => {
                        surface.push_transform(&Transform::from_translate(x, y));
                        surface
                            .draw_image(image, Size::from_wh(w, h).context("invalid image size")?);
                        surface.pop();
                    }
                    Picture::Svg(tree) => {
                        let (nw, nh) = (tree.size().width(), tree.size().height());
                        let scale = w / nw;
                        surface.push_transform(&Transform::from_row(scale, 0.0, 0.0, scale, x, y));
                        surface
                            .draw_svg(
                                &tree,
                                Size::from_wh(nw, nh).context("invalid SVG size")?,
                                ctx.svg_settings,
                            )
                            .context("krilla-svg failed to draw the background SVG")?;
                        surface.pop();
                    }
                }
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

    fn engine(cfg: &Config) -> Engine<'_> {
        Engine::new(cfg)
            .expect("the default fonts must be installed (see scripts/ci-install-tools.repo.sh)")
    }

    fn with_ctx<R>(cfg: &Config, f: impl FnOnce(&Ctx) -> R) -> R {
        let engine = engine(cfg);
        let fonts = engine.bytes.load().unwrap();
        let ctx = engine.ctx(engine.cfg, None, &fonts);
        f(&ctx)
    }

    /// Render `presentation` with a fresh engine over `cfg`.
    fn render(
        presentation: &Presentation,
        cfg: &Config,
        overflow: Overflow,
    ) -> Result<(Vec<u8>, Vec<String>)> {
        engine(cfg).render(presentation, overflow)
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

    const QUOTE_SLIDE: &str = "title: Quotes and columns
title_level: 2
body:
  - quote:
      - text: A quote with a https://example.org link
      - bullets: [quoted item]
  - columns:
      - body:
          - heading: {text: Left, level: 3}
          - code: {language: bash, source: echo left}
      - body:
          - text: right side";

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
        for item in [FULL_SLIDE, SVG_SLIDE, QUOTE_SLIDE] {
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
            let s = code_width_scale(ctx, &[("fn main() {}\n", ctx.content_width())]);
            assert_eq!(s, 1.0, "code is never enlarged");
        });
    }

    #[test]
    fn code_wider_than_the_slide_shrinks_to_fit_its_widest_line() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let line = "x".repeat(400);
            let width = ctx.content_width();
            let s = code_width_scale(ctx, &[("short", width), (&line, width)]);
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
                Op::Background { .. } => f32::MIN,
            })
            .fold(f32::MIN, f32::max)
    }

    #[test]
    fn code_keeps_its_size_when_the_content_only_runs_into_the_bottom_padding() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let full = ctx.em * cfg.code.font_size_em;
            let padded_bottom = ctx.height - ctx.pad_y();
            // Add bullets until the content reaches into the bottom padding.
            for n in 1..60 {
                let s = one_slide(&format!("{}\ncode: {{source: x = 1}}", bullets_yaml(n)));
                let Ok(ops) = layout_slide(ctx, &s, None) else {
                    panic!("{n} bullets and a line of code fit the page, or else stop earlier");
                };
                if ink_bottom(ctx, &ops) > padded_bottom {
                    assert!(
                        mono_sizes(&ops)
                            .iter()
                            .all(|&size| (size - full).abs() < 1e-3)
                    );
                    return;
                }
            }
            panic!("the content never reached the bottom padding");
        });
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

    /// A teaching-slides style diagram: a 1280x720 canvas whose drawing
    /// spans x 32..1247 and y 36..`bottom`.
    fn diagram(bottom: u32) -> String {
        format!(
            "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 1280 720'>\
             <defs><filter id='s'><feDropShadow dx='2' dy='2' stdDeviation='3'/></filter></defs>\
             <rect x='32' y='36' width='1215' height='{}' filter='url(#s)'/></svg>",
            bottom - 36
        )
    }

    fn svg_op_of(ops: &[Op]) -> (f32, f32, f32, f32) {
        ops.iter()
            .find_map(|op| match op {
                Op::Svg { tree, x, y, scale } => {
                    Some((*x, *y, *scale, tree.size().width() * scale))
                }
                _ => None,
            })
            .expect("an SVG op")
    }

    #[test]
    fn picture_is_placed_as_marp_places_it() {
        // Measured from marp's PDF of a teaching-slides slide: `## Title`
        // over a 1280x720 diagram draws its canvas at x 58.9 pt, 842.8 pt
        // wide, from y 114.3 pt, on the 960 x 540 pt page.
        let cfg = Config::default();
        assert_eq!(cfg.svg.size, SvgSize::Width);
        with_ctx(&cfg, |ctx| {
            let s = one_slide(&format!(
                "title: AI in CI/CD\ntitle_level: 2\nbody:\n  - svg: \"{}\"",
                diagram(620)
            ));
            let ops = layout_slide(ctx, &s, None).unwrap();
            let (x, y, _, w) = svg_op_of(&ops);
            assert!((x - 58.9).abs() < 1.0, "x {x}");
            assert!((w - 842.8).abs() < 1.0, "width {w}");
            assert!((y - 114.3).abs() < 1.5, "top {y}");
            // The canvas runs past the page, the drawing does not.
            assert!(y + w * 720.0 / 1280.0 > ctx.height);
            assert!(ink_bottom(ctx, &ops) < ctx.height);
        });
    }

    #[test]
    fn a_small_picture_keeps_its_size_and_is_centred_with_the_text() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let svg = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 400 200'><rect width='400' height='200'/></svg>";
            let s = one_slide(&format!("title: T\nbody:\n  - svg: \"{svg}\""));
            let ops = layout_slide(ctx, &s, None).unwrap();
            let (_, y, _, w) = svg_op_of(&ops);
            let natural = 400.0 * ctx.width / cfg.svg.slide_width_px;
            assert!((w - natural).abs() < 1e-3, "{w} vs {natural}");
            let title = first_baseline(&ops);
            assert!(
                title > ctx.pad_y() + ctx.em,
                "the slide is vertically centred"
            );
            assert!(y > title);
        });
    }

    #[test]
    fn a_drawing_that_runs_off_the_page_is_an_error() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let s = one_slide(&format!(
                "title: T\ntitle_level: 2\nbody:\n  - svg: \"{}\"",
                diagram(715)
            ));
            let err = layout_slide(ctx, &s, None).err().expect("must fail");
            assert!(err.to_string().contains("does not fit"), "{err}");
        });
    }

    #[test]
    fn quote_has_a_bar_and_muted_inset_text() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let s = one_slide(
                "body:\n  - quote:\n      - text: plain and https://example.org\n      - bullets: [item]",
            );
            let ops = layout_slide(ctx, &s, None).unwrap();
            let border = cfg.quote.border_em * ctx.em;
            let bar = ops
                .iter()
                .find_map(|op| match op {
                    Op::Rect {
                        x, y, w, h, color, ..
                    } if *color == cfg.colors.quote_border => Some((*x, *y, *w, *h)),
                    _ => None,
                })
                .expect("the quote bar");
            assert!((bar.0 - ctx.pad_x()).abs() < 1e-3 && (bar.2 - border).abs() < 1e-3);
            let text_x = ctx.pad_x() + border + cfg.quote.padding_em * ctx.em;
            for op in &ops {
                match op {
                    Op::Text {
                        text, x, y, color, ..
                    } if text == "plain" => {
                        assert!((x - text_x).abs() < 1e-3, "inset by bar and padding");
                        assert!(
                            *y > bar.1 && *y < bar.1 + bar.3,
                            "the bar spans the content"
                        );
                        assert_eq!(*color, cfg.colors.quote_text);
                    }
                    Op::Text { text, color, .. } if text.starts_with("https") => {
                        assert_eq!(*color, cfg.colors.link, "links keep their color");
                    }
                    Op::Rect { radius, color, .. } if *radius > 0.0 => {
                        assert_eq!(
                            *color, cfg.colors.quote_text,
                            "the bullet disc is muted too"
                        );
                    }
                    _ => {}
                }
            }
        });
    }

    #[test]
    fn a_column_keeps_its_first_and_last_margins_inside() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let s = one_slide(
                "body:\n  - columns:\n      - body:\n          - heading: {text: H, level: 3}\n          - bullets: [b]",
            );
            let Element::Columns(cols) = &s.body[0] else {
                panic!("columns")
            };
            let area = Area {
                x0: 0.0,
                width: ctx.content_width(),
                align: Align::Left,
            };
            let block = columns_block(ctx, cols, area, 1.0).unwrap();
            let heading = heading_block(ctx, "H", 3, 0.0, 100.0, Align::Left).unwrap();
            let empty = List {
                ordered: false,
                start: 1,
                items: Vec::new(),
            };
            let list = list_block(ctx, &empty, 0.0, 100.0, Align::Left).unwrap();
            let gap = heading.margin_bottom.max(list.margin_top);
            let lines = cfg.slide.line_height * ctx.em;
            let expected = heading.margin_top + heading.height + gap + lines + list.margin_bottom;
            assert!(
                (block.height - expected).abs() < 1e-3,
                "{} vs {expected}",
                block.height
            );
            assert_eq!((block.margin_top, block.margin_bottom), (0.0, 0.0));
        });
    }

    #[test]
    fn code_in_a_column_fits_the_column() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let long = "x".repeat(60);
            let s = one_slide(&format!(
                "body:\n  - columns:\n      - body:\n          - code: {{source: {long}}}\n      - body:\n          - text: other"
            ));
            let ops = layout_slide(ctx, &s, None).unwrap();
            let col_w = column_width(ctx, 2, ctx.content_width());
            let (size, x) = ops
                .iter()
                .find_map(|op| match op {
                    Op::Text {
                        font: FontKind::Mono,
                        size,
                        x,
                        ..
                    } => Some((*size, *x)),
                    _ => None,
                })
                .unwrap();
            let right = x + ctx.fonts.mono.width(&long, size);
            assert!(
                right <= ctx.pad_x() + col_w + 1e-2,
                "{right} beyond the column"
            );
            assert!(size < ctx.em * cfg.code.font_size_em, "it shrank");
        });
    }

    #[test]
    fn content_below_a_picture_follows_it() {
        let mut cfg = Config::default();
        cfg.svg.size = SvgSize::Fill;
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
            let Op::Text { x, text, size, .. } = page_number_op(ctx, 12, ctx.width) else {
                panic!("the page number is text")
            };
            assert_eq!(text, "12");
            let right = x + ctx.fonts.regular.width("12", size);
            let expected = ctx.width * (1.0 - cfg.pagination.right_ratio);
            assert!((right - expected).abs() < 1e-3, "{right} vs {expected}");
        });
    }

    #[test]
    fn credit_names_the_version_at_the_bottom_left() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let Op::Text { x, y, text, .. } = credit_op(ctx).unwrap() else {
                panic!("the credit is text")
            };
            assert!(text.contains(env!("CARGO_PKG_VERSION")), "{text}");
            assert!((x - ctx.width * cfg.credit.left_ratio).abs() < 1e-3);
            assert!(y > ctx.height * (1.0 - cfg.slide.padding_y_ratio) && y < ctx.height);
        });
    }

    #[test]
    fn render_produces_a_pdf_for_every_feature() {
        let yaml = format!(
            "title: Deck\npaginate: true\nslides:\n  - {}\n  - {}\n",
            FULL_SLIDE.replace('\n', "\n    "),
            SVG_SLIDE.replace('\n', "\n    ")
        );
        let (pdf, warnings) = render(&deck(&yaml), &Config::default(), Overflow::Error).unwrap();
        assert!(pdf.starts_with(b"%PDF-"), "not a PDF");
        assert!(warnings.is_empty());
    }

    #[test]
    fn render_reports_every_slide_that_does_not_fit() {
        let bad = bullets_yaml(40).replace('\n', "\n    ");
        let yaml = format!("slides:\n  - title: ok\n  - {bad}\n  - title: also ok\n  - {bad}\n");
        let err = format!(
            "{:#}",
            render(&deck(&yaml), &Config::default(), Overflow::Error).unwrap_err()
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
    fn with_overflow_warnings_every_slide_is_drawn_and_reported() {
        let bad = bullets_yaml(40).replace('\n', "\n    ");
        // A slide whose code has no room left below its text.
        let crowded =
            format!("{}\ncode: {{source: x = 1}}", bullets_yaml(40)).replace('\n', "\n    ");
        let yaml = format!("slides:\n  - title: ok\n  - {bad}\n  - {crowded}\n");
        let (pdf, warnings) = render(&deck(&yaml), &Config::default(), Overflow::Warn).unwrap();
        assert!(pdf.starts_with(b"%PDF-"), "not a PDF");
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(
            warnings[0].starts_with("slide 2/3: content does not fit"),
            "{warnings:?}"
        );
        assert!(
            warnings[1].starts_with("slide 3/3: content does not fit"),
            "{warnings:?}"
        );
        // Other errors stay errors.
        let missing = "slides:\n  - {title: T, image: /nonexistent.svg}\n";
        assert!(render(&deck(missing), &Config::default(), Overflow::Warn).is_err());
    }

    #[test]
    fn a_word_too_wide_for_a_line_breaks_as_in_marp() {
        let cfg = Config::default();
        let word = "x".repeat(200);
        with_ctx(&cfg, |ctx| {
            for (slide, right_edge) in [
                (
                    format!("title: T\nbody:\n  - text: see {word} end"),
                    ctx.width - ctx.pad_x(),
                ),
                (
                    format!("title: T\nbullets: [{word}]"),
                    ctx.width - ctx.pad_x(),
                ),
                (format!("title: {word}"), ctx.width - ctx.pad_x()),
                // In a column: up to the column's edge, not into the next.
                (
                    format!(
                        "title: T\nbody:\n  - columns:\n      - body: [{{text: {word}}}]\n      - body: [{{text: b}}]"
                    ),
                    ctx.pad_x() + column_width(ctx, 2, ctx.content_width()),
                ),
            ] {
                let ops = layout_slide(ctx, &one_slide(&slide), None)
                    .unwrap_or_else(|e| panic!("{slide:?}: {e:#}"));
                let xs: Vec<f32> = ops
                    .iter()
                    .filter_map(|op| match op {
                        Op::Text {
                            x,
                            font,
                            size,
                            text,
                            ..
                        } if text.starts_with("xx") => {
                            Some(x + ctx.fonts.get(*font).width(text, *size))
                        }
                        _ => None,
                    })
                    .collect();
                assert!(xs.len() > 1, "{slide:?}: the word spans several lines");
                assert!(
                    xs.iter().all(|&r| r <= right_edge + 1e-2),
                    "{slide:?}: {xs:?} past {right_edge}"
                );
                let chars: usize = ops
                    .iter()
                    .filter_map(|op| match op {
                        Op::Text { text, .. } => Some(text.matches('x').count()),
                        _ => None,
                    })
                    .sum();
                assert!(chars >= 200, "{slide:?}: every character is drawn");
            }
        });
    }

    #[test]
    fn a_table_too_wide_is_an_error_even_with_overflow_warnings() {
        let word = "x".repeat(200);
        let yaml =
            format!("slides:\n  - title: T\n    table: {{headers: [A, B], rows: [[{word}, b]]}}\n");
        for overflow in [Overflow::Error, Overflow::Warn] {
            let Err(err) = render(&deck(&yaml), &Config::default(), overflow) else {
                panic!("a too-wide table must fail with {overflow:?}");
            };
            assert!(format!("{err:#}").contains("table is too wide"), "{err:#}");
        }
    }

    #[test]
    fn unknown_code_theme_is_an_error() {
        let mut cfg = Config::default();
        cfg.code.theme = "NoSuchTheme".into();
        let err = Engine::new(&cfg).err().expect("must fail");
        assert!(err.to_string().contains("NoSuchTheme"), "{err}");
    }

    /// A 2x3 red PNG.
    const PNG_2X3: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0x01, 0x03, 0x00, 0x00, 0x00, 0x83,
        0x24, 0x4c, 0xc2, 0x00, 0x00, 0x00, 0x03, 0x50, 0x4c, 0x54, 0x45, 0xff, 0x00, 0x00, 0x19,
        0xe2, 0x09, 0x37, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x08, 0xd7, 0x63, 0x60,
        0x00, 0x01, 0x00, 0x00, 0x06, 0x00, 0x01, 0x66, 0xf4, 0xc9, 0x6c, 0x00, 0x00, 0x00, 0x00,
        0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    /// `bytes` written to a fresh file named `name` in the temp directory.
    fn temp_file(name: &str, bytes: &[u8]) -> String {
        let dir = std::env::temp_dir().join(format!("rsslide-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path.to_str().unwrap().to_string()
    }

    fn background(ops: &[Op]) -> (&Picture, f32, f32, f32, f32, [f32; 4]) {
        match &ops[0] {
            Op::Background {
                picture,
                x,
                y,
                w,
                h,
                clip,
            } => (picture, *x, *y, *w, *h, *clip),
            _ => panic!("the first op is the background"),
        }
    }

    #[test]
    fn a_left_background_covers_its_half_and_the_content_takes_the_other() {
        let cfg = Config::default();
        let png = temp_file("left.png", PNG_2X3);
        with_ctx(&cfg, |ctx| {
            let s = one_slide(&format!(
                "title: T\nbackground: {{image: {png:?}, split: left}}\nbullets: [one, two]"
            ));
            let ops = layout_slide(ctx, &s, None).unwrap();
            let (picture, x, y, w, h, clip) = background(&ops);
            assert!(matches!(picture, Picture::Raster(_)));
            let half = ctx.width / 2.0;
            assert_eq!(clip, [0.0, 0.0, half, ctx.height]);
            // Cover: the 2x3 picture is as wide as the half at least, as
            // tall as the page at least, and centred on the half.
            let scale = (half / 2.0).max(ctx.height / 3.0);
            assert!((w - 2.0 * scale).abs() < 1e-3 && (h - 3.0 * scale).abs() < 1e-3);
            assert!((x + w / 2.0 - half / 2.0).abs() < 1e-3);
            assert!((y + h / 2.0 - ctx.height / 2.0).abs() < 1e-3);
            for (text, tx, _) in texts(&ops) {
                assert!(tx >= half + ctx.pad_x() - 1e-3, "{text} at {tx}");
            }
        });
    }

    #[test]
    fn a_sized_right_background_is_a_share_of_its_side_width() {
        let cfg = Config::default();
        let svg = temp_file(
            "right.svg",
            b"<svg xmlns='http://www.w3.org/2000/svg' width='100' height='50'><rect width='100' height='50'/></svg>",
        );
        with_ctx(&cfg, |ctx| {
            let s = one_slide(&format!(
                "title: T\nbackground: {{image: {svg:?}, split: right, split_size: 40%, size: 80%}}"
            ));
            let ops = layout_slide(ctx, &s, None).unwrap();
            let (picture, x, _, w, h, clip) = background(&ops);
            assert!(matches!(picture, Picture::Svg(_)));
            let side = ctx.width * 0.4;
            assert!((clip[0] - (ctx.width - side)).abs() < 1e-3);
            assert!((w - side * 0.8).abs() < 1e-3 && (h - w / 2.0).abs() < 1e-3);
            assert!((x - (ctx.width - side + side * 0.1)).abs() < 1e-3);
            let (_, tx, _) = texts(&ops)[0];
            assert!((tx - ctx.pad_x()).abs() < 1e-3);
        });
    }

    #[test]
    fn a_background_that_is_not_a_picture_is_an_error() {
        let cfg = Config::default();
        let gif = temp_file("x.gif", b"GIF89a");
        with_ctx(&cfg, |ctx| {
            for image in [gif.as_str(), "/nonexistent/x.png"] {
                let s = one_slide(&format!(
                    "title: T\nbackground: {{image: {image:?}, split: left}}"
                ));
                assert!(layout_slide(ctx, &s, None).is_err(), "{image}");
            }
        });
    }

    #[test]
    fn a_slide_with_a_character_no_font_has_fails() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let s = one_slide("title: T\nbody:\n  - text: \"a \\U0010FFFD b\"");
            let Err(err) = layout_slide(ctx, &s, None) else {
                panic!("a character no font has must fail the slide");
            };
            assert!(format!("{err:#}").contains("U+10FFFD"), "{err:#}");
            let ok = one_slide("title: T → U\nbody:\n  - text: \"&theta; ≤ x\"");
            assert!(layout_slide(ctx, &ok, None).is_ok());
        });
    }

    #[test]
    fn a_numbered_list_counts_from_its_start() {
        let cfg = Config::default();
        with_ctx(&cfg, |ctx| {
            let s = one_slide("title: T\nbody:\n  - numbered: [six, seven]\n    start: 6");
            let ops = layout_slide(ctx, &s, None).unwrap();
            let labels: Vec<&str> = texts(&ops)
                .into_iter()
                .map(|(t, _, _)| t)
                .filter(|t| t.ends_with('.'))
                .collect();
            assert_eq!(labels, ["6.", "7."]);
        });
        assert!(
            crate::parser::parse("slides:\n  - body:\n      - bullets: [a]\n        start: 2\n")
                .is_err()
        );
    }

    #[test]
    fn an_svg_element_outside_its_canvas_is_not_ink() {
        let cfg = Config::default();
        // A label rotated off the bottom of its canvas, as a browser hides it.
        let svg = temp_file(
            "offcanvas.svg",
            b"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 1280 720'>\
              <rect x='40' y='40' width='1200' height='560' fill='#88a'/>\
              <text x='1240' y='342' font-size='20' transform='rotate(90,590,180)'>off canvas</text></svg>",
        );
        with_ctx(&cfg, |ctx| {
            let s = one_slide(&format!("title: T\nbody:\n  - image: {svg:?}"));
            let ops = layout_slide(ctx, &s, None).unwrap();
            assert!(ink_bottom(ctx, &ops) <= ctx.height);
        });
    }
}
