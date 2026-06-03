use heck::ToSnakeCase;
use std::fmt::Write;

use super::util::{enum_variant_name, is_rust_primitive, sanitize_type_name};
use super::CodeGenerator;
use crate::types::{MaxOccurs, SimpleTypeDef};

fn emit_doc_comment(output: &mut String, doc: &str, indent: &str) {
    for line in doc.lines() {
        writeln!(output, "{indent}/// {}", line.trim()).unwrap();
    }
}

fn has_try_new_restrictions(st: &SimpleTypeDef) -> bool {
    st.min_length.is_some()
        || st.max_length.is_some()
        || st.min_inclusive.is_some()
        || st.max_inclusive.is_some()
}

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
            emit_doc_comment(&mut self.output, doc, "");
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
                emit_doc_comment(&mut self.output, doc, "    ");
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
        if let Some(ref doc) = st.doc {
            emit_doc_comment(&mut self.output, doc, "");
        }
        writeln!(
            &mut self.output,
            "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]"
        )
        .unwrap();
        let type_name = sanitize_type_name(&st.name);
        writeln!(&mut self.output, "pub struct {type_name}(pub {base});\n").unwrap();

        if has_try_new_restrictions(st) && is_rust_primitive(&base) {
            self.emit_try_new(st, &type_name, &base);
        }
    }

    fn emit_try_new(&mut self, st: &SimpleTypeDef, type_name: &str, base: &str) {
        let is_string = base == "String";
        let is_decimal = base == "Decimal";

        writeln!(&mut self.output, "impl {type_name} {{").unwrap();
        writeln!(
            &mut self.output,
            "    /// Creates a new `{type_name}`, validating against schema restrictions."
        )
        .unwrap();
        writeln!(
            &mut self.output,
            "    pub fn try_new(value: {base}) -> Result<Self, String> {{"
        )
        .unwrap();

        if is_string {
            if let Some(min) = st.min_length {
                writeln!(
                    &mut self.output,
                    "        if value.len() < {min} {{\n            return Err(format!(\"Length {{}} is less than minimum {min}\", value.len()));\n        }}"
                )
                .unwrap();
            }
            if let Some(max) = st.max_length {
                writeln!(
                    &mut self.output,
                    "        if value.len() > {max} {{\n            return Err(format!(\"Length {{}} exceeds maximum {max}\", value.len()));\n        }}"
                )
                .unwrap();
            }
        } else if is_decimal {
            if let Some(ref min) = st.min_inclusive {
                writeln!(
                    &mut self.output,
                    "        if value < Decimal::from_str(\"{min}\").unwrap() {{\n            return Err(format!(\"Value {{}} is less than minimum {min}\", value));\n        }}"
                )
                .unwrap();
            }
            if let Some(ref max) = st.max_inclusive {
                writeln!(
                    &mut self.output,
                    "        if value > Decimal::from_str(\"{max}\").unwrap() {{\n            return Err(format!(\"Value {{}} exceeds maximum {max}\", value));\n        }}"
                )
                .unwrap();
            }
        } else {
            // Integer types
            if let Some(ref min) = st.min_inclusive {
                writeln!(
                    &mut self.output,
                    "        if value < {min}_{base} {{\n            return Err(format!(\"Value {{}} is less than minimum {min}\", value));\n        }}"
                )
                .unwrap();
            }
            if let Some(ref max) = st.max_inclusive {
                writeln!(
                    &mut self.output,
                    "        if value > {max}_{base} {{\n            return Err(format!(\"Value {{}} exceeds maximum {max}\", value));\n        }}"
                )
                .unwrap();
            }
        }

        writeln!(&mut self.output, "        Ok(Self(value))").unwrap();
        writeln!(&mut self.output, "    }}").unwrap();
        writeln!(&mut self.output, "}}\n").unwrap();
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

        if let Some(ref doc) = elem.doc {
            emit_doc_comment(&mut self.output, doc, "    ");
        }
        writeln!(
            &mut self.output,
            "    #[serde(rename = \"{}\")]\n    pub {field_name}: {ty},",
            elem.name
        )
        .unwrap();
    }
}
