use heck::ToSnakeCase;
use std::fmt::Write;

use super::util::{enum_variant_name, sanitize_type_name};
use super::CodeGenerator;
use crate::types::MaxOccurs;

impl CodeGenerator {
    pub(crate) fn emit_simple_type(&mut self, st: &crate::types::SimpleTypeDef) {
        if !st.enumerations.is_empty() {
            self.emit_enum_type(st);
        } else {
            self.emit_newtype(st);
        }
    }

    fn emit_enum_type(&mut self, st: &crate::types::SimpleTypeDef) {
        if let Some(ref doc) = st.doc {
            writeln!(&mut self.output, "/// {doc}").unwrap();
        }
        writeln!(
            &mut self.output,
            "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]"
        )
        .unwrap();
        let type_name = sanitize_type_name(&st.name);
        writeln!(&mut self.output, "pub enum {type_name} {{").unwrap();
        for (val, doc) in &st.enumerations {
            let variant = enum_variant_name(val);
            if let Some(doc) = doc {
                writeln!(&mut self.output, "    /// {doc}").unwrap();
            }
            writeln!(
                &mut self.output,
                "    #[serde(rename = \"{val}\")]\n    {variant},"
            )
            .unwrap();
        }
        writeln!(&mut self.output, "}}\n").unwrap();
    }

    fn emit_newtype(&mut self, st: &crate::types::SimpleTypeDef) {
        let base = self.resolve_field_type(&st.base);
        let base = if base.is_empty() {
            "String".to_string()
        } else {
            base
        };
        writeln!(
            &mut self.output,
            "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]"
        )
        .unwrap();
        let type_name = sanitize_type_name(&st.name);
        writeln!(&mut self.output, "pub struct {type_name}(pub {base});\n").unwrap();
    }

    pub(super) fn emit_field(&mut self, elem: &crate::types::ElementDef) {
        if elem.name.is_empty() {
            return;
        }
        let field_name = elem.name.to_snake_case();
        let rust_ty = self.field_type_for_element(elem);

        let ty = match (&elem.max_occurs, elem.min_occurs) {
            (MaxOccurs::Unbounded, _) => format!("Vec<{rust_ty}>"),
            (MaxOccurs::Bounded(n), _) if *n > 1 => {
                format!("Vec<{rust_ty}>")
            }
            (_, 0) => format!("Option<{rust_ty}>"),
            _ => rust_ty,
        };

        writeln!(
            &mut self.output,
            "    #[serde(rename = \"{}\")]\n    pub {field_name}: {ty},",
            elem.name
        )
        .unwrap();
    }
}
