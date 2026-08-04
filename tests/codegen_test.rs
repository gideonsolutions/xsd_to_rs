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
    assert!(output.contains("Single,"));
    assert!(output.contains("MarriedFilingJointly,"));
    assert!(output.contains("HeadOfHousehold,"));
    // String-restriction enums (de)serialize as their bare string value via
    // explicit impls (not `#[serde(rename)]`), so they emit as text content even
    // in `$value`/`$text` position rather than as a `<Variant/>` element.
    assert!(output.contains("impl Serialize for FilingStatusType {"));
    assert!(output.contains("FilingStatusType::Single => \"Single\","));
    assert!(output.contains("impl<'de> Deserialize<'de> for FilingStatusType {"));
    assert!(output.contains("\"Single\" => Ok(FilingStatusType::Single),"));
}

#[test]
fn generates_newtype_for_restricted_simple_type() {
    let output = generate("tests/fixtures/simple.xsd");
    assert!(output.contains("pub struct SSNType(pub String);"));
    assert!(output.contains("pub struct AmountType(pub Decimal);"));
}

#[test]
fn generates_doc_comments_for_types() {
    let output = generate("tests/fixtures/simple.xsd");
    assert!(output.contains("/// The filing status for a tax return."));
    assert!(output.contains("/// Social Security Number (9 digits)."));
    assert!(output.contains("/// Represents a person (filer or dependent)."));
}

#[test]
fn generates_doc_comments_for_enum_variants() {
    let output = generate("tests/fixtures/simple.xsd");
    assert!(output.contains("    /// Unmarried individual\n    Single,"));
}

#[test]
fn generates_doc_comments_for_fields() {
    let output = generate("tests/fixtures/simple.xsd");
    assert!(
        output.contains("    /// The person's first name.\n    #[serde(rename = \"FirstName\")]")
    );
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

#[test]
fn composite_choice_branches_sharing_an_element_declare_it_once() {
    // Branches of a composite choice are mutually exclusive, so the same
    // element may appear in several of them (IRS Form 8858 Schedule M repeats
    // its columns across transaction categories). Flattening must collapse
    // them: declaring the field per branch is a duplicate-field compile error.
    let output = generate("tests/fixtures/composite_choice.xsd");
    assert_eq!(
        output.matches("pub us_filer_grp:").count(),
        1,
        "shared branch element declared more than once\n{output}"
    );
    assert_eq!(
        output.matches("pub foreign_corp_grp:").count(),
        1,
        "shared branch element declared more than once\n{output}"
    );
    // The branch-discriminating elements are still each emitted, and every
    // flattened field is optional because another branch may have been chosen.
    assert!(output.contains("pub category_one_ind: Option<"), "{output}");
    assert!(output.contains("pub category_two_ind: Option<"), "{output}");
    assert!(output.contains("pub us_filer_grp: Option<"), "{output}");
}
