//! The operator matrix.
//!
//! The companion to `tests/suite/type_matrix.rs`. That one asks "does a
//! value of type B satisfy a declaration of type A?"; this one asks
//! "what does operator OP do between a value of type A and a value of
//! type B?".
//!
//! Every language's test suite has this grid somewhere — Go's
//! `test/`, Rust's `ui/binop`, `TypeScript`'s
//! `conformance/expressions/binaryOperators`. It is tedious to write
//! by hand and cheap to generate, so this generates it: fifteen
//! operators across sixteen types by sixteen types, compiled one cell
//! at a time, and the verdict grid written to an `insta` snapshot.
//!
//! The snapshot records today's behaviour rather than asserting a
//! right answer, so a change to the operator rules arrives as a
//! reviewable diff. On top of it, four tests assert the parts nobody
//! needs to think about: arithmetic works between two numbers of the
//! same type and nowhere else, comparison yields a boolean, the
//! logical operators take booleans only, and `==` works between any
//! two values of one type.

#![expect(
    clippy::panic,
    clippy::format_push_string,
    reason = "a table-driven test reports a bad cell by failing loudly, and the \
              generators build source text with format!"
)]

use crate::common::Checked;

use formalang::compile_to_ir;

// ---------------------------------------------------------------------------
// Operands
// ---------------------------------------------------------------------------

/// One operand: a printable name and an expression that produces it.
struct Operand {
    name: &'static str,
    value: &'static str,
}

/// Declarations every generated program shares.
const PRELUDE: &str = "\
pub struct Point {
    x: I32
}

pub enum Colour {
    red,
    green
}

";

const OPERANDS: &[Operand] = &[
    Operand {
        name: "I32",
        value: "1",
    },
    Operand {
        name: "I64",
        value: "1I64",
    },
    Operand {
        name: "F32",
        value: "1.5F32",
    },
    Operand {
        name: "F64",
        value: "1.5",
    },
    Operand {
        name: "Boolean",
        value: "true",
    },
    Operand {
        name: "String",
        value: "\"s\"",
    },
    Operand {
        name: "[I32]",
        value: "[1, 2]",
    },
    Operand {
        name: "[String]",
        value: "[\"a\"]",
    },
    Operand {
        name: "[String:I32]",
        value: "[\"k\": 1]",
    },
    Operand {
        name: "I32?",
        value: "nil",
    },
    Operand {
        name: "tuple",
        value: "(a: 1, b: 2)",
    },
    Operand {
        name: "closure",
        value: "(n: I32) -> n",
    },
    Operand {
        name: "Point",
        value: "Point(x: 1)",
    },
    Operand {
        name: "Colour",
        value: "Colour.red",
    },
    Operand {
        name: "range",
        value: "0..2",
    },
    Operand {
        name: "seq",
        value: "for i in 0..2 { i }",
    },
];

// ---------------------------------------------------------------------------
// Operators
// ---------------------------------------------------------------------------

/// One operator, with the class it belongs to.
struct Operator {
    symbol: &'static str,
    class: Class,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    /// `+ - * / %`
    Arithmetic,
    /// `< > <= >=`
    Ordering,
    /// `== !=`
    Equality,
    /// `&& ||`
    Logical,
    /// `..`
    Range,
}

const OPERATORS: &[Operator] = &[
    Operator {
        symbol: "+",
        class: Class::Arithmetic,
    },
    Operator {
        symbol: "-",
        class: Class::Arithmetic,
    },
    Operator {
        symbol: "*",
        class: Class::Arithmetic,
    },
    Operator {
        symbol: "/",
        class: Class::Arithmetic,
    },
    Operator {
        symbol: "%",
        class: Class::Arithmetic,
    },
    Operator {
        symbol: "<",
        class: Class::Ordering,
    },
    Operator {
        symbol: ">",
        class: Class::Ordering,
    },
    Operator {
        symbol: "<=",
        class: Class::Ordering,
    },
    Operator {
        symbol: ">=",
        class: Class::Ordering,
    },
    Operator {
        symbol: "==",
        class: Class::Equality,
    },
    Operator {
        symbol: "!=",
        class: Class::Equality,
    },
    Operator {
        symbol: "&&",
        class: Class::Logical,
    },
    Operator {
        symbol: "||",
        class: Class::Logical,
    },
    Operator {
        symbol: "..",
        class: Class::Range,
    },
];

/// The numeric operand names, which are the ones arithmetic and
/// ordering are defined on.
const NUMERIC: &[&str] = &["I32", "I64", "F32", "F64"];

/// The integer operand names. A range counts in steps of one, so only
/// these build one.
const INTEGER: &[&str] = &["I32", "I64"];

// ---------------------------------------------------------------------------
// Running one cell
// ---------------------------------------------------------------------------

/// A program that binds `left OP right` and discards it, so the cell
/// tests the operator rather than what the result is used for.
fn program(left: &Operand, op: &str, right: &Operand) -> String {
    format!(
        "{PRELUDE}pub fn f() -> I32 {{\n    let a = {}\n    let b = {}\n    \
         let v = a {op} b\n    0\n}}\n",
        left.value, right.value
    )
}

fn accepted(left: &Operand, op: &str, right: &Operand) -> bool {
    compile_to_ir(&program(left, op, right)).is_ok()
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

/// The whole grid, as a snapshot.
#[test]
fn the_matrix() {
    let mut out = String::new();
    let mut checked = Checked::new("operator cells evaluated", 3000);

    out.push_str("Legend: . accepted   x rejected\n");
    out.push_str("Rows are the left operand, columns the right.\n\n");

    for operator in OPERATORS {
        out.push_str(&format!("=== {} ===\n", operator.symbol));
        out.push_str(&format!("{:<14}", ""));
        for right in OPERANDS {
            out.push_str(&format!("{:<14}", right.name));
        }
        out.push('\n');

        for left in OPERANDS {
            out.push_str(&format!("{:<14}", left.name));
            for right in OPERANDS {
                let mark = if accepted(left, operator.symbol, right) {
                    '.'
                } else {
                    'x'
                };
                out.push_str(&format!("{mark:<12}"));
                checked.hit();
            }
            out.push('\n');
        }
        out.push('\n');
    }

    insta::assert_snapshot!("operator_matrix", out);
}

/// Arithmetic works between two numbers of the same type.
#[test]
fn arithmetic_works_between_two_numbers_of_one_type() {
    let mut checked = Checked::new("same-type arithmetic cells", 20);
    let mut rejected = Vec::new();

    for operator in OPERATORS.iter().filter(|o| o.class == Class::Arithmetic) {
        for name in NUMERIC {
            let Some(operand) = OPERANDS.iter().find(|o| o.name == *name) else {
                panic!("no operand named {name}");
            };
            if !accepted(operand, operator.symbol, operand) {
                rejected.push(format!("{name} {} {name}", operator.symbol));
            }
            checked.hit();
        }
    }

    assert!(
        rejected.is_empty(),
        "arithmetic was rejected between two numbers of one type: {rejected:?}"
    );
}

/// Arithmetic is rejected where neither operand is a number.
///
/// `+` between two strings is the one exception: it concatenates.
#[test]
fn arithmetic_is_rejected_on_non_numbers() {
    let mut checked = Checked::new("non-numeric arithmetic cells", 100);
    let mut accepted_cells = Vec::new();

    let non_numeric: Vec<&Operand> = OPERANDS
        .iter()
        .filter(|o| !NUMERIC.contains(&o.name) && o.name != "I32?")
        .collect();

    for operator in OPERATORS.iter().filter(|o| o.class == Class::Arithmetic) {
        for left in &non_numeric {
            for right in &non_numeric {
                // String concatenation is defined.
                if operator.symbol == "+" && left.name == "String" && right.name == "String" {
                    continue;
                }
                if accepted(left, operator.symbol, right) {
                    accepted_cells
                        .push(format!("{} {} {}", left.name, operator.symbol, right.name));
                }
                checked.hit();
            }
        }
    }

    assert!(
        accepted_cells.is_empty(),
        "{} arithmetic cell(s) between non-numbers were accepted: {:?}",
        accepted_cells.len(),
        accepted_cells
    );
}

/// The logical operators take booleans and nothing else.
#[test]
fn the_logical_operators_take_booleans_only() {
    let mut checked = Checked::new("logical cells", 100);
    let mut wrong = Vec::new();

    for operator in OPERATORS.iter().filter(|o| o.class == Class::Logical) {
        for left in OPERANDS {
            for right in OPERANDS {
                let both_boolean = left.name == "Boolean" && right.name == "Boolean";
                let got = accepted(left, operator.symbol, right);
                if got != both_boolean {
                    wrong.push(format!(
                        "{} {} {} was {}",
                        left.name,
                        operator.symbol,
                        right.name,
                        if got { "accepted" } else { "rejected" }
                    ));
                }
                checked.hit();
            }
        }
    }

    assert!(
        wrong.is_empty(),
        "{} logical cell(s) disagree with the boolean-only rule:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// Comparison and equality produce a boolean, not their operands'
/// type.
#[test]
fn comparison_produces_a_boolean() {
    let mut checked = Checked::new("comparison result cells", 6);
    let mut wrong = Vec::new();

    for operator in OPERATORS
        .iter()
        .filter(|o| o.class == Class::Ordering || o.class == Class::Equality)
    {
        // `let v: Boolean = a OP b` must compile for two integers.
        let source = format!(
            "pub fn f() -> Boolean {{\n    let a = 1\n    let b = 2\n    \
             let v: Boolean = a {} b\n    v\n}}\n",
            operator.symbol
        );
        if compile_to_ir(&source).is_err() {
            wrong.push(format!("{} did not produce a boolean", operator.symbol));
        }
        checked.hit();
    }

    assert!(
        wrong.is_empty(),
        "{} comparison operator(s) did not produce a boolean: {wrong:?}",
        wrong.len()
    );
}

/// Every operand type compares to itself with `==` and `!=`.
///
/// Equality is total in this language: it is defined structurally, so
/// two values of one type always compare.
#[test]
fn equality_works_within_every_type() {
    let mut checked = Checked::new("equality diagonal cells", 20);
    let mut rejected = Vec::new();

    for operator in OPERATORS.iter().filter(|o| o.class == Class::Equality) {
        for operand in OPERANDS {
            // A closure has no structural identity to compare.
            if operand.name == "closure" {
                continue;
            }
            if !accepted(operand, operator.symbol, operand) {
                rejected.push(format!(
                    "{} {} {}",
                    operand.name, operator.symbol, operand.name
                ));
            }
            checked.hit();
        }
    }

    assert!(
        rejected.is_empty(),
        "{} equality cell(s) between two values of one type were rejected: {:?}",
        rejected.len(),
        rejected
    );
}

/// `..` builds a range from two integers, and from nothing else.
///
/// A float bound is rejected on purpose: a range counts from the start
/// to the end in steps of one, which `1.5..3.5` has no answer for.
#[test]
fn a_range_is_built_from_two_integers() {
    let Some(i32_operand) = OPERANDS.iter().find(|o| o.name == "I32") else {
        panic!("no I32 operand");
    };
    assert!(
        accepted(i32_operand, "..", i32_operand),
        "`0..2` should build a range"
    );

    let mut checked = Checked::new("range cells", 14);
    let mut wrong = Vec::new();
    for left in OPERANDS.iter().filter(|o| !INTEGER.contains(&o.name)) {
        if accepted(left, "..", i32_operand) {
            wrong.push(format!("{} .. I32 was accepted", left.name));
        }
        checked.hit();
    }
    assert!(
        wrong.is_empty(),
        "a range was built from something that is not an integer: {wrong:?}"
    );
}

/// A closure is not equatable, at the top level or inside a container.
///
/// Equality here is structural, and a closure has no structure to
/// compare: after closure conversion it is a code pointer plus a
/// captured environment. Two closures written the same way are still
/// two different values.
#[test]
fn a_closure_is_not_equatable() {
    let cases = [
        (
            "a bare closure",
            "let a = (n: I32) -> n\n    let b = (n: I32) -> n",
        ),
        (
            "an array of closures",
            "let a = [(n: I32) -> n]\n    let b = [(n: I32) -> n]",
        ),
        (
            "an optional closure",
            "let a: ((I32) -> I32)? = (n: I32) -> n\n    let b: ((I32) -> I32)? = nil",
        ),
    ];

    let mut checked = Checked::new("closure equality cases", 3);
    let mut accepted_cases = Vec::new();

    for (label, bindings) in cases {
        for op in ["==", "!="] {
            let source = format!("pub fn f() -> Boolean {{\n    {bindings}\n    a {op} b\n}}\n");
            if compile_to_ir(&source).is_ok() {
                accepted_cases.push(format!("{label} with {op}"));
            }
        }
        checked.hit();
    }

    assert!(
        accepted_cases.is_empty(),
        "equality was accepted on a closure: {accepted_cases:?}"
    );
}

/// A struct that holds a closure is not equatable either, because
/// structural equality reaches every field.
#[test]
fn a_struct_holding_a_closure_is_not_equatable() {
    let source = "\
struct Holder {
    f: (I32) -> I32
}

pub fn g() -> Boolean {
    let a = Holder(f: (n: I32) -> n)
    let b = Holder(f: (n: I32) -> n)
    a == b
}
";
    assert!(
        compile_to_ir(source).is_err(),
        "a struct with a closure field should not compare"
    );
}
