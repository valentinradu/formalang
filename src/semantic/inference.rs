use super::module_resolver::ModuleResolver;
use super::SemanticAnalyzer;
use crate::ast::{BinaryOperator, Definition, Expr, File, Literal, Statement, UnaryOperator};

use super::collect_bindings_from_pattern;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Infer the type of an expression (simplified type inference)
    pub(super) fn infer_type(&self, expr: &Expr, file: &File) -> String {
        match expr {
            Expr::Literal(lit) => match lit {
                Literal::String(_) => "String".to_string(),
                Literal::Number(_) => "Number".to_string(),
                Literal::Boolean(_) => "Boolean".to_string(),
                Literal::Regex { .. } => "Regex".to_string(),
                Literal::Path(_) => "Path".to_string(),
                Literal::Nil => "Nil".to_string(),
            },
            Expr::Array { elements, .. } => elements.first().map_or_else(
                || "[Unknown]".to_string(),
                |first| format!("[{}]", self.infer_type(first, file)),
            ),
            Expr::Tuple { fields, .. } => {
                let field_types: Vec<String> = fields
                    .iter()
                    .map(|(name, expr)| format!("{}: {}", name.name, self.infer_type(expr, file)))
                    .collect();
                format!("({})", field_types.join(", "))
            }
            Expr::Invocation {
                path,
                type_args,
                args,
                ..
            } => self.infer_type_invocation(path, type_args, args, file),
            Expr::EnumInstantiation { enum_name, .. } => enum_name.name.clone(),
            Expr::InferredEnumInstantiation { .. } => "InferredEnum".to_string(),
            Expr::Reference { path, .. } => self.infer_type_reference(path, file),
            Expr::BinaryOp { left, op, .. } => self.infer_type_binary_op(left, *op, file),
            Expr::UnaryOp { op, operand, .. } => match op {
                UnaryOperator::Neg => self.infer_type(operand, file),
                UnaryOperator::Not => "Boolean".to_string(),
            },
            Expr::ForExpr { body, .. } => format!("[{}]", self.infer_type(body, file)),
            Expr::IfExpr { then_branch, .. } => self.infer_type(then_branch, file),
            Expr::MatchExpr { arms, .. } => arms.first().map_or_else(
                || "Unknown".to_string(),
                |arm| self.infer_type(&arm.body, file),
            ),
            Expr::Group { expr, .. } => self.infer_type(expr, file),
            Expr::DictLiteral { entries, .. } => {
                if let Some((first_key, first_value)) = entries.first() {
                    let key_type = self.infer_type(first_key, file);
                    let value_type = self.infer_type(first_value, file);
                    format!("[{key_type}: {value_type}]")
                } else {
                    "[Unknown: Unknown]".to_string()
                }
            }
            Expr::DictAccess { dict, .. } => {
                // Gap 3: Infer value type from dict type "[K: V]"
                let dict_type = self.infer_type(dict, file);
                if let Some(inner) = dict_type
                    .strip_prefix('[')
                    .and_then(|s| s.strip_suffix(']'))
                    .filter(|s| s.contains(": "))
                {
                    if let Some(colon_pos) = inner.find(": ") {
                        if let Some(after) = inner.get(colon_pos.saturating_add(2)..) {
                            return after.to_string();
                        }
                    }
                }
                "Unknown".to_string()
            }
            Expr::FieldAccess { .. } | Expr::MethodCall { .. } => "Unknown".to_string(),
            Expr::ClosureExpr { params, body, .. } => {
                let body_type = self.infer_type(body, file);
                if params.is_empty() {
                    format!("() -> {body_type}")
                } else if params.len() == 1 {
                    let param_type = params[0]
                        .ty
                        .as_ref()
                        .map_or_else(|| "Unknown".to_string(), Self::type_to_string);
                    format!("{param_type} -> {body_type}")
                } else {
                    let param_types: Vec<String> = params
                        .iter()
                        .map(|p| {
                            p.ty.as_ref()
                                .map_or_else(|| "Unknown".to_string(), Self::type_to_string)
                        })
                        .collect();
                    format!("{} -> {body_type}", param_types.join(", "))
                }
            }
            Expr::LetExpr { body, .. } => self.infer_type(body, file),
            Expr::Block { result, .. } => self.infer_type(result, file),
        }
    }

    /// Infer the type of an invocation expression (struct instantiation or function call)
    fn infer_type_invocation(
        &self,
        path: &[crate::ast::Ident],
        type_args: &[crate::ast::Type],
        _args: &[(Option<crate::ast::Ident>, Expr)],
        _file: &File,
    ) -> String {
        let name = path
            .iter()
            .map(|id| id.name.as_str())
            .collect::<Vec<_>>()
            .join("::");

        if self.symbols.is_struct(&name) {
            // Struct instantiation — return the struct type, with generic args if present
            if type_args.is_empty() {
                name
            } else {
                let arg_types: Vec<String> = type_args
                    .iter()
                    .map(|ty| Self::type_to_string(ty))
                    .collect();
                format!("{}<{}>", name, arg_types.join(", "))
            }
        } else if let Some(func_info) = self.symbols.get_function(&name) {
            // User-defined standalone function — return its declared return type
            func_info
                .return_type
                .as_ref()
                .map_or_else(|| "nil".to_string(), |ty| Self::type_to_string(ty))
        } else {
            "Unknown".to_string()
        }
    }

    /// Infer the type of a reference expression
    fn infer_type_reference(&self, path: &[crate::ast::Ident], _file: &File) -> String {
        // Handle self.field references
        if path.first().is_some_and(|p| p.name == "self") {
            if let Some(field_ident) = path.get(1).filter(|_| path.len() == 2) {
                let field_name = &field_ident.name;
                if let Some(ref struct_name) = self.current_impl_struct {
                    if let Some(struct_info) = self.symbols.get_struct(struct_name) {
                        for field in &struct_info.fields {
                            if field.name == *field_name {
                                return Self::type_to_string(&field.ty);
                            }
                        }
                    }
                }
            }
            return "Unknown".to_string();
        }

        // Simple single-segment reference
        if let Some(first) = path.first().filter(|_| path.len() == 1) {
            let name = &first.name;
            if let Some(let_type) = self.symbols.get_let_type(name) {
                return let_type.to_string();
            }
            if let Some((local_type, _mutable)) = self.local_let_bindings.get(name) {
                return local_type.clone();
            }
            if let Some(ref struct_name) = self.current_impl_struct {
                if let Some(struct_info) = self.symbols.get_struct(struct_name) {
                    for field in &struct_info.fields {
                        if field.name == *name {
                            return Self::type_to_string(&field.ty);
                        }
                    }
                }
            }
        }
        // Multi-segment path or unresolved — cannot determine type statically
        "Unknown".to_string()
    }

    /// Infer the result type of a binary operator expression
    fn infer_type_binary_op(&self, left: &Expr, op: BinaryOperator, file: &File) -> String {
        match op {
            BinaryOperator::Add
            | BinaryOperator::Sub
            | BinaryOperator::Mul
            | BinaryOperator::Div
            | BinaryOperator::Mod => self.infer_type(left, file),
            BinaryOperator::Lt
            | BinaryOperator::Gt
            | BinaryOperator::Le
            | BinaryOperator::Ge
            | BinaryOperator::Eq
            | BinaryOperator::Ne
            | BinaryOperator::And
            | BinaryOperator::Or => "Boolean".to_string(),
            BinaryOperator::Range => format!("Range<{}>", self.infer_type(left, file)),
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
            | Expr::Literal(_)
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
            current_type = Self::get_field_type(&current_type, &field_ident.name, file);
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

    /// Get the type of a struct field
    pub(super) fn get_field_type(type_name: &str, field_name: &str, file: &File) -> String {
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                if let Definition::Struct(struct_def) = &**def {
                    if struct_def.name.name == type_name {
                        for field in &struct_def.fields {
                            if field.name.name == field_name {
                                return Self::type_to_string(&field.ty);
                            }
                        }
                    }
                }
            }
        }
        "Unknown".to_string()
    }
}
