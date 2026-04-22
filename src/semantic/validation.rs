use super::module_resolver::ModuleResolver;
use super::SemanticAnalyzer;
use crate::ast::{
    BinaryOperator, BindingPattern, BlockStatement, Definition, Expr, File, Statement, StructDef,
    Type,
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
                    // Gap 1: nil can only be assigned to optional types
                    if let Some(type_ann) = &let_binding.type_annotation {
                        let declared = Self::type_to_string(type_ann);
                        let inferred = self.infer_type(&let_binding.value, file);
                        if inferred == "Nil" && !declared.ends_with('?') {
                            self.errors.push(CompilerError::NilAssignedToNonOptional {
                                expected: declared,
                                span: let_binding.span,
                            });
                        }
                    }
                    // Register closure-typed module-level bindings for call-site enforcement
                    if let Some(Type::Closure { params, .. }) = &let_binding.type_annotation {
                        let conventions: Vec<_> = params.iter().map(|(c, _)| *c).collect();
                        for binding in collect_bindings_from_pattern(&let_binding.pattern) {
                            self.closure_binding_conventions
                                .insert(binding.name, conventions.clone());
                        }
                    }
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
                        self.current_impl_struct = Some(impl_def.name.name.clone());
                        self.local_let_bindings.clear();
                        self.consumed_bindings.clear();
                        for func in &impl_def.functions {
                            self.validate_function_return_type(func, file);
                        }
                        self.current_impl_struct = None;
                        self.local_let_bindings.clear();
                        self.consumed_bindings.clear();
                    }
                    Definition::Function(func_def) => {
                        self.local_let_bindings.clear();
                        self.consumed_bindings.clear();
                        for param in &func_def.params {
                            if let Some(ty) = &param.ty {
                                self.validate_type(ty);
                            }
                            let ty_str = param.ty.as_ref().map_or_else(
                                || "Unknown".to_string(),
                                |ty| Self::type_to_string(ty),
                            );
                            let mutable = matches!(
                                param.convention,
                                crate::ast::ParamConvention::Mut
                                    | crate::ast::ParamConvention::Sink
                            );
                            self.local_let_bindings
                                .insert(param.name.name.clone(), (ty_str, mutable));
                        }
                        if let Some(body) = &func_def.body {
                            self.validate_expr(body, file);
                        }
                        self.local_let_bindings.clear();
                        self.consumed_bindings.clear();
                    }
                    Definition::Module(module_def) => {
                        for nested_def in &module_def.definitions {
                            if let Definition::Impl(impl_def) = nested_def {
                                self.current_impl_struct = Some(impl_def.name.name.clone());
                                self.local_let_bindings.clear();
                                self.consumed_bindings.clear();
                                for func in &impl_def.functions {
                                    self.validate_function_return_type(func, file);
                                }
                                self.current_impl_struct = None;
                                self.local_let_bindings.clear();
                                self.consumed_bindings.clear();
                            }
                        }
                    }
                    Definition::Trait(_) | Definition::Enum(_) => {}
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
                // Check that the default expression type matches the declared field type
                let inferred = self.infer_type(default_expr, file);
                let declared = Self::type_to_string(&field.ty);
                // nil is compatible with any optional type
                let nil_to_optional = inferred == "Nil" && declared.ends_with('?');
                // a value of type T is compatible with T? (implicit wrapping)
                let inner_to_optional =
                    declared.ends_with('?') && declared.trim_end_matches('?') == inferred.as_str();
                // treat any type containing "Unknown" or "InferredEnum" as indeterminate
                let has_unknown = inferred.contains("Unknown") || inferred.contains("InferredEnum");
                if !nil_to_optional
                    && !inner_to_optional
                    && !has_unknown
                    && inferred != "Unknown"
                    && declared != "Unknown"
                    && !self.type_strings_compatible(&declared, &inferred)
                {
                    self.errors.push(CompilerError::TypeMismatch {
                        expected: declared,
                        found: inferred,
                        span: field.span,
                    });
                }
            }
        }
    }

    /// Validate a single expression (recursively)
    #[expect(
        clippy::too_many_lines,
        reason = "dispatcher match over 18+ Expr variants; each arm is a single call"
    )]
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
                span,
            } => {
                self.validate_expr_invocation(path, type_args, args, *span, file);
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
            Expr::BinaryOp {
                left,
                op,
                right,
                span,
            } => {
                self.validate_expr(left, file);
                self.validate_expr(right, file);
                self.validate_binary_op(left, *op, right, *span, file);
            }
            Expr::UnaryOp { operand, .. } => {
                self.validate_expr(operand, file);
            }
            Expr::ForExpr {
                var,
                collection,
                body,
                span,
            } => {
                self.validate_expr(collection, file);
                let mut scope = HashSet::new();
                scope.insert(var.name.clone());
                self.loop_var_scopes.push(scope);
                self.validate_expr(body, file);
                self.loop_var_scopes.pop();
                self.validate_for_loop(collection, *span, file);
            }
            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
                span,
            } => {
                self.validate_expr(condition, file);
                // Each branch is a separate control-flow path. Snapshot consumed_bindings
                // before each branch and take the union afterward so that a binding consumed
                // in either branch is considered consumed in the join point (conservative but
                // never unsound: it can only produce false-positive UseAfterSink, never miss one).
                let pre_if = self.consumed_bindings.clone();
                self.validate_expr(then_branch, file);
                let after_then = self.consumed_bindings.clone();
                self.consumed_bindings = pre_if.clone();
                if let Some(else_expr) = else_branch {
                    self.validate_expr(else_expr, file);
                    // Check that both branch types are compatible
                    let then_type = self.infer_type(then_branch, file);
                    let else_type = self.infer_type(else_expr, file);
                    // Skip when either type is unknown or contains unknown (e.g. [Unknown])
                    if !then_type.contains("Unknown")
                        && !else_type.contains("Unknown")
                        && !self.type_strings_compatible(&then_type, &else_type)
                    {
                        self.errors.push(CompilerError::TypeMismatch {
                            expected: then_type,
                            found: else_type,
                            span: *span,
                        });
                    }
                }
                let after_else = self.consumed_bindings.clone();
                // Union: consumed if consumed in then OR else
                self.consumed_bindings = after_then;
                self.consumed_bindings.extend(after_else);
                self.validate_if_condition(condition, *span, file);
            }
            Expr::MatchExpr {
                scrutinee,
                arms,
                span,
            } => {
                self.validate_expr(scrutinee, file);
                let pre_match = self.consumed_bindings.clone();
                let mut post_union = pre_match.clone();
                let mut arm_types: Vec<String> = Vec::new();
                for arm in arms {
                    self.consumed_bindings = pre_match.clone();
                    // Register arm pattern bindings into a temporary scope
                    if let crate::ast::Pattern::Variant { bindings, .. } = &arm.pattern {
                        let scope: HashSet<String> =
                            bindings.iter().map(|b| b.name.clone()).collect();
                        self.closure_param_scopes.push(scope);
                        self.validate_expr(&arm.body, file);
                        self.closure_param_scopes.pop();
                    } else {
                        self.validate_expr(&arm.body, file);
                    }
                    let arm_type = self.infer_type(&arm.body, file);
                    arm_types.push(arm_type);
                    post_union.extend(self.consumed_bindings.iter().cloned());
                }
                self.consumed_bindings = post_union;
                // Check that all arm types are compatible with the first arm's type
                if let Some(first_type) = arm_types.first().cloned() {
                    if !first_type.contains("Unknown") {
                        for (arm, arm_type) in arms.iter().zip(arm_types.iter()).skip(1) {
                            if !arm_type.contains("Unknown")
                                && !self.type_strings_compatible(&first_type, arm_type)
                            {
                                self.errors.push(CompilerError::TypeMismatch {
                                    expected: first_type.clone(),
                                    found: arm_type.clone(),
                                    span: arm.span,
                                });
                            }
                        }
                    }
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
            Expr::DictAccess { dict, key, span } => {
                self.validate_expr(dict, file);
                self.validate_expr(key, file);
                // Gap 3: Validate key type against declared dict type
                let dict_type = self.infer_type(dict, file);
                if let Some(inner) = dict_type
                    .strip_prefix('[')
                    .and_then(|s| s.strip_suffix(']'))
                    .filter(|s| s.contains(": "))
                {
                    if let Some(colon_pos) = inner.find(": ") {
                        let expected_key_type = &inner[..colon_pos];
                        let actual_key_type = self.infer_type(key, file);
                        if actual_key_type != "Unknown" && actual_key_type != expected_key_type {
                            self.errors.push(CompilerError::TypeMismatch {
                                expected: expected_key_type.to_string(),
                                found: actual_key_type,
                                span: *span,
                            });
                        }
                    }
                }
            }
            Expr::FieldAccess {
                object,
                field,
                span,
            } => {
                self.validate_expr(object, file);
                let obj_type = self.infer_type(object, file);
                if obj_type != "Unknown" {
                    // Gap 1: Field access on optional type requires unwrapping
                    if obj_type.ends_with('?') {
                        let base = obj_type.trim_end_matches('?');
                        if base != "Unknown" && self.symbols.get_struct(base).is_some() {
                            self.errors.push(CompilerError::OptionalUsedAsNonOptional {
                                actual: obj_type.clone(),
                                expected: base.to_string(),
                                span: *span,
                            });
                        }
                    } else {
                        // Gap 5: Check field existence
                        let base_type = obj_type.trim_end_matches('?');
                        if let Some(struct_info) = self.symbols.get_struct(base_type) {
                            if !struct_info.fields.iter().any(|f| f.name == field.name) {
                                self.errors.push(CompilerError::UnknownField {
                                    field: field.name.clone(),
                                    type_name: base_type.to_string(),
                                    span: field.span,
                                });
                            }
                        }
                    }
                }
            }
            Expr::ClosureExpr { params, body, .. } => {
                self.validate_expr_closure(params, body, file);
            }
            Expr::LetExpr { .. } => {
                self.validate_expr_let(expr, file);
            }
            Expr::MethodCall {
                receiver,
                method,
                args,
                span,
            } => {
                self.validate_expr_method_call(receiver, method, args.as_slice(), *span, file);
            }
            Expr::Block {
                statements, result, ..
            } => {
                self.validate_expr_block(statements, result, file);
            }
        }

        self.validate_expr_depth = self.validate_expr_depth.saturating_sub(1);
    }

    /// Check module visibility for a multi-segment path (mod::item).
    /// Returns true if access is allowed, false if a VisibilityViolation was emitted.
    fn check_module_visibility(&mut self, path: &[crate::ast::Ident], span: Span) -> bool {
        if path.len() < 2 {
            return true;
        }
        let module_name = &path[0].name;
        if let Some(module_info) = self.symbols.modules.get(module_name.as_str()) {
            // Look up the item in the module's symbol table
            let item_name = &path[1].name;
            let item_visibility = module_info
                .symbols
                .structs
                .get(item_name.as_str())
                .map(|s| s.visibility)
                .or_else(|| {
                    module_info
                        .symbols
                        .functions
                        .get(item_name.as_str())
                        .and_then(|overloads| overloads.first().map(|f| f.visibility))
                })
                .or_else(|| {
                    module_info
                        .symbols
                        .enums
                        .get(item_name.as_str())
                        .map(|e| e.visibility)
                })
                .or_else(|| {
                    module_info
                        .symbols
                        .traits
                        .get(item_name.as_str())
                        .map(|t| t.visibility)
                })
                .or_else(|| {
                    module_info
                        .symbols
                        .lets
                        .get(item_name.as_str())
                        .map(|l| l.visibility)
                });

            if let Some(crate::ast::Visibility::Private) = item_visibility {
                self.errors.push(CompilerError::VisibilityViolation {
                    name: item_name.clone(),
                    span,
                });
                return false;
            }
        }
        true
    }

    /// Validate a reference expression (path lookup)
    fn validate_expr_reference(&mut self, path: &[crate::ast::Ident], span: Span, _file: &File) {
        if let Some(first) = path.first() {
            if self.consumed_bindings.contains(&first.name) {
                self.errors.push(CompilerError::UseAfterSink {
                    name: first.name.clone(),
                    span,
                });
                return;
            }
        }
        // Check module visibility for qualified paths (mod::item)
        if !self.check_module_visibility(path, span) {
            return;
        }
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
                || self.symbols.functions.contains_key(name.as_str())
            {
                return;
            }
            if let Some(ref struct_name) = self.current_impl_struct.clone() {
                if let Some(struct_info) = self.symbols.get_struct(struct_name) {
                    for field in &struct_info.fields {
                        if field.name == *name {
                            return;
                        }
                    }
                }
            }
            self.errors.push(CompilerError::UndefinedReference {
                name: name.clone(),
                span,
            });
        }
    }

    /// Validate an invocation expression (struct instantiation or function call)
    fn validate_expr_invocation(
        &mut self,
        path: &[crate::ast::Ident],
        type_args: &[crate::ast::Type],
        args: &[(Option<crate::ast::Ident>, Expr)],
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
        for type_arg in type_args {
            self.validate_type(type_arg);
        }

        // Check module visibility for qualified paths (mod::item)
        if !self.check_module_visibility(path, span) {
            return;
        }

        let is_struct = self.symbols.get_struct_qualified(&name).is_some();
        if is_struct {
            self.validate_expr_invocation_struct(&name, type_args, args, span, file);
        } else {
            self.validate_expr_invocation_function(&name, type_args, args, span, file);
        }
    }

    /// Validate a struct instantiation invocation
    fn validate_expr_invocation_struct(
        &mut self,
        name: &str,
        type_args: &[crate::ast::Type],
        args: &[(Option<crate::ast::Ident>, Expr)],
        span: Span,
        file: &File,
    ) {
        let named_args: Vec<(crate::ast::Ident, Expr)> = args
            .iter()
            .filter_map(|(name_opt, expr)| name_opt.as_ref().map(|n| (n.clone(), expr.clone())))
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
            } else {
                // Validate each type arg satisfies its constraints
                for (type_arg, generic_param) in type_args.iter().zip(expected_params.iter()) {
                    for constraint in &generic_param.constraints {
                        let crate::ast::GenericConstraint::Trait(trait_ref) = constraint;
                        if !self.type_satisfies_trait_constraint(type_arg, &trait_ref.name) {
                            self.errors.push(CompilerError::GenericConstraintViolation {
                                arg: Self::type_to_string(type_arg),
                                constraint: trait_ref.name.clone(),
                                span,
                            });
                        }
                    }
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

        self.validate_struct_fields(name, &named_args, span, file);
        self.validate_struct_mutability(name, &named_args, file, span);
    }

    /// Validate a function call invocation, performing overload resolution when multiple
    /// overloads exist for the same name.
    fn validate_expr_invocation_function(
        &mut self,
        name: &str,
        type_args: &[crate::ast::Type],
        args: &[(Option<crate::ast::Ident>, crate::ast::Expr)],
        span: Span,
        file: &File,
    ) {
        // Gap 4: Validate generic type arguments against function's generic parameters
        if !type_args.is_empty() {
            let simple_name_for_lookup = name.rsplit("::").next().unwrap_or(name);
            let overloads_for_generics = {
                let direct = self.symbols.get_function_overloads(name);
                if direct.is_empty() {
                    self.symbols.get_function_overloads(simple_name_for_lookup)
                } else {
                    direct
                }
            };
            let func_generics = overloads_for_generics
                .first()
                .map(|f| f.generics.clone())
                .unwrap_or_default();

            if func_generics.is_empty() {
                self.errors.push(CompilerError::GenericArityMismatch {
                    name: name.to_string(),
                    expected: 0,
                    actual: type_args.len(),
                    span,
                });
            } else if type_args.len() != func_generics.len() {
                self.errors.push(CompilerError::GenericArityMismatch {
                    name: name.to_string(),
                    expected: func_generics.len(),
                    actual: type_args.len(),
                    span,
                });
            } else {
                // Validate each type arg satisfies constraints
                for (type_arg, generic_param) in type_args.iter().zip(func_generics.iter()) {
                    for constraint in &generic_param.constraints {
                        let crate::ast::GenericConstraint::Trait(trait_ref) = constraint;
                        if !self.type_satisfies_trait_constraint(type_arg, &trait_ref.name) {
                            self.errors.push(CompilerError::GenericConstraintViolation {
                                arg: Self::type_to_string(type_arg),
                                constraint: trait_ref.name.clone(),
                                span,
                            });
                        }
                    }
                }
            }
        }

        let simple_name = name.rsplit("::").next().unwrap_or(name);
        let overloads: &[_] = {
            let direct = self.symbols.get_function_overloads(name);
            if direct.is_empty() {
                self.symbols.get_function_overloads(simple_name)
            } else {
                direct
            }
        };

        match overloads.len() {
            0 => {
                // Check if this is a closure binding call — enforce closure param conventions
                let closure_conventions =
                    self.closure_binding_conventions.get(simple_name).cloned();
                if let Some(conventions) = closure_conventions {
                    self.validate_closure_call_conventions(&conventions, args, span, file);
                } else if !self.resolve_qualified_function(name) {
                    self.errors.push(CompilerError::UndefinedType {
                        name: format!("function '{name}'"),
                        span,
                    });
                }
            }
            1 => {
                // Single overload — check mut param mutability
                if let Some(info) = overloads.first() {
                    let params = info.params.clone();
                    self.validate_mut_param_args(&params, args, span, file);
                }
            }
            _ => {
                // Multiple overloads: resolve by argument labels or first-arg type
                let call_labels: Vec<Option<String>> = args
                    .iter()
                    .map(|(label, _)| label.as_ref().map(|l| l.name.clone()))
                    .collect();

                let matching: Vec<_> = overloads
                    .iter()
                    .filter(|overload| self.overload_matches(overload, &call_labels, args, file))
                    .collect();

                match matching.len() {
                    0 => {
                        self.errors.push(CompilerError::NoMatchingOverload {
                            function: name.rsplit("::").next().unwrap_or(name).to_string(),
                            span,
                        });
                    }
                    1 => {
                        // Resolved to a unique overload — check mut param mutability
                        if let Some(info) = matching.first() {
                            let params = info.params.clone();
                            self.validate_mut_param_args(&params, args, span, file);
                        }
                    }
                    _ => {
                        self.errors.push(CompilerError::AmbiguousCall {
                            function: name.rsplit("::").next().unwrap_or(name).to_string(),
                            span,
                        });
                    }
                }
            }
        }
    }

    /// For each `mut`-convention parameter, verify the corresponding call argument is mutable.
    fn validate_mut_param_args(
        &mut self,
        params: &[crate::semantic::symbol_table::ParamInfo],
        args: &[(Option<crate::ast::Ident>, crate::ast::Expr)],
        span: Span,
        file: &File,
    ) {
        use crate::ast::ParamConvention;
        let non_self: Vec<_> = params.iter().filter(|p| p.name.name != "self").collect();
        for (i, (label_opt, arg_expr)) in args.iter().enumerate() {
            let param = label_opt.as_ref().map_or_else(
                || non_self.get(i).copied(),
                |label| {
                    non_self
                        .iter()
                        .find(|p| {
                            p.external_label
                                .as_ref()
                                .is_some_and(|l| l.name == label.name)
                                || p.name.name == label.name
                        })
                        .map(|v| &**v)
                },
            );
            if let Some(param) = param {
                if param.convention == ParamConvention::Mut && !self.is_expr_mutable(arg_expr, file)
                {
                    self.errors.push(CompilerError::MutabilityMismatch {
                        param: param.name.name.clone(),
                        span,
                    });
                }
                if param.convention == ParamConvention::Sink {
                    if let crate::ast::Expr::Reference { path, .. } = arg_expr {
                        if let Some(first) = path.first() {
                            self.consumed_bindings.insert(first.name.clone());
                        }
                    }
                }
            }
        }
    }

    /// Check whether a single overload matches the given call arguments.
    ///
    /// Resolution order:
    /// 1. If all call arguments have labels, match by label set.
    /// 2. If no call arguments have labels, try to match by first-argument type.
    fn overload_matches(
        &self,
        overload: &crate::semantic::symbol_table::FunctionInfo,
        call_labels: &[Option<String>],
        args: &[(Option<crate::ast::Ident>, crate::ast::Expr)],
        file: &File,
    ) -> bool {
        let params = &overload.params;
        // Collect overload parameter labels (external_label if set, else param name)
        let param_labels: Vec<String> = params
            .iter()
            .filter(|p| p.name.name != "self")
            .map(|p| {
                p.external_label
                    .as_ref()
                    .map_or_else(|| p.name.name.clone(), |l| l.name.clone())
            })
            .collect();

        let all_labeled = call_labels.iter().all(Option::is_some);
        let none_labeled = call_labels.iter().all(Option::is_none);

        if all_labeled && !call_labels.is_empty() {
            // Mode A: match by label set
            let call_label_set: Vec<&str> =
                call_labels.iter().filter_map(|l| l.as_deref()).collect();
            let param_label_set: Vec<&str> = param_labels.iter().map(String::as_str).collect();
            call_label_set == param_label_set
        } else if none_labeled && !args.is_empty() {
            // Mode B: arity check first, then match by first-argument type
            let non_self_count = params.iter().filter(|p| p.name.name != "self").count();
            if args.len() != non_self_count {
                return false;
            }

            let first_arg_type = args.first().map_or_else(
                || "Unknown".to_string(),
                |(_, expr)| self.infer_type(expr, file),
            );

            let first_param_type = params
                .iter()
                .find(|p| p.name.name != "self")
                .and_then(|p| p.ty.as_ref())
                .map_or_else(|| "Unknown".to_string(), Self::type_to_string);

            // Unknown means we can't tell — accept it (conservative)
            first_arg_type == "Unknown"
                || first_param_type == "Unknown"
                || self.type_strings_compatible(&first_param_type, &first_arg_type)
        } else {
            // Mixed or empty args — accept (fallback)
            true
        }
    }

    /// Resolve a qualified function path like `math::compute` by traversing module symbol tables.
    #[expect(clippy::indexing_slicing, reason = "parts length checked above")]
    fn resolve_qualified_function(&self, name: &str) -> bool {
        let parts: Vec<&str> = name.splitn(2, "::").collect();
        if parts.len() != 2 {
            return false;
        }
        let (module_name, rest) = (parts[0], parts[1]);
        if let Some(module_info) = self.symbols.modules.get(module_name) {
            // Recurse into nested module paths
            if rest.contains("::") {
                let parts2: Vec<&str> = rest.splitn(2, "::").collect();
                if parts2.len() == 2 {
                    let (sub_module, fn_name) = (parts2[0], parts2[1]);
                    if let Some(sub_mod) = module_info.symbols.modules.get(sub_module) {
                        return sub_mod.symbols.get_function(fn_name).is_some();
                    }
                }
                false
            } else {
                module_info.symbols.get_function(rest).is_some()
            }
        } else {
            false
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
        let Expr::LetExpr {
            mutable,
            pattern,
            ty,
            value,
            body,
            span,
        } = expr
        else {
            return;
        };
        if let Some(type_ann) = ty {
            self.validate_type(type_ann);
        }
        self.validate_expr(value, file);
        // Gap 1: nil can only be assigned to optional types
        if let Some(type_ann) = ty {
            let declared = Self::type_to_string(type_ann);
            let inferred = self.infer_type(value, file);
            if inferred == "Nil" && !declared.ends_with('?') {
                self.errors.push(CompilerError::NilAssignedToNonOptional {
                    expected: declared,
                    span: *span,
                });
            }
        }
        self.validate_destructuring_pattern(pattern, value, *span, file);
        for binding in collect_bindings_from_pattern(pattern) {
            let inferred_ty = self.infer_type(value, file);
            // If annotated as a closure type, record param conventions for call-site enforcement
            if let Some(Type::Closure { params, .. }) = ty {
                let conventions: Vec<_> = params.iter().map(|(c, _)| *c).collect();
                self.closure_binding_conventions
                    .insert(binding.name.clone(), conventions);
            }
            self.local_let_bindings
                .insert(binding.name, (inferred_ty, *mutable));
        }
        self.validate_expr(body, file);
    }

    /// Validate a method call expression
    fn validate_expr_method_call(
        &mut self,
        receiver: &Expr,
        method: &crate::ast::Ident,
        args: &[(Option<crate::ast::Ident>, Expr)],
        span: Span,
        file: &File,
    ) {
        self.validate_expr(receiver, file);
        for (_, arg) in args {
            self.validate_expr(arg, file);
        }
        let receiver_type = self.infer_type(receiver, file);
        if let Some(fn_def) = self.find_method_fn_def(&receiver_type, &method.name, file) {
            let params = fn_def.params.clone();
            self.validate_fn_param_conventions_receiver(receiver, &params, span, file);
            self.validate_fn_param_conventions_args(&params, args, span, file);
        } else if !self.method_exists_on_type(&receiver_type, &method.name, file) {
            self.errors.push(CompilerError::UndefinedReference {
                name: format!("method '{}' on type '{}'", method.name, receiver_type),
                span,
            });
        }
    }

    /// Find the `FnDef` for `method_name` on the given type by scanning the file's impl blocks.
    fn find_method_fn_def<'f>(
        &self,
        type_name: &str,
        method_name: &str,
        file: &'f File,
    ) -> Option<&'f crate::ast::FnDef> {
        if type_name == "Unknown" || type_name.contains("Unknown") {
            return None;
        }
        for stmt in &file.statements {
            if let crate::ast::Statement::Definition(def) = stmt {
                if let crate::ast::Definition::Impl(impl_def) = &**def {
                    if impl_def.name.name == type_name {
                        for func in &impl_def.functions {
                            if func.name.name == method_name {
                                return Some(func);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Check `mut self` / `sink self` convention against the receiver expression.
    fn validate_fn_param_conventions_receiver(
        &mut self,
        receiver: &Expr,
        params: &[crate::ast::FnParam],
        span: Span,
        file: &File,
    ) {
        use crate::ast::ParamConvention;
        let Some(self_param) = params.iter().find(|p| p.name.name == "self") else {
            return;
        };
        match self_param.convention {
            ParamConvention::Mut => {
                if !self.is_expr_mutable(receiver, file) {
                    self.errors.push(CompilerError::MutabilityMismatch {
                        param: "self".to_string(),
                        span,
                    });
                }
            }
            ParamConvention::Sink => {
                if let Expr::Reference { path, .. } = receiver {
                    if let Some(first) = path.first() {
                        self.consumed_bindings.insert(first.name.clone());
                    }
                }
            }
            ParamConvention::Let => {}
        }
    }

    /// Check `mut` / `sink` conventions on non-self parameters using AST `FnParam` directly.
    fn validate_fn_param_conventions_args(
        &mut self,
        params: &[crate::ast::FnParam],
        args: &[(Option<crate::ast::Ident>, Expr)],
        span: Span,
        file: &File,
    ) {
        use crate::ast::ParamConvention;
        let non_self: Vec<_> = params.iter().filter(|p| p.name.name != "self").collect();
        for (i, (label_opt, arg_expr)) in args.iter().enumerate() {
            let param = label_opt.as_ref().map_or_else(
                || non_self.get(i).copied(),
                |label| {
                    non_self
                        .iter()
                        .find(|p| {
                            p.external_label
                                .as_ref()
                                .is_some_and(|l| l.name == label.name)
                                || p.name.name == label.name
                        })
                        .copied()
                },
            );
            if let Some(param) = param {
                if param.convention == ParamConvention::Mut && !self.is_expr_mutable(arg_expr, file)
                {
                    self.errors.push(CompilerError::MutabilityMismatch {
                        param: param.name.name.clone(),
                        span,
                    });
                }
                if param.convention == ParamConvention::Sink {
                    if let Expr::Reference { path, .. } = arg_expr {
                        if let Some(first) = path.first() {
                            self.consumed_bindings.insert(first.name.clone());
                        }
                    }
                }
            }
        }
    }

    /// Enforce closure param conventions at a call site where the callee is a closure binding.
    fn validate_closure_call_conventions(
        &mut self,
        conventions: &[crate::ast::ParamConvention],
        args: &[(Option<crate::ast::Ident>, Expr)],
        span: Span,
        file: &File,
    ) {
        use crate::ast::ParamConvention;
        for (i, (_, arg_expr)) in args.iter().enumerate() {
            let Some(&convention) = conventions.get(i) else {
                break;
            };
            match convention {
                ParamConvention::Mut => {
                    if !self.is_expr_mutable(arg_expr, file) {
                        self.errors.push(CompilerError::MutabilityMismatch {
                            param: format!("arg{i}"),
                            span,
                        });
                    }
                }
                ParamConvention::Sink => {
                    if let Expr::Reference { path, .. } = arg_expr {
                        if let Some(first) = path.first() {
                            self.consumed_bindings.insert(first.name.clone());
                        }
                    }
                }
                ParamConvention::Let => {}
            }
        }
    }

    /// Validate a block expression (statements + result)
    fn validate_expr_block(&mut self, statements: &[BlockStatement], result: &Expr, file: &File) {
        for stmt in statements {
            match stmt {
                BlockStatement::Let {
                    mutable,
                    pattern,
                    value,
                    ty,
                    ..
                } => {
                    self.validate_expr(value, file);
                    let ty_str = ty
                        .as_ref()
                        .map_or_else(|| self.infer_type(value, file), |t| Self::type_to_string(t));
                    for binding in collect_bindings_from_pattern(pattern) {
                        if let Some(Type::Closure { params, .. }) = ty {
                            let conventions: Vec<_> = params.iter().map(|(c, _)| *c).collect();
                            self.closure_binding_conventions
                                .insert(binding.name.clone(), conventions);
                        }
                        self.local_let_bindings
                            .insert(binding.name, (ty_str.clone(), *mutable));
                    }
                }
                BlockStatement::Assign {
                    target,
                    value,
                    span,
                } => {
                    self.validate_expr(target, file);
                    self.validate_expr(value, file);
                    if !self.is_expr_mutable(target, file) {
                        self.errors
                            .push(CompilerError::AssignmentToImmutable { span: *span });
                    }
                    // Check that value type is compatible with target's declared type
                    let value_type = self.infer_type(value, file);
                    let target_type = self.infer_type(target, file);
                    if !value_type.contains("Unknown")
                        && !target_type.contains("Unknown")
                        && !self.type_strings_compatible(&target_type, &value_type)
                    {
                        self.errors.push(CompilerError::TypeMismatch {
                            expected: target_type,
                            found: value_type,
                            span: *span,
                        });
                    }
                }
                BlockStatement::Expr(expr) => {
                    self.validate_expr(expr, file);
                }
            }
        }
        self.validate_expr(result, file);
    }

    /// Validate struct field requirements: all required fields must be provided, no unknown fields
    pub(super) fn validate_struct_fields(
        &mut self,
        struct_name: &str,
        args: &[(crate::ast::Ident, Expr)],
        span: Span,
        file: &File,
    ) {
        // Find the struct definition in current file or module cache
        // Clone necessary data to avoid borrow checker issues
        let (field_names, required_fields) = {
            if let Some(def) = self.find_struct_def_in_files(struct_name, file) {
                let field_names: Vec<String> =
                    def.fields.iter().map(|f| f.name.name.clone()).collect();

                let required_fields: Vec<String> = def
                    .fields
                    .iter()
                    .filter(|f| {
                        // Field is required if it has no inline default and is not optional
                        f.default.is_none() && !f.optional
                    })
                    .map(|f| f.name.name.clone())
                    .collect();

                (field_names, required_fields)
            } else {
                return; // Struct not found, skip validation
            }
        };

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

        // Skip validation when either operand type is unknown (field access, method calls, etc.)
        if left_type == "Unknown" || right_type == "Unknown" {
            return;
        }

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

    /// Validate for loop collection is an array or range
    pub(super) fn validate_for_loop(&mut self, collection: &Expr, span: Span, file: &File) {
        let collection_type = self.infer_type(collection, file);

        let is_iterable = collection_type.starts_with('[')
            || collection_type.starts_with("Range<")
            || collection_type == "Unknown";

        if !is_iterable {
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

        // Skip destructuring validation when value type is unknown (field access, etc.)
        if value_type == "Unknown" {
            return;
        }

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
            BindingPattern::Tuple { elements, .. } => {
                // Gap 2: Validate tuple pattern arity against tuple type "(x: T, y: U, ...)"
                if let Some(inner) = value_type
                    .strip_prefix('(')
                    .and_then(|s| s.strip_suffix(')'))
                {
                    let field_count = if inner.is_empty() {
                        0
                    } else {
                        inner.split(", ").count()
                    };
                    let pattern_count = elements.len();
                    if pattern_count > field_count && field_count > 0 {
                        self.errors.push(CompilerError::TypeMismatch {
                            expected: format!("tuple with {field_count} field(s)"),
                            found: value_type,
                            span,
                        });
                    }
                }
            }
            BindingPattern::Simple(_) => {
                // Simple patterns don't require type validation here
            }
        }
    }

    /// Validate if condition is boolean or optional
    pub(super) fn validate_if_condition(&mut self, condition: &Expr, span: Span, file: &File) {
        let condition_type = self.infer_type(condition, file);

        // Skip when type is unknown (field access, method calls — IR lowering handles these)
        if condition_type == "Unknown" {
            return;
        }

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

        // Skip when type is unknown (field access, method calls — IR lowering handles these)
        if scrutinee_type == "Unknown" {
            return;
        }

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
        // Clear local let bindings and sink-consumed bindings for this function
        self.local_let_bindings.clear();
        self.consumed_bindings.clear();

        // Register function parameters as local bindings
        for param in &func.params {
            if let Some(ty) = &param.ty {
                self.validate_type(ty);
            }
            let ty_str = param.ty.as_ref().map_or_else(
                || {
                    if param.name.name == "self" {
                        self.current_impl_struct
                            .clone()
                            .unwrap_or_else(|| "Unknown".to_string())
                    } else {
                        "Unknown".to_string()
                    }
                },
                |ty| Self::type_to_string(ty),
            );
            let mutable = matches!(
                param.convention,
                crate::ast::ParamConvention::Mut | crate::ast::ParamConvention::Sink
            );
            self.local_let_bindings
                .insert(param.name.name.clone(), (ty_str, mutable));
        }

        // Validate the function body expression (only if body exists)
        if let Some(body) = &func.body {
            self.validate_expr(body, file);

            // If there's a declared return type, check it matches the body type
            if let Some(declared_return_type) = &func.return_type {
                let body_type = self.infer_type(body, file);
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
        // Clear local let bindings and sink-consumed bindings for this function
        self.local_let_bindings.clear();
        self.consumed_bindings.clear();

        // Register function parameters as local bindings
        for param in &func.params {
            if let Some(ty) = &param.ty {
                self.validate_type(ty);
            }
            let ty_str = param
                .ty
                .as_ref()
                .map_or_else(|| "Unknown".to_string(), |ty| Self::type_to_string(ty));
            let mutable = matches!(
                param.convention,
                crate::ast::ParamConvention::Mut | crate::ast::ParamConvention::Sink
            );
            self.local_let_bindings
                .insert(param.name.name.clone(), (ty_str, mutable));
        }

        // Validate return type if declared
        if let Some(return_type) = &func.return_type {
            self.validate_type(return_type);
        }

        // Validate the function body if present
        if let Some(body) = &func.body {
            self.validate_expr(body, file);

            // If there's a declared return type, check it matches the body type
            if let Some(declared_return_type) = &func.return_type {
                let body_type = self.infer_type(body, file);
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
        }

        // Clear local let bindings after function
        self.local_let_bindings.clear();
    }

    /// Check if a method exists on a given type
    ///
    /// Handles user-defined methods in impl blocks.
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
        }

        false
    }
}
