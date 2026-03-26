pub mod codegen;
pub mod directory;
pub mod parser;
pub mod types;

use anyhow::{Context, Result};
use std::path::Path;

pub fn convert_file(input: &Path, output: &Path, use_imports: &[String]) -> Result<()> {
    let xsd =
        parser::parse_xsd(input).with_context(|| format!("Failed to parse {}", input.display()))?;

    eprintln!(
        "Parsed {} ({} simple, {} complex types)",
        input.display(),
        xsd.simple_types.len(),
        xsd.complex_types.len(),
    );

    let mut gen = codegen::CodeGenerator::new();
    gen.generate(&xsd, use_imports);

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(output, &gen.output)
        .with_context(|| format!("Failed to write {}", output.display()))?;

    eprintln!("Wrote {} bytes to {}", gen.output.len(), output.display());

    Ok(())
}
