use quick_xml::events::Event;
use quick_xml::reader::Reader;

use super::{get_attr, local_name_end, local_name_owned};
use crate::types::SimpleTypeDef;

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
    };

    let mut buf = Vec::new();
    let mut depth = 1i32;
    let mut current_enum_value: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let local = local_name_owned(e);
                handle_facet(&local, e, &mut st, &mut current_enum_value);
            }
            Ok(Event::Empty(ref e)) => {
                let local = local_name_owned(e);
                handle_facet(&local, e, &mut st, &mut current_enum_value);
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
