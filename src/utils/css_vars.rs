//! Pre-resolve `var(--name)` references in an SVG string.
//!
//! usvg 0.45 (the version krilla-svg 0.7 depends on) does not resolve CSS
//! custom properties: an element with `stroke="var(--foo)"` silently drops
//! the stroke. This pass reads `--name: value;` declarations from any
//! `<style>` block and substitutes every `var(--name)` / `var(--name, fb)`
//! occurrence with the literal value.
//!
//! Scope is limited to the cases rsslide sees in practice:
//! - declarations inside `<style>` (any selector — `:root`, `svg`, `*`);
//! - usage in attribute values or inline styles.
//!
//! Not supported: cascade by element, @supports, calc(), nested vars.
//! Those aren't worth implementing here; the inputs we care about use a
//! single flat palette.

use std::collections::HashMap;

/// Resolve CSS variables in the given SVG source. Returns the SVG with
/// every recognised `var(--name)` replaced by its literal value.
pub fn flatten(svg: &str) -> String {
    let decls = collect_declarations(svg);
    // Still run the replacement even with no declarations — `var(--x, fb)`
    // can resolve to its fallback.
    if decls.is_empty() && !svg.contains("var(") {
        return svg.to_string();
    }
    replace_vars(svg, &decls)
}

fn collect_declarations(svg: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut cursor = 0;
    while let Some(open_rel) = svg[cursor..].find("<style") {
        let open = cursor + open_rel;
        let Some(gt) = svg[open..].find('>') else { break };
        let body_start = open + gt + 1;
        let Some(close_rel) = svg[body_start..].find("</style>") else { break };
        let body_end = body_start + close_rel;
        for (name, value) in parse_decls(&svg[body_start..body_end]) {
            out.entry(name).or_insert(value);
        }
        cursor = body_end + "</style>".len();
    }
    out
}

fn parse_decls(css: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = css.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
            let name_start = i;
            while i < bytes.len() && is_ident_byte(bytes[i]) {
                i += 1;
            }
            let name = &css[name_start..i];
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b':' {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                let value_start = i;
                while i < bytes.len() && bytes[i] != b';' && bytes[i] != b'}' {
                    i += 1;
                }
                let value = css[value_start..i].trim().to_string();
                if !value.is_empty() {
                    out.push((name.to_string(), value));
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

fn replace_vars(svg: &str, decls: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(svg.len());
    let mut i = 0;
    let bytes = svg.as_bytes();
    while i < bytes.len() {
        if svg[i..].starts_with("var(") {
            let close = match find_matching_close_paren(&svg[i + 4..]) {
                Some(n) => n,
                None => {
                    out.push_str(&svg[i..]);
                    return out;
                }
            };
            let inner = &svg[i + 4..i + 4 + close];
            if let Some(replacement) = resolve_var(inner, decls) {
                out.push_str(&replacement);
            } else {
                out.push_str(&svg[i..i + 4 + close + 1]);
            }
            i += 4 + close + 1;
        } else {
            let ch_end = next_char_boundary(bytes, i);
            out.push_str(&svg[i..ch_end]);
            i = ch_end;
        }
    }
    out
}

fn next_char_boundary(bytes: &[u8], i: usize) -> usize {
    let mut j = i + 1;
    while j < bytes.len() && (bytes[j] & 0xC0) == 0x80 {
        j += 1;
    }
    j
}

fn find_matching_close_paren(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

fn resolve_var(inner: &str, decls: &HashMap<String, String>) -> Option<String> {
    let inner = inner.trim();
    let (name, fallback) = match inner.find(',') {
        Some(idx) => (inner[..idx].trim(), Some(inner[idx + 1..].trim())),
        None => (inner, None),
    };
    if let Some(v) = decls.get(name) {
        return Some(v.clone());
    }
    fallback.map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flattens_basic_var() {
        let svg = r##"<svg><style>:root { --c: #ff0000; }</style><rect fill="var(--c)"/></svg>"##;
        let out = flatten(svg);
        assert!(out.contains(r##"fill="#ff0000""##));
        assert!(!out.contains("var(--c)"));
    }

    #[test]
    fn falls_back_when_undefined() {
        let svg = r##"<svg><rect fill="var(--missing, #00ff00)"/></svg>"##;
        let out = flatten(svg);
        assert!(out.contains(r##"fill="#00ff00""##));
    }

    #[test]
    fn preserves_unknown_without_fallback() {
        let svg = r#"<svg><rect fill="var(--missing)"/></svg>"#;
        let out = flatten(svg);
        assert!(out.contains("var(--missing)"));
    }

    #[test]
    fn handles_multiple_style_blocks() {
        let svg = r#"<svg>
            <style>:root { --a: red; }</style>
            <style>:root { --b: blue; }</style>
            <rect fill="var(--a)"/>
            <rect fill="var(--b)"/>
        </svg>"#;
        let out = flatten(svg);
        assert!(out.contains(r#"fill="red""#));
        assert!(out.contains(r#"fill="blue""#));
    }

    #[test]
    fn first_declaration_wins() {
        let svg = r#"<svg>
            <style>:root { --a: red; }</style>
            <style>:root { --a: green; }</style>
            <rect fill="var(--a)"/>
        </svg>"#;
        let out = flatten(svg);
        assert!(out.contains(r#"fill="red""#));
    }

    #[test]
    fn noop_when_no_style_block() {
        let svg = r##"<svg><rect fill="#abc"/></svg>"##;
        assert_eq!(flatten(svg), svg);
    }

    #[test]
    fn resolves_many_vars_in_one_pass() {
        let svg = r##"<svg>
            <style>:root {
                --bg: #fff;
                --fg: #000;
            }</style>
            <rect fill="var(--bg)" stroke="var(--fg)"/>
        </svg>"##;
        let out = flatten(svg);
        assert!(out.contains(r##"fill="#fff""##));
        assert!(out.contains(r##"stroke="#000""##));
    }
}
