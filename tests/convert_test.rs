use std::path::Path;
use tempfile::TempDir;

#[test]
fn convert_single_file() {
    let tmp = TempDir::new().unwrap();
    let out = tmp.path().join("output.rs");

    xsd_to_rs::convert_file(Path::new("tests/fixtures/simple.xsd"), &out, &[]).unwrap();

    let content = std::fs::read_to_string(&out).unwrap();
    assert!(content.contains("pub struct PersonType {"));
    assert!(content.contains("pub enum FilingStatusType {"));
}

#[test]
fn convert_directory_creates_mod_files() {
    let tmp = TempDir::new().unwrap();
    let out_dir = tmp.path().join("out");

    xsd_to_rs::directory::convert_directory(Path::new("tests/fixtures"), &out_dir).unwrap();

    let lib_rs = out_dir.join("lib.rs");
    assert!(lib_rs.exists(), "lib.rs should be generated");

    let content = std::fs::read_to_string(&lib_rs).unwrap();
    assert!(content.contains("pub mod simple;"));
    assert!(content.contains("pub mod refs;"));
}

#[test]
fn mod_prefix_nests_imports_and_writes_mod_rs() {
    let src = TempDir::new().unwrap();
    std::fs::write(
        src.path().join("base.xsd"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema">
  <xsd:complexType name="BaseType">
    <xsd:sequence><xsd:element name="Id" type="xsd:string"/></xsd:sequence>
  </xsd:complexType>
</xsd:schema>"#,
    )
    .unwrap();
    std::fs::write(
        src.path().join("main.xsd"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema">
  <xsd:include schemaLocation="base.xsd"/>
  <xsd:complexType name="MainType">
    <xsd:sequence><xsd:element name="Base" type="BaseType"/></xsd:sequence>
  </xsd:complexType>
</xsd:schema>"#,
    )
    .unwrap();

    let out = TempDir::new().unwrap();
    let out_dir = out.path().join("v2026v1_0");
    xsd_to_rs::directory::convert_directory_with_prefix(src.path(), &out_dir, Some("v2026v1_0"))
        .unwrap();

    // Root module file is mod.rs, not lib.rs, when nested.
    assert!(
        out_dir.join("mod.rs").exists(),
        "nested root should be mod.rs"
    );
    assert!(
        !out_dir.join("lib.rs").exists(),
        "nested root should not be lib.rs"
    );

    // Cross-file imports are rewritten under the module prefix.
    let main_rs = std::fs::read_to_string(out_dir.join("main.rs")).unwrap();
    assert!(
        main_rs.contains("use crate::v2026v1_0::base::*;"),
        "import should be prefixed; got:\n{main_rs}"
    );
}
