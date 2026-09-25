//! Integer arithmetic tables from the `WebAssembly` specification.
//!
//! The rows come from `test/core/i32.wast` and `test/core/i64.wast` in
//! `WebAssembly/spec`. Those files give the exact answer of each
//! operation at the edges of the range. The rows here keep only the
//! operations whose answer does not overflow, because
//! `docs/user/types.md` leaves the result of an overflow to the
//! backend. `div_s` and `rem_s` truncate toward zero, which is the rule
//! of every mainstream language.
//!
//! The spec writes the lowest value as `0x80000000`. `FormaLang` has no
//! hex literal, so the rows write `-2147483648`. That is the same I32
//! value.
//!
//! Each row runs twice under the reference interpreter: once as the
//! lowering gives it, and once after `ConstantFoldingPass`. Both runs
//! must hold.

use crate::common::interpreter::Interpreter;
use formalang::compile_to_ir;
use formalang::ir::ConstantFoldingPass;
use formalang::pipeline::Pipeline;

const I32_MIN: &str = "-2147483648";
const I32_MAX: &str = "2147483647";
const I64_MIN: &str = "-9223372036854775808I64";
const I64_MAX: &str = "9223372036854775807I64";

/// Run `pub fn run_checks() { assert(condition: <check>) }` without the
/// fold and with it. Return why it failed, or `None` when both runs
/// hold.
fn failure(check: &str) -> Option<String> {
    let source = format!("pub fn run_checks() {{\n    assert(condition: {check})\n}}\n");
    let module = match compile_to_ir(&source) {
        Ok(module) => module,
        Err(errors) => {
            let names: Vec<String> = errors.iter().map(ToString::to_string).collect();
            return Some(format!("{check}: does not compile: {names:?}"));
        }
    };
    if let Err(fault) = Interpreter::new(&module).run("run_checks") {
        return Some(format!("{check}: fails as lowered: {fault}"));
    }
    let optimised = match Pipeline::new().pass(ConstantFoldingPass::new()).run(module) {
        Ok(module) => module,
        Err(error) => return Some(format!("{check}: the fold pass fails: {error:?}")),
    };
    if let Err(fault) = Interpreter::new(&optimised).run("run_checks") {
        return Some(format!("{check}: fails after the fold: {fault}"));
    }
    None
}

/// Check each `(left, operator, right, answer)` row.
fn assert_table(width: &str, rows: &[(&str, &str, &str, &str)]) {
    let failures: Vec<String> = rows
        .iter()
        .filter_map(|(l, op, r, answer)| failure(&format!("({l}) {op} ({r}) == ({answer})")))
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} {width} row(s) fail:\n{}",
        failures.len(),
        rows.len(),
        failures.join("\n")
    );
}

/// Check each `(left, operator, right, answer)` comparison row.
fn assert_comparisons(width: &str, rows: &[(&str, &str, &str, bool)]) {
    let failures: Vec<String> = rows
        .iter()
        .filter_map(|(l, op, r, answer)| failure(&format!("(({l}) {op} ({r})) == {answer}")))
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} {width} comparison row(s) fail:\n{}",
        failures.len(),
        rows.len(),
        failures.join("\n")
    );
}

#[test]
fn wasm_i32_add_and_sub_rows_hold() {
    assert_table(
        "i32.add/i32.sub",
        &[
            ("1", "+", "1", "2"),
            ("1", "+", "0", "1"),
            ("-1", "+", "-1", "-2"),
            ("-1", "+", "1", "0"),
            (I32_MIN, "+", "0", I32_MIN),
            (I32_MAX, "+", "-1", "2147483646"),
            (I32_MIN, "+", I32_MAX, "-1"),
            ("1", "-", "1", "0"),
            ("1", "-", "0", "1"),
            ("-1", "-", "-1", "0"),
            (I32_MAX, "-", I32_MAX, "0"),
            (I32_MIN, "-", I32_MIN, "0"),
            ("-1", "-", I32_MAX, I32_MIN),
        ],
    );
}

#[test]
fn wasm_i32_mul_rows_hold() {
    assert_table(
        "i32.mul",
        &[
            ("1", "*", "1", "1"),
            ("1", "*", "0", "0"),
            ("-1", "*", "-1", "1"),
            (I32_MIN, "*", "1", I32_MIN),
            (I32_MIN, "*", "0", "0"),
            (I32_MAX, "*", "-1", "-2147483647"),
            ("123456", "*", "1000", "123456000"),
            ("-46340", "*", "46340", "-2147395600"),
        ],
    );
}

#[test]
fn wasm_i32_div_s_rows_hold() {
    assert_table(
        "i32.div_s",
        &[
            ("1", "/", "1", "1"),
            ("0", "/", "1", "0"),
            ("0", "/", "-1", "0"),
            ("-1", "/", "-1", "1"),
            (I32_MIN, "/", "2", "-1073741824"),
            ("-2147483647", "/", "1000", "-2147483"),
            ("5", "/", "2", "2"),
            ("-5", "/", "2", "-2"),
            ("5", "/", "-2", "-2"),
            ("-5", "/", "-2", "2"),
            ("7", "/", "3", "2"),
            ("-7", "/", "3", "-2"),
            ("7", "/", "-3", "-2"),
            ("-7", "/", "-3", "2"),
            ("11", "/", "5", "2"),
            ("17", "/", "7", "2"),
        ],
    );
}

#[test]
fn wasm_i32_rem_s_rows_hold() {
    assert_table(
        "i32.rem_s",
        &[
            (I32_MAX, "%", "-1", "0"),
            ("1", "%", "1", "0"),
            ("0", "%", "1", "0"),
            ("0", "%", "-1", "0"),
            ("-1", "%", "-1", "0"),
            (I32_MIN, "%", "-1", "0"),
            (I32_MIN, "%", "2", "0"),
            ("-2147483647", "%", "1000", "-647"),
            ("5", "%", "2", "1"),
            ("-5", "%", "2", "-1"),
            ("5", "%", "-2", "1"),
            ("-5", "%", "-2", "-1"),
            ("7", "%", "3", "1"),
            ("-7", "%", "3", "-1"),
            ("7", "%", "-3", "1"),
            ("-7", "%", "-3", "-1"),
            ("11", "%", "5", "1"),
            ("17", "%", "7", "3"),
        ],
    );
}

#[test]
fn wasm_i32_signed_comparison_rows_hold() {
    assert_comparisons(
        "i32",
        &[
            (I32_MIN, "<", I32_MIN, false),
            (I32_MAX, "<", I32_MAX, false),
            (I32_MIN, "<", I32_MAX, true),
            (I32_MAX, "<", I32_MIN, false),
            ("-1", "<", I32_MIN, false),
            (I32_MIN, "<", "-1", true),
            ("0", "<", "-1", false),
            ("-1", "<", "0", true),
            (I32_MIN, "<=", I32_MIN, true),
            (I32_MAX, ">=", I32_MIN, true),
            (I32_MIN, ">", "0", false),
            ("0", ">", I32_MIN, true),
            (I32_MIN, "==", I32_MIN, true),
            (I32_MIN, "!=", I32_MAX, true),
            (I32_MIN, "==", "-2147483647", false),
        ],
    );
}

#[test]
fn wasm_i64_add_sub_and_mul_rows_hold() {
    assert_table(
        "i64.add/i64.sub/i64.mul",
        &[
            ("1I64", "+", "1I64", "2I64"),
            (I64_MIN, "+", I64_MAX, "-1I64"),
            (I64_MAX, "+", "-1I64", "9223372036854775806I64"),
            (I64_MIN, "-", I64_MIN, "0I64"),
            ("-1I64", "-", I64_MAX, I64_MIN),
            (I64_MIN, "*", "1I64", I64_MIN),
            (I64_MAX, "*", "-1I64", "-9223372036854775807I64"),
            (
                "1311768467294899695I64",
                "*",
                "2I64",
                "2623536934589799390I64",
            ),
        ],
    );
}

#[test]
fn wasm_i64_div_s_and_rem_s_rows_hold() {
    assert_table(
        "i64.div_s/i64.rem_s",
        &[
            (I64_MIN, "/", "2I64", "-4611686018427387904I64"),
            (
                "-9223372036854775807I64",
                "/",
                "1000I64",
                "-9223372036854775I64",
            ),
            ("-5I64", "/", "2I64", "-2I64"),
            ("5I64", "/", "-2I64", "-2I64"),
            ("-7I64", "/", "-3I64", "2I64"),
            (I64_MIN, "%", "-1I64", "0I64"),
            (I64_MIN, "%", "2I64", "0I64"),
            ("-9223372036854775807I64", "%", "1000I64", "-807I64"),
            ("-5I64", "%", "2I64", "-1I64"),
            ("5I64", "%", "-2I64", "1I64"),
            ("17I64", "%", "7I64", "3I64"),
        ],
    );
}

#[test]
fn wasm_i64_signed_comparison_rows_hold() {
    assert_comparisons(
        "i64",
        &[
            (I64_MIN, "<", I64_MAX, true),
            (I64_MAX, "<", I64_MIN, false),
            ("-1I64", "<", I64_MIN, false),
            (I64_MIN, "<", "-1I64", true),
            (I64_MIN, "==", I64_MIN, true),
            (I64_MIN, "<=", "-9223372036854775807I64", true),
        ],
    );
}

/// Rows whose operands or answer is exactly the lowest I32. These
/// are the edges where a width mistake shows first.
#[test]
fn wasm_i32_rows_at_the_lowest_value_hold() {
    let rows = [
        ("-2147483647", "-", "1", I32_MIN),
        ("-1073741824", "*", "2", I32_MIN),
        (I32_MIN, "/", "-2", "1073741824"),
    ];
    assert_table("i32 edge", &rows);
}
