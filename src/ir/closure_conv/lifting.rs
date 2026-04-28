//! Closure lifting: replacing an [`IrExpr::Closure`] with an
//! [`IrExpr::ClosureRef`] paired with a synthesized top-level
//! [`IrFunction`] and the matching capture-environment struct.
//!
//! Also provides the index-allocation helpers used by
//! [`super::state::ConversionState`] to pick fresh
//! `(__ClosureEnv<N>, __closure<N>)` pairs.

use crate::ast::ParamConvention;
use crate::ir::{IrExpr, IrFunction, IrFunctionParam, IrModule, ResolvedType, StructId};

use super::capture_rewrite::CaptureCtx;
use super::env_synthesis::synthesize_env_struct;
use super::state::ConversionState;
use super::ENV_PARAM_NAME;

impl ConversionState {
    /// Replace an [`IrExpr::Closure`] node with [`IrExpr::ClosureRef`],
    /// synthesizing the matching env struct and lifted function on
    /// the way.
    pub(super) fn lift_closure(
        &mut self,
        params: &[(ParamConvention, crate::ir::BindingId, String, ResolvedType)],
        captures: &[(crate::ir::BindingId, String, ParamConvention, ResolvedType)],
        body: IrExpr,
        closure_ty: ResolvedType,
        outer_ctx: &CaptureCtx,
    ) -> IrExpr {
        let (_idx, env_name, func_name, env_id) = self.allocate();

        // Synthesize the env struct.
        let env_struct = synthesize_env_struct(env_name, captures);
        self.envs.push(env_struct);

        // Recursively process the closure body with a fresh inner
        // context driven by THIS closure's captures.
        let inner_ctx = CaptureCtx::for_closure(captures, ResolvedType::Struct(env_id));
        let lifted_body = self.process(body, &inner_ctx);

        // Extract return type from the closure's resolved type.
        let return_ty = if let ResolvedType::Closure { return_ty, .. } = &closure_ty {
            (**return_ty).clone()
        } else {
            ResolvedType::Error
        };

        // Synthesize the lifted top-level function.
        let lifted_fn =
            build_lifted_function(func_name.clone(), env_id, params, return_ty, lifted_body);
        self.lifted.push(lifted_fn);

        // Build the env-struct constructor: each capture's value is
        // its name as visible in the OUTER scope, processed by the
        // outer context (so an outer-capture name resolves to
        // `__env.<name>`).
        let env_fields = captures
            .iter()
            .enumerate()
            .map(|(i, (_bid, name, _convention, capture_ty))| {
                let raw_ref = IrExpr::Reference {
                    path: vec![name.clone()],
                    target: crate::ir::ReferenceTarget::Unresolved,
                    ty: capture_ty.clone(),
                };
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "capture count is bounded by the upstream u32 ceiling on field count"
                )]
                let idx = crate::ir::FieldIdx(i as u32);
                (name.clone(), idx, self.process(raw_ref, outer_ctx))
            })
            .collect();

        let env_inst = IrExpr::StructInst {
            struct_id: Some(env_id),
            type_args: Vec::new(),
            fields: env_fields,
            ty: ResolvedType::Struct(env_id),
        };

        IrExpr::ClosureRef {
            funcref: vec![func_name],
            env_struct: Box::new(env_inst),
            ty: closure_ty,
        }
    }
}

/// Build the lifted top-level function for a closure: env param
/// first, then the closure's own params; body and return type are
/// supplied by the caller (the body has already been recursively
/// processed by the time this is called).
fn build_lifted_function(
    name: String,
    env_struct_id: StructId,
    closure_params: &[(ParamConvention, crate::ir::BindingId, String, ResolvedType)],
    return_ty: ResolvedType,
    body: IrExpr,
) -> IrFunction {
    let env_param = IrFunctionParam {
        binding_id: crate::ir::BindingId(0),
        name: ENV_PARAM_NAME.to_string(),
        external_label: None,
        ty: Some(ResolvedType::Struct(env_struct_id)),
        default: None,
        convention: ParamConvention::Let,
    };

    let mut params = Vec::with_capacity(closure_params.len().saturating_add(1));
    params.push(env_param);
    for (convention, _bid, param_name, param_ty) in closure_params {
        params.push(IrFunctionParam {
            binding_id: crate::ir::BindingId(0),
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
        return_type: Some(return_ty),
        body: Some(body),
        extern_abi: None,
        attributes: Vec::new(),
        doc: Some(
            "Auto-generated lifted closure body. Produced by `ClosureConversionPass`. The first parameter `__env` carries the closure's captures."
                .to_string(),
        ),
    }
}

/// Scan the module for any pre-existing struct/function whose name
/// already follows the generated prefix + integer pattern. Return one
/// past the highest matching index (or `0` if no matches), which is
/// safe to use as a sequential allocation start.
pub(super) fn first_free_index(
    module: &IrModule,
    env_struct_prefix: &str,
    lifted_fn_prefix: &str,
) -> usize {
    let max_struct = module
        .structs
        .iter()
        .filter_map(|s| parse_suffix_index(&s.name, env_struct_prefix))
        .max();
    let max_func = module
        .functions
        .iter()
        .filter_map(|f| parse_suffix_index(&f.name, lifted_fn_prefix))
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
