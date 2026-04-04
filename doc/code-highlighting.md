# Code Highlighting in rsslide

## Current implementation

Code blocks are syntax-highlighted in the PDF exporter using the
[`syntect`](https://github.com/trishume/syntect) crate with the built-in
`InspiredGitHub` theme.

### How it works

1. The YAML `code` field supplies a `language` token (e.g. `rust`, `python`),
   a `source` string, and an optional `trim` flag.
2. If `trim: true`, trailing newlines are stripped from `source` before
   rendering.  The recommended alternative is to use YAML's strip-chomping
   indicator (`|-`) which achieves the same result at the YAML level and keeps
   the model clean.
3. `syntect` resolves the language to a syntax definition via
   `SyntaxSet::find_syntax_by_token`.  If the language is unknown it falls back
   to plain text.
4. `HighlightLines` iterates over every line from `LinesWithEndings` and returns
   a list of `(Style, &str)` token pairs.
5. Each token is rendered with `PdfLayerReference::use_text` at an absolute X
   position.  The X cursor advances by
   `token.chars().count() × COURIER_CHAR_WIDTH_MM` after each token, where
   `COURIER_CHAR_WIDTH_MM = 0.6 × CODE_FONT_SIZE(pt) × (25.4mm / 72pt)`.
6. A light-gray background rectangle (`PaintMode::Fill`, colour `#F0F0F0`) is
   drawn *before* the text so the box sits behind all tokens.

### Box geometry

```
box_top    = first_line_baseline + CODE_PADDING
box_bottom = last_line_baseline  - CODE_PADDING
           = first_line_baseline - (N-1) × CODE_LINE_HEIGHT - CODE_PADDING
```

Using `(N-1)` (not `N`) is intentional: the loop decrements `cursor_y` after
every line including the last, but the box must be anchored to the last *text
baseline*, not one full line-height below it.  Using `N` creates a visual
phantom blank line at the bottom of every block.

### Key constants (src/exporter/pdf.rs)

| Constant | Value | Purpose |
|---|---|---|
| `CODE_FONT_SIZE` | 12 pt | Courier font size |
| `CODE_LINE_HEIGHT` | 6.5 mm | Vertical advance per source line |
| `CODE_PADDING` | 4 mm | Equal top/bottom padding inside the background box |
| `CODE_BG` | 0.94 | Gray level of the background fill (RGB all equal) |
| `COURIER_CHAR_WIDTH_MM` | ≈ 2.54 mm | Advance width per character at 12 pt |
| `ICON_SIZE_MM` | 16 mm | Rendered size of the language icon |
| `ICON_INSET_MM` | 2 mm | Gap between icon and box edges |

---

## Language icons

When a `language` is specified, a small icon representing that language is
displayed in the top-right corner of the gray background box.

### Rendering pipeline

SVG bytes are embedded at compile time with `include_str!` in `src/assets.rs`.
At render time:

```
SVG string
  └─► Svg::parse()          (printpdf svg feature — uses svg2pdf + usvg internally)
        └─► SvgXObjectRef   (PDF XObject embedded in the document)
              └─► add_to_layer(transform)  ──► placed at top-right of code box
```

The `printpdf` `svg` feature must be explicitly enabled in `Cargo.toml`:

```toml
printpdf = { version = "0.7", features = ["svg"] }
```

### Scale formula

```
scale = Mm(ICON_SIZE_MM).into_pt() / svg.width.into_pt(72.0)
```

At `dpi = 72`, one SVG user unit = one point.  `svg.width` is read from the
parsed XObject BBox so it works for any viewBox — not just the 24×24 grid used
by simple-icons.

### Placement geometry

```
┌─────────────────────────────────────┬──────┐  ← box_top
│  fn main() {                        │ 🦀   │
│      println!("Hello!");            └──────┤
│  }                                        │
└───────────────────────────────────────────┘  ← box_bottom
   ^                                    ^
   MARGIN_X - CODE_PADDING        SLIDE_W - MARGIN_X + CODE_PADDING
```

Icon bottom-left: `(box_right - ICON_INSET_MM - ICON_SIZE_MM, box_top - ICON_INSET_MM - ICON_SIZE_MM)`

### Bundled icons (assets/icons/)

| Language token(s) | File |
|---|---|
| `rust` | `rust.svg` (simple-icons) |
| `python` | `python.svg` (simple-icons) |
| `javascript`, `js` | `javascript.svg` (simple-icons) |
| `typescript`, `ts` | `typescript.svg` (simple-icons) |
| `go`, `golang` | `go.svg` (golang-samples Gopher vector) |
| `java` | `openjdk.svg` (simple-icons) |
| `cpp`, `c++` | `cplusplus.svg` (simple-icons) |
| `ruby`, `rb` | `ruby.svg` (simple-icons) |
| `bash`, `sh`, `shell` | `gnubash.svg` (simple-icons) |
| `html` | `html5.svg` (simple-icons) |
| `css` | `css3.svg` (simple-icons) |

> **Note on Go:** the official Go wordmark SVG (simple-icons `go.svg`) uses a
> wide, flat layout that appears visually small at icon size.  The Go Gopher
> vector from [golang-samples](https://github.com/golang-samples/gopher-vector)
> fills the canvas better and is used instead.

### Unknown languages

If `crate::assets::language_icon(lang)` returns `None`, no icon is drawn.
The code block renders normally without an icon.

### Open questions

- Should the icon be tinted to match the `syntect` theme's keyword colour, or
  always rendered in the language's official brand colour?
- Should icons also appear in the HTML exporter (as `<img>` tags)?

## Current implementation

Code blocks are syntax-highlighted in the PDF exporter using the
[`syntect`](https://github.com/trishume/syntect) crate with the built-in
`InspiredGitHub` theme.

### How it works

1. The YAML `code` field supplies a `language` token (e.g. `rust`, `python`) and
   a `source` string.
2. `syntect` resolves the language to a syntax definition via
   `SyntaxSet::find_syntax_by_token`.  If the language is unknown it falls back
   to plain text.
3. `HighlightLines` iterates over every line from `LinesWithEndings` and returns
   a list of `(Style, &str)` token pairs.
4. Each token is rendered with `PdfLayerReference::use_text` at an absolute X
   position.  The X cursor advances by
   `token.chars().count() × COURIER_CHAR_WIDTH_MM` after each token, where
   `COURIER_CHAR_WIDTH_MM = 0.6 × CODE_FONT_SIZE(pt) × (25.4mm / 72pt)`.
5. A light-gray background rectangle (`PaintMode::Fill`, colour `#F0F0F0`) is
   drawn *before* the text so the box sits behind all tokens.

### Key constants (src/exporter/pdf.rs)

| Constant | Value | Purpose |
|---|---|---|
| `CODE_FONT_SIZE` | 12 pt | Courier font size |
| `CODE_LINE_HEIGHT` | 6.5 mm | Vertical advance per source line |
| `CODE_PADDING` | 4 mm | Top and bottom padding inside the background box |
| `CODE_BG` | 0.94 | Gray level of the background fill (RGB all equal) |
| `COURIER_CHAR_WIDTH_MM` | ≈ 2.54 mm | Advance width per character at 12 pt |

---

## Planned: language icon in the top-right corner of the code box

When a `language` is specified, display a small icon representing that language
in the top-right corner of the gray background box.

### Icon source

Use [simple-icons](https://simpleicons.org/) SVG files.  Simple-icons are
single-colour, flat SVGs — they render correctly at small sizes and have no
gradients or external references.

Curated starter set (subject to change):

`rust` · `python` · `javascript` · `typescript` · `go` · `java` · `c` · `cpp`
· `ruby` · `bash` · `sql` · `html` · `css`

### Rendering pipeline

```
SVG bytes  ──►  resvg (rasterise to RGBA pixels)
           ──►  encode as PNG bytes
           ──►  printpdf image API  ──►  placed in PDF at (box_right - icon_size - padding, box_top - padding)
```

**Crate additions:**

| Crate | Purpose |
|---|---|
| `resvg` | Pure-Rust SVG rasteriser |
| `tiny-skia` | Pixel buffer used by resvg (pulled in transitively) |
| `png` (or `image`) | Encode the RGBA pixel buffer to PNG bytes for printpdf |

### Asset bundling

SVG files are stored in `assets/icons/<language>.svg` and embedded at compile
time with `include_bytes!`.  A `match` statement in the exporter maps
`language` strings to the appropriate byte slice (returning `None` for unknown
languages, in which case no icon is drawn).

### Placement geometry

```
┌─────────────────────────────────────┬──────┐  ← box_top
│  fn main() {                        │ 🦀   │
│      println!("Hello!");            └──────┤
│  }                                        │
└───────────────────────────────────────────┘  ← box_bottom
   ^                                    ^
   MARGIN_X - CODE_PADDING              box_right
```

Icon size: **12 × 12 mm**.  Inset from the top-right corner of the box by
`CODE_PADDING` on both axes.

### Open questions

- Should the icon be tinted to match the `syntect` theme's keyword colour, or
  always rendered in the language's official brand colour?
- For languages without a simple-icons entry, fall back silently (no icon) or
  use a generic "code" icon?
- Should icons also appear in the HTML exporter (as `<img>` tags from a CDN
  URL)?
