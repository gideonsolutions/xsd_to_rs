use anyhow::Result;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use super::element::{make_attribute, make_element_from_empty, parse_element_start};
use super::{get_attr, local_name_owned};
use crate::types::{ChoiceGroup, ComplexTypeDef, SequenceMember};

pub(super) fn parse_complex_type(reader: &mut Reader<&[u8]>, name: &str) -> Result<ComplexTypeDef> {
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
                        ct.members.push(SequenceMember::Element(Box::new(elem)));
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
                        ct.attributes.push(make_attribute(e));
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let local = local_name_owned(e);
                match local.as_str() {
                    "element" => {
                        ct.members.push(SequenceMember::Element(Box::new(
                            make_element_from_empty(e),
                        )));
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
