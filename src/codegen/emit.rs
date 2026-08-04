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
        if st.is_union {
            // A union of string types accepts any of its members' values; modelling
            // it as the inline restriction's enum alone would wrongly reject the
            // rest. Emit a permissive `String` newtype.
            self.emit_union_newtype(st);
        } else if !st.enumerations.is_empty() {
            self.emit_enum_type(st);
        } else {
            self.emit_newtype(st);
        }
    }

    /// The union's full set of allowed values, or `None` when it cannot be
    /// enumerated. A union is enumerable only if every `memberTypes` entry
    /// resolves to an enumeration simple type; a member like `YearMonthType`
    /// (pattern-restricted) makes the value set open, and rejecting anything
    /// outside the inline enumeration would then be wrong.
    fn union_value_set(&self, st: &crate::types::SimpleTypeDef) -> Option<Vec<String>> {
        if st.member_types.is_empty() {
            return None;
        }
        let mut values: Vec<String> = st.enumerations.iter().map(|(v, _)| v.clone()).collect();
        for member in &st.member_types {
            let member_values = self.simple_type_enums.get(member)?;
            values.extend(member_values.iter().cloned());
        }
        (!values.is_empty()).then_some(values)
    }

    fn emit_union_newtype(&mut self, st: &crate::types::SimpleTypeDef) {
        if let Some(ref doc) = st.doc {
            emit_doc_comment(&mut self.output, doc, "");
        }
        let type_name = sanitize_type_name(&st.name);

        // A union stays a `String` newtype rather than an enum: its value set is
        // the union of its members', which the inline restriction alone does not
        // capture. When every member *is* an enumeration the set is still known,
        // so validate against it on the way in — otherwise the type would accept
        // any string and silently pass a bad code through to the IRS.
        let Some(values) = self.union_value_set(st) else {
            writeln!(
                &mut self.output,
                "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]"
            )
            .unwrap();
            writeln!(&mut self.output, "pub struct {type_name}(pub String);\n").unwrap();
            return;
        };

        writeln!(&mut self.output, "#[derive(Debug, Clone, PartialEq, Serialize)]").unwrap();
        writeln!(&mut self.output, "pub struct {type_name}(pub String);\n").unwrap();
        writeln!(&mut self.output, "impl {type_name} {{").unwrap();
        writeln!(&mut self.output, "    /// Every value this union accepts.").unwrap();
        writeln!(
            &mut self.output,
            "    pub const ALLOWED: &'static [&'static str] = &["
        )
        .unwrap();
        for v in &values {
            writeln!(&mut self.output, "        {v:?},").unwrap();
        }
        writeln!(&mut self.output, "    ];").unwrap();
        writeln!(&mut self.output, "}}\n").unwrap();
        writeln!(
            &mut self.output,
            "impl<'de> Deserialize<'de> for {type_name} {{"
        )
        .unwrap();
        writeln!(
            &mut self.output,
            "    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {{"
        )
        .unwrap();
        writeln!(&mut self.output, "        let s = String::deserialize(deserializer)?;").unwrap();
        writeln!(&mut self.output, "        if Self::ALLOWED.contains(&s.as_str()) {{").unwrap();
        writeln!(&mut self.output, "            Ok({type_name}(s))").unwrap();
        writeln!(&mut self.output, "        }} else {{").unwrap();
        writeln!(
            &mut self.output,
            "            Err(serde::de::Error::custom(format!(\"invalid {type_name} value {{s:?}}\")))"
        )
        .unwrap();
        writeln!(&mut self.output, "        }}").unwrap();
        writeln!(&mut self.output, "    }}").unwrap();
        writeln!(&mut self.output, "}}\n").unwrap();
    }

    fn emit_enum_type(&mut self, st: &crate::types::SimpleTypeDef) {
        if let Some(ref doc) = st.doc {
            emit_doc_comment(&mut self.output, doc, "");
        }
        let type_name = sanitize_type_name(&st.name);

        // A string-restriction enum (de)serializes as its bare string value via
        // explicit impls rather than `#[derive(Serialize, Deserialize)]`. A
        // derived externally-tagged enum serializes a unit variant as a
        // `<Variant/>` *element*, which is wrong wherever the value is text
        // content — `$value`/`$text` position, e.g. a `CheckboxType` inside an
        // indicator that extends it with attributes — and is rejected by MeF
        // schema validation (cvc-complex-type.2.2 / X0000-005). A string is
        // correct in every position and unchanged for plain elements
        // (`<Elem>val</Elem>`).
        writeln!(&mut self.output, "#[derive(Debug, Clone, PartialEq)]").unwrap();
        writeln!(&mut self.output, "pub enum {type_name} {{").unwrap();
        for (val, doc) in &st.enumerations {
            let variant = enum_variant_name(val);
            if let Some(doc) = doc {
                emit_doc_comment(&mut self.output, doc, "    ");
            }
            writeln!(&mut self.output, "    {variant},").unwrap();
        }
        writeln!(&mut self.output, "}}\n").unwrap();

        // Serialize: variant -> its XSD string value.
        writeln!(&mut self.output, "impl Serialize for {type_name} {{").unwrap();
        writeln!(
            &mut self.output,
            "    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {{"
        )
        .unwrap();
        writeln!(&mut self.output, "        serializer.serialize_str(match self {{").unwrap();
        for (val, _) in &st.enumerations {
            let variant = enum_variant_name(val);
            writeln!(&mut self.output, "            {type_name}::{variant} => \"{val}\",").unwrap();
        }
        writeln!(&mut self.output, "        }})").unwrap();
        writeln!(&mut self.output, "    }}").unwrap();
        writeln!(&mut self.output, "}}\n").unwrap();

        // Deserialize: XSD string value -> variant.
        writeln!(
            &mut self.output,
            "impl<'de> Deserialize<'de> for {type_name} {{"
        )
        .unwrap();
        writeln!(
            &mut self.output,
            "    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {{"
        )
        .unwrap();
        writeln!(
            &mut self.output,
            "        match String::deserialize(deserializer)?.as_str() {{"
        )
        .unwrap();
        for (val, _) in &st.enumerations {
            let variant = enum_variant_name(val);
            writeln!(
                &mut self.output,
                "            \"{val}\" => Ok({type_name}::{variant}),"
            )
            .unwrap();
        }
        writeln!(
            &mut self.output,
            "            other => Err(serde::de::Error::custom(format!(\"invalid {type_name} value {{other:?}}\"))),"
        )
        .unwrap();
        writeln!(&mut self.output, "        }}").unwrap();
        writeln!(&mut self.output, "    }}").unwrap();
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

        // Repeated elements -> Vec (skip when empty); optional elements ->
        // Option (skip when None); otherwise a plain required field. A `Vec` also
        // gets `default`: serde auto-defaults a missing `Option` to `None`, but a
        // missing repeated element must default to an empty `Vec` rather than
        // failing deserialization with "missing field". `default` affects only
        // deserialization — serialized output is unchanged.
        let (ty, skip, is_vec) = match (&elem.max_occurs, elem.min_occurs) {
            (MaxOccurs::Unbounded, _) => (format!("Vec<{rust_ty}>"), Some("Vec::is_empty"), true),
            (MaxOccurs::Bounded(n), _) if *n > 1 => {
                (format!("Vec<{rust_ty}>"), Some("Vec::is_empty"), true)
            }
            (_, 0) => (format!("Option<{rust_ty}>"), Some("Option::is_none"), false),
            _ => (rust_ty, None, false),
        };

        let mut serde_parts = vec![format!("rename = \"{}\"", elem.name)];
        if is_vec {
            serde_parts.push("default".to_string());
        }
        if let Some(skip) = skip {
            serde_parts.push(format!("skip_serializing_if = \"{skip}\""));
        }
        let serde_attr = format!("    #[serde({})]", serde_parts.join(", "));

        if let Some(ref doc) = elem.doc {
            emit_doc_comment(&mut self.output, doc, "    ");
        }
        writeln!(
            &mut self.output,
            "{serde_attr}\n    pub {field_name}: {ty},"
        )
        .unwrap();
    }
}
