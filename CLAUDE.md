# Project conventions

## Strictness

Always be strict. Always fail on any minor issue.

- Never silently skip, ignore, or paper over problems (missing files, missing
  SVGs, missing fields, unexpected input, lint warnings, etc.).
- Prefer hard errors with a clear message over best-effort fallbacks.
- Warnings count as failures unless the user has explicitly opted in.
- Do not add config overrides, `#[allow(...)]`, `# yamllint disable ...`, or
  similar suppressions to make problems go away. If a check flags something,
  fix the underlying cause.

## No absolute coordinates in layout

Layout code must never use hardcoded absolute point/pixel values. All
positions, margins, font sizes, and offsets must be expressed as ratios of
the page size (or other relative units).

- Reject constants like `x = 40.0` or `font_size = 36.0` in layout code.
- Derive every coordinate from `page_width` / `page_height` or a named ratio.
- Changing the page size must be a one-line change — every other element
  must scale automatically.
