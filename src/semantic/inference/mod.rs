mod calls;
mod fields;

use super::module_resolver::ModuleResolver;
use super::sem_type::SemType;
use super::SemanticAnalyzer;
use crate::ast::{BinaryOperator, Expr, File, Literal, Statement, UnaryOperator};
use std::collections::HashMap;

use super::collect_bindings_from_pattern;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Walk a destructuring pattern alongside the value's inferred type
    /// and return each leaf binding paired with the type it should
    /// receive. Without this, `let {field as alias} = value` previously
    /// gave every alias the whole-struct type instead of the field's
    /// type; the same shortcoming bit array and tuple patterns.
    ///
    /// Falls back to the value's whole type for any leaf the walk can't
    /// resolve (rest patterns, unknown receivers, mismatched shapes), so
    /// destructurings that the type system can't unpack still produce
    /// usable bindings instead of "Unknown".
    pub(in crate::semantic) fn pattern_binding_types(
        &self,
        pattern: &crate::ast::BindingPattern,
        value_ty: &SemType,
        file: &File,
    ) -> Vec<(String, SemType)> {
        let mut out = Vec::new();
        self.walk_pattern_types(pattern, value_ty, file, &mut out);
        out
    }

    fn walk_pattern_types(
        &self,
        pattern: &crate::ast::BindingPattern,
        value_ty: &SemType,
        file: &File,
        out: &mut Vec<(String, SemType)>,
    ) {
        use crate::ast::{ArrayPatternElement, BindingPattern};
        match pattern {
            BindingPattern::Simple(ident) => {
                out.push((ident.name.clone(), value_ty.clone()));
            }
            BindingPattern::Struct { fields, .. } => {
                for f in fields {
                    let binding_name = f
                        .alias
                        .as_ref()
                        .map_or_else(|| f.name.name.clone(), |a| a.name.clone());
                    let field_ty = self.infer_field_type(value_ty, &f.name.name);
                    out.push((binding_name, field_ty));
                }
            }
            BindingPattern::Array { elements, .. } => {
                let element_ty = match value_ty.strip_optional() {
                    SemType::Array(inner) => *inner,
                    _ => SemType::Unknown,
                };
                for elem in elements {
                    match elem {
                        ArrayPatternElement::Binding(inner_pat) => {
                            self.walk_pattern_types(inner_pat, &element_ty, file, out);
                        }
                        ArrayPatternElement::Rest(Some(ident)) => {
                            out.push((
                                ident.name.clone(),
                                SemType::Array(Box::new(element_ty.clone())),
                            ));
                        }
                        ArrayPatternElement::Rest(None) | ArrayPatternElement::Wildcard => {}
                    }
                }
            }
            BindingPattern::Tuple { elements, .. } => {
                let tuple_fields = match value_ty.strip_optional() {
                    SemType::Tuple(fields) => fields,
                    _ => Vec::new(),
                };
                for (i, inner_pat) in elements.iter().enumerate() {
                    let elem_ty = tuple_fields
                        .get(i)
                        .map_or(SemType::Unknown, |(_, t)| t.clone());
                    self.walk_pattern_types(inner_pat, &elem_ty, file, out);
                }
            }
        }
    }
}

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Infer the type of an expression as a structural [`SemType`].
    #[expect(
        clippy::too_many_lines,
        reason = "dispatcher match over all Expr variants"
    )]
    pub(super) fn infer_type_sem(&self, expr: &Expr, file: &File) -> SemType {
        use crate::ast::PrimitiveType;
        match expr {
            Expr::Literal { value: lit, .. } => match lit {
                Literal::String(_) => SemType::Primitive(PrimitiveType::String),
                Literal::Number(n) => SemType::Primitive(n.primitive_type()),
                Literal::Boolean(_) => SemType::Primitive(PrimitiveType::Boolean),
                Literal::Regex { .. } => SemType::Primitive(PrimitiveType::Regex),
                Literal::Path(_) => SemType::Primitive(PrimitiveType::Path),
                Literal::Nil => SemType::Nil,
            },
            Expr::Array { elements, .. } => elements.first().map_or_else(
                || SemType::array_of(SemType::Unknown),
                |first| SemType::array_of(self.infer_type_sem(first, file)),
            ),
            Expr::Tuple { fields, .. } => SemType::Tuple(
                fields
                    .iter()
                    .map(|(name, expr)| (name.name.clone(), self.infer_type_sem(expr, file)))
                    .collect(),
            ),
            Expr::Invocation {
                path,
                type_args,
                args,
                ..
            } => self.infer_type_invocation(path, type_args, args, file),
            Expr::EnumInstantiation { enum_name, .. } => SemType::Named(enum_name.name.clone()),
            Expr::InferredEnumInstantiation { .. } => SemType::InferredEnum,
            Expr::Reference { path, .. } => self.infer_type_reference(path, file),
            Expr::BinaryOp { left, op, .. } => self.infer_type_binary_op(left, *op, file),
            Expr::UnaryOp { op, operand, .. } => match op {
                UnaryOperator::Neg => self.infer_type_sem(operand, file),
                UnaryOperator::Not => SemType::Primitive(PrimitiveType::Boolean),
            },
            Expr::ForExpr { body, .. } => SemType::array_of(self.infer_type_sem(body, file)),
            Expr::IfExpr {
                then_branch,
                else_branch,
                ..
            } => {
                let then_ty = self.infer_type_sem(then_branch, file);
                else_branch.as_ref().map_or_else(
                    || then_ty.clone(),
                    |else_expr| {
                        let else_ty = self.infer_type_sem(else_expr, file);
                        SemType::widen_branches(&then_ty, &else_ty)
                    },
                )
            }
            Expr::MatchExpr {
                scrutinee, arms, ..
            } => {
                // pre-populate each arm's pattern bindings into
                // an inference-scope frame so references inside the arm
                // body resolve to concrete types instead of "Unknown".
                let scrutinee_ty = self.infer_type_sem(scrutinee, file);
                let mut types: Vec<SemType> = Vec::with_capacity(arms.len());
                for arm in arms {
                    let frame = self.build_match_arm_scope_for_type(&scrutinee_ty, &arm.pattern);
                    self.inference_scope_stack.borrow_mut().push(frame);
                    types.push(self.infer_type_sem(&arm.body, file));
                    self.inference_scope_stack.borrow_mut().pop();
                }
                let Some(mut result) = types.pop() else {
                    return SemType::Unknown;
                };
                while let Some(next) = types.pop() {
                    result = SemType::widen_branches(&result, &next);
                }
                result
            }
            Expr::Group { expr, .. } => self.infer_type_sem(expr, file),
            Expr::DictLiteral { entries, .. } => {
                if let Some((first_key, first_value)) = entries.first() {
                    let key = self.infer_type_sem(first_key, file);
                    let value = self.infer_type_sem(first_value, file);
                    SemType::dictionary(key, value)
                } else {
                    SemType::dictionary(SemType::Unknown, SemType::Unknown)
                }
            }
            Expr::DictAccess { dict, .. } => {
                // Both array indexing (`xs[i]`) and dictionary lookup
                // (`d[k]`) yield an optional: the index can be out of
                // range, the key can be absent. Wrap in Optional so the
                // call site is forced to handle `nil`. SB-5 `s[i]` on a
                // String stays `I32` because the desugaring routes to
                // a method that returns a primitive byte.
                let receiver = self.infer_type_sem(dict, file);
                match receiver {
                    SemType::Dictionary { value, .. } => SemType::optional_of(*value),
                    SemType::Array(element) => SemType::optional_of(*element),
                    SemType::Primitive(crate::ast::PrimitiveType::String) => {
                        SemType::Primitive(crate::ast::PrimitiveType::I32)
                    }
                    SemType::Primitive(_)
                    | SemType::Named(_)
                    | SemType::Optional(_)
                    | SemType::Tuple(_)
                    | SemType::Generic { .. }
                    | SemType::Closure { .. }
                    | SemType::Unknown
                    | SemType::InferredEnum
                    | SemType::Nil => SemType::Unknown,
                }
            }
            Expr::FieldAccess { object, field, .. } => {
                let obj_type = self.infer_type_sem(object, file);
                self.infer_field_type(&obj_type, &field.name)
            }
            Expr::MethodCall {
                receiver, method, ..
            } => {
                let receiver_type = self.infer_type_sem(receiver, file);
                self.infer_method_return_type(&receiver_type, &method.name, file)
            }
            Expr::ClosureExpr {
                params,
                return_type,
                body,
                ..
            } => {
                // Push closure params into the inference-scope stack so
                // references inside the body resolve to their declared
                // types instead of "Unknown".
                let mut frame: HashMap<String, SemType> = HashMap::new();
                for p in params {
                    if let Some(ty) = &p.ty {
                        frame.insert(p.name.name.clone(), SemType::from_ast(ty));
                    }
                }
                self.inference_scope_stack.borrow_mut().push(frame);
                let inferred_body_type = self.infer_type_sem(body, file);
                self.inference_scope_stack.borrow_mut().pop();
                // prefer the explicit return type when present;
                // fall back to body inference otherwise.
                let return_ty = return_type
                    .as_ref()
                    .map_or(inferred_body_type, SemType::from_ast);
                let param_tys: Vec<SemType> = params
                    .iter()
                    .map(|p| p.ty.as_ref().map_or(SemType::Unknown, SemType::from_ast))
                    .collect();
                SemType::closure(param_tys, return_ty)
            }
            Expr::LetExpr { body, .. } => self.infer_type_sem(body, file),
            Expr::Block {
                statements, result, ..
            } => {
                // Push a frame and grow it statement-by-statement so each
                // let binding is visible to the inference of the next
                // statement's value. Without per-iteration growth, an
                // alias chain like `let r = c` loses track of `c`'s type
                // because `c` hasn't been pushed onto the stack yet at
                // the time `r`'s value is inferred.
                self.inference_scope_stack.borrow_mut().push(HashMap::new());
                for stmt in statements {
                    if let crate::ast::BlockStatement::Let {
                        pattern, ty, value, ..
                    } = stmt
                    {
                        let value_sem = ty.as_ref().map_or_else(
                            || self.infer_type_sem(value, file),
                            SemType::from_ast,
                        );
                        // Destructure-aware: each leaf binding picks up
                        // the type at its pattern position so that
                        // `let {field as alias} = value` exposes alias
                        // with the field's type, not the struct's.
                        for (binding_name, binding_sem) in
                            self.pattern_binding_types(pattern, &value_sem, file)
                        {
                            if let Some(top) =
                                self.inference_scope_stack.borrow_mut().last_mut()
                            {
                                top.insert(binding_name, binding_sem);
                            }
                        }
                    }
                }
                let out = self.infer_type_sem(result, file);
                self.inference_scope_stack.borrow_mut().pop();
                out
            }
        }
    }

    fn infer_type_reference(&self, path: &[crate::ast::Ident], _file: &File) -> SemType {
        let Some(first) = path.first() else {
            return SemType::Unknown;
        };

        // Consult the inference-scope stack first so pattern-introduced
        // bindings (match arms, etc.) resolve to their concrete types
        // instead of falling through to "Unknown".
        let scope_lookup = {
            let stack = self.inference_scope_stack.borrow();
            stack
                .iter()
                .rev()
                .find_map(|frame| frame.get(&first.name).cloned())
        };
        #[expect(
            clippy::option_if_let_else,
            reason = "five-branch resolution: if/else-if reads clearer than chained map_or_else"
        )]
        let root_type: SemType = if let Some(scope_ty) = scope_lookup {
            scope_ty
        } else if first.name == "self" {
            self.current_impl_struct
                .as_ref()
                .map_or(SemType::Unknown, |s| SemType::Named(s.clone()))
        } else if let Some(let_type) = self.symbols.get_let_type(&first.name) {
            let_type.clone()
        } else if let Some((local_type, _mutable)) = self.local_let_bindings.get(&first.name) {
            local_type.clone()
        } else if let Some(ref struct_name) = self.current_impl_struct {
            // Top-level field reference in an impl body — resolve against self.
            self.symbols
                .get_struct(struct_name)
                .map_or(SemType::Unknown, |struct_info| {
                    struct_info
                        .fields
                        .iter()
                        .find(|f| f.name == first.name)
                        .map_or(SemType::Unknown, |field| SemType::from_ast(&field.ty))
                })
        } else {
            SemType::Unknown
        };

        if path.len() == 1 {
            return root_type;
        }

        // Walk the field chain from the root type.
        let mut current = root_type;
        for seg in path.iter().skip(1) {
            current = self.infer_field_type(&current, &seg.name);
            if current.is_unknown() {
                return current;
            }
        }
        current
    }

    /// Infer the result type of a binary operator expression
    fn infer_type_binary_op(&self, left: &Expr, op: BinaryOperator, file: &File) -> SemType {
        use crate::ast::PrimitiveType;
        match op {
            BinaryOperator::Add
            | BinaryOperator::Sub
            | BinaryOperator::Mul
            | BinaryOperator::Div
            | BinaryOperator::Mod => self.infer_type_sem(left, file),
            BinaryOperator::Lt
            | BinaryOperator::Gt
            | BinaryOperator::Le
            | BinaryOperator::Ge
            | BinaryOperator::Eq
            | BinaryOperator::Ne
            | BinaryOperator::And
            | BinaryOperator::Or => SemType::Primitive(PrimitiveType::Boolean),
            BinaryOperator::Range => SemType::Generic {
                base: "Range".to_string(),
                args: vec![self.infer_type_sem(left, file)],
            },
        }
    }

    /// Check if an expression is mutable
    /// An expression is mutable if:
    /// - It's a reference to a mutable let binding
    /// - It's a field access where the entire chain is mutable (upward propagation)
    /// - It's a context access that was marked as mutable
    /// - It's an array element where the array is mutable
    #[expect(
        clippy::indexing_slicing,
        reason = "path[1..] is valid: path.len() >= 2 is guaranteed by the len==1 early return above"
    )]
    pub(super) fn is_expr_mutable(&self, expr: &Expr, file: &File) -> bool {
        match expr {
            // References can be mutable if they refer to mutable let bindings or fields
            Expr::Reference { path, .. } => {
                let Some(first) = path.first() else {
                    return false;
                };

                // Check if this is a reference to a let binding
                if path.len() == 1 {
                    return self.is_let_mutable(&first.name, file);
                }

                // For field access like `user.email`, check if:
                // 1. The root (user) is mutable
                // 2. The field (email) is mutable
                // Both must be true (upward propagation)
                let root_name = &first.name;
                let is_root_mutable = self.is_let_mutable(root_name, file);

                if !is_root_mutable {
                    return false;
                }

                // Check if all fields in the chain are mutable
                // For user.profile.email, we need: user is mut, profile field is mut, email field is mut
                self.is_field_chain_mutable(&first.name, &path[1..], file)
            }

            // Literals, arrays, tuples, invocations, binary/unary ops,
            // for/if/match/closure/method-call expressions produce new values — not mutable
            Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::Literal { .. }
            | Expr::Invocation { .. }
            | Expr::EnumInstantiation { .. }
            | Expr::InferredEnumInstantiation { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::DictLiteral { .. }
            | Expr::DictAccess { .. }
            | Expr::ClosureExpr { .. }
            | Expr::MethodCall { .. } => false,

            // Grouped expressions delegate to inner expression
            Expr::Group { expr, .. } => self.is_expr_mutable(expr, file),

            // Field access depends on the object
            Expr::FieldAccess { object, .. } => self.is_expr_mutable(object, file),

            // Let expressions delegate to their body
            Expr::LetExpr { body, .. } => self.is_expr_mutable(body, file),

            // Block expressions delegate to their result
            Expr::Block { result, .. } => self.is_expr_mutable(result, file),
        }
    }

    /// Check if a let binding is mutable
    pub(super) fn is_let_mutable(&self, name: &str, file: &File) -> bool {
        // First check local let bindings (function params, block lets)
        if let Some((_, mutable)) = self.local_let_bindings.get(name) {
            return *mutable;
        }

        // Then check file-level let bindings
        for statement in &file.statements {
            if let Statement::Let(let_binding) = statement {
                // Check if the name is in any binding from this pattern
                for binding in collect_bindings_from_pattern(&let_binding.pattern) {
                    if binding.name == name {
                        return let_binding.mutable;
                    }
                }
            }
        }
        false
    }

    /// Check if a field access chain is mutable.
    ///
    /// Field-level mutability was removed; mutability lives entirely on
    /// the binding (`let mut`). A chain is mutable iff the root binding
    /// is — the field path itself adds no further restriction.
    pub(super) fn is_field_chain_mutable(
        &self,
        _root_name: &str,
        _field_path: &[crate::ast::Ident],
        _file: &File,
    ) -> bool {
        true
    }

}
