use super::helpers::{depth_zero_colon_index, has_depth_zero_colon, strip_array_type};

#[test]
fn test_strip_array_type_simple() {
    assert_eq!(strip_array_type("[String]"), Some("String"));
    assert_eq!(strip_array_type("[I32]"), Some("I32"));
}

#[test]
fn test_strip_array_type_nested_array() {
    // nested array used to be misclassified because the
    // outer `contains(':')` check fired on the inner dict's colon.
    assert_eq!(strip_array_type("[[I32]]"), Some("[I32]"));
    assert_eq!(strip_array_type("[[String: I32]]"), Some("[String: I32]"));
}

#[test]
fn test_strip_array_type_rejects_dict() {
    assert_eq!(strip_array_type("[String: I32]"), None);
    assert_eq!(strip_array_type("[K: V]"), None);
}

#[test]
fn test_strip_array_type_rejects_non_array() {
    assert_eq!(strip_array_type("String"), None);
    assert_eq!(strip_array_type("(x: I32)"), None);
}

#[test]
fn test_depth_zero_colon_index_basics() {
    assert_eq!(depth_zero_colon_index("a: b"), Some(1));
    assert_eq!(depth_zero_colon_index("[x: y]: v"), Some(6));
    assert_eq!(depth_zero_colon_index("(x: y, z: w)"), None);
    assert_eq!(depth_zero_colon_index("just text"), None);
}

#[test]
fn test_has_depth_zero_colon() {
    assert!(has_depth_zero_colon("a: b"));
    assert!(!has_depth_zero_colon("[a: b]"));
    assert!(has_depth_zero_colon("[a]: [b: c]"));
}
