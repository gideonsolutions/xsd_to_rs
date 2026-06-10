/// Represents an XSD simple type (restriction-based).
#[derive(Debug, Clone)]
pub struct SimpleTypeDef {
    pub name: String,
    pub base: String,
    pub enumerations: Vec<(String, Option<String>)>,
    pub pattern: Option<String>,
    pub min_length: Option<u64>,
    pub max_length: Option<u64>,
    pub total_digits: Option<u64>,
    pub fraction_digits: Option<u64>,
    pub min_inclusive: Option<String>,
    pub max_inclusive: Option<String>,
    pub doc: Option<String>,
    /// True for an `<xsd:union>` of string types (e.g. `AllCountriesType` =
    /// `CountryType` ∪ `{"US"}`). A union's full value set can't be captured by the
    /// inline restriction alone, so it is emitted as a permissive `String` newtype
    /// rather than a too-narrow enum.
    pub is_union: bool,
}

/// Represents an element inside a complexType sequence.
#[derive(Debug, Clone)]
pub struct ElementDef {
    pub name: String,
    pub type_name: Option<String>,
    pub min_occurs: u64,
    pub max_occurs: MaxOccurs,
    pub doc: Option<String>,
    pub inline_simple_type: Option<SimpleTypeDef>,
    /// An anonymous `<xsd:complexType>` declared directly inside this element.
    /// Hoisted into a named [`ComplexTypeDef`] (see `parser::hoist`) before
    /// codegen, with `type_name` rewritten to point at it.
    pub inline_complex_type: Option<Box<ComplexTypeDef>>,
}

#[derive(Debug, Clone)]
pub enum MaxOccurs {
    Bounded(u64),
    Unbounded,
}

/// Represents a choice group.
///
/// A *simple* choice (every branch is a single element) maps to a Rust enum: each
/// element becomes a variant. A *composite* choice — one whose branches are
/// `<xsd:sequence>`s, so selecting a branch yields more than one co-occurring
/// element (e.g. `BusinessNameControlTxt` + `BusinessName`) — cannot be a single
/// `$value` enum, because two sibling elements would collide. Composite choices are
/// instead flattened to optional fields on the parent struct.
#[derive(Debug, Clone)]
pub struct ChoiceGroup {
    pub min_occurs: u64,
    pub elements: Vec<ElementDef>,
    /// True when any branch is a `<sequence>` (or otherwise structurally grouped),
    /// so the choice must be flattened to optional fields rather than an enum.
    pub composite: bool,
}

/// A member of a complex type's body: a plain element, a choice, or a
/// reference to a named model group (`<xsd:group ref="...">`). Group refs are
/// expanded inline from the cross-file registry at codegen time, so no
/// `GroupRef` survives into the emitted output.
#[derive(Debug, Clone)]
pub enum SequenceMember {
    Element(Box<ElementDef>),
    Choice(ChoiceGroup),
    GroupRef { name: String, min_occurs: u64 },
}

/// Represents an XSD complex type.
#[derive(Debug, Clone)]
pub struct ComplexTypeDef {
    pub name: String,
    pub members: Vec<SequenceMember>,
    pub attributes: Vec<AttributeDef>,
    /// Names of `<xsd:attributeGroup ref="...">` referenced by this type. The
    /// referenced attributes are expanded inline at codegen time from the
    /// cross-file registry built in `directory`.
    pub attribute_group_refs: Vec<String>,
    pub base_type: Option<String>,
    /// True when `base_type` came from `<xsd:simpleContent>` (the base is the
    /// element's text value, emitted as a `$value` field) rather than
    /// `<xsd:complexContent>` (the base is flattened in).
    pub simple_content: bool,
    pub doc: Option<String>,
}

/// A reusable bundle of attributes (`<xsd:attributeGroup name="...">`),
/// referenced from complex types via `ref`.
#[derive(Debug, Clone)]
pub struct AttributeGroupDef {
    pub name: String,
    pub attributes: Vec<AttributeDef>,
}

/// A reusable model group (`<xsd:group name="...">`): an ordered run of
/// elements/choices referenced from complex types via `<xsd:group ref="...">`
/// and expanded inline at codegen time.
#[derive(Debug, Clone)]
pub struct ModelGroupDef {
    pub name: String,
    pub members: Vec<SequenceMember>,
}

/// Represents an attribute on a complex type.
#[derive(Debug, Clone)]
pub struct AttributeDef {
    pub name: String,
    pub type_name: String,
    pub required: bool,
    pub fixed: Option<String>,
}

/// Top-level element declaration.
#[derive(Debug, Clone)]
pub struct TopLevelElement {
    pub name: String,
    pub type_name: Option<String>,
    pub complex_type: Option<ComplexTypeDef>,
}

/// Parsed contents of a single XSD file.
#[derive(Debug, Clone)]
pub struct XsdFile {
    pub path: String,
    pub simple_types: Vec<SimpleTypeDef>,
    pub complex_types: Vec<ComplexTypeDef>,
    pub elements: Vec<TopLevelElement>,
    pub attribute_groups: Vec<AttributeGroupDef>,
    pub model_groups: Vec<ModelGroupDef>,
    pub includes: Vec<String>,
}
