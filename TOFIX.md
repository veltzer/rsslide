# TOFIX

Findings from a code scan on 2026-10-04.

## High

- `src/main.rs:43` - `process --format` defaults to `html`, but `src/main.rs:154` bails with "HTML export not yet implemented", so `rsslide process deck.yaml` with no flags always fails; default to `pdf` (the only implemented format) until HTML exists.

## Medium

- `README.md` (Output formats, Architecture) - says PDF is produced "via `printpdf`" and describes HTML and PPTX renderers that are stubs; the code uses `krilla`/`krilla-svg`/`usvg` (`Cargo.toml`); update the Output formats and Architecture sections.
- `Cargo.toml:18` - `serde_yaml` 0.9 is deprecated/unmaintained upstream; migrate to a maintained YAML crate (e.g. `serde_yml` / `serde_norway`).

## Low

- `scripts/yaml2pdf.sh:6` - unreferenced leftover wrapper (rsconstruct calls `rsslide generate` directly, `rsconstruct.toml:1-8`) that shells out via `cargo run`; delete it.
