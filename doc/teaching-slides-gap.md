# Moving teaching-slides from marp to rsslide: what is missing

Measured 2026-10-11 against all 1,691 marp decks in `~/git/teaching-slides`
(`marp/courses/` and `marp/lectures/`), compared with the marp PDFs that
repo's own build produces (`out/chapters/courses/`, `_site/marp/lectures/`).

## Summary

| Result | Decks |
|---|---|
| Import and render | 1,453 (86%) |
| Render fails | 238 |
| Import fails | 0 |

Every deck now imports. Of the 238 that fail to render, 235 fail only on
content that does not fit (section 1). The other 3 hit 2 empty code
blocks and 1 too-wide table; 1 of them overflows as well.

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
- `<div class="columns">` holds any content (headings, bold labels, code,
  bare `<div>` columns); the 11 decks that failed to import now do.
- Blockquotes (`>`, nested `>>`, lists inside), drawn as marp does: grey
  text behind a bar (checked against marp's render).
- Syntax highlighting decided as marp decides it; see below.
- Split backgrounds (`![bg left]`, `![bg right:40% 80%]`), with JPEG and
  PNG pictures: the 9 portrait decks and the 2 logo slides; text placed
  within 0.1 pt of marp's.
- SVG size: pictures are sized and placed as marp does (content width,
  flowing under the heading; checked within 1 pt against marp's PDF). A
  diagram's empty canvas may run off the page; its drawing may not.

## Blocks the build

### 1. Content that does not fit: 727 slides

Classified when the total was 724 (before the columns decks imported; the
5 slides they added are not classified):

- **708 are cut off in marp too**, some by a whole paragraph, some by the
  descenders of the last line, some by the bottom edge of a table or code
  box (the last row's padding and border; the text itself ends above the
  edge), 13 by a diagram drawn past the page (a two-line title pushing a
  full-height diagram down, two diagrams on one slide, a title SVG drawn
  to y=680, and `networking-basics/04_dns_deep_dive` slide 25, whose
  diagram text is broken in marp too). Chromium clips at the page edge and
  leaves the clipped text out of the PDF, so the cut goes unnoticed today.
  rsslide reports them as errors; each must be split in teaching-slides by
  hand (its CLAUDE.md forbids scripted content edits). This is the largest
  single piece of migration work.
- **15 fit in marp but not in rsslide**, all math slides (raw LaTeX is
  longer than typeset math; see Math below).
- 1 was an rsslide bug, now fixed: a slide whose text ran into the bottom
  padding had its code shrunk away ("no room left for the code"), although
  it fit the page as it does in marp
  (`elasticsearch-administration/08_performance_tuning` slide 12). Code now
  shrinks for height only when the content would run off the page.

How the 6 non-math suspects were checked: each slide was rendered on a
page twice as tall (same width, so the same text metrics) and its line
positions compared with marp's page. Three lay out within 0.3 pt of marp
and only a table or code box's bottom edge crosses 540 pt
(`advanced-spark-with-python/07_performance_tuning` 17,
`cyber-attacks-and-vectors/26_ransomware` 11 and `21_form_jacking` 4);
in two, marp's PDF is missing the last lines
(`architecture-patterns/13_data_processing_engines` 19,
`lectures/devops/devops-slides` 24).

### 2. Single cases

- Empty fenced code block, 2 decks (`technical-writing/06`, `08`): marp shows
  an empty box; rsslide rejects it. Decide: render empty, or fix the decks.
- Table too wide even with wrapped cells: 1 slide.

## Renders, but not like marp

| Gap | Slides / decks | What rsslide does today |
|---|---|---|
| Code that does not fit | most of the 162 code-overflow slides | Shrinks the code until it fits, sometimes to a barely readable size; marp clips it. These are in the 727 above: split the slides |
| Math (`$…$`, `$$…$$`) | 53 decks | Raw LaTeX source |

### Syntax highlighting

Done. A block is coloured exactly when marp colours it: when highlight.js
knows its language (`doc/code-highlighting.md`). Every corpus language
highlight.js knows now has a syntax, from bat's collection (two-face) or
bundled in `assets/syntaxes/` (`dts`, `arm`, `mips`); `console` is a shell
session as in highlight.js. Languages highlight.js does not know stay plain
as in marp: `misc` 390, `hcl` 255, `output` 158, `tree` 116, `gas` 92,
`config` 75, `nasm` 38, `template` 22, `asm` 17, `promql` 14, `gitignore` 14,
`conf` 12, and a few more. Token colours still differ in places where a
bat grammar scopes a token differently from highlight.js.

## Not a gap

- Task lists (`- [ ]`, 8 decks): marp has no task-list plugin and shows
  `[ ] item` literally, as rsslide does (checked against marp's text).
- Footnotes and inline `<b>`/`<i>`: every occurrence in the corpus is
  inside code or a regex, where both show it literally; marp has no
  footnote plugin either.
- Front matter (`tags`, `level`, `category`, `audience`, `duration_hours`)
  is metadata for the site; marp ignores it too.
- teaching-slides does not paginate, so page numbers are not needed.
- `<T>`, `<service>` and similar in prose are not HTML; marp shows them as
  text, and so does rsslide.

## Suggested order

1. The empty-code-block decision (2 decks).
2. Math (53 decks): needs a TeX-to-SVG step; the hardest item.
3. In teaching-slides: split the 703 slides marp clips today, then replace
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
