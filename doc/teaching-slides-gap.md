# Moving teaching-slides from marp to rsslide: what is missing

Measured 2026-10-11 against all 1,691 marp decks in `~/git/teaching-slides`
(`marp/courses/` and `marp/lectures/`), compared with the marp PDFs that
repo's own build produces (`out/chapters/courses/`, `_site/marp/lectures/`).

## Summary

| Result | Decks |
|---|---|
| Import and render | 1,436 (85%) |
| Render fails | 244 |
| Import fails | 11 |

Rendered decks keep their text: compared word by word with marp's PDFs,
0.02% of words are missing, all of them words split across lines.

Done since the first measurement:

- Importer YAML for code starting indented (58 decks failed to parse).
- Source order: a slide is a `body` of elements in the order written, so
  labels stay with their lists and text and code interleave.
- Numbered lists (`1.`, auto-numbered; i., a. when nested) and nested
  lists (disc, ring, square), measured against marp's render.
- Every table and code block of a slide is kept.
- Content above and below an image.
- Line breaks after in-word hyphens (`AI-generated`), as Chrome does.
- SVG size: pictures are sized and placed as marp does (content width,
  flowing under the heading; checked within 1 pt against marp's PDF). A
  diagram's empty canvas may run off the page; its drawing may not.

## Blocks the build

### 1. Content that does not fit: 724 slides

- **703 are cut off in marp too**, some by a whole paragraph, some by the
  descenders of the last line, 13 by a diagram drawn past the page (a
  two-line title pushing a full-height diagram down, two diagrams on one
  slide, a title SVG drawn to y=680, and
  `networking-basics/04_dns_deep_dive` slide 25, whose diagram text is
  broken in marp too). Chromium clips at the page edge and leaves
  the clipped text out of the PDF, so the cut goes unnoticed today. rsslide
  reports them as errors; each must be split in teaching-slides by hand
  (its CLAUDE.md forbids scripted content edits). This is the largest
  single piece of migration work.
- **21 fit in marp but not in rsslide.** 15 are math slides (raw LaTeX is
  longer than typeset math; see Math below). 6 are other small differences
  to investigate:
  `architecture-patterns/13_data_processing_engines` slide 19,
  `advanced-spark-with-python/07_performance_tuning` slide 17,
  `elasticsearch-administration/08_performance_tuning` slide 12 (code),
  `cyber-attacks-and-vectors/26_ransomware` slide 11 and `21_form_jacking`
  slide 4, `lectures/devops/devops-slides` slide 24.

### 2. `<div class="columns">` with richer content: 11 decks

The importer accepts only `### header` lines and bullets inside a columns
div. These decks also put bold labels (`**Input Chain:**`), headings inside
bare `<div>` columns, and other blocks there. Two more classes appear once
each: `<div class="content">` and `<div class="footer">`.

### 3. Raster images: 9 decks

`jpg/...` portraits (e.g. `jpg/courses/git/git/linus_torvalds.jpg`). rsslide
renders SVG only.

### 4. Single cases

- Empty fenced code block, 2 decks (`technical-writing/06`, `08`): marp shows
  an empty box; rsslide rejects it. Decide: render empty, or fix the decks.
- Table too wide even with wrapped cells: 3 slides.
- Two images on one slide: 1 slide.

## Renders, but not like marp

| Gap | Slides / decks | What rsslide does today |
|---|---|---|
| Code that does not fit | most of the 164 code-overflow slides | Shrinks the code until it fits, sometimes to a barely readable size; marp clips it. These are in the 690 above: split the slides |
| Blockquotes (`>`) | 56 decks | Text with a literal `>` |
| Math (`$…$`, `$$…$$`) | 53 decks | Raw LaTeX source |
| Background images (`![bg left]`, `![bg right:40% 80%]`) | 13 decks | Drawn as an ordinary image |
| Task lists (`- [ ]`) | 8 decks | Literal `[ ]` |
| Footnotes, inline `<b>`/`<i>` | 1-2 decks each | Literal |

### Syntax highlighting: 13% of code blocks are plain

1,890 of 14,129 fenced blocks use a language syntect's default syntaxes do
not cover. Some are deliberate plain text that marp shows plain too
(`misc` 391, `output` 158, `tree` 116, `config` 76, `template` 22,
`diagram` 9). The real gaps, by blocks: `hcl` 255, `cmake` 160, `gas` 92,
`dockerfile` 80, `ini` 62, `http` 53, `nasm` 38, `dts` 37, `typescript` 34,
`toml` 32, `graphql` 27, `console` 27, `vim` 23, `csharp` 23, `asm` 17,
`kotlin` 16, `jinja` 16, `nginx` 15, `promql` 14, `gitignore` 14, `conf` 13,
`protobuf` 12, `apache` 9, `ld` 7. Check each against what marp's
highlight.js actually highlights before adding syntaxes.

## Not a gap

- Front matter (`tags`, `level`, `category`, `audience`, `duration_hours`)
  is metadata for the site; marp ignores it too.
- teaching-slides does not paginate, so page numbers are not needed.
- `<T>`, `<service>` and similar in prose are not HTML; marp shows them as
  text, and so does rsslide.

## Suggested order

1. Richer columns content and blockquotes (11 + 56 decks).
2. Syntax definitions for the languages above.
3. Raster and background images (9 + 13 decks).
4. Math (53 decks): needs a TeX-to-SVG step; the hardest item.
5. In teaching-slides: split the 703 slides marp clips today, then replace
   the `marp.courses` / `marp.lectures` processors in its `rsconstruct.toml`
   with rsslide import + generate (the per-chapter PDFs still feed its
   `ipdfunite` merge).

## How to measure again

From the teaching-slides root, run `rsslide import` and
`rsslide generate --format=pdf` on every `marp/**/*.md`, and compare with
the marp PDF of the same deck. To tell rsslide-only overflows from slides
marp clips too, check whether the end of each failing slide's last source
line appears in marp's text for that page, and whether marp's last line
ends past the 540 pt page edge.
