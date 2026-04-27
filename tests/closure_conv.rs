//! Tests for `ClosureConversionPass` (PR 2).
//!
//! Each microcommit adds a snapshot test for the new behaviour it
//! introduces; together they document the cumulative shape of the
//! converted IR.

#![allow(clippy::expect_used)]

use formalang::ir::ClosureConversionPass;
use formalang::{compile_to_ir, IrPass};

/// mc3 — synthesizes one capture-environment struct per closure.
///
/// The fixture defines two closures: `make_adder` returning a closure
/// that captures `n`, and a module-level `format_tag` whose body has
/// no captures. After the pass, two `__ClosureEnv<N>` structs should
/// exist in the module.
#[test]
fn mc3_synthesizes_capture_env_structs() {
    let source = r"
        pub fn make_adder(sink n: I32) -> (I32) -> I32 {
            |x: I32| x + n
        }

        let format_tag: String -> String = |t: String| t
    ";
    let module = compile_to_ir(source).expect("should compile to IR");
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
