mod calls;
mod fields;

use super::module_resolver::ModuleResolver;
use super::sem_type::SemType;
use super::SemanticAnalyzer;
use crate::ast::{BinaryOperator, Definition, Expr, File, Literal, Statement, UnaryOperator};
use std::collections::HashMap;

use super::collect_bindings_from_pattern;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Infer the type of an expression and return the legacy
    /// string format. Thin bridge over [`Self::infer_type_sem`] —
    /// callers in `validation.rs` still consume strings; step 4 will
    /// migrate them and this wrapper goes away.
    pub(super) fn infer_type(&self, expr: &Expr, file: &File) -> String {
        self.infer_type_sem(expr, file).display()
    }

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
                let scrutinee_str = scrutinee_ty.display();
                let enum_name = scrutinee_str.trim_end_matches('?');
                let mut types: Vec<SemType> = Vec::with_capacity(arms.len());
                for arm in arms {
                    let frame = self.build_match_arm_scope(enum_name, &arm.pattern);
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
                // extract V from a Dictionary shape.
                // Structural unpacking — no string scanning needed.
                if let SemType::Dictionary { value, .. } = self.infer_type_sem(dict, file) {
                    *value
                } else {
                    SemType::Unknown
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
                let mut frame = HashMap::new();
                for p in params {
                    if let Some(ty) = &p.ty {
                        frame.insert(p.name.name.clone(), Self::type_to_string(ty));
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
                // walk the block's statements to push each
                // let binding into the inference-scope stack before
                // inferring the trailing expression. Without this the
                // function-return-type check below loses sight of any
                // bindings created inside a block body (validation
                // tears them down before the post-body infer_type call).
                let mut frame = HashMap::new();
                for stmt in statements {
                    if let crate::ast::BlockStatement::Let {
                        pattern, ty, value, ..
                    } = stmt
                    {
                        let value_ty = ty.as_ref().map_or_else(
                            || self.infer_type_sem(value, file).display(),
                            Self::type_to_string,
                        );
                        if let crate::ast::BindingPattern::Simple(ident) = pattern {
                            frame.insert(ident.name.clone(), value_ty);
                        }
                    }
                }
                self.inference_scope_stack.borrow_mut().push(frame);
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
            SemType::from_legacy_string(&scope_ty)
        } else if first.name == "self" {
            self.current_impl_struct
                .as_ref()
                .map_or(SemType::Unknown, |s| SemType::Named(s.clone()))
        } else if let Some(let_type) = self.symbols.get_let_type(&first.name) {
            SemType::from_legacy_string(let_type)
        } else if let Some((local_type, _mutable)) = self.local_let_bindings.get(&first.name) {
            SemType::from_legacy_string(local_type)
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

    /// Check if a field access chain is mutable
    /// For path like `["profile", "email"]`, check that both profile and email fields are mutable
    pub(super) fn is_field_chain_mutable(
        &self,
        root_name: &str,
        field_path: &[crate::ast::Ident],
        file: &File,
    ) -> bool {
        if field_path.is_empty() {
            return true;
        }

        // Get the type of the root to find which struct it refers to
        let root_type = self.get_let_type(root_name, file);

        // Check each field in the chain
        let mut current_type = root_type;
        for field_ident in field_path {
            // Check if the current field is mutable in its type
            if !Self::is_struct_field_mutable(&current_type, &field_ident.name, file) {
                return false;
            }

            // Get the type of this field to continue checking the chain
            current_type = Self::get_field_type(&current_type, &field_ident.name, file).display();
        }

        true
    }

    /// Get the type of a let binding
    pub(super) fn get_let_type(&self, name: &str, file: &File) -> String {
        for statement in &file.statements {
            if let Statement::Let(let_binding) = statement {
                // Check if the name is in any binding from this pattern
                for binding in collect_bindings_from_pattern(&let_binding.pattern) {
                    if binding.name == name {
                        return self.infer_type(&let_binding.value, file);
                    }
                }
            }
        }
        "Unknown".to_string()
    }

    /// Check if a struct field is mutable
    pub(super) fn is_struct_field_mutable(type_name: &str, field_name: &str, file: &File) -> bool {
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                if let Definition::Struct(struct_def) = &**def {
                    if struct_def.name.name == type_name {
                        for field in &struct_def.fields {
                            if field.name.name == field_name {
                                return field.mutable;
                            }
                        }
                    }
                }
            }
        }
        false
    }
    pub(super) fn get_field_type(type_name: &str, field_name: &str, file: &File) -> SemType {
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                if let Definition::Struct(struct_def) = &**def {
                    if struct_def.name.name == type_name {
                        for field in &struct_def.fields {
                            if field.name.name == field_name {
                                return SemType::from_ast(&field.ty);
                            }
                        }
                    }
                }
            }
        }
        SemType::Unknown
    }
}
