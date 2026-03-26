use std::path::Path;
use xsd_to_rs::codegen::CodeGenerator;
use xsd_to_rs::parser::parse_xsd;

fn generate(fixture: &str) -> String {
    let xsd = parse_xsd(Path::new(fixture)).unwrap();
    let mut gen = CodeGenerator::new();
    gen.generate(&xsd, &[]);
    gen.output
}

#[test]
fn generates_enum_for_enumeration() {
    let output = generate("tests/fixtures/simple.xsd");
    assert!(output.contains("pub enum FilingStatusType {"));
    assert!(output.contains("#[serde(rename = \"Single\")]"));
    assert!(output.contains("Single,"));
    assert!(output.contains("MarriedFilingJointly,"));
    assert!(output.contains("HeadOfHousehold,"));
}

#[test]
fn generates_newtype_for_restricted_simple_type() {
    let output = generate("tests/fixtures/simple.xsd");
    assert!(output.contains("pub struct SSNType(pub String);"));
    assert!(output.contains("pub struct AmountType(pub f64);"));
}

#[test]
fn generates_struct_for_complex_type() {
    let output = generate("tests/fixtures/simple.xsd");
    assert!(output.contains("pub struct PersonType {"));
    assert!(output.contains("pub first_name: String,"));
    assert!(output.contains("pub last_name: String,"));
    assert!(output.contains("pub ssn: SSNType,"));
    assert!(output.contains("pub middle_initial: Option<String>,"));
}

#[test]
fn generates_optional_and_vec_fields() {
    let output = generate("tests/fixtures/simple.xsd");
    assert!(output.contains("pub dependents: Vec<PersonType>,"));
}

#[test]
fn generates_choice_enum() {
    let output = generate("tests/fixtures/simple.xsd");
    assert!(output.contains("pub enum FormTypeChoice0 {"));
    assert!(output.contains("DirectDeposit(String),"));
    assert!(output.contains("MailCheck(String),"));
}

#[test]
fn generates_attribute_field() {
    let output = generate("tests/fixtures/simple.xsd");
    assert!(output.contains("#[serde(rename = \"@version\")]"));
    assert!(output.contains("pub version: String,"));
}

#[test]
fn generates_ref_elements() {
    let output = generate("tests/fixtures/refs.xsd");
    assert!(output.contains("pub struct Envelope {"));
    assert!(output.contains("pub header: Option<Header>,"));
    assert!(output.contains("pub body: Body,"));
}

#[test]
fn generates_serde_derive() {
    let output = generate("tests/fixtures/simple.xsd");
    assert!(output.contains("use serde::{Deserialize, Serialize};"));
    assert!(output.contains("#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]"));
}

#[test]
fn generates_use_imports_when_provided() {
    let xsd = parse_xsd(Path::new("tests/fixtures/simple.xsd")).unwrap();
    let mut gen = CodeGenerator::new();
    gen.generate(&xsd, &["use crate::common::efiletypes::*;".into()]);
    assert!(gen.output.contains("use crate::common::efiletypes::*;"));
}
