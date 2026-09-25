//! Constant folding and the width of the declared type.
//!
//! `ConstantFoldingPass` computes each integer operation in `i128` and
//! each float operation in `f64`. Its own contract, at
//! `src/ir/fold/ops.rs`, says: "overflow leaves the `BinaryOp` unfolded
//! so codegen can decide". The overflow that counts is the overflow of
//! the declared type (`I32`, `I64`, `F32`), not the overflow of `i128`.
//!
//! The tests below check three things after the pass:
//!
//! - each numeric literal in the IR fits its type, as the semantic pass
//!   requires of each literal in the source;
//! - a comparison that reads an overflowed value does not fold to an
//!   answer that no backend of that width can give;
//! - an `F32` operation folds to the `F32` answer, not the `F64` answer.
//!
//! The docs leave the result of an overflow to the backend, so no test
//! here says what the wrapped value is. A test says only that the fold
//! must not decide it with the wrong width.

#![expect(
    clippy::panic,
    clippy::indexing_slicing,
    clippy::option_if_let_else,
    reason = "a test reports its own failure by failing loudly; a JSON index \
              that misses gives `Null`, not a panic"
)]

use formalang::compile_to_ir;
use formalang::ir::ConstantFoldingPass;
use formalang::pipeline::Pipeline;
use formalang::IrModule;
use serde_json::Value as Json;

/// Compile `source`, then run the constant folder on the result.
fn folded(source: &str) -> IrModule {
    let module = match compile_to_ir(source) {
        Ok(module) => module,
        Err(errors) => panic!("the source must compile:\n{source}\n{errors:?}"),
    };
    match Pipeline::new().pass(ConstantFoldingPass::new()).run(module) {
        Ok(module) => module,
        Err(error) => panic!("the fold pass must run:\n{source}\n{error:?}"),
    }
}

/// The JSON form of `module`, which lets one walker read every
/// expression in every context.
fn json(module: &IrModule) -> Json {
    match serde_json::to_value(module) {
        Ok(value) => value,
        Err(error) => panic!("the module must serialise: {error}"),
    }
}

/// Say why a numeric literal does not fit its primitive type, or
/// `None` when it fits.
fn misfit(number: &Json, primitive: &str) -> Option<String> {
    let value = &number["value"];
    let integer = value.get("Integer").and_then(Json::as_i64);
    let float = value.get("Float");
    let describe = || format!("{value} as {primitive}");
    match primitive {
        "I32" => match integer {
            Some(v) if i32::try_from(v).is_ok() => None,
            _ => Some(describe()),
        },
        "I64" => match integer {
            Some(_) => None,
            None => Some(describe()),
        },
        "F32" | "F64" => {
            if value.get("Integer").is_some() {
                return None;
            }
            let Some(v) = float.and_then(Json::as_f64) else {
                // serde_json writes a NaN or an infinity as `null`.
                return Some(format!("{value} (not finite) as {primitive}"));
            };
            if primitive == "F32" && v.abs() > f64::from(f32::MAX) {
                return Some(describe());
            }
            None
        }
        _ => None,
    }
}

/// Walk `node` and collect each numeric literal that does not fit its
/// type.
fn collect_misfits(node: &Json, out: &mut Vec<String>) {
    match node {
        Json::Object(map) => {
            if let Some(literal) = map.get("Literal") {
                let number = &literal["value"]["Number"];
                let primitive = literal["ty"]["Primitive"].as_str();
                if let (false, Some(primitive)) = (number.is_null(), primitive) {
                    if let Some(why) = misfit(number, primitive) {
                        out.push(why);
                    }
                }
            }
            for child in map.values() {
                collect_misfits(child, out);
            }
        }
        Json::Array(items) => {
            for item in items {
                collect_misfits(item, out);
            }
        }
        Json::Null | Json::Bool(_) | Json::Number(_) | Json::String(_) => {}
    }
}

/// Fold each source, and fail with the list of literals that do not
/// fit their type.
fn assert_every_folded_literal_fits(sources: &[&str]) {
    let mut failures = Vec::new();
    for source in sources {
        let mut misfits = Vec::new();
        match serde_json::to_value(folded(source)) {
            Ok(tree) => collect_misfits(&tree, &mut misfits),
            Err(error) => misfits.push(format!("the folded module does not serialise: {error}")),
        }
        if !misfits.is_empty() {
            failures.push(format!("{source}\n    -> {}", misfits.join(", ")));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} program(s) fold to a literal outside its type:\n{}",
        failures.len(),
        sources.len(),
        failures.join("\n")
    );
}

/// The folded value of the top-level `let` called `name`.
fn folded_let(module: &IrModule, name: &str) -> Json {
    let tree = json(module);
    let Some(lets) = tree["lets"].as_array() else {
        panic!("the module has no `lets` array");
    };
    match lets.iter().find(|l| l["name"] == name) {
        Some(found) => found["value"].clone(),
        None => panic!("no top-level let called `{name}`"),
    }
}

/// For each `(source, the answer a backend of the right width gives)`,
/// fail if the fold turns `pub let x` into the other Boolean.
fn assert_fold_never_says(cases: &[(&str, bool)]) {
    let mut failures = Vec::new();
    for (source, right) in cases {
        let value = folded_let(&folded(source), "x");
        if let Some(said) = value["Literal"]["value"]["Boolean"].as_bool() {
            if said != *right {
                failures.push(format!(
                    "{source}\n    -> folds to {said}, the answer is {right}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} comparison(s) fold to the wrong answer:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

// I32 ---------------------------------------------------------------

#[test]
fn i32_addition_overflow_does_not_fold_outside_i32() {
    assert_every_folded_literal_fits(&[
        "pub let x: I32 = 2147483647 + 1",
        "pub let x: I32 = 1 + 2147483647",
        "pub let x: I32 = 2147483647 + 2147483647",
        "pub let x: I32 = -2147483647 + -2",
    ]);
}

#[test]
fn i32_subtraction_overflow_does_not_fold_outside_i32() {
    assert_every_folded_literal_fits(&[
        "pub let x: I32 = -2147483647 - 2",
        "pub let x: I32 = 0 - 2147483647 - 2",
        "pub let x: I32 = 2147483647 - -1",
    ]);
}

#[test]
fn i32_multiplication_overflow_does_not_fold_outside_i32() {
    assert_every_folded_literal_fits(&[
        "pub let x: I32 = 65536 * 65536",
        "pub let x: I32 = 2147483647 * 2",
        "pub let x: I32 = -2147483647 * 3",
        "pub let x: I32 = 46341 * 46341",
    ]);
}

#[test]
fn i32_minimum_divided_by_minus_one_does_not_fold_outside_i32() {
    assert_every_folded_literal_fits(&["pub let x: I32 = (-2147483647 - 1) / -1"]);
}

#[test]
fn i32_negation_of_the_minimum_does_not_fold_outside_i32() {
    assert_every_folded_literal_fits(&["pub let x: I32 = -(-2147483647 - 1)"]);
}

/// The fold must not put the wrong width into any context: a return,
/// a local, a field default, an argument, an element, a closure, a
/// branch, a match arm.
#[test]
fn i32_overflow_does_not_fold_outside_i32_in_any_context() {
    assert_every_folded_literal_fits(&[
        "pub fn f() -> I32 { 2147483647 + 1 }",
        "pub fn f() -> I32 {\n    let a = 2147483647 + 1\n    a\n}",
        "pub struct S { v: I32 = 2147483647 + 1 }",
        "fn g(v: I32) -> I32 { v }\npub fn f() -> I32 { g(v: 2147483647 + 1) }",
        "pub let x: [I32] = [2147483647 + 1]",
        "pub let x: [String: I32] = [\"k\": 2147483647 + 1]",
        "pub fn f() -> I32 {\n    let c: (I32) -> I32 = (v) -> v + 2147483647 + 1\n    c(0)\n}",
        "pub fn f(b: Boolean) -> I32 { if b { 2147483647 + 1 } else { 0 } }",
        "pub fn f(v: I32?) -> I32 {\n    if let n = v { n } else { 2147483647 + 1 }\n}",
        "fn g(v: I32 = 2147483647 + 1) -> I32 { v }\npub fn f() -> I32 { g() }",
        "pub let x: (a: I32, b: I32) = (a: 2147483647 + 1, b: 0)",
    ]);
}

/// `2147483647 + 1` is not greater than `2147483647` in any 32-bit
/// arithmetic: a wrapping backend gives the lowest I32, a trapping
/// backend stops. The fold must not say `true`.
#[test]
fn i32_comparison_after_overflow_does_not_fold_to_an_impossible_answer() {
    assert_fold_never_says(&[
        ("pub let x: Boolean = 2147483647 + 1 > 2147483647", false),
        ("pub let x: Boolean = 2147483647 + 1 > 0", false),
        ("pub let x: Boolean = -2147483647 - 2 < 0", false),
        ("pub let x: Boolean = 65536 * 65536 != 0", false),
        ("pub let x: Boolean = 65536 * 65536 > 65536", false),
    ]);
}

/// A control: the boundaries that fit still fold.
#[test]
fn i32_results_at_the_boundary_still_fold() {
    for (source, want) in [
        ("pub let x: I32 = 2147483646 + 1", 2_147_483_647_i64),
        ("pub let x: I32 = -2147483647 - 1", -2_147_483_648),
        ("pub let x: I32 = 46340 * 46340", 2_147_395_600),
    ] {
        let value = folded_let(&folded(source), "x");
        assert_eq!(
            value["Literal"]["value"]["Number"]["value"]["Integer"].as_i64(),
            Some(want),
            "{source} must fold to {want}, got {value}"
        );
    }
}

// I64 ---------------------------------------------------------------

#[test]
fn i64_addition_overflow_does_not_fold_outside_i64() {
    assert_every_folded_literal_fits(&[
        "pub let x: I64 = 9223372036854775807I64 + 1I64",
        "pub let x: I64 = 9223372036854775807I64 + 9223372036854775807I64",
    ]);
}

#[test]
fn i64_subtraction_overflow_does_not_fold_outside_i64() {
    assert_every_folded_literal_fits(&[
        "pub let x: I64 = -9223372036854775807I64 - 2I64",
        "pub let x: I64 = 9223372036854775807I64 - -1I64",
    ]);
}

#[test]
fn i64_multiplication_overflow_does_not_fold_outside_i64() {
    assert_every_folded_literal_fits(&[
        "pub let x: I64 = 4611686018427387904I64 * 2I64",
        "pub let x: I64 = 3037000500I64 * 3037000500I64",
    ]);
}

#[test]
fn i64_minimum_divided_by_minus_one_does_not_fold_outside_i64() {
    assert_every_folded_literal_fits(&[
        "pub let x: I64 = (-9223372036854775807I64 - 1I64) / -1I64",
    ]);
}

#[test]
fn i64_negation_of_the_minimum_does_not_fold_outside_i64() {
    assert_every_folded_literal_fits(&["pub let x: I64 = -(-9223372036854775807I64 - 1I64)"]);
}

#[test]
fn i64_comparison_after_overflow_does_not_fold_to_an_impossible_answer() {
    assert_fold_never_says(&[
        (
            "pub let x: Boolean = 9223372036854775807I64 + 1I64 > 9223372036854775807I64",
            false,
        ),
        (
            "pub let x: Boolean = 4611686018427387904I64 * 2I64 > 0I64",
            false,
        ),
    ]);
}

// F32 and F64 -------------------------------------------------------

/// Read `text` as a binary32 value, rounded to nearest.
fn f32_of(text: &str) -> f32 {
    match text.parse() {
        Ok(value) => value,
        Err(error) => panic!("{text} must read as an f32: {error}"),
    }
}

/// IEEE 754 equality of two binary32 values.
fn f32_equal(a: f32, b: f32) -> bool {
    a.partial_cmp(&b) == Some(std::cmp::Ordering::Equal)
}

/// The fold must give the `F32` answer to an `F32` question. Each
/// expected answer is computed below with Rust's `f32`, which is IEEE
/// 754 binary32, as `docs/user/types.md` specifies.
#[test]
fn f32_comparisons_fold_with_f32_arithmetic() {
    let sum_is_point_three = f32_equal(f32_of("0.1") + f32_of("0.2"), f32_of("0.3"));
    let same_after_rounding = f32_equal(f32_of("16777217"), f32_of("16777216"));
    let big = f32_of("16777216");
    let low_bit_lost = f32_equal(big + f32_of("1"), big);
    assert!(sum_is_point_three && same_after_rounding && low_bit_lost);

    assert_fold_never_says(&[
        (
            "pub let x: Boolean = 0.1F32 + 0.2F32 == 0.3F32",
            sum_is_point_three,
        ),
        (
            "pub let x: Boolean = 16777217.0F32 == 16777216.0F32",
            same_after_rounding,
        ),
        (
            "pub let x: Boolean = 16777216.0F32 + 1.0F32 == 16777216.0F32",
            low_bit_lost,
        ),
    ]);
}

#[test]
fn f32_overflow_does_not_fold_outside_f32() {
    assert_every_folded_literal_fits(&[
        "pub let x: F32 = 3.4e38F32 * 10.0F32",
        "pub let x: F32 = 3.4e38F32 + 3.4e38F32",
        "pub let x: F32 = -3.4e38F32 - 3.4e38F32",
    ]);
}

/// The semantic pass refuses `1e400` "rather than becoming an infinity
/// the IR cannot serialise" (`tests/conformance/literals/`). The fold
/// must not make that infinity from finite operands.
#[test]
fn f64_overflow_does_not_fold_to_an_infinity() {
    assert_every_folded_literal_fits(&[
        "pub let x: F64 = 1e308 * 10.0",
        "pub let x: F64 = 1e308 + 1e308",
        "pub let x: F64 = -1e308 * 10.0",
        "pub let x: F64 = (1e308 * 10.0) - (1e308 * 10.0)",
    ]);
}

/// A backend that reads the IR as JSON must get back the module that
/// the fold wrote.
#[test]
fn folded_float_overflow_round_trips_through_json() {
    let mut failures = Vec::new();
    for source in [
        "pub let x: F64 = 1e308 * 10.0",
        "pub let x: F32 = 3.4e38F32 * 10.0F32",
        "pub fn f() -> F64 { 1e308 + 1e308 }",
    ] {
        let module = folded(source);
        let text = match serde_json::to_string(&module) {
            Ok(text) => text,
            Err(error) => {
                failures.push(format!("{source}: does not serialise: {error}"));
                continue;
            }
        };
        match serde_json::from_str::<IrModule>(&text) {
            Ok(back) => {
                if json(&back) != json(&module) {
                    failures.push(format!("{source}: reads back as another module"));
                }
            }
            Err(error) => failures.push(format!("{source}: does not read back: {error}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
