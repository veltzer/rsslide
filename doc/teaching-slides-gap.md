# Moving teaching-slides from marp to rsslide: what is missing

Measured 2026-10-11 against all 1,691 marp decks in `~/git/teaching-slides`
(`marp/courses/` and `marp/lectures/`), compared with the marp PDFs that
repo's own build produces (`out/chapters/courses/`, `_site/marp/lectures/`).

## Summary

| Result | Decks |
|---|---|
| Import and render | 1,457 (86%) |
| Render fails | 234 |
| Import fails | 0 |

Every deck now imports. Of the 234 that fail to render, 233 fail only on
content that does not fit (section 1), all of it cut off in marp too. The
other one has a table too wide for the slide. On the rsslide side the
migration is done but for that table; what is left is splitting slides in
teaching-slides.

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
- Code fences as CommonMark defines them: a fence closes only on a run of
  the same character at least as long, so ````` ```` ````` blocks can show
  ```` ``` ```` examples (3 decks; one was silently cut short before), and
  an unclosed fence is an error. The two technical-writing decks that
  failed as "empty code blocks" had their inner example's closing fence
  mistyped as ```` ```misc ````; fixed in teaching-slides.
- HTML character references (`&theta;`, `&#8321;`, `&#x2192;`) are
  decoded in text, as CommonMark and marp do (not in code): 3,434
  references in 202 decks were printed literally, which made lines longer
  and pushed slides off the page.
- Fallback fonts for the characters Noto Sans lacks, in the order
  Chromium uses when it renders marp (Liberation Sans in the same style,
  Noto Sans Math, DejaVu Sans, FreeSans, Noto Sans CJK; DejaVu Sans Mono
  and FreeMono for code): arrows, math operators, check marks and CJK in
  327 decks had been drawn as blank boxes. A character no configured font
  has is now an error; default-ignorable characters (U+200B and the like)
  need no glyph.
- A word too long for a line of its own breaks between characters, as
  marp's `word-wrap: break-word` breaks it (2 slides: a URL and a Spark
  config key used to run off the side of the page without an error), and
  a line may break after a hyphen inside a word in inline code too; both
  checked against marp's line breaks. Anything still wider than its line
  is an error, and a table too wide for the slide stays an error with
  `--warn-overflow`.
- Split backgrounds (`![bg left]`, `![bg right:40% 80%]`), with JPEG and
  PNG pictures: the 9 portrait decks and the 2 logo slides; text placed
  within 0.1 pt of marp's.
- SVG size: pictures are sized and placed as marp does (content width,
  flowing under the heading; checked within 1 pt against marp's PDF). A
  diagram's empty canvas may run off the page; its drawing may not.

## Blocks the build

### 1. Content that does not fit: 709 slides

**All 709 are cut off in marp too**: some by a whole paragraph, some by the
descenders of the last line, some by the bottom edge of a table or code
box (the last row's padding and border; the text itself ends above the
edge), 13 by a diagram drawn past the page (a two-line title pushing a
full-height diagram down, two diagrams on one slide, a title SVG drawn to
y=680, and `networking-basics/04_dns_deep_dive` slide 25, whose diagram
text is broken in marp too). Chromium clips at the page edge and leaves the
clipped text out of the PDF, so the cut goes unnoticed today. rsslide
reports them as errors; each must be split in teaching-slides by hand (its
CLAUDE.md forbids scripted content edits). This is the remaining migration
work. Until they are split, `rsslide generate --warn-overflow` renders
every deck but the wide-table one (1,690 of 1,691), each cut slide
reported as a warning and drawn cut off as marp draws it.

No slide overflows in rsslide alone any more. The suspects that did were
rsslide bugs, now fixed: undecoded character references (the "math
slides" of earlier measurements), and code shrunk away when the text only
ran into the bottom padding (`elasticsearch-administration/08` slide 12).

### 2. A table too wide for the slide: 1 slide

Even with every cell wrapped.

## Renders, but not like marp

| Gap | Slides / decks | What rsslide does today |
|---|---|---|
| Code that does not fit | most of the 162 code-overflow slides | Shrinks the code until it fits, sometimes to a barely readable size; marp clips it. These are in the 709 above: split the slides |
| Emoji in text | about 20 decks | Noto Color Emoji glyphs; marp draws twemoji pictures (same emoji, different artwork) |

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

- Math. marp typesets `$…$` and `$$…$$` with MathJax, but the corpus has
  no math: no deck uses a TeX command, and run through marp-core's
  renderer only one deck produces any math at all, by accident:
  `assembly-programming-using-gas/20_arch_comparison` writes MIPS registers
  in prose (`($0-$31)`, a table cell `add $t0,$t1,$t2`), and marp garbles
  them into math. rsslide shows that text as written, which is what the
  author meant; the deck should put those in code spans. The earlier "53
  decks" counted `$` pairs (prices, shell variables) that marp leaves
  alone, and the math-looking slides were the character references above.
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

1. In teaching-slides: split the 709 slides marp clips today, narrow the
   one wide table, and put the MIPS registers of
   `assembly-programming-using-gas/20_arch_comparison` in code spans.
2. Optionally, draw emoji as twemoji pictures, as marp does.
3. Replace the `marp.courses` / `marp.lectures` processors in
   teaching-slides' `rsconstruct.toml` with rsslide import + generate (the
   per-chapter PDFs still feed its `ipdfunite` merge).

## How to measure again

From the teaching-slides root, run `rsslide import` and
`rsslide generate --format=pdf` on every `marp/**/*.md`, and compare with
the marp PDF of the same deck. To tell rsslide-only overflows from slides
marp clips too, render the deck with a config of twice the page height
(`slide.height_mm` doubled, `slide.padding_y_ratio` halved,
`slide.valign = "top"`: same width, so the same text metrics) and compare
the slide's text lines with marp's page: marp clips when its page is
missing lines, when its last line's box ends below 540 pt, or when a box
or picture reaches the bottom pixel row.
