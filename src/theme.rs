//! Page palettes for a deck, from the fleet's shared-themes repository.
//!
//! A marp deck names a theme in its front matter. The three marp built-ins
//! (`default`, `gaia`, `uncover`) mean "the look rsslide reproduces", as
//! before. Any other name is looked up in `themes.json` of the directory
//! `[theme] shared_themes` points at: a page palette whose tokens become the
//! slide colours (text, links, code boxes, table borders, pagination) and
//! the page background. Diagrams follow too: an SVG whose
//! `<style id="palette-vars" data-theme="flat">` block was installed from a
//! diagram theme gets that block swapped for the `flat-<palette>` variant in
//! `diagram-themes.json`, so its neutrals are the slide's.
//!
//! Without `[theme] shared_themes`, a theme name is ignored as it always
//! was; with it, a name that is neither a built-in nor a palette is an
//! error naming the palettes there are.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::config::{Color, Colors, Config};

/// marp's own themes: no palette, rsslide's default look.
pub const MARP_BUILTINS: &[&str] = &["default", "gaia", "uncover"];

#[derive(Debug, Deserialize)]
struct ThemesJson {
    themes: Vec<PaletteJson>,
}

#[derive(Debug, Deserialize)]
struct PaletteJson {
    code: String,
    tokens: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct DiagramThemesJson {
    variants: BTreeMap<String, DiagramVariantJson>,
}

#[derive(Debug, Deserialize)]
struct DiagramVariantJson {
    colors: BTreeMap<String, BTreeMap<String, String>>,
}

/// One page palette, resolved for rendering: the slide colours and the
/// diagram variants (by diagram theme name) that go with it.
#[derive(Debug, Clone)]
pub struct Palette {
    pub code: String,
    pub colors: Colors,
    /// Diagram theme name -> the `--role: value` declarations of its variant
    /// for this palette, in the order the style block lists them.
    diagram_vars: BTreeMap<String, Vec<(String, String)>>,
}

impl Palette {
    /// A dark page: relative luminance of the background below one half.
    /// Decides the code theme and the tint of monochrome icons.
    pub fn is_dark(&self) -> bool {
        let Color(r, g, b) = self.colors.background;
        let lum = 0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32;
        lum < 128.0
    }

    /// The declarations the `palette-vars` block of a diagram installed from
    /// `diagram_theme` should carry under this palette, if a variant exists.
    pub fn diagram_vars(&self, diagram_theme: &str) -> Option<&[(String, String)]> {
        self.diagram_vars.get(diagram_theme).map(Vec::as_slice)
    }
}

/// The shared-themes directory, loaded once per run.
#[derive(Debug)]
pub struct SharedThemes {
    dir: PathBuf,
    palettes: Vec<Palette>,
}

impl SharedThemes {
    /// Read `themes.json` and `diagram-themes.json` under `dir` and resolve
    /// every palette. Both files are JSON, which the YAML parser reads.
    pub fn load(dir: &Path) -> Result<Self> {
        let themes_path = dir.join("themes.json");
        let text = std::fs::read_to_string(&themes_path)
            .with_context(|| format!("reading {}", themes_path.display()))?;
        let themes: ThemesJson = serde_norway::from_str(&text)
            .with_context(|| format!("parsing {}", themes_path.display()))?;
        let diagram_path = dir.join("diagram-themes.json");
        let text = std::fs::read_to_string(&diagram_path)
            .with_context(|| format!("reading {}", diagram_path.display()))?;
        let diagrams: DiagramThemesJson = serde_norway::from_str(&text)
            .with_context(|| format!("parsing {}", diagram_path.display()))?;

        let mut palettes = Vec::new();
        for p in themes.themes {
            let colors = colors_from_tokens(&p.tokens)
                .with_context(|| format!("palette {:?} in {}", p.code, themes_path.display()))?;
            let suffix = format!("-{}", p.code);
            let diagram_vars = diagrams
                .variants
                .iter()
                .filter_map(|(name, v)| {
                    let theme = name.strip_suffix(&suffix)?;
                    let vars = v
                        .colors
                        .values()
                        .flat_map(|group| group.iter().map(|(k, v)| (k.clone(), v.clone())))
                        .collect();
                    Some((theme.to_string(), vars))
                })
                .collect();
            palettes.push(Palette {
                code: p.code,
                colors,
                diagram_vars,
            });
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            palettes,
        })
    }

    pub fn names(&self) -> Vec<&str> {
        self.palettes.iter().map(|p| p.code.as_str()).collect()
    }

    pub fn get(&self, code: &str) -> Option<&Palette> {
        self.palettes.iter().find(|p| p.code == code)
    }
}

/// The palette a deck's `theme:` selects: none for a marp built-in or an
/// unset theme; with shared themes configured, the named palette, or an
/// error for a name that is neither.
pub fn resolve<'a>(
    cfg: &Config,
    shared: Option<&'a SharedThemes>,
    theme: Option<&str>,
) -> Result<Option<&'a Palette>> {
    let Some(name) = theme else {
        return Ok(None);
    };
    if MARP_BUILTINS.contains(&name) {
        return Ok(None);
    }
    let Some(shared) = shared else {
        if cfg.theme.shared_themes.is_some() {
            unreachable!("shared themes are loaded whenever they are configured");
        }
        // No shared themes configured: the name is ignored, as before.
        return Ok(None);
    };
    match shared.get(name) {
        Some(p) => Ok(Some(p)),
        None => bail!(
            "unknown theme {name:?}: not a marp built-in ({}) and not a palette in {} (known: {})",
            MARP_BUILTINS.join(", "),
            shared.dir.join("themes.json").display(),
            shared.names().join(", "),
        ),
    }
}

/// The slide colours a palette's tokens give: each role of [`Colors`] from
/// the token that plays that role on the sites.
fn colors_from_tokens(tokens: &BTreeMap<String, String>) -> Result<Colors> {
    let token = |name: &str| -> Result<&str> {
        tokens
            .get(name)
            .map(String::as_str)
            .with_context(|| format!("palette lacks the token {name:?}"))
    };
    let background = parse_css_color(token("bg")?, None).context("token bg")?;
    let over = |name: &str| -> Result<Color> {
        parse_css_color(token(name)?, Some(background)).with_context(|| format!("token {name:?}"))
    };
    Ok(Colors {
        background,
        text: over("text-primary")?,
        h1: over("text-primary")?,
        h6: over("text-secondary")?,
        link: over("accent")?,
        code_background: over("bg-surface")?,
        code_border: over("border")?,
        inline_code_background: over("bg-elevated")?,
        table_border: over("border")?,
        table_stripe: over("bg-surface")?,
        pagination: over("text-muted")?,
        credit: over("text-muted")?,
        quote_text: over("text-secondary")?,
        quote_border: over("border")?,
    })
}

/// A CSS colour as the tokens write them: `#rrggbb`, `#rgb`, `rgb(r, g, b)`
/// or `rgba(r, g, b, a)`. The PDF has no alpha for text, so a translucent
/// colour is composited over `over` (the page background); a translucent
/// background itself is an error.
pub fn parse_css_color(s: &str, over: Option<Color>) -> Result<Color> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        let digits: Vec<u8> = hex
            .chars()
            .map(|c| c.to_digit(16).map(|d| d as u8))
            .collect::<Option<_>>()
            .with_context(|| format!("bad hex colour {s:?}"))?;
        return match digits.as_slice() {
            [r, g, b] => Ok(Color(r * 17, g * 17, b * 17)),
            [r1, r2, g1, g2, b1, b2] => Ok(Color(r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2)),
            _ => bail!("bad hex colour {s:?}"),
        };
    }
    let (func, inner) = s
        .split_once('(')
        .and_then(|(f, rest)| rest.strip_suffix(')').map(|inner| (f.trim(), inner)))
        .with_context(|| format!("unsupported colour {s:?}"))?;
    if func != "rgb" && func != "rgba" {
        bail!("unsupported colour {s:?}");
    }
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    let channel = |p: &str| -> Result<u8> {
        p.parse::<f32>()
            .ok()
            .filter(|v| (0.0..=255.0).contains(v))
            .map(|v| v.round() as u8)
            .with_context(|| format!("bad channel {p:?} in {s:?}"))
    };
    let (rgb, alpha) = match parts.as_slice() {
        [r, g, b] => ((channel(r)?, channel(g)?, channel(b)?), 1.0),
        [r, g, b, a] => {
            let a: f32 = a
                .parse()
                .ok()
                .filter(|v| (0.0..=1.0).contains(v))
                .with_context(|| format!("bad alpha {a:?} in {s:?}"))?;
            ((channel(r)?, channel(g)?, channel(b)?), a)
        }
        _ => bail!("unsupported colour {s:?}"),
    };
    if alpha >= 1.0 {
        return Ok(Color(rgb.0, rgb.1, rgb.2));
    }
    let Some(bg) = over else {
        bail!("translucent colour {s:?} where an opaque one is needed");
    };
    let blend = |fg: u8, bg: u8| (alpha * fg as f32 + (1.0 - alpha) * bg as f32).round() as u8;
    Ok(Color(
        blend(rgb.0, bg.0),
        blend(rgb.1, bg.1),
        blend(rgb.2, bg.2),
    ))
}

/// `svg` with its `palette-vars` style block rewritten to `palette`'s variant
/// of the diagram theme the block names in `data-theme`. Unchanged when the
/// SVG has no such block, the block names no theme, or the palette has no
/// variant for it: such a diagram keeps the colours it was installed with.
pub fn retheme_svg(svg: &str, palette: &Palette) -> String {
    let Some(open_at) = svg.find("<style id=\"palette-vars\"") else {
        return svg.to_string();
    };
    let head = &svg[open_at..];
    let Some(tag_end) = head.find('>') else {
        return svg.to_string();
    };
    let tag = &head[..tag_end];
    let Some(theme) = attr(tag, "data-theme") else {
        return svg.to_string();
    };
    let Some(vars) = palette.diagram_vars(theme) else {
        return svg.to_string();
    };
    let body_start = open_at + tag_end + 1;
    let Some(close_rel) = svg[body_start..].find("</style>") else {
        return svg.to_string();
    };
    let mut block = String::from("\n      :root, svg {\n");
    for (name, value) in vars {
        block.push_str(&format!("        --{name}: {value};\n"));
    }
    block.push_str("      }\n    ");
    let mut out = String::with_capacity(svg.len() + block.len());
    out.push_str(&svg[..body_start]);
    out.push_str(&block);
    out.push_str(&svg[body_start + close_rel..]);
    out
}

/// `#rrggbb` for a colour, as SVG attributes want it.
pub fn hex(color: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color.0, color.1, color.2)
}

/// A monochrome icon (no fill of its own, so black) given `color` as the
/// fill of its root element, which its paths inherit.
pub fn tint_icon(svg: &str, color: Color) -> String {
    match svg.find("<svg") {
        Some(at) => format!(
            "{}<svg fill=\"{}\"{}",
            &svg[..at],
            hex(color),
            &svg[at + 4..]
        ),
        None => svg.to_string(),
    }
}

/// The value of `name="..."` in an opening tag.
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let at = tag.find(&key)? + key.len();
    let rest = &tag[at..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette_with(theme: &str, vars: &[(&str, &str)]) -> Palette {
        let mut diagram_vars = BTreeMap::new();
        diagram_vars.insert(
            theme.to_string(),
            vars.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        );
        Palette {
            code: "midnight".into(),
            colors: Colors::default(),
            diagram_vars,
        }
    }

    #[test]
    fn dark_palettes_are_told_by_their_background() {
        let mut p = palette_with("flat", &[]);
        p.colors.background = Color(0x0f, 0x11, 0x17);
        assert!(p.is_dark());
        p.colors.background = Color(0xfd, 0xf6, 0xe3);
        assert!(!p.is_dark());
    }

    #[test]
    fn icons_take_the_tint_on_their_root() {
        let icon = "<svg role=\"img\" viewBox=\"0 0 24 24\"><path d=\"M0 0\"/></svg>";
        assert_eq!(
            tint_icon(icon, Color(0xe8, 0xea, 0xf0)),
            "<svg fill=\"#e8eaf0\" role=\"img\" viewBox=\"0 0 24 24\"><path d=\"M0 0\"/></svg>"
        );
        assert_eq!(hex(Color(1, 2, 255)), "#0102ff");
    }

    #[test]
    fn css_colours_parse_and_blend() {
        assert_eq!(
            parse_css_color("#4051b5", None).unwrap(),
            Color(0x40, 0x51, 0xb5)
        );
        assert_eq!(parse_css_color("#fff", None).unwrap(), Color(255, 255, 255));
        assert_eq!(
            parse_css_color("rgb(10, 20, 30)", None).unwrap(),
            Color(10, 20, 30)
        );
        // 87% black over white is #212121, what the sites render.
        assert_eq!(
            parse_css_color("rgba(0, 0, 0, 0.87)", Some(Color(255, 255, 255))).unwrap(),
            Color(33, 33, 33)
        );
        assert!(parse_css_color("rgba(0, 0, 0, 0.5)", None).is_err());
        assert!(parse_css_color("hsl(1, 2%, 3%)", None).is_err());
        assert!(parse_css_color("#12345", None).is_err());
    }

    #[test]
    fn tokens_become_slide_colours() {
        let tokens: BTreeMap<String, String> = [
            ("bg", "#0f1117"),
            ("bg-surface", "#161922"),
            ("bg-elevated", "#1e2130"),
            ("border", "#2a2e3f"),
            ("text-primary", "#e8eaf0"),
            ("text-secondary", "#8b90a0"),
            ("text-muted", "#5c6070"),
            ("accent", "#6c8cff"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let c = colors_from_tokens(&tokens).unwrap();
        assert_eq!(c.background, Color(0x0f, 0x11, 0x17));
        assert_eq!(c.text, Color(0xe8, 0xea, 0xf0));
        assert_eq!(c.link, Color(0x6c, 0x8c, 0xff));
        assert_eq!(c.code_background, Color(0x16, 0x19, 0x22));
        assert_eq!(c.pagination, Color(0x5c, 0x60, 0x70));
        let mut fewer = tokens.clone();
        fewer.remove("accent");
        let err = format!("{:#}", colors_from_tokens(&fewer).unwrap_err());
        assert!(err.contains("accent"), "{err}");
    }

    #[test]
    fn a_diagram_block_is_swapped_for_the_variant() {
        let svg = "<svg>\n  <defs>\n    <style id=\"palette-vars\" data-theme=\"flat\">\n      :root, svg {\n        --bg: #ffffff;\n        --text: #212121;\n      }\n    </style>\n  </defs>\n  <rect fill=\"var(--bg)\"/>\n</svg>";
        let palette = palette_with("flat", &[("bg", "#0f1117"), ("text", "#e8eaf0")]);
        let out = retheme_svg(svg, &palette);
        assert!(out.contains("--bg: #0f1117;"), "{out}");
        assert!(out.contains("--text: #e8eaf0;"), "{out}");
        assert!(!out.contains("#ffffff"), "{out}");
        assert!(out.contains("data-theme=\"flat\""), "{out}");
        assert!(out.ends_with("<rect fill=\"var(--bg)\"/>\n</svg>"), "{out}");
    }

    #[test]
    fn a_diagram_without_a_matching_variant_is_left_alone() {
        let palette = palette_with("flat", &[("bg", "#0f1117")]);
        for svg in [
            "<svg><rect fill=\"#fff\"/></svg>",
            "<svg><style id=\"palette-vars\">:root { --bg: #fff; }</style></svg>",
            "<svg><style id=\"palette-vars\" data-theme=\"intro\">:root { --bg: #fff; }</style></svg>",
        ] {
            assert_eq!(retheme_svg(svg, &palette), svg);
        }
    }

    #[test]
    fn builtins_and_unset_themes_mean_no_palette() {
        let cfg = Config::default();
        assert!(resolve(&cfg, None, None).unwrap().is_none());
        assert!(resolve(&cfg, None, Some("gaia")).unwrap().is_none());
        // Without shared themes configured an unknown name is ignored, as
        // it was before palettes existed.
        assert!(resolve(&cfg, None, Some("material")).unwrap().is_none());
    }

    #[test]
    fn with_shared_themes_an_unknown_name_is_an_error() {
        let shared = SharedThemes {
            dir: PathBuf::from("/shared"),
            palettes: vec![palette_with("flat", &[])],
        };
        let mut cfg = Config::default();
        cfg.theme.shared_themes = Some(PathBuf::from("/shared"));
        assert_eq!(
            resolve(&cfg, Some(&shared), Some("midnight"))
                .unwrap()
                .map(|p| p.code.as_str()),
            Some("midnight")
        );
        assert!(
            resolve(&cfg, Some(&shared), Some("default"))
                .unwrap()
                .is_none()
        );
        let err = format!(
            "{:#}",
            resolve(&cfg, Some(&shared), Some("nope")).unwrap_err()
        );
        assert!(err.contains("nope") && err.contains("midnight"), "{err}");
    }
}
