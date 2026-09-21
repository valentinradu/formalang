//! `IrFunction` carries the `pub` the author wrote.
//!
//! The AST's `FunctionDef` always had a `visibility` field, but
//! lowering dropped it, so nothing downstream could tell a public
//! function from a private one. A backend keys its export list on
//! exactly this: a `pub fn` becomes a symbol the host calls by name,
//! everything else stays internal.

#![expect(
    clippy::expect_used,
    clippy::unreachable,
    reason = "tests assert compilation succeeds; panicking on failure is the desired shape"
)]

use formalang::ast::Visibility;
use formalang::compile_to_ir;
use formalang::ir::IrModule;

fn visibility_of(module: &IrModule, name: &str) -> Visibility {
    module
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| unreachable!("function {name} not found"))
        .visibility
}

#[test]
fn carries_pub_through_lowering() {
    let module = compile_to_ir(
        "pub fn exported() -> I32 { 1 }
fn internal() -> I32 { 2 }
pub let n: I32 = exported() + internal()",
    )
    .expect("source compiles");

    assert_eq!(visibility_of(&module, "exported"), Visibility::Public);
    assert_eq!(visibility_of(&module, "internal"), Visibility::Private);
}

#[test]
fn an_extern_fn_keeps_its_visibility() {
    let module = compile_to_ir(
        "pub extern fn host_log(message: String)
extern fn host_tick() -> I32",
    )
    .expect("source compiles");

    assert_eq!(visibility_of(&module, "host_log"), Visibility::Public);
    assert_eq!(visibility_of(&module, "host_tick"), Visibility::Private);
}

/// A method's reach is its impl's, and an impl follows the type it is
/// written for. So a method is never an export in its own right.
#[test]
fn an_impl_method_is_private() {
    let module = compile_to_ir(
        "pub struct Counter { value: I32 }
impl Counter { fn bump(self) -> I32 { self.value + 1 } }",
    )
    .expect("source compiles");

    let method = module
        .impls
        .iter()
        .flat_map(|i| i.functions.iter())
        .find(|f| f.name == "bump")
        .expect("bump not found");
    assert_eq!(method.visibility, Visibility::Private);
}

/// Closure conversion lifts each closure body to a top-level function.
/// Those are internal call targets and must never reach an export list.
#[test]
fn a_lifted_closure_is_private() {
    let module = compile_to_ir(
        "pub enum Event { pressed }
struct Button { on_press: () -> Event = () -> .pressed }
pub let b: I32 = 0",
    )
    .expect("source compiles");

    let module = formalang::Pipeline::new()
        .pass(formalang::ir::ClosureConversionPass::new())
        .run(module)
        .expect("closure conversion succeeds");

    let lifted: Vec<_> = module
        .functions
        .iter()
        .filter(|f| f.name.starts_with("__closure"))
        .collect();
    assert!(!lifted.is_empty(), "expected at least one lifted closure");
    for f in lifted {
        assert_eq!(
            f.visibility,
            Visibility::Private,
            "lifted closure {} must stay internal",
            f.name
        );
    }
}

/// `Visibility` defaults to private, so a hand-built `IrFunction` and
/// IR serialised before the field existed both stay internal rather
/// than leaking as exports.
#[test]
fn visibility_defaults_to_private() {
    assert_eq!(Visibility::default(), Visibility::Private);
}
