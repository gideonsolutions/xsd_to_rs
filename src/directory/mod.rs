use anyhow::{bail, Context, Result};
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

/// Everything one pass over a schema package learns: the parsed entries, the
/// cross-file attribute/model-group registries, and the (canonical) include
/// graph. Shared by the single-tree and layered conversions.
struct Scanned {
    entries: Vec<XsdEntry>,
    attr_groups: HashMap<String, Vec<AttributeDef>>,
    model_groups: HashMap<String, Vec<SequenceMember>>,
    include_map: HashMap<PathBuf, Vec<PathBuf>>,
    canonical_root: PathBuf,
}

fn scan_package(input_dir: &Path) -> Result<Scanned> {
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

    Ok(Scanned {
        entries,
        attr_groups,
        model_groups,
        include_map,
        canonical_root,
    })
}

/// Resolve transitive includes of `start` into `visited`.
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

/// The canonical set of schemas reachable from `roots` (roots included), or an
/// error naming the first root that does not exist.
fn keep_set(input_dir: &Path, scanned: &Scanned, roots: &[PathBuf]) -> Result<HashSet<PathBuf>> {
    let mut keep = HashSet::new();
    for root in roots {
        let abs = input_dir.join(root);
        let canonical = abs
            .canonicalize()
            .with_context(|| format!("prune root not found: {}", abs.display()))?;
        collect_transitive(&canonical, &scanned.include_map, &mut keep);
        keep.insert(canonical);
    }
    Ok(keep)
}

pub fn convert_directory(input_dir: &Path, output_dir: &Path) -> Result<()> {
    convert_directory_inner(input_dir, output_dir, None, None)
}

/// Like [`convert_directory`], but emits only the schemas **reachable** from
/// `roots` (paths relative to `input_dir`) through transitive
/// `<xsd:include>`s — a tree-shake.
///
/// An IRS MeF package ships schemas beyond what its returns reach (message,
/// SOAP, and attachment plumbing at minimum); pruning to the reachable set
/// keeps the generated crate to what the form's returns actually use.
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
    let scanned = scan_package(input_dir)?;

    // With roots given, keep only the schemas reachable from them (the roots
    // themselves plus their transitive includes); otherwise keep everything.
    let keep: Option<HashSet<PathBuf>> = match roots {
        None => None,
        Some(roots) => Some(keep_set(input_dir, &scanned, roots)?),
    };

    let mut mod_paths: Vec<(PathBuf, String)> = Vec::new();

    for entry in &scanned.entries {
        let canonical = entry.abs_path.canonicalize().unwrap_or_default();
        if let Some(keep) = &keep {
            if !keep.contains(&canonical) {
                continue;
            }
        }
        let mut all_includes = HashSet::new();
        collect_transitive(&canonical, &scanned.include_map, &mut all_includes);

        let mut use_imports = Vec::new();
        for inc in &all_includes {
            if let Ok(inc_rel) = inc.strip_prefix(&scanned.canonical_root) {
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
            &scanned.attr_groups,
            &scanned.model_groups,
        )?;
        mod_paths.push((entry.out_rel.clone(), entry.stem.clone()));
    }

    generate_mod_files(output_dir, &mod_paths, mod_prefix)
}

/// One layer of a stacked-crate conversion — see [`convert_directory_layered`].
pub struct Layer<'a> {
    /// The layer crate's *ident* (underscored), used in cross-crate `use`
    /// paths emitted into higher layers, e.g. `gideon_mef_1040_2025v5_3_base`.
    pub crate_ident: &'a str,
    /// Where this layer's source tree is written (the crate's `src/`).
    pub out_dir: &'a Path,
    /// The sanitized top-level payload directories this layer owns, e.g.
    /// `["common", "shared"]`. Every emitted schema must fall under exactly
    /// one layer's directory.
    pub dirs: &'a [&'a str],
}

/// Convert one schema package into **stacked crates** instead of one tree.
///
/// rustc's peak memory scales with the size of the crate it is compiling, and
/// a full IRS MeF package generates ~100k lines — more front-end memory than a
/// stock CI runner has. Splitting the package along its own layering (the
/// common/shared types at the bottom, the dependency forms in the middle, the
/// filing family on top) divides that peak by the size of the largest layer
/// while keeping every type, and every module path within its layer, intact.
///
/// `layers` is ordered bottom-up. Each emitted file's cross-file imports
/// resolve within its own layer as `use crate::…` and against a *lower* layer
/// as `use <crate_ident>::…`. An include that resolves to a **higher** layer
/// is an error (the layering would not compile), as is a schema in a
/// directory no layer owns. Each layer gets its own `lib.rs` module tree.
/// `roots` prunes to the reachable set exactly like
/// [`convert_directory_pruned`].
pub fn convert_directory_layered(
    input_dir: &Path,
    layers: &[Layer<'_>],
    roots: &[PathBuf],
) -> Result<()> {
    let scanned = scan_package(input_dir)?;
    let keep = keep_set(input_dir, &scanned, roots)?;

    // Which layer owns a module path (by its first component).
    let layer_of = |rel: &Path| -> Option<usize> {
        let top = rel.components().next()?.as_os_str().to_string_lossy();
        layers.iter().position(|l| l.dirs.contains(&top.as_ref()))
    };

    let mut mod_paths: Vec<Vec<(PathBuf, String)>> = vec![Vec::new(); layers.len()];

    for entry in &scanned.entries {
        let canonical = entry.abs_path.canonicalize().unwrap_or_default();
        if !keep.contains(&canonical) {
            continue;
        }
        let Some(layer_idx) = layer_of(&entry.out_rel) else {
            bail!(
                "no layer owns {} (top-level directory not assigned)",
                entry.out_rel.display()
            );
        };
        let layer = &layers[layer_idx];

        let mut all_includes = HashSet::new();
        collect_transitive(&canonical, &scanned.include_map, &mut all_includes);

        let mut use_imports = Vec::new();
        for inc in &all_includes {
            let Ok(inc_rel) = inc.strip_prefix(&scanned.canonical_root) else {
                continue;
            };
            // The include's layer, from its *output* path (sanitized).
            let mut inc_out = PathBuf::new();
            for component in inc_rel.components() {
                inc_out.push(sanitize_ident(&component.as_os_str().to_string_lossy()));
            }
            let Some(inc_layer) = layer_of(&inc_out) else {
                bail!(
                    "no layer owns include {} (reached from {})",
                    inc_rel.display(),
                    entry.out_rel.display()
                );
            };
            if inc_layer > layer_idx {
                bail!(
                    "layering violation: {} (layer {}) includes {} (layer {}) — \
                     a lower layer cannot depend on a higher one",
                    entry.out_rel.display(),
                    layer.crate_ident,
                    inc_rel.display(),
                    layers[inc_layer].crate_ident,
                );
            }
            let mod_path = xsd_rel_to_mod_path(inc_rel);
            let import = if inc_layer == layer_idx {
                format!("use crate::{mod_path}::*;")
            } else {
                format!("use {}::{mod_path}::*;", layers[inc_layer].crate_ident)
            };
            use_imports.push(import);
        }
        use_imports.sort();

        let out_path = layer.out_dir.join(&entry.out_rel);
        convert_file_with_groups(
            &entry.abs_path,
            &out_path,
            &use_imports,
            &scanned.attr_groups,
            &scanned.model_groups,
        )?;
        mod_paths[layer_idx].push((entry.out_rel.clone(), entry.stem.clone()));
    }

    for (layer, paths) in layers.iter().zip(&mod_paths) {
        if paths.is_empty() {
            bail!(
                "layer {} matched no schemas — check its directory list",
                layer.crate_ident
            );
        }
        generate_mod_files(layer.out_dir, paths, None)?;
    }
    Ok(())
}
