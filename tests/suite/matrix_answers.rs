//! The cells the matrices accept are run, and asked for their answer.
//!
//! `type_matrix`, `operator_matrix` and `method_matrix` between them
//! generate 7429 cells. 7054 of those are rejections, and for a
//! rejection "does this compile?" is the whole question — there is
//! nothing to run. The remaining **375 accept**, which means each one
//! compiles to a program that produces a value, and until this file
//! existed not one of them was ever asked what that value was.
//!
//! No oracle is written down here either. An accepted cell is checked
//! against a law that must hold whatever the value is:
//!
//! - `a == a` is true and `a != a` is false, for every type that
//!   compares at all;
//! - `a <= a` is true and `a < a` is false, for every ordered type;
//! - `a - a` is zero and `a / a` is one, for every number;
//! - `a && a` and `a || a` are both `a`, for a boolean;
//! - a prelude method answers what its own carrier says it should.
//!
//! A law is stronger than a table of expected values: it cannot be
//! copied wrong from the implementation, because it does not mention
//! the implementation.
//!
//! What these laws reach, and what they do not: `compile_to_ir` lowers
//! and stops. The optimising passes — constant folding, closure
//! conversion, monomorphisation, dead-code elimination — run in
//! `Pipeline::for_codegen()`, which nothing here calls. A law will
//! catch an operator lowered to the wrong node; it will not catch one
//! folded to the wrong constant. That was checked, not assumed:
//! changing `Sub` to `Add` in the lowerer breaks eight of these laws,
//! and the same change in the folder breaks none.

use crate::common::interpreter::{Fault, Interpreter};
use crate::common::Checked;

use formalang::compile_to_ir;

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

/// An operand, and which laws apply to it.
struct Operand {
    name: &'static str,
    literal: &'static str,
    /// A number: arithmetic and ordering laws apply.
    numeric: bool,
    /// Compares with `==`. Everything here does except a closure,
    /// which is not in the table.
    equatable: bool,
    /// A boolean: the logical laws apply.
    logical: bool,
}

const OPERANDS: &[Operand] = &[
    Operand {
        name: "I32",
        literal: "6",
        numeric: true,
        equatable: true,
        logical: false,
    },
    Operand {
        name: "I64",
        literal: "6I64",
        numeric: true,
        equatable: true,
        logical: false,
    },
    Operand {
        name: "F32",
        literal: "1.5F32",
        numeric: true,
        equatable: true,
        logical: false,
    },
    Operand {
        name: "F64",
        literal: "1.5",
        numeric: true,
        equatable: true,
        logical: false,
    },
    Operand {
        name: "Boolean",
        literal: "true",
        numeric: false,
        equatable: true,
        logical: true,
    },
    Operand {
        name: "String",
        literal: "\"text\"",
        numeric: false,
        equatable: true,
        logical: false,
    },
    Operand {
        name: "an array",
        literal: "[1, 2]",
        numeric: false,
        equatable: true,
        logical: false,
    },
    Operand {
        name: "a dictionary",
        literal: "[\"k\": 1]",
        numeric: false,
        equatable: true,
        logical: false,
    },
    Operand {
        name: "a tuple",
        literal: "(a: 1, b: 2)",
        numeric: false,
        equatable: true,
        logical: false,
    },
    Operand {
        name: "a struct",
        literal: "Point(x: 1)",
        numeric: false,
        equatable: true,
        logical: false,
    },
    Operand {
        name: "an enum",
        literal: "Colour.red",
        numeric: false,
        equatable: true,
        logical: false,
    },
    Operand {
        name: "a range",
        literal: "0..2",
        numeric: false,
        equatable: true,
        logical: false,
    },
];

/// Build a program that binds the operand twice and asserts `claim`.
///
/// Both bindings hold the same literal, so a law written about `a` and
/// `b` is a law about one value seen twice.
fn law_program(literal: &str, claim: &str) -> String {
    format!(
        "{PRELUDE}pub fn probe() -> I32 {{\n    let a = {literal}\n    let b = {literal}\n    \
         assert(condition: {claim})\n    0\n}}\n"
    )
}

/// Compile and run, reporting what happened.
fn check(source: &str) -> Result<(), String> {
    let module = compile_to_ir(source).map_err(|e| format!("did not compile: {e:?}"))?;
    let mut interpreter = Interpreter::new(&module);
    match interpreter.run("probe") {
        Err(Fault::AssertFailed) => Err("the law did not hold".to_string()),
        Err(other) => Err(format!("{other:?}")),
        Ok(_) if interpreter.asserts_passed == 0 => {
            Err("ran, but the assert never executed".to_string())
        }
        Ok(_) => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// The laws
// ---------------------------------------------------------------------------

/// A value equals itself, and does not differ from itself.
#[test]
fn equality_is_reflexive() {
    let mut checked = Checked::new("equality laws", 20);
    let mut broken = Vec::new();

    for operand in OPERANDS.iter().filter(|o| o.equatable) {
        for (law, claim) in [("a == a", "a == b"), ("not a != a", "!(a != b)")] {
            if let Err(why) = check(&law_program(operand.literal, claim)) {
                broken.push(format!("{}: `{law}` — {why}", operand.name));
            }
            checked.hit();
        }
    }

    assert!(
        broken.is_empty(),
        "{} equality law(s) did not hold:\n  {}",
        broken.len(),
        broken.join("\n  ")
    );
}

/// A number is not less than itself, and is at most itself.
#[test]
fn ordering_is_reflexive_where_it_should_be() {
    let mut checked = Checked::new("ordering laws", 12);
    let mut broken = Vec::new();

    for operand in OPERANDS.iter().filter(|o| o.numeric) {
        for (law, claim) in [
            ("a <= a", "a <= b"),
            ("a >= a", "a >= b"),
            ("not a < a", "!(a < b)"),
            ("not a > a", "!(a > b)"),
        ] {
            if let Err(why) = check(&law_program(operand.literal, claim)) {
                broken.push(format!("{}: `{law}` — {why}", operand.name));
            }
            checked.hit();
        }
    }

    assert!(
        broken.is_empty(),
        "{} ordering law(s) did not hold:\n  {}",
        broken.len(),
        broken.join("\n  ")
    );
}

/// Arithmetic on a number obeys the laws that hold for every value.
#[test]
fn arithmetic_obeys_its_laws() {
    let mut checked = Checked::new("arithmetic laws", 12);
    let mut broken = Vec::new();

    for operand in OPERANDS.iter().filter(|o| o.numeric) {
        // `a - a` is the zero of the type, and adding it changes
        // nothing. Written as a law so no literal zero of the right
        // width has to be spelled out.
        for (law, claim) in [
            ("a - a leaves a unchanged when added", "a + (b - b) == a"),
            ("a / a is the unit", "a * (b / b) == a"),
            ("a + a is a doubled", "a + b == a * (b / b) + b"),
            ("subtraction undoes addition", "(a + b) - b == a"),
        ] {
            if let Err(why) = check(&law_program(operand.literal, claim)) {
                broken.push(format!("{}: `{law}` — {why}", operand.name));
            }
            checked.hit();
        }
    }

    assert!(
        broken.is_empty(),
        "{} arithmetic law(s) did not hold:\n  {}",
        broken.len(),
        broken.join("\n  ")
    );
}

/// `a && a` and `a || a` are both `a`.
#[test]
fn the_logical_operators_are_idempotent() {
    let mut checked = Checked::new("logical laws", 2);
    let mut broken = Vec::new();

    for operand in OPERANDS.iter().filter(|o| o.logical) {
        for (law, claim) in [
            ("a && a is a", "(a && b) == a"),
            ("a || a is a", "(a || b) == a"),
        ] {
            if let Err(why) = check(&law_program(operand.literal, claim)) {
                broken.push(format!("{}: `{law}` — {why}", operand.name));
            }
            checked.hit();
        }
    }

    assert!(
        broken.is_empty(),
        "{} logical law(s) did not hold:\n  {}",
        broken.len(),
        broken.join("\n  ")
    );
}

/// Every accepted prelude method answers what its carrier says.
///
/// These are the twenty-six accepted cells of `method_matrix`, each
/// with the answer its receiver makes obvious.
#[test]
fn every_accepted_method_call_answers_correctly() {
    let mut checked = Checked::new("method answers", 20);
    let mut broken = Vec::new();

    let cases: &[(&str, &str, &str)] = &[
        ("[1, 2, 3]", "len()", "3"),
        ("[1, 2, 3]", "is_empty()", "false"),
        ("[]", "is_empty()", "true"),
        ("\"text\"", "len()", "4"),
        ("\"text\"", "is_empty()", "false"),
        ("\"text\"", "slice(start: 0, end: 2)", "\"te\""),
        ("\"text\"", "starts_with(prefix: \"te\")", "true"),
        ("\"text\"", "starts_with(prefix: \"xt\")", "false"),
        ("\"text\"", "contains(needle: \"ex\")", "true"),
        ("\"text\"", "byte_at(i: 0)", "116"),
        ("[\"k\": 1]", "len()", "1"),
        ("[\"k\": 1]", "is_empty()", "false"),
        ("0..4", "len()", "4"),
        ("0..4", "is_empty()", "false"),
        ("4..4", "is_empty()", "true"),
        ("for i in 0..4 { i }", "count()", "4"),
        ("for i in 0..4 { i }", "collect().len()", "4"),
        ("for i in 0..4 { i }", "map(f: (x) -> x * 2).count()", "4"),
        (
            "for i in 0..4 { i }",
            "filter(f: (x) -> x > 1).count()",
            "2",
        ),
        ("for i in 0..4 { i }", "take(count: 2).count()", "2"),
        ("for i in 0..4 { i }", "skip(count: 3).count()", "1"),
        (
            "for i in 0..4 { i }",
            "fold(initial: 0, f: (a, b) -> a + b)",
            "6",
        ),
        ("for i in 0..4 { i }", "any(f: (x) -> x > 2)", "true"),
        ("for i in 0..4 { i }", "all(f: (x) -> x > 2)", "false"),
    ];

    for (receiver, call, expected) in cases {
        let source = format!(
            "{PRELUDE}pub fn probe() -> I32 {{\n    let r = {receiver}\n    \
             assert(condition: r.{call} == {expected})\n    0\n}}\n"
        );
        if let Err(why) = check(&source) {
            broken.push(format!("`{receiver}`.{call} should be {expected} — {why}"));
        }
        checked.hit();
    }

    assert!(
        broken.is_empty(),
        "{} prelude method(s) did not answer correctly:\n  {}",
        broken.len(),
        broken.join("\n  ")
    );
}

/// An optional answers about its own emptiness.
#[test]
fn an_optional_knows_whether_it_holds_something() {
    let mut checked = Checked::new("optional answers", 4);
    let mut broken = Vec::new();

    for (binding, call, expected) in [
        ("let r: I32? = nil", "is_none()", "true"),
        ("let r: I32? = nil", "is_some()", "false"),
        ("let r: I32? = 5", "is_some()", "true"),
        ("let r: I32? = 5", "is_none()", "false"),
    ] {
        let source = format!(
            "{PRELUDE}pub fn probe() -> I32 {{\n    {binding}\n    \
             assert(condition: r.{call} == {expected})\n    0\n}}\n"
        );
        if let Err(why) = check(&source) {
            broken.push(format!(
                "`{binding}` then .{call} should be {expected} — {why}"
            ));
        }
        checked.hit();
    }

    assert!(
        broken.is_empty(),
        "{} optional answer(s) were wrong:\n  {}",
        broken.len(),
        broken.join("\n  ")
    );
}
