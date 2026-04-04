# rsmarp

A fast, pure-Rust presentation tool inspired by [Marp](https://marp.app/).
Write your slides in YAML, export to HTML, PDF, or PPTX — no browser required.

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

### Slide fields

| Field        | Type                    | Description                         |
|--------------|-------------------------|-------------------------------------|
| `title`      | string                  | Slide heading                       |
| `content`    | string                  | Body text                           |
| `bullets`    | list of strings         | Bullet point list                   |
| `code`       | `{language, source}`    | Syntax-highlighted code block       |
| `image`      | string                  | Path to image file                  |
| `class`      | string                  | CSS class (e.g. `lead`)             |
| `background` | string                  | CSS background value or color       |

## Usage

```
rsmarp [OPTIONS] <INPUT>

Options:
  -o, --output <FILE>        Output file [default: <input stem>.<format>]
  -f, --format <FORMAT>      html | pdf | pptx  [default: html]
      --theme <THEME>        Built-in theme: default | gaia | uncover  [default: default]
      --theme-set <CSS>      Path to a custom CSS theme file
  -h, --help
  -V, --version
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
| `printpdf`      | Pure-Rust PDF generation       |
| `zip`           | PPTX (Open XML) packaging      |
| `anyhow`        | Error handling                 |

## Roadmap

- **Phase 1 (MVP):** YAML input, HTML/PDF/PPTX output, 3 built-in themes, custom CSS
- **Phase 2:** `--watch` mode, more slide field types, library crate
- **Phase 3:** Full Marp directive compatibility
