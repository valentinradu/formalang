//! Expression lowering helpers for the IR lowering pass.

use super::IrLowerer;
use crate::ast::{
    self, BinaryOperator, BindingPattern, BlockStatement, ClosureParam, Expr, Literal,
    ParamConvention, PrimitiveType, UnaryOperator,
};
use crate::error::CompilerError;
use crate::ir::{
    DispatchKind, ImplId, IrBlockStatement, IrExpr, IrMatchArm, ResolvedType, TraitId,
};

impl IrLowerer<'_> {
    pub(super) fn lower_expr(&mut self, expr: &Expr) -> IrExpr {
        // Track the span of the current expression so that InternalError
        // diagnostics surfaced during lowering carry a real source
        // location. See audit finding #31.
        let prev_span = self.current_span;
        self.current_span = expr.span();
        let out = self.lower_expr_inner(expr);
        self.current_span = prev_span;
        out
    }

    fn lower_expr_inner(&mut self, expr: &Expr) -> IrExpr {
        match expr {
            Expr::Literal { value: lit, .. } => IrExpr::Literal {
                value: lit.clone(),
                ty: Self::literal_type(lit),
            },
            Expr::Invocation {
                path,
                type_args,
                args,
                ..
            } => self.lower_invocation(path, type_args, args),
            Expr::EnumInstantiation {
                enum_name,
                variant,
                data,
                ..
            } => self.lower_enum_instantiation(&enum_name.name, &variant.name, data),
            Expr::InferredEnumInstantiation { variant, data, .. } => {
                self.lower_inferred_enum_instantiation(&variant.name, data)
            }
            Expr::Array { elements, .. } => self.lower_array_expr(elements),
            Expr::Tuple { fields, .. } => self.lower_tuple_expr(fields),
            Expr::Reference { path, .. } => self.lower_reference(path),
            Expr::BinaryOp {
                left, op, right, ..
            } => self.lower_binary_op_expr(left, *op, right),
            Expr::UnaryOp { op, operand, .. } => self.lower_unary_op_expr(*op, operand),
            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.lower_if_expr(condition, then_branch, else_branch.as_deref()),
            Expr::ForExpr {
                var,
                collection,
                body,
                ..
            } => self.lower_for_expr(var, collection, body),
            Expr::MatchExpr {
                scrutinee, arms, ..
            } => self.lower_match_expr(scrutinee, arms),
            Expr::Group { expr, .. } => self.lower_expr(expr),
            Expr::LetExpr {
                mutable,
                pattern,
                ty,
                value,
                body,
                ..
            } => self.lower_let_expr(*mutable, pattern, ty.as_ref(), value, body),
            Expr::DictLiteral { entries, .. } => self.lower_dict_literal(entries),
            Expr::DictAccess { dict, key, .. } => self.lower_dict_access(dict, key),
            Expr::ClosureExpr { params, body, .. } => self.lower_closure(params, body),
            Expr::FieldAccess { object, field, .. } => {
                let object_ir = self.lower_expr(object);
                let ty = self.resolve_field_type(object_ir.ty(), &field.name);
                IrExpr::FieldAccess {
                    object: Box::new(object_ir),
                    field: field.name.clone(),
                    ty,
                }
            }
            Expr::MethodCall {
                receiver,
                method,
                args,
                ..
            } => self.lower_method_call(receiver, &method.name, args.as_slice()),
            Expr::Block {
                statements, result, ..
            } => self.lower_block_expr(statements, result),
        }
    }

    fn lower_invocation(
        &mut self,
        path: &[crate::ast::Ident],
        type_args: &[crate::ast::Type],
        args: &[(Option<crate::ast::Ident>, Expr)],
    ) -> IrExpr {
        let name = path
            .iter()
            .map(|id| id.name.as_str())
            .collect::<Vec<_>>()
            .join("::");
        let type_args_resolved: Vec<ResolvedType> =
            type_args.iter().map(|t| self.lower_type(t)).collect();

        if let Some(id) = self.module.struct_id(&name) {
            let ty = if type_args_resolved.is_empty() {
                ResolvedType::Struct(id)
            } else {
                ResolvedType::Generic {
                    base: crate::ir::GenericBase::Struct(id),
                    args: type_args_resolved.clone(),
                }
            };
            let named_fields: Vec<(String, IrExpr)> = args
                .iter()
                .filter_map(|(name_opt, expr)| {
                    name_opt
                        .as_ref()
                        .map(|n| (n.name.clone(), self.lower_expr(expr)))
                })
                .collect();
            IrExpr::StructInst {
                struct_id: Some(id),
                type_args: type_args_resolved,
                fields: named_fields,
                ty,
            }
        } else if let Some(external_ty) = self.try_external_type(&name, type_args_resolved.clone())
        {
            let named_fields: Vec<(String, IrExpr)> = args
                .iter()
                .filter_map(|(name_opt, expr)| {
                    name_opt
                        .as_ref()
                        .map(|n| (n.name.clone(), self.lower_expr(expr)))
                })
                .collect();
            IrExpr::StructInst {
                struct_id: None,
                type_args: type_args_resolved,
                fields: named_fields,
                ty: external_ty,
            }
        } else {
            let path_strs: Vec<String> = path.iter().map(|i| i.name.clone()).collect();
            let lowered_args: Vec<(Option<String>, IrExpr)> = args
                .iter()
                .map(|(name_opt, expr)| {
                    (
                        name_opt.as_ref().map(|n| n.name.clone()),
                        self.lower_expr(expr),
                    )
                })
                .collect();
            let fn_name = path_strs.last().map_or("", std::string::String::as_str);
            let ty = self.resolve_function_return_type(fn_name, &lowered_args);
            IrExpr::FunctionCall {
                path: path_strs,
                args: lowered_args,
                ty,
            }
        }
    }

    fn lower_enum_instantiation(
        &mut self,
        enum_name: &str,
        variant: &str,
        data: &[(crate::ast::Ident, Expr)],
    ) -> IrExpr {
        let (enum_id, ty) = self.module.enum_id(enum_name).map_or_else(
            || {
                self.try_external_type(enum_name, vec![]).map_or_else(
                    || (None, ResolvedType::TypeParam(enum_name.to_string())),
                    |external_ty| (None, external_ty),
                )
            },
            |id| (Some(id), ResolvedType::Enum(id)),
        );
        IrExpr::EnumInst {
            enum_id,
            variant: variant.to_string(),
            fields: data
                .iter()
                .map(|(n, e)| (n.name.clone(), self.lower_expr(e)))
                .collect(),
            ty,
        }
    }

    fn lower_inferred_enum_instantiation(
        &mut self,
        variant: &str,
        data: &[(crate::ast::Ident, Expr)],
    ) -> IrExpr {
        // Inferred-enum uses outside a return-typed context (e.g. struct
        // field defaults, top-level lets) are a known gap — the upstream
        // context-threading work in audit finding #27 will surface the
        // real signal, so we leave a TypeParam placeholder without error.
        #[expect(
            clippy::option_if_let_else,
            reason = "three-branch resolution (local enum / external / error) reads clearer as if/else"
        )]
        let (enum_id, ty) = match self.current_function_return_type.clone() {
            None => (None, ResolvedType::TypeParam("InferredEnum".to_string())),
            Some(name) => {
                if let Some(id) = self.module.enum_id(&name) {
                    (Some(id), ResolvedType::Enum(id))
                } else if let Some(external_ty) = self.try_external_type(&name, vec![]) {
                    (None, external_ty)
                } else {
                    (
                        None,
                        self.internal_error_type(format!(
                            "inferred-enum `.{variant}` has no resolvable return-type enum `{name}`",
                        )),
                    )
                }
            }
        };
        IrExpr::EnumInst {
            enum_id,
            variant: variant.to_string(),
            fields: data
                .iter()
                .map(|(n, e)| (n.name.clone(), self.lower_expr(e)))
                .collect(),
            ty,
        }
    }

    fn lower_array_expr(&mut self, elements: &[Expr]) -> IrExpr {
        let lowered: Vec<IrExpr> = elements.iter().map(|e| self.lower_expr(e)).collect();
        let elem_ty = lowered.first().map_or_else(
            || ResolvedType::TypeParam("UnknownElement".to_string()),
            |e| e.ty().clone(),
        );
        IrExpr::Array {
            elements: lowered,
            ty: ResolvedType::Array(Box::new(elem_ty)),
        }
    }

    fn lower_tuple_expr(&mut self, fields: &[(crate::ast::Ident, Expr)]) -> IrExpr {
        let lowered: Vec<(String, IrExpr)> = fields
            .iter()
            .map(|(n, e)| (n.name.clone(), self.lower_expr(e)))
            .collect();
        let tuple_types: Vec<(String, ResolvedType)> = lowered
            .iter()
            .map(|(n, e)| (n.clone(), e.ty().clone()))
            .collect();
        IrExpr::Tuple {
            fields: lowered,
            ty: ResolvedType::Tuple(tuple_types),
        }
    }

    fn lower_reference(&mut self, path: &[crate::ast::Ident]) -> IrExpr {
        let path_strs: Vec<String> = path.iter().map(|i| i.name.clone()).collect();

        // Check for self.field pattern — bounds verified by len() == 2 check
        #[expect(
            clippy::indexing_slicing,
            reason = "len == 2 check above guarantees indices 0 and 1"
        )]
        if path_strs.len() == 2 && path_strs[0] == "self" {
            let field_name = &path_strs[1];
            let ty = self.resolve_self_field_type(field_name);
            return IrExpr::SelfFieldRef {
                field: field_name.clone(),
                ty,
            };
        }

        // Check for bare "self" in impl context — bounds verified by len() == 1 check
        #[expect(
            clippy::indexing_slicing,
            reason = "len == 1 check above guarantees index 0"
        )]
        if path_strs.len() == 1 && path_strs[0] == "self" {
            if let Some(impl_name) = self.current_impl_struct.clone() {
                let ty = self.resolve_impl_self_type(&impl_name);
                return IrExpr::Reference {
                    path: path_strs,
                    ty,
                };
            }
        }

        // Check for module-level let binding reference
        if path_strs.len() == 1 {
            #[expect(
                clippy::indexing_slicing,
                reason = "len == 1 check above guarantees index 0"
            )]
            let name = &path_strs[0];
            if let Some(let_type) = self.symbols.get_let_type(name) {
                let ty = self.string_to_resolved_type(let_type);
                return IrExpr::LetRef {
                    name: name.clone(),
                    ty,
                };
            }
        }

        // Resolve the root of the path against the local scope (function
        // parameters, `self`, enclosing closure bindings). For multi-
        // segment paths, walk each subsequent segment as a field access so
        // `u.x.y` resolves to the actual `y` field's type instead of a
        // `TypeParam("u.x.y")` placeholder.
        let ty = path_strs
            .first()
            .and_then(|n| self.lookup_local_binding(n).cloned())
            .map_or_else(
                || {
                    if path_strs.len() == 1 {
                        #[expect(
                            clippy::indexing_slicing,
                            reason = "len == 1 check above guarantees index 0"
                        )]
                        let t = ResolvedType::TypeParam(path_strs[0].clone());
                        t
                    } else {
                        ResolvedType::TypeParam(path_strs.join("."))
                    }
                },
                |root_ty| {
                    let mut current = root_ty;
                    for seg in path_strs.iter().skip(1) {
                        current = self.resolve_field_type(&current, seg);
                    }
                    current
                },
            );
        IrExpr::Reference {
            path: path_strs,
            ty,
        }
    }

    fn lower_binary_op_expr(&mut self, left: &Expr, op: BinaryOperator, right: &Expr) -> IrExpr {
        let left_ir = self.lower_expr(left);
        let right_ir = self.lower_expr(right);
        let ty = match op {
            BinaryOperator::Eq
            | BinaryOperator::Ne
            | BinaryOperator::Lt
            | BinaryOperator::Le
            | BinaryOperator::Gt
            | BinaryOperator::Ge
            | BinaryOperator::And
            | BinaryOperator::Or => ResolvedType::Primitive(PrimitiveType::Boolean),
            BinaryOperator::Add
            | BinaryOperator::Sub
            | BinaryOperator::Mul
            | BinaryOperator::Div
            | BinaryOperator::Mod => left_ir.ty().clone(),
            BinaryOperator::Range => ResolvedType::Range(Box::new(left_ir.ty().clone())),
        };
        IrExpr::BinaryOp {
            left: Box::new(left_ir),
            op,
            right: Box::new(right_ir),
            ty,
        }
    }

    fn lower_unary_op_expr(&mut self, op: UnaryOperator, operand: &Expr) -> IrExpr {
        let operand_ir = self.lower_expr(operand);
        let ty = match op {
            UnaryOperator::Not => ResolvedType::Primitive(PrimitiveType::Boolean),
            UnaryOperator::Neg => operand_ir.ty().clone(),
        };
        IrExpr::UnaryOp {
            op,
            operand: Box::new(operand_ir),
            ty,
        }
    }

    fn lower_if_expr(
        &mut self,
        condition: &Expr,
        then_branch: &Expr,
        else_branch: Option<&Expr>,
    ) -> IrExpr {
        let then_ir = self.lower_expr(then_branch);
        let ty = then_ir.ty().clone();
        IrExpr::If {
            condition: Box::new(self.lower_expr(condition)),
            then_branch: Box::new(then_ir),
            else_branch: else_branch.map(|e| Box::new(self.lower_expr(e))),
            ty,
        }
    }

    fn lower_for_expr(
        &mut self,
        var: &crate::ast::Ident,
        collection: &Expr,
        body: &Expr,
    ) -> IrExpr {
        let collection_ir = self.lower_expr(collection);
        let body_ir = self.lower_expr(body);
        let bad_collection = collection_ir.ty().clone();
        let var_ty = if let ResolvedType::Array(inner) | ResolvedType::Range(inner) =
            &bad_collection
        {
            (**inner).clone()
        } else {
            self.internal_error_type_if_concrete(
                &bad_collection,
                format!(
                    "for-loop collection lowered to non-iterable type {bad_collection:?}; semantic should have caught this",
                ),
            )
        };
        IrExpr::For {
            var: var.name.clone(),
            var_ty,
            collection: Box::new(collection_ir),
            body: Box::new(body_ir.clone()),
            ty: ResolvedType::Array(Box::new(body_ir.ty().clone())),
        }
    }

    fn lower_match_expr(&mut self, scrutinee: &Expr, arms: &[crate::ast::MatchArm]) -> IrExpr {
        let scrutinee_ir = self.lower_expr(scrutinee);
        let arms_ir: Vec<IrMatchArm> = arms
            .iter()
            .map(|arm| {
                let bindings = self.extract_pattern_bindings(&arm.pattern, &scrutinee_ir);
                IrMatchArm {
                    variant: match &arm.pattern {
                        ast::Pattern::Variant { name, .. } => name.name.clone(),
                        ast::Pattern::Wildcard => String::new(),
                    },
                    is_wildcard: matches!(&arm.pattern, ast::Pattern::Wildcard),
                    bindings,
                    body: self.lower_expr(&arm.body),
                }
            })
            .collect();
        let ty = arms_ir.first().map_or_else(
            || self.internal_error_type("match expression with no arms reached IR lowering".into()),
            |a| a.body.ty().clone(),
        );
        IrExpr::Match {
            scrutinee: Box::new(scrutinee_ir),
            arms: arms_ir,
            ty,
        }
    }

    fn lower_dict_literal(&mut self, entries: &[(Expr, Expr)]) -> IrExpr {
        let lowered_entries: Vec<(IrExpr, IrExpr)> = entries
            .iter()
            .map(|(k, v)| (self.lower_expr(k), self.lower_expr(v)))
            .collect();
        let ty = if let Some((k, v)) = lowered_entries.first() {
            ResolvedType::Dictionary {
                key_ty: Box::new(k.ty().clone()),
                value_ty: Box::new(v.ty().clone()),
            }
        } else {
            ResolvedType::Dictionary {
                key_ty: Box::new(ResolvedType::TypeParam("K".to_string())),
                value_ty: Box::new(ResolvedType::TypeParam("V".to_string())),
            }
        };
        IrExpr::DictLiteral {
            entries: lowered_entries,
            ty,
        }
    }

    fn lower_dict_access(&mut self, dict: &Expr, key: &Expr) -> IrExpr {
        let dict_ir = self.lower_expr(dict);
        let key_ir = self.lower_expr(key);
        let bad_dict = dict_ir.ty().clone();
        let ty = if let ResolvedType::Dictionary { value_ty, .. } = &bad_dict {
            (**value_ty).clone()
        } else {
            self.internal_error_type_if_concrete(
                &bad_dict,
                format!(
                    "dict-access receiver lowered to non-dictionary type {bad_dict:?}; semantic should have caught this",
                ),
            )
        };
        IrExpr::DictAccess {
            dict: Box::new(dict_ir),
            key: Box::new(key_ir),
            ty,
        }
    }

    fn lower_method_call(
        &mut self,
        receiver: &Expr,
        method_name: &str,
        args: &[(Option<crate::ast::Ident>, Expr)],
    ) -> IrExpr {
        let receiver_ir = self.lower_expr(receiver);
        let lowered_args: Vec<(Option<String>, IrExpr)> = args
            .iter()
            .map(|(label, expr)| {
                (
                    label.as_ref().map(|l| l.name.clone()),
                    self.lower_expr(expr),
                )
            })
            .collect();
        let ty = self.resolve_method_return_type(receiver_ir.ty(), method_name);
        let dispatch = self.resolve_dispatch_kind(receiver_ir.ty(), method_name);
        IrExpr::MethodCall {
            receiver: Box::new(receiver_ir),
            method: method_name.to_string(),
            args: lowered_args,
            dispatch,
            ty,
        }
    }

    /// Resolve the dispatch kind for a method call.
    ///
    /// * Concrete struct/enum receivers resolve to `Static` dispatch pointing
    ///   at the impl block that provides the method body. When the call site
    ///   is inside the impl that is still being lowered, the `ImplId` refers
    ///   to the slot that impl will occupy in `module.impls` once finalized.
    /// * Type-parameter receivers (`T: Trait`) and trait-object receivers
    ///   resolve to `Virtual` dispatch through the relevant trait.
    /// * Other receiver shapes (primitives, arrays, tuples, etc.) are a
    ///   compiler bug at this layer — semantic analysis should have rejected
    ///   them. We record an `InternalError` and return a sentinel
    ///   `Virtual` dispatch pointing at `TraitId(u32::MAX)` so downstream
    ///   code never silently emits against a bogus trait id.
    fn resolve_dispatch_kind(
        &mut self,
        receiver_ty: &ResolvedType,
        method_name: &str,
    ) -> DispatchKind {
        // Unwrap a Generic wrapper to its base so `Box<T>.method()` and
        // `Option<T>.method()` dispatch the same way a concrete Struct/Enum
        // receiver would.
        let concrete = match receiver_ty {
            ResolvedType::Generic { base, .. } => match base {
                crate::ir::GenericBase::Struct(id) => Some(ResolvedType::Struct(*id)),
                crate::ir::GenericBase::Enum(id) => Some(ResolvedType::Enum(*id)),
            },
            ResolvedType::Primitive(_)
            | ResolvedType::Struct(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Enum(_)
            | ResolvedType::Array(_)
            | ResolvedType::Range(_)
            | ResolvedType::Optional(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::Dictionary { .. }
            | ResolvedType::Closure { .. } => None,
        };
        let effective_ty = concrete.as_ref().unwrap_or(receiver_ty);

        if let ResolvedType::Struct(struct_id) = effective_ty {
            if let Some(impl_id) = self.find_impl_for_struct(*struct_id, method_name) {
                return DispatchKind::Static { impl_id };
            }
            return DispatchKind::Static {
                impl_id: self.next_impl_id_or_record(),
            };
        }

        if let ResolvedType::Enum(enum_id) = effective_ty {
            if let Some(impl_id) = self.find_impl_for_enum(*enum_id, method_name) {
                return DispatchKind::Static { impl_id };
            }
            return DispatchKind::Static {
                impl_id: self.next_impl_id_or_record(),
            };
        }

        if let ResolvedType::TypeParam(param_name) = receiver_ty {
            if let Some(trait_id) = self.find_trait_for_method(param_name, method_name) {
                return DispatchKind::Virtual {
                    trait_id,
                    method_name: method_name.to_string(),
                };
            }
        }

        if let ResolvedType::Trait(trait_id) = receiver_ty {
            return DispatchKind::Virtual {
                trait_id: *trait_id,
                method_name: method_name.to_string(),
            };
        }

        self.errors.push(CompilerError::InternalError {
            detail: format!(
                "IR lowering: cannot resolve dispatch for method `{method_name}` on receiver {receiver_ty:?}"
            ),
            span: crate::location::Span::default(),
        });
        DispatchKind::Virtual {
            trait_id: TraitId(u32::MAX),
            method_name: method_name.to_string(),
        }
    }

    /// Return the `ImplId` that will be assigned to the next impl block added.
    /// On u32 overflow, records a `TooManyDefinitions` error and returns a
    /// sentinel ID so compilation fails loudly rather than producing wrong dispatch.
    fn next_impl_id_or_record(&mut self) -> ImplId {
        self.module.next_impl_id().unwrap_or_else(|| {
            self.errors.push(CompilerError::TooManyDefinitions {
                kind: "impl",
                span: crate::location::Span::default(),
            });
            ImplId(u32::MAX)
        })
    }

    /// Record `TooManyDefinitions` for an impl index that does not fit in `u32`
    /// and return a sentinel `ImplId`. Callers should have already established
    /// an `add_impl`-enforced invariant; this path exists purely to keep the
    /// compiler type-safe without an unchecked cast.
    fn impl_id_from_idx(&mut self, idx: usize) -> ImplId {
        if let Ok(v) = u32::try_from(idx) {
            ImplId(v)
        } else {
            self.errors.push(CompilerError::TooManyDefinitions {
                kind: "impl",
                span: crate::location::Span::default(),
            });
            ImplId(u32::MAX)
        }
    }

    fn trait_id_from_idx(&mut self, idx: usize) -> TraitId {
        if let Ok(v) = u32::try_from(idx) {
            TraitId(v)
        } else {
            self.errors.push(CompilerError::TooManyDefinitions {
                kind: "trait",
                span: crate::location::Span::default(),
            });
            TraitId(u32::MAX)
        }
    }

    fn find_impl_for_struct(
        &mut self,
        id: crate::ir::StructId,
        method_name: &str,
    ) -> Option<ImplId> {
        let found_idx = self.module.impls.iter().enumerate().find_map(|(idx, b)| {
            if b.struct_id() == Some(id) && b.functions.iter().any(|f| f.name == method_name) {
                Some(idx)
            } else {
                None
            }
        })?;
        Some(self.impl_id_from_idx(found_idx))
    }

    fn find_impl_for_enum(&mut self, id: crate::ir::EnumId, method_name: &str) -> Option<ImplId> {
        let found_idx = self.module.impls.iter().enumerate().find_map(|(idx, b)| {
            if b.enum_id() == Some(id) && b.functions.iter().any(|f| f.name == method_name) {
                Some(idx)
            } else {
                None
            }
        })?;
        Some(self.impl_id_from_idx(found_idx))
    }

    /// Look up the trait that declares `method_name` among the constraints
    /// attached to generic parameter `param_name`. Walks the innermost
    /// generic scope outwards, finds the param by name, then scans its
    /// trait constraints. Falls back to a module-wide search only when the
    /// param is not in any active scope (e.g. a lowering invariant was
    /// violated upstream) — this matches the pre-#12 behaviour so we don't
    /// regress on cases where the scope hasn't been populated.
    fn find_trait_for_method(&mut self, param_name: &str, method_name: &str) -> Option<TraitId> {
        for frame in self.generic_scopes.iter().rev() {
            if let Some(param) = frame.iter().find(|p| p.name == param_name) {
                for trait_id in &param.constraints {
                    let idx = trait_id.0 as usize;
                    if let Some(trait_def) = self.module.traits.get(idx) {
                        if trait_def.methods.iter().any(|m| m.name == method_name) {
                            return Some(*trait_id);
                        }
                    }
                }
                // Param is in scope but none of its constraints declare the
                // method — the semantic analyser should already have flagged
                // this; return None rather than picking an unrelated trait.
                return None;
            }
        }
        // Fallback: no matching scope frame — behave as before.
        let found_idx = self
            .module
            .traits
            .iter()
            .enumerate()
            .find_map(|(idx, trait_def)| {
                trait_def
                    .methods
                    .iter()
                    .any(|m| m.name == method_name)
                    .then_some(idx)
            })?;
        Some(self.trait_id_from_idx(found_idx))
    }

    /// Lower a `let pat = val in body` expression into a block with the
    /// binding as one or more statements. Destructuring patterns are
    /// expanded into per-field let statements so the bindings actually
    /// reach the body — previously they collapsed to a single `_let`
    /// binding. See audit finding #21.
    fn lower_let_expr(
        &mut self,
        mutable: bool,
        pattern: &BindingPattern,
        ty: Option<&ast::Type>,
        value: &Expr,
        body: &Expr,
    ) -> IrExpr {
        let ir_value = self.lower_expr(value);
        let ir_ty = ty.map(|t| self.lower_type(t));

        let statements: Vec<IrBlockStatement> = match pattern {
            BindingPattern::Simple(ident) => vec![IrBlockStatement::Let {
                name: ident.name.clone(),
                mutable,
                ty: ir_ty,
                value: ir_value,
            }],
            BindingPattern::Array { elements, .. } => {
                self.lower_let_array_destructure(elements, mutable, &ir_value)
            }
            BindingPattern::Struct { fields, .. } => {
                self.lower_let_struct_destructure(fields, mutable, &ir_value)
            }
            BindingPattern::Tuple { elements, .. } => {
                self.lower_let_tuple_destructure(elements, mutable, &ir_value)
            }
        };
        let ir_body = self.lower_expr(body);
        let ty = ir_body.ty().clone();
        IrExpr::Block {
            statements,
            result: Box::new(ir_body),
            ty,
        }
    }

    fn lower_block_expr(&mut self, statements: &[BlockStatement], result: &Expr) -> IrExpr {
        let ir_statements: Vec<IrBlockStatement> = statements
            .iter()
            .flat_map(|stmt| self.lower_block_statement(stmt))
            .collect();
        let ir_result = self.lower_expr(result);
        let ty = ir_result.ty().clone();
        if ir_statements.is_empty() {
            return ir_result;
        }
        IrExpr::Block {
            statements: ir_statements,
            result: Box::new(ir_result),
            ty,
        }
    }

    /// Lower an AST block statement to one or more IR block statements.
    pub(super) fn lower_block_statement(&mut self, stmt: &BlockStatement) -> Vec<IrBlockStatement> {
        match stmt {
            BlockStatement::Let {
                mutable,
                pattern,
                ty,
                value,
                ..
            } => {
                let ir_value = self.lower_expr(value);
                let ir_ty = ty.as_ref().map(|t| self.lower_type(t));
                match pattern {
                    BindingPattern::Simple(ident) => vec![IrBlockStatement::Let {
                        name: ident.name.clone(),
                        mutable: *mutable,
                        ty: ir_ty,
                        value: ir_value,
                    }],
                    BindingPattern::Array { elements, .. } => {
                        self.lower_let_array_destructure(elements, *mutable, &ir_value)
                    }
                    BindingPattern::Struct { fields, .. } => {
                        self.lower_let_struct_destructure(fields, *mutable, &ir_value)
                    }
                    BindingPattern::Tuple { elements, .. } => {
                        self.lower_let_tuple_destructure(elements, *mutable, &ir_value)
                    }
                }
            }
            BlockStatement::Assign { target, value, .. } => {
                vec![IrBlockStatement::Assign {
                    target: self.lower_expr(target),
                    value: self.lower_expr(value),
                }]
            }
            BlockStatement::Expr(expr) => {
                vec![IrBlockStatement::Expr(self.lower_expr(expr))]
            }
        }
    }

    fn lower_let_array_destructure(
        &mut self,
        elements: &[crate::ast::ArrayPatternElement],
        mutable: bool,
        ir_value: &IrExpr,
    ) -> Vec<IrBlockStatement> {
        let bad_recv = ir_value.ty().clone();
        let elem_ty = if let ResolvedType::Array(inner) = &bad_recv {
            (**inner).clone()
        } else {
            self.internal_error_type_if_concrete(
                &bad_recv,
                format!("let array-destructure receiver lowered to non-array type {bad_recv:?}"),
            )
        };
        elements
            .iter()
            .enumerate()
            .filter_map(|(i, elem)| {
                Self::extract_block_binding_name(elem).map(|name| {
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "array indices are small positions that fit in f64 mantissa"
                    )]
                    let key = IrExpr::Literal {
                        value: Literal::Number(i as f64),
                        ty: ResolvedType::Primitive(PrimitiveType::Number),
                    };
                    IrBlockStatement::Let {
                        name,
                        mutable,
                        ty: Some(elem_ty.clone()),
                        value: IrExpr::DictAccess {
                            dict: Box::new(ir_value.clone()),
                            key: Box::new(key),
                            ty: elem_ty.clone(),
                        },
                    }
                })
            })
            .collect()
    }

    fn lower_let_struct_destructure(
        &mut self,
        fields: &[crate::ast::StructPatternField],
        mutable: bool,
        ir_value: &IrExpr,
    ) -> Vec<IrBlockStatement> {
        fields
            .iter()
            .map(|field| {
                let field_name = field.name.name.clone();
                let binding_name = field
                    .alias
                    .as_ref()
                    .map_or_else(|| field_name.clone(), |a| a.name.clone());
                let field_ty = self.get_field_type_from_resolved(ir_value.ty(), &field_name);
                IrBlockStatement::Let {
                    name: binding_name,
                    mutable,
                    ty: Some(field_ty.clone()),
                    value: IrExpr::FieldAccess {
                        object: Box::new(ir_value.clone()),
                        field: field_name,
                        ty: field_ty,
                    },
                }
            })
            .collect()
    }

    fn lower_let_tuple_destructure(
        &mut self,
        elements: &[crate::ast::BindingPattern],
        mutable: bool,
        ir_value: &IrExpr,
    ) -> Vec<IrBlockStatement> {
        let bad_tuple = ir_value.ty().clone();
        let tuple_types = if let ResolvedType::Tuple(fields) = &bad_tuple {
            fields.clone()
        } else {
            let _ = self.internal_error_type_if_concrete(
                &bad_tuple,
                format!("let tuple-destructure receiver lowered to non-tuple type {bad_tuple:?}"),
            );
            Vec::new()
        };
        // The "out-of-range" placeholder is only used if a binding index
        // overshoots the tuple's fields; we lazily build it to avoid
        // pushing a spurious error on every well-formed destructure.
        let out_of_range_ty = if elements.len() > tuple_types.len() && !tuple_types.is_empty() {
            self.internal_error_type(format!(
                "let tuple-destructure binds {} names but receiver has {} fields",
                elements.len(),
                tuple_types.len(),
            ))
        } else {
            ResolvedType::TypeParam("Unknown".to_string())
        };
        elements
            .iter()
            .enumerate()
            .filter_map(|(i, elem)| {
                IrLowerer::extract_simple_binding_name(elem).map(|name| {
                    let (field_name, ty) = tuple_types.get(i).map_or_else(
                        || (i.to_string(), out_of_range_ty.clone()),
                        |(n, t)| (n.clone(), t.clone()),
                    );
                    IrBlockStatement::Let {
                        name,
                        mutable,
                        ty: Some(ty.clone()),
                        value: IrExpr::FieldAccess {
                            object: Box::new(ir_value.clone()),
                            field: field_name,
                            ty,
                        },
                    }
                })
            })
            .collect()
    }

    fn extract_block_binding_name(elem: &crate::ast::ArrayPatternElement) -> Option<String> {
        match elem {
            crate::ast::ArrayPatternElement::Binding(p) => {
                IrLowerer::extract_simple_binding_name(p)
            }
            crate::ast::ArrayPatternElement::Rest(Some(ident)) => Some(ident.name.clone()),
            crate::ast::ArrayPatternElement::Rest(None)
            | crate::ast::ArrayPatternElement::Wildcard => None,
        }
    }

    pub(super) fn literal_type(lit: &Literal) -> ResolvedType {
        match lit {
            Literal::String(_) => ResolvedType::Primitive(PrimitiveType::String),
            Literal::Number(_) => ResolvedType::Primitive(PrimitiveType::Number),
            Literal::Boolean(_) => ResolvedType::Primitive(PrimitiveType::Boolean),
            Literal::Path(_) => ResolvedType::Primitive(PrimitiveType::Path),
            Literal::Regex { .. } => ResolvedType::Primitive(PrimitiveType::Regex),
            Literal::Nil => ResolvedType::TypeParam("Nil".to_string()),
        }
    }

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
                        span: crate::location::Span::default(),
                    });
                } else {
                    self.errors.push(CompilerError::InternalError {
                        detail: format!(
                            "IR lowering: struct id {} out of bounds during field access `{field_name}`",
                            struct_id.0
                        ),
                        span: crate::location::Span::default(),
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
                    span: crate::location::Span::default(),
                });
                ResolvedType::Primitive(PrimitiveType::Never)
            }
        }
    }

    /// Resolve the return type of a method call.
    ///
    /// Looks up user-defined methods in impl blocks. Records an
    /// `InternalError` when the method cannot be resolved on a concrete
    /// receiver — those cases should have been caught by semantic
    /// analysis and reaching here indicates a compiler bug.
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
                span: crate::location::Span::default(),
            });
            return ResolvedType::Primitive(PrimitiveType::Never);
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
                span: crate::location::Span::default(),
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
            span: crate::location::Span::default(),
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
            span: crate::location::Span::default(),
        });
        ResolvedType::Primitive(PrimitiveType::Never)
    }

    /// Lower a closure expression.
    ///
    /// Lowers parameters and body to a `Closure` IR node, and collects the
    /// free variables (captures) referenced by the body. The regular lowering
    /// path handles all closure cases uniformly, including closures whose body
    /// is an enum variant construction.
    fn lower_closure(&mut self, params: &[ClosureParam], body: &Expr) -> IrExpr {
        // General closure: lower params and body
        let lowered_params: Vec<(ParamConvention, String, ResolvedType)> = params
            .iter()
            .map(|p| {
                let ty = p.ty.as_ref().map_or_else(
                    || ResolvedType::TypeParam("Unknown".to_string()),
                    |t| self.lower_type(t),
                );
                (p.convention, p.name.name.clone(), ty)
            })
            .collect();

        let body_ir = self.lower_expr(body);
        let return_ty = body_ir.ty().clone();

        let param_names: std::collections::HashSet<String> =
            lowered_params.iter().map(|(_, n, _)| n.clone()).collect();
        let mut captures: Vec<(String, ResolvedType)> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        collect_free_refs(&body_ir, &param_names, &mut captures, &mut seen);

        // Audit #32: tag each capture with a convention. Today the IR
        // lowerer doesn't have access to the outer binding's declared
        // convention (the semantic layer has that in
        // `current_fn_param_conventions` and `closure_binding_conventions`
        // but those aren't plumbed into IrLowerer). For now every capture
        // defaults to `Let` (immutable view); backends requiring
        // move / mut / sink semantics can refine with a follow-up pass.
        let captures_with_mode: Vec<(String, ParamConvention, ResolvedType)> = captures
            .into_iter()
            .map(|(name, ty)| (name, ParamConvention::Let, ty))
            .collect();

        let ty = ResolvedType::Closure {
            param_tys: lowered_params
                .iter()
                .map(|(c, _, t)| (*c, t.clone()))
                .collect(),
            return_ty: Box::new(return_ty),
        };

        IrExpr::Closure {
            params: lowered_params,
            captures: captures_with_mode,
            body: Box::new(body_ir),
            ty,
        }
    }

    pub(super) fn extract_pattern_bindings(
        &mut self,
        pattern: &ast::Pattern,
        scrutinee: &IrExpr,
    ) -> Vec<(String, ResolvedType)> {
        match pattern {
            ast::Pattern::Variant { name, bindings } => {
                // Try to find variant field types from the enum
                let variant_fields = self.get_variant_fields(scrutinee.ty(), &name.name);
                let has_overflow = bindings.len() > variant_fields.len();
                // Only emit an error when the scrutinee's type is already a
                // concrete enum; if it's a TypeParam (unresolved path), the
                // overflow is a downstream artefact of the upstream gap.
                let out_of_range_ty = if has_overflow
                    && !matches!(scrutinee.ty(), ResolvedType::TypeParam(_))
                {
                    self.internal_error_type(format!(
                        "match pattern `{}` binds more names ({}) than the variant has fields ({}); semantic should have caught this",
                        name.name,
                        bindings.len(),
                        variant_fields.len(),
                    ))
                } else {
                    ResolvedType::TypeParam("Unknown".to_string())
                };
                bindings
                    .iter()
                    .enumerate()
                    .map(|(i, ident)| {
                        let ty = variant_fields
                            .get(i)
                            .cloned()
                            .unwrap_or_else(|| out_of_range_ty.clone());
                        (ident.name.clone(), ty)
                    })
                    .collect()
            }
            ast::Pattern::Wildcard => {
                // Wildcard has no bindings
                Vec::new()
            }
        }
    }
}

/// Walk `expr` and collect every single-name `Reference` whose name is not
/// bound inside the expression itself — i.e. the closure's free variables.
///
/// Captures are appended to `out` in first-reference order and deduplicated
/// via `seen`. The caller seeds `bound` with the closure's own parameter
/// names; nested lets and inner closures extend it locally during the walk.
#[expect(
    clippy::too_many_lines,
    reason = "exhaustive dispatch over every IrExpr variant — extracting arms would hide the structural walk"
)]
fn collect_free_refs(
    expr: &IrExpr,
    bound: &std::collections::HashSet<String>,
    out: &mut Vec<(String, ResolvedType)>,
    seen: &mut std::collections::HashSet<String>,
) {
    match expr {
        IrExpr::Reference { path, ty } => {
            if let [name] = path.as_slice() {
                if !bound.contains(name) && seen.insert(name.clone()) {
                    out.push((name.clone(), ty.clone()));
                }
            }
        }
        IrExpr::LetRef { name, ty } => {
            if !bound.contains(name) && seen.insert(name.clone()) {
                out.push((name.clone(), ty.clone()));
            }
        }
        IrExpr::Literal { .. } | IrExpr::SelfFieldRef { .. } => {}
        IrExpr::StructInst { fields, .. }
        | IrExpr::EnumInst { fields, .. }
        | IrExpr::Tuple { fields, .. } => {
            for (_, field_expr) in fields {
                collect_free_refs(field_expr, bound, out, seen);
            }
        }
        IrExpr::Array { elements, .. } => {
            for e in elements {
                collect_free_refs(e, bound, out, seen);
            }
        }
        IrExpr::FieldAccess { object, .. } => collect_free_refs(object, bound, out, seen),
        IrExpr::BinaryOp { left, right, .. } => {
            collect_free_refs(left, bound, out, seen);
            collect_free_refs(right, bound, out, seen);
        }
        IrExpr::UnaryOp { operand, .. } => collect_free_refs(operand, bound, out, seen),
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_free_refs(condition, bound, out, seen);
            collect_free_refs(then_branch, bound, out, seen);
            if let Some(e) = else_branch {
                collect_free_refs(e, bound, out, seen);
            }
        }
        IrExpr::For {
            var,
            collection,
            body,
            ..
        } => {
            collect_free_refs(collection, bound, out, seen);
            let mut inner = bound.clone();
            inner.insert(var.clone());
            collect_free_refs(body, &inner, out, seen);
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            collect_free_refs(scrutinee, bound, out, seen);
            for arm in arms {
                let mut inner = bound.clone();
                for (name, _) in &arm.bindings {
                    inner.insert(name.clone());
                }
                collect_free_refs(&arm.body, &inner, out, seen);
            }
        }
        IrExpr::FunctionCall { args, .. } => {
            for (_, a) in args {
                collect_free_refs(a, bound, out, seen);
            }
        }
        IrExpr::MethodCall { receiver, args, .. } => {
            collect_free_refs(receiver, bound, out, seen);
            for (_, a) in args {
                collect_free_refs(a, bound, out, seen);
            }
        }
        IrExpr::Closure {
            params, captures, ..
        } => {
            // Inner closure: its own captures are already computed relative to
            // its body. Any capture that is bound in the outer scope is not
            // free at this level; the rest bubble up as outer-closure captures.
            let inner_params: std::collections::HashSet<String> =
                params.iter().map(|(_, n, _)| n.clone()).collect();
            for (name, _, ty) in captures {
                if !inner_params.contains(name)
                    && !bound.contains(name)
                    && seen.insert(name.clone())
                {
                    out.push((name.clone(), ty.clone()));
                }
            }
        }
        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                collect_free_refs(k, bound, out, seen);
                collect_free_refs(v, bound, out, seen);
            }
        }
        IrExpr::DictAccess { dict, key, .. } => {
            collect_free_refs(dict, bound, out, seen);
            collect_free_refs(key, bound, out, seen);
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            let mut inner = bound.clone();
            for stmt in statements {
                match stmt {
                    IrBlockStatement::Let { name, value, .. } => {
                        collect_free_refs(value, &inner, out, seen);
                        inner.insert(name.clone());
                    }
                    IrBlockStatement::Assign { target, value } => {
                        collect_free_refs(target, &inner, out, seen);
                        collect_free_refs(value, &inner, out, seen);
                    }
                    IrBlockStatement::Expr(e) => collect_free_refs(e, &inner, out, seen),
                }
            }
            collect_free_refs(result, &inner, out, seen);
        }
    }
}
