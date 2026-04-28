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
    walk_expr_children, walk_module, IrExpr, IrModule, IrVisitor, ReferenceTarget, ResolvedType,
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

struct UnresolvedTargetCounter(usize);
impl IrVisitor for UnresolvedTargetCounter {
    fn visit_expr(&mut self, expr: &IrExpr) {
        if let IrExpr::Reference { target, .. } = expr {
            if matches!(target, ReferenceTarget::Unresolved) {
                self.0 = self.0.saturating_add(1);
            }
        }
        walk_expr_children(self, expr);
    }
}

fn count_unresolved_targets(m: &IrModule) -> usize {
    let mut c = UnresolvedTargetCounter(0);
    walk_module(&mut c, m);
    c.0
}

#[test]
fn complete_fv_has_no_error_types_through_the_full_pipeline() {
    let source = include_str!("fixtures/complete.fv");
    let module = compile_to_ir(source).expect("compile");
    assert_eq!(count_error_types(&module), 0, "before any pass");

    let m = Pipeline::for_codegen().run(module).expect("full pipeline");
    assert_eq!(count_error_types(&m), 0, "after Pipeline::for_codegen()");
    // After the full pipeline, every `IrExpr::Reference` (including
    // the synthesised `__env` refs inside lifted closure bodies)
    // must carry a resolved `ReferenceTarget`. `Unresolved` here is
    // a regression: backends keying on `target` would emit broken
    // code or fail their own `UndefinedReference` validation.
    assert_eq!(
        count_unresolved_targets(&m),
        0,
        "after Pipeline::for_codegen()"
    );
}
