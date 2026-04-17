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
    /// Import Marp Markdown files. Alternating input/output paths: IN1 OUT1 IN2 OUT2 ...
    Import {
        #[arg(required = true, num_args = 2..)]
        paths: Vec<PathBuf>,
    },
    /// Convert many (input, output) pairs.
    Generate {
        /// Output format for all pairs
        #[arg(short, long, value_enum)]
        format: Format,

        /// Alternating input/output paths: IN1 OUT1 IN2 OUT2 ...
        #[arg(required = true, num_args = 2..)]
        paths: Vec<PathBuf>,
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
        Some(Command::Import { paths }) => run_import(paths),
        Some(Command::Generate { format, paths }) => run_generate(format, paths),
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
    let output_path = output.unwrap_or_else(|| {
        let stem = input.file_stem().unwrap_or_default();
        let ext = match format {
            Format::Html => "html",
            Format::Pdf => "pdf",
            Format::Pptx => "pptx",
        };
        PathBuf::from(stem).with_extension(ext)
    });
    convert_one(&input, &output_path, format)
}

fn run_generate(format: Format, paths: Vec<PathBuf>) -> Result<()> {
    if paths.len() % 2 != 0 {
        anyhow::bail!(
            "generate requires an even number of arguments (input/output pairs), got {}",
            paths.len()
        );
    }
    for pair in paths.chunks_exact(2) {
        convert_one(&pair[0], &pair[1], format.clone())?;
    }
    Ok(())
}

fn convert_one(input: &std::path::Path, output: &std::path::Path, format: Format) -> Result<()> {
    let input_str = fs::read_to_string(input)
        .with_context(|| format!("Failed to read {}", input.display()))?;
    let presentation = parser::parse(&input_str)
        .with_context(|| format!("Failed to parse {}", input.display()))?;
    match format {
        Format::Html => anyhow::bail!("HTML export not yet implemented"),
        Format::Pdf => {
            exporter::pdf::export(&presentation, output).context("PDF export failed")?;
        }
        Format::Pptx => {
            exporter::pptx::export(&presentation, output).context("PPTX export failed")?;
        }
    }
    println!("Written: {}", output.display());
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

fn run_import(paths: Vec<PathBuf>) -> Result<()> {
    if paths.len() % 2 != 0 {
        anyhow::bail!(
            "import requires an even number of arguments (input/output pairs), got {}",
            paths.len()
        );
    }
    for pair in paths.chunks_exact(2) {
        import_one(&pair[0], &pair[1])?;
    }
    Ok(())
}

fn import_one(input: &std::path::Path, output: &std::path::Path) -> Result<()> {
    let input_str = fs::read_to_string(input)
        .with_context(|| format!("Failed to read {}", input.display()))?;
    let yaml = importer::marp::import(&input_str)
        .with_context(|| format!("Failed to import {}", input.display()))?;
    fs::write(output, yaml)
        .with_context(|| format!("Failed to write {}", output.display()))?;
    println!("Written: {}", output.display());
    Ok(())
}
