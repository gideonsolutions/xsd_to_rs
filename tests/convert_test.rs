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
