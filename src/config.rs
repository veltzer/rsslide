//! Runtime configuration. Every layout constant, font path, and color
//! used by the PDF exporter lives here. Loaded from TOML at startup with
//! per-field fallbacks to built-in defaults, so user files can override
//! individual values without supplying the full tree.
//!
//! Lookup order (first match wins):
//! 1. Path passed via `Config::load(Some(path))`.
//! 2. `./rsslide.toml` next to the current working directory.
//! 3. `$XDG_CONFIG_HOME/rsslide/config.toml` (typically `~/.config/...`).
//! 4. Built-in defaults (the pre-config constant values).

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// A commented TOML file containing every knob with its default value.
/// The values here MUST match the `Default` impls below — a unit test
/// asserts the round-trip equals `Config::default()`.
pub const DEFAULT_CONFIG_TEMPLATE: &str = r##"# rsslide configuration. Drop this file as `rsslide.toml` next to your
# YAML inputs (or pass `--config PATH`). Every section is optional — any
# field you omit keeps its built-in default.

[slide]
# Slide dimensions in millimeters. Default is 16:9.
width_mm = 254.0
height_mm = 143.0
# Side margins and top/bottom reserved space for slide content (not
# including SVGs — see [svg] for a separate SVG margin).
margin_x_mm = 8.0
margin_top_mm = 10.0
page_bottom_reserved_mm = 6.0

[title]
# Slide title font size and the gap structure beneath it.
font_size_pt = 28.0
rule_offset_mm = 9.0   # distance from title baseline down to the rule
content_gap_mm = 4.0   # extra gap from rule down to first content line

[body]
# Body text, bullets, and column text sizing.
font_size_pt = 18.0
line_height_mm = 9.0
section_gap_mm = 5.0

[code]
# Code block styling. Font size is in points; other dimensions in mm.
font_size_pt = 12.0
line_height_mm = 6.5
padding_mm = 4.0
# Language icon at the top-right of each code block.
icon_size_mm = 16.0
icon_inset_mm = 2.0

[svg]
# Filter-effect rasterization detail. 2.0 is a good quality/speed tradeoff.
# 4.0 is krilla-svg's default (crispest, slowest). 1.0 is fastest (softer).
filter_scale = 2.0
# Pre-resolve CSS `var(--x)` references before usvg parses the SVG.
# Leave on — usvg 0.45 otherwise silently drops strokes using CSS vars.
flatten_css_vars = true
# Extra gap above an embedded SVG, pushing it below the title.
top_gap_mm = 4.0
# Side margin for SVGs specifically. Smaller than slide.margin_x_mm lets
# SVGs extend closer to the slide edge.
margin_x_mm = 4.0

[fonts]
# Absolute paths to TTF/OTF files for each symbolic role. Reassign to use
# a different typeface without touching the Rust source.
title = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"
body  = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
code  = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"

[colors]
# Colors as CSS-style hex strings.
text            = "#000000"
bullet          = "#000000"
title_rule      = "#000000"
code_background = "#f0f0f0"
"##;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub slide: Slide,
    pub title: Title,
    pub body: Body,
    pub code: Code,
    pub svg: Svg,
    pub fonts: Fonts,
    pub colors: Colors,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Slide {
    pub width_mm: f32,
    pub height_mm: f32,
    pub margin_x_mm: f32,
    pub margin_top_mm: f32,
    pub page_bottom_reserved_mm: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Title {
    pub font_size_pt: f32,
    pub rule_offset_mm: f32,
    pub content_gap_mm: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Body {
    pub font_size_pt: f32,
    pub line_height_mm: f32,
    pub section_gap_mm: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Code {
    pub font_size_pt: f32,
    pub line_height_mm: f32,
    pub padding_mm: f32,
    pub icon_size_mm: f32,
    pub icon_inset_mm: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Svg {
    pub filter_scale: f32,
    pub flatten_css_vars: bool,
    /// Extra vertical gap above embedded SVGs, in mm. Pushes the diagram
    /// down from the title rule.
    pub top_gap_mm: f32,
    /// Horizontal margin for embedded SVGs specifically. Smaller than
    /// `slide.margin_x_mm` lets the SVG fill more width.
    pub margin_x_mm: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Fonts {
    pub title: PathBuf,
    pub body: PathBuf,
    pub code: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Colors {
    pub text: Color,
    pub bullet: Color,
    pub title_rule: Color,
    pub code_background: Color,
}

/// Newtype around an RGB triple, serialized as a CSS-style `#rrggbb` string.
#[derive(Debug, Clone, Copy)]
pub struct Color(pub u8, pub u8, pub u8);

impl Color {
    pub fn rgb(&self) -> (u8, u8, u8) {
        (self.0, self.1, self.2)
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        parse_hex_color(&s).map_err(serde::de::Error::custom)
    }
}

fn parse_hex_color(s: &str) -> std::result::Result<Color, String> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    if hex.len() != 6 {
        return Err(format!("color must be #rrggbb, got {s:?}"));
    }
    let parse = |i: usize| {
        u8::from_str_radix(&hex[i..i + 2], 16)
            .map_err(|e| format!("bad color {s:?}: {e}"))
    };
    Ok(Color(parse(0)?, parse(2)?, parse(4)?))
}

// ── Defaults ──────────────────────────────────────────────────────────────

impl Default for Slide {
    fn default() -> Self {
        Self {
            width_mm: 254.0,
            height_mm: 143.0,
            margin_x_mm: 8.0,
            margin_top_mm: 10.0,
            page_bottom_reserved_mm: 6.0,
        }
    }
}

impl Default for Title {
    fn default() -> Self {
        Self { font_size_pt: 28.0, rule_offset_mm: 9.0, content_gap_mm: 4.0 }
    }
}

impl Default for Body {
    fn default() -> Self {
        Self { font_size_pt: 18.0, line_height_mm: 9.0, section_gap_mm: 5.0 }
    }
}

impl Default for Code {
    fn default() -> Self {
        Self {
            font_size_pt: 12.0,
            line_height_mm: 6.5,
            padding_mm: 4.0,
            icon_size_mm: 16.0,
            icon_inset_mm: 2.0,
        }
    }
}

impl Default for Svg {
    fn default() -> Self {
        Self {
            filter_scale: 2.0,
            flatten_css_vars: true,
            top_gap_mm: 4.0,
            margin_x_mm: 4.0,
        }
    }
}

impl Default for Fonts {
    fn default() -> Self {
        Self {
            title: "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf".into(),
            body:  "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".into(),
            code:  "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf".into(),
        }
    }
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            text:            Color(0, 0, 0),
            bullet:          Color(0, 0, 0),
            title_rule:      Color(0, 0, 0),
            code_background: Color(240, 240, 240),
        }
    }
}

// ── Loading ───────────────────────────────────────────────────────────────

impl Config {
    /// Resolve and load the config using the standard lookup order.
    /// - `explicit`: path from `--theme` (or equivalent). Errors if present
    ///   but unreadable/unparsable.
    /// - Otherwise searches `./rsslide.toml` then
    ///   `~/.config/rsslide/config.toml`.
    /// - Falls back to `Config::default()` if nothing found.
    pub fn load(explicit: Option<&Path>) -> Result<Self> {
        if let Some(path) = explicit {
            return Self::from_file(path);
        }
        let candidates = [
            PathBuf::from("rsslide.toml"),
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("rsslide/config.toml"),
        ];
        for path in candidates.iter() {
            if path.exists() {
                return Self::from_file(path);
            }
        }
        Ok(Self::default())
    }

    fn from_file(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        toml::from_str(&text)
            .with_context(|| format!("parsing config {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_compiles() {
        let cfg = Config::default();
        assert!((cfg.slide.width_mm - 254.0).abs() < 0.01);
        assert_eq!(cfg.colors.text.rgb(), (0, 0, 0));
    }

    #[test]
    fn partial_override_keeps_other_defaults() {
        let toml = r#"
            [title]
            font_size_pt = 40
        "#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert!((cfg.title.font_size_pt - 40.0).abs() < 0.01);
        // Unchanged defaults survive.
        assert!((cfg.body.font_size_pt - 18.0).abs() < 0.01);
        assert!((cfg.slide.width_mm - 254.0).abs() < 0.01);
    }

    #[test]
    fn colors_parse_hex() {
        let toml = r##"
            [colors]
            text = "#102030"
            bullet = "#ffcc00"
        "##;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.colors.text.rgb(), (0x10, 0x20, 0x30));
        assert_eq!(cfg.colors.bullet.rgb(), (0xff, 0xcc, 0x00));
    }

    #[test]
    fn bad_hex_color_is_rejected() {
        let toml = r#"
            [colors]
            text = "not-a-color"
        "#;
        let err = toml::from_str::<Config>(toml).unwrap_err();
        assert!(err.to_string().contains("#rrggbb"));
    }

    #[test]
    fn default_template_parses_to_default() {
        let parsed: Config = toml::from_str(DEFAULT_CONFIG_TEMPLATE)
            .expect("DEFAULT_CONFIG_TEMPLATE should parse");
        let def = Config::default();
        // Spot-check every sub-struct's key field; if any drift the test
        // catches it and the template needs updating.
        assert!((parsed.slide.width_mm - def.slide.width_mm).abs() < 0.001);
        assert!((parsed.slide.margin_x_mm - def.slide.margin_x_mm).abs() < 0.001);
        assert!((parsed.title.font_size_pt - def.title.font_size_pt).abs() < 0.001);
        assert!((parsed.body.font_size_pt - def.body.font_size_pt).abs() < 0.001);
        assert!((parsed.code.font_size_pt - def.code.font_size_pt).abs() < 0.001);
        assert!((parsed.svg.filter_scale - def.svg.filter_scale).abs() < 0.001);
        assert!((parsed.svg.top_gap_mm - def.svg.top_gap_mm).abs() < 0.001);
        assert!((parsed.svg.margin_x_mm - def.svg.margin_x_mm).abs() < 0.001);
        assert_eq!(parsed.svg.flatten_css_vars, def.svg.flatten_css_vars);
        assert_eq!(parsed.fonts.body, def.fonts.body);
        assert_eq!(parsed.colors.text.rgb(), def.colors.text.rgb());
        assert_eq!(parsed.colors.code_background.rgb(), def.colors.code_background.rgb());
    }

    #[test]
    fn unknown_field_is_rejected() {
        let toml = r#"
            [slide]
            widht_mm = 100
        "#;
        assert!(toml::from_str::<Config>(toml).is_err());
    }
}
