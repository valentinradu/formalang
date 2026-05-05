//! Lowering for operator and call expressions: binary/unary ops, references,
//! method calls, plus shared helpers for inferring closure-typed argument
//! shapes.

use crate::ast::{BinaryOperator, Expr, PrimitiveType, UnaryOperator};
use crate::error::CompilerError;
use crate::ir::lower::IrLowerer;
use crate::ir::{IrExpr, ResolvedType};

impl IrLowerer<'_> {
    pub(super) fn lower_binary_op_expr(
        &mut self,
        left: &Expr,
        op: BinaryOperator,
        right: &Expr,
    ) -> IrExpr {
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
            BinaryOperator::Range => self
                .range_of(left_ir.ty().clone())
                .unwrap_or(ResolvedType::Error),
        };
        IrExpr::BinaryOp {
            left: Box::new(left_ir),
            op,
            right: Box::new(right_ir),
            ty,
            span: self.current_ir_span(),
        }
    }

    pub(super) fn lower_unary_op_expr(&mut self, op: UnaryOperator, operand: &Expr) -> IrExpr {
        let operand_ir = self.lower_expr(operand);
        let ty = match op {
            UnaryOperator::Not => ResolvedType::Primitive(PrimitiveType::Boolean),
            UnaryOperator::Neg => operand_ir.ty().clone(),
        };
        IrExpr::UnaryOp {
            op,
            operand: Box::new(operand_ir),
            ty,
            span: self.current_ir_span(),
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "single-pass reference resolution covers every binding shape (self, struct field, module-level let, local, function call); splitting would scatter the cases"
    )]
    pub(super) fn lower_reference(&mut self, path: &[crate::ast::Ident]) -> IrExpr {
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
                field_idx: crate::ir::FieldIdx(0),
                ty,
                span: self.current_ir_span(),
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
                    target: crate::ir::ReferenceTarget::Unresolved,
                    ty,
                    span: self.current_ir_span(),
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
            if let Some(let_type) = self
                .symbols
                .get_let_type(name)
                .map(crate::semantic::sem_type::SemType::display)
            {
                // prefer the simple-name resolution; fall
                // back to the value's known type for composite type
                // strings the helper can't reparse (closures, tuples,
                // arrays, etc.). The let was previously lowered, so its
                // resolved type is already cached on the IR side.
                if let Some(ty) = self.string_to_resolved_type(&let_type) {
                    return IrExpr::LetRef {
                        name: name.clone(),
                        binding_id: crate::ir::BindingId(0),
                        ty,
                        span: self.current_ir_span(),
                    };
                }
                let ty = self
                    .module
                    .lets
                    .iter()
                    .find(|l| l.name == *name)
                    .map_or_else(|| ResolvedType::Error, |l| l.value.ty().clone());
                return IrExpr::LetRef {
                    name: name.clone(),
                    binding_id: crate::ir::BindingId(0),
                    ty,
                    span: self.current_ir_span(),
                };
            }
        }

        // Resolve the root of the path. Try, in order:
        //   1. a local binding (function param, `self`, closure capture),
        //   2. a module-level `let` — needed for multi-segment paths like
        //      `sample.tags` where the single-segment LetRef branch above
        //      doesn't apply.
        // For a multi-segment path whose root is a local binding, emit a
        // chain of `FieldAccess` over a `LetRef` rather than keeping the
        // joined path on `IrExpr::Reference`. The resolve-references
        // pass only matches `Reference` paths against module-level
        // symbols, so a multi-segment `b.value` would otherwise surface
        // as `UndefinedReference("b::value")` even when `b` is a local.
        let root_is_local = path_strs
            .first()
            .is_some_and(|n| self.lookup_local_binding(n).is_some());
        if root_is_local && path_strs.len() > 1 {
            #[expect(
                clippy::indexing_slicing,
                reason = "first()/lookup_local_binding above guarantee at least one segment"
            )]
            let root_name = path_strs[0].clone();
            #[expect(
                clippy::expect_used,
                reason = "lookup_local_binding above proved this binding is in scope"
            )]
            let root_ty = self
                .lookup_local_binding(&root_name)
                .cloned()
                .expect("root local binding present");
            let mut current_expr = IrExpr::LetRef {
                name: root_name,
                binding_id: crate::ir::BindingId(0),
                ty: root_ty,
                span: self.current_ir_span(),
            };
            for seg in path_strs.iter().skip(1) {
                let field_ty = self.resolve_field_type(current_expr.ty(), seg);
                current_expr = IrExpr::FieldAccess {
                    object: Box::new(current_expr),
                    field: seg.clone(),
                    field_idx: crate::ir::FieldIdx(0),
                    ty: field_ty,
                    span: self.current_ir_span(),
                };
            }
            return current_expr;
        }
        let root = path_strs.first().and_then(|n| {
            self.lookup_local_binding(n).cloned().or_else(|| {
                self.module
                    .lets
                    .iter()
                    .find(|l| l.name == *n)
                    .map(|l| l.value.ty().clone())
            })
        });
        let ty = if let Some(root_ty) = root {
            let mut current = root_ty;
            for seg in path_strs.iter().skip(1) {
                current = self.resolve_field_type(&current, seg);
            }
            current
        } else {
            let span = path.first().map_or(self.current_span, |i| i.span);
            self.errors.push(CompilerError::UndefinedReference {
                name: path_strs.join("."),
                span,
            });
            ResolvedType::Error
        };
        IrExpr::Reference {
            path: path_strs,
            target: crate::ir::ReferenceTarget::Unresolved,
            ty,
            span: self.current_ir_span(),
        }
    }

    pub(super) fn lower_method_call(
        &mut self,
        receiver: &Expr,
        method_name: &str,
        args: &[(Option<crate::ast::Ident>, Expr)],
    ) -> IrExpr {
        let receiver_ir = self.lower_expr(receiver);
        // Closure-typed struct field: `f.onPress()` where the struct
        // declares `onPress: () -> E`. Lower as a closure call on the
        // field-access value, not as a method dispatch — there is no
        // impl method to call.
        if let Some(field_ty) = self.struct_field_closure_ty(receiver_ir.ty(), method_name) {
            if let ResolvedType::Closure {
                param_tys,
                return_ty,
            } = field_ty.clone()
            {
                let return_ty = (*return_ty).clone();
                let lowered_args: Vec<(Option<String>, IrExpr)> = args
                    .iter()
                    .enumerate()
                    .map(|(i, (label, expr))| {
                        let saved_closure = self.expected_closure_type.take();
                        self.expected_closure_type =
                            param_tys.get(i).map(|(_, t)| t.clone());
                        let lowered = self.lower_expr(expr);
                        self.expected_closure_type = saved_closure;
                        (label.as_ref().map(|l| l.name.clone()), lowered)
                    })
                    .collect();
                return IrExpr::CallClosure {
                    closure: Box::new(IrExpr::FieldAccess {
                        object: Box::new(receiver_ir),
                        field: method_name.to_string(),
                        field_idx: crate::ir::FieldIdx(0),
                        ty: field_ty,
                        span: self.current_ir_span(),
                    }),
                    args: lowered_args,
                    ty: return_ty,
                    span: self.current_ir_span(),
                };
            }
        }
        // same idea as the function-call path — pull the
        // method's expected param types so closure-literal arguments
        // get their `x` typed against what the method expects.
        let expected_param_tys = self.lookup_method_param_types(receiver_ir.ty(), method_name);
        let lowered_args: Vec<(Option<String>, IrExpr)> = args
            .iter()
            .enumerate()
            .map(|(i, (label, expr))| {
                let saved_closure = self.expected_closure_type.take();
                self.expected_closure_type =
                    Self::expected_arg_closure_ty(&expected_param_tys, i, label.as_ref());
                let lowered = self.lower_expr(expr);
                self.expected_closure_type = saved_closure;
                (label.as_ref().map(|l| l.name.clone()), lowered)
            })
            .collect();
        let ty = self.resolve_method_return_type(receiver_ir.ty(), method_name);
        let dispatch = self.resolve_dispatch_kind(receiver_ir.ty(), method_name);
        IrExpr::MethodCall {
            receiver: Box::new(receiver_ir),
            method: method_name.to_string(),
            method_idx: crate::ir::MethodIdx(0),
            args: lowered_args,
            dispatch,
            ty,
            span: self.current_ir_span(),
        }
    }

    /// If the receiver's type names a struct and that struct has a
    /// closure-typed field with the given name, return the field's
    /// resolved type. Used by `lower_method_call` to detect the
    /// `f.onPress()` (closure-field-invocation) pattern.
    fn struct_field_closure_ty(
        &self,
        receiver_ty: &ResolvedType,
        method_name: &str,
    ) -> Option<ResolvedType> {
        let struct_id = match receiver_ty {
            ResolvedType::Struct(id)
            | ResolvedType::Generic {
                base: crate::ir::GenericBase::Struct(id),
                ..
            } => *id,
            ResolvedType::Primitive(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Enum(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::Generic { .. }
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::Closure { .. }
            | ResolvedType::Error => return None,
        };
        let struct_def = self.module.get_struct(struct_id)?;
        for field in &struct_def.fields {
            if field.name == method_name {
                if matches!(field.ty, ResolvedType::Closure { .. }) {
                    return Some(field.ty.clone());
                }
                return None;
            }
        }
        None
    }

    /// find the IR function with the given name and
    /// return its parameter list as `(param_name, param_ty)` pairs. The
    /// caller uses the list to seed `expected_closure_type` for each
    /// argument before lowering. Returns an empty vec when the function
    /// isn't yet in the IR (forward reference) — in that case we fall
    /// back to `Unknown` for closure-literal params, same as before.
    pub(super) fn lookup_function_param_types(&self, fn_name: &str) -> Vec<(String, ResolvedType)> {
        // Match the same module-aware resolution `find_function_in_scope`
        // uses so a call from inside `mod foo { fn caller() { add(...) } }`
        // gets `foo::add`'s declared params (not a same-named top-level
        // function's, which lexical scoping says doesn't apply here).
        let f = if self.current_module_prefix.is_empty() {
            self.module.functions.iter().find(|f| f.name == fn_name)
        } else {
            let qualified = format!("{}::{}", self.current_module_prefix, fn_name);
            self.module
                .functions
                .iter()
                .find(|f| f.name == qualified)
                .or_else(|| self.module.functions.iter().find(|f| f.name == fn_name))
        };
        f.map(|f| {
            f.params
                .iter()
                .filter_map(|p| p.ty.as_ref().map(|t| (p.name.clone(), t.clone())))
                .collect()
        })
        .unwrap_or_default()
    }

    /// pick the expected parameter type for arg
    /// position `i`, preferring name match (for named args like
    /// `apply(callback: x -> x + 1)`) and falling back to positional
    /// index. Returns `Some(ty)` only when the matched parameter is a
    /// `Closure { .. }` — non-closure expected types don't influence
    /// closure-literal lowering.
    pub(super) fn expected_arg_closure_ty(
        expected: &[(String, ResolvedType)],
        i: usize,
        name: Option<&crate::ast::Ident>,
    ) -> Option<ResolvedType> {
        let candidate = name.map_or_else(
            || expected.get(i).map(|(_, t)| t.clone()),
            |n| {
                expected
                    .iter()
                    .find(|(pname, _)| pname == &n.name)
                    .map(|(_, t)| t.clone())
            },
        );
        candidate.filter(|t| matches!(t, ResolvedType::Closure { .. }))
    }

    /// locate the impl method matching
    /// `(receiver_ty, method_name)` and return its non-self parameter
    /// list as `(name, type)` pairs. The caller uses these to seed
    /// `expected_closure_type` for closure-literal arguments. Returns
    /// an empty vec when the method can't be resolved (forward
    /// reference, generic dispatch via trait, etc.) — in that case the
    /// arg-lowering falls back to `Unknown` for closure-literal params.
    pub(super) fn lookup_method_param_types(
        &self,
        receiver_ty: &ResolvedType,
        method_name: &str,
    ) -> Vec<(String, ResolvedType)> {
        let target = match receiver_ty {
            ResolvedType::Generic { base, .. } => match base {
                crate::ir::GenericBase::Struct(id) => Some(crate::ir::ImplTarget::Struct(*id)),
                crate::ir::GenericBase::Enum(id) => Some(crate::ir::ImplTarget::Enum(*id)),
                // A generic trait base can't be a method-call
                // receiver (FormaLang has no dynamic dispatch). Phase
                // E2 rejects trait values; this branch is here only
                // to keep the match exhaustive.
                crate::ir::GenericBase::Trait(_) => None,
            },
            ResolvedType::Struct(id) => Some(crate::ir::ImplTarget::Struct(*id)),
            ResolvedType::Enum(id) => Some(crate::ir::ImplTarget::Enum(*id)),
            ResolvedType::Primitive(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::Closure { .. }
            | ResolvedType::Error => None,
        };
        let Some(target) = target else {
            return Vec::new();
        };
        for impl_block in &self.module.impls {
            if impl_block.target != target {
                continue;
            }
            if let Some(func) = impl_block.functions.iter().find(|f| f.name == method_name) {
                return func
                    .params
                    .iter()
                    .filter(|p| p.name != "self")
                    .filter_map(|p| p.ty.as_ref().map(|t| (p.name.clone(), t.clone())))
                    .collect();
            }
        }
        Vec::new()
    }
}
