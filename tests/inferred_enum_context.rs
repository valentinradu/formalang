//! Regression coverage for inferred-enum (`.variant`) resolution.
//!
//! A `.variant` literal carries no enum name, so lowering must read the
//! enum from whatever type the surrounding expression expects. Before
//! this suite, lowering consulted a single stringly-typed "current
//! function return type" slot. That slot flattened `[Shape]` to the
//! name `"Array"` and carried the wrong type entirely into a call
//! argument, so both positions raised `[E934] Internal compiler error:
//! inferred-enum ... has no resolvable return-type enum`.
//!
//! Lowering now threads the expected `ResolvedType` structurally. Every
//! test below pins one position that must resolve.

#![expect(
    clippy::expect_used,
    reason = "tests assert compilation succeeds; expect() is the desired panic-on-failure shape"
)]

use formalang::error::CompilerError;
use formalang::ir::{walk_expr_children, walk_module, IrExpr, IrModule, IrVisitor, ResolvedType};
use formalang::{compile_to_ir, Pipeline};

/// Count `EnumInst` nodes that never found their enum. An unresolved
/// inferred enum lowers to `TypeParam("InferredEnum")` with no
/// `enum_id`, which `MonomorphisePass` later rejects as leftover.
struct UnresolvedEnums(usize);

impl IrVisitor for UnresolvedEnums {
    fn visit_expr(&mut self, expr: &IrExpr) {
        if let IrExpr::EnumInst { enum_id, ty, .. } = expr {
            let unresolved = enum_id.is_none()
                && matches!(ty, ResolvedType::TypeParam(name) if name == "InferredEnum");
            if unresolved {
                self.0 = self.0.saturating_add(1);
            }
        }
        walk_expr_children(self, expr);
    }
}

fn count_unresolved(module: &IrModule) -> usize {
    let mut counter = UnresolvedEnums(0);
    walk_module(&mut counter, module);
    counter.0
}

/// Compile `source` and assert every `.variant` found its enum.
///
/// Runs the canonical codegen pipeline too: `MonomorphisePass` reports
/// a leftover `TypeParam` as an internal error, so a pass there is an
/// independent check that nothing stayed unresolved.
fn assert_all_variants_resolve(source: &str) {
    let module = compile_to_ir(source).expect("source compiles");
    assert_eq!(
        count_unresolved(&module),
        0,
        "every inferred `.variant` must resolve to an enum"
    );
    Pipeline::for_codegen()
        .run(module)
        .expect("codegen pipeline leaves no unresolved type parameter");
}

const PRELUDE: &str = "
pub struct Square { side: I32 }
pub struct Rect { w: I32, h: I32 }
pub enum Shape { square(s: Square), rect(r: Rect) }
pub enum Status { pending, active }
";

#[test]
fn resolves_in_let_annotation() {
    assert_all_variants_resolve(&format!("{PRELUDE}\npub let s: Status = .pending"));
}

#[test]
fn resolves_in_array_element() {
    assert_all_variants_resolve(&format!(
        "{PRELUDE}
pub let xs: [Status] = [.pending, .active]"
    ));
}

#[test]
fn resolves_in_array_element_with_payload() {
    assert_all_variants_resolve(&format!(
        "{PRELUDE}
pub let xs: [Shape] = [.square(s: Square(side: 1)), .rect(r: Rect(w: 2, h: 3))]"
    ));
}

#[test]
fn resolves_in_call_argument() {
    assert_all_variants_resolve(&format!(
        "{PRELUDE}
fn describe(status: Status) -> I32 {{ match status {{ .pending: 0, .active: 1 }} }}
pub let n: I32 = describe(status: .active)"
    ));
}

#[test]
fn resolves_in_call_argument_with_payload() {
    assert_all_variants_resolve(&format!(
        "{PRELUDE}
fn area_of(shape: Shape) -> I32 {{
  match shape {{ .square(s): s.side, .rect(r): r.w }}
}}
pub let n: I32 = area_of(shape: .square(s: Square(side: 4)))"
    ));
}

#[test]
fn resolves_in_struct_field_argument() {
    assert_all_variants_resolve(&format!(
        "{PRELUDE}
pub struct Task {{ status: Status, name: String }}
pub let t: Task = Task(status: .pending, name: \"build\")"
    ));
}

#[test]
fn resolves_in_struct_field_default() {
    assert_all_variants_resolve(&format!(
        "{PRELUDE}
pub struct Task {{ status: Status = .pending, name: String }}
pub let t: Task = Task(name: \"build\")"
    ));
}

#[test]
fn resolves_in_function_return_position() {
    assert_all_variants_resolve(&format!(
        "{PRELUDE}
pub fn initial() -> Status {{ .pending }}"
    ));
}

#[test]
fn resolves_in_dictionary_value() {
    assert_all_variants_resolve(&format!(
        "{PRELUDE}
pub let d: [String: Status] = [\"build\": .pending, \"test\": .active]"
    ));
}

#[test]
fn resolves_in_optional_annotation() {
    assert_all_variants_resolve(&format!("{PRELUDE}\npub let s: Status? = .active"));
}

#[test]
fn resolves_in_nested_enum_payload() {
    assert_all_variants_resolve(&format!(
        "{PRELUDE}
pub enum Outer {{ wrap(inner: Status) }}
pub let o: Outer = .wrap(inner: .pending)"
    ));
}

#[test]
fn resolves_in_nested_array_of_arrays() {
    assert_all_variants_resolve(&format!(
        "{PRELUDE}
pub let xs: [[Status]] = [[.pending], [.active, .pending]]"
    ));
}

#[test]
fn resolves_in_if_branches() {
    assert_all_variants_resolve(&format!(
        "{PRELUDE}
pub fn pick(flag: Boolean) -> Status {{ if flag {{ .pending }} else {{ .active }} }}"
    ));
}

/// A closure literal without its own return annotation still knows the
/// return type, because the closure-typed field or parameter that
/// receives it declares one. Thread that through to the body.
#[test]
fn resolves_in_unannotated_closure_body() {
    assert_all_variants_resolve(&format!(
        "{PRELUDE}
struct Button {{ on_click: () -> Status = () -> .pending }}
pub let b: I32 = 0"
    ));
}

/// The same, with the closure type arriving as a call argument rather
/// than a field annotation.
#[test]
fn resolves_in_closure_argument_body() {
    assert_all_variants_resolve(&format!(
        "{PRELUDE}
fn run(f: () -> Status) -> Status {{ f() }}
pub let s: Status = run(f: () -> .active)"
    ));
}

/// With no expected type anywhere, the enum is genuinely unknowable
/// and the author must annotate. Report that directly instead of
/// leaking the old `TypeParam("InferredEnum")` placeholder, which
/// surfaced as the unhelpful `Undefined type 'InferredEnum'`.
#[test]
fn unannotated_let_asks_for_an_annotation() {
    let errors = compile_to_ir(&format!("{PRELUDE}\npub let s = .pending"))
        .expect_err("an un-annotated inferred enum cannot resolve");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CompilerError::CannotInferEnumType { variant, .. } if variant == "pending"
        )),
        "expected CannotInferEnumType, got {errors:?}"
    );
    assert!(
        !errors.iter().any(|e| matches!(
            e,
            CompilerError::UndefinedType { name, .. } if name == "InferredEnum"
        )),
        "the InferredEnum placeholder must not reach the user: {errors:?}"
    );
}
