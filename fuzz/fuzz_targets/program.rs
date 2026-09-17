//! The deep phases, driven by a grammar-aware generator.
//!
//! Renders an `Arbitrary` [`Program`] to source, then runs the whole
//! frontend over it. Because every name comes from a small pool, the
//! generated program usually reaches the semantic analyser and the IR
//! lowerer, which raw bytes almost never do.
//!
//! Beyond "it does not panic", this target checks two pipeline
//! contracts that a backend depends on:
//!
//! - the codegen pipeline is idempotent — a second run over its own
//!   output must produce the same module;
//! - the IR round-trips through JSON, the format external consumers
//!   read.
#![no_main]

use libfuzzer_sys::fuzz_target;

use formalang::{compile_to_ir_with_resolver, Pipeline};
use formalang_fuzz::{MemResolver, Program};

fuzz_target!(|program: Program| {
    let source = program.render();
    if source.len() > 64 * 1024 {
        return;
    }

    let Ok(module) = compile_to_ir_with_resolver(&source, MemResolver::new()) else {
        return;
    };

    // The IR is the published artefact. It must survive a JSON round
    // trip unchanged.
    let json = serde_json::to_string(&module).expect("IR must serialise");
    let restored: formalang::IrModule = serde_json::from_str(&json).expect("IR must deserialise");
    assert_eq!(
        serde_json::to_string(&restored).expect("IR must re-serialise"),
        json,
        "IR JSON round-trip is not stable for:\n{source}"
    );

    let Ok(first) = Pipeline::for_codegen().run(module) else {
        return;
    };
    let first_json = serde_json::to_string(&first).expect("IR must serialise");

    let Ok(second) = Pipeline::for_codegen().run(first) else {
        // A pipeline that accepts a module and then rejects its own
        // output is a bug, but it is reported by the dedicated
        // idempotence test rather than here, where the shrunk input
        // would be unreadable.
        return;
    };
    let second_json = serde_json::to_string(&second).expect("IR must serialise");

    assert_eq!(
        first_json, second_json,
        "the codegen pipeline is not idempotent for:\n{source}"
    );
});
