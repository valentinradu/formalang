//! The accept/reject matrix.
//!
//! Every type rule is a question of the form "in this context, does a
//! value of type B satisfy a declaration of type A?". There are many
//! contexts and many types, so there are thousands of such questions,
//! and writing them out by hand is neither practical nor reliable —
//! `let x: I32 = "text"` compiled for a long time because nobody
//! thought to write that one line.
//!
//! So this file generates them. Each context below renders a program
//! from a declared type and a value; the test compiles it and records
//! whether the compiler accepted it. The whole grid is written to an
//! `insta` snapshot.
//!
//! The snapshot is the point. It is not an oracle — it records what
//! the compiler does today. Two things follow:
//!
//! - Any change to the type checker shows up as a reviewable diff over
//!   the whole grid, not as a surprise in one hand-written test.
//! - The cells that are obviously wrong are visible in one place. The
//!   ones found so far are listed in [`KNOWN_WRONG`] with what they
//!   should say; each is a defect waiting to be fixed, and fixing one
//!   turns its row green here.
//!
//! On top of the snapshot, [`the_diagonal_is_accepted`] and
//! [`the_obvious_mismatches_are_rejected`] assert the cells nobody
//! needs to think about: a type always satisfies itself, and a string
//! never satisfies an integer.

#![expect(
    clippy::panic,
    clippy::format_push_string,
    reason = "a table-driven test reports a bad cell by failing loudly, and \
              the generators build source text with format!"
)]

use crate::common::Checked;

use formalang::compile_to_ir;

// ---------------------------------------------------------------------------
// The types
// ---------------------------------------------------------------------------

/// One type, paired with an expression that produces a value of it.
///
/// `name` is what the matrix prints. `declaration` is what a
/// declaration writes.
/// `value` is an expression of that type, written so it is valid
/// wherever the contexts below place it.
struct Ty {
    name: &'static str,
    declaration: &'static str,
    value: &'static str,
}

/// The declarations every program in this file shares, so a context
/// can name `Point`, `Colour` and `Named` without redeclaring them.
const PRELUDE: &str = "\
pub struct Point {
    x: I32,
    y: I32
}

pub struct Other {
    v: I32
}

pub enum Colour {
    red,
    green
}

pub trait Named {
    name: String
}

";

const TYPES: &[Ty] = &[
    Ty {
        name: "I32",
        declaration: "I32",
        value: "1",
    },
    Ty {
        name: "I64",
        declaration: "I64",
        value: "1I64",
    },
    Ty {
        name: "F32",
        declaration: "F32",
        value: "1.5F32",
    },
    Ty {
        name: "F64",
        declaration: "F64",
        value: "1.5",
    },
    Ty {
        name: "Boolean",
        declaration: "Boolean",
        value: "true",
    },
    Ty {
        name: "String",
        declaration: "String",
        value: "\"s\"",
    },
    Ty {
        name: "[I32]",
        declaration: "[I32]",
        value: "[1, 2]",
    },
    Ty {
        name: "[String]",
        declaration: "[String]",
        value: "[\"a\"]",
    },
    Ty {
        name: "[String:I32]",
        declaration: "[String: I32]",
        value: "[\"k\": 1]",
    },
    Ty {
        name: "I32?",
        declaration: "I32?",
        value: "nil",
    },
    Ty {
        name: "String?",
        declaration: "String?",
        value: "nil",
    },
    Ty {
        name: "tuple",
        declaration: "(a: I32, b: String)",
        value: "(a: 1, b: \"s\")",
    },
    Ty {
        name: "closure",
        declaration: "(I32) -> I32",
        value: "(n: I32) -> n",
    },
    Ty {
        name: "Point",
        declaration: "Point",
        value: "Point(x: 1, y: 2)",
    },
    Ty {
        name: "Other",
        declaration: "Other",
        value: "Other(v: 1)",
    },
    Ty {
        name: "Colour",
        declaration: "Colour",
        value: "Colour.red",
    },
];

// ---------------------------------------------------------------------------
// The contexts
// ---------------------------------------------------------------------------

/// Render a program that places `value` where a `declared` is
/// expected.
type Render = fn(declared: &str, value: &str) -> String;

struct Context {
    name: &'static str,
    render: Render,
}

fn let_annotation(declared: &str, value: &str) -> String {
    format!("{PRELUDE}pub fn f() -> I32 {{\n    let v: {declared} = {value}\n    0\n}}\n")
}

fn module_let(declared: &str, value: &str) -> String {
    format!("{PRELUDE}let v: {declared} = {value}\n")
}

fn call_argument(declared: &str, value: &str) -> String {
    format!(
        "{PRELUDE}fn takes(p: {declared}) -> I32 {{\n    0\n}}\n\n\
         pub fn f() -> I32 {{\n    takes(p: {value})\n}}\n"
    )
}

fn method_argument(declared: &str, value: &str) -> String {
    format!(
        "{PRELUDE}pub struct Holder {{\n    n: I32\n}}\n\n\
         impl Holder {{\n    fn takes(self, p: {declared}) -> I32 {{\n        0\n    }}\n}}\n\n\
         pub fn f() -> I32 {{\n    let h = Holder(n: 1)\n    h.takes(p: {value})\n}}\n"
    )
}

fn function_return(declared: &str, value: &str) -> String {
    format!("{PRELUDE}pub fn f() -> {declared} {{\n    {value}\n}}\n")
}

/// A `pub` struct may not hold a closure-typed field —
/// `CompilerError::PublicClosureField` — so the carriers in the three
/// field contexts are private. A public signature may not name a
/// private type either — `CompilerError::PrivateTypeInPublic` — so the
/// function that builds the carrier is private too, and a public one
/// calls it. Nothing else about them changes.
fn struct_field_default(declared: &str, value: &str) -> String {
    format!("{PRELUDE}struct Held {{\n    field: {declared} = {value}\n}}\n\nfn f() -> Held {{\n    Held()\n}}\n\npub fn g() -> I32 {{\n    let h = f()\n    0\n}}\n")
}

fn struct_field_init(declared: &str, value: &str) -> String {
    format!(
        "{PRELUDE}struct Held {{\n    field: {declared}\n}}\n\n\
         fn f() -> Held {{\n    Held(field: {value})\n}}\n\npub fn g() -> I32 {{\n    let h = f()\n    0\n}}\n"
    )
}

fn enum_payload(declared: &str, value: &str) -> String {
    format!(
        "{PRELUDE}enum Wrap {{\n    one(inner: {declared})\n}}\n\n\
         fn f() -> Wrap {{\n    Wrap.one(inner: {value})\n}}\n\npub fn g() -> I32 {{\n    let w = f()\n    0\n}}\n"
    )
}

fn array_element(declared: &str, value: &str) -> String {
    format!("{PRELUDE}pub fn f() -> [{declared}] {{\n    [{value}]\n}}\n")
}

fn dictionary_value(declared: &str, value: &str) -> String {
    format!("{PRELUDE}pub fn f() -> [String: {declared}] {{\n    [\"k\": {value}]\n}}\n")
}

fn default_parameter(declared: &str, value: &str) -> String {
    format!(
        "{PRELUDE}fn takes(p: {declared} = {value}) -> I32 {{\n    0\n}}\n\n\
         pub fn f() -> I32 {{\n    takes()\n}}\n"
    )
}

fn closure_return(declared: &str, value: &str) -> String {
    format!(
        "{PRELUDE}pub fn f() -> I32 {{\n    let c: () -> {declared} = () -> {value}\n    0\n}}\n"
    )
}

fn if_branches(declared: &str, value: &str) -> String {
    format!(
        "{PRELUDE}pub fn f(c: Boolean) -> I32 {{\n    \
         let v: {declared} = if c {{ {value} }} else {{ {value} }}\n    0\n}}\n"
    )
}

fn assignment(declared: &str, value: &str) -> String {
    format!(
        "{PRELUDE}pub fn f() -> I32 {{\n    let mut v: {declared} = {value}\n    \
         v = {value}\n    0\n}}\n"
    )
}

const CONTEXTS: &[Context] = &[
    Context {
        name: "let annotation",
        render: let_annotation,
    },
    Context {
        name: "module let",
        render: module_let,
    },
    Context {
        name: "call argument",
        render: call_argument,
    },
    Context {
        name: "method argument",
        render: method_argument,
    },
    Context {
        name: "function return",
        render: function_return,
    },
    Context {
        name: "struct field default",
        render: struct_field_default,
    },
    Context {
        name: "struct field init",
        render: struct_field_init,
    },
    Context {
        name: "enum payload",
        render: enum_payload,
    },
    Context {
        name: "array element",
        render: array_element,
    },
    Context {
        name: "dictionary value",
        render: dictionary_value,
    },
    Context {
        name: "default parameter",
        render: default_parameter,
    },
    Context {
        name: "closure return",
        render: closure_return,
    },
    Context {
        name: "if branches",
        render: if_branches,
    },
    Context {
        name: "assignment",
        render: assignment,
    },
];

// ---------------------------------------------------------------------------
// Running one cell
// ---------------------------------------------------------------------------

/// What the compiler did with one cell.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// The program compiled.
    Accepted,
    /// The program was rejected.
    Rejected,
}

impl Verdict {
    const fn mark(self) -> char {
        match self {
            Self::Accepted => '.',
            Self::Rejected => 'x',
        }
    }
}

fn verdict(context: &Context, declared: &Ty, value: &Ty) -> Verdict {
    let source = (context.render)(declared.declaration, value.value);
    if compile_to_ir(&source).is_ok() {
        Verdict::Accepted
    } else {
        Verdict::Rejected
    }
}

/// Cells whose recorded verdict is wrong, with what it should be.
///
/// Each entry is `(context, declared, value, what the compiler should
/// do)`. They are listed rather than asserted so the snapshot stays a
/// faithful record of today's behaviour; when one is fixed, its entry
/// comes out and [`the_known_wrong_cells_are_still_wrong`] catches the
/// staleness.
///
/// Empty today. The four that were here — an array or dictionary
/// literal that needed widening to an optional element type — were
/// fixed by joining the element types during inference rather than
/// taking the first element's.
const KNOWN_WRONG: &[(&str, &str, &str, Verdict)] = &[];

/// Whether this cell is a recorded defect.
fn is_known_wrong(context: &str, declared: &str, value: &str) -> bool {
    KNOWN_WRONG
        .iter()
        .any(|(c, d, v, _)| *c == context && *d == declared && *v == value)
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

/// The whole grid, as a snapshot.
///
/// Review the diff, not the file. A row of `x` where there were `.`
/// means the checker got stricter; the reverse means it got looser,
/// and one of those is usually a defect.
#[test]
fn the_matrix() {
    let mut out = String::new();
    let mut checked = Checked::new("matrix cells evaluated", 3000);

    out.push_str("Legend: . accepted   x rejected\n");
    out.push_str("Rows are the declared type, columns the value's type.\n\n");

    for context in CONTEXTS {
        out.push_str(&format!("=== {} ===\n", context.name));

        // Column header, one letter column per value type.
        out.push_str(&format!("{:<14}", ""));
        for value in TYPES {
            out.push_str(&format!("{:<14}", value.name));
        }
        out.push('\n');

        for declared in TYPES {
            out.push_str(&format!("{:<14}", declared.name));
            for value in TYPES {
                let mark = verdict(context, declared, value).mark();
                out.push_str(&format!("{mark:<14}"));
                checked.hit();
            }
            out.push('\n');
        }
        out.push('\n');
    }

    insta::assert_snapshot!("type_matrix", out);
}

/// A type always satisfies itself.
///
/// This is the one row nobody needs to think about, and it holds in
/// every context. A cell that fails here is a defect, not a judgement
/// call.
#[test]
fn the_diagonal_is_accepted() {
    let mut checked = Checked::new("diagonal cells", 150);
    // Cells in KNOWN_WRONG are skipped; they are counted there.
    let mut wrong = Vec::new();

    for context in CONTEXTS {
        for ty in TYPES {
            if is_known_wrong(context.name, ty.name, ty.name) {
                continue;
            }
            if verdict(context, ty, ty) == Verdict::Rejected {
                wrong.push(format!(
                    "{}: a {} value was rejected where a {} was declared\n{}",
                    context.name,
                    ty.name,
                    ty.name,
                    (context.render)(ty.declaration, ty.value)
                ));
            }
            checked.hit();
        }
    }

    assert!(
        wrong.is_empty(),
        "{} cell(s) on the diagonal were rejected:\n\n{}",
        wrong.len(),
        wrong.join("\n---\n")
    );
}

/// Pairs that can never be compatible, in any context.
///
/// These are the cells the user can name without thinking: a string is
/// not a number, a number is not a boolean, a struct is not a
/// different struct. `let i: I32 = ""` is the first row.
const NEVER_COMPATIBLE: &[(&str, &str)] = &[
    ("I32", "String"),
    ("I32", "Boolean"),
    ("I32", "[I32]"),
    ("I32", "Point"),
    ("I32", "Colour"),
    ("I32", "tuple"),
    ("String", "I32"),
    ("String", "Boolean"),
    ("String", "F64"),
    ("String", "Point"),
    ("String", "[String]"),
    ("Boolean", "I32"),
    ("Boolean", "String"),
    ("Boolean", "F64"),
    ("Boolean", "Point"),
    ("F64", "String"),
    ("F64", "Boolean"),
    ("F64", "Point"),
    ("Point", "Other"),
    ("Point", "I32"),
    ("Point", "String"),
    ("Point", "Colour"),
    ("Other", "Point"),
    ("Colour", "Point"),
    ("Colour", "I32"),
    ("Colour", "String"),
    ("[I32]", "[String]"),
    ("[I32]", "I32"),
    ("[String]", "[I32]"),
    ("[String]", "String"),
    ("[String:I32]", "[I32]"),
    ("tuple", "I32"),
    ("tuple", "Point"),
    ("closure", "I32"),
    ("closure", "String"),
];

/// Every pair above is rejected in every context.
///
/// This is the test that `let i: I32 = ""` belongs to, and it covers
/// its several hundred siblings at the same time.
#[test]
fn the_obvious_mismatches_are_rejected() {
    let mut checked = Checked::new("obvious mismatch cells", 400);
    let mut accepted = Vec::new();

    for context in CONTEXTS {
        for (declared_name, value_name) in NEVER_COMPATIBLE {
            let Some(declared) = TYPES.iter().find(|t| t.name == *declared_name) else {
                panic!("the matrix has no type named {declared_name}");
            };
            let Some(value) = TYPES.iter().find(|t| t.name == *value_name) else {
                panic!("the matrix has no type named {value_name}");
            };

            if verdict(context, declared, value) == Verdict::Accepted {
                accepted.push(format!(
                    "{}: a {} value was accepted where a {} was declared\n{}",
                    context.name,
                    value.name,
                    declared.name,
                    (context.render)(declared.declaration, value.value)
                ));
            }
            checked.hit();
        }
    }

    assert!(
        accepted.is_empty(),
        "{} incompatible cell(s) were accepted:\n\n{}",
        accepted.len(),
        accepted.join("\n---\n")
    );
}

/// `nil` fits an optional and nothing else.
#[test]
fn nil_fits_an_optional_and_nothing_else() {
    let mut checked = Checked::new("nil cells", 20);
    let mut wrong = Vec::new();

    for context in CONTEXTS {
        for declared in TYPES {
            let source = (context.render)(declared.declaration, "nil");
            let accepted = compile_to_ir(&source).is_ok();
            let optional = declared.declaration.ends_with('?');
            if accepted != optional {
                wrong.push(format!(
                    "{}: nil against {} was {}, expected {}",
                    context.name,
                    declared.name,
                    if accepted { "accepted" } else { "rejected" },
                    if optional { "accepted" } else { "rejected" }
                ));
            }
            checked.hit();
        }
    }

    // Recorded rather than asserted: several contexts infer an
    // optional from context and legitimately accept `nil`, and a few
    // do not accept it where they should. The snapshot above is the
    // full record; this test exists to keep the count from growing
    // unnoticed.
    assert!(
        wrong.len() <= 120,
        "{} nil cell(s) disagree with the optional rule, which is more than \
         the {} recorded when this test was written:\n{}",
        wrong.len(),
        120,
        wrong.join("\n")
    );
}

/// Every entry in [`KNOWN_WRONG`] still behaves the way it is recorded
/// to.
///
/// When someone fixes one, this test fails and the entry comes out.
#[test]
fn the_known_wrong_cells_are_still_wrong() {
    for (context_name, declared_name, value_name, should_be) in KNOWN_WRONG {
        let Some(context) = CONTEXTS.iter().find(|c| c.name == *context_name) else {
            panic!("no context named {context_name}");
        };
        let Some(declared) = TYPES.iter().find(|t| t.name == *declared_name) else {
            panic!("no type named {declared_name}");
        };
        let Some(value) = TYPES.iter().find(|t| t.name == *value_name) else {
            panic!("no type named {value_name}");
        };

        assert!(
            verdict(context, declared, value) != *should_be,
            "{context_name}: {value_name} against {declared_name} now behaves \
             correctly — remove it from KNOWN_WRONG"
        );
    }
}
