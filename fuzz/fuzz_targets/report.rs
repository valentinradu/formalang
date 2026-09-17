//! The diagnostic renderer.
//!
//! `report_errors` draws a source snippet around every error span. A
//! span that does not line up with the source — a stale span, a span
//! from the prelude, a span whose offsets fall inside a multi-byte
//! character — must still render, not panic.
//!
//! The input is split: the first half becomes the source that is
//! compiled, the second half becomes the source the errors are
//! rendered against. The mismatch is the point.
#![no_main]

use libfuzzer_sys::fuzz_target;

use formalang::{compile_to_ir_with_resolver, report_error, report_errors};
use formalang_fuzz::MemResolver;

fuzz_target!(|input: (&str, &str)| {
    let (compiled, rendered) = input;
    if compiled.len() > 8 * 1024 || rendered.len() > 8 * 1024 {
        return;
    }

    let Err(errors) = compile_to_ir_with_resolver(compiled, MemResolver::new()) else {
        return;
    };

    // Against the source the errors came from.
    let _ = report_errors(&errors, compiled, "input.fv");
    // And against an unrelated source, which is what an editor does
    // when the buffer moved on before the diagnostics arrived.
    let _ = report_errors(&errors, rendered, "input.fv");
    for error in &errors {
        let _ = report_error(error, rendered, "input.fv");
    }
});
