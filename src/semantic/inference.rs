use super::module_resolver::ModuleResolver;
use super::SemanticAnalyzer;
use crate::ast::{BinaryOperator, Definition, Expr, File, Literal, Statement, UnaryOperator};
use std::collections::HashMap;

use super::collect_bindings_from_pattern;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Infer the type of an expression (simplified type inference)
    #[expect(
        clippy::too_many_lines,
        reason = "dispatcher match over all Expr variants"
    )]
    pub(super) fn infer_type(&self, expr: &Expr, file: &File) -> String {
        match expr {
            Expr::Literal { value: lit, .. } => match lit {
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
            Expr::IfExpr {
                then_branch,
                else_branch,
                ..
            } => {
                let then_ty = self.infer_type(then_branch, file);
                else_branch.as_ref().map_or_else(
                    || then_ty.clone(),
                    |else_expr| {
                        let else_ty = self.infer_type(else_expr, file);
                        Self::widen_branch_types(&then_ty, &else_ty)
                    },
                )
            }
            Expr::MatchExpr {
                scrutinee, arms, ..
            } => {
                // Audit #27: pre-populate each arm's pattern bindings into
                // an inference-scope frame so references inside the arm
                // body resolve to concrete types instead of "Unknown".
                let scrutinee_ty = self.infer_type(scrutinee, file);
                let enum_name = scrutinee_ty.trim_end_matches('?');
                let mut types: Vec<String> = Vec::with_capacity(arms.len());
                for arm in arms {
                    let frame = self.build_match_arm_scope(enum_name, &arm.pattern);
                    self.inference_scope_stack.borrow_mut().push(frame);
                    types.push(self.infer_type(&arm.body, file));
                    self.inference_scope_stack.borrow_mut().pop();
                }
                let Some(mut result) = types.pop() else {
                    return "Unknown".to_string();
                };
                while let Some(next) = types.pop() {
                    result = Self::widen_branch_types(&result, &next);
                }
                result
            }
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
            Expr::FieldAccess { object, field, .. } => {
                let obj_type = self.infer_type(object, file);
                self.infer_field_type_from_string(&obj_type, &field.name)
            }
            Expr::MethodCall {
                receiver, method, ..
            } => {
                let receiver_type = self.infer_type(receiver, file);
                self.infer_method_return_type(&receiver_type, &method.name, file)
            }
            Expr::ClosureExpr { params, body, .. } => {
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
                let body_type = self.infer_type(body, file);
                self.inference_scope_stack.borrow_mut().pop();
                match params.split_first() {
                    None => format!("() -> {body_type}"),
                    Some((only, [])) => {
                        let param_type = only
                            .ty
                            .as_ref()
                            .map_or_else(|| "Unknown".to_string(), Self::type_to_string);
                        format!("{param_type} -> {body_type}")
                    }
                    Some(_) => {
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
            }
            Expr::LetExpr { body, .. } => self.infer_type(body, file),
            Expr::Block {
                statements, result, ..
            } => {
                // Audit #4/#27: walk the block's statements to push each
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
                        let value_ty = ty
                            .as_ref()
                            .map_or_else(|| self.infer_type(value, file), Self::type_to_string);
                        if let crate::ast::BindingPattern::Simple(ident) = pattern {
                            frame.insert(ident.name.clone(), value_ty);
                        }
                    }
                }
                self.inference_scope_stack.borrow_mut().push(frame);
                let out = self.infer_type(result, file);
                self.inference_scope_stack.borrow_mut().pop();
                out
            }
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

        // Closure-typed binding called as a function (`cb()` where
        // `cb: (...) -> R`). Strip the parameter list and use R.
        if path.len() == 1 {
            let scope_lookup = {
                let stack = self.inference_scope_stack.borrow();
                stack
                    .iter()
                    .rev()
                    .find_map(|frame| frame.get(&name).cloned())
            };
            let ty_str =
                scope_lookup.or_else(|| self.local_let_bindings.get(&name).map(|(t, _)| t.clone()));
            if let Some(t) = ty_str {
                if let Some(arrow) = t.rfind(" -> ") {
                    return t[arrow.saturating_add(4)..].to_string();
                }
            }
        }

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
        } else if path.len() >= 2 {
            // Audit #27: resolve impl-block static method calls
            // (`Type::method(...)`), enum variant constructors
            // (`Enum::variant(...)`) that weren't rewritten to
            // EnumInstantiation at parse time, and module-qualified
            // function calls (`math::compute(...)`).
            let (Some(first), Some(last)) = (path.first(), path.last()) else {
                return "Unknown".to_string();
            };
            let receiver = &first.name;
            let method_name = &last.name;
            if self.symbols.is_struct(receiver) {
                if let Some(ret) = self.infer_method_return_from_impls(receiver, method_name) {
                    return ret;
                }
            }
            if self.symbols.get_enum_variants(receiver).is_some() {
                return receiver.clone();
            }
            // Module-qualified function: walk through module symbol tables.
            if let Some(ret) = self.lookup_qualified_function_return(path) {
                return ret;
            }
            "Unknown".to_string()
        } else {
            "Unknown".to_string()
        }
    }

    /// Resolve a qualified function path (`a::b::compute`) by walking
    /// `self.symbols.modules` segment by segment, then through the
    /// imported-module cache. Returns the function's declared return
    /// type as a string when found.
    fn lookup_qualified_function_return(&self, path: &[crate::ast::Ident]) -> Option<String> {
        let last = path.last()?;
        let segments: Vec<&str> = path
            .iter()
            .take(path.len().saturating_sub(1))
            .map(|i| i.name.as_str())
            .collect();
        let look = |symbols: &super::SymbolTable| -> Option<String> {
            let mut current = symbols;
            for part in &segments {
                match current.modules.get(*part) {
                    Some(info) => current = &info.symbols,
                    None => return None,
                }
            }
            current.get_function(&last.name).map(|f| {
                f.return_type
                    .as_ref()
                    .map_or_else(|| "nil".to_string(), Self::type_to_string)
            })
        };
        if let Some(ty) = look(&self.symbols) {
            return Some(ty);
        }
        for (_, symbols) in self.module_cache.values() {
            if let Some(ty) = look(symbols) {
                return Some(ty);
            }
        }
        None
    }

    /// Build a per-arm inference scope from a match pattern's bindings.
    /// `enum_name` is the (optionally optional-stripped) name of the
    /// scrutinee's type. For a `Variant { name, bindings }` pattern with
    /// `n` bindings, looks up the variant's field types on the named
    /// enum and zips them with the binding identifiers. Variants on
    /// imported enums fall back through the module cache. Returns an
    /// empty map for `Wildcard` and for variants that can't be resolved
    /// (the body then falls back to existing inference behaviour).
    fn build_match_arm_scope(
        &self,
        enum_name: &str,
        pattern: &crate::ast::Pattern,
    ) -> HashMap<String, String> {
        use crate::ast::Pattern;
        let mut frame = HashMap::new();
        let Pattern::Variant { name, bindings } = pattern else {
            return frame;
        };
        let variant_field_tys = self
            .lookup_enum_variant_field_types(enum_name, &name.name)
            .unwrap_or_default();
        for (i, ident) in bindings.iter().enumerate() {
            if let Some(ty) = variant_field_tys.get(i) {
                frame.insert(ident.name.clone(), ty.clone());
            }
        }
        frame
    }

    /// Look up an enum variant's field types as type-strings, in the
    /// current symbol table first, then through any imported module
    /// cache. Returns `None` if the enum or variant isn't found.
    fn lookup_enum_variant_field_types(
        &self,
        enum_name: &str,
        variant_name: &str,
    ) -> Option<Vec<String>> {
        if let Some(info) = self.symbols.enums.get(enum_name) {
            if let Some(fields) = info.variant_fields.get(variant_name) {
                return Some(fields.iter().map(|f| Self::type_to_string(&f.ty)).collect());
            }
        }
        for (_, symbols) in self.module_cache.values() {
            if let Some(info) = symbols.enums.get(enum_name) {
                if let Some(fields) = info.variant_fields.get(variant_name) {
                    return Some(fields.iter().map(|f| Self::type_to_string(&f.ty)).collect());
                }
            }
        }
        None
    }

    /// Walk `self.symbols` for a method declared on an impl block whose
    /// target is `struct_name` and whose name is `method_name`; return the
    /// method's declared return type as a string if found. Used by
    /// `infer_type_invocation` for impl static calls.
    fn infer_method_return_from_impls(
        &self,
        struct_name: &str,
        method_name: &str,
    ) -> Option<String> {
        let trait_names = self.symbols.get_all_traits_for_struct(struct_name);
        for trait_name in trait_names {
            if let Some(trait_info) = self.symbols.get_trait(&trait_name) {
                for m in &trait_info.methods {
                    if m.name.name == method_name {
                        return Some(
                            m.return_type
                                .as_ref()
                                .map_or_else(|| "nil".to_string(), Self::type_to_string),
                        );
                    }
                }
            }
        }
        None
    }

    /// Infer the type of a reference expression.
    ///
    /// For multi-segment paths like `p.name.x`, walks the chain starting from
    /// the root binding's type through each field access. Returns "Unknown"
    /// when any link in the chain can't be resolved.
    fn infer_type_reference(&self, path: &[crate::ast::Ident], _file: &File) -> String {
        let Some(first) = path.first() else {
            return "Unknown".to_string();
        };

        // Resolve the root type. Audit #27: consult the inference-scope
        // stack first so match-arm pattern bindings (and similar
        // pattern-introduced bindings) resolve to their concrete types
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
        let root_type: String = if let Some(scope_ty) = scope_lookup {
            scope_ty
        } else if first.name == "self" {
            self.current_impl_struct
                .clone()
                .unwrap_or_else(|| "Unknown".to_string())
        } else if let Some(let_type) = self.symbols.get_let_type(&first.name) {
            let_type.to_string()
        } else if let Some((local_type, _mutable)) = self.local_let_bindings.get(&first.name) {
            local_type.clone()
        } else if let Some(ref struct_name) = self.current_impl_struct {
            // Top-level field reference in an impl body — resolve against self.
            self.symbols.get_struct(struct_name).map_or_else(
                || "Unknown".to_string(),
                |struct_info| {
                    struct_info
                        .fields
                        .iter()
                        .find(|f| f.name == first.name)
                        .map_or_else(
                            || "Unknown".to_string(),
                            |field| Self::type_to_string(&field.ty),
                        )
                },
            )
        } else {
            "Unknown".to_string()
        };

        if path.len() == 1 {
            return root_type;
        }

        // Walk the field chain from the root type.
        let mut current = root_type;
        for seg in path.iter().skip(1) {
            current = self.infer_field_type_from_string(&current, &seg.name);
            if current == "Unknown" {
                return current;
            }
        }
        current
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

    /// Combine two branch types for if-expressions and match expressions.
    ///
    /// Widening rules:
    /// - `T` and `Nil` -> `T?`
    /// - `T` and `T?` -> `T?`
    /// - Identical types -> themselves
    /// - Otherwise, return the first type (callers rely on validation to flag
    ///   incompatible branches separately; inference just picks a usable type).
    fn widen_branch_types(a: &str, b: &str) -> String {
        if a == b {
            return a.to_string();
        }
        if a == "Nil" && !b.is_empty() && b != "Nil" {
            return if b.ends_with('?') {
                b.to_string()
            } else {
                format!("{b}?")
            };
        }
        if b == "Nil" && !a.is_empty() && a != "Nil" {
            return if a.ends_with('?') {
                a.to_string()
            } else {
                format!("{a}?")
            };
        }
        if let Some(inner) = a.strip_suffix('?') {
            if inner == b {
                return a.to_string();
            }
        }
        if let Some(inner) = b.strip_suffix('?') {
            if inner == a {
                return b.to_string();
            }
        }
        // Audit #26: for truly incompatible branches we used to silently
        // return `a`, which both hid real type errors and let a wrong
        // inferred type flow downstream. Return "Unknown" so the validator
        // sees an indeterminate type (rejected by type_strings_compatible
        // unless the expected side is also Unknown, which would itself
        // surface upstream once the inference cleanup lands).
        "Unknown".to_string()
    }

    /// Infer the type of a field access given the receiver's type string.
    ///
    /// Handles optional receiver types by stripping `?`, looking up the struct,
    /// and re-wrapping the result as `T?`. Returns "Unknown" when the receiver
    /// is not a known struct or the field doesn't exist.
    fn infer_field_type_from_string(&self, obj_type: &str, field_name: &str) -> String {
        if obj_type == "Unknown" || obj_type.contains("Unknown") {
            return "Unknown".to_string();
        }
        let (base, is_optional) = obj_type
            .strip_suffix('?')
            .map_or((obj_type, false), |s| (s, true));
        // Strip generic args like Container<T> -> Container for struct lookup
        let lookup_name = base.split_once('<').map_or(base, |(n, _)| n);
        // Top-level struct lookup.
        if let Some(struct_info) = self.symbols.get_struct(lookup_name) {
            for field in &struct_info.fields {
                if field.name == field_name {
                    let ty = Self::type_to_string(&field.ty);
                    return if is_optional { format!("{ty}?") } else { ty };
                }
            }
        }
        // Trait-used-as-type field lookup. Traits in FormaLang declare
        // required fields; an `item: SomeTrait` parameter must allow
        // `item.field` access.
        if let Some(trait_info) = self.symbols.get_trait(lookup_name) {
            if let Some(ty) = trait_info.fields.get(field_name) {
                let ty_str = Self::type_to_string(ty);
                return if is_optional {
                    format!("{ty_str}?")
                } else {
                    ty_str
                };
            }
        }
        // Module-nested struct: walk the symbol table's modules.
        if let Some(ty) = Self::lookup_field_in_modules(&self.symbols, lookup_name, field_name) {
            return if is_optional { format!("{ty}?") } else { ty };
        }
        // Imported-module struct: walk the analyser's module cache.
        for (_, symbols) in self.module_cache.values() {
            if let Some(struct_info) = symbols.get_struct(lookup_name) {
                for field in &struct_info.fields {
                    if field.name == field_name {
                        let ty = Self::type_to_string(&field.ty);
                        return if is_optional { format!("{ty}?") } else { ty };
                    }
                }
            }
            if let Some(ty) = Self::lookup_field_in_modules(symbols, lookup_name, field_name) {
                return if is_optional { format!("{ty}?") } else { ty };
            }
        }
        "Unknown".to_string()
    }

    /// Parse generic arguments from a receiver type-string. For
    /// `"Box<Number>"` returns `["Number"]`; for `"Map<String, Item>"`
    /// returns `["String", "Item"]`; for non-generic types returns `[]`.
    /// Splits on the top-level commas inside the angle brackets so
    /// nested generics (`Box<Pair<A, B>>`) survive.
    fn parse_receiver_type_args(receiver: &str) -> Vec<String> {
        let Some(open) = receiver.find('<') else {
            return Vec::new();
        };
        let Some(close) = receiver.rfind('>') else {
            return Vec::new();
        };
        if close <= open.saturating_add(1) {
            return Vec::new();
        }
        let inner = &receiver[open.saturating_add(1)..close];
        let mut args = Vec::new();
        let mut depth: i32 = 0;
        let mut start = 0;
        for (i, ch) in inner.char_indices() {
            match ch {
                '<' | '(' | '[' => depth = depth.saturating_add(1),
                '>' | ')' | ']' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => {
                    args.push(inner[start..i].trim().to_string());
                    start = i.saturating_add(1);
                }
                _ => {}
            }
        }
        let tail = inner[start..].trim();
        if !tail.is_empty() {
            args.push(tail.to_string());
        }
        args
    }

    /// Substitute every standalone occurrence of the type-parameter name
    /// `param` in `ty` with `concrete`. "Standalone" means the name
    /// isn't part of a longer identifier (so `T` in `Box<T>` is
    /// substituted but `T` in `TList` is not).
    fn substitute_type_string(ty: &str, param: &str, concrete: &str) -> String {
        let mut out = String::with_capacity(ty.len());
        let bytes = ty.as_bytes();
        let plen = param.len();
        let mut i = 0;
        while i < bytes.len() {
            let rest = &ty[i..];
            if rest.starts_with(param) {
                let prev_is_ident = i > 0
                    && bytes
                        .get(i.saturating_sub(1))
                        .copied()
                        .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_');
                let next_is_ident = bytes
                    .get(i.saturating_add(plen))
                    .copied()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_');
                if !prev_is_ident && !next_is_ident {
                    out.push_str(concrete);
                    i = i.saturating_add(plen);
                    continue;
                }
            }
            let Some(ch) = ty[i..].chars().next() else {
                break;
            };
            out.push(ch);
            i = i.saturating_add(ch.len_utf8());
        }
        out
    }

    /// Walk a `SymbolTable`'s module hierarchy looking for a struct by
    /// (unqualified) name; if found, return the type-string of its
    /// `field_name` field. Used by `infer_field_type_from_string` so a
    /// struct nested inside `pub mod m { struct S { ... } }` resolves
    /// even when the impl method body refers to it as just `S`.
    fn lookup_field_in_modules(
        symbols: &super::SymbolTable,
        struct_name: &str,
        field_name: &str,
    ) -> Option<String> {
        for module_info in symbols.modules.values() {
            if let Some(struct_info) = module_info.symbols.get_struct(struct_name) {
                for field in &struct_info.fields {
                    if field.name == field_name {
                        return Some(Self::type_to_string(&field.ty));
                    }
                }
            }
            if let Some(ty) =
                Self::lookup_field_in_modules(&module_info.symbols, struct_name, field_name)
            {
                return Some(ty);
            }
        }
        None
    }

    /// Infer the return type of a method call given the receiver's type.
    ///
    /// Searches impl blocks in the current file and module cache for a matching
    /// method. Falls back to trait method signatures for types that implement the
    /// trait. Returns "Unknown" when the method cannot be resolved.
    fn infer_method_return_type(
        &self,
        receiver_type: &str,
        method_name: &str,
        file: &File,
    ) -> String {
        if receiver_type == "Unknown" || receiver_type.contains("Unknown") {
            return "Unknown".to_string();
        }
        let (base, is_optional) = receiver_type
            .strip_suffix('?')
            .map_or((receiver_type, false), |s| (s, true));
        let lookup_name = base.split_once('<').map_or(base, |(n, _)| n);
        // Parse generic args from the receiver type, if any (`Box<Number>`
        // → `["Number"]`). Used to substitute the impl method's
        // `TypeParam` references with concrete types.
        let receiver_type_args = Self::parse_receiver_type_args(base);

        let substitute = |ret: String| -> String {
            if receiver_type_args.is_empty() {
                return ret;
            }
            // Look up the struct's generic parameter names and substitute.
            let generics = self
                .symbols
                .structs
                .get(lookup_name)
                .map(|s| s.generics.clone())
                .or_else(|| {
                    self.symbols
                        .enums
                        .get(lookup_name)
                        .map(|e| e.generics.clone())
                })
                .unwrap_or_default();
            let mut out = ret;
            for (i, param) in generics.iter().enumerate() {
                if let Some(arg) = receiver_type_args.get(i) {
                    out = Self::substitute_type_string(&out, &param.name.name, arg);
                }
            }
            out
        };
        let wrap_if_optional = |ret: String| -> String {
            let ret = substitute(ret);
            if is_optional && !ret.ends_with('?') && ret != "Nil" {
                format!("{ret}?")
            } else {
                ret
            }
        };

        // Current file impl blocks
        if let Some(ret) = Self::find_method_return_in_file(lookup_name, method_name, file) {
            return wrap_if_optional(ret);
        }
        // Module cache impl blocks
        for (cached_file, _) in self.module_cache.values() {
            if let Some(ret) =
                Self::find_method_return_in_file(lookup_name, method_name, cached_file)
            {
                return wrap_if_optional(ret);
            }
        }
        // Trait method signatures
        if let Some(ret) = self.find_trait_method_return(lookup_name, method_name) {
            return wrap_if_optional(ret);
        }
        // Generic type parameter: look up its trait bounds in the active
        // generic-scope stack, then search those traits for the method.
        if let Some(constraints) = self.get_type_parameter_constraints(lookup_name) {
            for trait_name in &constraints {
                if let Some(ret) = self.find_trait_method_return(trait_name, method_name) {
                    return wrap_if_optional(ret);
                }
            }
        }
        "Unknown".to_string()
    }

    /// Search impl blocks in a file for `method_name` on `type_name`.
    fn find_method_return_in_file(
        type_name: &str,
        method_name: &str,
        file: &File,
    ) -> Option<String> {
        for stmt in &file.statements {
            if let Statement::Definition(def) = stmt {
                if let Definition::Impl(impl_def) = &**def {
                    if impl_def.name.name == type_name {
                        for func in &impl_def.functions {
                            if func.name.name == method_name {
                                return Some(func.return_type.as_ref().map_or_else(
                                    || "Nil".to_string(),
                                    |t| Self::type_to_string(t),
                                ));
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Look up a trait method signature.
    ///
    /// Tries two interpretations of `type_name`:
    /// 1. As a trait name itself — used when resolving methods on a
    ///    generic type parameter via its trait constraints.
    /// 2. As a struct name — walks every trait the struct implements
    ///    and searches their methods.
    fn find_trait_method_return(&self, type_name: &str, method_name: &str) -> Option<String> {
        if let Some(trait_info) = self.symbols.get_trait(type_name) {
            for method in &trait_info.methods {
                if method.name.name == method_name {
                    return Some(
                        method
                            .return_type
                            .as_ref()
                            .map_or_else(|| "Nil".to_string(), Self::type_to_string),
                    );
                }
            }
        }
        let trait_names = self.symbols.get_all_traits_for_struct(type_name);
        for trait_name in trait_names {
            if let Some(trait_info) = self.symbols.get_trait(&trait_name) {
                for method in &trait_info.methods {
                    if method.name.name == method_name {
                        return Some(
                            method
                                .return_type
                                .as_ref()
                                .map_or_else(|| "Nil".to_string(), Self::type_to_string),
                        );
                    }
                }
            }
        }
        None
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
