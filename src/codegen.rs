use crate::parser::*;
use heck::{ToSnakeCase, ToUpperCamelCase};
use std::collections::HashMap;
use std::fmt::Write;

/// Maps XSD base types to Rust types.
fn xsd_base_to_rust(base: &str) -> &str {
    let base = base.rsplit_once(':').map(|(_, l)| l).unwrap_or(base);
    match base {
        "string" | "normalizedString" | "token" | "anyURI" | "NMTOKEN" | "NMTOKENS"
        | "Name" | "NCName" | "QName" | "ID" | "IDREF" | "language" => "String",
        "boolean" => "bool",
        "integer" | "int" | "long" | "nonNegativeInteger" | "positiveInteger"
        | "negativeInteger" | "nonPositiveInteger" | "short" | "unsignedInt"
        | "unsignedLong" | "unsignedShort" | "byte" | "unsignedByte" => "i64",
        "decimal" | "float" | "double" => "f64",
        "date" | "dateTime" | "time" | "gYear" | "gYearMonth" | "gMonthDay" | "gMonth"
        | "gDay" | "duration" => "String",
        "base64Binary" | "hexBinary" => "String",
        _ => base,
    }
}

fn is_rust_primitive(ty: &str) -> bool {
    matches!(ty, "String" | "bool" | "i64" | "f64")
}

fn resolve_type(ty: &str) -> &str {
    ty.rsplit_once(':').map(|(_, l)| l).unwrap_or(ty)
}

pub struct CodeGenerator {
    pub simple_type_map: HashMap<String, String>,
    pub output: String,
}

impl CodeGenerator {
    pub fn new() -> Self {
        Self {
            simple_type_map: HashMap::new(),
            output: String::new(),
        }
    }

    pub fn generate(&mut self, file: &XsdFile) {
        writeln!(
            &mut self.output,
            "// Auto-generated from XSD schema: {}\n\
             // Do not edit manually.\n\n\
             use serde::{{Deserialize, Serialize}};\n",
            file.path
        )
        .unwrap();

        // First pass: collect simple types for type resolution.
        for st in &file.simple_types {
            let rust_ty = self.simple_type_to_rust(st);
            self.simple_type_map.insert(st.name.clone(), rust_ty);
        }

        // Emit simple types.
        for st in &file.simple_types {
            self.emit_simple_type(st);
        }

        // Emit complex types.
        let mut emitted_complex: HashMap<String, bool> = HashMap::new();
        for ct in &file.complex_types {
            if emitted_complex.contains_key(&ct.name) {
                continue;
            }
            emitted_complex.insert(ct.name.clone(), true);
            self.emit_complex_type(ct);
        }

        // Emit top-level elements with inline complex types.
        for elem in &file.elements {
            if let Some(ref ct) = elem.complex_type {
                if !emitted_complex.contains_key(&ct.name) {
                    self.emit_complex_type(ct);
                }
            }
        }
    }

    fn simple_type_to_rust(&self, st: &SimpleTypeDef) -> String {
        if !st.enumerations.is_empty() {
            return st.name.clone();
        }
        xsd_base_to_rust(&st.base).to_string()
    }

    fn emit_simple_type(&mut self, st: &SimpleTypeDef) {
        if !st.enumerations.is_empty() {
            self.emit_enum_type(st);
        } else {
            self.emit_newtype(st);
        }
    }

    fn emit_enum_type(&mut self, st: &SimpleTypeDef) {
        if let Some(ref doc) = st.doc {
            writeln!(&mut self.output, "/// {doc}").unwrap();
        }
        writeln!(
            &mut self.output,
            "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]"
        )
        .unwrap();
        writeln!(&mut self.output, "pub enum {} {{", st.name).unwrap();
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

    fn emit_newtype(&mut self, st: &SimpleTypeDef) {
        let base = xsd_base_to_rust(&st.base);
        if is_rust_primitive(base) && st.pattern.is_none() {
            return;
        }
        writeln!(
            &mut self.output,
            "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]"
        )
        .unwrap();
        writeln!(
            &mut self.output,
            "pub struct {}(pub {});\n",
            st.name, base
        )
        .unwrap();
    }

    fn emit_complex_type(&mut self, ct: &ComplexTypeDef) {
        if let Some(ref doc) = ct.doc {
            writeln!(&mut self.output, "/// {doc}").unwrap();
        }
        writeln!(
            &mut self.output,
            "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]"
        )
        .unwrap();
        writeln!(&mut self.output, "pub struct {} {{", ct.name).unwrap();

        if let Some(ref base) = ct.base_type {
            let rust_base = self.resolve_field_type(base);
            writeln!(
                &mut self.output,
                "    #[serde(flatten)]\n    pub base: {rust_base},"
            )
            .unwrap();
        }

        let mut choice_idx = 0usize;
        for member in &ct.members {
            match member {
                SequenceMember::Element(elem) => {
                    self.emit_field(elem);
                }
                SequenceMember::Choice(choice) => {
                    let enum_name = format!("{}Choice{}", ct.name, choice_idx);
                    let field_name = format!("choice_{}", choice_idx);
                    let optional = choice.min_occurs == 0;
                    let ty = if optional {
                        format!("Option<{enum_name}>")
                    } else {
                        enum_name.clone()
                    };
                    writeln!(&mut self.output, "    pub {field_name}: {ty},").unwrap();
                    choice_idx += 1;
                }
            }
        }

        for attr in &ct.attributes {
            if attr.name.is_empty() {
                continue;
            }
            let rust_ty = self.resolve_field_type(&attr.type_name);
            let field_name = attr.name.to_snake_case();
            let ty = if attr.required {
                rust_ty
            } else {
                format!("Option<{rust_ty}>")
            };
            writeln!(
                &mut self.output,
                "    #[serde(rename = \"@{}\")]\n    pub {field_name}: {ty},",
                attr.name
            )
            .unwrap();
        }

        writeln!(&mut self.output, "}}\n").unwrap();

        choice_idx = 0;
        for member in &ct.members {
            if let SequenceMember::Choice(choice) = member {
                let enum_name = format!("{}Choice{}", ct.name, choice_idx);
                self.emit_choice_enum(&enum_name, choice);
                choice_idx += 1;
            }
        }
    }

    fn emit_choice_enum(&mut self, name: &str, choice: &ChoiceGroup) {
        writeln!(
            &mut self.output,
            "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]"
        )
        .unwrap();
        writeln!(&mut self.output, "pub enum {name} {{").unwrap();
        for elem in &choice.elements {
            let rust_ty = self.field_type_for_element(elem);
            writeln!(
                &mut self.output,
                "    #[serde(rename = \"{}\")]\n    {}({}),",
                elem.name,
                elem.name.to_upper_camel_case(),
                rust_ty
            )
            .unwrap();
        }
        writeln!(&mut self.output, "}}\n").unwrap();
    }

    fn emit_field(&mut self, elem: &ElementDef) {
        let field_name = elem.name.to_snake_case();
        let rust_ty = self.field_type_for_element(elem);

        let ty = match (&elem.max_occurs, elem.min_occurs) {
            (MaxOccurs::Unbounded, _) => format!("Vec<{rust_ty}>"),
            (MaxOccurs::Bounded(n), _) if *n > 1 => format!("Vec<{rust_ty}>"),
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

    fn field_type_for_element(&self, elem: &ElementDef) -> String {
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

    fn resolve_field_type(&self, xsd_type: &str) -> String {
        let ty = resolve_type(xsd_type);
        if let Some(rust_ty) = self.simple_type_map.get(ty) {
            return rust_ty.clone();
        }
        let builtin = xsd_base_to_rust(ty);
        if builtin != ty {
            return builtin.to_string();
        }
        ty.to_string()
    }
}

fn enum_variant_name(val: &str) -> String {
    if val.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        return format!("V{}", val.replace(['-', '.', ' ', '/'], "_"));
    }
    let cleaned = val
        .replace([' ', '-', '.', '/', '(', ')'], "_")
        .replace("__", "_");
    let camel = cleaned.to_upper_camel_case();
    if camel.is_empty() {
        format!("V{val}")
    } else {
        camel
    }
}
