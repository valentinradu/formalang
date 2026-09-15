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

use formalang::error::CompilerError;
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

// ---------------------------------------------------------------------
// Linearity: exactly one thing consumes a sequence
// ---------------------------------------------------------------------

fn errors_of(source: &str) -> Vec<CompilerError> {
    compile_to_ir(source).expect_err("source must be rejected")
}

fn has_not_consumed(errors: &[CompilerError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, CompilerError::SeqNotConsumed { .. }))
}

/// The case that made linearity necessary. Lazy plus dropped means the
/// log never runs, and before this the author got silence.
#[test]
fn a_dropped_effectful_loop_is_rejected() {
    let errors = errors_of(
        "extern fn log(message: String)
pub fn f(xs: [I32]) -> I32 {
  for x in xs { log(message: \"x\") }
  0
}",
    );
    assert!(
        has_not_consumed(&errors),
        "expected SeqNotConsumed, got {errors:?}"
    );
}

/// A pure dropped loop computes nothing either. One rule covers both,
/// which is why no effect analysis is needed.
#[test]
fn a_dropped_pure_loop_is_rejected() {
    let errors = errors_of(
        "pub fn f(xs: [I32]) -> I32 {
  for x in xs { x * 2 }
  0
}",
    );
    assert!(
        has_not_consumed(&errors),
        "expected SeqNotConsumed, got {errors:?}"
    );
}

#[test]
fn a_bound_but_unread_sequence_is_rejected() {
    let errors = errors_of(
        "pub fn f(xs: [I32]) -> I32 {
  let s = for x in xs { x }
  0
}",
    );
    assert!(
        has_not_consumed(&errors),
        "expected SeqNotConsumed, got {errors:?}"
    );
}

/// A sequence runs once and keeps nothing, so a second read cannot
/// mean what the author wants.
#[test]
fn reading_a_sequence_twice_is_rejected() {
    let errors = errors_of(
        "pub fn f(xs: [I32]) -> I32 {
  let s = for x in xs { x }
  let a: I32 = s.count()
  let b: I32 = s.count()
  a + b
}",
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CompilerError::SeqUsedTwice { name, .. } if name == "s"
        )),
        "expected SeqUsedTwice for s, got {errors:?}"
    );
}

// ---------------------------------------------------------------------
// ... and what stays legal
// ---------------------------------------------------------------------

#[test]
fn run_satisfies_an_effectful_loop() {
    compile_and_lower(
        "extern fn log(message: String)
pub fn f(xs: [I32]) -> I32 {
  for x in xs { log(message: \"x\") }.run()
  0
}",
    );
}

#[test]
fn a_bound_sequence_read_once_is_fine() {
    compile_and_lower(
        "pub fn f(xs: [I32]) -> I32 {
  let s = for x in xs { x }
  s.count()
}",
    );
}

/// Two separate loops are two separate sequences. Only reading the
/// same one twice is an error.
#[test]
fn two_loops_over_the_same_array_are_fine() {
    compile_and_lower(
        "pub fn f(xs: [I32]) -> I32 {
  let a: I32 = for x in xs { x }.count()
  let b: I32 = for x in xs { x }.count()
  a + b
}",
    );
}

// ---------------------------------------------------------------------
// Placement: where a sequence may appear
// ---------------------------------------------------------------------

fn has_invalid_position(errors: &[CompilerError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, CompilerError::SeqInvalidPosition { .. }))
}

fn assert_position_rejected(source: &str) {
    let errors = errors_of(source);
    assert!(
        has_invalid_position(&errors),
        "expected SeqInvalidPosition, got {errors:?}"
    );
}

#[test]
fn a_sequence_cannot_be_a_struct_field() {
    assert_position_rejected("pub struct Holder { s: Seq<I32> }");
}

#[test]
fn a_sequence_cannot_be_an_enum_payload() {
    assert_position_rejected("pub enum Node { wrap(s: Seq<I32>) }");
}

#[test]
fn a_sequence_cannot_be_a_function_return_type() {
    assert_position_rejected("pub fn f(xs: [I32]) -> Seq<I32> { for x in xs { x } }");
}

#[test]
fn a_sequence_cannot_be_a_module_level_let() {
    assert_position_rejected("pub let g: Seq<I32> = for x in [1, 2] { x }");
}

/// A sequence parameter must be consumed by the callee, and `sink` is
/// how the language says exactly that.
#[test]
fn a_sequence_parameter_must_be_sink() {
    assert_position_rejected("fn f(s: Seq<I32>) -> I32 { s.count() }");
}

/// Nesting one inside a container is the same mistake: the container
/// would have to store something that cannot be stored.
#[test]
fn a_sequence_cannot_hide_inside_a_container() {
    assert_position_rejected("pub struct Holder { rows: [Seq<I32>] }");
    assert_position_rejected("pub struct Holder { row: Seq<I32>? }");
}

#[test]
fn a_sink_sequence_parameter_is_allowed() {
    compile_and_lower(
        "fn total(sink s: Seq<I32>) -> I32 { s.count() }
pub fn f(xs: [I32]) -> I32 {
  let s = for x in xs { x }
  total(s: s)
}",
    );
}

/// An `extern fn` returning a sequence is the host cursor: the host
/// produces the elements and the generated code pulls them. That is
/// the one return position a sequence may hold.
#[test]
fn an_extern_fn_may_return_a_sequence() {
    compile_and_lower(
        "pub struct Row { v: I32 }
extern fn rows() -> Seq<Row>
pub fn f() -> I32 { for r in rows() { r.v }.count() }",
    );
}
