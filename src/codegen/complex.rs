use heck::{ToSnakeCase, ToUpperCamelCase};
use std::fmt::Write;

use super::util::sanitize_type_name;
use super::CodeGenerator;
use crate::types::SequenceMember;

impl CodeGenerator {
    pub(crate) fn emit_complex_type(&mut self, ct: &crate::types::ComplexTypeDef) {
        if let Some(ref doc) = ct.doc {
            writeln!(&mut self.output, "/// {doc}").unwrap();
        }
        writeln!(
            &mut self.output,
            "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]"
        )
        .unwrap();
        let type_name = sanitize_type_name(&ct.name);
        writeln!(&mut self.output, "pub struct {type_name} {{").unwrap();

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
                SequenceMember::Element(elem) => self.emit_field(elem),
                SequenceMember::Choice(choice) => {
                    let enum_name = format!("{}Choice{}", ct.name, choice_idx);
                    let field_name = format!("choice_{choice_idx}");
                    let ty = if choice.min_occurs == 0 {
                        format!("Option<{enum_name}>")
                    } else {
                        enum_name.clone()
                    };
                    writeln!(&mut self.output, "    pub {field_name}: {ty},").unwrap();
                    choice_idx += 1;
                }
            }
        }

        self.emit_attributes(&ct.attributes);
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

    fn emit_attributes(&mut self, attrs: &[crate::types::AttributeDef]) {
        for attr in attrs {
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
    }

    fn emit_choice_enum(&mut self, name: &str, choice: &crate::types::ChoiceGroup) {
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
                "    #[serde(rename = \"{}\")]\n    {}({rust_ty}),",
                elem.name,
                elem.name.to_upper_camel_case(),
            )
            .unwrap();
        }
        writeln!(&mut self.output, "}}\n").unwrap();
    }
}
