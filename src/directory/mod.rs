use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

mod mod_gen;

use crate::convert_file_with_groups;
use crate::types::{AttributeDef, SequenceMember};
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
    convert_directory_inner(input_dir, output_dir, None, None)
}

/// Like [`convert_directory_with_prefix`], but emits only the schemas
/// **reachable** from `roots` (paths relative to `input_dir`) through
/// transitive `<xsd:include>`s — a tree-shake.
///
/// IRS MeF packages ship the entire common-dependency forest alongside a
/// form's own schemas; a 1040 package carries ~475 corporate-side files of
/// which the 1040 returns reach fewer than a fifth. Pruning to the reachable
/// set cuts the generated crate — and rustc's peak memory compiling it — by
/// roughly a third, with no change to any type that is actually used.
///
/// Every root must exist under `input_dir`, or this errors rather than
/// silently emitting an empty tree.
pub fn convert_directory_pruned(
    input_dir: &Path,
    output_dir: &Path,
    mod_prefix: Option<&str>,
    roots: &[PathBuf],
) -> Result<()> {
    convert_directory_inner(input_dir, output_dir, mod_prefix, Some(roots))
}

/// Like [`convert_directory`], but nests the generated tree as an inner module.
///
/// When `mod_prefix` is `Some(p)`, every emitted cross-file import becomes
/// `use crate::<p>::…::*;` (instead of `use crate::…::*;`) and the root module
/// file is written as `mod.rs` rather than `lib.rs`. Use this when one schema
/// package is one inner module of a larger crate (e.g. an IRS schema version
/// inside a per-form crate) rather than being the crate root itself.
pub fn convert_directory_with_prefix(
    input_dir: &Path,
    output_dir: &Path,
    mod_prefix: Option<&str>,
) -> Result<()> {
    convert_directory_inner(input_dir, output_dir, mod_prefix, None)
}

fn convert_directory_inner(
    input_dir: &Path,
    output_dir: &Path,
    mod_prefix: Option<&str>,
    roots: Option<&[PathBuf]>,
) -> Result<()> {
    let mut entries = Vec::new();
    // Cross-file registry of attribute groups (name -> attributes), so a
    // `<xsd:attributeGroup ref>` can be expanded even when the group is defined
    // in an included file. XSD attributeGroup names are unique within a
    // namespace, so a flat map across the package is sufficient.
    let mut attr_groups: HashMap<String, Vec<AttributeDef>> = HashMap::new();
    // Cross-file registry of model groups (name -> members), same rationale as
    // `attr_groups`, for `<xsd:group ref>` expansion.
    let mut model_groups: HashMap<String, Vec<SequenceMember>> = HashMap::new();

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

        for g in &xsd.attribute_groups {
            attr_groups
                .entry(g.name.clone())
                .or_insert_with(|| g.attributes.clone());
        }
        for g in &xsd.model_groups {
            model_groups
                .entry(g.name.clone())
                .or_insert_with(|| g.members.clone());
        }

        entries.push(XsdEntry {
            abs_path: path.to_path_buf(),
            out_rel,
            stem,
            includes: xsd.includes,
        });
    }

    let canonical_root = std::fs::canonicalize(input_dir)?;

    // Build a map from canonical path to direct includes (also canonical)
    let mut include_map: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    for entry in &entries {
        let mut resolved = Vec::new();
        for include in &entry.includes {
            let include_path = entry
                .abs_path
                .parent()
                .unwrap_or(Path::new(""))
                .join(include);
            if let Ok(canonical) = include_path.canonicalize() {
                resolved.push(canonical);
            }
        }
        if let Ok(canonical) = entry.abs_path.canonicalize() {
            include_map.insert(canonical, resolved);
        }
    }

    // Resolve transitive includes
    fn collect_transitive(
        start: &PathBuf,
        include_map: &HashMap<PathBuf, Vec<PathBuf>>,
        visited: &mut HashSet<PathBuf>,
    ) {
        if let Some(includes) = include_map.get(start) {
            for inc in includes {
                if visited.insert(inc.clone()) {
                    collect_transitive(inc, include_map, visited);
                }
            }
        }
    }

    // With roots given, keep only the schemas reachable from them (the roots
    // themselves plus their transitive includes); otherwise keep everything.
    let keep: Option<HashSet<PathBuf>> = match roots {
        None => None,
        Some(roots) => {
            let mut keep = HashSet::new();
            for root in roots {
                let abs = input_dir.join(root);
                let canonical = abs
                    .canonicalize()
                    .with_context(|| format!("prune root not found: {}", abs.display()))?;
                collect_transitive(&canonical, &include_map, &mut keep);
                keep.insert(canonical);
            }
            Some(keep)
        }
    };

    let mut mod_paths: Vec<(PathBuf, String)> = Vec::new();

    for entry in &entries {
        if let Some(keep) = &keep {
            let canonical = entry.abs_path.canonicalize().unwrap_or_default();
            if !keep.contains(&canonical) {
                continue;
            }
        }
        let canonical = entry.abs_path.canonicalize().unwrap_or_default();
        let mut all_includes = HashSet::new();
        collect_transitive(&canonical, &include_map, &mut all_includes);

        let mut use_imports = Vec::new();
        for inc in &all_includes {
            if let Ok(inc_rel) = inc.strip_prefix(&canonical_root) {
                let mod_path = xsd_rel_to_mod_path(inc_rel);
                let import = match mod_prefix {
                    Some(prefix) => format!("use crate::{prefix}::{mod_path}::*;"),
                    None => format!("use crate::{mod_path}::*;"),
                };
                use_imports.push(import);
            }
        }
        use_imports.sort();

        let out_path = output_dir.join(&entry.out_rel);
        convert_file_with_groups(
            &entry.abs_path,
            &out_path,
            &use_imports,
            &attr_groups,
            &model_groups,
        )?;
        mod_paths.push((entry.out_rel.clone(), entry.stem.clone()));
    }

    generate_mod_files(output_dir, &mod_paths, mod_prefix)
}
