//! Type-substitution and field/method/function return-type lookups shared
//! across the rest of the expression-lowering submodules.

use crate::ast::PrimitiveType;
use crate::error::CompilerError;
use crate::ir::lower::IrLowerer;
use crate::ir::{IrExpr, ResolvedType};
use std::collections::HashMap;

/// Substitute `TypeParam(name)` references inside `ty` using `subs`.
/// Used by `resolve_method_return_type` when the receiver is a
/// `Generic { base, args }` so the impl method's return type
/// (declared in terms of the struct's generic params) gets the
/// concrete instantiation's type arguments.
fn substitute_typeparam_in_resolved(ty: &mut ResolvedType, subs: &HashMap<String, ResolvedType>) {
    match ty {
        ResolvedType::TypeParam(name) => {
            if let Some(concrete) = subs.get(name) {
                *ty = concrete.clone();
            }
        }
        ResolvedType::Array(inner) | ResolvedType::Range(inner) | ResolvedType::Optional(inner) => {
            substitute_typeparam_in_resolved(inner, subs);
        }
        ResolvedType::Tuple(fields) => {
            for (_, t) in fields {
                substitute_typeparam_in_resolved(t, subs);
            }
        }
        ResolvedType::Dictionary { key_ty, value_ty } => {
            substitute_typeparam_in_resolved(key_ty, subs);
            substitute_typeparam_in_resolved(value_ty, subs);
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            for (_, t) in param_tys {
                substitute_typeparam_in_resolved(t, subs);
            }
            substitute_typeparam_in_resolved(return_ty, subs);
        }
        ResolvedType::Generic { args, .. } => {
            for a in args {
                substitute_typeparam_in_resolved(a, subs);
            }
        }
        ResolvedType::External { type_args, .. } => {
            for a in type_args {
                substitute_typeparam_in_resolved(a, subs);
            }
        }
        ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_)
        | ResolvedType::Error => {}
    }
}

impl IrLowerer<'_> {
    /// Resolve the type of a field access on an expression.
    ///
    /// Handles struct field access by looking up the field in the struct
    /// definition. Anything the semantic layer should have caught that
    /// still reaches here (missing field, field access on a non-struct
    /// type) records an `InternalError` so compilation fails loudly.
    pub(super) fn resolve_field_type(
        &mut self,
        object_ty: &ResolvedType,
        field_name: &str,
    ) -> ResolvedType {
        match object_ty {
            ResolvedType::Struct(struct_id) => {
                if let Some(struct_def) = self.module.get_struct(*struct_id) {
                    for field in &struct_def.fields {
                        if field.name == field_name {
                            return field.ty.clone();
                        }
                    }
                    self.errors.push(CompilerError::InternalError {
                        detail: format!(
                            "IR lowering: struct `{}` has no field `{field_name}`",
                            struct_def.name
                        ),
                        span: self.current_span,
                    });
                } else {
                    self.errors.push(CompilerError::InternalError {
                        detail: format!(
                            "IR lowering: struct id {} out of bounds during field access `{field_name}`",
                            struct_id.0
                        ),
                        span: self.current_span,
                    });
                }
                ResolvedType::Primitive(PrimitiveType::Never)
            }
            ResolvedType::Primitive(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Enum(_)
            | ResolvedType::Array(_)
            | ResolvedType::Range(_)
            | ResolvedType::Optional(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::Generic { .. }
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::Dictionary { .. }
            | ResolvedType::Closure { .. } => {
                self.errors.push(CompilerError::InternalError {
                    detail: format!(
                        "IR lowering: cannot access field `{field_name}` on non-struct receiver {object_ty:?}"
                    ),
                    span: self.current_span,
                });
                ResolvedType::Primitive(PrimitiveType::Never)
            }
            // Receiver was already an upstream error; the original
            // `CompilerError` has been recorded — propagate without cascading.
            ResolvedType::Error => ResolvedType::Error,
        }
    }

    /// Resolve the return type of a method call.
    ///
    /// Looks up user-defined methods in impl blocks. Records an
    /// `InternalError` when the method cannot be resolved on a concrete
    /// receiver — those cases should have been caught by semantic
    /// analysis and reaching here indicates a compiler bug.
    #[expect(
        clippy::too_many_lines,
        reason = "exhaustive resolution: pre-installed methods, struct/enum/Generic/TypeParam/Trait dispatch arms"
    )]
    pub(super) fn resolve_method_return_type(
        &mut self,
        receiver_ty: &ResolvedType,
        method_name: &str,
    ) -> ResolvedType {
        // If we are mid-lowering an impl block, its method set is recorded
        // in `current_impl_method_returns`. Forward references like
        // `self.other()` resolve against that map before the impl is
        // installed into `module.impls`.
        if let Some(returns) = &self.current_impl_method_returns {
            if let Some(entry) = returns.get(method_name) {
                return entry
                    .clone()
                    .unwrap_or(ResolvedType::Primitive(PrimitiveType::Never));
            }
        }

        if let ResolvedType::Struct(struct_id) = receiver_ty {
            for impl_block in &self.module.impls {
                if impl_block.struct_id() == Some(*struct_id) {
                    for func in &impl_block.functions {
                        if func.name == method_name {
                            return func
                                .return_type
                                .clone()
                                .or_else(|| func.body.as_ref().map(|b| b.ty().clone()))
                                .unwrap_or(ResolvedType::Primitive(PrimitiveType::Never));
                        }
                    }
                }
            }
            self.errors.push(CompilerError::InternalError {
                detail: format!(
                    "IR lowering: no impl method `{method_name}` for struct id {}",
                    struct_id.0
                ),
                span: self.current_span,
            });
            return ResolvedType::Primitive(PrimitiveType::Never);
        }

        // Generic receiver (`Box<I32>`): look up the impl on the
        // generic base, then substitute the impl method's TypeParams
        // with the concrete type arguments.
        if let ResolvedType::Generic { base, args } = receiver_ty {
            let (target_struct_id, target_enum_id) = match base {
                crate::ir::GenericBase::Struct(id) => (Some(*id), None),
                crate::ir::GenericBase::Enum(id) => (None, Some(*id)),
                // A trait base wouldn't appear here as a method-call
                // receiver post item E2. Skip and fall through.
                crate::ir::GenericBase::Trait(_) => (None, None),
            };
            let generic_params: Vec<String> = if let Some(sid) = target_struct_id {
                self.module
                    .get_struct(sid)
                    .map(|s| s.generic_params.iter().map(|p| p.name.clone()).collect())
                    .unwrap_or_default()
            } else if let Some(eid) = target_enum_id {
                self.module
                    .get_enum(eid)
                    .map(|e| e.generic_params.iter().map(|p| p.name.clone()).collect())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            for impl_block in &self.module.impls {
                let matches_target = match impl_block.target {
                    crate::ir::ImplTarget::Struct(id) => Some(id) == target_struct_id,
                    crate::ir::ImplTarget::Enum(id) => Some(id) == target_enum_id,
                };
                if !matches_target {
                    continue;
                }
                for func in &impl_block.functions {
                    if func.name == method_name {
                        let mut ret = func
                            .return_type
                            .clone()
                            .or_else(|| func.body.as_ref().map(|b| b.ty().clone()))
                            .unwrap_or(ResolvedType::Primitive(PrimitiveType::Never));
                        let subs: HashMap<String, ResolvedType> = generic_params
                            .iter()
                            .cloned()
                            .zip(args.iter().cloned())
                            .collect();
                        substitute_typeparam_in_resolved(&mut ret, &subs);
                        return ret;
                    }
                }
            }
        }

        if let ResolvedType::Enum(enum_id) = receiver_ty {
            for impl_block in &self.module.impls {
                if impl_block.enum_id() == Some(*enum_id) {
                    for func in &impl_block.functions {
                        if func.name == method_name {
                            return func
                                .return_type
                                .clone()
                                .or_else(|| func.body.as_ref().map(|b| b.ty().clone()))
                                .unwrap_or(ResolvedType::Primitive(PrimitiveType::Never));
                        }
                    }
                }
            }
            self.errors.push(CompilerError::InternalError {
                detail: format!(
                    "IR lowering: no impl method `{method_name}` for enum id {}",
                    enum_id.0
                ),
                span: self.current_span,
            });
            return ResolvedType::Primitive(PrimitiveType::Never);
        }

        // TypeParam (generic parameter) or Trait receiver: look up the
        // method's return type on any trait declaring it. Semantic analysis
        // has already verified the bound is in scope.
        if let ResolvedType::TypeParam(name) = receiver_ty {
            if let Some(trait_id) = self.find_trait_for_method(name, method_name) {
                if let Some(trait_def) = self.module.get_trait(trait_id) {
                    if let Some(sig) = trait_def.methods.iter().find(|m| m.name == method_name) {
                        return sig
                            .return_type
                            .clone()
                            .unwrap_or(ResolvedType::Primitive(PrimitiveType::Never));
                    }
                }
            }
        }
        if let ResolvedType::Trait(trait_id) = receiver_ty {
            if let Some(trait_def) = self.module.get_trait(*trait_id) {
                if let Some(sig) = trait_def.methods.iter().find(|m| m.name == method_name) {
                    return sig
                        .return_type
                        .clone()
                        .unwrap_or(ResolvedType::Primitive(PrimitiveType::Never));
                }
            }
        }

        self.errors.push(CompilerError::InternalError {
            detail: format!(
                "IR lowering: cannot resolve return type of `{method_name}` on receiver {receiver_ty:?}"
            ),
            span: self.current_span,
        });
        ResolvedType::Primitive(PrimitiveType::Never)
    }

    /// Resolve the return type of a function call.
    ///
    /// Looks first in the already-lowered IR (`module.functions`), then
    /// falls back to the semantic symbol table so forward references to
    /// functions declared later in the file resolve to their declared
    /// return types. Records an `InternalError` only when neither source
    /// has an entry — in that case semantic analysis has missed the
    /// reference, which is a compiler bug.
    pub(super) fn resolve_function_return_type(
        &mut self,
        fn_name: &str,
        _args: &[(Option<String>, IrExpr)],
    ) -> ResolvedType {
        if let Some(func_id) = self.module.function_id(fn_name) {
            if let Some(func) = self.module.get_function(func_id) {
                return func
                    .return_type
                    .clone()
                    .or_else(|| func.body.as_ref().map(|b| b.ty().clone()))
                    .unwrap_or(ResolvedType::Primitive(PrimitiveType::Never));
            }
        }

        if let Some(info) = self.symbols.get_function(fn_name) {
            return info
                .return_type
                .as_ref()
                .map_or(ResolvedType::Primitive(PrimitiveType::Never), |t| {
                    self.lower_type(t)
                });
        }

        self.errors.push(CompilerError::InternalError {
            detail: format!(
                "IR lowering: unknown function `{fn_name}` reached codegen — should have been caught by semantic analysis"
            ),
            span: self.current_span,
        });
        ResolvedType::Primitive(PrimitiveType::Never)
    }
}
