//! Cross-module programs with a stated answer.
//!
//! Each test writes one or more modules to a temporary directory,
//! compiles the entry source through a `FileSystemResolver`, runs its
//! `probe` function, and compares the answer with the stated one.
//!
//! The programs aim at the rules that make a module a unit: a private
//! name in a module means that module's definition, wherever a caller
//! imports from it; an imported type keeps its impl block; and a value
//! of a type from a module two imports away is still usable.

#![expect(clippy::panic, reason = "a test reports its failure by failing loudly")]

use crate::common::interpreter::{Interpreter, Value};
use crate::common::with_a_large_stack;

use formalang::{compile_to_ir_with_resolver, FileSystemResolver};

/// Compile `entry` against `modules`, run `probe`, and give the answer
/// as text: a number, or the reason there is none.
fn answer(entry: &str, modules: &[(&str, &str)]) -> String {
    let dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => panic!("cannot make a temporary directory: {e}"),
    };
    for (name, source) in modules {
        let path = dir.path().join(format!("{name}.fv"));
        if let Err(e) = std::fs::write(&path, source) {
            panic!("cannot write {}: {e}", path.display());
        }
    }
    let root = dir.path().to_path_buf();
    let entry = entry.to_string();
    with_a_large_stack(move || {
        let module = match compile_to_ir_with_resolver(&entry, FileSystemResolver::new(root)) {
            Ok(m) => m,
            Err(errors) => {
                let text: Vec<String> = errors.iter().map(ToString::to_string).collect();
                return format!("did not compile: {text:?}");
            }
        };
        match Interpreter::new(&module).run("probe") {
            Ok(Value::Int(n)) => n.to_string(),
            Ok(other) => format!("{other:?}"),
            Err(fault) => format!("a fault: {fault}"),
        }
    })
}

fn expect(entry: &str, modules: &[(&str, &str)], expected: i128) {
    let got = answer(entry, modules);
    assert_eq!(
        got,
        expected.to_string(),
        "the program must answer {expected}.\nentry:\n{entry}\nmodules: {modules:#?}"
    );
}

// A private name in a module means that module's definition.

#[test]
fn an_imported_function_calls_its_own_private_function() {
    expect(
        "use lib::open\nfn secret() -> I32 { 1 }\npub fn probe() -> I32 { open() * 10 + secret() }\n",
        &[(
            "lib",
            "fn secret() -> I32 { 42 }\npub fn open() -> I32 { secret() }\n",
        )],
        421,
    );
}

#[test]
fn an_imported_function_reads_its_own_private_let() {
    expect(
        "use lib::get\nlet base: I32 = 1\npub fn probe() -> I32 { get() + base }\n",
        &[("lib", "let base: I32 = 100\npub fn get() -> I32 { base }\n")],
        101,
    );
}

#[test]
fn an_imported_default_argument_calls_its_own_private_function() {
    expect(
        "use lib::add\nfn seed() -> I32 { 1 }\npub fn probe() -> I32 { add(a: 1) }\n",
        &[(
            "lib",
            "fn seed() -> I32 { 30 }\npub fn add(a: I32, b: I32 = seed()) -> I32 { a + b }\n",
        )],
        31,
    );
}

#[test]
fn an_imported_function_uses_its_own_private_struct() {
    expect(
        "use lib::make\nstruct Inner { b: I32, c: I32 }\n\
         pub fn probe() -> I32 { make() + Inner(b: 1, c: 1).b }\n",
        &[(
            "lib",
            "struct Inner { a: I32 }\npub fn make() -> I32 { Inner(a: 40).a }\n",
        )],
        41,
    );
}

#[test]
fn an_imported_method_calls_its_own_private_function() {
    expect(
        "use lib::Counter\nfn helper(x: I32) -> I32 { x + 1000 }\n\
         pub fn probe() -> I32 { Counter(v: 4).doubled() }\n",
        &[(
            "lib",
            "pub struct Counter { v: I32 }\n\
             impl Counter { fn doubled(self) -> I32 { helper(x: self.v) } }\n\
             fn helper(x: I32) -> I32 { x * 2 }\n",
        )],
        8,
    );
}

// An imported type keeps its impl block.

#[test]
fn an_imported_struct_keeps_its_methods() {
    expect(
        "use lib::Counter\npub fn probe() -> I32 { Counter(v: 4).doubled() }\n",
        &[(
            "lib",
            "pub struct Counter { v: I32 }\n\
             impl Counter { fn doubled(self) -> I32 { self.v * 2 } }\n",
        )],
        8,
    );
}

#[test]
fn an_imported_trait_satisfies_a_local_bound() {
    expect(
        "use lib::{Area, Sq}\nfn total<T: Area>(x: T) -> I32 { x.area() }\n\
         pub fn probe() -> I32 { total(x: Sq(s: 3)) }\n",
        &[(
            "lib",
            "pub trait Area { fn area(self) -> I32 }\n\
             pub struct Sq { s: I32 }\n\
             impl Area for Sq { fn area(self) -> I32 { self.s * self.s } }\n",
        )],
        9,
    );
}

// Generics and enums across modules.

#[test]
fn an_imported_generic_function_and_struct() {
    expect(
        "use lib::{identity, Box}\npub fn probe() -> I32 { identity(x: 4) + Box(value: 3).value }\n",
        &[(
            "lib",
            "pub fn identity<T>(x: T) -> T { x }\npub struct Box<T> { value: T }\n",
        )],
        7,
    );
}

#[test]
fn an_imported_generic_struct_at_two_types() {
    expect(
        "use lib::Box\n\
         pub fn probe() -> I32 { if Box(value: \"s\").value == \"s\" { Box(value: 5).value } else { 0 } }\n",
        &[("lib", "pub struct Box<T> { value: T }\n")],
        5,
    );
}

#[test]
fn an_imported_enum_with_a_leading_dot_variant() {
    expect(
        "use lib::{Shape, area}\n\
         pub fn probe() -> I32 { area(x: Shape.square(s: 4)) + area(x: .circle(r: 1)) }\n",
        &[(
            "lib",
            "pub enum Shape { circle(r: I32), square(s: I32) }\n\
             pub fn area(x: Shape) -> I32 { match x { .circle(r): r * 3, .square(s): s * s } }\n",
        )],
        19,
    );
}

#[test]
fn an_imported_enum_beside_a_local_enum_with_the_same_variants() {
    expect(
        "use lib::{Mode, speed}\npub enum Local { slow, fast }\n\
         pub fn probe() -> I32 { speed(m: Mode.slow) * 100 + speed(m: Mode.fast) }\n",
        &[(
            "lib",
            "pub enum Mode { fast, slow }\n\
             pub fn speed(m: Mode) -> I32 { match m { .fast: 10, .slow: 1 } }\n",
        )],
        110,
    );
}

#[test]
fn an_imported_function_that_takes_a_closure() {
    expect(
        "use lib::apply\npub fn probe() -> I32 {\n    let k = 2\n    apply(f: (n: I32) -> n * k, x: 21)\n}\n",
        &[(
            "lib",
            "pub fn apply(f: (I32) -> I32, x: I32) -> I32 { f(x) }\n",
        )],
        42,
    );
}

// Types that arrive through another module.

#[test]
fn a_value_of_a_type_from_a_module_two_imports_away() {
    expect(
        "use left::l\nuse right::r\npub fn probe() -> I32 { l().v * 10 + r().v }\n",
        &[
            (
                "base",
                "pub struct P { v: I32 }\npub fn mk(v: I32) -> P { P(v: v) }\n",
            ),
            ("left", "use base::{P, mk}\npub fn l() -> P { mk(v: 1) }\n"),
            ("right", "use base::{P, mk}\npub fn r() -> P { mk(v: 2) }\n"),
        ],
        12,
    );
}

#[test]
fn an_import_and_a_local_function_of_another_name() {
    expect(
        "use lib::helper\nfn local_helper(a: I32) -> I32 { a + 100 }\n\
         pub fn probe() -> I32 { helper(a: 1) + local_helper(a: 0) }\n",
        &[("lib", "pub fn helper(a: I32) -> I32 { a + 1 }\n")],
        102,
    );
}

/// `use lib::helper` names the module file `lib.fv`. A local `mod lib`
/// of the same name makes the path mean two things. The compiler must
/// refuse the ambiguity, or take the import. It must not take the
/// local module without a word.
#[test]
fn a_local_module_does_not_silently_replace_an_imported_one() {
    let got = answer(
        "use lib::helper\npub mod lib { pub fn helper(a: I32) -> I32 { a + 1000 } }\n\
         pub fn probe() -> I32 { helper(a: 1) }\n",
        &[("lib", "pub fn helper(a: I32) -> I32 { a + 1 }\n")],
    );
    assert!(
        got == "2" || got.starts_with("did not compile"),
        "the import must win, or the compiler must refuse; the answer is {got}"
    );
}
