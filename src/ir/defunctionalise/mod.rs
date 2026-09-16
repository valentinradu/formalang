//! Defunctionalisation: turn every closure value into an enum tag.
//!
//! [`ClosureConversionPass`](crate::ir::ClosureConversionPass) answers
//! "where does the body live?" — it lifts each body to a top-level
//! function and collects the captures into an env struct. This pass
//! answers the remaining question: **what is the closure value?**
//!
//! Two answers are possible. A closure value can be the *address* of
//! the lifted function paired with a pointer to its env, called
//! through an indirect jump. Or it can be a small number saying which
//! lifted function it is, dispatched by a `match`. This pass takes the
//! second.
//!
//! ```text
//! address form:    arena bytes → address → jump
//! defunctionalised: arena bytes → tag → match → direct call
//! ```
//!
//! # Why the tag, and not the address
//!
//! With the address form a code address lives inside a data value. Any
//! defect that corrupts those bytes becomes a jump to anywhere. With a
//! tag, the same corruption at worst selects the wrong arm: a wrong
//! answer, not a takeover. The generated module contains **no indirect
//! call**, so control-flow hijack is not expressible in it.
//!
//! A backend also gains: every call target is known, so each arm is a
//! direct call the target can inline.
//!
//! # What it builds
//!
//! One enum per distinct closure *type*, with one variant per lifted
//! function of that type. Each variant carries the env struct that
//! closure conversion already synthesised, so the captures travel
//! unchanged:
//!
//! ```text
//! enum __Fn0 {
//!     __closure0(env: __ClosureEnv0),
//!     __closure1(env: __ClosureEnv1),
//! }
//!
//! fn __call_Fn0(f: __Fn0, x: I32) -> I32 {
//!     match f {
//!         .__closure0(env): __closure0(__env: env, x: x),
//!         .__closure1(env): __closure1(__env: env, x: x),
//!     }
//! }
//! ```
//!
//! Then every `ClosureRef` becomes an `EnumInst`, every `CallClosure`
//! becomes a `FunctionCall` to `__call_Fn<K>`, and every occurrence of
//! the closure type becomes the enum type.
//!
//! # Closed world
//!
//! The pass has to see every closure of a shape before it can build
//! that shape's enum. A just-in-time backend compiles the whole
//! program at once, so this holds there. It would not hold under
//! separate compilation, which is why the pass is opt-in rather than
//! part of [`Pipeline::for_codegen`](crate::Pipeline::for_codegen).
//!
//! # Pipeline placement
//!
//! Run **after** `ClosureConversionPass`, which this pass consumes the
//! output of, and **before** `DeadCodeEliminationPass`, so the
//! synthesised enums and call functions face the same reachability
//! sweep as hand-written definitions.
//!
//! # Post-pass invariant
//!
//! No [`IrExpr::ClosureRef`] and no [`IrExpr::CallClosure`] remain.
//! The pass verifies this and reports any residual as an
//! [`CompilerError::InternalError`] — a violation is a bug here, not
//! bad input.

mod collect;
mod rewrite;
mod rewrite_expr;
mod synthesis;

use crate::error::CompilerError;
use crate::ir::{IrExpr, IrModule};
use crate::location::Span;
use crate::pipeline::IrPass;

/// Name prefix for a synthesised closure-tag enum.
const FN_ENUM_PREFIX: &str = "__Fn";

/// Name prefix for a synthesised dispatch function.
const CALL_FN_PREFIX: &str = "__call_Fn";

/// Field name holding the capture environment on each enum variant.
const ENV_FIELD_NAME: &str = "env";

/// Defunctionalisation pass.
///
/// See the module documentation for the algorithm and the reasoning.
#[expect(
    clippy::exhaustive_structs,
    reason = "no fields planned; a knob would be added explicitly later"
)]
#[derive(Debug, Clone, Default)]
pub struct DefunctionalisePass;

impl DefunctionalisePass {
    /// Create a new defunctionalisation pass.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl IrPass for DefunctionalisePass {
    fn name(&self) -> &'static str {
        "defunctionalise"
    }

    fn run(&mut self, module: IrModule) -> Result<IrModule, Vec<CompilerError>> {
        // Closure conversion must have run: this pass reads the
        // `ClosureRef` nodes it produces. Running it again is a cheap
        // no-op walk when it already has.
        let mut module = crate::ir::ClosureConversionPass::new().run(module)?;

        let shapes = collect::closure_shapes(&module);
        if shapes.is_empty() {
            return Ok(module);
        }

        let plan = synthesis::synthesise(&mut module, shapes)?;
        rewrite::apply(&mut module, &plan);
        module.rebuild_indices();

        let residuals = find_residuals(&module);
        if !residuals.is_empty() {
            return Err(residuals
                .into_iter()
                .map(|detail| CompilerError::InternalError {
                    detail: format!("defunctionalise: {detail}"),
                    span: Span::default(),
                })
                .collect());
        }

        // The rewrite introduced `Reference` nodes for the match-arm
        // bindings and the dispatch parameters. Resolve them.
        crate::ir::ResolveReferencesPass::new().run(module)
    }
}

/// Describe every closure value or indirect call still in the module.
/// Empty when the post-pass invariant holds.
fn find_residuals(module: &IrModule) -> Vec<String> {
    struct Residuals(Vec<String>);
    impl crate::ir::IrVisitor for Residuals {
        fn visit_expr(&mut self, expr: &IrExpr) {
            match expr {
                IrExpr::ClosureRef { funcref, .. } => self.0.push(format!(
                    "closure value for `{}` remains",
                    funcref.join("::")
                )),
                IrExpr::CallClosure { .. } => self.0.push("indirect call remains".to_string()),
                IrExpr::Literal { .. }
                | IrExpr::StructInst { .. }
                | IrExpr::EnumInst { .. }
                | IrExpr::Array { .. }
                | IrExpr::Tuple { .. }
                | IrExpr::Reference { .. }
                | IrExpr::SelfFieldRef { .. }
                | IrExpr::FieldAccess { .. }
                | IrExpr::LetRef { .. }
                | IrExpr::BinaryOp { .. }
                | IrExpr::UnaryOp { .. }
                | IrExpr::If { .. }
                | IrExpr::For { .. }
                | IrExpr::Match { .. }
                | IrExpr::FunctionCall { .. }
                | IrExpr::MethodCall { .. }
                | IrExpr::Closure { .. }
                | IrExpr::DictLiteral { .. }
                | IrExpr::DictAccess { .. }
                | IrExpr::Block { .. } => {}
            }
            crate::ir::walk_expr_children(self, expr);
        }
    }
    let mut found = Residuals(Vec::new());
    crate::ir::walk_module(&mut found, module);
    found.0
}
