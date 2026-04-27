use super::module_resolver::ModuleResolver;
use super::sem_type::SemType;
use super::SemanticAnalyzer;
use crate::ast::{
    BinaryOperator, BindingPattern, BlockStatement, Definition, Expr, File, Statement, StructDef,
    Type,
};
use crate::error::CompilerError;
use crate::location::Span;
use std::collections::{HashMap, HashSet};

use super::collect_bindings_from_pattern;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Pass 3: Validate expressions
    /// Validate operators and control flow without evaluation
    pub(super) fn validate_expressions(&mut self, file: &File) {
        for statement in &file.statements {
            match statement {
                Statement::Let(let_binding) => self.validate_let_statement(let_binding, file),
                Statement::Definition(def) => self.validate_definition_expressions(def, file),
                Statement::Use(_) => {}
            }
        }
    }

    /// Dispatch expression validation for a single `Definition`, recursing
    /// through nested modules so that function bodies, struct field defaults,
    /// and impl blocks inside `module { ... }` all receive Pass 3 checks.
    fn validate_definition_expressions(&mut self, def: &Definition, file: &File) {
        match def {
            Definition::Struct(struct_def) => self.validate_struct_expressions(struct_def, file),
            Definition::Impl(impl_def) => self.validate_impl_expressions(impl_def, file),
            Definition::Function(func_def) => self.validate_function_body(func_def, file),
            Definition::Module(module_def) => {
                for nested_def in &module_def.definitions {
                    self.validate_definition_expressions(nested_def, file);
                }
            }
            Definition::Trait(_) | Definition::Enum(_) => {}
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "validates several let-binding rules in sequence; splitting them obscures the shared `declared`/`inferred` derivation"
    )]
    fn validate_let_statement(&mut self, let_binding: &crate::ast::LetBinding, file: &File) {
        if let Some(type_ann) = &let_binding.type_annotation {
            // Tier-1 item E2: surface
            // `let x: SomeTrait = ...` as TraitUsedAsValueType (and
            // any other invalid type in the annotation) before the
            // value/declared compatibility check would mask it.
            self.validate_type(type_ann);
        }
        self.validate_expr(&let_binding.value, file);
        // nil literals must not be assigned to non-optional types, and
        // (audit2 B12) the inferred value type must be compatible with
        // the declared annotation. Previously only the nil case was
        // checked, so `let f: m::Foo = "wrong"` compiled silently.
        if let Some(type_ann) = &let_binding.type_annotation {
            let declared = Self::type_to_string(type_ann);
            let inferred_sem = self.infer_type_sem(&let_binding.value, file);
            let inferred = inferred_sem.display();
            if matches!(inferred_sem, SemType::Nil) && !declared.ends_with('?') {
                self.errors.push(CompilerError::NilAssignedToNonOptional {
                    expected: declared,
                    span: let_binding.span,
                });
            } else {
                // General type-mismatch check, mirroring the field-default
                // rule in `validate_struct_expressions`.
                let nil_to_optional =
                    matches!(inferred_sem, SemType::Nil) && declared.ends_with('?');
                let inner_to_optional =
                    declared.ends_with('?') && declared.trim_end_matches('?') == inferred.as_str();
                let is_closure_pair = matches!(type_ann, Type::Closure { .. })
                    && matches!(let_binding.value, Expr::ClosureExpr { .. });
                if !nil_to_optional
                    && !inner_to_optional
                    && !inferred_sem.is_indeterminate()
                    && declared != "Unknown"
                    && !is_closure_pair
                    && !self.type_strings_compatible(&declared, &inferred)
                {
                    self.errors.push(CompilerError::TypeMismatch {
                        expected: declared,
                        found: inferred,
                        span: let_binding.span,
                    });
                }
            }
        }
        // Audit2 B9: when a let binding declares a closure type and is
        // assigned a closure literal, type-check the closure body
        // bidirectionally — push the declared param types into the
        // inference scope (covering literal params with no annotation)
        // and verify the body's inferred type matches the declared
        // return type. Without this, untyped closure params silently
        // resolve to "Unknown", which unifies with anything and
        // suppresses real return-type mismatches.
        if let (
            Some(Type::Closure {
                params: declared_params,
                ret: declared_ret,
            }),
            Expr::ClosureExpr {
                params: lit_params,
                body,
                return_type: lit_ret,
                ..
            },
        ) = (&let_binding.type_annotation, &let_binding.value)
        {
            if lit_params.len() == declared_params.len() {
                let mut seed = HashMap::new();
                for (lit, (_, dty)) in lit_params.iter().zip(declared_params.iter()) {
                    let ty_str = lit
                        .ty
                        .as_ref()
                        .map_or_else(|| Self::type_to_string(dty), Self::type_to_string);
                    seed.insert(lit.name.name.clone(), ty_str);
                }
                self.inference_scope_stack.borrow_mut().push(seed);
                let inferred_body_sem = self.infer_type_sem(body, file);
                self.inference_scope_stack.borrow_mut().pop();
                let expected_ret = lit_ret
                    .as_ref()
                    .map_or_else(|| Self::type_to_string(declared_ret), Self::type_to_string);
                if !inferred_body_sem.is_indeterminate()
                    && !self.type_strings_compatible(&expected_ret, &inferred_body_sem.display())
                {
                    self.errors.push(CompilerError::TypeMismatch {
                        expected: expected_ret,
                        found: inferred_body_sem.display(),
                        span: let_binding.span,
                    });
                }
            }
        }
        // Register closure-typed module-level bindings for call-site enforcement
        if let Some(Type::Closure { params, .. }) = &let_binding.type_annotation {
            let conventions: Vec<_> = params.iter().map(|(c, _)| *c).collect();
            // If the value is a closure literal, record its free
            // variables so we can detect use-after-sink at call sites.
            let captures = if let Expr::ClosureExpr {
                params: cparams,
                body,
                ..
            } = &let_binding.value
            {
                let param_set: HashSet<String> =
                    cparams.iter().map(|p| p.name.name.clone()).collect();
                Some(Self::collect_free_variables(body, &param_set))
            } else {
                None
            };
            for binding in collect_bindings_from_pattern(&let_binding.pattern) {
                self.closure_binding_conventions
                    .insert(binding.name.clone(), conventions.clone());
                if let Some(caps) = &captures {
                    self.closure_binding_captures
                        .insert(binding.name.clone(), caps.clone());
                    self.fn_scope_closure_captures
                        .insert(binding.name, caps.clone());
                }
            }
        }
        self.validate_destructuring_pattern(
            &let_binding.pattern,
            &let_binding.value,
            let_binding.span,
            file,
        );
    }

    fn validate_impl_expressions(&mut self, impl_def: &crate::ast::ImplDef, file: &File) {
        // Push the impl's generic scope (merging target struct/enum
        // generics) so method bodies see trait bounds on type
        // parameters during expression validation. Audit #4/#27.
        self.push_impl_generic_scope(&impl_def.generics, &impl_def.name.name);
        self.current_impl_struct = Some(impl_def.name.name.clone());
        self.local_let_bindings.clear();
        self.consumed_bindings.clear();
        for func in &impl_def.functions {
            self.validate_function_return_type(func, file);
        }
        self.current_impl_struct = None;
        self.local_let_bindings.clear();
        self.consumed_bindings.clear();
        self.pop_generic_scope();
    }

    fn validate_function_body(&mut self, func_def: &crate::ast::FunctionDef, file: &File) {
        // Push the function's generic params so uses of `T` inside the
        // body and in param/return annotations don't trip the
        // OutOfScopeTypeParameter check.
        self.push_generic_scope(&func_def.generics);
        self.local_let_bindings.clear();
        self.consumed_bindings.clear();
        // Snapshot closure-binding maps so entries introduced in
        // this function body don't leak into later functions.
        let saved_closure_conventions = self.closure_binding_conventions.clone();
        let saved_closure_captures = self.closure_binding_captures.clone();
        let saved_fn_scope_captures = std::mem::take(&mut self.fn_scope_closure_captures);
        let saved_param_conventions = self.current_fn_param_conventions.clone();
        self.current_fn_param_conventions.clear();
        for param in &func_def.params {
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
            self.current_fn_param_conventions
                .insert(param.name.name.clone(), param.convention);
            // Register closure-typed parameters so they're callable inside the body.
            // Parameters have no captures of their own — no closure_binding_captures entry.
            if let Some(Type::Closure {
                params: closure_params,
                ..
            }) = &param.ty
            {
                let conventions: Vec<_> = closure_params.iter().map(|(c, _)| *c).collect();
                self.closure_binding_conventions
                    .insert(param.name.name.clone(), conventions);
            }
        }
        if let Some(body) = &func_def.body {
            self.validate_expr(body, file);
            self.validate_function_return_escape(func_def.return_type.as_ref(), body);
        }
        self.local_let_bindings.clear();
        self.consumed_bindings.clear();
        self.closure_binding_conventions = saved_closure_conventions;
        self.closure_binding_captures = saved_closure_captures;
        self.fn_scope_closure_captures = saved_fn_scope_captures;
        self.current_fn_param_conventions = saved_param_conventions;
        self.pop_generic_scope();
    }

    /// Validate expressions in struct field defaults
    pub(super) fn validate_struct_expressions(&mut self, struct_def: &StructDef, file: &File) {
        // Validate field defaults
        for field in &struct_def.fields {
            if let Some(default_expr) = &field.default {
                self.validate_expr(default_expr, file);
                // Check that the default expression type matches the declared field type
                let inferred_sem = self.infer_type_sem(default_expr, file);
                let inferred = inferred_sem.display();
                let declared = Self::type_to_string(&field.ty);
                // nil is compatible with any optional type
                let nil_to_optional =
                    matches!(inferred_sem, SemType::Nil) && declared.ends_with('?');
                // a value of type T is compatible with T? (implicit wrapping)
                let inner_to_optional =
                    declared.ends_with('?') && declared.trim_end_matches('?') == inferred.as_str();
                if !nil_to_optional
                    && !inner_to_optional
                    && !inferred_sem.is_indeterminate()
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
            Expr::Literal { .. } => {}
            Expr::Array { elements, span } => {
                for elem in elements {
                    self.validate_expr(elem, file);
                }
                // Escape analysis: any closure value stored in the array escapes
                // with the collection — mark its captures as consumed.
                for elem in elements {
                    self.escape_closure_value(elem);
                }
                // Audit #41: unify the element types so a heterogeneous
                // array literal (`[1, "two"]`) surfaces as a real
                // TypeMismatch instead of silently using the first
                // element's type.
                self.validate_array_homogeneity(elements, *span, file);
            }
            Expr::Tuple { fields, .. } => {
                for (_, field_expr) in fields {
                    self.validate_expr(field_expr, file);
                }
                // Escape analysis: closure values stored in a tuple escape.
                for (_, field_expr) in fields {
                    self.escape_closure_value(field_expr);
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
                // Audit #22: when the condition is an optional receiver
                // like `if foo` or `if user.nickname`, introduce the
                // unwrapped binding into the then-branch scope so the body
                // can reference it by its trailing name.
                let (auto_binding_name, auto_binding_prev) =
                    self.bind_optional_auto_binding(condition, file);
                // Each branch is a separate control-flow path. Snapshot
                // consumed_bindings before entering either branch so we can
                // compute the conservative post-join union. Conservative (never
                // unsound): may produce false-positive UseAfterSink but never
                // miss one.
                let pre_if = self.consumed_bindings.clone();
                self.validate_expr(then_branch, file);
                // Restore any auto-binding we installed before entering the
                // else branch (which does not see the unwrapped binding).
                if let Some(name) = auto_binding_name.as_ref() {
                    match auto_binding_prev {
                        Some(prev) => {
                            self.local_let_bindings.insert(name.clone(), prev);
                        }
                        None => {
                            self.local_let_bindings.remove(name);
                        }
                    }
                }
                // after_then takes over `self.consumed_bindings`; swap pre_if in
                // so the else branch starts from pre-branch state.
                let after_then = std::mem::replace(&mut self.consumed_bindings, pre_if);
                if let Some(else_expr) = else_branch {
                    self.validate_expr(else_expr, file);
                    // Check that both branch types are compatible.
                    // Widening rules: T and Nil unify to T?; T and T? unify to T?.
                    let then_sem = self.infer_type_sem(then_branch, file);
                    let else_sem = self.infer_type_sem(else_expr, file);
                    // Skip when either type is indeterminate (Unknown / nested Unknown / InferredEnum).
                    if !then_sem.is_indeterminate()
                        && !else_sem.is_indeterminate()
                        && !SemType::unifies_with_optional_widening(&then_sem, &else_sem)
                    {
                        let then_type = then_sem.display();
                        let else_type = else_sem.display();
                        if !self.type_strings_compatible(&then_type, &else_type) {
                            self.errors.push(CompilerError::TypeMismatch {
                                expected: then_type,
                                found: else_type,
                                span: *span,
                            });
                        }
                    }
                }
                // Current state = after_else (or pre_if if no else branch).
                // Fold in after_then to produce union.
                self.consumed_bindings.extend(after_then);
                self.validate_if_condition(condition, *span, file);
            }
            Expr::MatchExpr {
                scrutinee,
                arms,
                span,
            } => {
                self.validate_expr(scrutinee, file);
                let pre_match = self.consumed_bindings.clone();
                let mut post_union: HashSet<String> = HashSet::new();
                let mut arm_sems: Vec<SemType> = Vec::new();
                for arm in arms {
                    self.consumed_bindings.clone_from(&pre_match);
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
                    arm_sems.push(self.infer_type_sem(&arm.body, file));
                    // Drain the per-arm state into post_union without cloning.
                    post_union.extend(self.consumed_bindings.drain());
                }
                // Include pre_match (pass-through when no arm is taken).
                post_union.extend(pre_match);
                self.consumed_bindings = post_union;
                // Check that all arm types are compatible with the first arm's type.
                // Widening: variations of T and T?/Nil unify to T?.
                if let Some(first_sem) = arm_sems.first().cloned() {
                    if !first_sem.is_indeterminate() {
                        let first_type = first_sem.display();
                        for (arm, arm_sem) in arms.iter().zip(arm_sems.iter()).skip(1) {
                            if arm_sem.is_indeterminate()
                                || SemType::unifies_with_optional_widening(&first_sem, arm_sem)
                            {
                                continue;
                            }
                            let arm_type = arm_sem.display();
                            if !self.type_strings_compatible(&first_type, &arm_type) {
                                self.errors.push(CompilerError::TypeMismatch {
                                    expected: first_type.clone(),
                                    found: arm_type,
                                    span: arm.span,
                                });
                            }
                        }
                    }
                }
                self.validate_match(scrutinee, arms, *span, file);
            }
            Expr::Group { expr, .. } => self.validate_expr(expr, file),
            Expr::DictLiteral { entries, span, .. } => {
                for (key, value) in entries {
                    self.validate_expr(key, file);
                    self.validate_expr(value, file);
                }
                // Escape analysis: closure values stored as dict keys/values escape.
                for (key, value) in entries {
                    self.escape_closure_value(key);
                    self.escape_closure_value(value);
                }
                // Audit2 B11: unify key types and value types across
                // entries so a heterogeneous dict literal
                // (`["a": 1, "b": "two"]`) surfaces as a real
                // TypeMismatch instead of silently using the first
                // entry's type.
                self.validate_dict_homogeneity(entries, *span, file);
            }
            Expr::DictAccess { dict, key, span } => {
                self.validate_expr(dict, file);
                self.validate_expr(key, file);
                // Validate key type against declared dict type.
                // Structural unpacking — no string scanning needed.
                if let SemType::Dictionary {
                    key: expected_key, ..
                } = self.infer_type_sem(dict, file)
                {
                    let actual_key_sem = self.infer_type_sem(key, file);
                    if !actual_key_sem.is_unknown() && actual_key_sem != *expected_key {
                        self.errors.push(CompilerError::TypeMismatch {
                            expected: expected_key.display(),
                            found: actual_key_sem.display(),
                            span: *span,
                        });
                    }
                }
            }
            Expr::FieldAccess {
                object,
                field,
                span,
            } => {
                self.validate_expr(object, file);
                let obj_sem = self.infer_type_sem(object, file);
                if !obj_sem.is_unknown() {
                    // Field access on an optional type requires unwrapping
                    if let SemType::Optional(inner) = &obj_sem {
                        let base = inner.display();
                        if base != "Unknown" && self.symbols.get_struct(&base).is_some() {
                            self.errors.push(CompilerError::OptionalUsedAsNonOptional {
                                actual: obj_sem.display(),
                                expected: base,
                                span: *span,
                            });
                        }
                    } else {
                        // Field must exist on the struct
                        let base_type = obj_sem.display();
                        if let Some(struct_info) = self.symbols.get_struct(&base_type) {
                            if !struct_info.fields.iter().any(|f| f.name == field.name) {
                                self.errors.push(CompilerError::UnknownField {
                                    field: field.name.clone(),
                                    type_name: base_type,
                                    span: field.span,
                                });
                            }
                        }
                    }
                }
            }
            Expr::ClosureExpr {
                params,
                return_type,
                body,
                ..
            } => {
                self.validate_expr_closure(params, return_type.as_ref(), body, file);
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

    /// Return the name of the leftmost (root) binding referenced by `expr`, if any.
    ///
    /// Walks through `FieldAccess`, `Group`, and `Reference` nodes to find the
    /// root identifier. Returns `None` for expressions that don't reference a
    /// binding (literals, calls, etc.) — those are new values, not places.
    ///
    /// Used to mark the root binding as consumed when a compound expression
    /// (e.g., `x.field`) is passed to a sink parameter.
    fn root_binding(expr: &Expr) -> Option<String> {
        match expr {
            Expr::Reference { path, .. } => path.first().map(|id| id.name.clone()),
            Expr::FieldAccess { object, .. } => Self::root_binding(object),
            Expr::Group { expr, .. } => Self::root_binding(expr),
            Expr::Literal { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
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
            | Expr::LetExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Block { .. } => None,
        }
    }

    /// Extract the set of captures for an escaping closure value.
    ///
    /// - `Reference` to a tracked closure binding → its recorded captures.
    /// - `ClosureExpr` literal → free variables of the literal body.
    /// - `Group` → recurse on the inner expression.
    ///
    /// Returns `None` if `expr` is not a closure value in a form we can handle.
    fn closure_captures_of_expr(&self, expr: &Expr) -> Option<Vec<String>> {
        match expr {
            Expr::Reference { path, .. } => {
                if path.len() != 1 {
                    return None;
                }
                let name = &path.first()?.name;
                self.closure_binding_captures.get(name).cloned()
            }
            Expr::ClosureExpr { params, body, .. } => {
                let param_set: HashSet<String> =
                    params.iter().map(|p| p.name.name.clone()).collect();
                Some(Self::collect_free_variables(body, &param_set))
            }
            Expr::Group { expr, .. } => self.closure_captures_of_expr(expr),
            Expr::Literal { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
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
            | Expr::FieldAccess { .. }
            | Expr::LetExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Block { .. } => None,
        }
    }

    /// Mark the captures of an escaping closure as consumed.
    ///
    /// Given an initial list of captured names, walks transitively through
    /// `closure_binding_captures`: if any captured name is itself a tracked
    /// closure binding, its captures are included too. Each reached name is
    /// inserted into `consumed_bindings`. A visited set prevents infinite
    /// recursion on cyclic capture chains.
    fn mark_captures_consumed(&mut self, initial: &[String]) {
        let mut visited: HashSet<String> = HashSet::new();
        let mut stack: Vec<String> = initial.to_vec();
        while let Some(name) = stack.pop() {
            if !visited.insert(name.clone()) {
                continue;
            }
            // If `name` itself names a tracked closure binding, recurse into its captures.
            if let Some(nested) = self.closure_binding_captures.get(&name).cloned() {
                for cap in nested {
                    if !visited.contains(&cap) {
                        stack.push(cap);
                    }
                }
            }
            self.consumed_bindings.insert(name);
        }
    }

    /// Escape helper: if `expr` is a closure value (named binding or literal),
    /// mark its captures as consumed transitively.
    ///
    /// Used at escape sites: sink-pass, struct field assignment, array/dict
    /// element, and similar positions where the closure's owning scope changes.
    fn escape_closure_value(&mut self, expr: &Expr) {
        if let Some(caps) = self.closure_captures_of_expr(expr) {
            self.mark_captures_consumed(&caps);
        }
    }

    /// Validate that every key (and every value) in a dict literal has
    /// a compatible type with the first entry. Audit2 B11: previously
    /// the dict's type came from the first entry only and a mix like
    /// `["a": 1, "b": "two"]` lowered silently.
    fn validate_dict_homogeneity(&mut self, entries: &[(Expr, Expr)], span: Span, file: &File) {
        let mut iter = entries.iter();
        let Some((first_k, first_v)) = iter.next() else {
            return; // empty dict: nothing to unify
        };
        let first_key_sem = self.infer_type_sem(first_k, file);
        let first_val_sem = self.infer_type_sem(first_v, file);
        let key_indeterminate = first_key_sem.is_unknown();
        let val_indeterminate = first_val_sem.is_unknown();
        let first_key_ty = first_key_sem.display();
        let first_val_ty = first_val_sem.display();
        for (k, v) in iter {
            if !key_indeterminate {
                let kty_sem = self.infer_type_sem(k, file);
                if !kty_sem.is_unknown() {
                    let kty = kty_sem.display();
                    if !self.type_strings_compatible(&first_key_ty, &kty) {
                        self.errors.push(CompilerError::TypeMismatch {
                            expected: format!("[{first_key_ty}: {first_val_ty}]"),
                            found: format!("key of type {kty}"),
                            span,
                        });
                        return;
                    }
                }
            }
            if !val_indeterminate {
                let vty_sem = self.infer_type_sem(v, file);
                if !vty_sem.is_unknown() {
                    let vty = vty_sem.display();
                    if !self.type_strings_compatible(&first_val_ty, &vty) {
                        self.errors.push(CompilerError::TypeMismatch {
                            expected: format!("[{first_key_ty}: {first_val_ty}]"),
                            found: format!("value of type {vty}"),
                            span,
                        });
                        return;
                    }
                }
            }
        }
    }

    /// Validate that every element of an array literal has a compatible
    /// type. Audit #41: previously the array's type came from the first
    /// element only and a mix like `[1, "two"]` lowered silently.
    fn validate_array_homogeneity(&mut self, elements: &[Expr], span: Span, file: &File) {
        let mut iter = elements.iter();
        let Some(first) = iter.next() else {
            return; // empty array: nothing to unify
        };
        let first_sem = self.infer_type_sem(first, file);
        if first_sem.is_unknown() {
            // Can't trust the inference; skip rather than emit noise.
            return;
        }
        let first_ty = first_sem.display();
        for elem in iter {
            let elem_sem = self.infer_type_sem(elem, file);
            if elem_sem.is_unknown() {
                continue;
            }
            let elem_ty = elem_sem.display();
            if !self.type_strings_compatible(&first_ty, &elem_ty) {
                self.errors.push(CompilerError::TypeMismatch {
                    expected: format!("[{first_ty}]"),
                    found: format!("element of type {elem_ty}"),
                    span,
                });
                // Stop after the first mismatch so a single typo doesn't
                // cascade into N copies of the same diagnostic.
                break;
            }
        }
    }

    /// Walk the body's result expression and collect every closure value that
    /// would escape the function via `return` along with its captures.
    ///
    /// A closure "escapes via return" if it is the outermost value of the
    /// function body. That may be a direct `ClosureExpr`, a `Reference` to a
    /// closure-typed let binding, or a closure reachable through a `Block`,
    /// `LetExpr`, `IfExpr`, or `MatchExpr` result. Returns one `(captures,
    /// span)` entry per escaping closure (if/match branches contribute one
    /// entry per branch so per-branch error reporting is possible).
    fn collect_returned_closure_captures(&self, expr: &Expr) -> Vec<(Vec<String>, Span)> {
        let mut results: Vec<(Vec<String>, Span)> = Vec::new();
        self.collect_returned_closure_captures_rec(expr, &mut results);
        results
    }

    fn collect_returned_closure_captures_rec(
        &self,
        expr: &Expr,
        out: &mut Vec<(Vec<String>, Span)>,
    ) {
        match expr {
            Expr::ClosureExpr {
                params, body, span, ..
            } => {
                let param_set: HashSet<String> =
                    params.iter().map(|p| p.name.name.clone()).collect();
                let caps = Self::collect_free_variables(body, &param_set);
                out.push((caps, *span));
            }
            Expr::Reference { path, span } => {
                if path.len() == 1 {
                    if let Some(first) = path.first() {
                        // Prefer the flat function-scope map so bindings
                        // introduced inside a now-popped nested block still
                        // carry their captures for the return-escape check.
                        if let Some(caps) = self
                            .fn_scope_closure_captures
                            .get(&first.name)
                            .or_else(|| self.closure_binding_captures.get(&first.name))
                        {
                            out.push((caps.clone(), *span));
                        }
                    }
                }
            }
            Expr::Group { expr, .. } => {
                self.collect_returned_closure_captures_rec(expr, out);
            }
            Expr::Block { result, .. } => {
                self.collect_returned_closure_captures_rec(result, out);
            }
            Expr::LetExpr { body, .. } => {
                self.collect_returned_closure_captures_rec(body, out);
            }
            Expr::IfExpr {
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_returned_closure_captures_rec(then_branch, out);
                if let Some(else_expr) = else_branch {
                    self.collect_returned_closure_captures_rec(else_expr, out);
                }
            }
            Expr::MatchExpr { arms, .. } => {
                for arm in arms {
                    self.collect_returned_closure_captures_rec(&arm.body, out);
                }
            }
            // Tier-1 escape extension: a closure stored into a struct
            // / enum field that becomes part of the returned aggregate
            // also escapes via return. Walk constructor args, but only
            // when the path resolves to a struct (or the enum variant
            // is named) — function-call invocations don't return their
            // arguments and would over-trigger.
            Expr::Invocation { path, args, .. } => {
                let is_struct = path
                    .last()
                    .is_some_and(|seg| self.symbols.get_struct(&seg.name).is_some());
                if is_struct {
                    for (_, arg) in args {
                        self.collect_returned_closure_captures_rec(arg, out);
                    }
                }
            }
            Expr::EnumInstantiation { data, .. } | Expr::InferredEnumInstantiation { data, .. } => {
                for (_, field_expr) in data {
                    self.collect_returned_closure_captures_rec(field_expr, out);
                }
            }
            Expr::Tuple { fields, .. } => {
                for (_, field_expr) in fields {
                    self.collect_returned_closure_captures_rec(field_expr, out);
                }
            }
            Expr::Array { elements, .. } => {
                for elem in elements {
                    self.collect_returned_closure_captures_rec(elem, out);
                }
            }
            Expr::DictLiteral { entries, .. } => {
                for (k, v) in entries {
                    self.collect_returned_closure_captures_rec(k, out);
                    self.collect_returned_closure_captures_rec(v, out);
                }
            }
            Expr::Literal { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::DictAccess { .. }
            | Expr::FieldAccess { .. }
            | Expr::MethodCall { .. } => {}
        }
    }

    /// If `return_type` is a closure type, verify that every closure returned
    /// by `body` only captures bindings that outlive the function: outer-scope
    /// bindings (module-level or wider) and `sink` parameters. Local `let`
    /// bindings and `let`/`mut` parameters would die with the function frame
    /// and leave a dangling capture.
    fn validate_function_return_escape(&mut self, return_type: Option<&Type>, body: &Expr) {
        // The legacy fast-path: function returns a closure type directly
        // (`fn make() -> () -> Number`). The recursive walk handles every
        // concrete return shape — closure literals, references to
        // closure bindings, branches, blocks. Tier-1 escape extension
        // also fires on aggregate returns (struct / enum / tuple /
        // array / dict): walking those is harmless when no closure
        // hides inside them, since `collect_returned_closure_captures`
        // simply returns an empty list.
        let return_carries_aggregate = matches!(
            return_type,
            Some(
                Type::Closure { .. }
                    | Type::Ident(_)
                    | Type::Generic { .. }
                    | Type::Tuple(_)
                    | Type::Array(_)
                    | Type::Optional(_)
                    | Type::Dictionary { .. }
            )
        );
        if !return_carries_aggregate {
            return;
        }
        let escaping = self.collect_returned_closure_captures(body);
        if escaping.is_empty() {
            return;
        }
        for (captures, span) in escaping {
            self.validate_escaping_captures(&captures, span);
        }
    }

    /// Shared rule for "this closure value escapes the function frame".
    ///
    /// Captures are valid only when they refer to:
    ///
    /// - a `sink` parameter (ownership transfers into the closure;
    ///   binding is marked consumed),
    /// - a module-level `let` (outlives the function).
    ///
    /// `let`/`mut` parameters and function-local `let` bindings die
    /// with the frame and produce
    /// [`CompilerError::ClosureCaptureEscapesLocalBinding`].
    fn validate_escaping_captures(&mut self, captures: &[String], span: Span) {
        let param_convs = self.current_fn_param_conventions.clone();
        for cap in captures {
            if let Some(convention) = param_convs.get(cap) {
                match convention {
                    crate::ast::ParamConvention::Sink => {
                        self.consumed_bindings.insert(cap.clone());
                    }
                    crate::ast::ParamConvention::Let | crate::ast::ParamConvention::Mut => {
                        self.errors
                            .push(CompilerError::ClosureCaptureEscapesLocalBinding {
                                binding: cap.clone(),
                                span,
                            });
                    }
                }
            } else if self.symbols.is_let(cap) {
                // Module-level let — outlives the function. OK.
            } else {
                // Function-local `let` (or any other shorter-lifetime
                // binding the block scope has popped by now). Dies
                // with the frame.
                self.errors
                    .push(CompilerError::ClosureCaptureEscapesLocalBinding {
                        binding: cap.clone(),
                        span,
                    });
            }
        }
    }

    /// Check module visibility for a multi-segment path (`mod::item`,
    /// `outer::inner::item`, etc.).
    ///
    /// Walks the full module path, checking:
    /// 1. Each intermediate module segment must be `pub` to be accessible
    ///    across module boundaries.
    /// 2. The final item must be `pub` when accessed across any module boundary.
    ///
    /// Returns true if access is allowed, false if a `VisibilityViolation`
    /// was emitted.
    fn check_module_visibility(&mut self, path: &[crate::ast::Ident], span: Span) -> bool {
        let Some((first, rest)) = path.split_first() else {
            return true;
        };
        if rest.is_empty() {
            return true;
        }
        let Some(root_module) = self.symbols.modules.get(first.name.as_str()) else {
            return true;
        };
        // Walk intermediate modules (all rest segments except the last).
        // Each intermediate module must itself be `pub`.
        let mut current = &root_module.symbols;
        let Some((item_ident, middle)) = rest.split_last() else {
            return true;
        };
        for seg in middle {
            let name = seg.name.as_str();
            let Some(next) = current.modules.get(name) else {
                // Unknown module: leave error reporting to the caller.
                return true;
            };
            if matches!(next.visibility, crate::ast::Visibility::Private) {
                self.errors.push(CompilerError::VisibilityViolation {
                    name: name.to_string(),
                    span,
                });
                return false;
            }
            current = &next.symbols;
        }
        // Final segment is the item name
        let item_name = item_ident.name.as_str();
        let item_visibility = current
            .structs
            .get(item_name)
            .map(|s| s.visibility)
            .or_else(|| {
                current
                    .functions
                    .get(item_name)
                    .and_then(|overloads| overloads.first().map(|f| f.visibility))
            })
            .or_else(|| current.enums.get(item_name).map(|e| e.visibility))
            .or_else(|| current.traits.get(item_name).map(|t| t.visibility))
            .or_else(|| current.lets.get(item_name).map(|l| l.visibility))
            .or_else(|| current.modules.get(item_name).map(|m| m.visibility));

        if matches!(item_visibility, Some(crate::ast::Visibility::Private)) {
            self.errors.push(CompilerError::VisibilityViolation {
                name: item_name.to_string(),
                span,
            });
            return false;
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
            return;
        }

        // Multi-segment paths (e.g. `p.x.y`): walk each segment as a field
        // access from the root's inferred type and surface an
        // `UnknownField` error at the first broken link. Module-qualified
        // paths (handled above by `check_module_visibility`) fall through
        // this validation without firing since no let/local binding with
        // that name will be in scope.
        if path.len() >= 2 {
            let Some(first) = path.first() else {
                return;
            };
            // Root must be something we can infer a type for.
            let root_type_string = if let Some(ty) = self.symbols.get_let_type(&first.name) {
                ty.to_string()
            } else if let Some((ty, _)) = self.local_let_bindings.get(&first.name) {
                ty.clone()
            } else {
                return;
            };
            if let Some(rest) = path.get(1..) {
                self.validate_field_chain(&root_type_string, rest, span);
            }
        }
    }

    /// Walk a chain of field accesses starting from `root_type`, emitting
    /// `UnknownField` at the first segment that does not name a field of
    /// the current struct type. Bails silently if the type cannot be
    /// resolved — type inference is best-effort and we don't want to
    /// drown the user in spurious errors when inference itself is
    /// unreliable.
    fn validate_field_chain(&mut self, root_type: &str, rest: &[crate::ast::Ident], span: Span) {
        let mut current = root_type.trim_end_matches('?').to_string();
        for seg in rest {
            let Some(struct_info) = self.symbols.get_struct(&current) else {
                return;
            };
            if let Some(field) = struct_info.fields.iter().find(|f| f.name == seg.name) {
                current = Self::type_to_string(&field.ty)
                    .trim_end_matches('?')
                    .to_string();
            } else {
                self.errors.push(CompilerError::UnknownField {
                    field: seg.name.clone(),
                    type_name: current.clone(),
                    span,
                });
                return;
            }
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
            if expected == actual {
                // Validate each type arg satisfies its constraints
                for (type_arg, generic_param) in type_args.iter().zip(expected_params.iter()) {
                    for constraint in &generic_param.constraints {
                        let crate::ast::GenericConstraint::Trait {
                            name: trait_ref, ..
                        } = constraint;
                        if !self.type_satisfies_trait_constraint(type_arg, &trait_ref.name) {
                            self.errors.push(CompilerError::GenericConstraintViolation {
                                arg: Self::type_to_string(type_arg),
                                constraint: trait_ref.name.clone(),
                                span,
                            });
                        }
                    }
                }
            } else if actual == 0 && expected > 0 {
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
    #[expect(
        clippy::too_many_lines,
        reason = "covers generic-arity checks, overload resolution, closure binding checks (conventions + captures) — splitting hurts readability"
    )]
    fn validate_expr_invocation_function(
        &mut self,
        name: &str,
        type_args: &[crate::ast::Type],
        args: &[(Option<crate::ast::Ident>, crate::ast::Expr)],
        span: Span,
        file: &File,
    ) {
        // Validate generic type arguments against the function's generic parameters
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
                        let crate::ast::GenericConstraint::Trait {
                            name: trait_ref, ..
                        } = constraint;
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
                    // Before applying param conventions (which may mark new bindings
                    // as consumed), check if any captured binding has already been
                    // consumed — that's an after-the-fact use-after-sink via the
                    // closure.
                    if let Some(captures) = self.closure_binding_captures.get(simple_name).cloned()
                    {
                        for captured in &captures {
                            if self.consumed_bindings.contains(captured) {
                                self.errors.push(CompilerError::UseAfterSink {
                                    name: captured.clone(),
                                    span,
                                });
                            }
                        }
                    }
                    self.validate_closure_call_conventions(&conventions, args, span, file);
                } else if !self.resolve_qualified_function(name) {
                    // Audit #42: a missing function is an undefined
                    // reference, not an undefined type — use the correct
                    // error variant so downstream tooling can distinguish
                    // the two cases.
                    self.errors.push(CompilerError::UndefinedReference {
                        name: name.to_string(),
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
                    if let Some(root) = Self::root_binding(arg_expr) {
                        self.consumed_bindings.insert(root);
                    }
                    // Escape analysis: a closure value passed to a sink param
                    // escapes with its captures — mark them consumed.
                    self.escape_closure_value(arg_expr);
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
        } else if none_labeled && args.is_empty() {
            // Zero-arg call: match only zero-arg overloads.
            // Without context-type disambiguation (e.g., from a let annotation),
            // multiple zero-arg overloads will be reported as AmbiguousCall by the
            // caller. This is the scope-limited behavior — see Fix 6 notes.
            params.iter().filter(|p| p.name.name != "self").count() == 0
        } else if none_labeled && !args.is_empty() {
            // Mode B: arity check first, then match by first-argument type
            let non_self_count = params.iter().filter(|p| p.name.name != "self").count();
            if args.len() != non_self_count {
                return false;
            }

            let first_arg_sem = args.first().map_or(SemType::Unknown, |(_, expr)| {
                self.infer_type_sem(expr, file)
            });

            let first_param_type = params
                .iter()
                .find(|p| p.name.name != "self")
                .and_then(|p| p.ty.as_ref())
                .map_or_else(|| "Unknown".to_string(), Self::type_to_string);

            // Unknown means we can't tell — accept it (conservative)
            first_arg_sem.is_unknown()
                || first_param_type == "Unknown"
                || self.type_strings_compatible(&first_param_type, &first_arg_sem.display())
        } else {
            // Mixed labeled / unlabeled args — reject. FormaLang's overload
            // resolution modes are "all-labeled" (A) or "all-unlabeled" (B);
            // a mix has no defined match and previously fell through to
            // `true`, silently accepting overloads whose label patterns were
            // incompatible. See audit finding #14.
            false
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
    ///
    /// Checks that the closure body does not capture any binding that has
    /// already been consumed by a sink parameter at closure-creation time.
    /// The complementary after-the-fact check — closure created with a live
    /// capture, capture consumed later, then closure invoked — fires at the
    /// invocation site (see the `closure_binding_captures` lookup in the
    /// closure-call branch of `validate_expr_invocation`), so dormant
    /// closures whose captures are consumed but never invoked are tolerated
    /// by design.
    fn validate_expr_closure(
        &mut self,
        params: &[crate::ast::ClosureParam],
        return_type: Option<&crate::ast::Type>,
        body: &Expr,
        file: &File,
    ) {
        for param in params {
            if let Some(ty) = &param.ty {
                self.validate_type(ty);
            }
        }
        if let Some(ty) = return_type {
            self.validate_type(ty);
        }
        let mut param_scope = HashSet::new();
        for param in params {
            param_scope.insert(param.name.name.clone());
        }
        // Detect closure bodies referencing bindings already consumed by a sink.
        let consumed = self.consumed_bindings.clone();
        let mut inner_scopes: Vec<HashSet<String>> = Vec::new();
        Self::check_captures_rec(
            body,
            &param_scope,
            &consumed,
            &mut self.errors,
            &mut inner_scopes,
        );
        self.closure_param_scopes.push(param_scope);
        self.validate_expr(body, file);
        self.closure_param_scopes.pop();

        // Audit #38: when a pipe closure declares a return type, verify the
        // body's inferred type is compatible. Mirrors the function-return
        // mismatch check; reuses `FunctionReturnTypeMismatch` with a
        // synthetic `<closure>` function name since closures don't have one.
        if let Some(declared) = return_type {
            // Push the closure's typed params so the body sees them while
            // inferring (otherwise references like `x + 1` resolve to
            // `Unknown` and trip a spurious mismatch).
            let mut frame = HashMap::new();
            for p in params {
                if let Some(ty) = &p.ty {
                    frame.insert(p.name.name.clone(), Self::type_to_string(ty));
                }
            }
            self.inference_scope_stack.borrow_mut().push(frame);
            let body_sem = self.infer_type_sem(body, file);
            self.inference_scope_stack.borrow_mut().pop();
            let body_type = body_sem.display();
            let expected = Self::type_to_string(declared);
            if !self.type_strings_compatible(&expected, &body_type) {
                // Audit2 B13: cite the body span (the offending expression),
                // not the whole closure-position span — IDE goto-definition
                // and `cargo check` output now point at the wrong return.
                self.errors.push(CompilerError::FunctionReturnTypeMismatch {
                    function: "<closure>".to_string(),
                    expected,
                    actual: body_type,
                    span: body.span(),
                });
            }
        }
    }

    /// Walk `expr` and emit `UseAfterSink` for any `Reference` whose root
    /// binding is in `consumed` and is not shadowed by a closure parameter
    /// or a binding introduced inside `expr`.
    #[expect(
        clippy::too_many_lines,
        reason = "dispatcher match over all Expr and BlockStatement variants"
    )]
    fn check_captures_rec(
        expr: &Expr,
        outer_params: &HashSet<String>,
        consumed: &HashSet<String>,
        errors: &mut Vec<CompilerError>,
        inner_scopes: &mut Vec<HashSet<String>>,
    ) {
        let is_shadowed = |name: &str| -> bool {
            if outer_params.contains(name) {
                return true;
            }
            inner_scopes.iter().any(|s| s.contains(name))
        };
        match expr {
            Expr::Reference { path, span } => {
                if let Some(first) = path.first() {
                    if !is_shadowed(&first.name) && consumed.contains(&first.name) {
                        errors.push(CompilerError::UseAfterSink {
                            name: first.name.clone(),
                            span: *span,
                        });
                    }
                }
            }
            Expr::Literal { .. } | Expr::InferredEnumInstantiation { .. } => {}
            Expr::Array { elements, .. } => {
                for e in elements {
                    Self::check_captures_rec(e, outer_params, consumed, errors, inner_scopes);
                }
            }
            Expr::Tuple { fields, .. } => {
                for (_, e) in fields {
                    Self::check_captures_rec(e, outer_params, consumed, errors, inner_scopes);
                }
            }
            Expr::Invocation { args, .. } => {
                for (_, e) in args {
                    Self::check_captures_rec(e, outer_params, consumed, errors, inner_scopes);
                }
            }
            Expr::EnumInstantiation { data, .. } => {
                for (_, e) in data {
                    Self::check_captures_rec(e, outer_params, consumed, errors, inner_scopes);
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                Self::check_captures_rec(left, outer_params, consumed, errors, inner_scopes);
                Self::check_captures_rec(right, outer_params, consumed, errors, inner_scopes);
            }
            Expr::UnaryOp { operand, .. } => {
                Self::check_captures_rec(operand, outer_params, consumed, errors, inner_scopes);
            }
            Expr::ForExpr {
                var,
                collection,
                body,
                ..
            } => {
                Self::check_captures_rec(collection, outer_params, consumed, errors, inner_scopes);
                let mut scope = HashSet::new();
                scope.insert(var.name.clone());
                inner_scopes.push(scope);
                Self::check_captures_rec(body, outer_params, consumed, errors, inner_scopes);
                inner_scopes.pop();
            }
            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                Self::check_captures_rec(condition, outer_params, consumed, errors, inner_scopes);
                Self::check_captures_rec(then_branch, outer_params, consumed, errors, inner_scopes);
                if let Some(e) = else_branch {
                    Self::check_captures_rec(e, outer_params, consumed, errors, inner_scopes);
                }
            }
            Expr::MatchExpr {
                scrutinee, arms, ..
            } => {
                Self::check_captures_rec(scrutinee, outer_params, consumed, errors, inner_scopes);
                for arm in arms {
                    let mut scope = HashSet::new();
                    if let crate::ast::Pattern::Variant { bindings, .. } = &arm.pattern {
                        for b in bindings {
                            scope.insert(b.name.clone());
                        }
                    }
                    inner_scopes.push(scope);
                    Self::check_captures_rec(
                        &arm.body,
                        outer_params,
                        consumed,
                        errors,
                        inner_scopes,
                    );
                    inner_scopes.pop();
                }
            }
            Expr::Group { expr, .. } => {
                Self::check_captures_rec(expr, outer_params, consumed, errors, inner_scopes);
            }
            Expr::DictLiteral { entries, .. } => {
                for (k, v) in entries {
                    Self::check_captures_rec(k, outer_params, consumed, errors, inner_scopes);
                    Self::check_captures_rec(v, outer_params, consumed, errors, inner_scopes);
                }
            }
            Expr::DictAccess { dict, key, .. } => {
                Self::check_captures_rec(dict, outer_params, consumed, errors, inner_scopes);
                Self::check_captures_rec(key, outer_params, consumed, errors, inner_scopes);
            }
            Expr::FieldAccess { object, .. } => {
                Self::check_captures_rec(object, outer_params, consumed, errors, inner_scopes);
            }
            Expr::ClosureExpr { params, body, .. } => {
                let mut scope = HashSet::new();
                for p in params {
                    scope.insert(p.name.name.clone());
                }
                inner_scopes.push(scope);
                Self::check_captures_rec(body, outer_params, consumed, errors, inner_scopes);
                inner_scopes.pop();
            }
            Expr::LetExpr {
                pattern,
                value,
                body,
                ..
            } => {
                Self::check_captures_rec(value, outer_params, consumed, errors, inner_scopes);
                let mut scope = HashSet::new();
                for b in collect_bindings_from_pattern(pattern) {
                    scope.insert(b.name);
                }
                inner_scopes.push(scope);
                Self::check_captures_rec(body, outer_params, consumed, errors, inner_scopes);
                inner_scopes.pop();
            }
            Expr::MethodCall { receiver, args, .. } => {
                Self::check_captures_rec(receiver, outer_params, consumed, errors, inner_scopes);
                for (_, e) in args {
                    Self::check_captures_rec(e, outer_params, consumed, errors, inner_scopes);
                }
            }
            Expr::Block {
                statements, result, ..
            } => {
                let mut scope = HashSet::new();
                for stmt in statements {
                    match stmt {
                        BlockStatement::Let { pattern, value, .. } => {
                            Self::check_captures_rec(
                                value,
                                outer_params,
                                consumed,
                                errors,
                                inner_scopes,
                            );
                            for b in collect_bindings_from_pattern(pattern) {
                                scope.insert(b.name);
                            }
                        }
                        BlockStatement::Assign { target, value, .. } => {
                            Self::check_captures_rec(
                                target,
                                outer_params,
                                consumed,
                                errors,
                                inner_scopes,
                            );
                            Self::check_captures_rec(
                                value,
                                outer_params,
                                consumed,
                                errors,
                                inner_scopes,
                            );
                        }
                        BlockStatement::Expr(e) => {
                            Self::check_captures_rec(
                                e,
                                outer_params,
                                consumed,
                                errors,
                                inner_scopes,
                            );
                        }
                    }
                }
                inner_scopes.push(scope);
                Self::check_captures_rec(result, outer_params, consumed, errors, inner_scopes);
                inner_scopes.pop();
            }
        }
    }

    /// Collect the free variables referenced in a closure body.
    ///
    /// A free variable is any single-segment `Expr::Reference` path whose root
    /// identifier is not bound by the closure's own parameters, nor by any
    /// binding introduced within the body (nested closure params, `for`/`match`
    /// bindings, block/LetExpr locals). Ordering of the returned list is the
    /// order first encountered; duplicates are suppressed.
    pub(super) fn collect_free_variables(
        body: &Expr,
        closure_params: &HashSet<String>,
    ) -> Vec<String> {
        let mut captures: Vec<String> = Vec::new();
        let mut inner_scopes: Vec<HashSet<String>> = Vec::new();
        Self::collect_free_vars_rec(body, closure_params, &mut inner_scopes, &mut captures);
        captures
    }

    #[expect(
        clippy::too_many_lines,
        reason = "dispatcher match over all Expr and BlockStatement variants"
    )]
    fn collect_free_vars_rec(
        expr: &Expr,
        outer_params: &HashSet<String>,
        inner_scopes: &mut Vec<HashSet<String>>,
        captures: &mut Vec<String>,
    ) {
        let is_bound = |name: &str, inner: &Vec<HashSet<String>>| -> bool {
            if outer_params.contains(name) {
                return true;
            }
            inner.iter().any(|s| s.contains(name))
        };
        match expr {
            Expr::Reference { path, .. } => {
                if path.len() == 1 {
                    if let Some(first) = path.first() {
                        let name = &first.name;
                        if !is_bound(name, inner_scopes)
                            && !captures.iter().any(|n| n == name)
                            && name != "self"
                        {
                            captures.push(name.clone());
                        }
                    }
                }
            }
            Expr::Literal { .. } | Expr::InferredEnumInstantiation { .. } => {}
            Expr::Array { elements, .. } => {
                for e in elements {
                    Self::collect_free_vars_rec(e, outer_params, inner_scopes, captures);
                }
            }
            Expr::Tuple { fields, .. } => {
                for (_, e) in fields {
                    Self::collect_free_vars_rec(e, outer_params, inner_scopes, captures);
                }
            }
            Expr::Invocation { path, args, .. } => {
                // The function/struct name itself is a bound symbol or a name;
                // if it is a single-segment reference to a let binding, it
                // should count as a capture too (so we can detect calling a
                // captured closure binding that was consumed).
                if path.len() == 1 {
                    if let Some(first) = path.first() {
                        let name = &first.name;
                        if !is_bound(name, inner_scopes)
                            && !captures.iter().any(|n| n == name)
                            && name != "self"
                        {
                            captures.push(name.clone());
                        }
                    }
                }
                for (_, e) in args {
                    Self::collect_free_vars_rec(e, outer_params, inner_scopes, captures);
                }
            }
            Expr::EnumInstantiation { data, .. } => {
                for (_, e) in data {
                    Self::collect_free_vars_rec(e, outer_params, inner_scopes, captures);
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                Self::collect_free_vars_rec(left, outer_params, inner_scopes, captures);
                Self::collect_free_vars_rec(right, outer_params, inner_scopes, captures);
            }
            Expr::UnaryOp { operand, .. } => {
                Self::collect_free_vars_rec(operand, outer_params, inner_scopes, captures);
            }
            Expr::ForExpr {
                var,
                collection,
                body,
                ..
            } => {
                Self::collect_free_vars_rec(collection, outer_params, inner_scopes, captures);
                let mut scope = HashSet::new();
                scope.insert(var.name.clone());
                inner_scopes.push(scope);
                Self::collect_free_vars_rec(body, outer_params, inner_scopes, captures);
                inner_scopes.pop();
            }
            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                Self::collect_free_vars_rec(condition, outer_params, inner_scopes, captures);
                Self::collect_free_vars_rec(then_branch, outer_params, inner_scopes, captures);
                if let Some(e) = else_branch {
                    Self::collect_free_vars_rec(e, outer_params, inner_scopes, captures);
                }
            }
            Expr::MatchExpr {
                scrutinee, arms, ..
            } => {
                Self::collect_free_vars_rec(scrutinee, outer_params, inner_scopes, captures);
                for arm in arms {
                    let mut scope = HashSet::new();
                    if let crate::ast::Pattern::Variant { bindings, .. } = &arm.pattern {
                        for b in bindings {
                            scope.insert(b.name.clone());
                        }
                    }
                    inner_scopes.push(scope);
                    Self::collect_free_vars_rec(&arm.body, outer_params, inner_scopes, captures);
                    inner_scopes.pop();
                }
            }
            Expr::Group { expr, .. } => {
                Self::collect_free_vars_rec(expr, outer_params, inner_scopes, captures);
            }
            Expr::DictLiteral { entries, .. } => {
                for (k, v) in entries {
                    Self::collect_free_vars_rec(k, outer_params, inner_scopes, captures);
                    Self::collect_free_vars_rec(v, outer_params, inner_scopes, captures);
                }
            }
            Expr::DictAccess { dict, key, .. } => {
                Self::collect_free_vars_rec(dict, outer_params, inner_scopes, captures);
                Self::collect_free_vars_rec(key, outer_params, inner_scopes, captures);
            }
            Expr::FieldAccess { object, .. } => {
                Self::collect_free_vars_rec(object, outer_params, inner_scopes, captures);
            }
            Expr::ClosureExpr { params, body, .. } => {
                let mut scope = HashSet::new();
                for p in params {
                    scope.insert(p.name.name.clone());
                }
                inner_scopes.push(scope);
                Self::collect_free_vars_rec(body, outer_params, inner_scopes, captures);
                inner_scopes.pop();
            }
            Expr::LetExpr {
                pattern,
                value,
                body,
                ..
            } => {
                Self::collect_free_vars_rec(value, outer_params, inner_scopes, captures);
                let mut scope = HashSet::new();
                for b in collect_bindings_from_pattern(pattern) {
                    scope.insert(b.name);
                }
                inner_scopes.push(scope);
                Self::collect_free_vars_rec(body, outer_params, inner_scopes, captures);
                inner_scopes.pop();
            }
            Expr::MethodCall { receiver, args, .. } => {
                Self::collect_free_vars_rec(receiver, outer_params, inner_scopes, captures);
                for (_, e) in args {
                    Self::collect_free_vars_rec(e, outer_params, inner_scopes, captures);
                }
            }
            Expr::Block {
                statements, result, ..
            } => {
                let mut scope = HashSet::new();
                for stmt in statements {
                    match stmt {
                        BlockStatement::Let { pattern, value, .. } => {
                            Self::collect_free_vars_rec(
                                value,
                                outer_params,
                                inner_scopes,
                                captures,
                            );
                            for b in collect_bindings_from_pattern(pattern) {
                                scope.insert(b.name);
                            }
                        }
                        BlockStatement::Assign { target, value, .. } => {
                            Self::collect_free_vars_rec(
                                target,
                                outer_params,
                                inner_scopes,
                                captures,
                            );
                            Self::collect_free_vars_rec(
                                value,
                                outer_params,
                                inner_scopes,
                                captures,
                            );
                        }
                        BlockStatement::Expr(e) => {
                            Self::collect_free_vars_rec(e, outer_params, inner_scopes, captures);
                        }
                    }
                }
                inner_scopes.push(scope);
                Self::collect_free_vars_rec(result, outer_params, inner_scopes, captures);
                inner_scopes.pop();
            }
        }
    }

    /// Validate a let expression
    ///
    /// Like block statements, `let ... in body` introduces bindings that are
    /// scoped to `body` and must not leak out. Snapshots are taken on entry
    /// and restored on exit.
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
        // nil literals must not be assigned to non-optional types
        if let Some(type_ann) = ty {
            let declared = Self::type_to_string(type_ann);
            let inferred_sem = self.infer_type_sem(value, file);
            if matches!(inferred_sem, SemType::Nil) && !declared.ends_with('?') {
                self.errors.push(CompilerError::NilAssignedToNonOptional {
                    expected: declared,
                    span: *span,
                });
            }
        }
        self.validate_destructuring_pattern(pattern, value, *span, file);
        let saved_let_bindings = self.local_let_bindings.clone();
        let saved_closure_conventions = self.closure_binding_conventions.clone();
        let saved_closure_captures = self.closure_binding_captures.clone();
        let saved_consumed = self.consumed_bindings.clone();
        // Collect closure captures once for reuse across all pattern bindings.
        let captures = if matches!(ty, Some(Type::Closure { .. })) {
            if let Expr::ClosureExpr {
                params: cparams,
                body,
                ..
            } = &**value
            {
                let param_set: HashSet<String> =
                    cparams.iter().map(|p| p.name.name.clone()).collect();
                Some(Self::collect_free_variables(body, &param_set))
            } else {
                None
            }
        } else {
            None
        };
        for binding in collect_bindings_from_pattern(pattern) {
            if super::is_primitive_name(&binding.name) {
                self.errors.push(CompilerError::PrimitiveRedefinition {
                    name: binding.name.clone(),
                    span: binding.span,
                });
                continue;
            }
            let inferred_ty = self.infer_type_sem(value, file).display();
            // If annotated as a closure type, record param conventions for call-site enforcement
            if let Some(Type::Closure { params, .. }) = ty {
                let conventions: Vec<_> = params.iter().map(|(c, _)| *c).collect();
                self.closure_binding_conventions
                    .insert(binding.name.clone(), conventions);
            }
            if let Some(caps) = &captures {
                self.closure_binding_captures
                    .insert(binding.name.clone(), caps.clone());
                self.fn_scope_closure_captures
                    .insert(binding.name.clone(), caps.clone());
            }
            self.local_let_bindings
                .insert(binding.name, (inferred_ty, *mutable));
        }
        self.validate_expr(body, file);
        // Preserve consumption for outer-scope names (function locals, module
        // lets, closure captures). Drop only names introduced by this LetExpr.
        let mut restored_consumed = saved_consumed;
        for name in &self.consumed_bindings {
            let introduced_here = self.local_let_bindings.contains_key(name)
                && !saved_let_bindings.contains_key(name);
            if !introduced_here {
                restored_consumed.insert(name.clone());
            }
        }
        self.local_let_bindings = saved_let_bindings;
        self.closure_binding_conventions = saved_closure_conventions;
        self.closure_binding_captures = saved_closure_captures;
        self.consumed_bindings = restored_consumed;
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
        let receiver_type = self.infer_type_sem(receiver, file).display();
        if let Some(fn_def) = Self::find_method_fn_def(&receiver_type, &method.name, file) {
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
                if let Some(root) = Self::root_binding(receiver) {
                    self.consumed_bindings.insert(root);
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
                    if let Some(root) = Self::root_binding(arg_expr) {
                        self.consumed_bindings.insert(root);
                    }
                    // Escape analysis: sink-passed closure carries its captures away.
                    self.escape_closure_value(arg_expr);
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
                    if let Some(root) = Self::root_binding(arg_expr) {
                        self.consumed_bindings.insert(root);
                    }
                    // Escape analysis: sink-passed closure carries its captures away.
                    self.escape_closure_value(arg_expr);
                }
                ParamConvention::Let => {}
            }
        }
    }

    /// Validate a block expression (statements + result)
    ///
    /// Block-local let bindings, closure conventions, and sink-consumption flags
    /// are isolated from the enclosing scope: snapshots are taken on entry and
    /// restored on exit so block-internal names do not leak.
    #[expect(
        clippy::too_many_lines,
        reason = "linear pass over block statements + assign-time escape check; splitting hides flow"
    )]
    fn validate_expr_block(&mut self, statements: &[BlockStatement], result: &Expr, file: &File) {
        let saved_let_bindings = self.local_let_bindings.clone();
        let saved_closure_conventions = self.closure_binding_conventions.clone();
        let saved_closure_captures = self.closure_binding_captures.clone();
        let saved_consumed = self.consumed_bindings.clone();
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
                    let ty_str = ty.as_ref().map_or_else(
                        || self.infer_type_sem(value, file).display(),
                        |t| Self::type_to_string(t),
                    );
                    // Collect free variables once if this is a closure literal,
                    // so we can reuse them across all bindings in the pattern.
                    let captures = if matches!(ty, Some(Type::Closure { .. })) {
                        if let Expr::ClosureExpr {
                            params: cparams,
                            body,
                            ..
                        } = value
                        {
                            let param_set: HashSet<String> =
                                cparams.iter().map(|p| p.name.name.clone()).collect();
                            Some(Self::collect_free_variables(body, &param_set))
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    for binding in collect_bindings_from_pattern(pattern) {
                        if super::is_primitive_name(&binding.name) {
                            self.errors.push(CompilerError::PrimitiveRedefinition {
                                name: binding.name.clone(),
                                span: binding.span,
                            });
                            continue;
                        }
                        if let Some(Type::Closure { params, .. }) = ty {
                            let conventions: Vec<_> = params.iter().map(|(c, _)| *c).collect();
                            self.closure_binding_conventions
                                .insert(binding.name.clone(), conventions);
                        }
                        if let Some(caps) = &captures {
                            self.closure_binding_captures
                                .insert(binding.name.clone(), caps.clone());
                            self.fn_scope_closure_captures
                                .insert(binding.name.clone(), caps.clone());
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
                    let value_sem = self.infer_type_sem(value, file);
                    let target_sem = self.infer_type_sem(target, file);
                    if !value_sem.is_indeterminate() && !target_sem.is_indeterminate() {
                        let value_type = value_sem.display();
                        let target_type = target_sem.display();
                        if !self.type_strings_compatible(&target_type, &value_type) {
                            self.errors.push(CompilerError::TypeMismatch {
                                expected: target_type,
                                found: value_type,
                                span: *span,
                            });
                        }
                    }
                    // Tier-1 escape extension: a closure value assigned
                    // to an outer-scope `mut` binding outlives this
                    // block; its captures must outlive the function
                    // frame. `saved_let_bindings` is the set in scope
                    // before this block opened — bindings declared in
                    // *this* block are absent from it.
                    if let Expr::Reference { path, .. } = target {
                        if let [seg] = path.as_slice() {
                            if saved_let_bindings.contains_key(&seg.name) {
                                if let Some(caps) = self.closure_captures_of_expr(value) {
                                    self.validate_escaping_captures(&caps, *span);
                                }
                            }
                        }
                    }
                }
                BlockStatement::Expr(expr) => {
                    self.validate_expr(expr, file);
                }
            }
        }
        self.validate_expr(result, file);
        // Restore outer let/closure-convention scope. For consumption flags, keep
        // any binding consumed inside the block that belongs to an outer scope
        // (block did not introduce it). This preserves consumption of outer
        // locals AND module-level lets — dropping only flags for names the
        // block itself introduced.
        let mut restored_consumed = saved_consumed;
        for name in &self.consumed_bindings {
            let introduced_here = self.local_let_bindings.contains_key(name)
                && !saved_let_bindings.contains_key(name);
            if !introduced_here {
                restored_consumed.insert(name.clone());
            }
        }
        self.local_let_bindings = saved_let_bindings;
        self.closure_binding_conventions = saved_closure_conventions;
        self.closure_binding_captures = saved_closure_captures;
        self.consumed_bindings = restored_consumed;
    }

    /// If `condition` is a reference or field access whose type is optional
    /// (`T?`), install a local binding whose name matches the trailing
    /// segment with the unwrapped type `T` and return the binding name
    /// (plus the prior entry, if any, so the caller can restore it after
    /// the then-branch). Otherwise returns (None, None). Audit #22.
    fn bind_optional_auto_binding(
        &mut self,
        condition: &Expr,
        file: &File,
    ) -> (Option<String>, Option<(String, bool)>) {
        let cond_sem = self.infer_type_sem(condition, file);
        let SemType::Optional(inner) = &cond_sem else {
            return (None, None);
        };
        let unwrapped_owned = inner.display();
        let unwrapped = unwrapped_owned.as_str();
        let name_opt = match condition {
            Expr::Reference { path, .. } => path.last().map(|id| id.name.clone()),
            Expr::FieldAccess { field, .. } => Some(field.name.clone()),
            Expr::Literal { .. }
            | Expr::Invocation { .. }
            | Expr::EnumInstantiation { .. }
            | Expr::InferredEnumInstantiation { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::Group { .. }
            | Expr::DictLiteral { .. }
            | Expr::DictAccess { .. }
            | Expr::ClosureExpr { .. }
            | Expr::LetExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Block { .. } => None,
        };
        let Some(name) = name_opt else {
            return (None, None);
        };
        let prev = self.local_let_bindings.get(&name).cloned();
        self.local_let_bindings
            .insert(name.clone(), (unwrapped.to_string(), false));
        (Some(name), prev)
    }

    /// Validate struct field requirements: all required fields must be provided, no unknown fields
    pub(super) fn validate_struct_fields(
        &mut self,
        struct_name: &str,
        args: &[(crate::ast::Ident, Expr)],
        span: Span,
        file: &File,
    ) {
        // Find the struct definition in current file or module cache.
        // Clone the field name + declared type pairs so we can release the
        // borrow on `self` before recursing into type inference calls below.
        let (field_names, field_types, required_fields, generic_params) = {
            if let Some(def) = self.find_struct_def_in_files(struct_name, file) {
                let field_names: Vec<String> =
                    def.fields.iter().map(|f| f.name.name.clone()).collect();

                let field_types: Vec<(String, String)> = def
                    .fields
                    .iter()
                    .map(|f| (f.name.name.clone(), Self::type_to_string(&f.ty)))
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

                let generic_params: Vec<String> =
                    def.generics.iter().map(|g| g.name.name.clone()).collect();

                (field_names, field_types, required_fields, generic_params)
            } else {
                return; // Struct not found, skip validation
            }
        };

        // Check all provided regular fields exist and type-check each value.
        for (arg_name, arg_value) in args {
            if !field_names.contains(&arg_name.name) {
                self.errors.push(CompilerError::UnknownField {
                    field: arg_name.name.clone(),
                    type_name: struct_name.to_string(),
                    span: arg_name.span,
                });
                continue;
            }
            let Some((_, declared)) = field_types.iter().find(|(n, _)| n == &arg_name.name) else {
                continue;
            };
            // Skip the check if the declared type references a generic
            // parameter of the struct — generic substitution is handled by
            // the IR monomorphisation pass, not the string-level comparison
            // here.
            if generic_params.iter().any(|g| declared.contains(g)) {
                continue;
            }
            let inferred_sem = self.infer_type_sem(arg_value, file);
            let inferred = inferred_sem.display();
            // nil is compatible with any optional type
            let nil_to_optional = matches!(inferred_sem, SemType::Nil) && declared.ends_with('?');
            // T is compatible with T? (implicit wrapping)
            let inner_to_optional =
                declared.ends_with('?') && declared.trim_end_matches('?') == inferred.as_str();
            // declared can still be a string with "Unknown" in it (e.g. unresolved
            // type annotation); preserve the legacy guard for that case.
            let declared_indeterminate = declared.contains("Unknown");
            if !nil_to_optional
                && !inner_to_optional
                && !inferred_sem.is_indeterminate()
                && !declared_indeterminate
                && !self.type_strings_compatible(declared, &inferred)
            {
                self.errors.push(CompilerError::TypeMismatch {
                    expected: declared.clone(),
                    found: inferred,
                    span: arg_value.span(),
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
        // Collect closure-typed field names and mutability info from the struct def,
        // dropping the borrow before mutating `self` for escape tracking.
        let struct_info: Option<Vec<(String, bool, bool)>> = {
            let mut found = None;
            for statement in &file.statements {
                if let Statement::Definition(def) = statement {
                    if let Definition::Struct(struct_def) = &**def {
                        if struct_def.name.name == struct_name {
                            let info: Vec<(String, bool, bool)> = struct_def
                                .fields
                                .iter()
                                .map(|f| {
                                    (
                                        f.name.name.clone(),
                                        f.mutable,
                                        matches!(f.ty, crate::ast::Type::Closure { .. }),
                                    )
                                })
                                .collect();
                            found = Some(info);
                            break;
                        }
                    }
                }
            }
            // Fall back to module cache if not found in current file.
            if found.is_none() {
                for (cached_file, _) in self.module_cache.values() {
                    for statement in &cached_file.statements {
                        if let Statement::Definition(def) = statement {
                            if let Definition::Struct(struct_def) = &**def {
                                if struct_def.name.name == struct_name {
                                    let info: Vec<(String, bool, bool)> = struct_def
                                        .fields
                                        .iter()
                                        .map(|f| {
                                            (
                                                f.name.name.clone(),
                                                f.mutable,
                                                matches!(f.ty, crate::ast::Type::Closure { .. }),
                                            )
                                        })
                                        .collect();
                                    found = Some(info);
                                    break;
                                }
                            }
                        }
                    }
                    if found.is_some() {
                        break;
                    }
                }
            }
            found
        };
        let Some(fields) = struct_info else {
            return;
        };
        for (arg_name, arg_expr) in args {
            let Some((_, field_mutable, field_is_closure)) =
                fields.iter().find(|(n, _, _)| n == &arg_name.name)
            else {
                continue;
            };
            if *field_mutable && !self.is_expr_mutable(arg_expr, file) {
                self.errors.push(CompilerError::MutabilityMismatch {
                    param: arg_name.name.clone(),
                    span,
                });
            }
            // Escape analysis: a closure value stored in a struct field escapes
            // with the struct — mark its captures as consumed.
            if *field_is_closure {
                self.escape_closure_value(arg_expr);
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
        let left_sem = self.infer_type_sem(left, file);
        let right_sem = self.infer_type_sem(right, file);

        // Skip validation when either operand type is unknown (field access, method calls, etc.)
        if left_sem.is_unknown() || right_sem.is_unknown() {
            return;
        }
        let left_type = left_sem.display();
        let right_type = right_sem.display();

        // Check type compatibility based on operator. Audit #44: the
        // hardcoded GPU numeric compat (`f32`, `vec3`, etc.) was a wart
        // from the retired WGSL backend and is gone — backends needing
        // arithmetic over backend-specific scalar/vector types should
        // implement their own type-compat rules in their codegen pass.
        // Numeric primitives accepted for arithmetic / comparison / range.
        // Both operands must agree (no implicit promotion across widths).
        let is_numeric = |s: &str| matches!(s, "I32" | "I64" | "F32" | "F64");
        let valid = match op {
            // Add: matched-numeric pair, or String + String (concatenation)
            BinaryOperator::Add => {
                (is_numeric(&left_type) && left_type == right_type)
                    || (left_type == "String" && right_type == "String")
            }
            // Arithmetic, comparison, and range operators: matched-numeric pair
            BinaryOperator::Sub
            | BinaryOperator::Mul
            | BinaryOperator::Div
            | BinaryOperator::Mod
            | BinaryOperator::Lt
            | BinaryOperator::Gt
            | BinaryOperator::Le
            | BinaryOperator::Ge
            | BinaryOperator::Range => is_numeric(&left_type) && left_type == right_type,
            // Equality operators: same types
            BinaryOperator::Eq | BinaryOperator::Ne => left_type == right_type,
            // Logical operators: Boolean + Boolean
            BinaryOperator::And | BinaryOperator::Or => {
                left_type == "Boolean" && right_type == "Boolean"
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

    /// Validate for loop collection is an array or range
    pub(super) fn validate_for_loop(&mut self, collection: &Expr, span: Span, file: &File) {
        let collection_sem = self.infer_type_sem(collection, file);

        let is_iterable = matches!(collection_sem, SemType::Array(_) | SemType::Unknown)
            || matches!(&collection_sem, SemType::Generic { base, .. } if base == "Range");

        if !is_iterable {
            self.errors.push(CompilerError::ForLoopNotArray {
                actual: collection_sem.display(),
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
        let value_sem = self.infer_type_sem(value, file);

        // Skip destructuring validation when value type is unknown (field access, etc.)
        if value_sem.is_unknown() {
            return;
        }

        match pattern {
            BindingPattern::Array { elements, .. } => {
                // Array destructuring requires an array type
                if !matches!(value_sem, SemType::Array(_)) {
                    self.errors.push(CompilerError::ArrayDestructuringNotArray {
                        actual: value_sem.display(),
                        span,
                    });
                } else if let Expr::Array {
                    elements: literal_elems,
                    ..
                } = value
                {
                    // Known array length: pattern must not demand more fixed
                    // elements than the array provides. Partial patterns that
                    // cover fewer positions than the array (e.g.,
                    // `let [a, b] = [1, 2, 3]`) are permitted — extra values
                    // are simply unbound. A rest element accepts any tail.
                    let pattern_fixed = elements
                        .iter()
                        .filter(|e| !matches!(e, crate::ast::ArrayPatternElement::Rest(_)))
                        .count();
                    let actual = literal_elems.len();
                    if pattern_fixed > actual {
                        self.errors.push(CompilerError::TypeMismatch {
                            expected: format!("array with at least {pattern_fixed} element(s)"),
                            found: format!("array with {actual} element(s)"),
                            span,
                        });
                    }
                }
            }
            BindingPattern::Struct { fields, .. } => {
                // Struct destructuring requires a struct type.
                // The type may also be `Generic { base, .. }` for instantiated
                // generic structs — strip args for the lookup.
                let lookup_name = match &value_sem {
                    SemType::Generic { base, .. } | SemType::Named(base) => Some(base.as_str()),
                    SemType::Primitive(_)
                    | SemType::Array(_)
                    | SemType::Optional(_)
                    | SemType::Tuple(_)
                    | SemType::Dictionary { .. }
                    | SemType::Closure { .. }
                    | SemType::Unknown
                    | SemType::InferredEnum
                    | SemType::Nil => None,
                };
                if let Some(struct_info) = lookup_name.and_then(|n| self.symbols.get_struct(n)) {
                    let field_names: Vec<&str> =
                        struct_info.fields.iter().map(|f| f.name.as_str()).collect();
                    for field in fields {
                        if !field_names.contains(&field.name.name.as_str()) {
                            self.errors.push(CompilerError::UnknownField {
                                field: field.name.name.clone(),
                                type_name: value_sem.display(),
                                span: field.name.span,
                            });
                        }
                    }
                } else {
                    // Not a known struct - report error (includes primitives)
                    self.errors
                        .push(CompilerError::StructDestructuringNotStruct {
                            actual: value_sem.display(),
                            span,
                        });
                }
            }
            BindingPattern::Tuple { elements, .. } => {
                // Validate tuple pattern arity against tuple type "(x: T, y: U, ...)"
                if let SemType::Tuple(fields) = &value_sem {
                    let field_count = fields.len();
                    let pattern_count = elements.len();
                    if pattern_count > field_count && field_count > 0 {
                        self.errors.push(CompilerError::TypeMismatch {
                            expected: format!("tuple with {field_count} field(s)"),
                            found: value_sem.display(),
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
        use crate::ast::PrimitiveType;
        let condition_sem = self.infer_type_sem(condition, file);

        // Skip when type is unknown (field access, method calls — IR lowering handles these)
        if condition_sem.is_unknown() {
            return;
        }

        // Condition must be Boolean or optional
        let is_valid = matches!(
            condition_sem,
            SemType::Primitive(PrimitiveType::Boolean) | SemType::Optional(_)
        );
        if !is_valid {
            self.errors.push(CompilerError::InvalidIfCondition {
                actual: condition_sem.display(),
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
        let scrutinee_type = self.infer_type_sem(scrutinee, file).display();

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
        // Snapshot closure-binding maps so entries introduced in this function
        // body don't leak into later functions.
        let saved_closure_conventions = self.closure_binding_conventions.clone();
        let saved_closure_captures = self.closure_binding_captures.clone();
        let saved_fn_scope_captures = self.fn_scope_closure_captures.clone();
        let saved_param_conventions = self.current_fn_param_conventions.clone();
        self.current_fn_param_conventions.clear();
        self.fn_scope_closure_captures.clear();

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
            self.current_fn_param_conventions
                .insert(param.name.name.clone(), param.convention);
            // Register closure-typed parameters so they're callable inside the
            // body. Parameters have no captures of their own — no
            // closure_binding_captures entry.
            if let Some(Type::Closure {
                params: closure_params,
                ..
            }) = &param.ty
            {
                let conventions: Vec<_> = closure_params.iter().map(|(c, _)| *c).collect();
                self.closure_binding_conventions
                    .insert(param.name.name.clone(), conventions);
            }
        }

        // Validate the function body expression (only if body exists)
        if let Some(body) = &func.body {
            self.validate_expr(body, file);
            self.validate_function_return_escape(func.return_type.as_ref(), body);

            // If there's a declared return type, check it matches the body type
            if let Some(declared_return_type) = &func.return_type {
                let body_type = self.infer_type_sem(body, file).display();
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
        self.closure_binding_conventions = saved_closure_conventions;
        self.closure_binding_captures = saved_closure_captures;
        self.fn_scope_closure_captures = saved_fn_scope_captures;
        self.current_fn_param_conventions = saved_param_conventions;
    }

    /// Validate a standalone function definition (outside of impl blocks)
    pub(super) fn validate_standalone_function(
        &mut self,
        func: &crate::ast::FunctionDef,
        file: &File,
    ) {
        // Push the function's own generic parameters so its param/return
        // types and body can reference them without triggering
        // OutOfScopeTypeParameter.
        self.push_generic_scope(&func.generics);
        // Clear local let bindings and sink-consumed bindings for this function
        self.local_let_bindings.clear();
        self.consumed_bindings.clear();
        // Snapshot closure-binding maps so entries introduced in this function
        // body don't leak into later functions.
        let saved_closure_conventions = self.closure_binding_conventions.clone();
        let saved_closure_captures = self.closure_binding_captures.clone();
        let saved_fn_scope_captures = self.fn_scope_closure_captures.clone();
        let saved_param_conventions = self.current_fn_param_conventions.clone();
        self.current_fn_param_conventions.clear();
        self.fn_scope_closure_captures.clear();

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
            self.current_fn_param_conventions
                .insert(param.name.name.clone(), param.convention);
            if let Some(Type::Closure {
                params: closure_params,
                ..
            }) = &param.ty
            {
                let conventions: Vec<_> = closure_params.iter().map(|(c, _)| *c).collect();
                self.closure_binding_conventions
                    .insert(param.name.name.clone(), conventions);
            }
        }

        // Validate return type if declared
        if let Some(return_type) = &func.return_type {
            self.validate_type(return_type);
        }

        // Validate the function body if present
        if let Some(body) = &func.body {
            self.validate_expr(body, file);
            self.validate_function_return_escape(func.return_type.as_ref(), body);

            // If there's a declared return type, check it matches the body type
            if let Some(declared_return_type) = &func.return_type {
                let body_type = self.infer_type_sem(body, file).display();
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
        self.closure_binding_conventions = saved_closure_conventions;
        self.closure_binding_captures = saved_closure_captures;
        self.fn_scope_closure_captures = saved_fn_scope_captures;
        self.current_fn_param_conventions = saved_param_conventions;
        self.pop_generic_scope();
    }

    /// Check if a method exists on a given type
    ///
    /// Handles user-defined methods in impl blocks and trait methods available
    /// to types that implement the trait (directly or via a generic constraint).
    #[expect(
        clippy::too_many_lines,
        reason = "exhaustive lookup across local impls, trait impls, generic param constraints, cached modules, and qualified-name nested modules — splitting reduces locality without simplifying"
    )]
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
        // Strip optional marker and generic args for lookups
        let base = type_name.trim_end_matches('?');
        let lookup = base.split_once('<').map_or(base, |(n, _)| n);

        // Check if it's a struct with an impl block containing the method
        if self.symbols.is_struct(lookup) {
            // Check impl blocks in the current file
            for statement in &file.statements {
                if let Statement::Definition(def) = statement {
                    if let Definition::Impl(impl_def) = &**def {
                        if impl_def.name.name == lookup {
                            for func in &impl_def.functions {
                                if func.name.name == method_name {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
            // Check trait methods for traits this struct implements
            let traits = self.symbols.get_all_traits_for_struct(lookup);
            for trait_name in traits {
                if let Some(info) = self.symbols.get_trait(&trait_name) {
                    for sig in &info.methods {
                        if sig.name.name == method_name {
                            return true;
                        }
                    }
                }
            }
        }

        // Check enum impl blocks
        if self.symbols.get_enum_variants(lookup).is_some() {
            for statement in &file.statements {
                if let Statement::Definition(def) = statement {
                    if let Definition::Impl(impl_def) = &**def {
                        if impl_def.name.name == lookup {
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

        // If the receiver type is an in-scope generic parameter, look for the
        // method on any of its trait constraints. generic_scopes is only
        // populated during type resolution, so also fall back to scanning the
        // current file's struct/impl definitions for a matching type parameter.
        if let Some(constraints) = self.get_type_parameter_constraints(lookup) {
            for trait_name in constraints {
                if let Some(info) = self.symbols.get_trait(&trait_name) {
                    for sig in &info.methods {
                        if sig.name.name == method_name {
                            return true;
                        }
                    }
                }
            }
        }
        if self.type_param_has_method(lookup, method_name, file) {
            return true;
        }

        // Cross-module lookup: the receiver's type may have been imported
        // via `use mod::Type`, in which case the impl lives in the module's
        // cached AST. Scan every cached module for a matching impl.
        for (cached_file, _) in self.module_cache.values() {
            for statement in &cached_file.statements {
                if let Statement::Definition(def) = statement {
                    if let Definition::Impl(impl_def) = &**def {
                        if impl_def.name.name == lookup {
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

        // Audit2 B14: qualified-type lookup — when the receiver type is
        // `m::Foo`, walk into the nested module path (inline modules in
        // the current file, then cached imported modules) and check for
        // an impl of `Foo` with the requested method. The bare-name
        // checks above don't handle qualified receivers, so prior to
        // this fix `f.method()` on an imported-module type silently
        // returned "not defined".
        if let Some((module_segments, bare_name)) = split_qualified_type(lookup) {
            // Inline modules in the current file.
            if let Some(defs) = find_nested_module_definitions(&file.statements, &module_segments) {
                if impl_method_in_definitions(defs, bare_name, method_name) {
                    return true;
                }
            }
            // Imported modules in the cache.
            for (cached_file, _) in self.module_cache.values() {
                if let Some(defs) =
                    find_nested_module_definitions(&cached_file.statements, &module_segments)
                {
                    if impl_method_in_definitions(defs, bare_name, method_name) {
                        return true;
                    }
                }
                // Also check the cached file's top-level when only the
                // last segment is the module name (e.g. `use mod::*`
                // re-exports flatten differently; staying defensive).
                if module_segments.len() == 1 {
                    for statement in &cached_file.statements {
                        if let Statement::Definition(def) = statement {
                            if let Definition::Impl(impl_def) = &**def {
                                if impl_def.name.name == bare_name {
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
            }
        }

        false
    }

    /// Check whether `name` is a generic type parameter on some struct/impl/enum
    /// in the file, and if so, whether any of its trait constraints provide
    /// `method_name`.
    fn type_param_has_method(&self, name: &str, method_name: &str, file: &File) -> bool {
        use crate::ast::GenericConstraint;
        let check_generics = |generics: &[crate::ast::GenericParam]| -> bool {
            for gp in generics {
                if gp.name.name != name {
                    continue;
                }
                for constraint in &gp.constraints {
                    let GenericConstraint::Trait {
                        name: trait_ref, ..
                    } = constraint;
                    if let Some(info) = self.symbols.get_trait(&trait_ref.name) {
                        for sig in &info.methods {
                            if sig.name.name == method_name {
                                return true;
                            }
                        }
                    }
                }
            }
            false
        };
        for stmt in &file.statements {
            if let Statement::Definition(def) = stmt {
                match &**def {
                    Definition::Struct(s) if check_generics(&s.generics) => return true,
                    Definition::Impl(i) if check_generics(&i.generics) => return true,
                    Definition::Enum(e) if check_generics(&e.generics) => return true,
                    Definition::Trait(t) if check_generics(&t.generics) => return true,
                    Definition::Struct(_)
                    | Definition::Impl(_)
                    | Definition::Enum(_)
                    | Definition::Trait(_)
                    | Definition::Module(_)
                    | Definition::Function(_) => {}
                }
            }
        }
        false
    }
}

/// Audit2 B14: split a qualified type name `m1::m2::Foo` into module
/// segments `["m1", "m2"]` and bare name `"Foo"`. Returns `None` if the
/// name has no `::`.
fn split_qualified_type(name: &str) -> Option<(Vec<&str>, &str)> {
    if !name.contains("::") {
        return None;
    }
    let mut parts: Vec<&str> = name.split("::").collect();
    let last = parts.pop()?;
    Some((parts, last))
}

/// Audit2 B14: walk a slice of `Statement`s looking for the nested
/// module path `segments`, returning that module's `definitions`.
/// Recurses into nested `Definition::Module` matches.
fn find_nested_module_definitions<'a>(
    statements: &'a [Statement],
    segments: &[&str],
) -> Option<&'a [Definition]> {
    let (head, rest) = segments.split_first()?;
    for stmt in statements {
        if let Statement::Definition(def) = stmt {
            if let Definition::Module(module_def) = &**def {
                if module_def.name.name == *head {
                    return if rest.is_empty() {
                        Some(&module_def.definitions)
                    } else {
                        find_nested_module_definitions_in_defs(&module_def.definitions, rest)
                    };
                }
            }
        }
    }
    None
}

fn find_nested_module_definitions_in_defs<'a>(
    definitions: &'a [Definition],
    segments: &[&str],
) -> Option<&'a [Definition]> {
    let (head, rest) = segments.split_first()?;
    for def in definitions {
        if let Definition::Module(module_def) = def {
            if module_def.name.name == *head {
                return if rest.is_empty() {
                    Some(&module_def.definitions)
                } else {
                    find_nested_module_definitions_in_defs(&module_def.definitions, rest)
                };
            }
        }
    }
    None
}

/// Audit2 B14: scan a slice of definitions for `impl <bare> { fn <method> }`.
fn impl_method_in_definitions(definitions: &[Definition], bare: &str, method: &str) -> bool {
    for def in definitions {
        if let Definition::Impl(impl_def) = def {
            if impl_def.name.name == bare {
                for func in &impl_def.functions {
                    if func.name.name == method {
                        return true;
                    }
                }
            }
        }
    }
    false
}
