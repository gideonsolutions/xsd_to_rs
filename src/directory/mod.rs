use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

mod mod_gen;

use crate::convert_file;
use mod_gen::generate_mod_files;

pub(crate) fn sanitize_ident(s: &str) -> String {
    let s = s
        .to_lowercase()
        .replace(|c: char| !c.is_ascii_alphanumeric() && c != '_', "_");
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{s}")
    } else {
        s
    }
}

fn xsd_rel_to_mod_path(xsd_rel: &Path) -> String {
    let mut parts = Vec::new();
    for component in xsd_rel.parent().unwrap_or(Path::new("")).components() {
        parts.push(sanitize_ident(&component.as_os_str().to_string_lossy()));
    }
    let stem = sanitize_ident(&xsd_rel.file_stem().unwrap_or_default().to_string_lossy());
    parts.push(stem);
    parts.join("::")
}

struct XsdEntry {
    abs_path: PathBuf,
    out_rel: PathBuf,
    stem: String,
    includes: Vec<String>,
}

pub fn convert_directory(input_dir: &Path, output_dir: &Path) -> Result<()> {
    let mut entries = Vec::new();

    for entry in WalkDir::new(input_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("xsd") {
            continue;
        }

        let rel = path
            .strip_prefix(input_dir)
            .with_context(|| format!("Failed to strip prefix from {}", path.display()))?;

        let mut out_rel = PathBuf::new();
        for component in rel.parent().unwrap_or(Path::new("")).components() {
            out_rel.push(sanitize_ident(&component.as_os_str().to_string_lossy()));
        }
        let stem = sanitize_ident(&rel.file_stem().unwrap_or_default().to_string_lossy());
        out_rel.push(format!("{stem}.rs"));

        let xsd = crate::parser::parse_xsd(path)?;

        entries.push(XsdEntry {
            abs_path: path.to_path_buf(),
            out_rel,
            stem,
            includes: xsd.includes,
        });
    }

    let canonical_root = std::fs::canonicalize(input_dir)?;
    let mut mod_paths: Vec<(PathBuf, String)> = Vec::new();

    for entry in &entries {
        let mut use_imports = Vec::new();
        for include in &entry.includes {
            let include_path = entry
                .abs_path
                .parent()
                .unwrap_or(Path::new(""))
                .join(include);
            if let Ok(canonical) = include_path.canonicalize() {
                if let Ok(inc_rel) = canonical.strip_prefix(&canonical_root) {
                    let mod_path = xsd_rel_to_mod_path(inc_rel);
                    use_imports.push(format!("use crate::{mod_path}::*;"));
                }
            }
        }

        let out_path = output_dir.join(&entry.out_rel);
        convert_file(&entry.abs_path, &out_path, &use_imports)?;
        mod_paths.push((entry.out_rel.clone(), entry.stem.clone()));
    }

    generate_mod_files(output_dir, &mod_paths)
}
