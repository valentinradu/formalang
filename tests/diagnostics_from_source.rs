//! Each diagnostic, provoked from real source.
//!
//! `tests/reporting_every_variant.rs` checks that every
//! `CompilerError` variant renders. This file checks the other half:
//! that a user can actually reach it by writing a program.
//!
//! Each row pairs a snippet with the variant it must produce. Two
//! things follow from that.
//!
//! - It pins the diagnostics contract. A change that makes a snippet
//!   report something else — or nothing — shows up here rather than
//!   in a user's terminal.
//! - It walks the validation paths that produce those errors, which
//!   is most of the semantic analyser.
//!
//! A few variants cannot be reached from source and are listed in
//! [`unreachable_from_source`] with the reason.

use std::collections::BTreeSet;

#[path = "common/mod.rs"]
mod common;

use common::Checked;

use formalang::{compile_to_ir, CompilerError};

/// The name of `error`'s variant, taken from its `Debug` form.
///
/// `Debug` prints `VariantName { .. }`, so the name is everything up
/// to the first space or brace.
fn variant_name(error: &CompilerError) -> String {
    let text = format!("{error:?}");
    text.split([' ', '{', '('])
        .next()
        .unwrap_or_default()
        .to_string()
}

/// Compile `source` and return the names of the variants it reports.
fn diagnose(source: &str) -> BTreeSet<String> {
    match compile_to_ir(source) {
        Ok(_) => BTreeSet::new(),
        Err(errors) => errors.iter().map(variant_name).collect(),
    }
}

/// `(expected variant, source)`.
///
/// The snippet must report the named variant. It may report others
/// too: one mistake often trips several checks, and pinning the exact
/// list would make every row brittle.
const CASES: &[(&str, &str)] = &[
    // --- lexical ---------------------------------------------------
    ("InvalidCharacter", "pub struct A { a: I32 }\u{0}"),
    ("UnterminatedString", "pub fn f() -> String {\n    \"open\n}\n"),
    (
        "UnterminatedBlockComment",
        "/* never closed\npub fn f() -> I32 { 1 }\n",
    ),
    (
        "InvalidUnicodeEscape",
        // Valid `\uXXXX` syntax, invalid code point: `D800` is half of
        // a UTF-16 surrogate pair and is not a character.
        "pub fn f() -> String {\n    \"\\uD800\"\n}\n",
    ),
    ("InvalidNumber", "pub fn f() -> F64 {\n    1e400\n}\n"),
    // --- syntax ----------------------------------------------------
    ("ParseError", "pub struct A { a: I32"),
    // --- names and types -------------------------------------------
    (
        "UndefinedReference",
        "pub fn f() -> I32 {\n    missing_name\n}\n",
    ),
    (
        "TypeMismatch",
        "pub fn f() -> I32 {\n    let x: I32 = \"text\"\n    x\n}\n",
    ),
    (
        "DuplicateDefinition",
        "pub struct A { a: I32 }\npub struct A { b: I32 }\n",
    ),
    ("UndefinedType", "pub struct A { a: NoSuchType }\n"),
    (
        "PrimitiveRedefinition",
        "pub struct I32 { a: I32 }\n",
    ),
    // --- modules ---------------------------------------------------
    ("ModuleNotFound", "use no_such_module::Thing\n"),
    // --- traits ----------------------------------------------------
    (
        "TraitUsedAsValueType",
        "pub trait Shape {\n    name: String\n}\n\npub fn f(s: Shape) -> String {\n    s.name\n}\n",
    ),
    (
        "UndefinedTrait",
        "pub trait Composed: NoSuchTrait {}\n",
    ),
    (
        "NotATrait",
        "pub struct A {\n    a: I32\n}\n\npub trait Composed: A {}\n",
    ),
    (
        "MissingTraitField",
        "pub trait Named {\n    name: String\n}\n\npub struct A {\n    other: I32\n}\n\nimpl Named for A {}\n",
    ),
    (
        "TraitFieldTypeMismatch",
        "pub trait Named {\n    name: String\n}\n\npub struct A {\n    name: I32\n}\n\nimpl Named for A {}\n",
    ),
    (
        "MissingTraitMethod",
        "pub trait Shape {\n    fn area(self) -> I32\n}\n\npub struct A {\n    side: I32\n}\n\nimpl Shape for A {}\n",
    ),
    // --- expressions -----------------------------------------------
    (
        "InvalidBinaryOp",
        "pub fn f() -> I32 {\n    let a: String = \"x\"\n    let b: Boolean = true\n    a + b\n}\n",
    ),
    (
        "ForLoopNotArray",
        "pub fn f(n: I32) -> I32 {\n    for x in n { x }.count()\n}\n",
    ),
    (
        "InvalidIfCondition",
        "pub fn f(n: I32) -> I32 {\n    if n { 1 } else { 0 }\n}\n",
    ),
    (
        "MatchNotEnum",
        "pub fn f(n: I32) -> I32 {\n    match n {\n        _: 0\n    }\n}\n",
    ),
    (
        "NonExhaustiveMatch",
        "pub enum E {\n    one,\n    two\n}\n\npub fn f(e: E) -> I32 {\n    match e {\n        .one: 1\n    }\n}\n",
    ),
    (
        "DuplicateMatchArm",
        "pub enum E {\n    one,\n    two\n}\n\npub fn f(e: E) -> I32 {\n    match e {\n        .one: 1,\n        .one: 2,\n        .two: 3\n    }\n}\n",
    ),
    (
        "UnknownEnumVariant",
        "pub enum E {\n    one\n}\n\npub fn f() -> E {\n    E.nope\n}\n",
    ),
    (
        "MissingField",
        "pub struct A {\n    a: I32,\n    b: I32\n}\n\npub fn f() -> A {\n    A(a: 1)\n}\n",
    ),
    (
        "UnknownField",
        "pub struct A {\n    a: I32\n}\n\npub fn f() -> I32 {\n    let v = A(a: 1)\n    v.nope\n}\n",
    ),
    (
        "PositionalArgInStruct",
        "pub struct A {\n    a: I32\n}\n\npub fn f() -> A {\n    A(1)\n}\n",
    ),
    (
        "EnumVariantRequiresData",
        "pub enum E {\n    one(x: I32)\n}\n\npub fn f() -> E {\n    E.one\n}\n",
    ),
    (
        "EnumVariantWithoutData",
        "pub enum E {\n    one\n}\n\npub fn f() -> E {\n    E.one(x: 1)\n}\n",
    ),
    // --- generics --------------------------------------------------
    (
        "GenericArityMismatch",
        "pub struct P<A, B> {\n    first: A,\n    second: B\n}\n\npub fn f() -> I32 {\n    let v: P<I32> = P<I32, I32>(first: 1, second: 2)\n    v.first\n}\n",
    ),
    (
        "DuplicateGenericParam",
        "pub struct A<T, T> {\n    value: T\n}\n",
    ),
    (
        "OutOfScopeTypeParameter",
        "pub struct A {\n    value: T\n}\n",
    ),
    // --- extern ----------------------------------------------------
    (
        "RegularFnWithoutBody",
        "pub struct A {\n    a: I32\n}\n\nimpl A {\n    fn m(self) -> I32\n}\n",
    ),
    (
        "ExternImplWithBody",
        "extern impl String {\n    fn len(self) -> I32 {\n        0\n    }\n}\n",
    ),
    // --- defaults and optionals ------------------------------------
    (
        "RequiredParamAfterDefault",
        "pub fn f(a: I32 = 1, b: I32) -> I32 {\n    a + b\n}\n",
    ),
    (
        "OptionalUsedAsNonOptional",
        "pub struct A {\n    a: I32\n}\n\npub fn f(xs: [A]) -> I32 {\n    xs[0].a\n}\n",
    ),
    // --- mutability and ownership ----------------------------------
    (
        "UseAfterSink",
        "fn take(sink s: String) -> I32 {\n    s.len()\n}\n\npub fn f() -> I32 {\n    let a: String = \"x\"\n    take(s: a) + take(s: a)\n}\n",
    ),
    // --- sequences -------------------------------------------------
    (
        "SeqNotConsumed",
        "pub fn f(xs: [I32]) -> I32 {\n    for x in xs { x }\n    0\n}\n",
    ),
    // --- dictionaries ----------------------------------------------
    (
        "FloatDictionaryKey",
        "pub fn f() -> I32 {\n    let d: [F64: I32] = [1.0: 1]\n    0\n}\n",
    ),
    // --- functions -------------------------------------------------
    (
        "FunctionReturnTypeMismatch",
        "pub fn f() -> I32 {\n    \"not an integer\"\n}\n",
    ),
    // --- numbers ---------------------------------------------------
    (
        "NumericOverflow",
        "pub fn f() -> I32 {\n    99999999999999\n}\n",
    ),
    // --- closures --------------------------------------------------
    (
        "PublicClosureField",
        "pub struct A {\n    callback: (I32) -> I32\n}\n",
    ),
];

/// Variants that no program can produce, with the reason.
///
/// Listing them here keeps the gap visible. If one becomes reachable,
/// move it into [`CASES`].
fn unreachable_from_source() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "InternalError",
            "raised only when the lowerer reaches a state its invariants rule out",
        ),
        (
            "TooManyDefinitions",
            "needs more than 2^32 definitions of one kind in a single module",
        ),
        (
            "ExpressionDepthExceeded",
            "guards the recursive walks; the parser rejects such input first",
        ),
        (
            "ModuleReadError",
            "needs a module file that exists but cannot be read, which is a filesystem state, not source",
        ),
        (
            "ExternFnWithBody",
            "the parser gives an extern declaration no body, so the cross-check in pass1 fires only under error recovery",
        ),
        (
            "UnexpectedToken",
            "the chumsky parser reports its own failures as ParseError",
        ),
        (
            "UnexpectedEof",
            "the chumsky parser reports end of input as ParseError",
        ),
    ]
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

/// Every row produces the variant it names.
#[test]
fn every_case_produces_its_variant() {
    let mut wrong = Vec::new();
    for (expected, source) in CASES {
        let found = diagnose(source);
        if !found.contains(*expected) {
            wrong.push(format!("{expected}: got {found:?} for\n{source}\n---"));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} case(s) did not produce the variant they name:\n\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// Every row reports something. A snippet that compiles cleanly is a
/// row that has stopped testing anything.
#[test]
fn every_case_fails_to_compile() {
    for (expected, source) in CASES {
        assert!(
            compile_to_ir(source).is_err(),
            "the snippet for {expected} compiled cleanly, so it no longer \
             tests anything:\n{source}"
        );
    }
}

/// No row is a duplicate: each names a different variant.
#[test]
fn the_table_names_each_variant_once() {
    let mut seen = BTreeSet::new();
    for (expected, _) in CASES {
        assert!(
            seen.insert(*expected),
            "{expected} appears twice in the table"
        );
    }
}

/// A variant is either covered by a row or listed as unreachable,
/// never both.
#[test]
fn the_unreachable_list_does_not_overlap_the_table() {
    let covered: BTreeSet<&str> = CASES.iter().map(|(name, _)| *name).collect();
    for (name, reason) in unreachable_from_source() {
        assert!(
            !covered.contains(name),
            "{name} is listed as unreachable ({reason}) but a row produces it"
        );
    }
}

/// Every diagnostic carries a span inside the source that produced it.
///
/// A span past the end of the source makes the renderer draw a
/// snippet around text that is not there.
#[test]
fn every_diagnostic_points_inside_its_source() {
    let mut checked = Checked::new("diagnostics span-checked", 40);
    for (expected, source) in CASES {
        let Err(errors) = compile_to_ir(source) else {
            continue;
        };
        for error in &errors {
            let span = error.span();
            assert!(
                span.start.offset <= span.end.offset,
                "{expected}: {} reported an inverted span {span:?}",
                variant_name(error)
            );
            assert!(
                span.end.offset <= source.len(),
                "{expected}: {} reported {span:?}, past the end of a {}-byte \
                 source",
                variant_name(error),
                source.len()
            );
            checked.hit();
        }
    }
}

/// Every diagnostic renders without falling back to the raw `Display`
/// form.
#[test]
fn every_diagnostic_renders_from_its_own_source() {
    let mut checked = Checked::new("diagnostics rendered", 40);
    for (expected, source) in CASES {
        let Err(errors) = compile_to_ir(source) else {
            continue;
        };
        let report = formalang::report_errors(&errors, source, "case.fv");
        assert!(!report.is_empty(), "{expected}: rendered an empty report");
        assert!(
            !report.contains("failed to render error"),
            "{expected}: the renderer fell back to Display:\n{report}"
        );
        checked.hit();
    }
}
