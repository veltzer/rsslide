mod assets;
mod config;
mod exporter;
mod importer;
mod model;
mod parser;
mod utils;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, ValueEnum)]
enum Format {
    Html,
    Pdf,
    Pptx,
}

#[derive(Debug, Parser)]
#[command(
    name = "rsslide",
    about = "Convert YAML presentations to HTML, PDF or PPTX"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Convert a YAML presentation to HTML, PDF or PPTX
    Process {
        /// Input YAML file
        input: PathBuf,

        /// Output file [default: <input stem>.<format>]
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Output format
        #[arg(short, long, value_enum, default_value = "html")]
        format: Format,

        /// Path to a rsslide.toml config file (overrides auto-discovery)
        #[arg(long)]
        config: Option<PathBuf>,

        /// Report content that runs off the page as a warning and draw
        /// the slide anyway, cut at the page edge as marp cuts it. Without
        /// it, such content is an error.
        #[arg(long)]
        warn_overflow: bool,
    },
    /// Import Marp Markdown files. Alternating input/output paths: IN1 OUT1 IN2 OUT2 ...
    Import {
        #[arg(required = true, num_args = 2..)]
        paths: Vec<PathBuf>,
    },
    /// Convert many (input, output) pairs. An input is rsslide YAML or a
    /// marp deck (.md), which is imported in memory first.
    Generate {
        /// Output format for all pairs
        #[arg(short, long, value_enum)]
        format: Format,

        /// Path to a rsslide.toml config file (overrides auto-discovery)
        #[arg(long)]
        config: Option<PathBuf>,

        /// Report content that runs off the page as a warning and draw
        /// the slide anyway, cut at the page edge as marp cuts it. Without
        /// it, such content is an error.
        #[arg(long)]
        warn_overflow: bool,

        /// Alternating input/output paths: IN1 OUT1 IN2 OUT2 ...
        #[arg(required = true, num_args = 2..)]
        paths: Vec<PathBuf>,
    },
    /// Print version information
    Version,
    /// Print a fully-commented config file with all defaults. Redirect to
    /// `rsslide.toml` to seed project-local settings.
    DumpConfig {
        /// Write to this file instead of stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Process {
            input,
            output,
            format,
            config,
            warn_overflow,
        }) => run_process(input, output, format, config, overflow(warn_overflow)),
        Some(Command::Import { paths }) => run_import(paths),
        Some(Command::Generate {
            format,
            config,
            warn_overflow,
            paths,
        }) => run_generate(format, config, overflow(warn_overflow), paths),
        Some(Command::Version) => {
            print_version();
            Ok(())
        }
        Some(Command::DumpConfig { output }) => run_dump_config(output),
        None => {
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
    }
}

fn overflow(warn: bool) -> exporter::pdf::Overflow {
    if warn {
        exporter::pdf::Overflow::Warn
    } else {
        exporter::pdf::Overflow::Error
    }
}

fn run_process(
    input: PathBuf,
    output: Option<PathBuf>,
    format: Format,
    config_path: Option<PathBuf>,
    overflow: exporter::pdf::Overflow,
) -> Result<()> {
    let output_path = output.unwrap_or_else(|| {
        let stem = input.file_stem().unwrap_or_default();
        let ext = match format {
            Format::Html => "html",
            Format::Pdf => "pdf",
            Format::Pptx => "pptx",
        };
        PathBuf::from(stem).with_extension(ext)
    });
    let cfg = config::Config::load(config_path.as_deref())?;
    convert_one(&input, &output_path, format, &cfg, overflow)
}

fn run_generate(
    format: Format,
    config_path: Option<PathBuf>,
    overflow: exporter::pdf::Overflow,
    paths: Vec<PathBuf>,
) -> Result<()> {
    if !paths.len().is_multiple_of(2) {
        anyhow::bail!(
            "generate requires an even number of arguments (input/output pairs), got {}",
            paths.len()
        );
    }
    let cfg = config::Config::load(config_path.as_deref())?;
    let (pairs, _) = paths.as_chunks::<2>();
    for pair in pairs {
        convert_one(&pair[0], &pair[1], format.clone(), &cfg, overflow)?;
    }
    Ok(())
}

fn convert_one(
    input: &std::path::Path,
    output: &std::path::Path,
    format: Format,
    cfg: &config::Config,
    overflow: exporter::pdf::Overflow,
) -> Result<()> {
    let presentation = load_presentation(input)?;
    match format {
        Format::Html => anyhow::bail!("HTML export not yet implemented"),
        Format::Pdf => {
            let warnings = exporter::pdf::export(&presentation, output, cfg, overflow)
                .context("PDF export failed")?;
            for warning in warnings {
                eprintln!("warning: {}: {warning}", input.display());
            }
        }
        Format::Pptx => {
            exporter::pptx::export(&presentation, output).context("PPTX export failed")?;
        }
    }
    println!("Written: {}", output.display());
    Ok(())
}

/// The presentation in `input`: rsslide YAML (`.yaml`, `.yml`), or a marp
/// deck (`.md`), imported in memory as `rsslide import` imports it.
fn load_presentation(input: &std::path::Path) -> Result<model::Presentation> {
    let text =
        fs::read_to_string(input).with_context(|| format!("Failed to read {}", input.display()))?;
    let extension = input
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase);
    let yaml = match extension.as_deref() {
        Some("md") => importer::marp::import(&text)
            .with_context(|| format!("Failed to import {}", input.display()))?,
        Some("yaml" | "yml") => text,
        _ => anyhow::bail!(
            "{}: unknown input type; expected rsslide YAML (.yaml, .yml) or a marp deck (.md)",
            input.display()
        ),
    };
    parser::parse(&yaml).with_context(|| format!("Failed to parse {}", input.display()))
}

fn run_dump_config(output: Option<PathBuf>) -> Result<()> {
    let text = config::DEFAULT_CONFIG_TEMPLATE;
    match output {
        Some(path) => {
            fs::write(&path, text)
                .with_context(|| format!("failed to write {}", path.display()))?;
            println!("Written: {}", path.display());
        }
        None => print!("{text}"),
    }
    Ok(())
}

fn print_version() {
    println!(
        "rsslide {} by {}",
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_AUTHORS")
    );
    println!("GIT_DESCRIBE: {}", env!("GIT_DESCRIBE"));
    println!("GIT_SHA: {}", env!("GIT_SHA"));
    println!("GIT_BRANCH: {}", env!("GIT_BRANCH"));
    println!("GIT_DIRTY: {}", env!("GIT_DIRTY"));
    println!("RUSTC_SEMVER: {}", env!("RUSTC_SEMVER"));
    println!("RUST_EDITION: {}", env!("RUST_EDITION"));
    println!("BUILD_TIMESTAMP: {}", env!("BUILD_TIMESTAMP"));
}

fn run_import(paths: Vec<PathBuf>) -> Result<()> {
    if !paths.len().is_multiple_of(2) {
        anyhow::bail!(
            "import requires an even number of arguments (input/output pairs), got {}",
            paths.len()
        );
    }
    let (pairs, _) = paths.as_chunks::<2>();
    for pair in pairs {
        import_one(&pair[0], &pair[1])?;
    }
    Ok(())
}

fn import_one(input: &std::path::Path, output: &std::path::Path) -> Result<()> {
    let input_str =
        fs::read_to_string(input).with_context(|| format!("Failed to read {}", input.display()))?;
    let yaml = importer::marp::import(&input_str)
        .with_context(|| format!("Failed to import {}", input.display()))?;
    fs::write(output, yaml).with_context(|| format!("Failed to write {}", output.display()))?;
    println!("Written: {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str, text: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rsslide-main-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn a_marp_deck_loads_like_its_import() {
        let md = "---\ntitle: Deck\n---\n\n# One\n\n- a\n\n---\n\n## Two\n\ntext\n";
        let from_md = load_presentation(&temp_file("deck.md", md)).unwrap();
        let yaml = importer::marp::import(md).unwrap();
        let from_yaml = load_presentation(&temp_file("deck.yaml", &yaml)).unwrap();
        assert_eq!(format!("{from_md:?}"), format!("{from_yaml:?}"));
        assert_eq!(from_md.slides.len(), 2);
    }

    #[test]
    fn an_unknown_input_type_is_an_error() {
        let err = load_presentation(&temp_file("deck.txt", "slides: []")).unwrap_err();
        assert!(err.to_string().contains("unknown input type"), "{err}");
    }
}
