mod exporter;
mod importer;
mod model;
mod parser;
mod assets;

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
#[command(name = "rsslide", about = "Convert YAML presentations to HTML, PDF or PPTX")]
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

        /// Built-in theme: default | gaia | uncover
        #[arg(long, default_value = "default")]
        theme: String,

        /// Path to a custom CSS theme file
        #[arg(long)]
        theme_set: Option<PathBuf>,
    },
    /// Import a Marp Markdown file and emit rsslide YAML
    Import {
        /// Input Marp .md file
        input: PathBuf,

        /// Output YAML file [default: stdout]
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Print version information
    Version,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Process { input, output, format, theme, theme_set }) => {
            run_process(input, output, format, theme, theme_set)
        }
        Some(Command::Import { input, output }) => run_import(input, output),
        Some(Command::Version) => {
            print_version();
            Ok(())
        }
        None => {
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
    }
}

fn run_process(
    input: PathBuf,
    output: Option<PathBuf>,
    format: Format,
    _theme: String,
    _theme_set: Option<PathBuf>,
) -> Result<()> {
    let input_str = fs::read_to_string(&input)
        .with_context(|| format!("Failed to read {}", input.display()))?;

    let presentation = parser::parse(&input_str)
        .with_context(|| format!("Failed to parse {}", input.display()))?;

    let output_path = output.unwrap_or_else(|| {
        let stem = input.file_stem().unwrap_or_default();
        let ext = match format {
            Format::Html => "html",
            Format::Pdf => "pdf",
            Format::Pptx => "pptx",
        };
        PathBuf::from(stem).with_extension(ext)
    });

    match format {
        Format::Html => {
            anyhow::bail!("HTML export not yet implemented")
        }
        Format::Pdf => {
            exporter::pdf::export(&presentation, &output_path)
                .context("PDF export failed")?;
            println!("Written: {}", output_path.display());
        }
        Format::Pptx => {
            exporter::pptx::export(&presentation, &output_path)
                .context("PPTX export failed")?;
            println!("Written: {}", output_path.display());
        }
    }

    Ok(())
}

fn print_version() {
    println!("rsslide {} by {}", env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_AUTHORS"));
    println!("GIT_DESCRIBE: {}", env!("GIT_DESCRIBE"));
    println!("GIT_SHA: {}", env!("GIT_SHA"));
    println!("GIT_BRANCH: {}", env!("GIT_BRANCH"));
    println!("GIT_DIRTY: {}", env!("GIT_DIRTY"));
    println!("RUSTC_SEMVER: {}", env!("RUSTC_SEMVER"));
    println!("RUST_EDITION: {}", env!("RUST_EDITION"));
    println!("BUILD_TIMESTAMP: {}", env!("BUILD_TIMESTAMP"));
}

fn run_import(input: PathBuf, output: Option<PathBuf>) -> Result<()> {
    let input_str = fs::read_to_string(&input)
        .with_context(|| format!("Failed to read {}", input.display()))?;
    let yaml = importer::marp::import(&input_str)
        .with_context(|| format!("Failed to import {}", input.display()))?;
    match output {
        Some(path) => {
            fs::write(&path, yaml)
                .with_context(|| format!("Failed to write {}", path.display()))?;
            println!("Written: {}", path.display());
        }
        None => print!("{}", yaml),
    }
    Ok(())
}
