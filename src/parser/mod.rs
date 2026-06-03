mod complex;
mod element;
mod simple;

use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::path::Path;

use complex::{parse_attribute_group, parse_complex_type};
use element::parse_top_level_element;
use simple::parse_simple_type;
use std::collections::HashSet;

use crate::types::{
    AttributeGroupDef, ComplexTypeDef, ElementDef, ModelGroupDef, SequenceMember, TopLevelElement,
    XsdFile,
};

pub(crate) fn local_name_owned(e: &quick_xml::events::BytesStart) -> String {
    let name = e.name();
    let full = std::str::from_utf8(name.as_ref()).unwrap_or("");
    full.rsplit_once(':')
        .map(|(_, l)| l)
        .unwrap_or(full)
        .to_string()
}

pub(crate) fn local_name_end(e: &quick_xml::events::BytesEnd) -> String {
    let name = e.name();
    let full = std::str::from_utf8(name.as_ref()).unwrap_or("");
    full.rsplit_once(':')
        .map(|(_, l)| l)
        .unwrap_or(full)
        .to_string()
}

pub(crate) fn get_attr(e: &quick_xml::events::BytesStart, key: &str) -> Option<String> {
    for attr in e.attributes().flatten() {
        if attr.key.as_ref() == key.as_bytes() {
            return String::from_utf8(attr.value.to_vec()).ok();
        }
    }
    None
}

pub fn parse_xsd(path: &Path) -> Result<XsdFile> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let mut reader = Reader::from_str(&content);

    let mut xsd = XsdFile {
        path: path.display().to_string(),
        simple_types: Vec::new(),
        complex_types: Vec::new(),
        elements: Vec::new(),
        attribute_groups: Vec::new(),
        model_groups: Vec::new(),
        includes: Vec::new(),
    };

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) => {
                let local = local_name_owned(e);
                if local == "include" {
                    if let Some(loc) = get_attr(e, "schemaLocation") {
                        xsd.includes.push(loc);
                    }
                } else if local == "element" {
                    if let Some(name) = get_attr(e, "name") {
                        xsd.elements.push(TopLevelElement {
                            name,
                            type_name: get_attr(e, "type"),
                            complex_type: None,
                        });
                    }
                }
            }
            Ok(Event::Start(ref e)) => {
                let local = local_name_owned(e);
                match local.as_str() {
                    "include" => {
                        if let Some(loc) = get_attr(e, "schemaLocation") {
                            xsd.includes.push(loc);
                        }
                    }
                    "simpleType" => {
                        if let Some(name) = get_attr(e, "name") {
                            if let Ok(st) = parse_simple_type(&mut reader, &name) {
                                xsd.simple_types.push(st);
                            }
                        }
                    }
                    "complexType" => {
                        if let Some(name) = get_attr(e, "name") {
                            if let Ok(ct) = parse_complex_type(&mut reader, &name) {
                                xsd.complex_types.push(ct);
                            }
                        }
                    }
                    "attributeGroup" => {
                        // A top-level group *definition* (has `name`); refs
                        // (`ref`) only appear inside complex types.
                        if let Some(name) = get_attr(e, "name") {
                            let attributes = parse_attribute_group(&mut reader)?;
                            xsd.attribute_groups
                                .push(AttributeGroupDef { name, attributes });
                        }
                    }
                    "group" => {
                        // A top-level model group *definition*. Its body is
                        // structurally a complex type's body (a sequence/choice
                        // of elements), so reuse the complex-type parser to
                        // collect the members. Consuming it here also stops the
                        // group's local elements from leaking into `elements`
                        // (where they'd be mis-emitted as top-level aliases).
                        if let Some(name) = get_attr(e, "name") {
                            let body = parse_complex_type(&mut reader, &name)?;
                            xsd.model_groups.push(ModelGroupDef {
                                name,
                                members: body.members,
                            });
                        }
                    }
                    "element" => {
                        if let Some(name) = get_attr(e, "name") {
                            let type_name = get_attr(e, "type");
                            let elem = parse_top_level_element(&mut reader, &name, type_name)?;
                            xsd.elements.push(elem);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                anyhow::bail!("XML parse error in {}: {}", path.display(), e);
            }
            _ => {}
        }
        buf.clear();
    }

    hoist_inline_complex_types(&mut xsd);

    Ok(xsd)
}

/// Lift every anonymous inline `<xsd:complexType>` (parsed onto an element via
/// `ElementDef::inline_complex_type`) into a named entry in
/// `xsd.complex_types`, rewriting the element's `type_name` to reference it.
///
/// After this pass no `inline_complex_type` remains, so the rest of the
/// pipeline only has to deal with named complex types and named type
/// references. Synthesized names are `{Element}Type`, suffixed (`Type2`, …) on
/// collision with an existing name.
fn hoist_inline_complex_types(xsd: &mut XsdFile) {
    let mut used: HashSet<String> = HashSet::new();
    for ct in &xsd.complex_types {
        used.insert(ct.name.clone());
    }
    for st in &xsd.simple_types {
        used.insert(st.name.clone());
    }
    for el in &xsd.elements {
        if let Some(ct) = &el.complex_type {
            used.insert(ct.name.clone());
        }
    }

    let mut hoisted: Vec<ComplexTypeDef> = Vec::new();
    for ct in &mut xsd.complex_types {
        hoist_in_complex(ct, &mut hoisted, &mut used);
    }
    for el in &mut xsd.elements {
        if let Some(ct) = &mut el.complex_type {
            hoist_in_complex(ct, &mut hoisted, &mut used);
        }
    }
    // Model-group bodies are expanded into referencing types at codegen, so
    // any inline complex types they carry must be hoisted here too.
    for g in &mut xsd.model_groups {
        for member in &mut g.members {
            hoist_in_member(member, &mut hoisted, &mut used);
        }
    }
    xsd.complex_types.extend(hoisted);
}

fn hoist_in_complex(
    ct: &mut ComplexTypeDef,
    out: &mut Vec<ComplexTypeDef>,
    used: &mut HashSet<String>,
) {
    for member in &mut ct.members {
        hoist_in_member(member, out, used);
    }
}

fn hoist_in_member(
    member: &mut SequenceMember,
    out: &mut Vec<ComplexTypeDef>,
    used: &mut HashSet<String>,
) {
    match member {
        SequenceMember::Element(elem) => hoist_in_element(elem, out, used),
        SequenceMember::Choice(choice) => {
            for elem in &mut choice.elements {
                hoist_in_element(elem, out, used);
            }
        }
        SequenceMember::GroupRef { .. } => {}
    }
}

/// Normalize a type reference to the Rust identifier codegen will emit for it:
/// strip any namespace prefix and upper-case a leading lowercase letter. Used
/// to compare a synthesized hoist name against a base reference.
fn norm_type_name(ty: &str) -> String {
    let local = ty.rsplit_once(':').map(|(_, l)| l).unwrap_or(ty);
    let mut chars = local.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => c.to_uppercase().chain(chars).collect(),
        _ => local.to_string(),
    }
}

fn hoist_in_element(
    elem: &mut ElementDef,
    out: &mut Vec<ComplexTypeDef>,
    used: &mut HashSet<String>,
) {
    let Some(mut boxed) = elem.inline_complex_type.take() else {
        return;
    };

    // Find a free name, preferring the parser's proposed `{Element}Type`. It
    // must also differ from the type's own base: an inline `<xsd:extension
    // base="X">` on an element named `X` would otherwise synthesize a struct
    // `X` that flattens/embeds base `X` — a self-referential type of infinite
    // size. (The base often lives in another file, so it isn't in `used`.)
    let base_norm = boxed.base_type.as_deref().map(norm_type_name);
    let mut name = boxed.name.clone();
    let mut n = 2;
    loop {
        let collides_base = base_norm.as_deref() == Some(norm_type_name(&name).as_str());
        if !collides_base && used.insert(name.clone()) {
            break;
        }
        name = format!("{}{n}", boxed.name);
        n += 1;
    }
    boxed.name = name.clone();
    elem.type_name = Some(name);

    // Recurse first so nested inline complex types are hoisted too.
    hoist_in_complex(&mut boxed, out, used);
    out.push(*boxed);
}
