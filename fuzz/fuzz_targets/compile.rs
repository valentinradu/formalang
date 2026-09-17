//! Full frontend on raw text.
//!
//! Drives lex -> parse -> semantic -> IR lowering, then the codegen
//! pipeline. Raw bytes rarely get past the parser; the `program` target
//! covers the deep phases. This target guards the shallow ones against
//! inputs no grammar generator would write.
#![no_main]

use libfuzzer_sys::fuzz_target;

use formalang::{compile_to_ir_with_resolver, Pipeline};
use formalang_fuzz::MemResolver;

fuzz_target!(|source: &str| {
    // Cap the input: the fuzzer finds quadratic shapes long before it
    // finds bugs, and a 1 MB source is not a useful report.
    if source.len() > 16 * 1024 {
        return;
    }
    let Ok(module) = compile_to_ir_with_resolver(source, MemResolver::new()) else {
        return;
    };
    let _ = Pipeline::for_codegen().run(module);
});
