# TOFIX

Findings from a code scan on 2026-10-04.

## High

- `src/main.rs:43` - `process --format` defaults to `html`, but `src/main.rs:154` bails with "HTML export not yet implemented", so `rsslide process deck.yaml` with no flags always fails; default to `pdf` (the only implemented format) until HTML exists.
- `README.md:82` - the Usage block documents `--theme` and `--theme-set` options that do not exist in the CLI (`src/main.rs:34-49`), omits the real `--config`, `generate` and `dump-config` subcommands, and advertises `html | pdf | pptx` although only PDF works (`src/main.rs:154`, `src/exporter/pptx.rs:7`); rewrite the section from `rsslide --help`.

## Medium

- `README.md:100` - says PDF is produced "via `printpdf`" and lists `printpdf` and `tera` under Key dependencies (`README.md:125-126`); the code uses `krilla`/`krilla-svg`/`usvg` (`Cargo.toml:15-17`) and no `printpdf`; update the Output formats, Architecture and dependency tables.
- `Cargo.toml:19` - `tera` and `zip` (`Cargo.toml:20`) are declared but never used anywhere in `src/` (HTML and PPTX export are stubs); drop them until the exporters that need them land.
- `Cargo.toml:18` - `serde_yaml` 0.9 is deprecated/unmaintained upstream; migrate to a maintained YAML crate (e.g. `serde_yml` / `serde_norway`).
- `src/exporter/pdf.rs:467` - `let _ = surface.draw_svg(...)` discards the render result, and the `if let Some(size)` at line 466 silently skips drawing on a zero-sized SVG; both contradict the "never silently skip" rule in `CLAUDE.md:7`; propagate an error instead.
- `src/exporter/pdf.rs:392` - an unrecognised code `language` silently falls back to plain text, and `highlight_line` errors are swallowed with `unwrap_or_default()` at line 398; fail hard with the offending language/line per `CLAUDE.md:9`.
- `src/exporter/pdf.rs:160` - unknown `valign` values fall through `_ =>` to top, and unknown `align` values fall through to left in `text_x` (`src/exporter/pdf.rs:823`, even asserted by the test at line 896); validate these enums at parse time (or make them serde enums) and reject bad values.

## Low

- `README.md:137` - links to `docs/code-highlighting.md`, but the file lives at `doc/code-highlighting.md`; fix the link.
- `doc/code-highlighting.md:73` - still documents the old `printpdf` implementation (`printpdf = { version = "0.7", features = ["svg"] }`, lines 68, 190, 199); update to the current krilla/usvg pipeline or delete.
- `README.md:153` - the PDF support table marks `image` as "planned", but SVG images are rendered (`src/exporter/pdf.rs:265`); mark it as supported (SVG only).
- `src/config.rs:327` - doc comment says the explicit path comes from `--theme`; the flag is `--config` (`src/main.rs:47`).
- `scripts/yaml2pdf.sh:6` - unreferenced leftover wrapper (rsconstruct calls `rsslide generate` directly, `rsconstruct.toml:1-8`) that shells out via `cargo run`; delete it.
