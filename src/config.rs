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

use crate::model::VAlign;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// A commented TOML file containing every knob with its default value.
/// The values here MUST match the `Default` impls below — a unit test
/// asserts the round-trip equals `Config::default()`.
pub const DEFAULT_CONFIG_TEMPLATE: &str = r##"# rsslide configuration. Drop this file as `rsslide.toml` next to your
# YAML inputs (or pass `--config PATH`). Every section is optional — any
# field you omit keeps its built-in default.
#
# The defaults reproduce marp's `default` theme. Layout is relative: the
# page size is the only absolute value. The base font size and the page
# padding are fractions of the page, and every other size is in `em`, a
# multiple of the base font size. Changing the page size rescales
# everything. Marp's own numbers are on a 1280 x 720 px canvas with a
# 29 px base font; the comments show where each default comes from.

[slide]
# Page size in millimeters. Marp's 1280 x 720 px slide prints as
# 960 x 540 pt (PowerPoint widescreen).
width_mm = 338.667
height_mm = 190.5
# Base font size as a fraction of the page width (29 / 1280).
font_size_ratio = 0.02265625
# Content padding: left/right as a fraction of the page width (78.5 / 1280),
# top/bottom as a fraction of the page height (78.5 / 720).
padding_x_ratio = 0.061328125
padding_y_ratio = 0.109028
# Line height of body text, as a multiple of its font size.
line_height = 1.5
# Default vertical placement of the content block: top | middle | bottom.
# A slide's own `valign` overrides it.
valign = "middle"

[heading]
# Font sizes of heading levels 1..6 (the title and the subtitles), in em.
font_sizes_em = [1.6, 1.3, 1.1, 1.05, 1.0, 0.9]
line_height = 1.25
# Padding below levels 1..6, in em of the heading's own size (GitHub's
# 0.3em under h1 and h2).
padding_bottom_em = [0.3, 0.3, 0.0, 0.0, 0.0, 0.0]
# Space above and below a heading (24 px and 16 px).
margin_top_em = 0.827586
margin_bottom_em = 0.551724

[block]
# Space below a paragraph, list, table or code block (16 px).
margin_bottom_em = 0.551724

[list]
# Indent of bullet text from the content edge; the bullet hangs in it.
indent_em = 2.0
# Extra space between consecutive bullets.
item_gap_em = 0.25
# The bullet disc, as Chromium draws it for marp: its diameter, how far
# its center sits left of the text, and how far above the baseline.
disc_size_em = 0.345
disc_offset_em = 0.741
disc_rise_em = 0.345

[columns]
# Gap between side-by-side columns.
gap_em = 1.0

[code]
# Code blocks. Font size in em of the base font; line height as a multiple
# of the code font size; the rest in em of the base font.
font_size_em = 0.85
line_height = 1.15
padding_em = 0.551724
radius_em = 0.206897
border_em = 0.034483
# Language icon at the top-right of each code block.
icon_size_em = 1.2
icon_inset_em = 0.3
# Highlighting theme: "GitHub" (bundled; the colors marp uses) or any of
# syntect's built-in themes, e.g. "InspiredGitHub" or "Solarized (light)".
theme = "GitHub"

[inline_code]
# `code` spans inside text. Font size relative to the surrounding text;
# padding relative to the code font size; radius in em of the base font.
font_size_em = 0.85
padding_x_em = 0.4
padding_y_em = 0.2
radius_em = 0.206897

[table]
# Cell padding (6 px x 13 px) and border width (1 px).
cell_padding_x_em = 0.448276
cell_padding_y_em = 0.206897
border_em = 0.034483

[pagination]
# The page number (24 px), 30 px from the right and 21 px from the bottom.
font_size_em = 0.827586
right_ratio = 0.0234375
bottom_ratio = 0.029167

[svg]
# Filter-effect rasterization detail. 2.0 is a good quality/speed tradeoff.
# 4.0 is krilla-svg's default (crispest, slowest). 1.0 is fastest (softer).
filter_scale = 2.0
# Pre-resolve CSS `var(--x)` references before usvg parses the SVG.
# Leave on — usvg otherwise silently drops strokes using CSS vars.
flatten_css_vars = true
# An SVG fills the space left below the slide's other content. Gap above it:
top_gap_em = 0.551724
# Side and bottom margins of that space, as fractions of the page width and
# height. Defaults to the content padding.
margin_x_ratio = 0.061328125
margin_bottom_ratio = 0.109028

[fonts]
# Absolute paths to TTF/OTF files for each role. Marp's default theme
# renders with Noto Sans (semibold for headings and bold) and the system
# monospace font.
regular     = "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf"
bold        = "/usr/share/fonts/truetype/noto/NotoSans-SemiBold.ttf"
italic      = "/usr/share/fonts/truetype/noto/NotoSans-Italic.ttf"
bold_italic = "/usr/share/fonts/truetype/noto/NotoSans-SemiBoldItalic.ttf"
mono        = "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf"

[colors]
# Colors as CSS-style hex strings.
text                   = "#1f2328"
h1                     = "#224466"
h6                     = "#59636e"
link                   = "#0969da"
code_background        = "#f6f8fa"
code_border            = "#d1d9e0"
inline_code_background = "#f0f1f3"
table_border           = "#d1d9e0"
table_stripe           = "#f6f8fa"
pagination             = "#777777"
"##;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub slide: Slide,
    pub heading: Heading,
    pub block: Block,
    pub list: List,
    pub columns: Columns,
    pub code: Code,
    pub inline_code: InlineCode,
    pub table: Table,
    pub pagination: Pagination,
    pub svg: Svg,
    pub fonts: Fonts,
    pub colors: Colors,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Slide {
    pub width_mm: f32,
    pub height_mm: f32,
    pub font_size_ratio: f32,
    pub padding_x_ratio: f32,
    pub padding_y_ratio: f32,
    pub line_height: f32,
    pub valign: VAlign,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Heading {
    /// Font sizes of heading levels 1..=6, in em.
    pub font_sizes_em: [f32; 6],
    pub line_height: f32,
    /// Padding below heading levels 1..=6, in em of the heading's size.
    pub padding_bottom_em: [f32; 6],
    pub margin_top_em: f32,
    pub margin_bottom_em: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Block {
    pub margin_bottom_em: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct List {
    pub indent_em: f32,
    pub item_gap_em: f32,
    pub disc_size_em: f32,
    pub disc_offset_em: f32,
    pub disc_rise_em: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Columns {
    pub gap_em: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Code {
    pub font_size_em: f32,
    pub line_height: f32,
    pub padding_em: f32,
    pub radius_em: f32,
    pub border_em: f32,
    pub icon_size_em: f32,
    pub icon_inset_em: f32,
    pub theme: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct InlineCode {
    pub font_size_em: f32,
    pub padding_x_em: f32,
    pub padding_y_em: f32,
    pub radius_em: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Table {
    pub cell_padding_x_em: f32,
    pub cell_padding_y_em: f32,
    pub border_em: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Pagination {
    pub font_size_em: f32,
    pub right_ratio: f32,
    pub bottom_ratio: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Svg {
    pub filter_scale: f32,
    pub flatten_css_vars: bool,
    pub top_gap_em: f32,
    pub margin_x_ratio: f32,
    pub margin_bottom_ratio: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Fonts {
    pub regular: PathBuf,
    pub bold: PathBuf,
    pub italic: PathBuf,
    pub bold_italic: PathBuf,
    pub mono: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Colors {
    pub text: Color,
    pub h1: Color,
    pub h6: Color,
    pub link: Color,
    pub code_background: Color,
    pub code_border: Color,
    pub inline_code_background: Color,
    pub table_border: Color,
    pub table_stripe: Color,
    pub pagination: Color,
}

/// Newtype around an RGB triple, serialized as a CSS-style `#rrggbb` string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    let hex = s
        .strip_prefix('#')
        .ok_or_else(|| format!("color must be #rrggbb, got {s:?}"))?;
    if hex.len() != 6 {
        return Err(format!("color must be #rrggbb, got {s:?}"));
    }
    let parse = |i: usize| {
        u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| format!("bad color {s:?}: {e}"))
    };
    Ok(Color(parse(0)?, parse(2)?, parse(4)?))
}

// ── Defaults ──────────────────────────────────────────────────────────────

impl Default for Slide {
    fn default() -> Self {
        Self {
            width_mm: 338.667,
            height_mm: 190.5,
            font_size_ratio: 0.02265625,
            padding_x_ratio: 0.061328125,
            padding_y_ratio: 0.109028,
            line_height: 1.5,
            valign: VAlign::Middle,
        }
    }
}

impl Default for Heading {
    fn default() -> Self {
        Self {
            font_sizes_em: [1.6, 1.3, 1.1, 1.05, 1.0, 0.9],
            line_height: 1.25,
            padding_bottom_em: [0.3, 0.3, 0.0, 0.0, 0.0, 0.0],
            margin_top_em: 0.827586,
            margin_bottom_em: 0.551724,
        }
    }
}

impl Default for Block {
    fn default() -> Self {
        Self {
            margin_bottom_em: 0.551724,
        }
    }
}

impl Default for List {
    fn default() -> Self {
        Self {
            indent_em: 2.0,
            item_gap_em: 0.25,
            disc_size_em: 0.345,
            disc_offset_em: 0.741,
            disc_rise_em: 0.345,
        }
    }
}

impl Default for Columns {
    fn default() -> Self {
        Self { gap_em: 1.0 }
    }
}

impl Default for Code {
    fn default() -> Self {
        Self {
            font_size_em: 0.85,
            line_height: 1.15,
            padding_em: 0.551724,
            radius_em: 0.206897,
            border_em: 0.034483,
            icon_size_em: 1.2,
            icon_inset_em: 0.3,
            theme: "GitHub".into(),
        }
    }
}

impl Default for InlineCode {
    fn default() -> Self {
        Self {
            font_size_em: 0.85,
            padding_x_em: 0.4,
            padding_y_em: 0.2,
            radius_em: 0.206897,
        }
    }
}

impl Default for Table {
    fn default() -> Self {
        Self {
            cell_padding_x_em: 0.448276,
            cell_padding_y_em: 0.206897,
            border_em: 0.034483,
        }
    }
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            font_size_em: 0.827586,
            right_ratio: 0.0234375,
            bottom_ratio: 0.029167,
        }
    }
}

impl Default for Svg {
    fn default() -> Self {
        Self {
            filter_scale: 2.0,
            flatten_css_vars: true,
            top_gap_em: 0.551724,
            margin_x_ratio: 0.061328125,
            margin_bottom_ratio: 0.109028,
        }
    }
}

impl Default for Fonts {
    fn default() -> Self {
        Self {
            regular: "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf".into(),
            bold: "/usr/share/fonts/truetype/noto/NotoSans-SemiBold.ttf".into(),
            italic: "/usr/share/fonts/truetype/noto/NotoSans-Italic.ttf".into(),
            bold_italic: "/usr/share/fonts/truetype/noto/NotoSans-SemiBoldItalic.ttf".into(),
            mono: "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf".into(),
        }
    }
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            text: Color(0x1f, 0x23, 0x28),
            h1: Color(0x22, 0x44, 0x66),
            h6: Color(0x59, 0x63, 0x6e),
            link: Color(0x09, 0x69, 0xda),
            code_background: Color(0xf6, 0xf8, 0xfa),
            code_border: Color(0xd1, 0xd9, 0xe0),
            inline_code_background: Color(0xf0, 0xf1, 0xf3),
            table_border: Color(0xd1, 0xd9, 0xe0),
            table_stripe: Color(0xf6, 0xf8, 0xfa),
            pagination: Color(0x77, 0x77, 0x77),
        }
    }
}

// ── Loading ───────────────────────────────────────────────────────────────

impl Config {
    /// Resolve and load the config using the standard lookup order.
    /// - `explicit`: path from `--config`. Errors if present
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
        toml::from_str(&text).with_context(|| format!("parsing config {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_override_keeps_other_defaults() {
        let toml = r#"
            [heading]
            line_height = 1.4
        "#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert!((cfg.heading.line_height - 1.4).abs() < 1e-6);
        // Unchanged defaults survive.
        let def = Config::default();
        assert_eq!(cfg.heading.font_sizes_em, def.heading.font_sizes_em);
        assert_eq!(cfg.slide.width_mm, def.slide.width_mm);
    }

    #[test]
    fn colors_parse_hex() {
        let toml = r##"
            [colors]
            text = "#102030"
            link = "#ffcc00"
        "##;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.colors.text.rgb(), (0x10, 0x20, 0x30));
        assert_eq!(cfg.colors.link.rgb(), (0xff, 0xcc, 0x00));
    }

    #[test]
    fn bad_hex_color_is_rejected() {
        for bad in ["not-a-color", "102030", "#12345"] {
            let toml = format!("[colors]\ntext = \"{bad}\"\n");
            let err = toml::from_str::<Config>(&toml).unwrap_err();
            assert!(err.to_string().contains("#rrggbb"), "{bad}: {err}");
        }
    }

    #[test]
    fn bad_valign_is_rejected() {
        let toml = "[slide]\nvalign = \"center\"\n";
        assert!(toml::from_str::<Config>(toml).is_err());
    }

    #[test]
    fn default_template_parses_to_default() {
        let parsed: Config =
            toml::from_str(DEFAULT_CONFIG_TEMPLATE).expect("DEFAULT_CONFIG_TEMPLATE should parse");
        // Every field, compared through Debug: any drift between the template
        // and the Default impls fails here.
        assert_eq!(format!("{parsed:?}"), format!("{:?}", Config::default()));
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
