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
//! - rewrites captured-name reads inside each lifted body so they
//!   load the value from the env struct instead of the (no-longer-
//!   visible) outer binding (PR 2 mc5).
//!
//! Still TODO across later microcommits: site rewrite to
//! [`IrExpr::ClosureRef`] (mc6), callsite rewrite (mc7), and the
//! post-pass invariant assertion (mc8).
//!
//! [`IrExpr::Closure`]: crate::ir::IrExpr::Closure
//! [`IrExpr::ClosureRef`]: crate::ir::IrExpr::ClosureRef
//! [`IrFunction`]: crate::ir::IrFunction
//! [`IrStruct`]: crate::ir::IrStruct

use std::collections::HashSet;

use crate::ast::{ParamConvention, Visibility};
use crate::error::CompilerError;
use crate::ir::{
    walk_module, IrBlockStatement, IrExpr, IrField, IrFunction, IrFunctionParam, IrMatchArm,
    IrModule, IrStruct, IrVisitor, ResolvedType, StructId,
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
/// own parameters. The body is cloned, then captured-name reads
/// (`Reference` / `LetRef` matching one of the closure's captures)
/// are rewritten to load from the env via
/// [`rewrite_captured_refs`].
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

    let captured_names: HashSet<String> =
        closure.captures.iter().map(|(n, _, _)| n.clone()).collect();
    let env_ty = ResolvedType::Struct(env_struct_id);
    let mut bound: HashSet<String> = HashSet::new();
    let body = rewrite_captured_refs(closure.body.clone(), &captured_names, &env_ty, &mut bound);

    IrFunction {
        name,
        generic_params: Vec::new(),
        params,
        return_type: Some(closure.return_ty.clone()),
        body: Some(body),
        extern_abi: None,
        attributes: Vec::new(),
        doc: Some(
            "Auto-generated lifted closure body. Produced by `ClosureConversionPass`. The first parameter `__env` carries the closure's captures."
                .to_string(),
        ),
    }
}

/// Walk a lifted closure body and replace every read of a captured
/// name with an env field access. Tracks shadowing introduced by
/// `let` bindings, `match` arm bindings, and `for` loop variables —
/// references to a name inside a scope where it's been re-bound are
/// left alone.
///
/// Nested [`IrExpr::Closure`] expressions are *not* recursed into:
/// each closure's body has already been (or will be) collected into
/// its own [`CollectedClosure`] and rewritten with its own captures
/// when its lifted function is synthesized. The nested Closure's
/// body field is also dead-by-construction — mc6 replaces the
/// Closure node with a [`IrExpr::ClosureRef`] that drops it.
#[expect(
    clippy::too_many_lines,
    reason = "exhaustive match across every IrExpr variant; splitting hurts readability"
)]
fn rewrite_captured_refs(
    expr: IrExpr,
    captured_names: &HashSet<String>,
    env_ty: &ResolvedType,
    bound: &mut HashSet<String>,
) -> IrExpr {
    match expr {
        IrExpr::Reference { path, ty } => {
            if let Some(head) = path.first() {
                if path.len() == 1 && captured_names.contains(head) && !bound.contains(head) {
                    return env_field_access(head.clone(), ty, env_ty);
                }
            }
            IrExpr::Reference { path, ty }
        }
        IrExpr::LetRef { name, ty } => {
            if captured_names.contains(&name) && !bound.contains(&name) {
                return env_field_access(name, ty, env_ty);
            }
            IrExpr::LetRef { name, ty }
        }

        IrExpr::Closure {
            params,
            captures,
            body,
            ty,
        } => IrExpr::Closure {
            params,
            captures,
            body,
            ty,
        },

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
            fields: rewrite_named_fields(fields, captured_names, env_ty, bound),
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
            fields: rewrite_named_fields(fields, captured_names, env_ty, bound),
            ty,
        },
        IrExpr::Tuple { fields, ty } => IrExpr::Tuple {
            fields: rewrite_named_fields(fields, captured_names, env_ty, bound),
            ty,
        },
        IrExpr::Array { elements, ty } => IrExpr::Array {
            elements: elements
                .into_iter()
                .map(|e| rewrite_captured_refs(e, captured_names, env_ty, bound))
                .collect(),
            ty,
        },
        IrExpr::FieldAccess { object, field, ty } => IrExpr::FieldAccess {
            object: Box::new(rewrite_captured_refs(*object, captured_names, env_ty, bound)),
            field,
            ty,
        },
        IrExpr::BinaryOp {
            left,
            op,
            right,
            ty,
        } => IrExpr::BinaryOp {
            left: Box::new(rewrite_captured_refs(*left, captured_names, env_ty, bound)),
            op,
            right: Box::new(rewrite_captured_refs(*right, captured_names, env_ty, bound)),
            ty,
        },
        IrExpr::UnaryOp { op, operand, ty } => IrExpr::UnaryOp {
            op,
            operand: Box::new(rewrite_captured_refs(*operand, captured_names, env_ty, bound)),
            ty,
        },
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ty,
        } => IrExpr::If {
            condition: Box::new(rewrite_captured_refs(
                *condition,
                captured_names,
                env_ty,
                bound,
            )),
            then_branch: Box::new(rewrite_captured_refs(
                *then_branch,
                captured_names,
                env_ty,
                bound,
            )),
            else_branch: else_branch
                .map(|e| Box::new(rewrite_captured_refs(*e, captured_names, env_ty, bound))),
            ty,
        },
        IrExpr::For {
            var,
            var_ty,
            collection,
            body,
            ty,
        } => {
            let new_collection = rewrite_captured_refs(*collection, captured_names, env_ty, bound);
            let inserted = bound.insert(var.clone());
            let new_body = rewrite_captured_refs(*body, captured_names, env_ty, bound);
            if inserted {
                bound.remove(&var);
            }
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
            scrutinee: Box::new(rewrite_captured_refs(
                *scrutinee,
                captured_names,
                env_ty,
                bound,
            )),
            arms: arms
                .into_iter()
                .map(|arm| rewrite_match_arm(arm, captured_names, env_ty, bound))
                .collect(),
            ty,
        },
        IrExpr::FunctionCall { path, args, ty } => IrExpr::FunctionCall {
            path,
            args: args
                .into_iter()
                .map(|(label, value)| {
                    (
                        label,
                        rewrite_captured_refs(value, captured_names, env_ty, bound),
                    )
                })
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
            receiver: Box::new(rewrite_captured_refs(
                *receiver,
                captured_names,
                env_ty,
                bound,
            )),
            method,
            args: args
                .into_iter()
                .map(|(label, value)| {
                    (
                        label,
                        rewrite_captured_refs(value, captured_names, env_ty, bound),
                    )
                })
                .collect(),
            dispatch,
            ty,
        },
        IrExpr::DictLiteral { entries, ty } => IrExpr::DictLiteral {
            entries: entries
                .into_iter()
                .map(|(k, v)| {
                    (
                        rewrite_captured_refs(k, captured_names, env_ty, bound),
                        rewrite_captured_refs(v, captured_names, env_ty, bound),
                    )
                })
                .collect(),
            ty,
        },
        IrExpr::DictAccess { dict, key, ty } => IrExpr::DictAccess {
            dict: Box::new(rewrite_captured_refs(*dict, captured_names, env_ty, bound)),
            key: Box::new(rewrite_captured_refs(*key, captured_names, env_ty, bound)),
            ty,
        },
        IrExpr::Block {
            statements,
            result,
            ty,
        } => {
            let mut introduced: Vec<String> = Vec::new();
            let new_stmts = statements
                .into_iter()
                .map(|stmt| {
                    rewrite_block_stmt(stmt, captured_names, env_ty, bound, &mut introduced)
                })
                .collect();
            let new_result = rewrite_captured_refs(*result, captured_names, env_ty, bound);
            for name in introduced {
                bound.remove(&name);
            }
            IrExpr::Block {
                statements: new_stmts,
                result: Box::new(new_result),
                ty,
            }
        }

        IrExpr::ClosureRef {
            funcref,
            env_struct,
            ty,
        } => IrExpr::ClosureRef {
            funcref,
            env_struct: Box::new(rewrite_captured_refs(
                *env_struct,
                captured_names,
                env_ty,
                bound,
            )),
            ty,
        },
    }
}

fn rewrite_named_fields(
    fields: Vec<(String, IrExpr)>,
    captured_names: &HashSet<String>,
    env_ty: &ResolvedType,
    bound: &mut HashSet<String>,
) -> Vec<(String, IrExpr)> {
    fields
        .into_iter()
        .map(|(name, value)| {
            (
                name,
                rewrite_captured_refs(value, captured_names, env_ty, bound),
            )
        })
        .collect()
}

fn rewrite_match_arm(
    arm: IrMatchArm,
    captured_names: &HashSet<String>,
    env_ty: &ResolvedType,
    bound: &mut HashSet<String>,
) -> IrMatchArm {
    let mut introduced: Vec<String> = Vec::new();
    for (name, _) in &arm.bindings {
        if bound.insert(name.clone()) {
            introduced.push(name.clone());
        }
    }
    let body = rewrite_captured_refs(arm.body, captured_names, env_ty, bound);
    for name in introduced {
        bound.remove(&name);
    }
    IrMatchArm {
        variant: arm.variant,
        is_wildcard: arm.is_wildcard,
        bindings: arm.bindings,
        body,
    }
}

fn rewrite_block_stmt(
    stmt: IrBlockStatement,
    captured_names: &HashSet<String>,
    env_ty: &ResolvedType,
    bound: &mut HashSet<String>,
    introduced: &mut Vec<String>,
) -> IrBlockStatement {
    match stmt {
        IrBlockStatement::Let {
            name,
            mutable,
            ty,
            value,
        } => {
            let new_value = rewrite_captured_refs(value, captured_names, env_ty, bound);
            if bound.insert(name.clone()) {
                introduced.push(name.clone());
            }
            IrBlockStatement::Let {
                name,
                mutable,
                ty,
                value: new_value,
            }
        }
        IrBlockStatement::Assign { target, value } => IrBlockStatement::Assign {
            target: rewrite_captured_refs(target, captured_names, env_ty, bound),
            value: rewrite_captured_refs(value, captured_names, env_ty, bound),
        },
        IrBlockStatement::Expr(e) => {
            IrBlockStatement::Expr(rewrite_captured_refs(e, captured_names, env_ty, bound))
        }
    }
}

/// Build an `__env.<name>` field-access expression carrying the
/// captured value's resolved type. The `__env` reference itself is
/// typed as the env struct so backends can resolve its layout.
fn env_field_access(field: String, ty: ResolvedType, env_ty: &ResolvedType) -> IrExpr {
    IrExpr::FieldAccess {
        object: Box::new(IrExpr::Reference {
            path: vec![ENV_PARAM_NAME.to_string()],
            ty: env_ty.clone(),
        }),
        field,
        ty,
    }
}
