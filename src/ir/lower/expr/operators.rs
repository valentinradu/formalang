//! Lowering for operator and call expressions: binary/unary ops, references,
//! method calls, plus shared helpers for inferring closure-typed argument
//! shapes.

use crate::ast::{BinaryOperator, Expr, PrimitiveType, UnaryOperator};
use crate::ir::lower::IrLowerer;
use crate::ir::monomorphise::specialise::substitute_type;
use crate::ir::{IrExpr, ResolvedType};
use std::collections::HashMap;

/// One parameter that a call argument can fill.
///
/// A labelled argument names the parameter by its external label or
/// by its name, as semantic analysis matches it. An unlabelled one
/// fills the parameter at its position, so each parameter keeps its
/// slot even when it has no type.
pub(in crate::ir::lower) struct ArgSlot {
    pub(in crate::ir::lower) name: String,
    pub(in crate::ir::lower) label: Option<String>,
    pub(in crate::ir::lower) ty: Option<ResolvedType>,
}

impl ArgSlot {
    /// The slot of an IR function parameter.
    pub(in crate::ir::lower) fn of_param(p: &crate::ir::IrFunctionParam) -> Self {
        Self {
            name: p.name.clone(),
            label: p.external_label.clone(),
            ty: p.ty.clone(),
        }
    }
}

/// A copy of `f` with no body: the parts a call site reads.
pub(in crate::ir::lower) fn signature_of(f: &crate::ir::IrFunction) -> crate::ir::IrFunction {
    crate::ir::IrFunction {
        name: f.name.clone(),
        visibility: f.visibility,
        generic_params: f.generic_params.clone(),
        params: f.params.clone(),
        return_type: f.return_type.clone(),
        body: None,
        extern_abi: f.extern_abi,
        attributes: f.attributes.clone(),
        doc: f.doc.clone(),
        span: f.span,
    }
}

impl IrLowerer<'_> {
    pub(super) fn lower_binary_op_expr(
        &mut self,
        left: &Expr,
        op: BinaryOperator,
        right: &Expr,
    ) -> IrExpr {
        // A leading-dot variant on one side of `==` or `!=` takes its
        // enum from the other side, as in `s == .a`.
        let (left_ir, right_ir) = if is_dot_variant(left) {
            let right_ir = self.lower_expr(right);
            let left_ir = self.lower_with_expected_value(left, Some(right_ir.ty()));
            (left_ir, right_ir)
        } else {
            let left_ir = self.lower_expr(left);
            let right_ir = if is_dot_variant(right) {
                self.lower_with_expected_value(right, Some(left_ir.ty()))
            } else {
                self.lower_expr(right)
            };
            (left_ir, right_ir)
        };
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

    /// The receiver of a static method call, `Counter` in
    /// `Counter.zero()`: a reference to the struct, with the struct as
    /// its type. A static method reads no value of it. The semantic
    /// pass gives a struct name this meaning also when a local binding
    /// has the same name.
    fn type_receiver(&self, receiver: &Expr) -> Option<IrExpr> {
        let Expr::Reference { path, span } = receiver else {
            return None;
        };
        let [name] = path.as_slice() else {
            return None;
        };
        let id = self.module.struct_id(&name.name)?;
        Some(IrExpr::Reference {
            path: vec![name.name.clone()],
            target: crate::ir::ReferenceTarget::Struct(id),
            ty: ResolvedType::Struct(id),
            span: self.ir_span(*span),
        })
    }

    pub(super) fn lower_method_call(
        &mut self,
        receiver: &Expr,
        method_name: &str,
        args: &[(Option<crate::ast::Ident>, Expr)],
    ) -> IrExpr {
        let receiver_ir = self
            .type_receiver(receiver)
            .unwrap_or_else(|| self.lower_expr(receiver));
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
                        let expected = param_tys.get(i).map(|(_, t)| t.clone());
                        let lowered = self.lower_with_expected_value(expr, expected.as_ref());
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
        let labels: Vec<Option<String>> = args
            .iter()
            .map(|(label, _)| label.as_ref().map(|l| l.name.clone()))
            .collect();
        let callee = self.method_callee(receiver_ir.ty(), method_name, &labels);
        // How the callee's declared types read at this call: the
        // receiver's type parameters take its type arguments, and the
        // method's own take fresh names. A receiver argument may name a
        // type parameter of the caller, and the caller's `U` must never
        // meet the method's `U`.
        let (view, fresh): (HashMap<String, ResolvedType>, Vec<String>) = callee
            .as_ref()
            .map_or_else(Default::default, |(f, receiver_subs)| {
                let mut view = receiver_subs.clone();
                let fresh: Vec<String> = f
                    .generic_params
                    .iter()
                    .map(|p| format!("{}'", p.name))
                    .collect();
                for (p, name) in f.generic_params.iter().zip(&fresh) {
                    view.insert(p.name.clone(), ResolvedType::TypeParam(name.clone()));
                }
                (view, fresh)
            });
        let expected_param_tys: Vec<ArgSlot> = callee.as_ref().map_or_else(Vec::new, |(f, _)| {
            f.params
                .iter()
                .filter(|p| p.name != "self")
                .map(|p| {
                    let mut slot = ArgSlot::of_param(p);
                    if let Some(ty) = &mut slot.ty {
                        substitute_type(ty, &view);
                    }
                    slot
                })
                .collect()
        });
        // A method with type parameters of its own takes their types
        // from the arguments, as a generic function does. The other
        // arguments lower first, so a closure argument for
        // `key: (T) -> K` gets `(I32) -> K`, and `K` binds to the type
        // that the closure body answers.
        let (lowered_args, mut method_subs) =
            self.lower_call_args(args, &expected_param_tys, !fresh.is_empty(), HashMap::new());
        method_subs.retain(|name, _| fresh.contains(name));
        let ty = match &callee {
            Some((f, _)) if !fresh.is_empty() => {
                let mut ty = f
                    .return_type
                    .clone()
                    .unwrap_or(ResolvedType::Primitive(PrimitiveType::Never));
                substitute_type(&mut ty, &view);
                substitute_type(&mut ty, &method_subs);
                ty
            }
            Some(_) | None => {
                self.resolve_method_return_type(receiver_ir.ty(), method_name, &lowered_args)
            }
        };
        let dispatch = self.resolve_dispatch_kind(receiver_ir.ty(), method_name);
        // Lowering knows which method the call means, so it says so
        // here rather than leaving a zero for `ResolveReferencesPass`
        // to correct. A module read before that pass runs — the
        // reference interpreter does, and so does every caller of
        // `compile_to_ir` — would otherwise see every call pointing at
        // the first method of its name, which is only right when the
        // name is not overloaded.
        let method_idx = self.method_index(&dispatch, method_name, &lowered_args);
        IrExpr::MethodCall {
            receiver: Box::new(receiver_ir),
            method: method_name.to_string(),
            method_idx,
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
            // A tuple names its fields too: `t.f(1)` calls the closure
            // in the field `f`.
            ResolvedType::Tuple(fields) => {
                return fields
                    .iter()
                    .find(|(name, ty)| {
                        name == method_name && matches!(ty, ResolvedType::Closure { .. })
                    })
                    .map(|(_, ty)| ty.clone());
            }
            ResolvedType::Primitive(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Enum(_)
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
    pub(super) fn lookup_function_param_types(&self, fn_name: &str) -> Vec<ArgSlot> {
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
        f.map(|f| f.params.iter().map(ArgSlot::of_param).collect())
            .unwrap_or_default()
    }

    /// The signature of the free function that a call means, when the
    /// call comes before the function in the file: the function is in
    /// `declared_functions` but not yet in `module.functions`. The
    /// module rule and the overload rule are the ones that
    /// `find_overload_in_scope` applies.
    pub(super) fn declared_callee(
        &self,
        fn_name: &str,
        arg_labels: &[Option<String>],
        arg_count: usize,
    ) -> Option<crate::ir::IrFunction> {
        let qualified = (!self.current_module_prefix.is_empty())
            .then(|| format!("{}::{}", self.current_module_prefix, fn_name));
        let mut candidates: Vec<&crate::ir::IrFunction> = self
            .declared_functions
            .iter()
            .filter(|f| qualified.as_deref() == Some(f.name.as_str()) || f.name == fn_name)
            .collect();
        if let Some(prefixed) = qualified.as_deref() {
            if candidates.iter().any(|f| f.name == prefixed) {
                candidates.retain(|f| f.name == prefixed);
            }
        }
        if candidates.len() > 1 {
            if let Some(chosen) = self
                .symbols
                .overload_choice(self.current_span, fn_name)
                .and_then(|index| candidates.get(index))
            {
                return Some(signature_of(chosen));
            }
        }
        let index = crate::ir::overload::choose(
            candidates.iter().copied().enumerate(),
            |f| f.params.as_slice(),
            arg_labels,
            arg_count,
        )?;
        candidates.get(index).map(|f| signature_of(f))
    }

    /// pick the expected parameter type for arg
    /// position `i`, preferring name match (for named args like
    /// `apply(callback: x -> x + 1)`) and falling back to positional
    /// index. Returns `Some(ty)` only when the matched parameter is a
    /// `Closure { .. }` — non-closure expected types don't influence
    /// closure-literal lowering.
    pub(super) fn expected_arg_ty(
        expected: &[ArgSlot],
        i: usize,
        name: Option<&crate::ast::Ident>,
    ) -> Option<ResolvedType> {
        let slot = name.map_or_else(
            || expected.get(i),
            |n| {
                expected
                    .iter()
                    .find(|s| s.label.as_deref() == Some(n.name.as_str()) || s.name == n.name)
            },
        );
        slot.and_then(|s| s.ty.clone())
    }
}

/// True when `expr` is a leading-dot variant, maybe in parentheses.
fn is_dot_variant(expr: &Expr) -> bool {
    if let Expr::Group { expr, .. } = expr {
        return is_dot_variant(expr);
    }
    matches!(expr, Expr::InferredEnumInstantiation { .. })
}
