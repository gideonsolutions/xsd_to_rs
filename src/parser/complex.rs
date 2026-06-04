use anyhow::Result;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use super::element::{make_attribute, make_element_from_empty, parse_element_start};
use super::{get_attr, local_name_end, local_name_owned};
use crate::types::{AttributeDef, ChoiceGroup, ComplexTypeDef, SequenceMember};

fn strip_ns(s: &str) -> &str {
    s.rsplit_once(':').map(|(_, l)| l).unwrap_or(s)
}

pub(super) fn parse_complex_type(reader: &mut Reader<&[u8]>, name: &str) -> Result<ComplexTypeDef> {
    let mut ct = ComplexTypeDef {
        name: name.to_string(),
        members: Vec::new(),
        attributes: Vec::new(),
        attribute_group_refs: Vec::new(),
        base_type: None,
        simple_content: false,
        doc: None,
    };

    let mut buf = Vec::new();
    let mut depth = 1i32;
    let mut in_doc = false;
    let mut in_simple_content = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let local = local_name_owned(e);
                match local.as_str() {
                    "documentation" => {
                        in_doc = true;
                    }
                    "simpleContent" => {
                        in_simple_content = true;
                    }
                    "extension" => {
                        if let Some(base) = get_attr(e, "base") {
                            ct.base_type = Some(base);
                        }
                        if in_simple_content {
                            ct.simple_content = true;
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
                    // `<xsd:group ref>` with an annotation child. Record the
                    // ref; depth bookkeeping consumes the annotation subtree.
                    "group" => {
                        if let Some(r) = get_attr(e, "ref") {
                            let min_occurs = get_attr(e, "minOccurs")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(1);
                            ct.members.push(SequenceMember::GroupRef {
                                name: strip_ns(&r).to_string(),
                                min_occurs,
                            });
                        }
                    }
                    "attribute" => {
                        ct.attributes.push(make_attribute(e));
                    }
                    "attributeGroup" => {
                        if let Some(r) = get_attr(e, "ref") {
                            ct.attribute_group_refs.push(strip_ns(&r).to_string());
                        }
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
                    "group" => {
                        if let Some(r) = get_attr(e, "ref") {
                            let min_occurs = get_attr(e, "minOccurs")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(1);
                            ct.members.push(SequenceMember::GroupRef {
                                name: strip_ns(&r).to_string(),
                                min_occurs,
                            });
                        }
                    }
                    "attribute" => {
                        ct.attributes.push(make_attribute(e));
                    }
                    "attributeGroup" => {
                        if let Some(r) = get_attr(e, "ref") {
                            ct.attribute_group_refs.push(strip_ns(&r).to_string());
                        }
                    }
                    "extension" => {
                        if let Some(base) = get_attr(e, "base") {
                            ct.base_type = Some(base);
                        }
                        if in_simple_content {
                            ct.simple_content = true;
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref t)) => {
                if in_doc {
                    let text = t.unescape().unwrap_or_default().trim().to_string();
                    if !text.is_empty() {
                        ct.doc = Some(text);
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let local = local_name_end(e);
                if local == "documentation" {
                    in_doc = false;
                } else if local == "simpleContent" {
                    in_simple_content = false;
                }
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

/// Parse the body of a top-level `<xsd:attributeGroup name="...">`, collecting
/// its `<xsd:attribute>` children. The enclosing start tag has already been
/// consumed by the caller. Nested `<xsd:attributeGroup ref>`s inside a group
/// definition are not expanded (none of the IRS groups use them); a warning is
/// emitted if one is seen.
pub(super) fn parse_attribute_group(reader: &mut Reader<&[u8]>) -> Result<Vec<AttributeDef>> {
    let mut attributes = Vec::new();
    let mut buf = Vec::new();
    let mut depth = 1i32;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                match local_name_owned(e).as_str() {
                    // Start-form attribute (carries an annotation child); its
                    // facts live on the start tag, so capture and let depth
                    // bookkeeping skip the annotation subtree.
                    "attribute" => attributes.push(make_attribute(e)),
                    "attributeGroup" => {
                        if let Some(r) = get_attr(e, "ref") {
                            eprintln!(
                                "warning: nested attributeGroup ref `{}` in a group definition is not expanded",
                                strip_ns(&r)
                            );
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                if local_name_owned(e) == "attribute" {
                    attributes.push(make_attribute(e));
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

    Ok(attributes)
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
