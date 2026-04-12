# rsslide vs Marp: Output Comparison

## Goal

Compare the PDF output of rsslide against Marp (the established Markdown-based slide tool) to identify visual quality gaps, rendering differences, and areas for improvement.

## Comparison Approaches

### 1. Side-by-Side Visual Inspection

Create equivalent slides in both formats (rsslide YAML and Marp Markdown), render both to PDF, and visually compare.

- Produce a set of "canonical" test slides covering: title slides, bullet lists, code blocks, multi-column layouts, alignment options
- Render each with both tools
- Open side-by-side and note differences in typography, spacing, color, and layout

### 2. Pixel-Level Diff

Render both PDFs to images (e.g. via `pdftoppm`) at the same DPI and compute a pixel diff.

- Highlights exact regions where output diverges
- Can be automated in CI to track visual regressions
- Tools: ImageMagick `compare`, `perceptualdiff`, or Python `pixelmatch`

### 3. Metric-Based Comparison

Extract measurable properties from both PDFs and compare numerically:

- **Page dimensions** — are slide sizes identical?
- **Font sizes and families** — compare with `pdffonts` or `pdftotext`
- **Bounding boxes** — extract text/image positions with `pdftohtml -xml` or `poppler` bindings
- **File size** — compare PDF bloat
- **Rendering time** — wall-clock time to produce the same slide deck

### 4. Feature Matrix

Document which features each tool supports and where rsslide is ahead or behind:

| Feature | rsslide | Marp |
|---------|---------|------|
| Bullet lists | yes | yes |
| Multi-column bullets | yes | via HTML/CSS |
| Syntax-highlighted code | yes | yes |
| SVG diagrams | yes | yes |
| Themes | limited | rich |
| Speaker notes | no | yes |
| HTML output | no | yes |
| PPTX output | no | yes |
| Custom CSS | no | yes |
| Math/LaTeX | no | yes (KaTeX) |
| Auto-scaling text | no | yes |
| Background images | no | yes |
| Transitions | no | no (PDF) |

### 5. Equivalent Test Deck

Create a single "comparison deck" that exercises all shared features. Maintain it in two formats:

- `comparison/slides.yaml` — rsslide input
- `comparison/slides.md` — Marp input

Both should produce visually similar output. Differences highlight where rsslide rendering needs work.

## Suggested Priority

1. Start with the **feature matrix** — quick to produce, immediately useful for roadmap planning
2. Build the **equivalent test deck** — gives concrete visual targets
3. Add **pixel-level diff** tooling — catches regressions as rsslide evolves
4. Track **metrics** over time — file size, render speed, text positioning accuracy

## Open Questions

- Should we aim for pixel-identical output or just "comparable quality"?
- Which Marp theme should be the baseline (default, gaia, uncover)?
- Do we want automated comparison in CI or manual spot-checks?
