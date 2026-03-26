pub(crate) fn sanitize_type_name(name: &str) -> String {
    if is_rust_primitive(name) {
        return name.to_string();
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {
            let mut s = c.to_uppercase().to_string();
            s.extend(chars);
            s
        }
        _ => name.to_string(),
    }
}

pub(crate) fn xsd_base_to_rust(base: &str) -> &str {
    let base = base.rsplit_once(':').map(|(_, l)| l).unwrap_or(base);
    match base {
        "string" | "normalizedString" | "token" | "anyURI" | "NMTOKEN" | "NMTOKENS" | "Name"
        | "NCName" | "QName" | "ID" | "IDREF" | "language" => "String",
        "boolean" => "bool",
        "integer" | "int" | "long" | "nonNegativeInteger" | "positiveInteger"
        | "negativeInteger" | "nonPositiveInteger" | "short" | "unsignedInt" | "unsignedLong"
        | "unsignedShort" | "byte" | "unsignedByte" => "i64",
        "decimal" | "float" | "double" => "f64",
        "date" | "dateTime" | "time" | "gYear" | "gYearMonth" | "gMonthDay" | "gMonth" | "gDay"
        | "duration" => "String",
        "base64Binary" | "hexBinary" => "String",
        _ => base,
    }
}

pub(crate) fn is_rust_primitive(ty: &str) -> bool {
    matches!(ty, "String" | "bool" | "i64" | "f64")
}

pub(super) fn resolve_type(ty: &str) -> &str {
    ty.rsplit_once(':').map(|(_, l)| l).unwrap_or(ty)
}

pub(crate) fn enum_variant_name(val: &str) -> String {
    if val
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        return format!("V{}", val.replace(['-', '.', ' ', '/'], "_"));
    }
    let cleaned = val
        .replace([' ', '-', '.', '/', '(', ')'], "_")
        .replace("__", "_");
    let camel = heck::ToUpperCamelCase::to_upper_camel_case(cleaned.as_str());
    if camel.is_empty() {
        format!("V{val}")
    } else {
        camel
    }
}
