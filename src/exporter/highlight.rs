//! Syntax highlighting of code blocks, deciding what to colour the way
//! marp does.
//!
//! marp colours a fenced block with highlight.js, and a fence language
//! highlight.js does not know is plain text there. rsslide colours with
//! syntect, so a fence language first goes through highlight.js's own
//! language table (`assets/syntaxes/highlightjs-languages.txt`): unknown
//! there means plain text, as in marp; known there means rsslide must have
//! a syntax for it, and a missing one is an error rather than a quietly
//! plain block.
//!
//! The syntaxes are two-face's (bat's collection, a superset of syntect's
//! defaults) plus the few in `assets/syntaxes/` that two-face lacks under
//! fancy-regex. highlight.js's shell sessions (`console`) are not a syntax:
//! a prompt line is the prompt and then bash, any other line is output.

use crate::config::Color;
use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Highlighter as ThemeHighlighter, Theme};
use syntect::parsing::{Scope, SyntaxReference, SyntaxSet, SyntaxSetBuilder};
use syntect::util::LinesWithEndings;

const HLJS_LANGUAGES: &str = include_str!("../../assets/syntaxes/highlightjs-languages.txt");

/// The syntaxes rsslide bundles itself: highlight.js languages that
/// two-face has no fancy-regex syntax for.
const OWN_SYNTAXES: [&str; 3] = [
    include_str!("../../assets/syntaxes/dts.sublime-syntax"),
    include_str!("../../assets/syntaxes/armasm.sublime-syntax"),
    include_str!("../../assets/syntaxes/mipsasm.sublime-syntax"),
];

/// highlight.js languages whose syntax a token lookup does not find,
/// by the highlight.js name and the syntax name.
const SYNTAX_NAMES: [(&str, &str); 4] = [
    ("apache", "Apache Conf"),
    ("armasm", "ARM Assembly"),
    ("dts", "Device Tree"),
    ("mipsasm", "MIPS Assembly"),
];

/// highlight.js's shell-session language, highlighted in code.
const SESSION: &str = "shell";

/// One run of a line in one colour.
pub type Run = (Color, String);

pub struct Highlighter {
    extra: SyntaxSet,
    own: SyntaxSet,
    /// highlight.js language names and aliases, lower case, to the name.
    hljs: HashMap<String, String>,
}

/// How a block is coloured.
enum Plan<'a> {
    Plain,
    Syntax(&'a SyntaxSet, &'a SyntaxReference),
    /// A shell session, its commands highlighted with this (bash) syntax.
    Session(&'a SyntaxSet, &'a SyntaxReference),
}

impl Highlighter {
    pub fn new() -> Result<Self> {
        let mut builder = SyntaxSetBuilder::new();
        for source in OWN_SYNTAXES {
            builder.add(crate::exporter::syntax_file::load(source)?);
        }
        Ok(Self {
            extra: two_face::syntax::extra_newlines(),
            own: builder.build(),
            hljs: parse_hljs_languages(HLJS_LANGUAGES)?,
        })
    }

    fn plan(&self, language: Option<&str>) -> Result<Plan<'_>> {
        let Some(language) = language else {
            return Ok(Plan::Plain);
        };
        // highlight.js looks languages up in lower case.
        let token = language.to_lowercase();
        let Some(name) = self.hljs.get(&token) else {
            return Ok(Plan::Plain);
        };
        if name == SESSION {
            let (set, bash) = self.syntax("bash")?;
            return Ok(Plan::Session(set, bash));
        }
        let (set, syntax) = self.syntax(&token).with_context(|| {
            format!("code language {language:?} (highlight.js language {name:?})")
        })?;
        Ok(Plan::Syntax(set, syntax))
    }

    /// The syntax for a highlight.js language `token`: by an explicit name,
    /// else by the token, the language's name or any of its aliases.
    fn syntax(&self, token: &str) -> Result<(&SyntaxSet, &SyntaxReference)> {
        let name = &self.hljs[token];
        if let Some((_, syntax_name)) = SYNTAX_NAMES.iter().find(|(n, _)| n == name) {
            return [&self.extra, &self.own]
                .into_iter()
                .find_map(|set| set.find_syntax_by_name(syntax_name).map(|s| (set, s)))
                .with_context(|| format!("no bundled syntax named {syntax_name:?}"));
        }
        let aliases = self
            .hljs
            .iter()
            .filter(|(_, n)| *n == name)
            .map(|(alias, _)| alias.as_str());
        std::iter::once(token)
            .chain(std::iter::once(name.as_str()))
            .chain(aliases)
            .find_map(|t| self.extra.find_syntax_by_token(t))
            .map(|s| (&self.extra, s))
            .with_context(|| {
                format!("marp highlights it, but rsslide bundles no syntax for {name:?}")
            })
    }

    /// Each line of `source` (newline removed) as runs of colour.
    pub fn highlight(
        &self,
        source: &str,
        language: Option<&str>,
        theme: &Theme,
    ) -> Result<Vec<Vec<Run>>> {
        let plan = self.plan(language)?;
        let colors = ThemeHighlighter::new(theme);
        let plain = to_color(colors.get_default().foreground);
        let lines: Vec<&str> = LinesWithEndings::from(source).collect();
        match plan {
            Plan::Plain => Ok(lines.iter().map(|l| plain_run(plain, l)).collect()),
            Plan::Syntax(set, syntax) => {
                let mut h = HighlightLines::new(syntax, theme);
                lines
                    .iter()
                    .enumerate()
                    .map(|(i, l)| syntax_runs(&mut h, set, i, l))
                    .collect()
            }
            Plan::Session(set, bash) => {
                // highlight.js's `meta.prompt` is themed as meta.
                let scope = Scope::new("constant.other.prompt.shell").expect("a valid scope");
                let prompt_color = to_color(colors.style_for_stack(&[scope]).foreground);
                let mut out = Vec::with_capacity(lines.len());
                let mut i = 0;
                while i < lines.len() {
                    let Some(len) = prompt_len(lines[i]) else {
                        out.push(plain_run(plain, lines[i]));
                        i += 1;
                        continue;
                    };
                    // The command runs to the first line that does not end
                    // in a backslash continuation (or is not empty).
                    let mut h = HighlightLines::new(bash, theme);
                    let (prompt, command) = lines[i].split_at(len);
                    let mut runs = vec![(prompt_color, prompt.to_string())];
                    runs.extend(syntax_runs(&mut h, set, i, command)?);
                    let mut continues = continues(command);
                    out.push(runs);
                    i += 1;
                    while continues && i < lines.len() {
                        out.push(syntax_runs(&mut h, set, i, lines[i])?);
                        continues = continues_line(lines[i]);
                        i += 1;
                    }
                }
                Ok(out)
            }
        }
    }
}

/// `highlightjs-languages.txt`: per line a name and its aliases.
fn parse_hljs_languages(text: &str) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let mut words = line.split_whitespace();
        let name = words
            .next()
            .context("empty line in the highlight.js language table")?;
        for word in std::iter::once(name).chain(words) {
            if map.insert(word.to_string(), name.to_string()).is_some() {
                bail!("{word:?} appears twice in the highlight.js language table");
            }
        }
    }
    Ok(map)
}

/// The length of the shell prompt starting `line`, as highlight.js's
/// `shell` grammar matches it: `^\s{0,3}[/~\w\d[\]()@-]*[>%$#][ ]?`.
fn prompt_len(line: &str) -> Option<usize> {
    let indent = line.len() - line.trim_start().len();
    if indent > 3 || line[..indent].contains(['\n', '\r']) {
        return None;
    }
    let rest = &line[indent..];
    let word = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || "_/~[]()@-".contains(c)))
        .unwrap_or(rest.len());
    let mark = rest[word..]
        .chars()
        .next()
        .filter(|c| ">%$#".contains(*c))?;
    let mut len = indent + word + mark.len_utf8();
    if line[len..].starts_with(' ') {
        len += 1;
    }
    Some(len)
}

/// Whether a command whose first line (after the prompt) is `command`
/// goes on to the next line: highlight.js ends it at the last character
/// of a line that is not a backslash, so an empty one carries on too.
fn continues(command: &str) -> bool {
    let command = command.trim_end();
    command.is_empty() || command.ends_with('\\')
}

fn continues_line(line: &str) -> bool {
    line.trim_end().ends_with('\\')
}

fn plain_run(color: Color, line: &str) -> Vec<Run> {
    let text = line.trim_end_matches(['\n', '\r']);
    if text.is_empty() {
        Vec::new()
    } else {
        vec![(color, text.to_string())]
    }
}

fn syntax_runs(h: &mut HighlightLines, set: &SyntaxSet, i: usize, line: &str) -> Result<Vec<Run>> {
    Ok(h.highlight_line(line, set)
        .with_context(|| format!("highlighting code line {}: {line:?}", i + 1))?
        .into_iter()
        .map(|(style, text)| {
            (
                to_color(style.foreground),
                text.trim_end_matches(['\n', '\r']).to_string(),
            )
        })
        .filter(|(_, text)| !text.is_empty())
        .collect())
}

fn to_color(c: syntect::highlighting::Color) -> Color {
    Color(c.r, c.g, c.b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use syntect::highlighting::ThemeSet;

    fn theme() -> Theme {
        ThemeSet::load_from_reader(&mut std::io::Cursor::new(include_str!(
            "../../assets/themes/github.tmTheme"
        )))
        .expect("the bundled theme")
    }

    fn colors(language: &str, source: &str) -> Vec<Vec<Run>> {
        Highlighter::new()
            .expect("highlighter")
            .highlight(source, Some(language), &theme())
            .expect("highlighted")
    }

    /// Every fence language of teaching-slides that highlight.js colours
    /// (measured 2026-10-11) has a syntax.
    #[test]
    fn every_corpus_language_marp_highlights_has_a_syntax() {
        let h = Highlighter::new().expect("highlighter");
        for language in [
            "bash",
            "python",
            "c",
            "cpp",
            "yaml",
            "rust",
            "sql",
            "json",
            "cmake",
            "javascript",
            "markdown",
            "java",
            "scala",
            "makefile",
            "tcl",
            "dockerfile",
            "ini",
            "http",
            "go",
            "dts",
            "typescript",
            "toml",
            "graphql",
            "console",
            "html",
            "vim",
            "csharp",
            "ruby",
            "xml",
            "kotlin",
            "jinja",
            "nginx",
            "groovy",
            "protobuf",
            "php",
            "properties",
            "lua",
            "apache",
            "jsx",
            "jsonc",
            "css",
            "arm",
            "diff",
            "mips",
        ] {
            let plan = h
                .plan(Some(language))
                .unwrap_or_else(|e| panic!("{language}: {e:#}"));
            assert!(!matches!(plan, Plan::Plain), "{language} is plain");
        }
    }

    #[test]
    fn languages_highlight_js_does_not_know_are_plain() {
        let h = Highlighter::new().expect("highlighter");
        for language in [
            "hcl",
            "misc",
            "output",
            "nasm",
            "gitignore",
            "conf",
            "promql",
        ] {
            assert!(
                matches!(h.plan(Some(language)), Ok(Plan::Plain)),
                "{language}"
            );
        }
        assert!(matches!(h.plan(None), Ok(Plan::Plain)));
    }

    #[test]
    fn language_lookup_ignores_case() {
        let h = Highlighter::new().expect("highlighter");
        assert!(matches!(h.plan(Some("Python")), Ok(Plan::Syntax(..))));
    }

    #[test]
    fn a_session_colours_prompts_and_commands_but_not_output() {
        let lines = colors(
            "console",
            "$ ls -la # list\ntotal 4\nuser@host:~$ echo hi\n",
        );
        assert_eq!(lines[0][0].1, "$ ");
        assert!(
            lines[0].len() > 2,
            "the command is highlighted: {:?}",
            lines[0]
        );
        assert_eq!(lines[1].len(), 1);
        // `:` cannot be part of a prompt, so this line is output.
        assert_eq!(lines[2].len(), 1);
    }

    #[test]
    fn a_continued_command_stays_bash() {
        let lines = colors("console", "$ echo a \\\n  # more\nout # not a comment\n");
        let comment = lines[1].last().expect("a run").0;
        assert_ne!(comment, lines[2][0].0);
        assert_eq!(lines[2].len(), 1);
    }

    #[test]
    fn prompts_match_highlight_js() {
        assert_eq!(prompt_len("$ ls"), Some(2));
        assert_eq!(prompt_len("   root@box# id"), Some(13));
        assert_eq!(prompt_len("PS> dir"), Some(4));
        assert_eq!(prompt_len("    $ four spaces"), None);
        assert_eq!(prompt_len("user@host:~$ x"), None);
        assert_eq!(prompt_len("plain output"), None);
    }

    #[test]
    fn bundled_syntaxes_colour_like_highlight_js() {
        let t = theme();
        let colors_of = |lang: &str, src: &str| colors(lang, src).concat();
        let color_of = |runs: &[Run], text: &str| {
            runs.iter()
                .find(|(_, t)| t.contains(text))
                .unwrap_or_else(|| panic!("{text:?} in {runs:?}"))
                .0
        };
        let plain = to_color(ThemeHighlighter::new(&t).get_default().foreground);
        let dts = colors_of(
            "dts",
            "/ {\n    reg = <0x10>; // c\n    node@1 {\n    };\n};\n",
        );
        assert_ne!(color_of(&dts, "reg"), plain);
        assert_ne!(color_of(&dts, "node@1"), plain);
        assert_ne!(color_of(&dts, "// c"), plain);
        let arm = colors_of("arm", "_start:\n    mov r0, #1 @ c\n");
        assert_ne!(color_of(&arm, "mov"), plain);
        assert_ne!(color_of(&arm, "r0"), plain);
        assert_ne!(color_of(&arm, "@ c"), plain);
        let mips = colors_of("mips", "main:\n    li $v0, 4 # c\n    syscall\n");
        assert_ne!(color_of(&mips, "syscall"), plain);
        assert_eq!(color_of(&mips, "li"), plain);
        assert_ne!(color_of(&mips, "# c"), plain);
    }

    #[test]
    fn the_language_table_has_no_duplicates() {
        assert!(parse_hljs_languages("a b\nc b\n").is_err());
    }
}
