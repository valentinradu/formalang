use super::module_resolver::ModuleResolver;
use super::SemanticAnalyzer;
use crate::ast::{
    BinaryOperator, BindingPattern, BlockStatement, Definition, Expr, File, PrimitiveType,
    Statement, StructDef, Type,
};
use crate::error::CompilerError;
use crate::location::Span;
use std::collections::HashSet;

use super::collect_bindings_from_pattern;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Pass 3: Validate expressions
    /// Validate operators and control flow without evaluation
    pub(super) fn validate_expressions(&mut self, file: &File) {
        for statement in &file.statements {
            match statement {
                Statement::Let(let_binding) => {
                    self.validate_expr(&let_binding.value, file);
                    // Validate destructuring pattern type compatibility
                    self.validate_destructuring_pattern(
                        &let_binding.pattern,
                        &let_binding.value,
                        let_binding.span,
                        file,
                    );
                }
                Statement::Definition(def) => match &**def {
                    Definition::Struct(struct_def) => {
                        self.validate_struct_expressions(struct_def, file);
                    }
                    Definition::Impl(impl_def) => {
                        // Set current impl struct for field type resolution
                        self.current_impl_struct = Some(impl_def.name.name.clone());
                        // Clear local let bindings for this impl block
                        self.local_let_bindings.clear();
                        // Clear impl struct context and local bindings
                        self.current_impl_struct = None;
                        self.local_let_bindings.clear();
                    }
                    Definition::Trait(_)
                    | Definition::Enum(_)
                    | Definition::Module(_)
                    | Definition::Function(_) => {}
                },
                Statement::Use(_) => {}
            }
        }
    }

    /// Validate expressions in struct field defaults
    pub(super) fn validate_struct_expressions(&mut self, struct_def: &StructDef, file: &File) {
        // Validate field defaults
        for field in &struct_def.fields {
            if let Some(default_expr) = &field.default {
                self.validate_expr(default_expr, file);
            }
        }
        // Validate mount field defaults
        for field in &struct_def.mount_fields {
            if let Some(default_expr) = &field.default {
                self.validate_expr(default_expr, file);
            }
        }
    }

    /// Validate a single expression (recursively)
    #[expect(clippy::too_many_lines, reason = "dispatcher match over 18+ Expr variants; each arm is a single call")]
    pub(super) fn validate_expr(&mut self, expr: &Expr, file: &File) {
        // Check recursion depth to prevent stack overflow
        const MAX_EXPR_DEPTH: usize = 500;
        self.validate_expr_depth = self.validate_expr_depth.saturating_add(1);
        if self.validate_expr_depth > MAX_EXPR_DEPTH {
            self.validate_expr_depth = self.validate_expr_depth.saturating_sub(1);
            self.errors
                .push(CompilerError::ExpressionDepthExceeded { span: expr.span() });
            return;
        }

        match expr {
            Expr::Literal(_) => {}
            Expr::Array { elements, .. } => {
                for elem in elements {
                    self.validate_expr(elem, file);
                }
            }
            Expr::Tuple { fields, .. } => {
                for (_, field_expr) in fields {
                    self.validate_expr(field_expr, file);
                }
            }
            Expr::Reference { path, span } => {
                self.validate_expr_reference(path, *span, file);
            }
            Expr::Invocation {
                path,
                type_args,
                args,
                mounts,
                span,
            } => {
                self.validate_expr_invocation(path, type_args, args, mounts, *span, file);
            }
            Expr::EnumInstantiation {
                enum_name,
                variant,
                data,
                span,
            } => {
                for (_, data_expr) in data {
                    self.validate_expr(data_expr, file);
                }
                self.validate_enum_instantiation(enum_name, variant, data, *span, file);
            }
            Expr::InferredEnumInstantiation { data, .. } => {
                for (_, data_expr) in data {
                    self.validate_expr(data_expr, file);
                }
            }
            Expr::BinaryOp { left, op, right, span } => {
                self.validate_expr(left, file);
                self.validate_expr(right, file);
                self.validate_binary_op(left, *op, right, *span, file);
            }
            Expr::UnaryOp { operand, .. } => {
                self.validate_expr(operand, file);
            }
            Expr::ForExpr { var, collection, body, span } => {
                self.validate_expr(collection, file);
                let mut scope = HashSet::new();
                scope.insert(var.name.clone());
                self.loop_var_scopes.push(scope);
                self.validate_expr(body, file);
                self.loop_var_scopes.pop();
                self.validate_for_loop(collection, *span, file);
            }
            Expr::IfExpr { condition, then_branch, else_branch, span } => {
                self.validate_expr(condition, file);
                self.validate_expr(then_branch, file);
                if let Some(else_expr) = else_branch {
                    self.validate_expr(else_expr, file);
                }
                self.validate_if_condition(condition, *span, file);
            }
            Expr::MatchExpr { scrutinee, arms, span } => {
                self.validate_expr(scrutinee, file);
                for arm in arms {
                    self.validate_expr(&arm.body, file);
                }
                self.validate_match(scrutinee, arms, *span, file);
            }
            Expr::Group { expr, .. } => self.validate_expr(expr, file),
            Expr::DictLiteral { entries, .. } => {
                for (key, value) in entries {
                    self.validate_expr(key, file);
                    self.validate_expr(value, file);
                }
            }
            Expr::DictAccess { dict, key, .. } => {
                self.validate_expr(dict, file);
                self.validate_expr(key, file);
            }
            Expr::FieldAccess { object, .. } => self.validate_expr(object, file),
            Expr::ClosureExpr { params, body, .. } => {
                self.validate_expr_closure(params, body, file);
            }
            Expr::LetExpr { .. } => {
                self.validate_expr_let(expr, file);
            }
            Expr::MethodCall { receiver, method, args, span } => {
                self.validate_expr_method_call(receiver, method, args, *span, file);
            }
            Expr::Block { statements, result, .. } => {
                self.validate_expr_block(statements, result, file);
            }
        }

        self.validate_expr_depth = self.validate_expr_depth.saturating_sub(1);
    }

    /// Validate a reference expression (path lookup)
    fn validate_expr_reference(
        &mut self,
        path: &[crate::ast::Ident],
        span: Span,
        _file: &File,
    ) {
        if path.first().is_some_and(|p| p.name == "self") {
            if self.current_impl_struct.is_none() {
                self.errors.push(CompilerError::UndefinedReference {
                    name: "self".to_string(),
                    span,
                });
                return;
            }
            if path.len() == 1 {
                return;
            }
            if let Some(field_ident) = path.get(1).filter(|_| path.len() == 2) {
                let field_name = &field_ident.name;
                if let Some(ref struct_name) = self.current_impl_struct {
                    if let Some(struct_info) = self.symbols.get_struct(struct_name) {
                        for field in &struct_info.fields {
                            if field.name == *field_name {
                                return;
                            }
                        }
                        for field in &struct_info.mount_fields {
                            if field.name == *field_name {
                                return;
                            }
                        }
                        self.errors.push(CompilerError::UndefinedReference {
                            name: format!("self.{field_name}"),
                            span,
                        });
                        return;
                    }
                }
            }
            return;
        }

        if let Some(first) = path.first().filter(|_| path.len() == 1) {
            let name = &first.name;
            if self.symbols.is_let(name) {
                return;
            }
            if self.local_let_bindings.contains_key(name) {
                return;
            }
            for scope in &self.loop_var_scopes {
                if scope.contains(name) {
                    return;
                }
            }
            for scope in &self.closure_param_scopes {
                if scope.contains(name) {
                    return;
                }
            }
            if self.symbols.is_struct(name)
                || self.symbols.is_enum(name)
                || self.symbols.is_trait(name)
            {
                return;
            }
            if let Some(ref struct_name) = self.current_impl_struct {
                if let Some(struct_info) = self.symbols.get_struct(struct_name) {
                    for field in &struct_info.fields {
                        if field.name == *name {
                            return;
                        }
                    }
                    for field in &struct_info.mount_fields {
                        if field.name == *name {
                            return;
                        }
                    }
                }
                self.errors.push(CompilerError::UndefinedReference {
                    name: name.clone(),
                    span,
                });
            }
        }
    }

    /// Validate an invocation expression (struct instantiation or function call)
    fn validate_expr_invocation(
        &mut self,
        path: &[crate::ast::Ident],
        type_args: &[crate::ast::Type],
        args: &[(Option<crate::ast::Ident>, Expr)],
        mounts: &[(crate::ast::Ident, Expr)],
        span: Span,
        file: &File,
    ) {
        let name = path
            .iter()
            .map(|id| id.name.as_str())
            .collect::<Vec<_>>()
            .join("::");

        for (_, arg_expr) in args {
            self.validate_expr(arg_expr, file);
        }
        for (_, mount_expr) in mounts {
            self.validate_expr(mount_expr, file);
        }
        for type_arg in type_args {
            self.validate_type(type_arg);
        }

        let is_struct = self.symbols.get_struct_qualified(&name).is_some();
        if is_struct {
            self.validate_expr_invocation_struct(&name, type_args, args, mounts, span, file);
        } else {
            self.validate_expr_invocation_function(&name, type_args, mounts, span);
        }
    }

    /// Validate a struct instantiation invocation
    fn validate_expr_invocation_struct(
        &mut self,
        name: &str,
        type_args: &[crate::ast::Type],
        args: &[(Option<crate::ast::Ident>, Expr)],
        mounts: &[(crate::ast::Ident, Expr)],
        span: Span,
        file: &File,
    ) {
        let named_args: Vec<(crate::ast::Ident, Expr)> = args
            .iter()
            .filter_map(|(name_opt, expr)| {
                name_opt.as_ref().map(|n| (n.clone(), expr.clone()))
            })
            .collect();

        for (i, (name_opt, arg_expr)) in args.iter().enumerate() {
            if name_opt.is_none() {
                self.errors.push(CompilerError::PositionalArgInStruct {
                    struct_name: name.to_string(),
                    position: i.saturating_add(1),
                    span: arg_expr.span(),
                });
            }
        }

        if let Some(expected_params) = self.symbols.get_generics(name) {
            let expected = expected_params.len();
            let actual = type_args.len();
            if expected != actual {
                if actual == 0 && expected > 0 {
                    self.errors.push(CompilerError::MissingGenericArguments {
                        name: name.to_string(),
                        span,
                    });
                } else {
                    self.errors.push(CompilerError::GenericArityMismatch {
                        name: name.to_string(),
                        expected,
                        actual,
                        span,
                    });
                }
            }
        } else if !type_args.is_empty() {
            self.errors.push(CompilerError::GenericArityMismatch {
                name: name.to_string(),
                expected: 0,
                actual: type_args.len(),
                span,
            });
        }

        self.validate_struct_fields(name, &named_args, mounts, span, file);
        self.validate_struct_mutability(name, &named_args, mounts, file, span);
    }

    /// Validate a function call invocation
    fn validate_expr_invocation_function(
        &mut self,
        name: &str,
        type_args: &[crate::ast::Type],
        mounts: &[(crate::ast::Ident, Expr)],
        span: Span,
    ) {
        if !type_args.is_empty() {
            self.errors.push(CompilerError::GenericArityMismatch {
                name: name.to_string(),
                expected: 0,
                actual: type_args.len(),
                span,
            });
        }
        if let Some(first_mount) = mounts.first() {
            self.errors.push(CompilerError::UnknownMount {
                mount: first_mount.0.name.clone(),
                struct_name: name.to_string(),
                span: first_mount.0.span,
            });
        }

        let simple_name = name.rsplit("::").next().unwrap_or(name);
        let is_builtin = crate::builtins::BuiltinRegistry::global().is_builtin(simple_name);
        let is_user_function = self.symbols.get_function(name).is_some()
            || self.symbols.get_function(simple_name).is_some();

        if !is_builtin && !is_user_function {
            self.errors.push(CompilerError::UndefinedType {
                name: format!("function '{name}'"),
                span,
            });
        }
    }

    /// Validate a closure expression
    fn validate_expr_closure(
        &mut self,
        params: &[crate::ast::ClosureParam],
        body: &Expr,
        file: &File,
    ) {
        for param in params {
            if let Some(ty) = &param.ty {
                self.validate_type(ty);
            }
        }
        let mut param_scope = HashSet::new();
        for param in params {
            param_scope.insert(param.name.name.clone());
        }
        self.closure_param_scopes.push(param_scope);
        self.validate_expr(body, file);
        self.closure_param_scopes.pop();
    }

    /// Validate a let expression
    fn validate_expr_let(&mut self, expr: &Expr, file: &File) {
        let Expr::LetExpr { mutable, pattern, ty, value, body, span } = expr else {
            return;
        };
        if let Some(type_ann) = ty {
            self.validate_type(type_ann);
        }
        self.validate_expr(value, file);
        self.validate_destructuring_pattern(pattern, value, *span, file);
        for binding in collect_bindings_from_pattern(pattern) {
            let inferred_ty = self.infer_type(value, file);
            self.local_let_bindings.insert(binding.name, (inferred_ty, *mutable));
        }
        self.validate_expr(body, file);
    }

    /// Validate a method call expression
    fn validate_expr_method_call(
        &mut self,
        receiver: &Expr,
        method: &crate::ast::Ident,
        args: &[Expr],
        span: Span,
        file: &File,
    ) {
        self.validate_expr(receiver, file);
        for arg in args {
            self.validate_expr(arg, file);
        }
        let receiver_type = self.infer_type(receiver, file);
        if !self.method_exists_on_type(&receiver_type, &method.name, file) {
            self.errors.push(CompilerError::UndefinedReference {
                name: format!("method '{}' on type '{}'", method.name, receiver_type),
                span,
            });
        }
    }

    /// Validate a block expression (statements + result)
    fn validate_expr_block(
        &mut self,
        statements: &[BlockStatement],
        result: &Expr,
        file: &File,
    ) {
        for stmt in statements {
            match stmt {
                BlockStatement::Let { mutable, pattern, value, ty, .. } => {
                    self.validate_expr(value, file);
                    let ty_str = ty.as_ref().map_or_else(
                        || self.infer_type(value, file),
                        |t| Self::type_to_string(t),
                    );
                    for binding in collect_bindings_from_pattern(pattern) {
                        self.local_let_bindings
                            .insert(binding.name, (ty_str.clone(), *mutable));
                    }
                }
                BlockStatement::Assign { target, value, span } => {
                    self.validate_expr(target, file);
                    self.validate_expr(value, file);
                    if !self.is_expr_mutable(target, file) {
                        self.errors
                            .push(CompilerError::AssignmentToImmutable { span: *span });
                    }
                }
                BlockStatement::Expr(expr) => {
                    self.validate_expr(expr, file);
                }
            }
        }
        self.validate_expr(result, file);
    }

    /// Validate that struct instantiation respects mutability requirements
    /// Validate struct field requirements: all required fields must be provided, no unknown fields
    pub(super) fn validate_struct_fields(
        &mut self,
        struct_name: &str,
        args: &[(crate::ast::Ident, Expr)],
        mounts: &[(crate::ast::Ident, Expr)],
        span: Span,
        file: &File,
    ) {
        // Find the struct definition in current file or module cache
        // Clone necessary data to avoid borrow checker issues
        let (field_names, mount_field_names, required_fields, required_mounts) = {
            if let Some(def) = self.find_struct_def_in_files(struct_name, file) {
                let field_names: Vec<String> =
                    def.fields.iter().map(|f| f.name.name.clone()).collect();
                let mount_field_names: Vec<String> = def
                    .mount_fields
                    .iter()
                    .map(|f| f.name.name.clone())
                    .collect();

                let required_fields: Vec<String> = def
                    .fields
                    .iter()
                    .filter(|f| {
                        // Field is required if it has no inline default and is not optional
                        f.default.is_none() && !f.optional
                    })
                    .map(|f| f.name.name.clone())
                    .collect();

                let required_mounts: Vec<String> = def
                    .mount_fields
                    .iter()
                    .filter(|f| {
                        // Mount fields with inline defaults are optional
                        if f.default.is_some() {
                            return false;
                        }
                        // Mount fields of type `Never` are always optional since
                        // they can never have a value (used by terminal types like Empty)
                        if matches!(&f.ty, Type::Primitive(PrimitiveType::Never)) {
                            return false;
                        }
                        true
                    })
                    .map(|f| f.name.name.clone())
                    .collect();

                (
                    field_names,
                    mount_field_names,
                    required_fields,
                    required_mounts,
                )
            } else {
                return; // Struct not found, skip validation
            }
        };

        // Now we can safely borrow self.errors mutably
        // Check all provided regular fields exist
        for (arg_name, _) in args {
            if !field_names.contains(&arg_name.name) {
                self.errors.push(CompilerError::UnknownField {
                    field: arg_name.name.clone(),
                    type_name: struct_name.to_string(),
                    span: arg_name.span,
                });
            }
        }

        // Check all provided mount fields exist
        for (mount_name, _) in mounts {
            if !mount_field_names.contains(&mount_name.name) {
                self.errors.push(CompilerError::UnknownMount {
                    mount: mount_name.name.clone(),
                    struct_name: struct_name.to_string(),
                    span: mount_name.span,
                });
            }
        }

        // Check all required regular fields are provided
        for field_name in required_fields {
            if !args.iter().any(|(name, _)| name.name == field_name) {
                self.errors.push(CompilerError::MissingField {
                    field: field_name,
                    type_name: struct_name.to_string(),
                    span,
                });
            }
        }

        // Check all required mount fields are provided
        for mount_name in required_mounts {
            if !mounts.iter().any(|(name, _)| name.name == mount_name) {
                self.errors.push(CompilerError::MissingField {
                    field: mount_name,
                    type_name: struct_name.to_string(),
                    span,
                });
            }
        }
    }

    /// Find a struct definition in the current file and module cache
    pub(super) fn find_struct_def_in_files<'a>(
        &'a self,
        struct_name: &str,
        current_file: &'a File,
    ) -> Option<&'a StructDef> {
        // Search in current file
        for statement in &current_file.statements {
            if let Statement::Definition(def) = statement {
                if let Definition::Struct(struct_def) = &**def {
                    if struct_def.name.name == struct_name {
                        return Some(struct_def);
                    }
                }
            }
        }

        // Search in module cache
        for (file, _) in self.module_cache.values() {
            for statement in &file.statements {
                if let Statement::Definition(def) = statement {
                    if let Definition::Struct(struct_def) = &**def {
                        if struct_def.name.name == struct_name {
                            return Some(struct_def);
                        }
                    }
                }
            }
        }

        None
    }

    pub(super) fn validate_struct_mutability(
        &mut self,
        struct_name: &str,
        args: &[(crate::ast::Ident, Expr)],
        mounts: &[(crate::ast::Ident, Expr)],
        file: &File,
        span: Span,
    ) {
        // Find the struct definition
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                if let Definition::Struct(struct_def) = &**def {
                    if struct_def.name.name == struct_name {
                        // Check each regular field argument
                        for (arg_name, arg_expr) in args {
                            // Find the corresponding field in the struct
                            if let Some(field) = struct_def
                                .fields
                                .iter()
                                .find(|f| f.name.name == arg_name.name)
                            {
                                // If field is mutable, check that the arg expression is mutable
                                if field.mutable && !self.is_expr_mutable(arg_expr, file) {
                                    self.errors.push(CompilerError::MutabilityMismatch {
                                        param: arg_name.name.clone(),
                                        span,
                                    });
                                }
                            }
                        }
                        // Check each mount field argument
                        for (mount_name, mount_expr) in mounts {
                            // Find the corresponding mount field in the struct
                            if let Some(field) = struct_def
                                .mount_fields
                                .iter()
                                .find(|f| f.name.name == mount_name.name)
                            {
                                // If mount field is mutable, check that the mount expression is mutable
                                if field.mutable && !self.is_expr_mutable(mount_expr, file) {
                                    self.errors.push(CompilerError::MutabilityMismatch {
                                        param: mount_name.name.clone(),
                                        span,
                                    });
                                }
                            }
                        }
                        return;
                    }
                }
            }
        }
    }

    /// Validate binary operator type compatibility
    pub(super) fn validate_binary_op(
        &mut self,
        left: &Expr,
        op: BinaryOperator,
        right: &Expr,
        span: Span,
        file: &File,
    ) {
        let left_type = self.infer_type(left, file);
        let right_type = self.infer_type(right, file);

        // Check type compatibility based on operator
        let valid = match op {
            // Add: Number + Number or String + String (concatenation) or GPU numeric types
            BinaryOperator::Add => {
                matches!(
                    (&left_type[..], &right_type[..]),
                    ("Number", "Number") | ("String", "String")
                ) || Self::are_gpu_numeric_compatible(&left_type, &right_type)
            }
            // Arithmetic, comparison, and range operators: Number + Number or GPU numeric types
            BinaryOperator::Sub
            | BinaryOperator::Mul
            | BinaryOperator::Div
            | BinaryOperator::Mod
            | BinaryOperator::Lt
            | BinaryOperator::Gt
            | BinaryOperator::Le
            | BinaryOperator::Ge
            | BinaryOperator::Range => {
                matches!((&left_type[..], &right_type[..]), ("Number", "Number"))
                    || Self::are_gpu_numeric_compatible(&left_type, &right_type)
            }
            // Equality operators: same types or compatible GPU types
            BinaryOperator::Eq | BinaryOperator::Ne => {
                left_type == right_type || Self::are_gpu_numeric_compatible(&left_type, &right_type)
            }
            // Logical operators: Boolean + Boolean or bool + bool
            BinaryOperator::And | BinaryOperator::Or => {
                (left_type == "Boolean" && right_type == "Boolean")
                    || (left_type == "bool" && right_type == "bool")
            }
        };

        if !valid {
            self.errors.push(CompilerError::InvalidBinaryOp {
                op: format!("{op:?}"),
                left_type,
                right_type,
                span,
            });
        }
    }

    /// Check if two types are compatible GPU numeric types
    pub(super) fn are_gpu_numeric_compatible(left: &str, right: &str) -> bool {
        // GPU scalar types
        const GPU_SCALARS: &[&str] = &["f32", "i32", "u32"];
        // GPU vector types (same component type can do arithmetic)
        const GPU_FLOAT_VECTORS: &[&str] = &["vec2", "vec3", "vec4"];
        const GPU_INT_VECTORS: &[&str] = &["ivec2", "ivec3", "ivec4"];
        const GPU_UINT_VECTORS: &[&str] = &["uvec2", "uvec3", "uvec4"];

        // Same scalar type
        if left == right && GPU_SCALARS.contains(&left) {
            return true;
        }

        // Same vector type
        if left == right
            && (GPU_FLOAT_VECTORS.contains(&left)
                || GPU_INT_VECTORS.contains(&left)
                || GPU_UINT_VECTORS.contains(&left))
        {
            return true;
        }

        // Scalar with matching vector (for scalar*vector operations)
        if left == "f32" && GPU_FLOAT_VECTORS.contains(&right) {
            return true;
        }
        if right == "f32" && GPU_FLOAT_VECTORS.contains(&left) {
            return true;
        }

        false
    }

    /// Validate for loop collection is an array
    pub(super) fn validate_for_loop(&mut self, collection: &Expr, span: Span, file: &File) {
        let collection_type = self.infer_type(collection, file);

        // Check if it's an array type (starts with '[')
        if !collection_type.starts_with('[') {
            self.errors.push(CompilerError::ForLoopNotArray {
                actual: collection_type,
                span,
            });
        }
    }

    /// Validate destructuring pattern matches the value type
    pub(super) fn validate_destructuring_pattern(
        &mut self,
        pattern: &BindingPattern,
        value: &Expr,
        span: Span,
        file: &File,
    ) {
        let value_type = self.infer_type(value, file);

        match pattern {
            BindingPattern::Array { .. } => {
                // Array destructuring requires an array type
                if !value_type.starts_with('[') {
                    self.errors.push(CompilerError::ArrayDestructuringNotArray {
                        actual: value_type,
                        span,
                    });
                }
            }
            BindingPattern::Struct { fields, .. } => {
                // Struct destructuring requires a struct type
                // Check if the type is a known struct
                if let Some(struct_info) = self.symbols.get_struct(&value_type) {
                    // Validate that all destructured fields exist on the struct
                    let field_names: Vec<&str> =
                        struct_info.fields.iter().map(|f| f.name.as_str()).collect();
                    for field in fields {
                        if !field_names.contains(&field.name.name.as_str()) {
                            self.errors.push(CompilerError::UnknownField {
                                field: field.name.name.clone(),
                                type_name: value_type.clone(),
                                span: field.name.span,
                            });
                        }
                    }
                } else {
                    // Not a known struct - report error (includes primitives)
                    self.errors
                        .push(CompilerError::StructDestructuringNotStruct {
                            actual: value_type,
                            span,
                        });
                }
            }
            BindingPattern::Tuple { .. } | BindingPattern::Simple(_) => {
                // Tuple and simple patterns don't require type validation here
            }
        }
    }

    /// Validate if condition is boolean or optional
    pub(super) fn validate_if_condition(&mut self, condition: &Expr, span: Span, file: &File) {
        let condition_type = self.infer_type(condition, file);

        // Condition must be Boolean or optional (ends with '?')
        if condition_type != "Boolean" && !condition_type.ends_with('?') {
            self.errors.push(CompilerError::InvalidIfCondition {
                actual: condition_type,
                span,
            });
        }
    }

    /// Validate match expression exhaustiveness
    pub(super) fn validate_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[crate::ast::MatchArm],
        span: Span,
        file: &File,
    ) {
        // Infer scrutinee type - must be an enum
        let scrutinee_type = self.infer_type(scrutinee, file);

        // Check if scrutinee is an enum (look it up in symbol table)
        if !self.symbols.is_enum(&scrutinee_type) {
            self.errors.push(CompilerError::MatchNotEnum {
                actual: scrutinee_type,
                span,
            });
            return;
        }

        // Get enum variants from symbol table
        let variants = match self.symbols.get_enum_variants(&scrutinee_type) {
            Some(v) => v.clone(),
            None => return, // Should not happen if is_enum returned true
        };

        // Collect all variant names from match arms
        let mut covered_variants = HashSet::new();
        let mut has_wildcard = false;
        for arm in arms {
            match &arm.pattern {
                crate::ast::Pattern::Variant { name, bindings } => {
                    // Check for duplicate arms
                    if !covered_variants.insert(name.name.clone()) {
                        self.errors.push(CompilerError::DuplicateMatchArm {
                            variant: name.name.clone(),
                            span: arm.span,
                        });
                        continue;
                    }

                    // Validate variant exists and arity matches
                    self.validate_match_arm(
                        &scrutinee_type,
                        &name.name,
                        bindings.len(),
                        arm.span,
                        &variants,
                    );
                }
                crate::ast::Pattern::Wildcard => {
                    // Wildcard covers all remaining variants
                    has_wildcard = true;
                }
            }
        }

        // Check exhaustiveness - all variants must be covered (unless there's a wildcard)
        if !has_wildcard {
            let missing_variants: Vec<String> = variants
                .keys()
                .filter(|v| !covered_variants.contains(*v))
                .cloned()
                .collect();

            if !missing_variants.is_empty() {
                self.errors.push(CompilerError::NonExhaustiveMatch {
                    missing: missing_variants.join(", "),
                    span,
                });
            }
        }
    }

    /// Validate enum instantiation with named parameters
    pub(super) fn validate_enum_instantiation(
        &mut self,
        enum_name: &crate::ast::Ident,
        variant_name: &crate::ast::Ident,
        data: &[(crate::ast::Ident, Expr)],
        span: Span,
        file: &File,
    ) {
        // Check if the enum exists
        if !self.symbols.is_enum(&enum_name.name) {
            self.errors.push(CompilerError::UndefinedType {
                name: enum_name.name.clone(),
                span: enum_name.span,
            });
            return;
        }

        // Get the enum definition to access variant field information
        let variant_fields =
            self.get_enum_variant_fields(&enum_name.name, &variant_name.name, file);

        match variant_fields {
            Some(fields) => {
                // Check if variant has no fields but data was provided
                if fields.is_empty() && !data.is_empty() {
                    self.errors.push(CompilerError::EnumVariantWithoutData {
                        variant: variant_name.name.clone(),
                        enum_name: enum_name.name.clone(),
                        span,
                    });
                    return;
                }

                // Check if variant has fields but no data was provided
                if !fields.is_empty() && data.is_empty() {
                    self.errors.push(CompilerError::EnumVariantRequiresData {
                        variant: variant_name.name.clone(),
                        enum_name: enum_name.name.clone(),
                        span,
                    });
                    return;
                }

                // Check that all required fields are provided
                let provided_fields: HashSet<&str> =
                    data.iter().map(|(name, _)| name.name.as_str()).collect();
                let required_fields: HashSet<&str> =
                    fields.iter().map(|f| f.name.name.as_str()).collect();

                // Check for missing fields
                for field in &required_fields {
                    if !provided_fields.contains(field) {
                        self.errors.push(CompilerError::MissingField {
                            field: field.to_string(),
                            type_name: format!("{}.{}", enum_name.name, variant_name.name),
                            span,
                        });
                    }
                }

                // Check for unknown fields
                for (provided_field, _) in data {
                    if !required_fields.contains(provided_field.name.as_str()) {
                        self.errors.push(CompilerError::UnknownField {
                            field: provided_field.name.clone(),
                            type_name: format!("{}.{}", enum_name.name, variant_name.name),
                            span: provided_field.span,
                        });
                    }
                }
            }
            None => {
                // Variant doesn't exist
                self.errors.push(CompilerError::UnknownEnumVariant {
                    variant: variant_name.name.clone(),
                    enum_name: enum_name.name.clone(),
                    span: variant_name.span,
                });
            }
        }
    }

    /// Get the field definitions for a specific enum variant
    /// Returns None if the enum or variant doesn't exist
    pub(super) fn get_enum_variant_fields(
        &self,
        enum_name: &str,
        variant_name: &str,
        current_file: &File,
    ) -> Option<Vec<crate::ast::FieldDef>> {
        // First, search in the current file
        for statement in &current_file.statements {
            if let Statement::Definition(def) = statement {
                if let Definition::Enum(enum_def) = &**def {
                    if enum_def.name.name == enum_name {
                        // Find the variant
                        for variant in &enum_def.variants {
                            if variant.name.name == variant_name {
                                return Some(variant.fields.clone());
                            }
                        }
                        return None; // Variant not found
                    }
                }
            }
        }

        // If not found in current file, search through module cache
        for (file, _) in self.module_cache.values() {
            for statement in &file.statements {
                if let Statement::Definition(def) = statement {
                    if let Definition::Enum(enum_def) = &**def {
                        if enum_def.name.name == enum_name {
                            // Find the variant
                            for variant in &enum_def.variants {
                                if variant.name.name == variant_name {
                                    return Some(variant.fields.clone());
                                }
                            }
                            return None; // Variant not found
                        }
                    }
                }
            }
        }
        None // Enum not found
    }

    /// Validate a single match arm
    pub(super) fn validate_match_arm(
        &mut self,
        enum_name: &str,
        variant_name: &str,
        binding_count: usize,
        span: Span,
        variants: &std::collections::HashMap<String, (usize, Span)>,
    ) {
        // Check if variant exists
        match variants.get(variant_name) {
            Some((expected_arity, _)) => {
                // Check arity matches
                if *expected_arity != binding_count {
                    self.errors.push(CompilerError::VariantArityMismatch {
                        variant: variant_name.to_string(),
                        expected: *expected_arity,
                        actual: binding_count,
                        span,
                    });
                }
            }
            None => {
                // Variant doesn't exist in enum
                self.errors.push(CompilerError::UnknownEnumVariant {
                    variant: variant_name.to_string(),
                    enum_name: enum_name.to_string(),
                    span,
                });
            }
        }
    }

    /// Validate function return type matches the body expression type
    pub(super) fn validate_function_return_type(&mut self, func: &crate::ast::FnDef, file: &File) {
        // Clear local let bindings for this function
        self.local_let_bindings.clear();

        // Register function parameters as local bindings
        // Function parameters are mutable by default (can be assigned to)
        for param in &func.params {
            if let Some(ty) = &param.ty {
                self.validate_type(ty);
            }
            let ty_str = param
                .ty
                .as_ref()
                .map_or_else(|| "Unknown".to_string(), |ty| Self::type_to_string(ty));
            // Register parameter as a local binding with its type (mutable=true for params)
            self.local_let_bindings
                .insert(param.name.name.clone(), (ty_str, true));
        }

        // Validate the function body expression
        self.validate_expr(&func.body, file);

        // If there's a declared return type, check it matches the body type
        if let Some(declared_return_type) = &func.return_type {
            let body_type = self.infer_type(&func.body, file);
            let expected_type = Self::type_to_string(declared_return_type);

            // Check if types are compatible
            if !self.type_strings_compatible(&expected_type, &body_type) {
                self.errors.push(CompilerError::FunctionReturnTypeMismatch {
                    function: func.name.name.clone(),
                    expected: expected_type,
                    actual: body_type,
                    span: func.name.span,
                });
            }
        }

        // Clear local let bindings after function
        self.local_let_bindings.clear();
    }

    /// Validate a standalone function definition (outside of impl blocks)
    pub(super) fn validate_standalone_function(
        &mut self,
        func: &crate::ast::FunctionDef,
        file: &File,
    ) {
        // Clear local let bindings for this function
        self.local_let_bindings.clear();

        // Register function parameters as local bindings
        // Function parameters are mutable by default (can be assigned to)
        for param in &func.params {
            if let Some(ty) = &param.ty {
                self.validate_type(ty);
            }
            let ty_str = param
                .ty
                .as_ref()
                .map_or_else(|| "Unknown".to_string(), |ty| Self::type_to_string(ty));
            // Register parameter as a local binding with its type (mutable=true for params)
            self.local_let_bindings
                .insert(param.name.name.clone(), (ty_str, true));
        }

        // Validate return type if declared
        if let Some(return_type) = &func.return_type {
            self.validate_type(return_type);
        }

        // Validate the function body expression
        self.validate_expr(&func.body, file);

        // If there's a declared return type, check it matches the body type
        if let Some(declared_return_type) = &func.return_type {
            let body_type = self.infer_type(&func.body, file);
            let expected_type = Self::type_to_string(declared_return_type);

            // Check if types are compatible
            if !self.type_strings_compatible(&expected_type, &body_type) {
                self.errors.push(CompilerError::FunctionReturnTypeMismatch {
                    function: func.name.name.clone(),
                    expected: expected_type,
                    actual: body_type,
                    span: func.name.span,
                });
            }
        }

        // Clear local let bindings after function
        self.local_let_bindings.clear();
    }

    /// Check if a method exists on a given type
    ///
    /// Handles:
    /// 1. Builtin methods on primitive types (`vec3.normalize()`, `mat4.transpose()`, etc.)
    /// 2. User-defined methods in impl blocks
    pub(super) fn method_exists_on_type(
        &self,
        type_name: &str,
        method_name: &str,
        file: &File,
    ) -> bool {
        // Skip validation for unknown types (chained method calls where we can't infer intermediate types)
        if type_name == "Unknown" || type_name.contains("Unknown") {
            return true;
        }

        // Check if it's a primitive GPU type with builtin methods
        if let Some(prim) = Self::string_to_primitive_type(type_name) {
            if crate::builtins::resolve_method_type(prim, method_name).is_some() {
                return true;
            }
        }

        // Check if the method is a common builtin that works on numbers/vectors
        // This handles chained calls where type inference might not propagate correctly
        let common_builtins = [
            "abs",
            "sign",
            "floor",
            "ceil",
            "round",
            "trunc",
            "fract",
            "sin",
            "cos",
            "tan",
            "asin",
            "acos",
            "atan",
            "exp",
            "log",
            "sqrt",
            "pow",
            "min",
            "max",
            "clamp",
            "normalize",
            "length",
            "distance",
            "dot",
            "cross",
            "saturate",
            "radians",
            "degrees",
        ];
        if common_builtins.contains(&method_name) {
            return true;
        }

        // Check if it's a struct with an impl block containing the method
        if self.symbols.is_struct(type_name) {
            // Check impl blocks in the current file
            for statement in &file.statements {
                if let Statement::Definition(def) = statement {
                    if let Definition::Impl(impl_def) = &**def {
                        if impl_def.name.name == type_name {
                            for func in &impl_def.functions {
                                if func.name.name == method_name {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
            // Also check if there's an impl in the symbol table
            if self.symbols.impls.contains_key(type_name) {
                // The impl exists, but we need to check for the method
                // For now, if we find an impl, we assume the method might exist
                // (the impl block methods aren't stored in the symbol table currently)
            }
        }

        false
    }
}
