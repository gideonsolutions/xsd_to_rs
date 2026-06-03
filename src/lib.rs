pub mod codegen;
pub mod directory;
pub mod parser;
pub mod types;

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

use crate::types::{AttributeDef, SequenceMember};

pub fn convert_file(input: &Path, output: &Path, use_imports: &[String]) -> Result<()> {
    convert_file_with_groups(input, output, use_imports, &HashMap::new(), &HashMap::new())
}

/// Like [`convert_file`], but with pre-built cross-file registries for
/// attribute groups (name -> attributes) and model groups (name -> members) so
/// `<xsd:attributeGroup ref>` / `<xsd:group ref>` pointing at groups defined in
/// *included* files can still be expanded. The file's own groups are merged in
/// by the code generator.
pub fn convert_file_with_groups(
    input: &Path,
    output: &Path,
    use_imports: &[String],
    attr_groups: &HashMap<String, Vec<AttributeDef>>,
    model_groups: &HashMap<String, Vec<SequenceMember>>,
) -> Result<()> {
    let xsd =
        parser::parse_xsd(input).with_context(|| format!("Failed to parse {}", input.display()))?;

    eprintln!(
        "Parsed {} ({} simple, {} complex types)",
        input.display(),
        xsd.simple_types.len(),
        xsd.complex_types.len(),
    );

    let mut gen = codegen::CodeGenerator::new();
    gen.attribute_groups = attr_groups.clone();
    gen.model_groups = model_groups.clone();
    gen.generate(&xsd, use_imports);

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(output, &gen.output)
        .with_context(|| format!("Failed to write {}", output.display()))?;

    eprintln!("Wrote {} bytes to {}", gen.output.len(), output.display());

    Ok(())
}
