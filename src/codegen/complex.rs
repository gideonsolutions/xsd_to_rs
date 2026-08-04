use heck::{ToSnakeCase, ToUpperCamelCase};
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use super::util::sanitize_type_name;
use super::CodeGenerator;
use crate::types::{MaxOccurs, SequenceMember};

/// Force a member to be optional (used when an enclosing group ref has
/// `minOccurs="0"`): the group may be absent, so each expanded field is too.
fn make_member_optional(member: &mut SequenceMember) {
    match member {
        SequenceMember::Element(elem) => elem.min_occurs = 0,
        SequenceMember::Choice(choice) => choice.min_occurs = 0,
        SequenceMember::GroupRef { min_occurs, .. } => *min_occurs = 0,
    }
}

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
            if ct.simple_content {
                // `<simpleContent>`: the base is the element's text value.
                writeln!(
                    &mut self.output,
                    "    #[serde(rename = \"$value\")]\n    pub value: {rust_base},"
                )
                .unwrap();
            } else {
                // `<complexContent>`: flatten the base type's fields in.
                writeln!(
                    &mut self.output,
                    "    #[serde(flatten)]\n    pub base: {rust_base},"
                )
                .unwrap();
            }
        }

        // Expand `<xsd:group ref>` members inline before emitting, so the
        // struct carries the group's fields and no `GroupRef` reaches output.
        let members = self.resolve_group_refs(&ct.members);

        // A choice serializes via quick-xml's `$value` so the selected variant
        // element appears directly (no `choice_N` wrapper). That only works
        // when the struct has a single choice; with two or more there is no way
        // to disambiguate, so they fall back to the bare field name.
        // Only *simple* (enum-backed) choices use `$value`/`flatten`; composite
        // choices are flattened to plain optional fields and don't count here.
        let single_choice = members
            .iter()
            .filter(|m| matches!(m, SequenceMember::Choice(c) if !c.composite))
            .count()
            == 1;

        // Field name -> emitted Rust type, so flattening a composite choice can
        // tell a legitimate repeat across mutually exclusive branches from a
        // genuine type conflict. See the composite arm below.
        let mut emitted: HashMap<String, String> = HashMap::new();

        let mut choice_idx = 0usize;
        for member in &members {
            match member {
                SequenceMember::Element(elem) => {
                    if !elem.name.is_empty() {
                        emitted
                            .insert(elem.name.to_snake_case(), self.field_type_for_element(elem));
                    }
                    self.emit_field(elem);
                }
                // A composite choice (sequence branches) can't be a single `$value`
                // enum: a selected branch yields several co-occurring elements. Emit
                // each branch element as an optional field on the parent instead;
                // exactly-one-branch is then a business rule, not a type invariant.
                SequenceMember::Choice(choice) if choice.composite => {
                    for elem in &choice.elements {
                        // A flattened branch element may be absent (a different
                        // branch was chosen), so force it optional.
                        let mut elem = elem.clone();
                        elem.min_occurs = 0;
                        if elem.name.is_empty() {
                            continue;
                        }

                        // The same element can appear in several branches — Form
                        // 8858 Schedule M repeats columns (a)-(f) across its
                        // transaction-category branches. Declaring the field once
                        // per branch is a duplicate-field compile error, so
                        // collapse them: at most one branch is ever selected, and
                        // one optional field represents whichever it was.
                        //
                        // Only collapse when the type matches too. Same name and a
                        // different type is a real conflict that must not be
                        // silently mapped onto one field, so it still emits twice
                        // and fails loudly at compile time.
                        let name = elem.name.to_snake_case();
                        let ty = self.field_type_for_element(&elem);
                        if emitted.get(&name) == Some(&ty) {
                            continue;
                        }
                        emitted.insert(name, ty);
                        self.emit_field(&elem);
                    }
                }
                SequenceMember::Choice(choice) => {
                    let enum_name = format!("{}Choice{}", ct.name, choice_idx);
                    let field_name = format!("choice_{choice_idx}");
                    // A choice whose branch elements may repeat (`maxOccurs>1` /
                    // `unbounded`) selects one branch but lets that element occur
                    // many times — e.g. an IRIS submission detail with many
                    // `Form1099ADetail` records. The field is then a `Vec` of the
                    // variant enum (one entry per occurrence), matching how
                    // `emit_field` maps a repeating element. The enum variants stay
                    // single-valued.
                    let repeating = choice.elements.iter().any(|e| {
                        matches!(e.max_occurs, MaxOccurs::Unbounded)
                            || matches!(e.max_occurs, MaxOccurs::Bounded(n) if n > 1)
                    });
                    // Optional when the choice itself is `minOccurs="0"`, or when
                    // every branch element is optional so selecting none is valid
                    // (e.g. a 2290 payment choice where EFW checks neither box).
                    let optional =
                        choice.min_occurs == 0 || choice.elements.iter().all(|e| e.min_occurs == 0);
                    let (ty, skip) = if repeating {
                        (
                            format!("Vec<{enum_name}>"),
                            ", skip_serializing_if = \"Vec::is_empty\"",
                        )
                    } else if optional {
                        (
                            format!("Option<{enum_name}>"),
                            ", skip_serializing_if = \"Option::is_none\"",
                        )
                    } else {
                        (enum_name.clone(), "")
                    };
                    // A lone choice maps to quick-xml's `$value` (the variant
                    // element appears directly). With two or more choices in one
                    // struct there can only be one `$value`, so each is flattened
                    // instead — the variant element still serializes inline.
                    if single_choice {
                        writeln!(&mut self.output, "    #[serde(rename = \"$value\"{skip})]")
                            .unwrap();
                    } else {
                        writeln!(&mut self.output, "    #[serde(flatten{skip})]").unwrap();
                    }
                    writeln!(&mut self.output, "    pub {field_name}: {ty},").unwrap();
                    choice_idx += 1;
                }
                SequenceMember::GroupRef { .. } => {
                    unreachable!("group refs are expanded by resolve_group_refs")
                }
            }
        }

        // Direct attributes plus those pulled in via attributeGroup refs.
        let mut attrs = ct.attributes.clone();
        for gref in &ct.attribute_group_refs {
            match self.attribute_groups.get(gref) {
                Some(group) => attrs.extend(group.iter().cloned()),
                None => eprintln!(
                    "warning: unresolved attributeGroup ref `{gref}` in `{}`",
                    ct.name
                ),
            }
        }
        self.emit_attributes(&attrs);
        writeln!(&mut self.output, "}}\n").unwrap();

        choice_idx = 0;
        for member in &members {
            if let SequenceMember::Choice(choice) = member {
                if choice.composite {
                    continue; // flattened to optional fields; no enum emitted
                }
                let enum_name = format!("{}Choice{}", ct.name, choice_idx);
                self.emit_choice_enum(&enum_name, choice);
                choice_idx += 1;
            }
        }
    }

    /// Recursively expand `<xsd:group ref>` members from the model-group
    /// registry, yielding a flat member list of only elements and choices. A
    /// ref with `minOccurs="0"` makes its expanded members optional. Cyclic
    /// refs (which valid XSD never has) are broken and warned about.
    fn resolve_group_refs(&self, members: &[SequenceMember]) -> Vec<SequenceMember> {
        let mut out = Vec::new();
        self.expand_members(members, &mut out, &mut Vec::new());
        out
    }

    fn expand_members(
        &self,
        members: &[SequenceMember],
        out: &mut Vec<SequenceMember>,
        stack: &mut Vec<String>,
    ) {
        for member in members {
            match member {
                SequenceMember::GroupRef { name, min_occurs } => {
                    if stack.iter().any(|n| n == name) {
                        eprintln!("warning: recursive group ref `{name}` skipped");
                        continue;
                    }
                    let Some(group_members) = self.model_groups.get(name) else {
                        eprintln!("warning: unresolved group ref `{name}`");
                        continue;
                    };
                    stack.push(name.clone());
                    let mut expanded = Vec::new();
                    self.expand_members(group_members, &mut expanded, stack);
                    stack.pop();
                    if *min_occurs == 0 {
                        for m in &mut expanded {
                            make_member_optional(m);
                        }
                    }
                    out.extend(expanded);
                }
                other => out.push(other.clone()),
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
            let serde_attr = if attr.required {
                format!("    #[serde(rename = \"@{}\")]", attr.name)
            } else {
                format!(
                    "    #[serde(rename = \"@{}\", skip_serializing_if = \"Option::is_none\")]",
                    attr.name
                )
            };
            let ty = if attr.required {
                rust_ty
            } else {
                format!("Option<{rust_ty}>")
            };
            writeln!(
                &mut self.output,
                "{serde_attr}\n    pub {field_name}: {ty},"
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
        // A choice may list the same element name more than once (e.g. when the
        // schema repeats branches). Each maps to the same variant identifier, so
        // emit each only once to avoid duplicate-variant errors.
        let mut seen = HashSet::new();
        for elem in &choice.elements {
            let variant = elem.name.to_upper_camel_case();
            if !seen.insert(variant.clone()) {
                continue;
            }
            let rust_ty = self.field_type_for_element(elem);
            writeln!(
                &mut self.output,
                "    #[serde(rename = \"{}\")]\n    {variant}({rust_ty}),",
                elem.name,
            )
            .unwrap();
        }
        writeln!(&mut self.output, "}}\n").unwrap();
    }
}
