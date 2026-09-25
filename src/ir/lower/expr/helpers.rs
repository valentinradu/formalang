//! Type-substitution and field/method/function return-type lookups shared
//! across the rest of the expression-lowering submodules.

use super::type_params::substitute_typeparam_in_resolved;
use crate::ast::PrimitiveType;
use crate::error::CompilerError;
use crate::ir::lower::IrLowerer;
use crate::ir::{IrExpr, ResolvedType};
use std::collections::HashMap;

impl IrLowerer<'_> {
    /// Resolve the type of a field access on an expression.
    ///
    /// Handles struct field access by looking up the field in the struct
    /// definition. Anything the semantic layer should have caught that
    /// still reaches here (missing field, field access on a non-struct
    /// type) records an `InternalError` so compilation fails loudly.
    #[expect(
        clippy::too_many_lines,
        reason = "exhaustive match over every ResolvedType variant; structural walk is the point"
    )]
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
            // Generic receiver (`Pair<I32, I32>`): peel to the base struct,
            // look up the field, and substitute the struct's `TypeParam`
            // references with the concrete argument tuple.
            ResolvedType::Generic {
                base: crate::ir::GenericBase::Struct(struct_id),
                args,
            } => {
                if let Some(struct_def) = self.module.get_struct(*struct_id) {
                    let generic_params: Vec<String> = struct_def
                        .generic_params
                        .iter()
                        .map(|p| p.name.clone())
                        .collect();
                    for field in &struct_def.fields {
                        if field.name == field_name {
                            let mut ty = field.ty.clone();
                            let subs: HashMap<String, ResolvedType> = generic_params
                                .into_iter()
                                .zip(args.iter().cloned())
                                .collect();
                            substitute_typeparam_in_resolved(&mut ty, &subs);
                            return ty;
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
            // Named-field tuple receiver: look the field up by name.
            ResolvedType::Tuple(fields) => {
                for (n, t) in fields {
                    if n == field_name {
                        return t.clone();
                    }
                }
                self.errors.push(CompilerError::InternalError {
                    detail: format!(
                        "IR lowering: tuple has no field `{field_name}` ({object_ty:?})"
                    ),
                    span: self.current_span,
                });
                ResolvedType::Primitive(PrimitiveType::Never)
            }
            // `TypeParam` receiver: a generic-parameter-typed value
            // (e.g. `<T: Tagged>` and `t.name`). Resolve the field
            // through any trait the parameter is bounded by.
            ResolvedType::TypeParam(name) => {
                if let Some(trait_id) = self.find_trait_for_field(name, field_name) {
                    if let Some(trait_def) = self.module.get_trait(trait_id) {
                        if let Some(field) = trait_def.fields.iter().find(|f| f.name == field_name)
                        {
                            return field.ty.clone();
                        }
                    }
                }
                self.errors.push(CompilerError::InternalError {
                    detail: format!(
                        "IR lowering: cannot access field `{field_name}` on TypeParam(`{name}`)"
                    ),
                    span: self.current_span,
                });
                ResolvedType::Primitive(PrimitiveType::Never)
            }
            // An imported struct. The type stays `External` until
            // `MonomorphisePass` clones the definition into this
            // module, but lowering needs the field's type now, to
            // build the access node. The symbol table already carries
            // the imported struct's fields — that is how the semantic
            // pass type-checked this access — so read the field there
            // and lower its declared type.
            ResolvedType::External {
                module_path,
                name,
                type_args,
                ..
            } => self.resolve_imported_field_type(module_path, name, type_args, field_name),
            ResolvedType::Primitive(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Enum(_)
            | ResolvedType::Generic { .. }
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
        call_args: &[(Option<String>, IrExpr)],
    ) -> ResolvedType {
        // A method of the impl that is being lowered, and a method of
        // an impl later in the file, resolve through `declared_impls`:
        // the declare pass lowered every signature first.
        let labels: Vec<Option<String>> =
            call_args.iter().map(|(label, _)| label.clone()).collect();
        let arg_types: Vec<ResolvedType> =
            call_args.iter().map(|(_, arg)| arg.ty().clone()).collect();
        if let ResolvedType::Struct(struct_id) = receiver_ty {
            for impl_block in self.module.impls.iter().chain(&self.declared_impls) {
                if impl_block.struct_id() == Some(*struct_id) {
                    if let Some(func) =
                        Self::method_for_typed_call(impl_block, method_name, &labels, &arg_types)
                    {
                        {
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
        if let ResolvedType::Generic { base, .. } = receiver_ty {
            let target = match base {
                crate::ir::GenericBase::Struct(id) => Some(crate::ir::ImplTarget::Struct(*id)),
                crate::ir::GenericBase::Enum(id) => Some(crate::ir::ImplTarget::Enum(*id)),
                // A trait base wouldn't appear here as a method-call
                // receiver post item E2. Skip and fall through.
                crate::ir::GenericBase::Trait(_) => None,
            };
            let found = self
                .module
                .impls
                .iter()
                .chain(&self.declared_impls)
                .filter(|b| Some(b.target) == target)
                .find_map(|b| {
                    Self::method_for_typed_call(b, method_name, &labels, &arg_types).map(|f| (b, f))
                });
            if let Some((impl_block, func)) = found {
                let mut ret = func
                    .return_type
                    .clone()
                    .or_else(|| func.body.as_ref().map(|b| b.ty().clone()))
                    .unwrap_or(ResolvedType::Primitive(PrimitiveType::Never));
                substitute_typeparam_in_resolved(
                    &mut ret,
                    &self.receiver_subs(receiver_ty, impl_block),
                );
                return ret;
            }
        }

        if let ResolvedType::Primitive(prim) = receiver_ty {
            for impl_block in self.module.impls.iter().chain(&self.declared_impls) {
                if matches!(impl_block.target, crate::ir::ImplTarget::Primitive(p) if p == *prim) {
                    if let Some(func) =
                        Self::method_for_typed_call(impl_block, method_name, &labels, &arg_types)
                    {
                        {
                            return func
                                .return_type
                                .clone()
                                .or_else(|| func.body.as_ref().map(|b| b.ty().clone()))
                                .unwrap_or(ResolvedType::Primitive(PrimitiveType::Never));
                        }
                    }
                }
            }
            // Fall through to the catch-all error: a missing primitive
            // method is a compiler bug.
        }

        if let ResolvedType::Enum(enum_id) = receiver_ty {
            for impl_block in self.module.impls.iter().chain(&self.declared_impls) {
                if impl_block.enum_id() == Some(*enum_id) {
                    if let Some(func) =
                        Self::method_for_typed_call(impl_block, method_name, &labels, &arg_types)
                    {
                        {
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
                        let mut ret = sig
                            .return_type
                            .clone()
                            .unwrap_or(ResolvedType::Primitive(PrimitiveType::Never));
                        // A bound on a generic trait, `<T: Container<I32>>`,
                        // gives the trait's own parameters their types.
                        // Without this, the `T` of `Container<T>` stays a
                        // type parameter, and the `T` of the function
                        // replaces it later.
                        let args = self.bound_trait_args(name, trait_id);
                        let subs: std::collections::HashMap<String, ResolvedType> = trait_def
                            .generic_params
                            .iter()
                            .zip(args)
                            .map(|(p, a)| (p.name.clone(), a))
                            .collect();
                        super::type_params::substitute_typeparam_in_resolved(&mut ret, &subs);
                        return ret;
                    }
                }
            }
        }
        if let ResolvedType::Trait(trait_id) = receiver_ty {
            // Walk the trait's own method list plus every composed
            // (super-)trait so methods inherited from a parent trait
            // resolve through the child trait's vtable.
            if let Some(ret) = self.find_trait_method_return_in_chain(*trait_id, method_name) {
                return ret;
            }
        }

        // The receiver was already an error, and a diagnostic for it has
        // been recorded. Propagate rather than cascade: a second report
        // here blames the compiler for a mistake the user has already
        // been told about. `resolve_field_type` does the same.
        if matches!(receiver_ty, ResolvedType::Error) {
            return ResolvedType::Error;
        }

        self.errors.push(CompilerError::InternalError {
            detail: format!(
                "IR lowering: cannot resolve return type of `{method_name}` on receiver {receiver_ty:?}"
            ),
            span: self.current_span,
        });
        ResolvedType::Primitive(PrimitiveType::Never)
    }

    /// Walk a trait's own methods and every composed (super-)trait
    /// chain looking for `method_name`. Used by trait-typed dispatch
    /// so a method declared on a parent trait resolves through a child
    /// trait. Returns the method's declared return type when found.
    fn find_trait_method_return_in_chain(
        &self,
        trait_id: crate::ir::TraitId,
        method_name: &str,
    ) -> Option<ResolvedType> {
        let mut visited: std::collections::HashSet<crate::ir::TraitId> =
            std::collections::HashSet::new();
        self.find_trait_method_return_visiting(trait_id, method_name, &mut visited)
    }

    fn find_trait_method_return_visiting(
        &self,
        trait_id: crate::ir::TraitId,
        method_name: &str,
        visited: &mut std::collections::HashSet<crate::ir::TraitId>,
    ) -> Option<ResolvedType> {
        if !visited.insert(trait_id) {
            return None;
        }
        let trait_def = self.module.get_trait(trait_id)?;
        if let Some(sig) = trait_def.methods.iter().find(|m| m.name == method_name) {
            return Some(
                sig.return_type
                    .clone()
                    .unwrap_or(ResolvedType::Primitive(PrimitiveType::Never)),
            );
        }
        for parent_id in trait_def.composed_traits.clone() {
            if let Some(ret) =
                self.find_trait_method_return_visiting(parent_id, method_name, visited)
            {
                return Some(ret);
            }
        }
        None
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
            // The return type can name the callee's own generic
            // parameters, so it lowers in the callee's generic scope.
            let generics = self.lower_generic_params(&info.generics);
            self.generic_scopes.push(generics);
            let ty = info
                .return_type
                .as_ref()
                .map_or(ResolvedType::Primitive(PrimitiveType::Never), |t| {
                    self.lower_type(t)
                });
            self.generic_scopes.pop();
            return ty;
        }

        self.errors.push(CompilerError::InternalError {
            detail: format!(
                "IR lowering: unknown function `{fn_name}` reached codegen — should have been caught by semantic analysis"
            ),
            span: self.current_span,
        });
        ResolvedType::Primitive(PrimitiveType::Never)
    }

    /// The type of `field_name` on an imported struct.
    ///
    /// `module_path` and `name` come from the `External` placeholder
    /// that lowering carries for an imported type. The struct's fields
    /// live in the symbol table, so the lookup reads them there and
    /// lowers the declared type of the matching field.
    ///
    /// Two things make that lowering different from an ordinary one.
    /// The field's type is written in the imported module's namespace,
    /// so `imported_source_context` is set while it is lowered: a name
    /// that resolves to nothing local then becomes another `External`
    /// for the same module, which the monomorphise pass picks up on its
    /// next pass. And a generic import (`Box<I32>`) carries its type
    /// arguments on the placeholder, so they are substituted for the
    /// struct's own parameters.
    fn resolve_imported_field_type(
        &mut self,
        module_path: &[String],
        name: &str,
        type_args: &[ResolvedType],
        field_name: &str,
    ) -> ResolvedType {
        let Some(info) = self.symbols.get_struct_qualified(name) else {
            self.errors.push(CompilerError::InternalError {
                detail: format!("IR lowering: imported struct `{name}` is not in the symbol table"),
                span: self.current_span,
            });
            return ResolvedType::Primitive(PrimitiveType::Never);
        };

        let Some(field) = info.fields.iter().find(|f| f.name == field_name) else {
            self.errors.push(CompilerError::UnknownField {
                field: field_name.to_string(),
                type_name: name.to_string(),
                span: self.current_span,
            });
            return ResolvedType::Primitive(PrimitiveType::Never);
        };

        // Both are read while `info` is still borrowed from the symbol
        // table; lowering the type below needs `&mut self`.
        let declared = field.ty.clone();
        let subs: HashMap<String, ResolvedType> = info
            .generics
            .iter()
            .map(|g| g.name.name.clone())
            .zip(type_args.iter().cloned())
            .collect();

        let saved = self.imported_source_context.replace(module_path.to_vec());
        let mut resolved = self.lower_type(&declared);
        self.imported_source_context = saved;

        substitute_typeparam_in_resolved(&mut resolved, &subs);
        resolved
    }
}
