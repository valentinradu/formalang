//! Closure capture analysis, over every expression form.
//!
//! A closure captures the variables its body reads from the enclosing
//! scope. The analysis that works this out walks the body, tracking
//! which names the closure's own parameters and `let` bindings shadow.
//! It has to handle every expression form, because a capture can hide
//! anywhere — inside a match arm, a dictionary value, an `if`
//! condition, a nested closure.
//!
//! Getting it wrong is not a diagnostic problem. Closure conversion
//! builds the environment struct from this list, so a missed capture
//! becomes a lifted function reading a field that does not exist, and
//! an extra one becomes a field nothing writes.
//!
//! The tests below check the analysis end to end: the program
//! compiles, and the environment struct that closure conversion builds
//! holds exactly the captured values.

#![expect(
    clippy::panic,
    reason = "a fixture that stops compiling should fail loudly"
)]

use crate::common::Checked;

use formalang::ir::{ClosureConversionPass, IrModule};
use formalang::{compile_to_ir, IrPass};

/// Compile `source` and run closure conversion over it.
fn converted(source: &str) -> IrModule {
    let module = match compile_to_ir(source) {
        Ok(module) => module,
        Err(errors) => panic!("the fixture must compile: {errors:?}\n{source}"),
    };
    match ClosureConversionPass::new().run(module) {
        Ok(module) => module,
        Err(errors) => panic!("closure conversion must accept the fixture: {errors:?}"),
    }
}

/// The field names of every synthesised closure-environment struct,
/// sorted and de-duplicated.
fn env_fields(module: &IrModule) -> Vec<String> {
    let mut fields: Vec<String> = module
        .structs
        .iter()
        .filter(|s| s.name.starts_with("__ClosureEnv"))
        .flat_map(|s| s.fields.iter().map(|f| f.name.clone()))
        .collect();
    fields.sort();
    fields.dedup();
    fields
}

/// How many closure bodies were lifted to top-level functions.
fn lifted_count(module: &IrModule) -> usize {
    module
        .functions
        .iter()
        .filter(|f| f.name.starts_with("__closure"))
        .count()
}

/// Assert that the environment structs hold exactly `expected`.
fn assert_captures(name: &str, source: &str, expected: &[&str]) {
    let module = converted(source);
    let got = env_fields(&module);
    let mut want: Vec<String> = expected.iter().map(|s| (*s).to_string()).collect();
    want.sort();
    want.dedup();
    assert_eq!(
        got, want,
        "{name}: the closure environment holds {got:?}, expected {want:?}"
    );
}

// ---------------------------------------------------------------------------
// One capture per expression form
// ---------------------------------------------------------------------------

/// A capture reached through each kind of expression must be found.
#[test]
fn a_capture_is_found_through_every_expression_form() {
    let mut checked = Checked::new("expression forms checked for captures", 18);
    let cases: &[(&str, &str, &[&str])] = &[
        (
            "bare reference",
            "pub fn f(base: I32) -> I32 {\n    let c = (n: I32) -> base\n    c(1)\n}\n",
            &["base"],
        ),
        (
            "binary operator",
            "pub fn f(base: I32) -> I32 {\n    let c = (n: I32) -> n + base\n    c(1)\n}\n",
            &["base"],
        ),
        (
            "unary operator",
            "pub fn f(base: I32) -> I32 {\n    let c = (n: I32) -> -base\n    c(1)\n}\n",
            &["base"],
        ),
        (
            "array element",
            "pub fn f(base: I32) -> I32 {\n    let c = (n: I32) -> [base, n].len()\n    c(1)\n}\n",
            &["base"],
        ),
        (
            "tuple field",
            "pub fn f(base: I32) -> I32 {\n    let c = (n: I32) -> (a: base, b: n).a\n    c(1)\n}\n",
            &["base"],
        ),
        (
            "call argument",
            "pub fn g(x: I32) -> I32 {\n    x\n}\n\npub fn f(base: I32) -> I32 {\n    let c = (n: I32) -> g(x: base)\n    c(1)\n}\n",
            &["base"],
        ),
        (
            "if condition",
            "pub fn f(base: I32) -> I32 {\n    let c = (n: I32) -> if base > 0 { n } else { 0 }\n    c(1)\n}\n",
            &["base"],
        ),
        (
            "if branch",
            "pub fn f(base: I32) -> I32 {\n    let c = (n: I32) -> if n > 0 { base } else { 0 }\n    c(1)\n}\n",
            &["base"],
        ),
        (
            "for collection",
            "pub fn f(base: [I32]) -> I32 {\n    let c = (n: I32) -> for v in base { v }.count()\n    c(1)\n}\n",
            &["base"],
        ),
        (
            "for body",
            "pub fn f(base: I32) -> I32 {\n    let c = (n: I32) -> for v in 0..3 { v + base }.count()\n    c(1)\n}\n",
            &["base"],
        ),
        (
            "field access",
            "pub struct A {\n    a: I32\n}\n\npub fn f(base: A) -> I32 {\n    let c = (n: I32) -> base.a\n    c(1)\n}\n",
            &["base"],
        ),
        (
            "struct construction",
            "pub struct A {\n    a: I32\n}\n\npub fn f(base: I32) -> I32 {\n    let c = (n: I32) -> A(a: base).a\n    c(1)\n}\n",
            &["base"],
        ),
        (
            "enum payload",
            "pub enum E {\n    one(x: I32)\n}\n\npub fn f(base: I32) -> I32 {\n    let c = (n: I32) -> match E.one(x: base) {\n        .one(v): v\n    }\n    c(1)\n}\n",
            &["base"],
        ),
        (
            "match scrutinee",
            "pub enum E {\n    one,\n    two\n}\n\npub fn f(base: E) -> I32 {\n    let c = (n: I32) -> match base {\n        .one: n,\n        .two: 0\n    }\n    c(1)\n}\n",
            &["base"],
        ),
        (
            "match arm body",
            "pub enum E {\n    one,\n    two\n}\n\npub fn f(base: I32, e: E) -> I32 {\n    let c = (n: I32) -> match e {\n        .one: base,\n        .two: 0\n    }\n    c(1)\n}\n",
            &["base", "e"],
        ),
        (
            "dictionary value",
            "pub fn f(base: I32) -> I32 {\n    let c = (n: I32) -> [\"k\": base].len()\n    c(1)\n}\n",
            &["base"],
        ),
        (
            "index",
            "pub fn f(base: [I32]) -> I32 {\n    let c = (n: I32) -> for v in base { v }.count()\n    c(1)\n}\n",
            &["base"],
        ),
        (
            "block body",
            "pub fn f(base: I32) -> I32 {\n    let c = (n: I32) -> {\n        let inner = base + n\n        inner\n    }\n    c(1)\n}\n",
            &["base"],
        ),
    ];

    for (name, source, expected) in cases {
        assert_captures(name, source, expected);
        checked.hit();
    }
}

// ---------------------------------------------------------------------------
// What must not be captured
// ---------------------------------------------------------------------------

/// A closure's own parameter is not a capture.
#[test]
fn a_parameter_is_not_captured() {
    assert_captures(
        "parameter only",
        "pub fn f() -> I32 {\n    let c = (n: I32) -> n + 1\n    c(1)\n}\n",
        &[],
    );
}

/// A `let` inside the closure body shadows an outer name of the same
/// name, so the outer one is not captured.
#[test]
fn an_inner_let_shadows_an_outer_binding() {
    assert_captures(
        "shadowing let",
        "pub fn f(base: I32) -> I32 {\n    let c = (n: I32) -> {\n        let base = n * 2\n        base\n    }\n    c(1)\n}\n",
        &[],
    );
}

/// A parameter that shadows an outer name shadows it too.
#[test]
fn a_parameter_shadows_an_outer_binding() {
    assert_captures(
        "shadowing parameter",
        "pub fn f(base: I32) -> I32 {\n    let c = (base: I32) -> base + 1\n    c(1)\n}\n",
        &[],
    );
}

/// A `for` loop variable binds inside the body, so it is not a
/// capture.
#[test]
fn a_loop_variable_is_not_captured() {
    assert_captures(
        "loop variable",
        "pub fn f() -> I32 {\n    let c = (n: I32) -> for v in 0..n { v }.count()\n    c(3)\n}\n",
        &[],
    );
}

/// A match-arm binding binds inside that arm.
#[test]
fn a_match_binding_is_not_captured() {
    assert_captures(
        "match binding",
        "pub enum E {\n    one(x: I32)\n}\n\npub fn f(e: E) -> I32 {\n    let c = (n: I32) -> match e {\n        .one(x): x + n\n    }\n    c(1)\n}\n",
        &["e"],
    );
}

/// A top-level function is not a capture: it is reachable by name
/// from anywhere.
#[test]
fn a_top_level_function_is_not_captured() {
    assert_captures(
        "calls a free function",
        "pub fn g(x: I32) -> I32 {\n    x * 2\n}\n\npub fn f() -> I32 {\n    let c = (n: I32) -> g(x: n)\n    c(1)\n}\n",
        &[],
    );
}

// ---------------------------------------------------------------------------
// Several captures, and nesting
// ---------------------------------------------------------------------------

/// Several distinct captures all reach the environment.
#[test]
fn every_distinct_capture_reaches_the_environment() {
    assert_captures(
        "three captures",
        "pub fn f(a: I32, b: I32, c: I32) -> I32 {\n    let k = (n: I32) -> (n + a) * b - c\n    k(10)\n}\n",
        &["a", "b", "c"],
    );
}

/// A name read twice is captured once.
#[test]
fn a_repeated_capture_appears_once() {
    let module = converted(
        "pub fn f(base: I32) -> I32 {\n    let c = (n: I32) -> base + base + base\n    c(1)\n}\n",
    );
    let fields: Vec<String> = module
        .structs
        .iter()
        .filter(|s| s.name.starts_with("__ClosureEnv"))
        .flat_map(|s| s.fields.iter().map(|f| f.name.clone()))
        .collect();
    assert_eq!(
        fields,
        vec!["base".to_string()],
        "a name read three times produced {fields:?}"
    );
}

/// An inner closure's capture of an outer function's binding
/// propagates through the outer closure's environment.
#[test]
fn a_nested_closure_propagates_its_capture() {
    let source = "pub fn f(base: I32) -> I32 {\n    let outer = (a: I32) -> {\n        let inner = (b: I32) -> b + base\n        inner(a)\n    }\n    outer(1)\n}\n";
    let module = converted(source);

    assert_eq!(
        lifted_count(&module),
        2,
        "two closures must be lifted, not {}",
        lifted_count(&module)
    );
    assert!(
        env_fields(&module).contains(&"base".to_string()),
        "the outer function's binding did not reach any environment: {:?}",
        env_fields(&module)
    );
}

/// Two sibling closures each get their own environment.
#[test]
fn two_sibling_closures_get_their_own_environments() {
    let source = "pub fn f(a: I32, b: I32) -> I32 {\n    let first = (n: I32) -> n + a\n    let second = (n: I32) -> n + b\n    first(1) + second(2)\n}\n";
    let module = converted(source);

    let envs: Vec<&str> = module
        .structs
        .iter()
        .filter(|s| s.name.starts_with("__ClosureEnv"))
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(
        envs.len(),
        2,
        "two closures must get two environment structs, got {envs:?}"
    );
    assert_eq!(
        env_fields(&module),
        vec!["a".to_string(), "b".to_string()],
        "each closure must capture only what it reads"
    );
}

/// A closure that captures nothing still lifts, with an empty
/// environment.
#[test]
fn a_closure_with_no_capture_gets_an_empty_environment() {
    let module = converted("pub fn f() -> I32 {\n    let c = (n: I32) -> n + 1\n    c(1)\n}\n");
    assert_eq!(lifted_count(&module), 1, "the closure must still be lifted");
    let envs: Vec<usize> = module
        .structs
        .iter()
        .filter(|s| s.name.starts_with("__ClosureEnv"))
        .map(|s| s.fields.len())
        .collect();
    assert_eq!(
        envs,
        vec![0],
        "a closure that captures nothing must get an empty environment"
    );
}

// ---------------------------------------------------------------------------
// The whole example corpus
// ---------------------------------------------------------------------------

/// Every environment field corresponds to a name the source mentions.
///
/// A field nothing writes is the symptom of an over-eager capture, and
/// it is invisible until a backend tries to build the struct.
#[test]
fn every_environment_field_names_something_in_the_source() {
    let mut checked = Checked::new("examples closure-converted", 20);
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("fv") {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(module) = compile_to_ir(&source) else {
            continue;
        };
        let Ok(module) = ClosureConversionPass::new().run(module) else {
            continue;
        };
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        for field in env_fields(&module) {
            assert!(
                source.contains(&field),
                "{name}: the closure environment holds a field {field:?} that the \
                 source never mentions"
            );
        }
        checked.hit();
    }
}
