//! The prelude method matrix.
//!
//! The third mechanical grid, after `tests/suite/type_matrix.rs` (does this
//! value satisfy this declaration?) and `tests/suite/operator_matrix.rs`
//! (what does this operator do between these two operands?). This one
//! asks: what happens when you call this prelude method on this
//! receiver?
//!
//! The prelude declares twenty methods across six carriers —
//! `Optional`, `Array`, `Seq`, `Dictionary`, `Range` and `String`. Each
//! belongs to exactly one carrier, so nineteen of every twenty cells
//! should be rejected. Nothing checked that: a method is looked up on
//! the receiver's type, and until this grid existed, no test asked what
//! happens when the lookup is asked for `"text".fold(...)` or
//! `[1, 2].is_some()`.
//!
//! On top of the grid, four tests assert the parts nobody needs to
//! think about: each method works on its own carrier, no method works
//! on a receiver that has none, a call with the wrong argument count is
//! rejected, and a call with the wrong argument type is rejected.

#![expect(
    clippy::format_push_string,
    reason = "the generators build source text with format!"
)]

use crate::common::Checked;

use formalang::compile_to_ir;

/// A receiver: a printable name, an expression, and the prelude
/// carrier whose methods it should accept.
struct Receiver {
    name: &'static str,
    value: &'static str,
    carrier: &'static str,
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

const RECEIVERS: &[Receiver] = &[
    Receiver {
        name: "I32",
        value: "1",
        carrier: "none",
    },
    Receiver {
        name: "I64",
        value: "1I64",
        carrier: "none",
    },
    Receiver {
        name: "F64",
        value: "1.5",
        carrier: "none",
    },
    Receiver {
        name: "Boolean",
        value: "true",
        carrier: "none",
    },
    Receiver {
        name: "String",
        value: "\"text\"",
        carrier: "String",
    },
    Receiver {
        name: "[I32]",
        value: "[1, 2]",
        carrier: "Array",
    },
    Receiver {
        name: "[String]",
        value: "[\"a\"]",
        carrier: "Array",
    },
    Receiver {
        name: "[String:I32]",
        value: "[\"k\": 1]",
        carrier: "Dictionary",
    },
    Receiver {
        name: "I32?",
        value: "nil",
        carrier: "Optional",
    },
    Receiver {
        name: "range",
        value: "0..4",
        carrier: "Range",
    },
    Receiver {
        name: "seq",
        value: "for i in 0..4 { i }",
        carrier: "Seq",
    },
    Receiver {
        name: "tuple",
        value: "(a: 1, b: 2)",
        carrier: "none",
    },
    Receiver {
        name: "closure",
        value: "(n: I32) -> n",
        carrier: "none",
    },
    Receiver {
        name: "struct",
        value: "Point(x: 1)",
        carrier: "none",
    },
    Receiver {
        name: "enum",
        value: "Colour.red",
        carrier: "none",
    },
];

/// A prelude method: its name, the carrier that declares it, and the
/// argument list a correct call gives it.
struct Method {
    name: &'static str,
    carrier: &'static str,
    /// A call that is right for the declaring carrier.
    good_args: &'static str,
    /// A call with the right count but a wrong type, or `None` when
    /// the method takes nothing.
    bad_type_args: Option<&'static str>,
    /// A call with the wrong number of arguments.
    bad_count_args: &'static str,
}

const METHODS: &[Method] = &[
    Method {
        name: "is_some",
        carrier: "Optional",
        good_args: "",
        bad_type_args: None,
        bad_count_args: "1",
    },
    Method {
        name: "is_none",
        carrier: "Optional",
        good_args: "",
        bad_type_args: None,
        bad_count_args: "1",
    },
    Method {
        name: "len",
        carrier: "Array",
        good_args: "",
        bad_type_args: None,
        bad_count_args: "1",
    },
    Method {
        name: "is_empty",
        carrier: "Array",
        good_args: "",
        bad_type_args: None,
        bad_count_args: "1",
    },
    Method {
        name: "map",
        carrier: "Seq",
        good_args: "f: (x) -> x",
        bad_type_args: Some("f: 1"),
        bad_count_args: "",
    },
    Method {
        name: "filter",
        carrier: "Seq",
        good_args: "f: (x) -> true",
        bad_type_args: Some("f: 1"),
        bad_count_args: "",
    },
    Method {
        name: "take",
        carrier: "Seq",
        good_args: "count: 1",
        bad_type_args: Some("count: \"x\""),
        bad_count_args: "",
    },
    Method {
        name: "skip",
        carrier: "Seq",
        good_args: "count: 1",
        bad_type_args: Some("count: \"x\""),
        bad_count_args: "",
    },
    Method {
        name: "collect",
        carrier: "Seq",
        good_args: "",
        bad_type_args: None,
        bad_count_args: "1",
    },
    Method {
        name: "count",
        carrier: "Seq",
        good_args: "",
        bad_type_args: None,
        bad_count_args: "1",
    },
    Method {
        name: "first",
        carrier: "Seq",
        good_args: "",
        bad_type_args: None,
        bad_count_args: "1",
    },
    Method {
        name: "any",
        carrier: "Seq",
        good_args: "f: (x) -> true",
        bad_type_args: Some("f: 1"),
        bad_count_args: "",
    },
    Method {
        name: "all",
        carrier: "Seq",
        good_args: "f: (x) -> true",
        bad_type_args: Some("f: 1"),
        bad_count_args: "",
    },
    Method {
        name: "slice",
        carrier: "String",
        good_args: "start: 0, end: 1",
        bad_type_args: Some("start: \"a\", end: 1"),
        bad_count_args: "start: 0",
    },
    Method {
        name: "starts_with",
        carrier: "String",
        good_args: "prefix: \"t\"",
        bad_type_args: Some("prefix: 1"),
        bad_count_args: "",
    },
    Method {
        name: "contains",
        carrier: "String",
        good_args: "needle: \"t\"",
        bad_type_args: Some("needle: 1"),
        bad_count_args: "",
    },
    Method {
        name: "byte_at",
        carrier: "String",
        good_args: "i: 0",
        bad_type_args: Some("i: \"a\""),
        bad_count_args: "",
    },
];

/// A program that calls `receiver.method(args)` and discards the
/// result, so the cell tests the call rather than what the result is
/// used for.
fn program(receiver: &Receiver, method: &str, args: &str) -> String {
    // `nil` on its own has no type, so the optional receiver carries an
    // annotation. Everything else infers from its literal.
    let binding = if receiver.name == "I32?" {
        format!("let r: I32? = {}", receiver.value)
    } else {
        format!("let r = {}", receiver.value)
    };
    format!(
        "{PRELUDE}pub fn f() -> I32 {{\n    {binding}\n    let v = r.{method}({args})\n    0\n}}\n"
    )
}

fn accepted(receiver: &Receiver, method: &str, args: &str) -> bool {
    compile_to_ir(&program(receiver, method, args)).is_ok()
}

/// Whether this receiver should accept this method.
///
/// Each method belongs to exactly one carrier. An `Array` and a
/// `Range` do not carry the `Seq` combinators: a `for` over them
/// yields the sequence, and the combinators hang off that. So
/// `[1, 2].map(...)` is rejected and `for x in [1, 2] { x }.map(...)`
/// is not.
fn should_accept(receiver: &Receiver, method: &Method) -> bool {
    receiver.carrier == method.carrier
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

/// The whole grid, as a snapshot.
#[test]
fn the_matrix() {
    let mut out = String::new();
    let mut checked = Checked::new("method cells evaluated", 250);

    out.push_str("Legend: . accepted   x rejected\n");
    out.push_str("Rows are the receiver, columns the method.\n");
    out.push_str("Each cell is a call with the right arguments for the method.\n\n");

    out.push_str(&format!("{:<14}", ""));
    for method in METHODS {
        out.push_str(&format!("{:<14}", method.name));
    }
    out.push('\n');

    for receiver in RECEIVERS {
        out.push_str(&format!("{:<14}", receiver.name));
        for method in METHODS {
            let mark = if accepted(receiver, method.name, method.good_args) {
                '.'
            } else {
                'x'
            };
            out.push_str(&format!("{mark:<14}"));
            checked.hit();
        }
        out.push('\n');
    }

    insta::assert_snapshot!("method_matrix", out);
}

/// Every method works on the carrier that declares it.
#[test]
fn a_method_works_on_its_own_carrier() {
    let mut checked = Checked::new("carrier cells", 15);
    let mut rejected = Vec::new();

    for method in METHODS {
        for receiver in RECEIVERS.iter().filter(|r| should_accept(r, method)) {
            if !accepted(receiver, method.name, method.good_args) {
                rejected.push(format!("{}.{}()", receiver.name, method.name));
            }
            checked.hit();
        }
    }

    assert!(
        rejected.is_empty(),
        "{} method(s) were rejected on a carrier that declares them: {:?}",
        rejected.len(),
        rejected
    );
}

/// No prelude method works on a receiver that has none.
///
/// A number, a boolean, a tuple, a closure, a struct and an enum
/// declare no prelude methods at all.
#[test]
fn no_method_works_on_a_receiver_without_one() {
    let mut checked = Checked::new("carrier-free cells", 100);
    let mut accepted_cells = Vec::new();

    for method in METHODS {
        for receiver in RECEIVERS.iter().filter(|r| r.carrier == "none") {
            if accepted(receiver, method.name, method.good_args) {
                accepted_cells.push(format!("{}.{}()", receiver.name, method.name));
            }
            checked.hit();
        }
    }

    assert!(
        accepted_cells.is_empty(),
        "{} method call(s) on a receiver with no methods were accepted: {:?}",
        accepted_cells.len(),
        accepted_cells
    );
}

/// A call with the wrong number of arguments is rejected, on the
/// carrier that declares the method.
#[test]
fn the_argument_count_is_checked() {
    let mut checked = Checked::new("argument-count cells", 15);
    let mut accepted_cells = Vec::new();

    for method in METHODS {
        for receiver in RECEIVERS.iter().filter(|r| should_accept(r, method)) {
            if accepted(receiver, method.name, method.bad_count_args) {
                accepted_cells.push(format!(
                    "{}.{}({})",
                    receiver.name, method.name, method.bad_count_args
                ));
            }
            checked.hit();
        }
    }

    assert!(
        accepted_cells.is_empty(),
        "{} call(s) with the wrong argument count were accepted: {:?}",
        accepted_cells.len(),
        accepted_cells
    );
}

/// A call with the right number of arguments but a wrong type is
/// rejected.
#[test]
fn the_argument_types_are_checked() {
    let mut checked = Checked::new("argument-type cells", 10);
    let mut accepted_cells = Vec::new();

    for method in METHODS {
        let Some(bad) = method.bad_type_args else {
            continue;
        };
        for receiver in RECEIVERS.iter().filter(|r| should_accept(r, method)) {
            if accepted(receiver, method.name, bad) {
                accepted_cells.push(format!("{}.{}({bad})", receiver.name, method.name));
            }
            checked.hit();
        }
    }

    assert!(
        accepted_cells.is_empty(),
        "{} call(s) with a wrong argument type were accepted: {:?}",
        accepted_cells.len(),
        accepted_cells
    );
}
