//! Conversion state and the recursive expression-rewrite walk.
//!
//! [`ConversionState`] threads the synthesized-name counter and the
//! growing collections of env structs / lifted functions through a
//! depth-first walk over an [`IrExpr`]. The walk both rewrites
//! captured-name reads (`Reference` / `LetRef`) into env field
//! accesses and replaces every [`IrExpr::Closure`] with an
//! [`IrExpr::ClosureRef`] (delegated to [`super::lifting`]).

use crate::ir::{
    IrBlockStatement, IrExpr, IrFunction, IrFunctionParam, IrMatchArm, IrModule, IrStruct,
    ResolvedType, StructId,
};

use super::capture_rewrite::{env_field_access, CaptureCtx};
use super::lifting::first_free_index;
use super::{ENV_STRUCT_PREFIX, LIFTED_FN_PREFIX};

/// Mutable state threaded through the recursive walk: the next
/// synthesized-name index, and the accumulated env structs / lifted
/// functions waiting to be appended to the module at the end.
pub(super) struct ConversionState {
    /// Sequential index for the next `(__ClosureEnv<N>, __closure<N>)`
    /// allocation.
    next_idx: usize,
    /// Index `(== module.structs.len())` at which freshly synthesized
    /// env structs land. Used to assign correct `StructId`s without
    /// pushing them into the module mid-walk.
    base_struct_index: usize,
    /// Synthesized capture-environment structs, appended to the
    /// module after the walk.
    pub(super) envs: Vec<IrStruct>,
    /// Synthesized lifted closure functions, appended to the module
    /// after the walk.
    pub(super) lifted: Vec<IrFunction>,
}

impl ConversionState {
    pub(super) fn new(module: &IrModule) -> Self {
        Self {
            next_idx: first_free_index(module, ENV_STRUCT_PREFIX, LIFTED_FN_PREFIX),
            base_struct_index: module.structs.len(),
            envs: Vec::new(),
            lifted: Vec::new(),
        }
    }

    /// Consume the state and return the accumulated env structs and
    /// lifted functions so the orchestrator can append them to the
    /// module.
    pub(super) fn into_outputs(self) -> (Vec<IrStruct>, Vec<IrFunction>) {
        (self.envs, self.lifted)
    }

    /// Allocate the next `(idx, env_name, func_name, env_struct_id)`
    /// quadruple. The returned `StructId` reflects where the env
    /// struct will land in `module.structs` after the walk's
    /// trailing `extend`.
    pub(super) fn allocate(&mut self) -> (usize, String, String, StructId) {
        let idx = self.next_idx;
        self.next_idx = self.next_idx.saturating_add(1);
        let env_name = format!("{ENV_STRUCT_PREFIX}{idx}");
        let func_name = format!("{LIFTED_FN_PREFIX}{idx}");
        #[expect(
            clippy::cast_possible_truncation,
            reason = "module struct count bounded by add_struct's u32 check upstream; \
                      env-struct overflow would require billions of closures"
        )]
        let env_id = StructId(self.base_struct_index.saturating_add(self.envs.len()) as u32);
        (idx, env_name, func_name, env_id)
    }

    /// Process function-parameter default expressions, threading the
    /// module-level capture context (no captures, no env).
    pub(super) fn process_param_defaults(
        &mut self,
        params: Vec<IrFunctionParam>,
    ) -> Vec<IrFunctionParam> {
        params
            .into_iter()
            .map(|mut p| {
                p.default = p
                    .default
                    .map(|d| self.process(d, &CaptureCtx::module_level()));
                p
            })
            .collect()
    }

    /// Recursively process an expression: rewrite captured-name reads
    /// to env field access (mc5) AND replace `IrExpr::Closure` with
    /// `IrExpr::ClosureRef` plus synthesized env / lifted function
    /// (mc6). Tracks shadowing introduced by `let`, `match` arm
    /// bindings, and `for` loop variables.
    #[expect(
        clippy::too_many_lines,
        reason = "exhaustive match across every IrExpr variant; splitting hurts readability"
    )]
    pub(super) fn process(&mut self, expr: IrExpr, ctx: &CaptureCtx) -> IrExpr {
        match expr {
            IrExpr::Reference {
                path,
                target,
                ty,
                span,
            } => {
                if let crate::ir::ReferenceTarget::Local(id)
                | crate::ir::ReferenceTarget::Param(id) = target
                {
                    if ctx.is_captured(id) {
                        // The capture's introducing name was recorded in
                        // `ctx.capture_names`; prefer it over `path[0]`
                        // because a shadowed-then-resolved path could
                        // disagree with the canonical capture name.
                        let name = ctx.capture_name(id).map_or_else(
                            || path.first().cloned().unwrap_or_default(),
                            str::to_string,
                        );
                        return env_field_access(name, ty, ctx.env_ty(), span);
                    }
                }
                IrExpr::Reference {
                    path,
                    target,
                    ty,
                    span,
                }
            }
            IrExpr::LetRef {
                name,
                binding_id,
                ty,
                span,
            } => {
                if ctx.is_captured(binding_id) {
                    return env_field_access(name, ty, ctx.env_ty(), span);
                }
                IrExpr::LetRef {
                    name,
                    binding_id,
                    ty,

                    span,
                }
            }

            IrExpr::Closure {
                params,
                captures,
                body,
                ty,
                span,
            } => self.lift_closure(&params, &captures, *body, ty, span, ctx),

            IrExpr::Literal { value, ty, span } => IrExpr::Literal { value, ty, span },
            IrExpr::SelfFieldRef {
                field,
                field_idx,
                ty,
                span,
            } => IrExpr::SelfFieldRef {
                field,
                field_idx,
                ty,

                span,
            },

            IrExpr::StructInst {
                struct_id,
                type_args,
                fields,
                ty,
                span,
            } => IrExpr::StructInst {
                struct_id,
                type_args,
                fields: self.process_indexed_fields(fields, ctx),
                ty,

                span,
            },
            IrExpr::EnumInst {
                enum_id,
                variant,
                variant_idx,
                fields,
                ty,
                span,
            } => IrExpr::EnumInst {
                enum_id,
                variant,
                variant_idx,
                fields: self.process_indexed_fields(fields, ctx),
                ty,

                span,
            },
            IrExpr::Tuple { fields, ty, span } => IrExpr::Tuple {
                fields: self.process_named_fields(fields, ctx),
                ty,

                span,
            },
            IrExpr::Array { elements, ty, span } => IrExpr::Array {
                elements: elements.into_iter().map(|e| self.process(e, ctx)).collect(),
                ty,

                span,
            },
            IrExpr::FieldAccess {
                object,
                field,
                field_idx,
                ty,
                span,
            } => IrExpr::FieldAccess {
                object: Box::new(self.process(*object, ctx)),
                field,
                field_idx,
                ty,

                span,
            },
            IrExpr::BinaryOp {
                left,
                op,
                right,
                ty,
                span,
            } => IrExpr::BinaryOp {
                left: Box::new(self.process(*left, ctx)),
                op,
                right: Box::new(self.process(*right, ctx)),
                ty,

                span,
            },
            IrExpr::UnaryOp {
                op,
                operand,
                ty,
                span,
            } => IrExpr::UnaryOp {
                op,
                operand: Box::new(self.process(*operand, ctx)),
                ty,

                span,
            },
            IrExpr::If {
                condition,
                then_branch,
                else_branch,
                ty,
                span,
            } => IrExpr::If {
                condition: Box::new(self.process(*condition, ctx)),
                then_branch: Box::new(self.process(*then_branch, ctx)),
                else_branch: else_branch.map(|e| Box::new(self.process(*e, ctx))),
                ty,

                span,
            },
            IrExpr::For {
                var,
                var_ty,
                var_binding_id,
                collection,
                body,
                ty,
                span,
            } => {
                // No `inner_ctx.bind(var)` needed: BindingId-based
                // detection inherently distinguishes the for-loop
                // variable's id from any outer capture id.
                let new_collection = self.process(*collection, ctx);
                let new_body = self.process(*body, ctx);
                IrExpr::For {
                    var,
                    var_ty,
                    var_binding_id,
                    collection: Box::new(new_collection),
                    body: Box::new(new_body),
                    ty,

                    span,
                }
            }
            IrExpr::Match {
                scrutinee,
                arms,
                ty,
                span,
            } => IrExpr::Match {
                scrutinee: Box::new(self.process(*scrutinee, ctx)),
                arms: arms
                    .into_iter()
                    .map(|arm| self.process_match_arm(arm, ctx))
                    .collect(),
                ty,

                span,
            },
            IrExpr::FunctionCall {
                path,
                function_id,
                args,
                ty,
                span,
            } => IrExpr::FunctionCall {
                path,
                function_id,
                args: args
                    .into_iter()
                    .map(|(label, value)| (label, self.process(value, ctx)))
                    .collect(),
                ty,

                span,
            },
            IrExpr::CallClosure {
                closure,
                args,
                ty,
                span,
            } => IrExpr::CallClosure {
                closure: Box::new(self.process(*closure, ctx)),
                args: args
                    .into_iter()
                    .map(|(label, value)| (label, self.process(value, ctx)))
                    .collect(),
                ty,

                span,
            },
            IrExpr::MethodCall {
                receiver,
                method,
                method_idx,
                args,
                dispatch,
                ty,
                span,
            } => IrExpr::MethodCall {
                receiver: Box::new(self.process(*receiver, ctx)),
                method,
                method_idx,
                args: args
                    .into_iter()
                    .map(|(label, value)| (label, self.process(value, ctx)))
                    .collect(),
                dispatch,
                ty,

                span,
            },
            IrExpr::DictLiteral { entries, ty, span } => IrExpr::DictLiteral {
                entries: entries
                    .into_iter()
                    .map(|(k, v)| (self.process(k, ctx), self.process(v, ctx)))
                    .collect(),
                ty,

                span,
            },
            IrExpr::DictAccess {
                dict,
                key,
                ty,
                span,
            } => IrExpr::DictAccess {
                dict: Box::new(self.process(*dict, ctx)),
                key: Box::new(self.process(*key, ctx)),
                ty,

                span,
            },
            IrExpr::Block {
                statements,
                result,
                ty,
                span,
            } => self.process_block(statements, *result, ty, span, ctx),

            IrExpr::ClosureRef {
                funcref,
                env_struct,
                ty,
                span,
            } => IrExpr::ClosureRef {
                funcref,
                env_struct: Box::new(self.process(*env_struct, ctx)),
                ty,

                span,
            },
        }
    }

    pub(super) fn process_named_fields(
        &mut self,
        fields: Vec<(String, IrExpr)>,
        ctx: &CaptureCtx,
    ) -> Vec<(String, IrExpr)> {
        fields
            .into_iter()
            .map(|(name, value)| (name, self.process(value, ctx)))
            .collect()
    }

    pub(super) fn process_indexed_fields(
        &mut self,
        fields: Vec<(String, crate::ir::FieldIdx, IrExpr)>,
        ctx: &CaptureCtx,
    ) -> Vec<(String, crate::ir::FieldIdx, IrExpr)> {
        fields
            .into_iter()
            .map(|(name, idx, value)| (name, idx, self.process(value, ctx)))
            .collect()
    }

    fn process_match_arm(&mut self, arm: IrMatchArm, ctx: &CaptureCtx) -> IrMatchArm {
        // BindingId-based detection: the match-arm bindings have
        // their own fresh ids assigned by ResolveReferencesPass, so
        // a reference to one resolves to its inner id and is *not*
        // in the captured set. No shadow-tracking needed.
        IrMatchArm {
            variant: arm.variant,
            variant_idx: arm.variant_idx,
            is_wildcard: arm.is_wildcard,
            bindings: arm.bindings,
            body: self.process(arm.body, ctx),
        }
    }

    fn process_block(
        &mut self,
        statements: Vec<IrBlockStatement>,
        result: IrExpr,
        ty: ResolvedType,
        span: crate::ir::IrSpan,
        ctx: &CaptureCtx,
    ) -> IrExpr {
        let new_stmts = statements
            .into_iter()
            .map(|stmt| self.process_block_stmt(stmt, ctx))
            .collect();
        let new_result = self.process(result, ctx);
        IrExpr::Block {
            statements: new_stmts,
            result: Box::new(new_result),
            ty,

            span,
        }
    }

    fn process_block_stmt(&mut self, stmt: IrBlockStatement, ctx: &CaptureCtx) -> IrBlockStatement {
        match stmt {
            // BindingId-based detection inherently handles shadowing —
            // the let's `binding_id` differs from any outer capture's
            // id, so references to it after the let resolve to the
            // inner id and stay un-rewritten.
            IrBlockStatement::Let {
                binding_id,
                name,
                mutable,
                ty,
                value,
                span,
            } => {
                let new_value = self.process(value, ctx);
                IrBlockStatement::Let {
                    binding_id,
                    name,
                    mutable,
                    ty,
                    value: new_value,

                    span,
                }
            }
            IrBlockStatement::Assign {
                target,
                value,
                span,
            } => IrBlockStatement::Assign {
                target: self.process(target, ctx),
                value: self.process(value, ctx),

                span,
            },
            IrBlockStatement::Expr(e) => IrBlockStatement::Expr(self.process(e, ctx)),
        }
    }
}
