//! Parser totality.
//!
//! `parse_only` either returns an AST or a non-empty error list. It must
//! never panic, and it must never report an empty error list.
#![no_main]

use libfuzzer_sys::fuzz_target;

use formalang::parse_only;

fuzz_target!(|source: &str| {
    match parse_only(source) {
        Ok(file) => {
            // A successfully parsed file must round-trip through serde.
            let json = serde_json::to_string(&file).expect("AST must serialise");
            match serde_json::from_str::<formalang::File>(&json) {
                Ok(back) => assert_eq!(file, back, "AST round-trip changed the tree"),
                Err(error) => {
                    // `serde_json` refuses to read past 128 nested
                    // levels, and the AST reaches that at roughly 37
                    // nested array literals or 58 nested operators.
                    // That limit is the reader's, not a defect in the
                    // tree, and `tests/ast_serde_depth.rs` pins where
                    // it falls. Anything else is a real failure.
                    assert!(
                        error.to_string().contains("recursion limit exceeded"),
                        "AST failed to deserialise: {error}"
                    );
                }
            }
        }
        Err(errors) => {
            assert!(
                !errors.is_empty(),
                "parse failed with an empty error list for {source:?}"
            );
        }
    }
});
