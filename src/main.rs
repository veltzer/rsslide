mod exporter;
mod model;
mod parser;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let input_str = fs::read_to_string(&cli.input)
        .with_context(|| format!("Failed to read {}", cli.input.display()))?;

    let presentation = parser::parse(&input_str)
        .with_context(|| format!("Failed to parse {}", cli.input.display()))?;

    let output_path = cli.output.unwrap_or_else(|| {
        let stem = cli.input.file_stem().unwrap_or_default();
        let ext = match cli.format {
            Format::Html => "html",
            Format::Pdf => "pdf",
            Format::Pptx => "pptx",
        };
        PathBuf::from(stem).with_extension(ext)
    });

    match cli.format {
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
