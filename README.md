# rsslide

A fast, pure-Rust presentation tool inspired by [Marp](https://marp.app/).

## Input format

Presentations are written as `.yaml` files:

```yaml
title: My Presentation
theme: default
paginate: true

slides:
  - title: Welcome
    class: lead
    background: "#1a1a2e"
    content: Hello world

  - title: Code Example
    code:
      language: rust
      source: |
        fn main() {
            println!("hello");
        }

  - title: Key Points
    bullets:
      - Fast
      - No browser needed
    image: assets/diagram.png
```

### Presentation fields

| Field      | Type   | Default | Description |
|------------|--------|---------|-------------|
| `title`    | string | —       | Presentation title (used in PDF metadata) |
| `theme`    | string | `default` | Built-in theme name |
| `paginate` | bool   | `false` | Show the page number in the bottom-right corner of each slide |
| `slides`   | list   | —       | List of slide objects (see below) |

### Slide fields

| Field        | Type                              | Description                         |
|--------------|-----------------------------------|-------------------------------------|
| `title`      | string                            | Slide heading                       |
| `title_level` | 1..6                             | Heading level of the title (default 1, like `#`; `rsslide import` keeps the marp level) |
| `subtitle` / `subtitles` | string \| `{text, level}` \| list of either | One or more sub-headings rendered stacked under the title (level 2..6, default 2) |
| `content`    | string                            | Body text                           |
| `bullets`    | list of strings                   | Bullet point list                   |
| `code`       | `{language, source, trim?}`       | Syntax-highlighted code block       |
| `align`      | `left` \| `center` \| `right`    | Horizontal text alignment (default `left`) |
| `title_align` / `content_align` | `left` \| `center` \| `right` | Override `align` for the title / for everything else |
| `valign`     | `top` \| `middle` \| `bottom`    | Vertical content alignment (default from config: `middle`, like marp) |
| `columns`    | list of `{header?, bullets}`      | Side-by-side bullet columns         |
| `bullets_first` | bool                           | Draw `bullets` above `columns` (default: below) |
| `image`      | string                            | Path to an SVG file                 |
| `svg`        | string                            | Inline SVG markup                   |
| `table`      | `{headers, rows, aligns?}`        | Grid table with header + body rows  |
| `class`      | string                            | CSS class (e.g. `lead`)             |
| `background` | string                            | CSS background value or color       |

#### Table fields

| Field     | Type                            | Default       | Description |
|-----------|---------------------------------|---------------|-------------|
| `headers` | list of strings                 | —             | Header-row cells. Must be non-empty. |
| `rows`    | list of list of strings         | —             | Body rows. Every row must have exactly `len(headers)` cells (hard error otherwise). |
| `aligns`  | list of `auto`/`left`/`center`/`right` | all `auto` | Per-column horizontal alignment. Length must equal `len(headers)`. `auto` centers the header and left-aligns the body, as a browser does for a GFM column without alignment colons. |

GFM tables in `.md` inputs (`| A | B |` / `|:--|--:|`) are translated to this
form by `rsslide import` — the alignment markers in the separator row are
preserved.

Text in titles, subtitles, content, bullets, columns and table cells may use
inline markdown: `**bold**`, `*italic*`, `` `code` ``, `[text](url)`; bare
URLs and e-mail addresses are shown as links.

#### Code block fields

| Field      | Type    | Default | Description |
|------------|---------|---------|-------------|
| `language` | string  | —       | Language token for syntax highlighting (e.g. `rust`, `python`) |
| `source`   | string  | —       | Source code. Use `|-` (YAML strip chomping) to avoid a trailing blank line |
| `trim`     | bool    | `false` | Strip trailing newlines in code; prefer `|-` in YAML instead |

## Usage

```text
rsslide process [OPTIONS] <INPUT>
rsslide import  [OPTIONS] <INPUT>
rsslide version

process options:
  -o, --output <FILE>        Output file [default: <input stem>.<format>]
  -f, --format <FORMAT>      html | pdf | pptx  [default: html]
      --theme <THEME>        Built-in theme: default | gaia | uncover  [default: default]
      --theme-set <CSS>      Path to a custom CSS theme file
  -h, --help
```

## Output formats

| Format | How it works |
|--------|-------------|
| HTML   | Tera-templated full-page slide deck |
| PDF    | Pure Rust via `printpdf` — no Chromium or wkhtmltopdf needed |
| PPTX   | Open XML generated from scratch using `zip` |

## Themes

Built-in themes: `default`, `gaia`, `uncover`.
Supply your own with `--theme-set my-theme.css`.

## Architecture

```text
YAML file
  └─► Parser (serde_yaml)
        └─► Slide model (Presentation + Vec<Slide>)
              ├─► HTML Renderer  ──► .html
              ├─► PDF Exporter   ──► .pdf
              └─► PPTX Exporter  ──► .pptx
```

### Key dependencies

| Crate           | Purpose                        |
|-----------------|--------------------------------|
| `clap`          | CLI argument parsing           |
| `serde_yaml`    | YAML input parsing             |
| `tera`          | HTML slide templating          |
| `printpdf`      | Pure-Rust PDF generation (svg feature for icons) |
| `zip`           | PPTX (Open XML) packaging      |
| `syntect`       | Syntax highlighting in PDF     |
| `anyhow`        | Error handling                 |

## PDF rendering

The PDF exporter reproduces the look of marp's `default` theme: Noto Sans
text (semibold headings), the GitHub palette, the same font sizes, margins,
list markers, table styling and page-number position, and content centered
vertically on the slide. Text is measured with the same shaper that draws it
(rustybuzz), so wrapping and alignment are exact.

Layout is relative: the page size is the only absolute value. The base font
size and the paddings are fractions of the page, and every other size is a
multiple of the base font size, so changing the page size rescales the whole
slide. Every value is configurable; `rsslide dump-config` prints them all
with comments, and an `rsslide.toml` next to the inputs overrides any of
them.

Code blocks are highlighted with `syntect` using a bundled theme with the
colors marp uses (configurable as `code.theme`), drawn in a bordered,
rounded box, and shrunk to fit the slide when they are too large, like
marp's auto-scaling. When a `language` is recognised, its icon is placed in
the top-right corner of the box. Supported icons: Rust, Python, JavaScript,
TypeScript, Go (Gopher), Java, C++, Ruby, Bash, HTML, CSS.

Content that does not fit on the page is a hard error naming the slide;
marp would silently cut it off at the page edge.

### Model fields and PDF support

Some slide fields defined in the YAML schema are not yet rendered by the PDF exporter and are reserved for future use:

| Field        | PDF support |
|--------------|-------------|
| `title`      | ✅ rendered  |
| `subtitle` / `subtitles` | ✅ rendered (font size scales with `level` 2..6; multiple stack vertically) |
| `content`    | ✅ rendered  |
| `bullets`    | ✅ rendered  |
| `code`       | ✅ rendered (syntax-highlighted, language icon, shrinks to fit) |
| `align`      | ✅ rendered (`left` / `center` / `right`) |
| `valign`     | ✅ rendered (`top` / `middle` / `bottom`) |
| `columns`    | ✅ rendered  |
| `table`      | ✅ rendered (per-column alignment; cells wrap; fails hard if even wrapped it is too wide) |
| `image` / `svg` | ✅ rendered (SVG only; fills the space below the other content) |
| `class`      | 🔜 planned   |
| `background` | 🔜 planned   |
| `theme`      | 🔜 planned   |

## Roadmap

- **Phase 1 (MVP):** YAML input, HTML/PDF/PPTX output, 3 built-in themes, custom CSS
- **Phase 2:** `--watch` mode, more slide field types, library crate
- **Phase 3:** Full Marp directive compatibility
