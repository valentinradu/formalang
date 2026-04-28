//! Closure-conversion pass.
//!
//! Lifts each [`IrExpr::Closure`] in the module to a top-level
//! [`IrFunction`] paired with a synthesized capture-environment
//! [`IrStruct`], replacing the original expression with an
//! [`IrExpr::ClosureRef`] that names the lifted function and constructs
//! the env value at runtime. The body of every lifted function is
//! rewritten so that captured-name reads (`Reference` / `LetRef`) load
//! from the env struct's fields instead.
//!
//! After the pass runs, no [`IrExpr::Closure`] node remains in the
//! module. Backends targeting languages without first-class closures
//! (notably the WASM component-model backend) can then lower the
//! resulting `funcref` + env pair as a function pointer plus a heap-
//! allocated record.
//!
//! # Pipeline placement
//!
//! Run **after** [`MonomorphisePass`](crate::ir::MonomorphisePass) so
//! that capture types and closure parameter types are concrete (no
//! leftover [`ResolvedType::TypeParam`](crate::ir::ResolvedType)).
//! Run **before**
//! [`DeadCodeEliminationPass`](crate::ir::DeadCodeEliminationPass) so
//! that the env structs and lifted functions go through the same
//! reachability sweep as hand-written definitions.
//!
//! # Algorithm
//!
//! A single recursive walk visits every site that can transitively
//! contain an [`IrExpr::Closure`] (module-level lets, function
//! bodies, impl-block method bodies, struct/enum field defaults,
//! function-parameter defaults). When a [`IrExpr::Closure`] node is
//! encountered:
//!
//! 1. A fresh `(__ClosureEnv<N>, __closure<N>)` name pair is
//!    allocated and an [`IrStruct`] is synthesized whose fields
//!    mirror the closure's `captures`.
//! 2. The closure's body is processed *recursively* (so nested
//!    closures get their own env / lifted-function pair) with the
//!    inner closure's captures driving the capture-rewrite.
//! 3. The processed body becomes the body of a new [`IrFunction`]
//!    whose first parameter is `__env: __ClosureEnv<N>`, followed by
//!    the closure's own parameters.
//! 4. The original [`IrExpr::Closure`] is replaced in place by an
//!    [`IrExpr::ClosureRef`] whose `funcref` names the lifted
//!    function and whose `env_struct` is a [`IrExpr::StructInst`]
//!    constructing the env. Each env field's value is the
//!    capture name as it appears in the *outer* scope — that
//!    expression goes through the same capture-rewrite, so a
//!    nested closure's env construction at an outer-closure-of-
//!    outer-closure capture site correctly produces
//!    `__env.<name>` rather than a raw `Reference`.
//!
//! # Closure-call sites — currently a no-op
//!
//! In a language with first-class closure invocation, the pass
//! would also rewrite calls of the form `f(arg)` (where `f` is a
//! closure-typed binding) into an indirect dispatch through the
//! lifted-function pointer carried by the closure value. Today,
//! `FormaLang`'s [`IrExpr::FunctionCall`] takes a path
//! ([`Vec<String>`]) that resolves only to *named* top-level
//! definitions — there is no surface syntax for applying a
//! closure-typed local. As a result, the converted IR still routes
//! every call through a top-level function, and the
//! [`ClosureRef`](IrExpr::ClosureRef) values produced here are only
//! consumed at points where the closure value itself is needed
//! (returned, stored in a `let`, passed as an argument). When the
//! language gains closure-application, the conversion will need a
//! callsite-rewrite step that targets this pass.
//!
//! [`IrExpr::FunctionCall`]: crate::ir::IrExpr::FunctionCall
//!
//! # Post-pass invariant
//!
//! After [`ClosureConversionPass::run`] returns successfully, no
//! [`IrExpr::Closure`] node remains anywhere in the module. The pass
//! verifies this before returning and reports any residual via
//! [`CompilerError::InternalError`] — a violation indicates a bug in
//! the pass itself (a missed walk site or recursion path), not user
//! input.
//!
//! # Numbering — iteration-order contract
//!
//! Synthesized names (`__ClosureEnv<N>`, `__closure<N>`) are
//! allocated by a single sequential counter as the recursive walk
//! encounters [`IrExpr::Closure`] nodes. The walk visits sites in
//! this fixed order:
//!
//! 1. Module-level `let` bindings (in declaration order).
//! 2. Standalone function bodies, then their parameter defaults
//!    (functions in declaration order).
//! 3. Impl-block method bodies, then their parameter defaults
//!    (impls in declaration order, methods in declaration order
//!    within each impl).
//! 4. Struct field defaults (structs in declaration order, fields
//!    in declaration order within each struct).
//! 5. Enum-variant field defaults (enums in declaration order,
//!    variants then fields in declaration order).
//!
//! Within each expression, sub-expressions are visited in a
//! left-to-right depth-first order; a nested closure inside a
//! parent closure's body is allocated *immediately after* the
//! parent (the recursion processes the parent's body before
//! returning to the parent's site).
//!
//! Backends that consume the synthesized names should treat them as
//! opaque — relying on a particular `<N>` value couples the backend
//! to this iteration order, and any change here will silently shift
//! the numbers. The
//! `numbering_follows_documented_walk_order` regression test in
//! `tests/closure_conv.rs` pins the order in place: if the test
//! breaks, either fix the regression or update both this section
//! and the test together.
//!
//! [`IrExpr::Closure`]: crate::ir::IrExpr::Closure
//! [`IrExpr::ClosureRef`]: crate::ir::IrExpr::ClosureRef
//! [`IrFunction`]: crate::ir::IrFunction
//! [`IrStruct`]: crate::ir::IrStruct

mod capture_rewrite;
mod env_synthesis;
mod lifting;
mod state;
#[cfg(test)]
mod tests;

use crate::error::CompilerError;
use crate::ir::{IrBlockStatement, IrExpr, IrModule};
use crate::location::Span;
use crate::pipeline::IrPass;

use self::capture_rewrite::CaptureCtx;
use self::state::ConversionState;

/// Name prefix for synthesized capture-environment structs.
const ENV_STRUCT_PREFIX: &str = "__ClosureEnv";

/// Name prefix for synthesized lifted closure functions.
const LIFTED_FN_PREFIX: &str = "__closure";

/// Parameter name used for the env-struct argument prepended to every
/// lifted closure function.
const ENV_PARAM_NAME: &str = "__env";

/// Closure-conversion pass.
///
/// See the module-level documentation in `src/ir/closure_conv/mod.rs`
/// for the full algorithm and pipeline placement.
#[expect(
    clippy::exhaustive_structs,
    reason = "no fields planned; flag-typed knobs would be added explicitly later"
)]
#[derive(Debug, Clone, Default)]
pub struct ClosureConversionPass;

impl ClosureConversionPass {
    /// Create a new closure-conversion pass.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl IrPass for ClosureConversionPass {
    fn name(&self) -> &'static str {
        "closure-conversion"
    }

    fn run(&mut self, mut module: IrModule) -> Result<IrModule, Vec<CompilerError>> {
        let mut state = ConversionState::new(&module);

        // Module-level let bindings.
        let lets = std::mem::take(&mut module.lets);
        module.lets = lets
            .into_iter()
            .map(|mut l| {
                l.value = state.process(l.value, &CaptureCtx::module_level());
                l
            })
            .collect();

        // Standalone function bodies.
        let functions = std::mem::take(&mut module.functions);
        module.functions = functions
            .into_iter()
            .map(|mut f| {
                f.body = f
                    .body
                    .map(|b| state.process(b, &CaptureCtx::module_level()));
                f.params = state.process_param_defaults(f.params);
                f
            })
            .collect();

        // Impl-block method bodies.
        let impls = std::mem::take(&mut module.impls);
        module.impls = impls
            .into_iter()
            .map(|mut i| {
                let methods = std::mem::take(&mut i.functions);
                i.functions = methods
                    .into_iter()
                    .map(|mut f| {
                        f.body = f
                            .body
                            .map(|b| state.process(b, &CaptureCtx::module_level()));
                        f.params = state.process_param_defaults(f.params);
                        f
                    })
                    .collect();
                i
            })
            .collect();

        // Struct field defaults.
        let structs = std::mem::take(&mut module.structs);
        module.structs = structs
            .into_iter()
            .map(|mut s| {
                let fields = std::mem::take(&mut s.fields);
                s.fields = fields
                    .into_iter()
                    .map(|mut field| {
                        field.default = field
                            .default
                            .map(|d| state.process(d, &CaptureCtx::module_level()));
                        field
                    })
                    .collect();
                s
            })
            .collect();

        // Enum variant field defaults (rare but possible).
        let enums = std::mem::take(&mut module.enums);
        module.enums = enums
            .into_iter()
            .map(|mut e| {
                for variant in &mut e.variants {
                    let fields = std::mem::take(&mut variant.fields);
                    variant.fields = fields
                        .into_iter()
                        .map(|mut field| {
                            field.default = field
                                .default
                                .map(|d| state.process(d, &CaptureCtx::module_level()));
                            field
                        })
                        .collect();
                }
                e
            })
            .collect();

        // Append synthesized envs and lifted functions in declaration
        // order (matches the recursive walk order, which makes the
        // resulting module deterministic).
        let (envs, lifted) = state.into_outputs();
        module.structs.extend(envs);
        module.functions.extend(lifted);

        module.rebuild_indices();

        // Post-pass invariant: after closure conversion, no
        // `IrExpr::Closure` may remain anywhere in the module. A
        // violation here is a bug in this pass (a missed walk site
        // or a recursion that skipped a sub-expression), so report
        // as `InternalError` with a span hint pointing at the
        // residual node's enclosing definition where possible.
        let residuals = find_residual_closures(&module);
        if !residuals.is_empty() {
            return Err(residuals
                .into_iter()
                .map(|location| CompilerError::InternalError {
                    detail: format!(
                        "closure-conversion: residual IrExpr::Closure remains in {location}"
                    ),
                    span: Span::default(),
                })
                .collect());
        }

        Ok(module)
    }
}

/// Walk the module and return a human-readable description of every
/// site that still contains an [`IrExpr::Closure`] node. Empty when
/// the post-pass invariant holds.
fn find_residual_closures(module: &IrModule) -> Vec<String> {
    let mut hits: Vec<String> = Vec::new();

    for l in &module.lets {
        if expr_has_closure(&l.value) {
            hits.push(format!("module-level let `{}`", l.name));
        }
    }
    for f in &module.functions {
        if let Some(body) = &f.body {
            if expr_has_closure(body) {
                hits.push(format!("function `{}` body", f.name));
            }
        }
        for p in &f.params {
            if let Some(default) = &p.default {
                if expr_has_closure(default) {
                    hits.push(format!(
                        "default of parameter `{}` on function `{}`",
                        p.name, f.name
                    ));
                }
            }
        }
    }
    for i in &module.impls {
        for f in &i.functions {
            if let Some(body) = &f.body {
                if expr_has_closure(body) {
                    hits.push(format!("impl method `{}` body", f.name));
                }
            }
            for p in &f.params {
                if let Some(default) = &p.default {
                    if expr_has_closure(default) {
                        hits.push(format!(
                            "default of parameter `{}` on impl method `{}`",
                            p.name, f.name
                        ));
                    }
                }
            }
        }
    }
    for s in &module.structs {
        for field in &s.fields {
            if let Some(default) = &field.default {
                if expr_has_closure(default) {
                    hits.push(format!(
                        "default of field `{}` on struct `{}`",
                        field.name, s.name
                    ));
                }
            }
        }
    }
    for e in &module.enums {
        for variant in &e.variants {
            for field in &variant.fields {
                if let Some(default) = &field.default {
                    if expr_has_closure(default) {
                        hits.push(format!(
                            "default of field `{}` on enum `{}::{}`",
                            field.name, e.name, variant.name
                        ));
                    }
                }
            }
        }
    }

    hits
}

/// Recursively scan an expression for any [`IrExpr::Closure`]
/// node. Returns `true` on the first match found.
fn expr_has_closure(expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Closure { .. } => true,
        IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::LetRef { .. } => false,
        IrExpr::Tuple { fields, .. } => fields.iter().any(|(_, v)| expr_has_closure(v)),
        IrExpr::StructInst { fields, .. } | IrExpr::EnumInst { fields, .. } => {
            fields.iter().any(|(_, _, v)| expr_has_closure(v))
        }
        IrExpr::Array { elements, .. } => elements.iter().any(expr_has_closure),
        IrExpr::FieldAccess { object, .. } => expr_has_closure(object),
        IrExpr::BinaryOp { left, right, .. } => expr_has_closure(left) || expr_has_closure(right),
        IrExpr::UnaryOp { operand, .. } => expr_has_closure(operand),
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            expr_has_closure(condition)
                || expr_has_closure(then_branch)
                || else_branch.as_ref().is_some_and(|e| expr_has_closure(e))
        }
        IrExpr::For {
            collection, body, ..
        } => expr_has_closure(collection) || expr_has_closure(body),
        IrExpr::Match {
            scrutinee, arms, ..
        } => expr_has_closure(scrutinee) || arms.iter().any(|arm| expr_has_closure(&arm.body)),
        IrExpr::FunctionCall { args, .. } => args.iter().any(|(_, v)| expr_has_closure(v)),
        IrExpr::MethodCall { receiver, args, .. } => {
            expr_has_closure(receiver) || args.iter().any(|(_, v)| expr_has_closure(v))
        }
        IrExpr::DictLiteral { entries, .. } => entries
            .iter()
            .any(|(k, v)| expr_has_closure(k) || expr_has_closure(v)),
        IrExpr::DictAccess { dict, key, .. } => expr_has_closure(dict) || expr_has_closure(key),
        IrExpr::Block {
            statements, result, ..
        } => {
            statements.iter().any(|stmt| match stmt {
                IrBlockStatement::Let { value, .. } => expr_has_closure(value),
                IrBlockStatement::Assign { target, value } => {
                    expr_has_closure(target) || expr_has_closure(value)
                }
                IrBlockStatement::Expr(e) => expr_has_closure(e),
            }) || expr_has_closure(result)
        }
        IrExpr::ClosureRef { env_struct, .. } => expr_has_closure(env_struct),
    }
}
