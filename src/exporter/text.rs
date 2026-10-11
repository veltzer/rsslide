//! Rich text for the PDF exporter: inline markdown parsing, font metrics
//! and line wrapping.
//!
//! Text is measured with rustybuzz, the shaper krilla itself draws with, so
//! measured widths match the drawn glyphs exactly.

use anyhow::{Context, Result};
use krilla::text::Font;
use rustybuzz::{Face, UnicodeBuffer};

/// Which face of the configured font family a run is set in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontKind {
    Regular,
    Bold,
    Italic,
    BoldItalic,
    Mono,
}

/// One loaded font: the krilla handle used for drawing plus the rustybuzz
/// face used for measuring, both over the same bytes.
pub struct LoadedFont<'a> {
    pub krilla: Font,
    face: Face<'a>,
    /// Ascender and descender as fractions of the em (descender negative).
    pub ascent: f32,
    pub descent: f32,
}

impl<'a> LoadedFont<'a> {
    pub fn new(bytes: &'a [u8], what: &str) -> Result<Self> {
        let face = Face::from_slice(bytes, 0).with_context(|| format!("parsing {what} font"))?;
        let krilla = Font::new(bytes.to_vec().into(), 0)
            .with_context(|| format!("loading {what} font into krilla"))?;
        let upem = face.units_per_em() as f32;
        Ok(Self {
            krilla,
            ascent: face.ascender() as f32 / upem,
            descent: face.descender() as f32 / upem,
            face,
        })
    }

    /// Advance width of `text` set at `size`, in the same unit as `size`.
    pub fn width(&self, text: &str, size: f32) -> f32 {
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        let shaped = rustybuzz::shape(&self.face, &[], buffer);
        let units: i32 = shaped.glyph_positions().iter().map(|p| p.x_advance).sum();
        units as f32 / self.face.units_per_em() as f32 * size
    }
}

/// The raw bytes of the configured fonts; `FontSet` borrows from them.
pub struct FontBytes {
    regular: Vec<u8>,
    bold: Vec<u8>,
    italic: Vec<u8>,
    bold_italic: Vec<u8>,
    mono: Vec<u8>,
}

impl FontBytes {
    pub fn read(fonts: &crate::config::Fonts) -> Result<Self> {
        let read = |path: &std::path::Path, what: &str| {
            std::fs::read(path).with_context(|| format!("reading {what} font {}", path.display()))
        };
        Ok(Self {
            regular: read(&fonts.regular, "regular")?,
            bold: read(&fonts.bold, "bold")?,
            italic: read(&fonts.italic, "italic")?,
            bold_italic: read(&fonts.bold_italic, "bold_italic")?,
            mono: read(&fonts.mono, "mono")?,
        })
    }

    pub fn load(&self) -> Result<FontSet<'_>> {
        Ok(FontSet {
            regular: LoadedFont::new(&self.regular, "regular")?,
            bold: LoadedFont::new(&self.bold, "bold")?,
            italic: LoadedFont::new(&self.italic, "italic")?,
            bold_italic: LoadedFont::new(&self.bold_italic, "bold_italic")?,
            mono: LoadedFont::new(&self.mono, "mono")?,
        })
    }

    pub fn regular(&self) -> &[u8] {
        &self.regular
    }

    pub fn mono(&self) -> &[u8] {
        &self.mono
    }

    /// The faces other than regular and mono.
    pub fn others(&self) -> [&[u8]; 3] {
        [&self.bold, &self.italic, &self.bold_italic]
    }
}

pub struct FontSet<'a> {
    pub regular: LoadedFont<'a>,
    pub bold: LoadedFont<'a>,
    pub italic: LoadedFont<'a>,
    pub bold_italic: LoadedFont<'a>,
    pub mono: LoadedFont<'a>,
}

impl<'a> FontSet<'a> {
    pub fn get(&self, kind: FontKind) -> &LoadedFont<'a> {
        match kind {
            FontKind::Regular => &self.regular,
            FontKind::Bold => &self.bold,
            FontKind::Italic => &self.italic,
            FontKind::BoldItalic => &self.bold_italic,
            FontKind::Mono => &self.mono,
        }
    }
}

// ── inline markdown ───────────────────────────────────────────────────────

/// A run of text with uniform inline style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub link: bool,
}

impl Span {
    pub fn font(&self) -> FontKind {
        match (self.code, self.bold, self.italic) {
            (true, _, _) => FontKind::Mono,
            (false, true, true) => FontKind::BoldItalic,
            (false, true, false) => FontKind::Bold,
            (false, false, true) => FontKind::Italic,
            (false, false, false) => FontKind::Regular,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct InlineState {
    bold: bool,
    italic: bool,
    link: bool,
}

/// Parse the inline markdown marp renders inside a line of text: `code`,
/// **bold** / __bold__, *italic* / _italic_, [links](url), backslash
/// escapes, and bare URLs and e-mail addresses (marp autolinks them).
/// A delimiter with no matching closer is kept as literal text, as in
/// CommonMark.
pub fn parse_inline(text: &str) -> Vec<Span> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    parse_range(&chars, InlineState::default(), &mut out);
    let out = autolink(out);
    merge_adjacent(out)
}

fn push(out: &mut Vec<Span>, text: &str, st: InlineState, code: bool) {
    if text.is_empty() {
        return;
    }
    out.push(Span {
        text: text.to_string(),
        bold: st.bold,
        italic: st.italic,
        code,
        link: st.link,
    });
}

fn parse_range(chars: &[char], st: InlineState, out: &mut Vec<Span>) {
    let mut buf = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() && chars[i + 1].is_ascii_punctuation() {
            buf.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if c == '`' {
            let run = count_run(chars, i, '`');
            if let Some(close) = find_backtick_closer(chars, i + run, run) {
                push(out, &buf, st, false);
                buf.clear();
                let inner: String = chars[i + run..close].iter().collect();
                // CommonMark strips one space of padding from each side.
                let inner = if inner.len() >= 2
                    && inner.starts_with(' ')
                    && inner.ends_with(' ')
                    && !inner.trim().is_empty()
                {
                    inner[1..inner.len() - 1].to_string()
                } else {
                    inner
                };
                push(out, &inner, st, true);
                i = close + run;
                continue;
            }
            buf.extend(&chars[i..i + run]);
            i += run;
            continue;
        }
        if c == '['
            && let Some((text_end, url_end)) = find_link(chars, i)
        {
            push(out, &buf, st, false);
            buf.clear();
            let inner = InlineState { link: true, ..st };
            parse_range(&chars[i + 1..text_end], inner, out);
            i = url_end + 1;
            continue;
        }
        if c == '*' || c == '_' {
            let run = count_run(chars, i, c);
            let width = if run >= 2 { 2 } else { 1 };
            if can_open(chars, i, width, c)
                && let Some(close) = find_emphasis_closer(chars, i + width, c, width)
            {
                push(out, &buf, st, false);
                buf.clear();
                let inner = if width == 2 {
                    InlineState { bold: true, ..st }
                } else {
                    InlineState { italic: true, ..st }
                };
                parse_range(&chars[i + width..close], inner, out);
                i = close + width;
                continue;
            }
            buf.extend(&chars[i..i + run]);
            i += run;
            continue;
        }
        buf.push(c);
        i += 1;
    }
    push(out, &buf, st, false);
}

fn count_run(chars: &[char], start: usize, c: char) -> usize {
    chars[start..].iter().take_while(|&&x| x == c).count()
}

fn find_backtick_closer(chars: &[char], from: usize, run: usize) -> Option<usize> {
    let mut j = from;
    while j < chars.len() {
        if chars[j] == '`' {
            let r = count_run(chars, j, '`');
            if r == run {
                return Some(j);
            }
            j += r;
        } else {
            j += 1;
        }
    }
    None
}

/// `[text](url)` starting at `start`: returns the index of `]` and of `)`.
fn find_link(chars: &[char], start: usize) -> Option<(usize, usize)> {
    let mut depth = 0;
    let mut j = start;
    let text_end = loop {
        match chars.get(j)? {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    break j;
                }
            }
            _ => {}
        }
        j += 1;
    };
    if chars.get(text_end + 1) != Some(&'(') {
        return None;
    }
    let url_end = (text_end + 2..chars.len()).find(|&k| chars[k] == ')')?;
    Some((text_end, url_end))
}

/// A delimiter opens when followed by non-whitespace; an underscore must
/// also not sit inside a word (`snake_case` stays literal).
fn can_open(chars: &[char], i: usize, width: usize, c: char) -> bool {
    let next = chars.get(i + width);
    if next.is_none_or(|n| n.is_whitespace()) {
        return false;
    }
    !(c == '_' && i > 0 && chars[i - 1].is_alphanumeric())
}

fn find_emphasis_closer(chars: &[char], from: usize, c: char, width: usize) -> Option<usize> {
    let mut j = from;
    while j < chars.len() {
        if chars[j] == '`' {
            let run = count_run(chars, j, '`');
            match find_backtick_closer(chars, j + run, run) {
                Some(close) => j = close + run,
                None => j += run,
            }
            continue;
        }
        if chars[j] == '\\' {
            j += 2;
            continue;
        }
        if chars[j] == c {
            let run = count_run(chars, j, c);
            let prev_ok = j > from && !chars[j - 1].is_whitespace();
            let next_ok = c != '_' || chars.get(j + run).is_none_or(|n| !n.is_alphanumeric());
            if prev_ok && next_ok && (run == width || (run >= 3 && width == 2)) {
                return Some(j);
            }
            j += run;
            continue;
        }
        j += 1;
    }
    None
}

fn is_autolink(word: &str) -> bool {
    if word.starts_with("http://") || word.starts_with("https://") {
        return word.len() > "https://".len();
    }
    let Some((user, domain)) = word.split_once('@') else {
        return false;
    };
    !user.is_empty()
        && user
            .chars()
            .all(|c| c.is_alphanumeric() || "._%+-".contains(c))
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && domain
            .chars()
            .all(|c| c.is_alphanumeric() || ".-".contains(c))
}

/// Split plain spans so that bare URLs and e-mail addresses become links.
fn autolink(spans: Vec<Span>) -> Vec<Span> {
    let mut out = Vec::new();
    for span in spans {
        if span.code || span.link {
            out.push(span);
            continue;
        }
        let mut rest = span.text.as_str();
        let mut plain = String::new();
        while !rest.is_empty() {
            let ws = rest
                .find(|c: char| !c.is_whitespace())
                .unwrap_or(rest.len());
            plain.push_str(&rest[..ws]);
            rest = &rest[ws..];
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let word = &rest[..end];
            let core = word.trim_end_matches(['.', ',', ';', ':', ')', '!', '?']);
            if is_autolink(core) {
                if !plain.is_empty() {
                    out.push(Span {
                        text: std::mem::take(&mut plain),
                        ..span.clone()
                    });
                }
                out.push(Span {
                    text: core.to_string(),
                    link: true,
                    ..span.clone()
                });
                plain.push_str(&word[core.len()..]);
            } else {
                plain.push_str(word);
            }
            rest = &rest[end..];
        }
        if !plain.is_empty() {
            out.push(Span {
                text: plain,
                ..span
            });
        }
    }
    out
}

fn merge_adjacent(spans: Vec<Span>) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::with_capacity(spans.len());
    for s in spans {
        if let Some(last) = out.last_mut()
            && !s.code
            && !last.code
            && (last.bold, last.italic, last.link) == (s.bold, s.italic, s.link)
        {
            last.text.push_str(&s.text);
            continue;
        }
        out.push(s);
    }
    out
}

// ── line breaking ─────────────────────────────────────────────────────────

/// A measured fragment of one span, placed on a line.
#[derive(Debug, Clone)]
pub struct Piece {
    pub span: Span,
    /// Offset from the start of the line.
    pub x: f32,
    /// Advance width, including any code padding.
    pub width: f32,
    /// Code padding before the text: set on the first fragment of a `code`
    /// span (and the right padding is in the width of its last one).
    pub pad_left: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Line {
    pub pieces: Vec<Piece>,
    pub width: f32,
}

/// Sizes needed to measure text of one block.
#[derive(Clone, Copy)]
pub struct TextMetrics {
    /// Font size of ordinary text in the block.
    pub size: f32,
    /// Font size of `code` spans.
    pub code_size: f32,
    /// Horizontal padding on each side of a `code` span.
    pub code_pad: f32,
}

/// A measured fragment before it is placed on a line.
#[derive(Clone)]
struct Frag {
    span: Span,
    width: f32,
    pad_left: f32,
}

/// An unbreakable unit: consecutive non-space fragments, possibly of
/// several styles (`**bold**:` is one atom). `space` is the whitespace
/// that follows it, or `None` at the end of the text.
struct Atom {
    parts: Vec<Frag>,
    width: f32,
    space: Option<Frag>,
}

fn measure(span: Span, fonts: &FontSet, m: TextMetrics, pad_left: f32, pad_right: f32) -> Frag {
    let width = if span.code {
        fonts.mono.width(&span.text, m.code_size)
    } else {
        fonts.get(span.font()).width(&span.text, m.size)
    };
    Frag {
        span,
        width: width + pad_left + pad_right,
        pad_left,
    }
}

/// Split spans into atoms. Lines may break at any whitespace, including
/// whitespace inside `code` (marp's `white-space: break-spaces`); the code
/// padding goes on the outer edges of the span only.
fn atoms(spans: &[Span], fonts: &FontSet, m: TextMetrics) -> Vec<Atom> {
    let mut out: Vec<Atom> = Vec::new();
    let mut current: Vec<Frag> = Vec::new();
    let mut flush = |current: &mut Vec<Frag>, space: Option<Frag>| {
        if current.is_empty() {
            // Whitespace with nothing before it on the line is dropped,
            // like CSS collapsible whitespace at a line start.
            return;
        }
        let width = current.iter().map(|f| f.width).sum();
        out.push(Atom {
            parts: std::mem::take(current),
            width,
            space,
        });
    };
    for span in spans {
        let pad = if span.code { m.code_pad } else { 0.0 };
        let mut rest = span.text.as_str();
        let mut first = true;
        while !rest.is_empty() {
            let is_space = rest.starts_with(char::is_whitespace);
            let end = if is_space {
                rest.find(|c: char| !c.is_whitespace())
            } else {
                rest.find(char::is_whitespace)
            }
            .unwrap_or(rest.len());
            let last = end == rest.len();
            let left = if first { pad } else { 0.0 };
            let right = if last { pad } else { 0.0 };
            if is_space {
                let space = Span {
                    text: " ".into(),
                    ..span.clone()
                };
                flush(&mut current, Some(measure(space, fonts, m, left, right)));
            } else if span.code {
                let word = Span {
                    text: rest[..end].to_string(),
                    ..span.clone()
                };
                current.push(measure(word, fonts, m, left, right));
            } else {
                // A word may break after a hyphen inside it; each part but
                // the last ends an atom with a zero-width break.
                let parts = hyphen_parts(&rest[..end]);
                let n = parts.len();
                for (k, part) in parts.into_iter().enumerate() {
                    let word = Span {
                        text: part.to_string(),
                        ..span.clone()
                    };
                    current.push(measure(word, fonts, m, 0.0, 0.0));
                    if k + 1 < n {
                        let soft = Span {
                            text: String::new(),
                            ..span.clone()
                        };
                        flush(&mut current, Some(measure(soft, fonts, m, 0.0, 0.0)));
                    }
                }
            }
            first = false;
            rest = &rest[end..];
        }
    }
    flush(&mut current, None);
    out
}

/// Split a word after each hyphen a line may break at: one between a letter
/// or digit and a letter (`AI-generated`, `x86-based`), as browsers break
/// per Unicode line breaking. A hyphen at the start (`-5`, `--flag`) or
/// before a digit is not a break.
fn hyphen_parts(word: &str) -> Vec<&str> {
    let chars: Vec<(usize, char)> = word.char_indices().collect();
    let mut parts = Vec::new();
    let mut start = 0;
    for w in chars.windows(3) {
        let [(_, before), (i, '-'), (_, after)] = w else {
            continue;
        };
        if before.is_alphanumeric() && after.is_alphabetic() {
            parts.push(&word[start..=*i]);
            start = i + 1;
        }
    }
    parts.push(&word[start..]);
    parts
}

/// Greedily break `spans` into lines no wider than `max_width`. A single
/// atom wider than the line is placed on its own line and overflows, as
/// in a browser.
pub fn wrap(spans: &[Span], max_width: f32, fonts: &FontSet, m: TextMetrics) -> Vec<Line> {
    // Widths are sums of floats added in varying orders, so text measured
    // as exactly `max_width` elsewhere (a table column sized to its
    // content) may come out a rounding error wider here. It still fits.
    let limit = max_width * (1.0 + 1e-5);
    let mut lines = Vec::new();
    let mut line = Line::default();
    let mut pending_space: Option<Frag> = None;
    let push = |line: &mut Line, f: Frag| {
        line.pieces.push(Piece {
            span: f.span,
            x: line.width,
            width: f.width,
            pad_left: f.pad_left,
        });
        line.width += f.width;
    };
    for atom in atoms(spans, fonts, m) {
        let space_w = pending_space.as_ref().map_or(0.0, |f| f.width);
        if !line.pieces.is_empty() && line.width + space_w + atom.width > limit {
            lines.push(std::mem::take(&mut line));
            pending_space = None;
        }
        if !line.pieces.is_empty()
            && let Some(space) = pending_space.take()
        {
            push(&mut line, space);
        }
        for frag in atom.parts {
            push(&mut line, frag);
        }
        pending_space = atom.space;
    }
    if !line.pieces.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Span {
        fn plain(text: &str) -> Self {
            Span {
                text: text.to_string(),
                bold: false,
                italic: false,
                code: false,
                link: false,
            }
        }
    }

    fn texts(spans: &[Span]) -> Vec<(&str, bool, bool, bool, bool)> {
        spans
            .iter()
            .map(|s| (s.text.as_str(), s.bold, s.italic, s.code, s.link))
            .collect()
    }

    #[test]
    fn plain_text_is_one_span() {
        assert_eq!(
            parse_inline("hello world"),
            vec![Span::plain("hello world")]
        );
    }

    #[test]
    fn bold_and_code() {
        let s = parse_inline("**Slow Start**: use `ss -tlnp` now");
        assert_eq!(
            texts(&s),
            vec![
                ("Slow Start", true, false, false, false),
                (": use ", false, false, false, false),
                ("ss -tlnp", false, false, true, false),
                (" now", false, false, false, false),
            ]
        );
    }

    #[test]
    fn italic_star_and_underscore() {
        let s = parse_inline("an *a* and _b_");
        assert_eq!(
            texts(&s),
            vec![
                ("an ", false, false, false, false),
                ("a", false, true, false, false),
                (" and ", false, false, false, false),
                ("b", false, true, false, false),
            ]
        );
    }

    #[test]
    fn intraword_underscore_is_literal() {
        assert_eq!(
            parse_inline("TIME_WAIT and snake_case_name"),
            vec![Span::plain("TIME_WAIT and snake_case_name")]
        );
    }

    #[test]
    fn unmatched_delimiters_stay_literal() {
        assert_eq!(parse_inline("2 * 3 = 6"), vec![Span::plain("2 * 3 = 6")]);
        assert_eq!(parse_inline("a ` b"), vec![Span::plain("a ` b")]);
        assert_eq!(parse_inline("**open"), vec![Span::plain("**open")]);
    }

    #[test]
    fn escapes_are_literal() {
        assert_eq!(parse_inline(r"\*not\*"), vec![Span::plain("*not*")]);
    }

    #[test]
    fn markdown_link_shows_text_only() {
        let s = parse_inline("[me@x.org](mailto:me@x.org)");
        assert_eq!(texts(&s), vec![("me@x.org", false, false, false, true)]);
    }

    #[test]
    fn bare_email_and_url_are_links() {
        let s = parse_inline("mail me@x.org, or see https://x.org.");
        assert_eq!(
            texts(&s),
            vec![
                ("mail ", false, false, false, false),
                ("me@x.org", false, false, false, true),
                (", or see ", false, false, false, false),
                ("https://x.org", false, false, false, true),
                (".", false, false, false, false),
            ]
        );
    }

    #[test]
    fn code_keeps_inner_markup() {
        let s = parse_inline("`**x**`");
        assert_eq!(texts(&s), vec![("**x**", false, false, true, false)]);
    }

    // ── wrapping, measured with the configured fonts ──────────────────────

    fn font_bytes() -> FontBytes {
        FontBytes::read(&crate::config::Config::default().fonts)
            .expect("the default fonts must be installed (see scripts/ci-install-tools.repo.sh)")
    }

    const M: TextMetrics = TextMetrics {
        size: 20.0,
        code_size: 17.0,
        code_pad: 6.8,
    };

    const LOREM: &str = "The quick brown fox jumps over the lazy dog while \
        `cargo build --release` runs and **bold words** and *italic ones* wrap";

    fn line_text(line: &Line) -> String {
        line.pieces.iter().map(|p| p.span.text.as_str()).collect()
    }

    #[test]
    fn width_scales_linearly_with_size() {
        let bytes = font_bytes();
        let fonts = bytes.load().unwrap();
        let w10 = fonts.regular.width("Hello, world", 10.0);
        let w30 = fonts.regular.width("Hello, world", 30.0);
        assert!(w10 > 0.0);
        assert!((w30 - 3.0 * w10).abs() < 1e-3, "{w10} {w30}");
    }

    #[test]
    fn wrapped_lines_fit_the_width() {
        let bytes = font_bytes();
        let fonts = bytes.load().unwrap();
        let spans = parse_inline(LOREM);
        for max in [150.0, 220.0, 400.0, 900.0] {
            let lines = wrap(&spans, max, &fonts, M);
            assert!(!lines.is_empty());
            for line in &lines {
                assert!(
                    line.width <= max + 1e-3,
                    "{max}: {:?} is {}",
                    line_text(line),
                    line.width
                );
            }
        }
    }

    #[test]
    fn pieces_are_contiguous_and_sum_to_the_line_width() {
        let bytes = font_bytes();
        let fonts = bytes.load().unwrap();
        for line in wrap(&parse_inline(LOREM), 250.0, &fonts, M) {
            let mut x = 0.0;
            for p in &line.pieces {
                assert!(
                    (p.x - x).abs() < 1e-3,
                    "{:?} at {} expected {x}",
                    p.span.text,
                    p.x
                );
                x += p.width;
            }
            assert!((line.width - x).abs() < 1e-3);
        }
    }

    #[test]
    fn wrapping_keeps_every_word_and_trims_line_edges() {
        let bytes = font_bytes();
        let fonts = bytes.load().unwrap();
        let spans = parse_inline(LOREM);
        let lines = wrap(&spans, 200.0, &fonts, M);
        assert!(lines.len() > 2, "expected several lines");
        for line in &lines {
            let text = line_text(line);
            assert_eq!(text.trim(), text, "line has a space at an edge: {text:?}");
        }
        let joined: Vec<String> = lines.iter().map(line_text).collect();
        let original: String = spans.iter().map(|s| s.text.as_str()).collect();
        let words = |s: &str| s.split_whitespace().map(str::to_string).collect::<Vec<_>>();
        assert_eq!(words(&joined.join(" ")), words(&original));
    }

    #[test]
    fn a_word_longer_than_the_line_gets_a_line_of_its_own() {
        let bytes = font_bytes();
        let fonts = bytes.load().unwrap();
        let spans = parse_inline("a Pneumonoultramicroscopicsilicovolcanoconiosis b");
        let lines = wrap(&spans, 60.0, &fonts, M);
        let texts: Vec<String> = lines.iter().map(line_text).collect();
        assert_eq!(
            texts,
            ["a", "Pneumonoultramicroscopicsilicovolcanoconiosis", "b"]
        );
        assert!(
            lines[1].width > 60.0,
            "the long word overflows rather than splits"
        );
    }

    #[test]
    fn code_span_breaks_at_spaces_with_padding_on_its_outer_edges() {
        let bytes = font_bytes();
        let fonts = bytes.load().unwrap();
        let spans = parse_inline("`alpha beta gamma`");
        // Unwrapped: one line, mono text plus padding on both sides only.
        let one = wrap(&spans, f32::INFINITY, &fonts, M);
        assert_eq!(one.len(), 1);
        let expected = fonts.mono.width("alpha beta gamma", M.code_size) + 2.0 * M.code_pad;
        assert!(
            (one[0].width - expected).abs() < 1e-3,
            "{} vs {expected}",
            one[0].width
        );

        // Narrow: one word per line; left padding only on the first piece,
        // right padding only in the last piece's width.
        let lines = wrap(&spans, 1.0, &fonts, M);
        let texts: Vec<String> = lines.iter().map(line_text).collect();
        assert_eq!(texts, ["alpha", "beta", "gamma"]);
        let mono = |t: &str| fonts.mono.width(t, M.code_size);
        let first = &lines[0].pieces[0];
        assert_eq!(first.pad_left, M.code_pad);
        assert!((first.width - (mono("alpha") + M.code_pad)).abs() < 1e-3);
        let middle = &lines[1].pieces[0];
        assert_eq!(middle.pad_left, 0.0);
        assert!((middle.width - mono("beta")).abs() < 1e-3);
        let last = &lines[2].pieces[0];
        assert_eq!(last.pad_left, 0.0);
        assert!((last.width - (mono("gamma") + M.code_pad)).abs() < 1e-3);
    }

    #[test]
    fn a_line_may_break_after_a_hyphen_inside_a_word() {
        let bytes = font_bytes();
        let fonts = bytes.load().unwrap();
        let spans = parse_inline("review all AI-generated code");
        // One point short of "review all AI-generated": the line breaks
        // after the hyphen, and "generated code" fits on the next line.
        let width = fonts.regular.width("review all AI-generated", M.size) - 1.0;
        assert!(fonts.regular.width("generated code", M.size) < width);
        let lines = wrap(&spans, width, &fonts, M);
        let texts: Vec<String> = lines.iter().map(line_text).collect();
        assert_eq!(texts, ["review all AI-", "generated code"]);
        // Unbroken, the word is drawn whole, with nothing between its parts.
        let one = wrap(&spans, f32::INFINITY, &fonts, M);
        assert_eq!(line_text(&one[0]), "review all AI-generated code");
        let natural = fonts.regular.width("review all AI-generated code", M.size);
        assert!(
            (one[0].width - natural).abs() < 0.5,
            "{} vs {natural}",
            one[0].width
        );
    }

    #[test]
    fn hyphens_that_are_not_inside_a_word_do_not_break() {
        let bytes = font_bytes();
        let fonts = bytes.load().unwrap();
        for text in ["value -5", "use --verbose", "x - y"] {
            let lines = wrap(&parse_inline(text), 0.0, &fonts, M);
            let texts: Vec<String> = lines.iter().map(line_text).collect();
            let words: Vec<&str> = text.split(' ').filter(|w| *w != "-").collect();
            assert!(
                texts.iter().all(|t| !t.ends_with('-') || t == "-"),
                "{text}: {texts:?}"
            );
            assert!(texts.len() >= words.len(), "{text}: {texts:?}");
        }
    }

    #[test]
    fn styled_runs_use_their_own_face() {
        let bytes = font_bytes();
        let fonts = bytes.load().unwrap();
        let plain = wrap(&parse_inline("Wide"), f32::INFINITY, &fonts, M)[0].width;
        let bold = wrap(&parse_inline("**Wide**"), f32::INFINITY, &fonts, M)[0].width;
        assert!((plain - fonts.regular.width("Wide", M.size)).abs() < 1e-3);
        assert!((bold - fonts.bold.width("Wide", M.size)).abs() < 1e-3);
        assert!(bold > plain);
    }
}
