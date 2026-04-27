//! Tests for `ClosureConversionPass` (PR 2).
//!
//! Each microcommit adds a snapshot test for the new behaviour it
//! introduces; together they document the cumulative shape of the
//! converted IR.

#![allow(clippy::expect_used)]

use formalang::ir::ClosureConversionPass;
use formalang::{compile_to_ir, IrPass};

/// Standard two-closure fixture used by every microcommit's snapshot
/// test, kept in one place so each snapshot exercises the same input.
const TWO_CLOSURES_SOURCE: &str = r"
    pub fn make_adder(sink n: I32) -> (I32) -> I32 {
        |x: I32| x + n
    }

    let format_tag: String -> String = |t: String| t
";

/// mc3 — synthesizes one capture-environment struct per closure.
///
/// `make_adder` returns a closure that captures `n`; `format_tag` is
/// a module-level closure with no captures. After the pass, two
/// `__ClosureEnv<N>` structs should exist in the module.
#[test]
fn mc3_synthesizes_capture_env_structs() {
    let module = compile_to_ir(TWO_CLOSURES_SOURCE).expect("should compile to IR");
    let original_struct_count = module.structs.len();

    let converted = ClosureConversionPass::new()
        .run(module)
        .expect("closure conversion succeeds");

    let new_structs: Vec<_> = converted
        .structs
        .iter()
        .skip(original_struct_count)
        .collect();
    insta::assert_debug_snapshot!("mc3_capture_env_structs", new_structs);
}

/// mc4 — synthesizes one lifted function per closure, prepended with
/// an `__env` parameter that points at the corresponding env struct.
///
/// The lifted function's body is the closure body verbatim; mc5
/// rewrites captured-name reads to load from `__env` fields.
#[test]
fn mc4_synthesizes_lifted_functions() {
    let module = compile_to_ir(TWO_CLOSURES_SOURCE).expect("should compile to IR");
    let original_function_count = module.functions.len();

    let converted = ClosureConversionPass::new()
        .run(module)
        .expect("closure conversion succeeds");

    let lifted: Vec<_> = converted
        .functions
        .iter()
        .skip(original_function_count)
        .collect();
    insta::assert_debug_snapshot!("mc4_lifted_functions", lifted);
}

/// mc5 — captured-name reads inside lifted bodies become field
/// accesses on `__env`, while parameter-name reads stay as raw
/// references.
///
/// In the `make_adder` closure body `x + n`, `x` is a parameter
/// (untouched) but `n` is a capture and must become
/// `__env.n`. The trivial `format_tag` body `t` references only its
/// own param.
#[test]
fn mc5_rewrites_captured_refs_to_env_field_access() {
    let module = compile_to_ir(TWO_CLOSURES_SOURCE).expect("should compile to IR");
    let original_function_count = module.functions.len();

    let converted = ClosureConversionPass::new()
        .run(module)
        .expect("closure conversion succeeds");

    let lifted_bodies: Vec<_> = converted
        .functions
        .iter()
        .skip(original_function_count)
        .map(|f| f.body.as_ref())
        .collect();
    insta::assert_debug_snapshot!("mc5_lifted_bodies", lifted_bodies);
}

/// mc5 — `let` shadowing: an inner `let` binding with the same name
/// as a capture must mask the env access for references *after* the
/// `let`.
#[test]
fn mc5_let_shadowing_blocks_env_rewrite() {
    let source = r"
        pub fn make(sink n: I32) -> (I32) -> I32 {
            |x: I32| (
                let n: I32 = 100
                in x + n
            )
        }
    ";
    let module = compile_to_ir(source).expect("should compile to IR");
    let original_function_count = module.functions.len();

    let converted = ClosureConversionPass::new()
        .run(module)
        .expect("closure conversion succeeds");

    // The lifted closure's body is a Block whose result `x + n` reads
    // a *local* `n`. The shadowing rule should leave that read as a
    // raw `Reference { path: ["n"] }`, not rewrite it to `__env.n`.
    let lifted_body = converted
        .functions
        .iter()
        .skip(original_function_count)
        .find_map(|f| f.body.as_ref())
        .expect("at least one lifted closure body");
    insta::assert_debug_snapshot!("mc5_let_shadowing", lifted_body);
}
