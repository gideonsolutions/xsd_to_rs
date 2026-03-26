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
}

#[derive(Debug, Clone)]
pub enum MaxOccurs {
    Bounded(u64),
    Unbounded,
}

/// Represents a choice group (maps to Rust enum).
#[derive(Debug, Clone)]
pub struct ChoiceGroup {
    pub min_occurs: u64,
    pub elements: Vec<ElementDef>,
}

/// A member of a complex type's body: either a plain element or a choice.
#[derive(Debug, Clone)]
pub enum SequenceMember {
    Element(Box<ElementDef>),
    Choice(ChoiceGroup),
}

/// Represents an XSD complex type.
#[derive(Debug, Clone)]
pub struct ComplexTypeDef {
    pub name: String,
    pub members: Vec<SequenceMember>,
    pub attributes: Vec<AttributeDef>,
    pub base_type: Option<String>,
    pub doc: Option<String>,
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
    pub includes: Vec<String>,
}
