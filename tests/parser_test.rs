use std::path::Path;
use xsd_to_rs::parser::parse_xsd;

#[test]
fn parse_simple_types() {
    let xsd = parse_xsd(Path::new("tests/fixtures/simple.xsd")).unwrap();
    assert_eq!(xsd.simple_types.len(), 3);

    let filing = &xsd.simple_types[0];
    assert_eq!(filing.name, "FilingStatusType");
    assert_eq!(filing.enumerations.len(), 3);
    assert_eq!(filing.enumerations[0].0, "Single");

    let ssn = &xsd.simple_types[1];
    assert_eq!(ssn.name, "SSNType");
    assert_eq!(ssn.pattern, Some("[0-9]{9}".to_string()));

    let amount = &xsd.simple_types[2];
    assert_eq!(amount.name, "AmountType");
    assert_eq!(amount.total_digits, Some(12));
    assert_eq!(amount.fraction_digits, Some(2));
}

#[test]
fn parse_complex_types() {
    let xsd = parse_xsd(Path::new("tests/fixtures/simple.xsd")).unwrap();
    assert_eq!(xsd.complex_types.len(), 2);

    let person = &xsd.complex_types[0];
    assert_eq!(person.name, "PersonType");
    assert_eq!(person.members.len(), 4);

    let form = &xsd.complex_types[1];
    assert_eq!(form.name, "FormType");
    assert_eq!(form.attributes.len(), 1);
    assert_eq!(form.attributes[0].name, "version");
    assert!(form.attributes[0].required);
}

#[test]
fn parse_ref_elements() {
    let xsd = parse_xsd(Path::new("tests/fixtures/refs.xsd")).unwrap();

    let envelope = &xsd.complex_types[0];
    assert_eq!(envelope.name, "Envelope");
    assert_eq!(envelope.members.len(), 2);
}

#[test]
fn parse_list_type() {
    let xsd = parse_xsd(Path::new("tests/fixtures/refs.xsd")).unwrap();
    let list_type = &xsd.simple_types[0];
    assert_eq!(list_type.name, "ListType");
    assert_eq!(list_type.base, "string");
}

#[test]
fn parse_includes() {
    let xsd = parse_xsd(Path::new("tests/fixtures/simple.xsd")).unwrap();
    assert_eq!(xsd.includes.len(), 0);
}
