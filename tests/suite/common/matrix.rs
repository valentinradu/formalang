//! The value-by-context matrix, shared by the tests that use it.
//!
//! One table of values and one of contexts, and a renderer that puts a
//! value into a context, reads it back as `got`, and has the program
//! itself assert that nothing changed. `tests/suite/execution_matrix.rs`
//! compiles and runs each cell; `tests/suite/optimised_answers.rs` runs each
//! cell again through every optimising pass and compares.
//!
//! Keeping the tables here means one place to add a value or a
//! context, and both suites widen at once.

/// A value that can be compared to itself.
///
/// A closure is not here: `==` on one has no answer, which is a rule
/// this suite enforces elsewhere. Nor is a sequence, which may be read
/// only once and so cannot be both returned and compared.
pub struct Value {
    pub name: &'static str,
    pub ty: &'static str,
    pub literal: &'static str,
    /// How to check that `got` still holds the value. Usually a
    /// straight comparison; an optional has to be unwrapped first,
    /// because `I32?` and `I32` are different types and `==` wants one
    /// type on both sides.
    pub check: Option<&'static str>,
    /// Contexts this value cannot travel through, and why. A `nil`
    /// with no annotation has no element type, so there is nothing to
    /// ask `is_none` of — that is the language working, not a defect.
    pub skip: &'static [&'static str],
}

/// Declarations every generated program shares.
pub const PRELUDE: &str = "\
pub struct Point {
    x: I32,
    y: I32
}

pub enum Colour {
    red,
    green
}

";

pub const VALUES: &[Value] = &[
    Value {
        name: "I32",
        ty: "I32",
        literal: "7",
        check: None,
        skip: &[],
    },
    Value {
        name: "a negative I32",
        ty: "I32",
        literal: "-7",
        check: None,
        skip: &[],
    },
    Value {
        name: "I64",
        ty: "I64",
        literal: "7I64",
        check: None,
        skip: &[],
    },
    Value {
        name: "F64",
        ty: "F64",
        literal: "1.5",
        check: None,
        skip: &[],
    },
    Value {
        name: "F32",
        ty: "F32",
        literal: "1.5F32",
        check: None,
        skip: &[],
    },
    Value {
        name: "Boolean",
        ty: "Boolean",
        literal: "true",
        check: None,
        skip: &[],
    },
    Value {
        name: "String",
        ty: "String",
        literal: "\"text\"",
        check: None,
        skip: &[],
    },
    Value {
        name: "an array",
        ty: "[I32]",
        literal: "[1, 2, 3]",
        check: None,
        skip: &[],
    },
    Value {
        name: "a nested array",
        ty: "[[I32]]",
        literal: "[[1], [2]]",
        check: None,
        skip: &[],
    },
    Value {
        name: "a dictionary",
        ty: "[String:I32]",
        literal: "[\"k\": 1]",
        check: None,
        skip: &[],
    },
    Value {
        name: "a tuple",
        ty: "(a: I32, b: I32)",
        literal: "(a: 1, b: 2)",
        check: None,
        skip: &[],
    },
    Value {
        name: "a struct",
        ty: "Point",
        literal: "Point(x: 1, y: 2)",
        check: None,
        skip: &[],
    },
    Value {
        name: "an enum",
        ty: "Colour",
        literal: "Colour.red",
        check: None,
        skip: &[],
    },
    Value {
        // `nil` rather than a wrapped value: `let a: I32? = 3` is
        // accepted, but `let xs: [I32?] = [3]` is not, so a wrapped
        // value cannot travel through every context here. See
        // TESTING.md for that open question.
        name: "an empty optional",
        ty: "I32?",
        literal: "nil",
        check: Some("got.is_none()"),
        skip: &["a let without an annotation"],
    },
];

/// A place a value can be put and read back out.
///
/// `render` receives the type and the literal, and returns the body of
/// a function that binds the value as `got`. The harness appends the
/// comparison.
pub struct Context {
    pub what: &'static str,
    pub render: fn(ty: &str, literal: &str) -> String,
}

pub const CONTEXTS: &[Context] = &[
    Context {
        what: "a let with an annotation",
        render: |ty, literal| format!("    let got: {ty} = {literal}\n"),
    },
    Context {
        what: "a let without an annotation",
        render: |_, literal| format!("    let got = {literal}\n"),
    },
    Context {
        what: "a block result",
        render: |ty, literal| format!("    let got: {ty} = {{\n        {literal}\n    }}\n"),
    },
    Context {
        what: "a doubly nested block",
        render: |ty, literal| {
            format!(
                "    let got: {ty} = {{\n        {{\n            {literal}\n        }}\n    }}\n"
            )
        },
    },
    Context {
        what: "a struct field",
        render: |ty, literal| format!("    let got: {ty} = Holder(field: {literal}).field\n"),
    },
    Context {
        what: "a struct field default",
        render: |ty, _| format!("    let got: {ty} = Defaulted().field\n"),
    },
    Context {
        what: "a tuple field",
        render: |ty, literal| format!("    let got: {ty} = (only: {literal}).only\n"),
    },
    Context {
        what: "an array element",
        render: |ty, literal| {
            format!(
                "    let xs: [{ty}] = [{literal}]\n    \
                 let got: {ty} = if let picked = xs[0] {{ picked }} else {{ {literal} }}\n"
            )
        },
    },
    Context {
        what: "a dictionary value",
        render: |ty, literal| {
            format!(
                "    let d: [String:{ty}] = [\"k\": {literal}]\n    \
                 let got: {ty} = if let picked = d[\"k\"] {{ picked }} else {{ {literal} }}\n"
            )
        },
    },
    Context {
        what: "a function argument and return",
        render: |ty, literal| format!("    let got: {ty} = identity(item: {literal})\n"),
    },
    Context {
        what: "a default parameter",
        render: |ty, _| format!("    let got: {ty} = defaulted()\n"),
    },
    Context {
        what: "two calls deep",
        render: |ty, literal| format!("    let got: {ty} = outer(item: {literal})\n"),
    },
    Context {
        what: "a closure return",
        render: |ty, literal| {
            format!("    let make = () -> {literal}\n    let got: {ty} = make()\n")
        },
    },
    Context {
        what: "a closure argument",
        render: |ty, literal| {
            format!("    let got: {ty} = apply(f: (v: {ty}) -> v, x: {literal})\n")
        },
    },
    Context {
        what: "both branches of an if",
        render: |ty, literal| {
            format!("    let got: {ty} = if true {{ {literal} }} else {{ {literal} }}\n")
        },
    },
    Context {
        what: "the false branch of an if",
        render: |ty, literal| {
            format!("    let got: {ty} = if false {{ {literal} }} else {{ {literal} }}\n")
        },
    },
    Context {
        what: "a match arm",
        render: |ty, literal| {
            format!(
                "    let got: {ty} = match Colour.red {{\n        \
                 .red: {literal},\n        .green: {literal}\n    }}\n"
            )
        },
    },
    Context {
        what: "a closure that captures it",
        render: |ty, literal| {
            format!(
                "    let held: {ty} = {literal}\n    let make = () -> held\n    \
                 let got: {ty} = make()\n"
            )
        },
    },
];

/// The declarations a context needs around it.
pub fn support(ty: &str, literal: &str) -> String {
    format!(
        "struct Holder {{\n    field: {ty}\n}}\n\n\
         struct Defaulted {{\n    field: {ty} = {literal}\n}}\n\n\
         fn identity(item: {ty}) -> {ty} {{\n    item\n}}\n\n\
         fn defaulted(item: {ty} = {literal}) -> {ty} {{\n    item\n}}\n\n\
         fn outer(item: {ty}) -> {ty} {{\n    identity(item: item)\n}}\n\n\
         fn apply(f: ({ty}) -> {ty}, x: {ty}) -> {ty} {{\n    f(x)\n}}\n\n"
    )
}

/// A whole program: the value goes into the context, comes back as
/// `got`, and the program itself checks that nothing changed.
pub fn program(value: &Value, context: &Context) -> String {
    let body = (context.render)(value.ty, value.literal);
    let check = value
        .check
        .map_or_else(|| format!("got == {}", value.literal), ToString::to_string);
    format!(
        "{PRELUDE}{}pub fn probe() -> I32 {{\n{body}    \
         assert(condition: {check})\n    0\n}}\n",
        support(value.ty, value.literal)
    )
}
