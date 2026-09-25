//! Parser totality.
//!
//! `parse_only` either returns an AST or a non-empty error list. It must
//! never panic, and it must never report an empty error list.
#![no_main]

use libfuzzer_sys::fuzz_target;

use formalang::parse_only;

fuzz_target!(|source: &str| {
    match parse_only(source) {
        Ok(_) => {}
        Err(errors) => {
            assert!(
                !errors.is_empty(),
                "parse failed with an empty error list for {source:?}"
            );
        }
    }
});
