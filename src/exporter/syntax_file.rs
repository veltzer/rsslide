//! A loader for the `.sublime-syntax` files in `assets/syntaxes/`.
//!
//! syntect's own loader needs its `yaml-load` feature, which pulls in the
//! unmaintained `yaml-rust` (RUSTSEC-2024-0320). The bundled syntaxes use a
//! small subset of the format, which this reads with serde_norway and builds
//! into syntect's structures the way its loader does: `__start`/`__main`
//! contexts around `main`, anonymous contexts for inline `push` lists, and
//! `$` rewritten to match before the newline that each line keeps. Any key
//! outside the subset is an error.

use anyhow::{Context as _, Result, bail};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use syntect::parsing::syntax_definition::{
    Context, ContextReference, MatchOperation, MatchPattern, Pattern,
};
use syntect::parsing::{Regex, Scope, SyntaxDefinition};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    name: String,
    scope: String,
    file_extensions: Vec<String>,
    contexts: BTreeMap<String, Vec<Entry>>,
}

/// One entry of a context. Each variant is its own struct so that unknown
/// keys are rejected (an untagged enum alone would ignore them).
#[derive(Deserialize)]
#[serde(untagged)]
enum Entry {
    MetaScope(MetaScope),
    Include(Include),
    Match(Match),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MetaScope {
    meta_scope: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Include {
    include: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Match {
    #[serde(rename = "match")]
    regex: String,
    scope: Option<String>,
    captures: Option<BTreeMap<usize, String>>,
    push: Option<Push>,
    #[serde(default)]
    pop: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Push {
    Named(String),
    Inline(Vec<Entry>),
}

/// The syntax in `source`, for lines that keep their newline.
pub fn load(source: &str) -> Result<SyntaxDefinition> {
    let file: File = serde_norway::from_str(source).context("parsing a .sublime-syntax file")?;
    let name = file.name.clone();
    let mut contexts = HashMap::new();
    for (context_name, entries) in &file.contexts {
        build_context(context_name, entries, &mut contexts)
            .with_context(|| format!("syntax {name:?}, context {context_name:?}"))?;
    }
    if !contexts.contains_key("main") {
        bail!("syntax {name:?} has no `main` context");
    }
    // What syntect's loader adds: `__start` carries the syntax's scope and
    // pushes `__main`, which includes `main`.
    let mut start = Context::new(false);
    start.meta_content_scope = vec![scope(&file.scope)?];
    start.patterns.push(Pattern::Match(MatchPattern::new(
        false,
        String::new(),
        Vec::new(),
        None,
        MatchOperation::Push(vec![named("__main")?]),
        None,
    )));
    let mut main = Context::new(true);
    main.patterns.push(Pattern::Include(named("main")?));
    contexts.insert("__start".into(), start);
    contexts.insert("__main".into(), main);
    Ok(SyntaxDefinition {
        name: file.name,
        file_extensions: file.file_extensions,
        scope: scope(&file.scope)?,
        first_line_match: None,
        hidden: false,
        variables: HashMap::new(),
        contexts,
    })
}

/// Build the context `name` from `entries` into `contexts`, with any
/// inline `push` lists as `#anon_<name>_<n>` contexts.
fn build_context(
    name: &str,
    entries: &[Entry],
    contexts: &mut HashMap<String, Context>,
) -> Result<()> {
    let mut context = Context::new(true);
    let mut anonymous = 0;
    for entry in entries {
        match entry {
            Entry::MetaScope(MetaScope { meta_scope }) => {
                context.meta_scope = vec![scope(meta_scope)?]
            }
            Entry::Include(Include { include }) => context
                .patterns
                .push(Pattern::Include(named(include.as_str())?)),
            Entry::Match(Match {
                regex,
                scope: match_scope,
                captures,
                push,
                pop,
            }) => {
                let operation = match (push, pop) {
                    (Some(_), true) => bail!("`push` and `pop` on one match: {regex:?}"),
                    (None, true) => MatchOperation::Pop,
                    (None, false) => MatchOperation::None,
                    (Some(Push::Named(target)), false) => {
                        MatchOperation::Push(vec![named(target.as_str())?])
                    }
                    (Some(Push::Inline(inner)), false) => {
                        let inner_name = format!("#anon_{name}_{anonymous}");
                        anonymous += 1;
                        build_context(&inner_name, inner, contexts)?;
                        MatchOperation::Push(vec![named(&inner_name)?])
                    }
                };
                let captures = captures
                    .as_ref()
                    .map(|c| {
                        c.iter()
                            .map(|(group, s)| Ok((*group, vec![scope(s)?])))
                            .collect::<Result<Vec<_>>>()
                    })
                    .transpose()?;
                let regex = for_newlines(regex);
                if let Some(e) = Regex::try_compile(&regex) {
                    bail!("regex {regex:?}: {e}");
                }
                context.patterns.push(Pattern::Match(MatchPattern::new(
                    false,
                    regex,
                    match_scope
                        .as_deref()
                        .map(scope)
                        .transpose()?
                        .into_iter()
                        .collect(),
                    captures,
                    operation,
                    None,
                )));
            }
        }
    }
    if contexts.insert(name.to_string(), context).is_some() {
        bail!("context {name:?} defined twice");
    }
    Ok(())
}

/// A reference to the context `name`. syntect marks the variant
/// non-exhaustive, so it cannot be built outside the crate; it is built
/// through its serde form instead (in serde_norway, the tagged `!Named name`).
fn named(name: &str) -> Result<ContextReference> {
    let value = serde_norway::Value::Tagged(Box::new(serde_norway::value::TaggedValue {
        tag: serde_norway::value::Tag::new("Named"),
        value: name.into(),
    }));
    serde_norway::from_value(value).context("building a context reference")
}

fn scope(s: &str) -> Result<Scope> {
    Scope::new(s).map_err(|e| anyhow::anyhow!("scope {s:?}: {e:?}"))
}

/// `regex` with every `$` anchor (not escaped, not in a character class)
/// made `(?m:$)`, so it matches before the newline a line keeps, as
/// syntect's loader does.
fn for_newlines(regex: &str) -> String {
    let mut out = String::with_capacity(regex.len());
    let mut chars = regex.chars();
    let mut in_class = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                out.push(c);
                out.extend(chars.next());
            }
            '[' if !in_class => {
                in_class = true;
                out.push(c);
                // A `]` first in the class is a literal.
                let mut rest = chars.clone();
                if rest.next() == Some(']') {
                    out.push(']');
                    chars = rest;
                }
            }
            ']' if in_class => {
                in_class = false;
                out.push(c);
            }
            '$' if !in_class => out.push_str("(?m:$)"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dollar_anchors_match_before_the_newline() {
        assert_eq!(for_newlines("a$"), "a(?m:$)");
        assert_eq!(for_newlines(r"\$x[$]"), r"\$x[$]");
        assert_eq!(for_newlines(r"[]$]$"), r"[]$](?m:$)");
    }

    #[test]
    fn unknown_keys_are_errors() {
        let base = "name: X\nscope: source.x\nfile_extensions: []\ncontexts:\n  main:\n";
        assert!(load(&format!("{base}    - match: a\n      scope: k\n")).is_ok());
        assert!(load(&format!("{base}    - match: a\n      set: other\n")).is_err());
        assert!(
            load(&format!(
                "{base}    - match: a\n      push: x\n      pop: true\n"
            ))
            .is_err()
        );
        assert!(
            load("name: X\nscope: source.x\nfile_extensions: []\ncontexts:\n  other: []\n")
                .is_err()
        );
        assert!(load(&format!("{base}    - match: '('\n")).is_err());
    }
}
