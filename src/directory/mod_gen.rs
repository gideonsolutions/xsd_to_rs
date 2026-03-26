use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub(crate) fn generate_mod_files(output_dir: &Path, mod_paths: &[(PathBuf, String)]) -> Result<()> {
    let mut dir_children: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();

    for (rel_path, _) in mod_paths {
        let parent = rel_path.parent().unwrap_or(Path::new("")).to_path_buf();
        let mod_name = rel_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        dir_children
            .entry(parent.clone())
            .or_default()
            .insert(mod_name);

        let mut ancestor = parent;
        while let Some(grandparent) = ancestor.parent() {
            let dir_mod = ancestor
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if !dir_mod.is_empty() {
                dir_children
                    .entry(grandparent.to_path_buf())
                    .or_default()
                    .insert(dir_mod);
            }
            if grandparent.as_os_str().is_empty() {
                break;
            }
            ancestor = grandparent.to_path_buf();
        }
    }

    for (dir, children) in &dir_children {
        let mod_file = if dir.as_os_str().is_empty() {
            output_dir.join("lib.rs")
        } else {
            output_dir.join(dir).join("mod.rs")
        };

        let mut content = String::from("// Auto-generated module declarations.\n\n");
        for child in children {
            content.push_str(&format!("pub mod {child};\n"));
        }

        if let Some(parent) = mod_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&mod_file, &content)?;
        eprintln!("Wrote {}", mod_file.display());
    }

    Ok(())
}
