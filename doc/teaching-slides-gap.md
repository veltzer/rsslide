# Moving teaching-slides from marp to rsslide: what is missing

Measured 2026-10-11 with rsslide 0.1.9 against all 1,691 marp decks in
`~/git/teaching-slides` (`marp/courses/` and `marp/lectures/`), compared with
the marp PDFs that repo's own build produces (`out/chapters/courses/`,
`_site/marp/lectures/`).

## Summary

| Result | Decks |
|---|---|
| Import and render | 1,404 (83%) |
| Render fails | 276 |
| Import fails | 11 |

The decks that do render keep their text: compared word by word with marp's
PDFs, 0.02% of words are missing, and those are words split across lines,
not dropped content. What they lose is structure (list numbering and
nesting, element order, blockquotes, math), listed under "Renders, but not
like marp" below.

## Blocks the build

### 1. Invalid YAML from the importer: 58 decks

A fenced code block whose first line is indented deeper than a later line
(a snippet cut from the middle of a function) is emitted as a YAML block
scalar without an indentation indicator. YAML takes the indentation from the
first line, so the less-indented line ends the scalar and the file does not
parse. Fix: emit `|2-` (an explicit indentation indicator) for code, and for
`svg:` blocks, when the first line starts with whitespace. Small importer fix.

### 2. Content that does not fit: 629 slides

- **548 are also cut off in marp.** Chromium clips them at the page edge and
  leaves the clipped text out of the PDF, so the cut goes unnoticed today.
  rsslide reports them as errors; each must be split in teaching-slides by
  hand (its CLAUDE.md forbids scripted content edits). This is the largest
  single piece of migration work.
- **81 fit in marp but not in rsslide.** Of these, 51 overshoot by at most
  5% and 28 by more; nearly all are slides mixing bold labels (`**Pros**:`),
  lists and prose, which the importer reorders and merges (see "element
  order" below). 2 are code blocks with no room left.

### 3. `<div class="columns">` with richer content: 11 decks

The importer accepts only `### header` lines and bullets inside a columns
div. These decks also put bold labels (`**Input Chain:**`), headings inside
bare `<div>` columns, and other blocks there. Two more classes appear once
each: `<div class="content">` and `<div class="footer">`.

### 4. Raster images: 9 decks

`jpg/...` portraits (e.g. `jpg/courses/git/git/linus_torvalds.jpg`). rsslide
renders SVG only.

### 5. Single cases

- Empty fenced code block, 2 decks (`technical-writing/06`, `08`): marp shows
  an empty box; rsslide rejects it. Decide: render empty, or fix the decks.
- Table too wide even with wrapped cells, 1 deck
  (`lectures/architecting/idempotency.md`).

## Renders, but not like marp

Ordered by how many decks are affected.

| Gap | Decks | What rsslide does today |
|---|---|---|
| Ordered lists (`1.`) | 537 | Kept as text lines, so every item shows "1." (the corpus writes `1. 1. 1.` and relies on auto-numbering) |
| Element order | most | Fixed order (title, subtitles, text, bullets/columns, table, code) instead of source order: a label is separated from its list or code, text and code do not interleave |
| Several code blocks on a slide | 276 | All kept since 0.1.8, but stacked after all text and shrunk together, sometimes to a barely readable size |
| Nested lists (4-space indent) | 260 | Flattened to one level |
| Blockquotes (`>`) | 56 | Shown as text with a literal `>` |
| Math (`$…$`, `$$…$$`) | 53 | Raw LaTeX source |
| Nested ordered lists | 18 | As ordered lists, flattened |
| Background images (`![bg left]`, `![bg right:40% 80%]`) | 13 | Ignored as a layout directive: drawn as an ordinary image |
| Task lists (`- [ ]`) | 8 | Literal `[ ]` |
| Two tables on a slide | 3 | The second becomes text |
| Footnotes, inline `<b>`/`<i>` | 1-2 each | Literal |

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

1. Importer YAML indentation fix (58 decks build).
2. Ordered and nested lists in the model (537 + 260 decks).
3. Source-order layout: one list of elements per slide instead of fixed
   fields. Fixes element order, multiple tables, label/list pairing, and
   probably most of the 81 rsslide-only overflows.
4. Richer columns content and blockquotes (11 + 56 decks).
5. Syntax definitions for the languages above.
6. Raster and background images (9 + 13 decks).
7. Math (53 decks): needs a TeX-to-SVG step; the hardest item.
8. In teaching-slides: split the 548 slides marp clips today, then replace
   the `marp.courses` / `marp.lectures` processors in its `rsconstruct.toml`
   with rsslide import + generate (the per-chapter PDFs still feed its
   `ipdfunite` merge).
