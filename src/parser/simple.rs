use quick_xml::events::Event;
use quick_xml::reader::Reader;

use super::{get_attr, local_name_end, local_name_owned};
use crate::types::SimpleTypeDef;

/// The `memberTypes` attribute of an `<xsd:union>`, split on whitespace and
/// stripped of namespace prefixes.
fn union_member_types(e: &quick_xml::events::BytesStart) -> Vec<String> {
    get_attr(e, "memberTypes")
        .unwrap_or_default()
        .split_whitespace()
        .map(|t| t.rsplit(':').next().unwrap_or(t).to_string())
        .collect()
}

pub(super) fn parse_simple_type(
    reader: &mut Reader<&[u8]>,
    name: &str,
) -> anyhow::Result<SimpleTypeDef> {
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
        is_union: false,
        member_types: Vec::new(),
    };

    let mut buf = Vec::new();
    let mut depth = 1i32;
    let mut current_enum_value: Option<String> = None;
    let mut in_doc = false;
    let mut current_enum_doc: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let local = local_name_owned(e);
                if local == "documentation" {
                    in_doc = true;
                } else if local == "union" {
                    st.is_union = true;
                    st.member_types = union_member_types(e);
                } else {
                    handle_facet(&local, e, &mut st, &mut current_enum_value);
                }
            }
            Ok(Event::Empty(ref e)) => {
                let local = local_name_owned(e);
                if local == "union" {
                    st.is_union = true;
                    st.member_types = union_member_types(e);
                }
                handle_facet(&local, e, &mut st, &mut current_enum_value);
                if local == "enumeration" {
                    if let Some(val) = current_enum_value.take() {
                        st.enumerations.push((val, None));
                    }
                }
            }
            Ok(Event::Text(ref t)) => {
                if in_doc {
                    let text = t.unescape().unwrap_or_default().trim().to_string();
                    if !text.is_empty() {
                        if current_enum_value.is_some() {
                            current_enum_doc = Some(text);
                        } else {
                            st.doc = Some(text);
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                depth -= 1;
                let local = local_name_end(e);
                if local == "documentation" {
                    in_doc = false;
                } else if local == "enumeration" {
                    if let Some(val) = current_enum_value.take() {
                        st.enumerations.push((val, current_enum_doc.take()));
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

fn handle_facet(
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
        "list" => {
            st.base = "string".to_string();
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
