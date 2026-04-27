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
//! [`IrExpr::Closure`]: crate::ir::IrExpr::Closure
//! [`IrExpr::ClosureRef`]: crate::ir::IrExpr::ClosureRef
//! [`IrFunction`]: crate::ir::IrFunction
//! [`IrStruct`]: crate::ir::IrStruct

use std::collections::HashSet;

use crate::ast::{ParamConvention, Visibility};
use crate::error::CompilerError;
use crate::ir::{
    IrBlockStatement, IrExpr, IrField, IrFunction, IrFunctionParam, IrMatchArm, IrModule,
    IrStruct, ResolvedType, StructId,
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
        module.structs.extend(state.envs);
        module.functions.extend(state.lifted);

        module.rebuild_indices();
        Ok(module)
    }
}

/// Mutable state threaded through the recursive walk: the next
/// synthesized-name index, and the accumulated env structs / lifted
/// functions waiting to be appended to the module at the end.
struct ConversionState {
    /// Sequential index for the next `(__ClosureEnv<N>, __closure<N>)`
    /// allocation.
    next_idx: usize,
    /// Index `(== module.structs.len())` at which freshly synthesized
    /// env structs land. Used to assign correct `StructId`s without
    /// pushing them into the module mid-walk.
    base_struct_index: usize,
    /// Synthesized capture-environment structs, appended to the
    /// module after the walk.
    envs: Vec<IrStruct>,
    /// Synthesized lifted closure functions, appended to the module
    /// after the walk.
    lifted: Vec<IrFunction>,
}

impl ConversionState {
    fn new(module: &IrModule) -> Self {
        Self {
            next_idx: first_free_index(module),
            base_struct_index: module.structs.len(),
            envs: Vec::new(),
            lifted: Vec::new(),
        }
    }

    /// Allocate the next `(idx, env_name, func_name, env_struct_id)`
    /// quadruple. The returned `StructId` reflects where the env
    /// struct will land in `module.structs` after the walk's
    /// trailing `extend`.
    fn allocate(&mut self) -> (usize, String, String, StructId) {
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
    fn process_param_defaults(&mut self, params: Vec<IrFunctionParam>) -> Vec<IrFunctionParam> {
        params
            .into_iter()
            .map(|mut p| {
                p.default = p.default.map(|d| self.process(d, &CaptureCtx::module_level()));
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
    fn process(&mut self, expr: IrExpr, ctx: &CaptureCtx) -> IrExpr {
        match expr {
            IrExpr::Reference { path, ty } => {
                if path.len() == 1 {
                    if let Some(head) = path.first() {
                        if ctx.is_captured(head) {
                            return env_field_access(head.clone(), ty, ctx.env_ty.as_ref());
                        }
                    }
                }
                IrExpr::Reference { path, ty }
            }
            IrExpr::LetRef { name, ty } => {
                if ctx.is_captured(&name) {
                    return env_field_access(name, ty, ctx.env_ty.as_ref());
                }
                IrExpr::LetRef { name, ty }
            }

            IrExpr::Closure {
                params,
                captures,
                body,
                ty,
            } => self.lift_closure(&params, &captures, *body, ty, ctx),

            IrExpr::Literal { value, ty } => IrExpr::Literal { value, ty },
            IrExpr::SelfFieldRef { field, ty } => IrExpr::SelfFieldRef { field, ty },

            IrExpr::StructInst {
                struct_id,
                type_args,
                fields,
                ty,
            } => IrExpr::StructInst {
                struct_id,
                type_args,
                fields: self.process_named_fields(fields, ctx),
                ty,
            },
            IrExpr::EnumInst {
                enum_id,
                variant,
                fields,
                ty,
            } => IrExpr::EnumInst {
                enum_id,
                variant,
                fields: self.process_named_fields(fields, ctx),
                ty,
            },
            IrExpr::Tuple { fields, ty } => IrExpr::Tuple {
                fields: self.process_named_fields(fields, ctx),
                ty,
            },
            IrExpr::Array { elements, ty } => IrExpr::Array {
                elements: elements.into_iter().map(|e| self.process(e, ctx)).collect(),
                ty,
            },
            IrExpr::FieldAccess { object, field, ty } => IrExpr::FieldAccess {
                object: Box::new(self.process(*object, ctx)),
                field,
                ty,
            },
            IrExpr::BinaryOp {
                left,
                op,
                right,
                ty,
            } => IrExpr::BinaryOp {
                left: Box::new(self.process(*left, ctx)),
                op,
                right: Box::new(self.process(*right, ctx)),
                ty,
            },
            IrExpr::UnaryOp { op, operand, ty } => IrExpr::UnaryOp {
                op,
                operand: Box::new(self.process(*operand, ctx)),
                ty,
            },
            IrExpr::If {
                condition,
                then_branch,
                else_branch,
                ty,
            } => IrExpr::If {
                condition: Box::new(self.process(*condition, ctx)),
                then_branch: Box::new(self.process(*then_branch, ctx)),
                else_branch: else_branch.map(|e| Box::new(self.process(*e, ctx))),
                ty,
            },
            IrExpr::For {
                var,
                var_ty,
                collection,
                body,
                ty,
            } => {
                let new_collection = self.process(*collection, ctx);
                let mut inner_ctx = ctx.clone();
                inner_ctx.bound.insert(var.clone());
                let new_body = self.process(*body, &inner_ctx);
                IrExpr::For {
                    var,
                    var_ty,
                    collection: Box::new(new_collection),
                    body: Box::new(new_body),
                    ty,
                }
            }
            IrExpr::Match {
                scrutinee,
                arms,
                ty,
            } => IrExpr::Match {
                scrutinee: Box::new(self.process(*scrutinee, ctx)),
                arms: arms
                    .into_iter()
                    .map(|arm| self.process_match_arm(arm, ctx))
                    .collect(),
                ty,
            },
            IrExpr::FunctionCall { path, args, ty } => IrExpr::FunctionCall {
                path,
                args: args
                    .into_iter()
                    .map(|(label, value)| (label, self.process(value, ctx)))
                    .collect(),
                ty,
            },
            IrExpr::MethodCall {
                receiver,
                method,
                args,
                dispatch,
                ty,
            } => IrExpr::MethodCall {
                receiver: Box::new(self.process(*receiver, ctx)),
                method,
                args: args
                    .into_iter()
                    .map(|(label, value)| (label, self.process(value, ctx)))
                    .collect(),
                dispatch,
                ty,
            },
            IrExpr::DictLiteral { entries, ty } => IrExpr::DictLiteral {
                entries: entries
                    .into_iter()
                    .map(|(k, v)| (self.process(k, ctx), self.process(v, ctx)))
                    .collect(),
                ty,
            },
            IrExpr::DictAccess { dict, key, ty } => IrExpr::DictAccess {
                dict: Box::new(self.process(*dict, ctx)),
                key: Box::new(self.process(*key, ctx)),
                ty,
            },
            IrExpr::Block {
                statements,
                result,
                ty,
            } => self.process_block(statements, *result, ty, ctx),

            IrExpr::ClosureRef {
                funcref,
                env_struct,
                ty,
            } => IrExpr::ClosureRef {
                funcref,
                env_struct: Box::new(self.process(*env_struct, ctx)),
                ty,
            },
        }
    }

    fn process_named_fields(
        &mut self,
        fields: Vec<(String, IrExpr)>,
        ctx: &CaptureCtx,
    ) -> Vec<(String, IrExpr)> {
        fields
            .into_iter()
            .map(|(name, value)| (name, self.process(value, ctx)))
            .collect()
    }

    fn process_match_arm(&mut self, arm: IrMatchArm, ctx: &CaptureCtx) -> IrMatchArm {
        let mut inner_ctx = ctx.clone();
        for (name, _) in &arm.bindings {
            inner_ctx.bound.insert(name.clone());
        }
        IrMatchArm {
            variant: arm.variant,
            is_wildcard: arm.is_wildcard,
            bindings: arm.bindings,
            body: self.process(arm.body, &inner_ctx),
        }
    }

    fn process_block(
        &mut self,
        statements: Vec<IrBlockStatement>,
        result: IrExpr,
        ty: ResolvedType,
        ctx: &CaptureCtx,
    ) -> IrExpr {
        let mut inner_ctx = ctx.clone();
        let new_stmts = statements
            .into_iter()
            .map(|stmt| self.process_block_stmt(stmt, &mut inner_ctx))
            .collect();
        let new_result = self.process(result, &inner_ctx);
        IrExpr::Block {
            statements: new_stmts,
            result: Box::new(new_result),
            ty,
        }
    }

    fn process_block_stmt(
        &mut self,
        stmt: IrBlockStatement,
        ctx: &mut CaptureCtx,
    ) -> IrBlockStatement {
        match stmt {
            IrBlockStatement::Let {
                name,
                mutable,
                ty,
                value,
            } => {
                let new_value = self.process(value, ctx);
                ctx.bound.insert(name.clone());
                IrBlockStatement::Let {
                    name,
                    mutable,
                    ty,
                    value: new_value,
                }
            }
            IrBlockStatement::Assign { target, value } => IrBlockStatement::Assign {
                target: self.process(target, ctx),
                value: self.process(value, ctx),
            },
            IrBlockStatement::Expr(e) => IrBlockStatement::Expr(self.process(e, ctx)),
        }
    }

    /// Replace an [`IrExpr::Closure`] node with [`IrExpr::ClosureRef`],
    /// synthesizing the matching env struct and lifted function on
    /// the way.
    fn lift_closure(
        &mut self,
        params: &[(ParamConvention, String, ResolvedType)],
        captures: &[(String, ParamConvention, ResolvedType)],
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
        let lifted_fn = build_lifted_function(
            func_name.clone(),
            env_id,
            params,
            return_ty,
            lifted_body,
        );
        self.lifted.push(lifted_fn);

        // Build the env-struct constructor: each capture's value is
        // its name as visible in the OUTER scope, processed by the
        // outer context (so an outer-capture name resolves to
        // `__env.<name>`).
        let env_fields = captures
            .iter()
            .map(|(name, _convention, capture_ty)| {
                let raw_ref = IrExpr::Reference {
                    path: vec![name.clone()],
                    ty: capture_ty.clone(),
                };
                (name.clone(), self.process(raw_ref, outer_ctx))
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

/// Capture context threaded through the recursive walk. A node's
/// context determines whether a `Reference` / `LetRef` of a given
/// name should be rewritten to `__env.<name>` (for the *current*
/// enclosing closure's captures) or left as-is (parameter or local
/// binding).
#[derive(Clone, Default)]
struct CaptureCtx {
    /// Names captured by the immediately-enclosing closure (or empty
    /// at module level).
    captured_names: HashSet<String>,
    /// Resolved type of the current closure's env struct, used as the
    /// `ty` of the synthesized `Reference { path: [__env] }`. `None`
    /// at module level (no env in scope).
    env_ty: Option<ResolvedType>,
    /// Names introduced by `let` / `match` / `for` since the
    /// enclosing closure boundary. References to these shadow
    /// captures of the same name.
    bound: HashSet<String>,
}

impl CaptureCtx {
    fn module_level() -> Self {
        Self::default()
    }

    fn for_closure(
        captures: &[(String, ParamConvention, ResolvedType)],
        env_ty: ResolvedType,
    ) -> Self {
        Self {
            captured_names: captures.iter().map(|(n, _, _)| n.clone()).collect(),
            env_ty: Some(env_ty),
            bound: HashSet::new(),
        }
    }

    fn is_captured(&self, name: &str) -> bool {
        self.captured_names.contains(name) && !self.bound.contains(name)
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

/// Build the lifted top-level function for a closure: env param
/// first, then the closure's own params; body and return type are
/// supplied by the caller (the body has already been recursively
/// processed by the time this is called).
fn build_lifted_function(
    name: String,
    env_struct_id: StructId,
    closure_params: &[(ParamConvention, String, ResolvedType)],
    return_ty: ResolvedType,
    body: IrExpr,
) -> IrFunction {
    let env_param = IrFunctionParam {
        name: ENV_PARAM_NAME.to_string(),
        external_label: None,
        ty: Some(ResolvedType::Struct(env_struct_id)),
        default: None,
        convention: ParamConvention::Let,
    };

    let mut params = Vec::with_capacity(closure_params.len().saturating_add(1));
    params.push(env_param);
    for (convention, param_name, param_ty) in closure_params {
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

/// Build an `__env.<name>` field-access expression carrying the
/// captured value's resolved type. The `__env` reference itself is
/// typed as the env struct so backends can resolve its layout.
///
/// `env_ty` is `None` only at module level — and at module level
/// nothing is "captured", so this helper is never reached with
/// `env_ty == None` in practice. The fallback to
/// [`ResolvedType::Error`] keeps the function total without
/// panicking.
fn env_field_access(field: String, ty: ResolvedType, env_ty: Option<&ResolvedType>) -> IrExpr {
    IrExpr::FieldAccess {
        object: Box::new(IrExpr::Reference {
            path: vec![ENV_PARAM_NAME.to_string()],
            ty: env_ty.cloned().unwrap_or(ResolvedType::Error),
        }),
        field,
        ty,
    }
}
