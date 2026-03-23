use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::path::Path;

/// Represents an XSD simple type (restriction-based).
#[derive(Debug, Clone)]
pub struct SimpleTypeDef {
    pub name: String,
    pub base: String,
    pub enumerations: Vec<(String, Option<String>)>,
    pub pattern: Option<String>,
    pub min_length: Option<u64>,
    pub max_length: Option<u64>,
    pub total_digits: Option<u64>,
    pub fraction_digits: Option<u64>,
    pub min_inclusive: Option<String>,
    pub max_inclusive: Option<String>,
    pub doc: Option<String>,
}

/// Represents an element inside a complexType sequence.
#[derive(Debug, Clone)]
pub struct ElementDef {
    pub name: String,
    pub type_name: Option<String>,
    pub min_occurs: u64,
    pub max_occurs: MaxOccurs,
    pub doc: Option<String>,
    pub inline_simple_type: Option<SimpleTypeDef>,
}

#[derive(Debug, Clone)]
pub enum MaxOccurs {
    Bounded(u64),
    Unbounded,
}

/// Represents a choice group (maps to Rust enum).
#[derive(Debug, Clone)]
pub struct ChoiceGroup {
    pub min_occurs: u64,
    pub elements: Vec<ElementDef>,
}

/// A member of a complex type's body: either a plain element or a choice.
#[derive(Debug, Clone)]
pub enum SequenceMember {
    Element(ElementDef),
    Choice(ChoiceGroup),
}

/// Represents an XSD complex type.
#[derive(Debug, Clone)]
pub struct ComplexTypeDef {
    pub name: String,
    pub members: Vec<SequenceMember>,
    pub attributes: Vec<AttributeDef>,
    pub base_type: Option<String>,
    pub doc: Option<String>,
}

/// Represents an attribute on a complex type.
#[derive(Debug, Clone)]
pub struct AttributeDef {
    pub name: String,
    pub type_name: String,
    pub required: bool,
    pub fixed: Option<String>,
}

/// Top-level element declaration.
#[derive(Debug, Clone)]
pub struct TopLevelElement {
    pub name: String,
    pub type_name: Option<String>,
    pub complex_type: Option<ComplexTypeDef>,
}

/// Parsed contents of a single XSD file.
#[derive(Debug, Clone)]
pub struct XsdFile {
    pub path: String,
    pub simple_types: Vec<SimpleTypeDef>,
    pub complex_types: Vec<ComplexTypeDef>,
    pub elements: Vec<TopLevelElement>,
    pub includes: Vec<String>,
}

fn local_name_owned(e: &quick_xml::events::BytesStart) -> String {
    let name = e.name();
    let full = std::str::from_utf8(name.as_ref()).unwrap_or("");
    full.rsplit_once(':').map(|(_, l)| l).unwrap_or(full).to_string()
}

fn local_name_end(e: &quick_xml::events::BytesEnd) -> String {
    let name = e.name();
    let full = std::str::from_utf8(name.as_ref()).unwrap_or("");
    full.rsplit_once(':').map(|(_, l)| l).unwrap_or(full).to_string()
}

fn get_attr(e: &quick_xml::events::BytesStart, key: &str) -> Option<String> {
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

    Ok(xsd)
}

fn parse_top_level_element(
    reader: &mut Reader<&[u8]>,
    name: &str,
    type_name: Option<String>,
) -> Result<TopLevelElement> {
    let mut buf = Vec::new();
    let mut depth = 1i32;
    let mut complex_type = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let local = local_name_owned(e);
                if local == "complexType" {
                    if let Ok(ct) = parse_complex_type(reader, name) {
                        complex_type = Some(ct);
                    }
                    depth -= 1;
                }
            }
            Ok(Event::End(_)) => {
                depth -= 1;
                if depth <= 0 {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(TopLevelElement {
        name: name.to_string(),
        type_name,
        complex_type,
    })
}

fn parse_simple_type(reader: &mut Reader<&[u8]>, name: &str) -> Result<SimpleTypeDef> {
    let mut st = SimpleTypeDef {
        name: name.to_string(),
        base: String::new(),
        enumerations: Vec::new(),
        pattern: None,
        min_length: None,
        max_length: None,
        total_digits: None,
        fraction_digits: None,
        min_inclusive: None,
        max_inclusive: None,
        doc: None,
    };

    let mut buf = Vec::new();
    let mut depth = 1i32;
    let mut current_enum_value: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let local = local_name_owned(e);
                handle_simple_type_facet(&local, e, &mut st, &mut current_enum_value);
            }
            Ok(Event::Empty(ref e)) => {
                let local = local_name_owned(e);
                handle_simple_type_facet(&local, e, &mut st, &mut current_enum_value);
                if local == "enumeration" {
                    if let Some(val) = current_enum_value.take() {
                        st.enumerations.push((val, None));
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                depth -= 1;
                let local = local_name_end(e);
                if local == "enumeration" {
                    if let Some(val) = current_enum_value.take() {
                        st.enumerations.push((val, None));
                    }
                }
                if depth <= 0 {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(st)
}

fn handle_simple_type_facet(
    local: &str,
    e: &quick_xml::events::BytesStart,
    st: &mut SimpleTypeDef,
    current_enum_value: &mut Option<String>,
) {
    match local {
        "restriction" => {
            if let Some(base) = get_attr(e, "base") {
                st.base = base;
            }
        }
        "enumeration" => {
            if let Some(val) = get_attr(e, "value") {
                *current_enum_value = Some(val);
            }
        }
        "pattern" => {
            if let Some(val) = get_attr(e, "value") {
                st.pattern = Some(val);
            }
        }
        "minLength" => {
            if let Some(val) = get_attr(e, "value") {
                st.min_length = val.parse().ok();
            }
        }
        "maxLength" => {
            if let Some(val) = get_attr(e, "value") {
                st.max_length = val.parse().ok();
            }
        }
        "totalDigits" => {
            if let Some(val) = get_attr(e, "value") {
                st.total_digits = val.parse().ok();
            }
        }
        "fractionDigits" => {
            if let Some(val) = get_attr(e, "value") {
                st.fraction_digits = val.parse().ok();
            }
        }
        "minInclusive" => {
            if let Some(val) = get_attr(e, "value") {
                st.min_inclusive = Some(val);
            }
        }
        "maxInclusive" => {
            if let Some(val) = get_attr(e, "value") {
                st.max_inclusive = Some(val);
            }
        }
        _ => {}
    }
}

fn parse_complex_type(reader: &mut Reader<&[u8]>, name: &str) -> Result<ComplexTypeDef> {
    let mut ct = ComplexTypeDef {
        name: name.to_string(),
        members: Vec::new(),
        attributes: Vec::new(),
        base_type: None,
        doc: None,
    };

    let mut buf = Vec::new();
    let mut depth = 1i32;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let local = local_name_owned(e);
                match local.as_str() {
                    "extension" => {
                        if let Some(base) = get_attr(e, "base") {
                            ct.base_type = Some(base);
                        }
                    }
                    "element" => {
                        let elem = parse_element_start(reader, e);
                        ct.members.push(SequenceMember::Element(elem));
                        depth -= 1;
                    }
                    "choice" => {
                        let min_occurs = get_attr(e, "minOccurs")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(1);
                        let choice = parse_choice(reader, min_occurs)?;
                        ct.members.push(SequenceMember::Choice(choice));
                        depth -= 1;
                    }
                    "attribute" => {
                        let attr = make_attribute(e);
                        ct.attributes.push(attr);
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let local = local_name_owned(e);
                match local.as_str() {
                    "element" => {
                        ct.members.push(SequenceMember::Element(make_element_from_empty(e)));
                    }
                    "attribute" => {
                        ct.attributes.push(make_attribute(e));
                    }
                    "extension" => {
                        if let Some(base) = get_attr(e, "base") {
                            ct.base_type = Some(base);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(_)) => {
                depth -= 1;
                if depth <= 0 {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(ct)
}

fn parse_choice(reader: &mut Reader<&[u8]>, min_occurs: u64) -> Result<ChoiceGroup> {
    let mut choice = ChoiceGroup {
        min_occurs,
        elements: Vec::new(),
    };
    let mut buf = Vec::new();
    let mut depth = 1i32;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let local = local_name_owned(e);
                if local == "element" {
                    let elem = parse_element_start(reader, e);
                    choice.elements.push(elem);
                    depth -= 1;
                }
            }
            Ok(Event::Empty(ref e)) => {
                let local = local_name_owned(e);
                if local == "element" {
                    choice.elements.push(make_element_from_empty(e));
                }
            }
            Ok(Event::End(_)) => {
                depth -= 1;
                if depth <= 0 {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(choice)
}

fn parse_element_start(
    reader: &mut Reader<&[u8]>,
    start: &quick_xml::events::BytesStart,
) -> ElementDef {
    let name = get_attr(start, "name").unwrap_or_default();
    let type_name = get_attr(start, "type");
    let min_occurs = get_attr(start, "minOccurs")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let max_occurs = match get_attr(start, "maxOccurs").as_deref() {
        Some("unbounded") => MaxOccurs::Unbounded,
        Some(n) => MaxOccurs::Bounded(n.parse().unwrap_or(1)),
        None => MaxOccurs::Bounded(1),
    };

    let mut inline_simple_type = None;
    let mut buf = Vec::new();
    let mut depth = 1i32;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let local = local_name_owned(e);
                if local == "simpleType" {
                    if let Ok(st) = parse_simple_type(reader, &format!("{}Inline", name)) {
                        inline_simple_type = Some(st);
                        depth -= 1;
                    }
                }
            }
            Ok(Event::End(_)) => {
                depth -= 1;
                if depth <= 0 {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            _ => {}
        }
        buf.clear();
    }

    ElementDef {
        name,
        type_name,
        min_occurs,
        max_occurs,
        doc: None,
        inline_simple_type,
    }
}

fn make_element_from_empty(e: &quick_xml::events::BytesStart) -> ElementDef {
    let name = get_attr(e, "name").unwrap_or_default();
    let type_name = get_attr(e, "type");
    let min_occurs = get_attr(e, "minOccurs")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let max_occurs = match get_attr(e, "maxOccurs").as_deref() {
        Some("unbounded") => MaxOccurs::Unbounded,
        Some(n) => MaxOccurs::Bounded(n.parse().unwrap_or(1)),
        None => MaxOccurs::Bounded(1),
    };

    ElementDef {
        name,
        type_name,
        min_occurs,
        max_occurs,
        doc: None,
        inline_simple_type: None,
    }
}

fn make_attribute(e: &quick_xml::events::BytesStart) -> AttributeDef {
    AttributeDef {
        name: get_attr(e, "name").unwrap_or_default(),
        type_name: get_attr(e, "type").unwrap_or_else(|| "String".into()),
        required: get_attr(e, "use").as_deref() == Some("required"),
        fixed: get_attr(e, "fixed"),
    }
}
