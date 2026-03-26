mod complex;
mod emit;
pub(crate) mod util;

use std::collections::HashMap;
use std::fmt::Write;

use util::resolve_type;
pub(crate) use util::{sanitize_type_name, xsd_base_to_rust};

use crate::types::XsdFile;

#[derive(Default)]
pub struct CodeGenerator {
    pub simple_type_map: HashMap<String, String>,
    pub output: String,
}

impl CodeGenerator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn generate(&mut self, file: &XsdFile, use_imports: &[String]) {
        writeln!(
            &mut self.output,
            "// Auto-generated from XSD schema: {}\n\
             // Do not edit manually.\n\n\
             #[allow(unused_imports)]\n\
             use serde::{{Deserialize, Serialize}};",
            file.path
        )
        .unwrap();
        for imp in use_imports {
            writeln!(&mut self.output, "#[allow(unused_imports)]\n{imp}").unwrap();
        }
        writeln!(&mut self.output).unwrap();

        for st in &file.simple_types {
            let rust_ty = sanitize_type_name(&st.name);
            self.simple_type_map.insert(st.name.clone(), rust_ty);
        }

        for st in &file.simple_types {
            self.emit_simple_type(st);
        }

        let mut emitted_complex: HashMap<String, bool> = HashMap::new();
        for ct in &file.complex_types {
            if emitted_complex.contains_key(&ct.name) {
                continue;
            }
            emitted_complex.insert(ct.name.clone(), true);
            self.emit_complex_type(ct);
        }

        for elem in &file.elements {
            if let Some(ref ct) = elem.complex_type {
                if !emitted_complex.contains_key(&ct.name) {
                    self.emit_complex_type(ct);
                }
            }
        }
    }

    pub(crate) fn field_type_for_element(&self, elem: &crate::types::ElementDef) -> String {
        if let Some(ref ist) = elem.inline_simple_type {
            if !ist.enumerations.is_empty() {
                return "String /* inline enum */".to_string();
            }
            return xsd_base_to_rust(&ist.base).to_string();
        }
        if let Some(ref tn) = elem.type_name {
            self.resolve_field_type(tn)
        } else {
            "String".to_string()
        }
    }

    pub(crate) fn resolve_field_type(&self, xsd_type: &str) -> String {
        let ty = resolve_type(xsd_type);
        if let Some(rust_ty) = self.simple_type_map.get(ty) {
            return sanitize_type_name(rust_ty);
        }
        let builtin = xsd_base_to_rust(ty);
        if builtin != ty {
            return builtin.to_string();
        }
        sanitize_type_name(ty)
    }
}
