mod complex;
mod element;
mod simple;

use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::path::Path;

use complex::parse_complex_type;
use element::parse_top_level_element;
use simple::parse_simple_type;

use crate::types::{TopLevelElement, XsdFile};

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
