//! `for` yields a lazy sequence, not an array.
//!
//! Before this, every loop allocated: `IrExpr::For` carried
//! `ty: Array(body_type)`, so a three-stage pipeline over a million
//! values allocated three arrays to produce one number. A loop now
//! produces `Seq<T>` and runs nothing until a terminal combinator
//! consumes it, so a pipeline collapses into one pass with no
//! intermediate collection.
//!
//! `collect()` is the only step that allocates for the data, and it
//! is visible on the line that asks for it.

#![expect(
    clippy::expect_used,
    reason = "tests assert compilation succeeds; expect() is the desired panic-on-failure shape"
)]

use formalang::ir::{walk_expr_children, walk_module, IrExpr, IrModule, IrVisitor, ResolvedType};
use formalang::{compile_to_ir, Pipeline};

fn compile(body: &str) -> IrModule {
    compile_to_ir(body).expect("source compiles")
}

/// Compile and push the result through the canonical codegen
/// pipeline, which rejects any type parameter left unspecialised.
fn compile_and_lower(body: &str) -> IrModule {
    let module = compile(body);
    Pipeline::for_codegen()
        .run(module)
        .expect("codegen pipeline accepts the module")
}

/// The resolved type of the first `For` node found anywhere.
struct ForType(Option<ResolvedType>);

impl IrVisitor for ForType {
    fn visit_expr(&mut self, expr: &IrExpr) {
        if self.0.is_none() {
            if let IrExpr::For { ty, .. } = expr {
                self.0 = Some(ty.clone());
            }
        }
        walk_expr_children(self, expr);
    }
}

// ---------------------------------------------------------------------
// The type of a loop
// ---------------------------------------------------------------------

#[test]
fn a_loop_yields_a_sequence_not_an_array() {
    let module = compile("pub fn f(xs: [I32]) -> I32 { for x in xs { x }.count() }");
    let mut found = ForType(None);
    walk_module(&mut found, &module);
    let ty = found.0.expect("a For node exists");

    let seq = module
        .prelude_seq_id()
        .expect("the prelude declares Seq<T>");
    assert!(
        matches!(&ty, ResolvedType::Generic { base, .. }
            if *base == formalang::ir::GenericBase::Struct(seq)),
        "a loop must produce Seq<T>, got {ty:?}"
    );
    assert!(
        module.array_element_ty(&ty).is_none(),
        "a loop must not produce an array"
    );
}

#[test]
fn a_loop_assigned_to_an_array_is_a_mismatch() {
    let errors = compile_to_ir("pub let xs: [I32] = for x in [1, 2, 3] { x }")
        .expect_err("a sequence is not an array");
    assert!(
        errors.iter().any(|e| format!("{e:?}").contains("Seq")),
        "the mismatch should name the sequence type: {errors:?}"
    );
}

// ---------------------------------------------------------------------
// Terminals
// ---------------------------------------------------------------------

#[test]
fn collect_produces_an_array() {
    compile_and_lower("pub fn f(xs: [I32]) -> [I32] { for x in xs { x * 2 }.collect() }");
}

#[test]
fn count_produces_an_integer() {
    compile_and_lower("pub fn f(xs: [I32]) -> I32 { for x in xs { x }.count() }");
}

#[test]
fn fold_reduces_to_one_value() {
    compile_and_lower(
        "pub fn f(xs: [I32]) -> I32 {
  for x in xs { x }.fold(initial: 0, f: (a, b) -> a + b)
}",
    );
}

#[test]
fn first_produces_an_optional() {
    compile_and_lower("pub fn f(xs: [I32]) -> I32? { for x in xs { x }.first() }");
}

#[test]
fn run_executes_for_effects() {
    compile_and_lower(
        "extern fn log(message: String)
pub fn f(xs: [I32]) { for x in xs { log(message: \"x\") }.run() }",
    );
}

#[test]
fn any_and_all_produce_booleans() {
    compile_and_lower(
        "pub fn f(xs: [I32]) -> Boolean { for x in xs { x }.any(f: (v) -> v > 2) }
pub fn g(xs: [I32]) -> Boolean { for x in xs { x }.all(f: (v) -> v > 2) }",
    );
}

// ---------------------------------------------------------------------
// Adapters and chaining
// ---------------------------------------------------------------------

#[test]
fn a_three_stage_pipeline_chains() {
    compile_and_lower(
        "pub fn f(xs: [I32]) -> I32 {
  for x in xs { x * 2 }.filter(f: (v) -> v > 4).count()
}",
    );
}

#[test]
fn take_and_skip_chain() {
    compile_and_lower(
        "pub fn f(xs: [I32]) -> I32? {
  for x in xs { x }.skip(count: 1).take(count: 2).first()
}",
    );
}

/// An un-annotated closure parameter takes its type from the
/// combinator's signature, instantiated at the receiver's element
/// type. Without that substitution `T` survives into monomorphisation.
#[test]
fn an_unannotated_combinator_closure_takes_the_element_type() {
    compile_and_lower(
        "pub fn f(xs: [I32]) -> I32 { for x in xs { x }.map(f: (v) -> v + 1).count() }",
    );
}

// ---------------------------------------------------------------------
// Sources
// ---------------------------------------------------------------------

#[test]
fn a_range_is_a_source() {
    compile_and_lower("pub fn f(n: I32) -> I32 { for i in 0..n { i }.count() }");
}

/// A sequence is itself iterable, so one pipeline feeds the next.
#[test]
fn a_sequence_is_a_source() {
    compile_and_lower(
        "pub fn f(xs: [I32]) -> [I32] {
  for y in (for x in xs { x * 2 }) { y + 1 }.collect()
}",
    );
}

/// The inner sequence is created and consumed within one step of the
/// outer one, so nesting holds without either loop materialising.
#[test]
fn loops_nest() {
    compile_and_lower(
        "pub fn f(xs: [I32]) -> I32 {
  for x in xs {
    for y in xs { x + y }.fold(initial: 0, f: (a, b) -> a + b)
  }.fold(initial: 0, f: (a, b) -> a + b)
}",
    );
}

// ---------------------------------------------------------------------
// The loop variable's type
// ---------------------------------------------------------------------

/// A loop variable was never given a type, so every loop body inferred
/// as `Unknown` and a loop in any declared position looked like a
/// mismatch. It now binds the collection's element type.
#[test]
fn the_loop_variable_carries_the_element_type() {
    compile_and_lower(
        "pub struct Item { value: String }
pub fn f(xs: [Item]) -> [String] { for x in xs { x.value }.collect() }",
    );
}

#[test]
fn a_nested_loop_variable_shadows_correctly() {
    compile_and_lower(
        "pub fn f(rows: [[I32]]) -> [[I32]] {
  for row in rows { for cell in row { cell + 1 }.collect() }.collect()
}",
    );
}
