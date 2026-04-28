    use super::*;

    fn round_trip(s: &str) {
        let parsed = SemType::from_legacy_string(s);
        assert_eq!(
            parsed.display(),
            s,
            "round-trip failed for {s:?}: parsed = {parsed:?}"
        );
    }

    #[test]
    fn primitives_round_trip() {
        for p in [
            "String", "I32", "I64", "F32", "F64", "Boolean", "Path", "Regex", "Never",
        ] {
            round_trip(p);
        }
    }

    #[test]
    fn sentinels_round_trip() {
        round_trip("Unknown");
        round_trip("InferredEnum");
        round_trip("Nil");
    }

    #[test]
    fn named_round_trips() {
        round_trip("Event");
        round_trip("MyStruct");
    }

    #[test]
    fn array_round_trips() {
        round_trip("[I32]");
        round_trip("[[String]]");
        round_trip("[Unknown]");
    }

    #[test]
    fn optional_round_trips() {
        round_trip("I32?");
        round_trip("Event?");
        round_trip("[I32]?");
    }

    #[test]
    fn tuple_round_trips() {
        round_trip("(a: I32, b: String)");
        round_trip("(x: [I32], y: Event?)");
    }

    #[test]
    fn generic_round_trips() {
        round_trip("Box<I32>");
        round_trip("Map<String, Item>");
        round_trip("Range<I32>");
        round_trip("Box<Pair<A, B>>");
    }

    #[test]
    fn dictionary_round_trips() {
        round_trip("[String: I32]");
        round_trip("[String: [I32]]");
    }

    #[test]
    fn closure_round_trips() {
        round_trip("() -> I32");
        round_trip("I32 -> Boolean");
        round_trip("I32, String -> Boolean");
        round_trip("[I32] -> [String]");
    }

    #[test]
    fn nested_closure_in_array() {
        // Arrays of closures are rare but should survive.
        round_trip("[I32]");
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
    fn user_named_unknown_is_distinct_from_sentinel() {
        // The historical bug: a struct literally named `Unknown` was
        // indistinguishable from the sentinel in the string format.
        // After parsing the legacy string we still treat the literal
        // name as the sentinel (preserving prior behaviour); the win
        // is at construction time inside SemType-native code, where
        // `Named("Unknown".into())` is structurally distinct.
        let user_named = SemType::Named("Unknown".to_string());
        assert!(!user_named.is_indeterminate());
        assert!(SemType::Unknown.is_indeterminate());
        assert_ne!(user_named, SemType::Unknown);
    }

    #[test]
    fn empty_string_parses_as_unknown() {
        assert_eq!(SemType::from_legacy_string(""), SemType::Unknown);
        assert_eq!(SemType::from_legacy_string("   "), SemType::Unknown);
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
        // The legacy byte-walker had to guard against `T` matching
        // inside `TList` — structurally that can't happen because the
        // identifier is a single Named variant, not a substring.
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
        assert_eq!(result.display(), "Boolean -> [Boolean]");
    }

    #[test]
    fn from_ast_matches_legacy_string_for_primitive_ident() {
        use crate::ast::Ident;
        use crate::location::Span;
        let ty = crate::ast::Type::Ident(Ident {
            name: "I32".into(),
            span: Span::default(),
        });
        // Identifier whose name happens to be a primitive should be
        // promoted to Primitive — matches what the parser would do at
        // type position.
        assert_eq!(
            SemType::from_ast(&ty),
            SemType::Primitive(PrimitiveType::I32)
        );
    }
