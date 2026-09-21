//! Renaming something must not change what the compiler decides.
//!
//! A metamorphic test: it needs no oracle, because the answer is
//! "whatever the first spelling gave". Take a program, rename a
//! generic parameter, a binding or a field, and compile both. The
//! verdict has to match. A name is not part of the meaning.
//!
//! This class of test exists because of a defect it would have caught
//! in one run. The argument-type check skipped any parameter whose
//! declared type *mentioned* a generic parameter, and the test for
//! "mentions" was `declared.contains(name)` — a substring match. So
//! `String` contains `S`, and a function declared
//! `fn pick<S>(item: S, label: String)` silently skipped the check on
//! `label`. The same bug sat in the enum-payload check.
//!
//! Every generic test in the corpus used `T`. `T` appears in no
//! primitive type name, so nothing ever hit it. Searching the whole
//! test tree for generic parameters found `<T` 189 times and `<S`,
//! `<I`, `<V` and `<E` not once.
//!
//! The adversarial names below are each a substring of a type the
//! language ships: `S` of `String`, `I` of `I32`, `B` of `Boolean`,
//! `F` of `F64`, `N` of `Never`, `D` of `Dictionary`, `A` of `Array`,
//! `R` of `Range`, `E` of `Element`, `O` of `Optional`.

use crate::common::Checked;

use formalang::compile_to_ir;

/// Names that are each a substring of a type the language ships.
const ADVERSARIAL: &[&str] = &["T", "S", "I", "B", "F", "N", "D", "A", "R", "E", "O"];

/// Ordinary names, as a control. A defect that fires for every name is
/// not a naming defect.
const ORDINARY: &[&str] = &["T", "Item", "Value", "Element"];

/// A program shape with one hole for the generic parameter's name.
///
/// Each is written so the verdict must not depend on what fills the
/// hole. `NAME` is the placeholder.
struct Shape {
    what: &'static str,
    source: &'static str,
    /// Whether the shape should compile.
    accepts: bool,
}

const SHAPES: &[Shape] = &[
    Shape {
        what: "a wrong argument type beside a generic parameter",
        source: "pub fn pick<NAME>(item: NAME, label: String) -> I32 {\n    0\n}\n\n\
                 pub fn f() -> I32 {\n    pick(item: 1, label: 99)\n}\n",
        accepts: false,
    },
    Shape {
        what: "a right argument type beside a generic parameter",
        source: "pub fn pick<NAME>(item: NAME, label: String) -> I32 {\n    0\n}\n\n\
                 pub fn f() -> I32 {\n    pick(item: 1, label: \"a\")\n}\n",
        accepts: true,
    },
    Shape {
        what: "a wrong enum payload beside a generic parameter",
        source: "pub enum Wrap<NAME> {\n    one(inner: String),\n    two(v: NAME)\n}\n\n\
                 pub fn f() -> I32 {\n    let w = Wrap.one(inner: 42)\n    0\n}\n",
        accepts: false,
    },
    Shape {
        what: "a right enum payload beside a generic parameter",
        source: "pub enum Wrap<NAME> {\n    one(inner: String),\n    two(v: NAME)\n}\n\n\
                 pub fn f() -> I32 {\n    let w = Wrap.one(inner: \"x\")\n    0\n}\n",
        accepts: true,
    },
    Shape {
        what: "a wrong field type on a generic struct",
        source: "pub struct Holder<NAME> {\n    tag: String,\n    value: NAME\n}\n\n\
                 pub fn f() -> I32 {\n    let h = Holder(tag: 7, value: 1)\n    0\n}\n",
        accepts: false,
    },
    Shape {
        what: "a generic bound that the argument does not satisfy",
        source: "pub trait Shape {\n    fn area(self) -> I32\n}\n\n\
                 pub struct Plain {\n    n: I32\n}\n\n\
                 pub fn total<NAME: Shape>(item: NAME) -> I32 {\n    item.area()\n}\n\n\
                 pub fn f() -> I32 {\n    total(item: Plain(n: 1))\n}\n",
        accepts: false,
    },
    Shape {
        what: "a generic return type inferred from the argument",
        source: "pub fn identity<NAME>(item: NAME) -> NAME {\n    item\n}\n\n\
                 pub fn f() -> I32 {\n    let a = identity(item: 7)\n    a + 1\n}\n",
        accepts: true,
    },
    Shape {
        what: "a wrong argument to a generic method",
        source: "pub struct Box<NAME> {\n    value: NAME\n}\n\n\
                 impl Box<NAME> {\n    fn get(self, n: String) -> NAME { self.value }\n}\n\n\
                 pub fn f() -> I32 {\n    let b = Box(value: 1)\n    b.get(n: 5)\n}\n",
        accepts: false,
    },
];

fn render(shape: &Shape, name: &str) -> String {
    shape.source.replace("NAME", name)
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

/// Renaming a generic parameter does not change the verdict.
#[test]
fn a_generic_parameter_may_be_called_anything() {
    let mut checked = Checked::new("renamed shapes", 80);
    let mut wrong = Vec::new();

    for shape in SHAPES {
        for name in ADVERSARIAL {
            let accepted = compile_to_ir(&render(shape, name)).is_ok();
            if accepted != shape.accepts {
                wrong.push(format!(
                    "{}: with the parameter called `{name}` it was {}, but `{}` is right",
                    shape.what,
                    if accepted { "accepted" } else { "rejected" },
                    if shape.accepts { "accept" } else { "reject" },
                ));
            }
            checked.hit();
        }
    }

    assert!(
        wrong.is_empty(),
        "{} shape(s) changed their verdict when the generic parameter was renamed. \
         A name is not part of the meaning:\n  {}",
        wrong.len(),
        wrong.join("\n  ")
    );
}

/// The ordinary names agree with the adversarial ones.
///
/// Without this, a defect that rejects every spelling would pass the
/// test above: all eleven names would agree with each other and
/// disagree with the truth.
#[test]
fn an_ordinary_name_and_an_adversarial_one_agree() {
    let mut checked = Checked::new("control comparisons", 30);
    let mut wrong = Vec::new();

    for shape in SHAPES {
        for ordinary in ORDINARY {
            let baseline = compile_to_ir(&render(shape, ordinary)).is_ok();
            if baseline != shape.accepts {
                wrong.push(format!(
                    "{}: the ordinary name `{ordinary}` already disagrees with the shape",
                    shape.what
                ));
            }
            checked.hit();
        }
    }

    assert!(
        wrong.is_empty(),
        "{} shape(s) are wrong even before renaming:\n  {}",
        wrong.len(),
        wrong.join("\n  ")
    );
}

/// Renaming a binding does not change the verdict either.
#[test]
fn a_binding_may_be_called_anything() {
    let mut checked = Checked::new("renamed bindings", 8);
    let mut wrong = Vec::new();

    // Names that collide with a prelude method, a field name and a
    // word that reads like a keyword. A primitive type's own name is
    // not in the list: `let String = 1` is rejected on purpose, as
    // `CannotRedefinePrimitive`, and that is a rule rather than an
    // accident of spelling.
    for name in [
        "x",
        "len",
        "value",
        "self_value",
        "map",
        "count",
        "first",
        "item",
    ] {
        let source = format!("pub fn f() -> I32 {{\n    let {name} = 1\n    {name} + 1\n}}\n");
        if compile_to_ir(&source).is_err() {
            wrong.push(name.to_string());
        }
        checked.hit();
    }

    assert!(
        wrong.is_empty(),
        "a binding was rejected because of what it was called: {wrong:?}"
    );
}
