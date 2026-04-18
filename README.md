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
| `paginate` | bool   | `false` | Show `N / total` page numbers on each slide |
| `slides`   | list   | —       | List of slide objects (see below) |

### Slide fields

| Field        | Type                              | Description                         |
|--------------|-----------------------------------|-------------------------------------|
| `title`      | string                            | Slide heading                       |
| `content`    | string                            | Body text                           |
| `bullets`    | list of strings                   | Bullet point list                   |
| `code`       | `{language, source, trim?}`       | Syntax-highlighted code block       |
| `align`      | `left` \| `center` \| `right`    | Horizontal text alignment           |
| `valign`     | `top` \| `middle` \| `bottom`    | Vertical content alignment          |
| `image`      | string                            | Path to image file                  |
| `table`      | `{headers, rows, aligns?}`        | Grid table with header + body rows  |
| `class`      | string                            | CSS class (e.g. `lead`)             |
| `background` | string                            | CSS background value or color       |

#### Table fields

| Field     | Type                            | Default       | Description |
|-----------|---------------------------------|---------------|-------------|
| `headers` | list of strings                 | —             | Header-row cells. Must be non-empty. |
| `rows`    | list of list of strings         | —             | Body rows. Every row must have exactly `len(headers)` cells (hard error otherwise). |
| `aligns`  | list of `left`/`center`/`right` | all `left`    | Per-column horizontal alignment. Length must equal `len(headers)`. |

GFM tables in `.md` inputs (`| A | B |` / `|:--|--:|`) are translated to this
form by `rsslide import` — the alignment markers in the separator row are
preserved.

#### Code block fields

| Field      | Type    | Default | Description |
|------------|---------|---------|-------------|
| `language` | string  | —       | Language token for syntax highlighting (e.g. `rust`, `python`) |
| `source`   | string  | —       | Source code. Use `|-` (YAML strip chomping) to avoid a trailing blank line |
| `trim`     | bool    | `false` | Strip trailing newlines in code; prefer `|-` in YAML instead |

## Usage

```
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

```
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

## PDF code rendering

Code blocks are syntax-highlighted using `syntect` with the `InspiredGitHub` theme. Each token is coloured individually by advancing the X cursor by the Courier glyph width (0.6 × font-size × mm/pt). Font size is 12 pt; line spacing is 6.5 mm. A light-gray background box is drawn behind the block with 4 mm padding on all sides.

When a `language` is recognised, a language icon (SVG via `printpdf`'s `svg` feature) is placed in the top-right corner of the code box at 16 mm. Supported languages: Rust, Python, JavaScript, TypeScript, Go (Gopher), Java, C++, Ruby, Bash, HTML, CSS.

See [`docs/code-highlighting.md`](docs/code-highlighting.md) for full implementation details.

### Model fields and PDF support

Some slide fields defined in the YAML schema are not yet rendered by the PDF exporter and are reserved for future use:

| Field        | PDF support |
|--------------|-------------|
| `title`      | ✅ rendered  |
| `content`    | ✅ rendered  |
| `bullets`    | ✅ rendered  |
| `code`       | ✅ rendered (syntax-highlighted, language icon) |
| `align`      | ✅ rendered (`left` / `center` / `right`) |
| `valign`     | ✅ rendered (`top` / `middle` / `bottom`) |
| `table`      | ✅ rendered (grid with per-column alignment; fails hard on overflow) |
| `image`      | 🔜 planned   |
| `class`      | 🔜 planned   |
| `background` | 🔜 planned   |
| `theme`      | 🔜 planned   |

## Roadmap

- **Phase 1 (MVP):** YAML input, HTML/PDF/PPTX output, 3 built-in themes, custom CSS
- **Phase 2:** `--watch` mode, more slide field types, library crate
- **Phase 3:** Full Marp directive compatibility
