//! The credit line on a deck's first slide: when the deck was compiled, by
//! whom, and with which rsslide (whose author is a different person from
//! the one compiling, in general).

use crate::config::Credit;
use anyhow::{Context, Result, bail};
use std::process::Command;

/// The person compiling the deck.
#[derive(Debug, Clone, PartialEq)]
pub struct Compiler {
    pub name: String,
    pub email: String,
}

impl Compiler {
    /// `[credit] name` and `email` when set (both, or neither), otherwise
    /// git's `user.name` and `user.email`. Fails when neither source has
    /// them.
    pub fn resolve(cfg: &Credit) -> Result<Self> {
        match (&cfg.name, &cfg.email) {
            (Some(name), Some(email)) => Ok(Self {
                name: name.clone(),
                email: email.clone(),
            }),
            (None, None) => Ok(Self {
                name: git_config("user.name")?,
                email: git_config("user.email")?,
            }),
            _ => bail!("[credit] name and email must be set together, or both left unset"),
        }
    }
}

/// The value of git config `key`; an error when git is missing or the key
/// is unset or empty.
fn git_config(key: &str) -> Result<String> {
    let out = Command::new("git")
        .args(["config", "--get", key])
        .output()
        .with_context(|| format!("running `git config --get {key}` for the credit line"))?;
    let value = String::from_utf8(out.stdout)
        .with_context(|| format!("git config {key} is not UTF-8"))?
        .trim()
        .to_string();
    if !out.status.success() || value.is_empty() {
        bail!(
            "the credit line needs who compiles the deck: set [credit] name and email in \
             rsslide.toml, or git config {key} (or [credit] enabled = false)"
        );
    }
    Ok(value)
}

/// The credit line for a deck `compiler` compiles on `date`.
pub fn line(compiler: &Compiler, date: jiff::civil::Date) -> String {
    format!(
        "Compiled on {date} by {} <{}> with rsslide {} by {}",
        compiler.name,
        compiler.email,
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_AUTHORS"),
    )
}

/// The credit line for this run, or None when `[credit]` is disabled.
pub fn for_run(cfg: &Credit) -> Result<Option<String>> {
    if !cfg.enabled {
        return Ok(None);
    }
    let compiler = Compiler::resolve(cfg)?;
    Ok(Some(line(&compiler, jiff::Zoned::now().date())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credit(name: Option<&str>, email: Option<&str>) -> Credit {
        Credit {
            name: name.map(str::to_string),
            email: email.map(str::to_string),
            ..Credit::default()
        }
    }

    #[test]
    fn configured_compiler_wins() {
        let c = Compiler::resolve(&credit(Some("Jane Doe"), Some("jane@example.com"))).unwrap();
        assert_eq!(c.name, "Jane Doe");
        assert_eq!(c.email, "jane@example.com");
    }

    #[test]
    fn half_a_compiler_is_rejected() {
        for cfg in [credit(Some("Jane"), None), credit(None, Some("j@x"))] {
            let err = Compiler::resolve(&cfg).unwrap_err().to_string();
            assert!(err.contains("set together"), "{err}");
        }
    }

    #[test]
    fn line_names_date_compiler_and_rsslide() {
        let compiler = Compiler {
            name: "Jane Doe".into(),
            email: "jane@example.com".into(),
        };
        let text = line(&compiler, jiff::civil::date(2026, 10, 25));
        assert_eq!(
            text,
            format!(
                "Compiled on 2026-10-25 by Jane Doe <jane@example.com> with rsslide {} by {}",
                env!("CARGO_PKG_VERSION"),
                env!("CARGO_PKG_AUTHORS")
            )
        );
    }

    #[test]
    fn disabled_credit_needs_no_compiler() {
        let cfg = Credit {
            enabled: false,
            ..Credit::default()
        };
        assert_eq!(for_run(&cfg).unwrap(), None);
    }
}
