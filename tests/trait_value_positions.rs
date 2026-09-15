//! A trait is a constraint, never the type of a value.
//!
//! A trait-typed value needs a vtable and an indirect call at every
//! site, which a machine-code backend would have to invent per trait.
//! Both replacements dispatch statically: a generic bound when one
//! concrete type is enough, an enum when the choice happens at run
//! time.
//!
//! The first group pins the rejection. The second pins what must keep
//! working, because over-reaching here would take generic bounds with
//! it.

#![expect(
    clippy::expect_used,
    reason = "tests assert compilation succeeds; expect() is the desired panic-on-failure shape"
)]

use formalang::error::CompilerError;
use formalang::{compile_to_ir, Pipeline};

const TRAIT_AND_IMPLS: &str = "
pub trait Shape { fn area(self) -> I32 }
pub struct Square { side: I32 }
pub struct Rect { w: I32, h: I32 }
impl Shape for Square { fn area(self) -> I32 { self.side * self.side } }
impl Shape for Rect { fn area(self) -> I32 { self.w * self.h } }
";

/// Assert the source is rejected, naming `Shape` as a value type.
fn assert_rejected(body: &str) {
    let source = format!("{TRAIT_AND_IMPLS}{body}");
    let errors = compile_to_ir(&source).expect_err("a trait-typed value must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CompilerError::TraitUsedAsValueType { trait_name, .. } if trait_name == "Shape"
        )),
        "expected TraitUsedAsValueType for Shape, got {errors:?}"
    );
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, CompilerError::InternalError { .. })),
        "a rejected program must not also report a compiler bug: {errors:?}"
    );
}

fn assert_compiles(body: &str) {
    let source = format!("{TRAIT_AND_IMPLS}{body}");
    let module = compile_to_ir(&source).expect("source compiles");
    Pipeline::for_codegen()
        .run(module)
        .expect("codegen pipeline accepts the module");
}

// ---------------------------------------------------------------------
// Rejected: every position where a trait could be the type of a value
// ---------------------------------------------------------------------

#[test]
fn rejects_struct_field() {
    assert_rejected("pub struct Scene { root: Shape }");
}

#[test]
fn rejects_enum_variant_field() {
    assert_rejected("pub enum Node { leaf(value: Shape) }");
}

#[test]
fn rejects_function_parameter() {
    assert_rejected("pub fn area_of(shape: Shape) -> I32 { shape.area() }");
}

#[test]
fn rejects_function_return_type() {
    assert_rejected("pub fn pick() -> Shape { Square(side: 1) }");
}

#[test]
fn rejects_module_let_annotation() {
    assert_rejected("pub let s: Shape = Square(side: 1)");
}

/// The block-level `let` annotation went unvalidated entirely, so this
/// reached IR lowering and surfaced as an internal error rather than a
/// diagnostic the author can act on.
#[test]
fn rejects_block_let_annotation() {
    assert_rejected(
        "pub fn f(k: I32) -> I32 {
  let s: Shape = if k == 0 { Square(side: 1) } else { Rect(w: 2, h: 3) }
  s.area()
}",
    );
}

#[test]
fn rejects_array_element() {
    assert_rejected("pub let xs: [Shape] = [Square(side: 1)]");
}

#[test]
fn rejects_dictionary_value() {
    assert_rejected("pub let d: [String: Shape] = [\"a\": Square(side: 1)]");
}

#[test]
fn rejects_optional() {
    assert_rejected("pub let s: Shape? = nil");
}

#[test]
fn rejects_closure_parameter() {
    assert_rejected("pub struct Handler { f: (Shape) -> I32 }");
}

// ---------------------------------------------------------------------
// Still accepted: traits as constraints
// ---------------------------------------------------------------------

#[test]
fn accepts_generic_bound() {
    assert_compiles(
        "pub fn area_of<T: Shape>(shape: T) -> I32 { shape.area() }
pub let n: I32 = area_of(shape: Square(side: 3))",
    );
}

#[test]
fn accepts_generic_bound_on_a_struct() {
    assert_compiles(
        "pub struct Holder<T: Shape> { item: T }
pub let h: Holder<Square> = Holder<Square>(item: Square(side: 2))",
    );
}

#[test]
fn accepts_impl_trait_for_struct() {
    assert_compiles("pub let n: I32 = Square(side: 4).area()");
}

/// A composed trait declares nothing itself, so a bound written
/// `<T: Both>` has to reach through the composition to find the
/// parents' methods. Only the trait-value form used to resolve these,
/// so cutting it without this would have made composition useless.
#[test]
fn accepts_composed_trait_as_a_bound() {
    let source = "
pub trait Named { fn name(self) -> String }
pub trait Rendered { fn render(self) -> String }
pub trait NamedRendered: Named + Rendered {}

pub struct Widget { label: String }
impl Named for Widget { fn name(self) -> String { self.label } }
impl Rendered for Widget { fn render(self) -> String { self.label } }
impl NamedRendered for Widget {}

pub fn show<T: NamedRendered>(item: T) -> String { item.render() }
pub let s: String = show(item: Widget(label: \"hi\"))
";
    let module = compile_to_ir(source).expect("a composed trait works as a bound");
    Pipeline::for_codegen()
        .run(module)
        .expect("codegen pipeline accepts the module");
}

// ---------------------------------------------------------------------
// The replacement the diagnostic recommends
// ---------------------------------------------------------------------

#[test]
fn enum_replaces_a_mixed_collection() {
    assert_compiles(
        "pub enum AnyShape { square(value: Square), rect(value: Rect) }

pub fn area_of(shape: AnyShape) -> I32 {
  match shape { .square(value): value.area(), .rect(value): value.area() }
}

pub let a: AnyShape = .square(value: Square(side: 3))
pub let xs: [AnyShape] = [a]
pub let n: I32 = area_of(shape: a)",
    );
}

#[test]
fn enum_replaces_a_unified_branch() {
    assert_compiles(
        "pub enum AnyShape { square(value: Square), rect(value: Rect) }

pub fn pick(k: I32) -> I32 {
  let s: AnyShape = if k == 0 {
    .square(value: Square(side: 2))
  } else {
    .rect(value: Rect(w: 2, h: 3))
  }
  match s { .square(value): value.area(), .rect(value): value.area() }
}",
    );
}
