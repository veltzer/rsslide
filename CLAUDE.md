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
