use quick_xml::events::Event;
use quick_xml::reader::Reader;

use super::complex::parse_complex_type;
use super::simple::parse_simple_type;
use super::{get_attr, local_name_owned};
use crate::types::{AttributeDef, ElementDef, MaxOccurs, TopLevelElement};

fn strip_ns(s: &str) -> &str {
    s.rsplit_once(':').map(|(_, l)| l).unwrap_or(s)
}

pub(super) fn element_name_from(e: &quick_xml::events::BytesStart) -> String {
    get_attr(e, "name")
        .or_else(|| get_attr(e, "ref").map(|r| strip_ns(&r).to_string()))
        .unwrap_or_default()
}

pub(super) fn element_type_from(e: &quick_xml::events::BytesStart) -> Option<String> {
    get_attr(e, "type").or_else(|| get_attr(e, "ref").map(|r| strip_ns(&r).to_string()))
}

fn parse_occurs(e: &quick_xml::events::BytesStart) -> (u64, MaxOccurs) {
    let min_occurs = get_attr(e, "minOccurs")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let max_occurs = match get_attr(e, "maxOccurs").as_deref() {
        Some("unbounded") => MaxOccurs::Unbounded,
        Some(n) => MaxOccurs::Bounded(n.parse().unwrap_or(1)),
        None => MaxOccurs::Bounded(1),
    };
    (min_occurs, max_occurs)
}

pub(super) fn parse_element_start(
    reader: &mut Reader<&[u8]>,
    start: &quick_xml::events::BytesStart,
) -> ElementDef {
    let name = element_name_from(start);
    let type_name = element_type_from(start);
    let (min_occurs, max_occurs) = parse_occurs(start);

    let mut inline_simple_type = None;
    let mut buf = Vec::new();
    let mut depth = 1i32;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let local = local_name_owned(e);
                if local == "simpleType" {
                    if let Ok(st) = parse_simple_type(reader, &format!("{name}Inline")) {
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

pub(super) fn make_element_from_empty(e: &quick_xml::events::BytesStart) -> ElementDef {
    let name = element_name_from(e);
    let type_name = element_type_from(e);
    let (min_occurs, max_occurs) = parse_occurs(e);

    ElementDef {
        name,
        type_name,
        min_occurs,
        max_occurs,
        doc: None,
        inline_simple_type: None,
    }
}

pub(super) fn make_attribute(e: &quick_xml::events::BytesStart) -> AttributeDef {
    AttributeDef {
        name: get_attr(e, "name").unwrap_or_default(),
        type_name: get_attr(e, "type").unwrap_or_else(|| "String".into()),
        required: get_attr(e, "use").as_deref() == Some("required"),
        fixed: get_attr(e, "fixed"),
    }
}

pub(super) fn parse_top_level_element(
    reader: &mut Reader<&[u8]>,
    name: &str,
    type_name: Option<String>,
) -> anyhow::Result<TopLevelElement> {
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
