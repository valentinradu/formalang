//! No wrong program earns an internal compiler error.
//!
//! The compiler has two kinds of rejection. One names what the user
//! did wrong: "Unknown field 'z'". The other says the compiler broke
//! and asks the user to file a bug. The second kind is correct only
//! when the compiler really did break. For a mistake in the user's own
//! program it is the worst answer available: it blames the tool, hides
//! the real problem, and gives the user nothing to act on.
//!
//! Several passes were built on the assumption that the semantic pass
//! checks a shape before IR lowering reaches it. The assumption is
//! written into the source as a comment — "semantic should have caught
//! this" — and where the semantic check was missing, ordinary wrong
//! programs reached lowering and came back as internal errors.
//!
//! So this test does not check one shape. It generates every
//! combination of a value and a context that consumes one, compiles
//! each, and asserts that no rejection is an internal error. What the
//! verdict is does not matter here: a wrong program may be accepted
//! (other tests cover that) or rejected (good). It may not come back
//! as a bug report.

#[path = "common/mod.rs"]
mod common;

use common::Checked;

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

/// One value of each shape the language can produce.
const VALUES: &[(&str, &str)] = &[
    ("I32", "1"),
    ("I64", "1I64"),
    ("F32", "1.5F32"),
    ("F64", "1.5"),
    ("Boolean", "true"),
    ("String", "\"s\""),
    ("array", "[1, 2]"),
    ("array of strings", "[\"a\"]"),
    ("dictionary", "[\"k\": 1]"),
    ("nil", "nil"),
    ("tuple", "(a: 1, b: 2)"),
    ("closure", "(n: I32) -> n"),
    ("struct", "Point(x: 1)"),
    ("enum", "Colour.red"),
    // The inferred form takes its enum from the surrounding context.
    // Wherever a context forgot to offer one, the variant lowered to an
    // unresolved placeholder and the pass after it reported an internal
    // error. Only this spelling reaches that path.
    ("inferred enum variant", ".red"),
    ("range", "0..2"),
    ("sequence", "for i in 0..2 { i }"),
    ("nested array", "[[1], [2]]"),
    ("dictionary of arrays", "[\"k\": [1]]"),
    ("array of nil", "[nil]"),
];

/// One type of each shape a declaration can name.
const TYPES: &[&str] = &[
    "I32",
    "I64",
    "F32",
    "F64",
    "Boolean",
    "String",
    "[I32]",
    "[String]",
    "[String:I32]",
    "I32?",
    "(a: I32, b: I32)",
    "(I32) -> I32",
    "Point",
    "Colour",
    "[[I32]]",
    "[String:[I32]]",
];

/// Field names to read off a value, including ones that name a method.
const FIELDS: &[&str] = &["x", "a", "z", "len"];

/// Method calls to make on a value.
const METHODS: &[&str] = &[
    "len()",
    "count()",
    "collect()",
    "gone()",
    "is_some()",
    "map(f: (x) -> x)",
];

/// Keys to index a value with.
const KEYS: &[&str] = &["0", "\"k\"", "true"];

/// Compile one program and return the internal error it produced, if
/// it produced one.
fn internal_error(source: &str) -> Option<String> {
    let errors = compile_to_ir(source).err()?;
    errors
        .iter()
        .map(|e| format!("{e:?}"))
        .find(|d| d.starts_with("InternalError"))
}

/// Every context that consumes a value, paired with every value.
fn every_program() -> Vec<(String, String)> {
    let mut out = Vec::new();

    for ty in TYPES {
        for (name, value) in VALUES {
            out.push((
                format!("let v: {ty} = {name}"),
                format!("{PRELUDE}pub fn f() -> I32 {{\n    let v: {ty} = {value}\n    0\n}}\n"),
            ));
            out.push((
                format!("return {name} as {ty}"),
                format!("{PRELUDE}pub fn f() -> {ty} {{\n    {value}\n}}\n"),
            ));
            out.push((
                format!("pass {name} to a {ty} parameter"),
                format!(
                    "{PRELUDE}pub fn g(p: {ty}) -> I32 {{\n    0\n}}\n\n\
                     pub fn f() -> I32 {{\n    g(p: {value})\n}}\n"
                ),
            ));
        }
    }

    for (name, value) in VALUES {
        for field in FIELDS {
            out.push((
                format!("read .{field} off a {name}"),
                format!("{PRELUDE}pub fn f() -> I32 {{\n    let v = {value}\n    v.{field}\n}}\n"),
            ));
        }
        for method in METHODS {
            out.push((
                format!("call .{method} on a {name}"),
                format!("{PRELUDE}pub fn f() -> I32 {{\n    let v = {value}\n    v.{method}\n}}\n"),
            ));
        }
        for key in KEYS {
            out.push((
                format!("index a {name} with {key}"),
                format!(
                    "{PRELUDE}pub fn f() -> I32 {{\n    let v = {value}\n    \
                     let w = v[{key}]\n    0\n}}\n"
                ),
            ));
        }
        out.push((
            format!("iterate a {name}"),
            format!("{PRELUDE}pub fn f() -> I32 {{\n    for i in {value} {{ i }}.count()\n}}\n"),
        ));
        out.push((
            format!("match a {name}"),
            format!(
                "{PRELUDE}pub fn f() -> I32 {{\n    let v = {value}\n    \
                 match v {{ .red: 1, _: 0 }}\n}}\n"
            ),
        ));
        out.push((
            format!("if let on a {name}"),
            format!(
                "{PRELUDE}pub fn f() -> I32 {{\n    if let n = {value} {{ 1 }} else {{ 0 }}\n}}\n"
            ),
        ));
        out.push((
            format!("call a {name}"),
            format!("{PRELUDE}pub fn f() -> I32 {{\n    let v = {value}\n    v(1)\n}}\n"),
        ));
    }

    // A value in a container literal, keyed and unkeyed, and a value
    // assigned to a binding. The first two generators walked a value
    // through a `let`, a return, an argument and a member access, but
    // never through a dictionary key or an assignment — and both
    // turned out to reach IR lowering with no expected type, where an
    // unresolved placeholder became an internal error on correct code.
    for ty in TYPES {
        for (name, value) in VALUES {
            out.push((
                format!("{name} as a dictionary key against [{ty}: I32]"),
                format!(
                    "{PRELUDE}pub fn f() -> I32 {{\n                         let m: [{ty}: I32] = [{value}: 1]\n    0\n}}\n"
                ),
            ));
            out.push((
                format!("{name} read as a dictionary key against [{ty}: I32]"),
                format!(
                    "{PRELUDE}pub fn f() -> I32 {{\n    let m: [{ty}: I32] = [:]\n                         let v = m[{value}]\n    0\n}}\n"
                ),
            ));
            out.push((
                format!("assign {name} to a mut {ty}"),
                format!(
                    "{PRELUDE}pub fn f(p: {ty}) -> I32 {{\n    let mut v = p\n                         v = {value}\n    0\n}}\n"
                ),
            ));
        }
    }

    out
}

#[test]
fn no_wrong_program_earns_an_internal_error() {
    let programs = every_program();
    let mut checked = Checked::new("programs compiled", 2000);
    let mut blamed = Vec::new();

    for (name, source) in &programs {
        if let Some(detail) = internal_error(source) {
            blamed.push(format!("{name}\n      {detail}"));
        }
        checked.hit();
    }

    assert!(
        blamed.is_empty(),
        "{} of {} wrong program(s) were rejected with an internal compiler error, \
         which tells the user to file a bug for a mistake in their own program:\n  {}",
        blamed.len(),
        programs.len(),
        blamed.join("\n  ")
    );
}

/// The generator must keep covering a wide surface. A refactor that
/// quietly empties one of the tables would leave the test passing on
/// nothing.
#[test]
fn the_generator_covers_a_wide_surface() {
    let programs = every_program();
    assert!(
        programs.len() > 2000,
        "the generator produced only {} program(s); it should cover more than 2000",
        programs.len()
    );

    let mut names: Vec<&str> = programs.iter().map(|(n, _)| n.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(
        names.len(),
        programs.len(),
        "the generator produced two programs with the same name"
    );
}
