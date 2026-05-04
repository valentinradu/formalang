use super::*;

#[test]
fn primitive_display_matches_canonical_name() {
    for (sem, expected) in [
        (SemType::Primitive(PrimitiveType::String), "String"),
        (SemType::Primitive(PrimitiveType::I32), "I32"),
        (SemType::Primitive(PrimitiveType::I64), "I64"),
        (SemType::Primitive(PrimitiveType::F32), "F32"),
        (SemType::Primitive(PrimitiveType::F64), "F64"),
        (SemType::Primitive(PrimitiveType::Boolean), "Boolean"),
        (SemType::Primitive(PrimitiveType::Path), "Path"),
        (SemType::Primitive(PrimitiveType::Regex), "Regex"),
        (SemType::Primitive(PrimitiveType::Never), "Never"),
    ] {
        assert_eq!(sem.display(), expected);
    }
}

#[test]
fn sentinel_display_forms() {
    assert_eq!(SemType::Unknown.display(), "Unknown");
    assert_eq!(SemType::InferredEnum.display(), "InferredEnum");
    assert_eq!(SemType::Nil.display(), "Nil");
}

#[test]
fn array_display() {
    let inner = SemType::Primitive(PrimitiveType::I32);
    assert_eq!(SemType::array_of(inner).display(), "[I32]");
}

#[test]
fn optional_display() {
    let inner = SemType::Primitive(PrimitiveType::I32);
    assert_eq!(SemType::optional_of(inner).display(), "I32?");
}

#[test]
fn tuple_display() {
    let t = SemType::Tuple(vec![
        ("a".into(), SemType::Primitive(PrimitiveType::I32)),
        ("b".into(), SemType::Primitive(PrimitiveType::String)),
    ]);
    assert_eq!(t.display(), "(a: I32, b: String)");
}

#[test]
fn generic_display() {
    let t = SemType::Generic {
        base: "Box".into(),
        args: vec![SemType::Primitive(PrimitiveType::I32)],
    };
    assert_eq!(t.display(), "Box<I32>");
}

#[test]
fn dictionary_display() {
    let d = SemType::dictionary(
        SemType::Primitive(PrimitiveType::String),
        SemType::Primitive(PrimitiveType::I32),
    );
    assert_eq!(d.display(), "[String: I32]");
}

#[test]
fn closure_display() {
    let c = SemType::closure(
        vec![SemType::Primitive(PrimitiveType::I32)],
        SemType::Primitive(PrimitiveType::Boolean),
    );
    assert_eq!(c.display(), "(I32) -> Boolean");
}

#[test]
fn unknown_propagates_via_is_indeterminate() {
    assert!(SemType::Unknown.is_indeterminate());
    assert!(SemType::array_of(SemType::Unknown).is_indeterminate());
    assert!(SemType::optional_of(SemType::Unknown).is_indeterminate());
    assert!(!SemType::Primitive(PrimitiveType::I32).is_indeterminate());
    assert!(!SemType::Named("Foo".to_string()).is_indeterminate());
}

#[test]
fn user_named_unknown_is_structurally_distinct_from_sentinel() {
    // Structural representation: a struct literally named `Unknown`
    // never collapses to the sentinel.
    let user_named = SemType::Named("Unknown".to_string());
    assert!(!user_named.is_indeterminate());
    assert!(SemType::Unknown.is_indeterminate());
    assert_ne!(user_named, SemType::Unknown);
}

#[test]
fn optional_of_optional_is_idempotent() {
    let t = SemType::optional_of(SemType::Primitive(PrimitiveType::I32));
    let twice = SemType::optional_of(t.clone());
    assert_eq!(t, twice);
}

#[test]
fn strip_optional_unwraps_one_layer() {
    let t = SemType::optional_of(SemType::Primitive(PrimitiveType::I32));
    assert_eq!(t.strip_optional(), SemType::Primitive(PrimitiveType::I32));
    let bare = SemType::Primitive(PrimitiveType::I32);
    assert_eq!(bare.strip_optional(), bare);
}

#[test]
fn substitute_named_replaces_param_only_at_named_positions() {
    let t = SemType::Generic {
        base: "Box".into(),
        args: vec![SemType::Named("T".into())],
    };
    let result = t.substitute_named("T", &SemType::Primitive(PrimitiveType::I32));
    assert_eq!(result.display(), "Box<I32>");
}

#[test]
fn substitute_named_skips_substring_collisions() {
    // The identifier is a single `Named` variant, so a `T` parameter
    // never matches `TList` by substring.
    let t = SemType::Generic {
        base: "TList".into(),
        args: vec![SemType::Named("T".into())],
    };
    let result = t.substitute_named("T", &SemType::Primitive(PrimitiveType::I32));
    assert_eq!(result.display(), "TList<I32>");
}

#[test]
fn substitute_named_recurses_through_closure() {
    let t = SemType::closure(
        vec![SemType::Named("T".into())],
        SemType::array_of(SemType::Named("T".into())),
    );
    let result = t.substitute_named("T", &SemType::Primitive(PrimitiveType::Boolean));
    assert_eq!(result.display(), "(Boolean) -> [Boolean]");
}

#[test]
fn from_ast_promotes_primitive_named_idents() {
    use crate::ast::Ident;
    use crate::location::Span;
    let ty = crate::ast::Type::Ident(Ident {
        name: "I32".into(),
        span: Span::default(),
    });
    assert_eq!(
        SemType::from_ast(&ty),
        SemType::Primitive(PrimitiveType::I32)
    );
}
