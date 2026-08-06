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

/// Pruned conversion emits only the schemas reachable from the given roots:
/// the root, its transitive includes — and not the orphan sitting beside them
/// (an IRS MeF package ships the whole common-dependency forest; most of it is
/// unreachable from the form's returns).
#[test]
fn convert_directory_pruned_drops_unreachable_schemas() {
    let src = TempDir::new().unwrap();
    std::fs::create_dir_all(src.path().join("deps")).unwrap();
    std::fs::write(
        src.path().join("deps/base.xsd"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema">
  <xsd:complexType name="BaseType">
    <xsd:sequence><xsd:element name="Name" type="xsd:string"/></xsd:sequence>
  </xsd:complexType>
</xsd:schema>"#,
    )
    .unwrap();
    std::fs::write(
        src.path().join("Return.xsd"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema">
  <xsd:include schemaLocation="deps/base.xsd"/>
  <xsd:complexType name="ReturnType">
    <xsd:sequence><xsd:element name="Base" type="BaseType"/></xsd:sequence>
  </xsd:complexType>
</xsd:schema>"#,
    )
    .unwrap();
    // Unreachable from Return.xsd: must not be emitted.
    std::fs::write(
        src.path().join("deps/orphan.xsd"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema">
  <xsd:complexType name="OrphanType">
    <xsd:sequence><xsd:element name="Junk" type="xsd:string"/></xsd:sequence>
  </xsd:complexType>
</xsd:schema>"#,
    )
    .unwrap();

    let out = TempDir::new().unwrap();
    let out_dir = out.path().join("v1");
    xsd_to_rs::directory::convert_directory_pruned(
        src.path(),
        &out_dir,
        Some("v1"),
        &[std::path::PathBuf::from("Return.xsd")],
    )
    .unwrap();

    assert!(out_dir.join("return.rs").exists());
    assert!(out_dir.join("deps/base.rs").exists());
    assert!(
        !out_dir.join("deps/orphan.rs").exists(),
        "orphan.xsd is unreachable from the root and must be pruned"
    );
    // The module tree only declares what was emitted.
    let deps_mod = std::fs::read_to_string(out_dir.join("deps/mod.rs")).unwrap();
    assert!(deps_mod.contains("pub mod base;"));
    assert!(!deps_mod.contains("orphan"));

    // A missing root is an error, not an empty tree.
    let err = xsd_to_rs::directory::convert_directory_pruned(
        src.path(),
        &out.path().join("v2"),
        Some("v2"),
        &[std::path::PathBuf::from("NoSuch.xsd")],
    );
    assert!(err.is_err());
}

/// Layered conversion: each schema lands in its owning layer's crate, imports
/// within a layer stay `crate::…`, imports of a lower layer go through the
/// lower crate's ident, each layer gets a lib.rs — and a lower layer including
/// a higher one is a hard error, not a crate that fails to compile later.
#[test]
fn convert_directory_layered_stacks_crates() {
    use std::path::PathBuf;
    use xsd_to_rs::directory::{convert_directory_layered, Layer};

    let src = TempDir::new().unwrap();
    std::fs::create_dir_all(src.path().join("Common")).unwrap();
    std::fs::create_dir_all(src.path().join("Family")).unwrap();
    std::fs::write(
        src.path().join("Common/base.xsd"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema">
  <xsd:complexType name="BaseType">
    <xsd:sequence><xsd:element name="Name" type="xsd:string"/></xsd:sequence>
  </xsd:complexType>
</xsd:schema>"#,
    )
    .unwrap();
    std::fs::write(
        src.path().join("Family/helper.xsd"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema">
  <xsd:complexType name="HelperType">
    <xsd:sequence><xsd:element name="Tag" type="xsd:string"/></xsd:sequence>
  </xsd:complexType>
</xsd:schema>"#,
    )
    .unwrap();
    std::fs::write(
        src.path().join("Family/Return.xsd"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema">
  <xsd:include schemaLocation="../Common/base.xsd"/>
  <xsd:include schemaLocation="helper.xsd"/>
  <xsd:complexType name="ReturnType">
    <xsd:sequence>
      <xsd:element name="Base" type="BaseType"/>
      <xsd:element name="Helper" type="HelperType"/>
    </xsd:sequence>
  </xsd:complexType>
</xsd:schema>"#,
    )
    .unwrap();

    let out = TempDir::new().unwrap();
    let base_dir = out.path().join("base/src");
    let fam_dir = out.path().join("fam/src");
    convert_directory_layered(
        src.path(),
        &[
            Layer {
                crate_ident: "demo_base",
                out_dir: &base_dir,
                dirs: &["common"],
            },
            Layer {
                crate_ident: "demo_family",
                out_dir: &fam_dir,
                dirs: &["family"],
            },
        ],
        &[PathBuf::from("Family/Return.xsd")],
    )
    .unwrap();

    // Files landed in their layers, each with its own lib.rs.
    assert!(base_dir.join("common/base.rs").exists());
    assert!(base_dir.join("lib.rs").exists());
    assert!(fam_dir.join("family/return.rs").exists());
    assert!(fam_dir.join("lib.rs").exists());
    assert!(!fam_dir.join("common").exists());

    // Same-layer import stays crate::, lower-layer import goes cross-crate.
    let ret = std::fs::read_to_string(fam_dir.join("family/return.rs")).unwrap();
    assert!(ret.contains("use crate::family::helper::*;"), "{ret}");
    assert!(ret.contains("use demo_base::common::base::*;"), "{ret}");

    // A lower layer including a higher one is a layering violation.
    std::fs::write(
        src.path().join("Common/bad.xsd"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema">
  <xsd:include schemaLocation="../Family/helper.xsd"/>
  <xsd:complexType name="BadType">
    <xsd:sequence><xsd:element name="Helper" type="HelperType"/></xsd:sequence>
  </xsd:complexType>
</xsd:schema>"#,
    )
    .unwrap();
    let err = convert_directory_layered(
        src.path(),
        &[
            Layer {
                crate_ident: "demo_base",
                out_dir: &out.path().join("b2/src"),
                dirs: &["common"],
            },
            Layer {
                crate_ident: "demo_family",
                out_dir: &out.path().join("f2/src"),
                dirs: &["family"],
            },
        ],
        &[
            PathBuf::from("Family/Return.xsd"),
            PathBuf::from("Common/bad.xsd"),
        ],
    );
    let msg = format!("{:#}", err.unwrap_err());
    assert!(msg.contains("layering violation"), "{msg}");
}
