//! Regression coverage for the IR-shape invariant on `complete.fv`:
//! after each stage of the canonical codegen pipeline, no expression
//! should carry `ResolvedType::Error`.
//!
//! Previously flagged as a `ResolvedType::Error` issue in
//! `MonomorphisePass` that blocked the full pipeline on `complete.fv`.
//! The fix landed before this test; this guard pins the
//! absence-of-regression rather than driving new behaviour.

#![expect(
    clippy::expect_used,
    reason = "test asserts pipeline success; expect() is the desired panic-on-failure shape"
)]

use formalang::compile_to_ir;
use formalang::ir::{
    walk_expr_children, walk_module, ClosureConversionPass, DeadCodeEliminationPass, IrExpr,
    IrModule, IrVisitor, MonomorphisePass, ResolveReferencesPass, ResolvedType,
};
use formalang::Pipeline;

struct ErrorCounter(usize);
impl IrVisitor for ErrorCounter {
    fn visit_expr(&mut self, expr: &IrExpr) {
        if matches!(expr.ty(), ResolvedType::Error) {
            self.0 = self.0.saturating_add(1);
        }
        walk_expr_children(self, expr);
    }
}

fn count_error_types(m: &IrModule) -> usize {
    let mut c = ErrorCounter(0);
    walk_module(&mut c, m);
    c.0
}

#[test]
fn complete_fv_has_no_error_types_through_the_full_pipeline() {
    let source = include_str!("fixtures/complete.fv");
    let module = compile_to_ir(source).expect("compile");
    assert_eq!(count_error_types(&module), 0, "before any pass");

    let mut p = Pipeline::new()
        .pass(MonomorphisePass::default())
        .pass(ResolveReferencesPass::new())
        .pass(ClosureConversionPass::new())
        .pass(DeadCodeEliminationPass::new());
    let m = p.run(module).expect("full pipeline");
    assert_eq!(
        count_error_types(&m),
        0,
        "after Mono+ResolveRefs+ClosureConv+DCE"
    );
}
