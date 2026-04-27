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
//! # Status
//!
//! In progress. The pass currently:
//! - synthesizes one capture-environment [`IrStruct`] per
//!   [`IrExpr::Closure`] (PR 2 mc3).
//! - synthesizes one top-level lifted [`IrFunction`] per closure,
//!   prepended with an `__env: __ClosureEnv<N>` parameter (PR 2 mc4).
//!
//! Still TODO across later microcommits: body capture rewrite (mc5),
//! site rewrite to [`IrExpr::ClosureRef`] (mc6), callsite rewrite
//! (mc7), and the post-pass invariant assertion (mc8).
//!
//! [`IrExpr::Closure`]: crate::ir::IrExpr::Closure
//! [`IrExpr::ClosureRef`]: crate::ir::IrExpr::ClosureRef
//! [`IrFunction`]: crate::ir::IrFunction
//! [`IrStruct`]: crate::ir::IrStruct

use crate::ast::{ParamConvention, Visibility};
use crate::error::CompilerError;
use crate::ir::{
    walk_module, IrExpr, IrField, IrFunction, IrFunctionParam, IrModule, IrStruct, IrVisitor,
    ResolvedType, StructId,
};
use crate::pipeline::IrPass;

/// Name prefix for synthesized capture-environment structs.
const ENV_STRUCT_PREFIX: &str = "__ClosureEnv";

/// Name prefix for synthesized lifted closure functions.
const LIFTED_FN_PREFIX: &str = "__closure";

/// Parameter name used for the env-struct argument prepended to every
/// lifted closure function.
const ENV_PARAM_NAME: &str = "__env";

/// Closure-conversion pass.
///
/// See the module-level documentation in `src/ir/closure_conv.rs` for
/// the full algorithm and pipeline placement.
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
        // Walk the module in deterministic order, collecting each
        // closure's full data so we can synthesize an env struct +
        // lifted function pair for every site. Subsequent microcommits
        // (mc5+) re-use this same walk order to rewrite bodies and
        // the closure expressions themselves.
        let mut collector = ClosureCollector::default();
        walk_module(&mut collector, &module);

        // Find a starting index past any pre-existing struct or
        // function whose name already matches our generated patterns,
        // so we can issue sequential names without per-step collision
        // checks.
        let mut next_idx = first_free_index(&module);

        for closure in collector.closures {
            let idx = next_idx;
            next_idx = next_idx.saturating_add(1);
            let env_name = format!("{ENV_STRUCT_PREFIX}{idx}");
            let func_name = format!("{LIFTED_FN_PREFIX}{idx}");

            let env_struct = synthesize_env_struct(env_name.clone(), &closure.captures);

            let env_struct_idx = module.structs.len();
            #[expect(
                clippy::cast_possible_truncation,
                reason = "module.structs.len() is bounded by add_struct's u32 check"
            )]
            let env_struct_id = StructId(env_struct_idx as u32);
            module.structs.push(env_struct);

            let lifted = synthesize_lifted_function(func_name, env_struct_id, &closure);
            module.functions.push(lifted);
        }

        module.rebuild_indices();
        Ok(module)
    }
}

/// One closure's full lifting data, captured during the walk.
struct CollectedClosure {
    params: Vec<(ParamConvention, String, ResolvedType)>,
    captures: Vec<(String, ParamConvention, ResolvedType)>,
    body: IrExpr,
    return_ty: ResolvedType,
}

/// Visitor that records every closure's lifting data in module-walk
/// order.
#[derive(Default)]
struct ClosureCollector {
    closures: Vec<CollectedClosure>,
}

impl IrVisitor for ClosureCollector {
    fn visit_expr(&mut self, e: &IrExpr) {
        if let IrExpr::Closure {
            params,
            captures,
            body,
            ty,
        } = e
        {
            // Lowering attaches `Closure { .. }` to every
            // `IrExpr::Closure`. If anything else slips through (e.g.
            // an `Error` placeholder propagated from an earlier
            // failure), fall back to `Error` so we still produce a
            // well-formed function shape — the surrounding
            // CompilerError already describes the problem.
            let return_ty = if let ResolvedType::Closure { return_ty, .. } = ty {
                (**return_ty).clone()
            } else {
                ResolvedType::Error
            };
            self.closures.push(CollectedClosure {
                params: params.clone(),
                captures: captures.clone(),
                body: (**body).clone(),
                return_ty,
            });
        }
        crate::ir::walk_expr_children(self, e);
    }
}

/// Scan the module for any pre-existing struct/function whose name
/// already follows the generated prefix + integer pattern. Return one
/// past the highest matching index (or `0` if no matches), which is
/// safe to use as a sequential allocation start.
fn first_free_index(module: &IrModule) -> usize {
    let max_struct = module
        .structs
        .iter()
        .filter_map(|s| parse_suffix_index(&s.name, ENV_STRUCT_PREFIX))
        .max();
    let max_func = module
        .functions
        .iter()
        .filter_map(|f| parse_suffix_index(&f.name, LIFTED_FN_PREFIX))
        .max();
    match (max_struct, max_func) {
        (Some(s), Some(f)) => s.max(f).saturating_add(1),
        (Some(s), None) => s.saturating_add(1),
        (None, Some(f)) => f.saturating_add(1),
        (None, None) => 0,
    }
}

/// If `name` is exactly `<prefix><N>` for some non-negative integer
/// `N`, return `N`.
fn parse_suffix_index(name: &str, prefix: &str) -> Option<usize> {
    name.strip_prefix(prefix).and_then(|tail| tail.parse().ok())
}

/// Build the capture-environment struct for a closure with the given
/// captures. Each capture becomes a private field carrying the
/// captured value's type. The capture's [`ParamConvention`] is not
/// materialised on the field today; later microcommits (mc9) will
/// revisit how `Mut` captures preserve borrow semantics through the
/// env.
fn synthesize_env_struct(
    name: String,
    captures: &[(String, ParamConvention, ResolvedType)],
) -> IrStruct {
    let fields = captures
        .iter()
        .map(|(field_name, _convention, ty)| IrField {
            name: field_name.clone(),
            ty: ty.clone(),
            mutable: false,
            optional: false,
            default: None,
            doc: None,
        })
        .collect();

    IrStruct {
        name,
        visibility: Visibility::Private,
        traits: Vec::new(),
        fields,
        generic_params: Vec::new(),
        doc: Some(
            "Auto-generated capture environment for a lifted closure. Produced by `ClosureConversionPass`."
                .to_string(),
        ),
    }
}

/// Build the lifted top-level function for a closure. The first
/// parameter is the env struct (immutable convention — the closure
/// reads its captures); the remaining parameters mirror the closure's
/// own parameters. The body is cloned verbatim in this microcommit;
/// mc5 will rewrite captured-name reads to load from `__env` fields.
fn synthesize_lifted_function(
    name: String,
    env_struct_id: StructId,
    closure: &CollectedClosure,
) -> IrFunction {
    let env_param = IrFunctionParam {
        name: ENV_PARAM_NAME.to_string(),
        external_label: None,
        ty: Some(ResolvedType::Struct(env_struct_id)),
        default: None,
        convention: ParamConvention::Let,
    };

    let mut params = Vec::with_capacity(closure.params.len().saturating_add(1));
    params.push(env_param);
    for (convention, param_name, param_ty) in &closure.params {
        params.push(IrFunctionParam {
            name: param_name.clone(),
            external_label: None,
            ty: Some(param_ty.clone()),
            default: None,
            convention: *convention,
        });
    }

    IrFunction {
        name,
        generic_params: Vec::new(),
        params,
        return_type: Some(closure.return_ty.clone()),
        body: Some(closure.body.clone()),
        extern_abi: None,
        attributes: Vec::new(),
        doc: Some(
            "Auto-generated lifted closure body. Produced by `ClosureConversionPass`. The first parameter `__env` carries the closure's captures."
                .to_string(),
        ),
    }
}
