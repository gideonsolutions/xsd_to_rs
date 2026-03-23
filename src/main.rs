mod codegen;
mod parser;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "xsd_to_rs", about = "Convert a single XSD file to a Rust module")]
struct Args {
    /// Input XSD file
    #[arg(short, long)]
    input: PathBuf,

    /// Output Rust file path
    #[arg(short, long)]
    output: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let xsd = parser::parse_xsd(&args.input)
        .with_context(|| format!("Failed to parse {}", args.input.display()))?;

    eprintln!(
        "Parsed {} ({} simple, {} complex types)",
        args.input.display(),
        xsd.simple_types.len(),
        xsd.complex_types.len(),
    );

    let mut gen = codegen::CodeGenerator::new();
    gen.generate(&xsd);

    std::fs::write(&args.output, &gen.output)
        .with_context(|| format!("Failed to write {}", args.output.display()))?;

    eprintln!(
        "Wrote {} bytes to {}",
        gen.output.len(),
        args.output.display()
    );

    Ok(())
}
