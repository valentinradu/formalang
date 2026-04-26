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
                Literal::Number(_) => SemType::Primitive(PrimitiveType::Number),
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
                // Audit #27: pre-populate each arm's pattern bindings into
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
                // Audit2 B8: extract V from a Dictionary shape.
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
                // Audit #38: prefer the explicit return type when present;
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

    /// Infer the type of an invocation expression (struct instantiation or function call)
    fn infer_type_invocation(
        &self,
        path: &[crate::ast::Ident],
        type_args: &[crate::ast::Type],
        args: &[(Option<crate::ast::Ident>, Expr)],
        file: &File,
    ) -> SemType {
        let name = path
            .iter()
            .map(|id| id.name.as_str())
            .collect::<Vec<_>>()
            .join("::");

        // Closure-typed binding called as a function (`cb()` where
        // `cb: (...) -> R`). Unpack the closure shape structurally
        // and yield its return type.
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
                if let SemType::Closure { return_ty, .. } = SemType::from_legacy_string(&t) {
                    return *return_ty;
                }
            }
        }

        if self.symbols.is_struct(&name) {
            // Struct instantiation — return the struct type, with generic args if present
            if type_args.is_empty() {
                SemType::Named(name)
            } else {
                SemType::Generic {
                    base: name,
                    args: type_args.iter().map(SemType::from_ast).collect(),
                }
            }
        } else if let Some(func_info) = self.symbols.get_function(&name) {
            // User-defined standalone function — return its declared return type
            let raw = func_info
                .return_type
                .as_ref()
                .map_or(SemType::Nil, SemType::from_ast);
            // Tier-1 follow-up to item E2: if the function is generic
            // and the declared return type is itself a generic
            // parameter (`fn id<T>(x: T) -> T`), substitute the
            // inferred concrete type from a matching argument so the
            // call site sees `Number` instead of the placeholder `T`.
            // Other shapes (`[T]`, `T -> U`, etc.) fall through and
            // keep the original generic-param string — extending
            // this to compound shapes lives with the broader generic-
            // function inference work.
            self.specialise_generic_return(func_info, raw, args, file)
        } else if path.len() >= 2 {
            // Audit #27: resolve impl-block static method calls
            // (`Type::method(...)`), enum variant constructors
            // (`Enum::variant(...)`) that weren't rewritten to
            // EnumInstantiation at parse time, and module-qualified
            // function calls (`math::compute(...)`).
            let (Some(first), Some(last)) = (path.first(), path.last()) else {
                return SemType::Unknown;
            };
            let receiver = &first.name;
            let method_name = &last.name;
            if self.symbols.is_struct(receiver) {
                if let Some(ret) = self.infer_method_return_from_impls(receiver, method_name) {
                    return ret;
                }
            }
            if self.symbols.get_enum_variants(receiver).is_some() {
                return SemType::Named(receiver.clone());
            }
            // Module-qualified function: walk through module symbol tables.
            if let Some(ret) = self.lookup_qualified_function_return(path) {
                return ret;
            }
            SemType::Unknown
        } else {
            SemType::Unknown
        }
    }

    /// If the function is generic and its declared return type is a
    /// bare generic-parameter name, substitute the inferred type from
    /// the matching argument. Used by the call-site inference path so
    /// `let n: Number = identity(1)` doesn't surface `T` to the
    /// type-mismatch checker.
    fn specialise_generic_return(
        &self,
        func_info: &super::symbol_table::FunctionInfo,
        raw_ret: SemType,
        args: &[(Option<crate::ast::Ident>, Expr)],
        file: &File,
    ) -> SemType {
        if func_info.generics.is_empty() {
            return raw_ret;
        }
        // Only a bare generic parameter (`-> T`) qualifies for the
        // shortcut substitution. Compound shapes (`[T]`, `T -> U`,
        // ...) are handled by the broader generic-function inference
        // path elsewhere.
        let SemType::Named(ref param_name) = raw_ret else {
            return raw_ret;
        };
        if !func_info
            .generics
            .iter()
            .any(|g| g.name.name == *param_name)
        {
            return raw_ret;
        }
        // Find the first parameter whose declared type is exactly
        // this generic param name; the corresponding argument's
        // inferred type is the substitution.
        for (i, param) in func_info.params.iter().enumerate() {
            let Some(declared) = &param.ty else { continue };
            let crate::ast::Type::Ident(ident) = declared else {
                continue;
            };
            if ident.name != *param_name {
                continue;
            }
            let arg_expr = args
                .iter()
                .find_map(|(n, e)| {
                    n.as_ref()
                        .filter(|name| name.name == param.name.name)
                        .map(|_| e)
                })
                .or_else(|| args.get(i).map(|(_, e)| e));
            if let Some(arg) = arg_expr {
                return self.infer_type_sem(arg, file);
            }
        }
        raw_ret
    }

    /// Resolve a qualified function path (`a::b::compute`) by walking
    /// `self.symbols.modules` segment by segment, then through the
    /// imported-module cache. Returns the function's declared return
    /// type as a string when found.
    fn lookup_qualified_function_return(&self, path: &[crate::ast::Ident]) -> Option<SemType> {
        let last = path.last()?;
        let segments: Vec<&str> = path
            .iter()
            .take(path.len().saturating_sub(1))
            .map(|i| i.name.as_str())
            .collect();
        let look = |symbols: &super::SymbolTable| -> Option<SemType> {
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
                    .map_or(SemType::Nil, SemType::from_ast)
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
    ) -> Option<SemType> {
        let trait_names = self.symbols.get_all_traits_for_struct(struct_name);
        for trait_name in trait_names {
            if let Some(trait_info) = self.symbols.get_trait(&trait_name) {
                for m in &trait_info.methods {
                    if m.name.name == method_name {
                        return Some(
                            m.return_type
                                .as_ref()
                                .map_or(SemType::Nil, SemType::from_ast),
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
    fn infer_type_reference(&self, path: &[crate::ast::Ident], _file: &File) -> SemType {
        let Some(first) = path.first() else {
            return SemType::Unknown;
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

    /// Infer the type of a field access given the receiver's type.
    ///
    /// Handles optional receiver types by stripping `?`, looking up the struct,
    /// and re-wrapping the result as `T?`. Returns [`SemType::Unknown`] when
    /// the receiver is not a known struct or the field doesn't exist.
    fn infer_field_type(&self, obj_type: &SemType, field_name: &str) -> SemType {
        if obj_type.is_indeterminate() {
            return SemType::Unknown;
        }
        let is_optional = obj_type.is_optional();
        let stripped = obj_type.strip_optional();
        // Strip generic args like Container<T> -> Container for struct lookup.
        let lookup_name: &str = match &stripped {
            SemType::Generic { base, .. } | SemType::Named(base) => base.as_str(),
            SemType::Primitive(_)
            | SemType::Array(_)
            | SemType::Optional(_)
            | SemType::Tuple(_)
            | SemType::Dictionary { .. }
            | SemType::Closure { .. }
            | SemType::Unknown
            | SemType::InferredEnum
            | SemType::Nil => return SemType::Unknown,
        };
        let wrap = |ty: SemType| -> SemType {
            if is_optional {
                SemType::optional_of(ty)
            } else {
                ty
            }
        };
        // Top-level struct lookup.
        if let Some(struct_info) = self.symbols.get_struct(lookup_name) {
            for field in &struct_info.fields {
                if field.name == field_name {
                    return wrap(SemType::from_ast(&field.ty));
                }
            }
        }
        // Trait-used-as-type field lookup. Traits in FormaLang declare
        // required fields; an `item: SomeTrait` parameter must allow
        // `item.field` access.
        if let Some(trait_info) = self.symbols.get_trait(lookup_name) {
            if let Some(field) = trait_info.fields.iter().find(|f| f.name == field_name) {
                return wrap(SemType::from_ast(&field.ty));
            }
        }
        // Module-nested struct: walk the symbol table's modules.
        if let Some(ty) = Self::lookup_field_in_modules(&self.symbols, lookup_name, field_name) {
            return wrap(ty);
        }
        // Imported-module struct: walk the analyser's module cache.
        for (_, symbols) in self.module_cache.values() {
            if let Some(struct_info) = symbols.get_struct(lookup_name) {
                for field in &struct_info.fields {
                    if field.name == field_name {
                        return wrap(SemType::from_ast(&field.ty));
                    }
                }
            }
            if let Some(ty) = Self::lookup_field_in_modules(symbols, lookup_name, field_name) {
                return wrap(ty);
            }
        }
        SemType::Unknown
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
    ) -> Option<SemType> {
        for module_info in symbols.modules.values() {
            if let Some(struct_info) = module_info.symbols.get_struct(struct_name) {
                for field in &struct_info.fields {
                    if field.name == field_name {
                        return Some(SemType::from_ast(&field.ty));
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
        receiver_type: &SemType,
        method_name: &str,
        file: &File,
    ) -> SemType {
        if receiver_type.is_indeterminate() {
            return SemType::Unknown;
        }
        let is_optional = receiver_type.is_optional();
        let stripped = receiver_type.strip_optional();
        // Receiver-side generic args (`Box<Number>` → `["Number"]`)
        // for substituting the impl method's `TypeParam` references
        // with concrete types.
        let (lookup_name, receiver_type_args): (&str, Vec<SemType>) = match &stripped {
            SemType::Generic { base, args } => (base.as_str(), args.clone()),
            SemType::Named(base) => (base.as_str(), Vec::new()),
            SemType::Primitive(_)
            | SemType::Array(_)
            | SemType::Optional(_)
            | SemType::Tuple(_)
            | SemType::Dictionary { .. }
            | SemType::Closure { .. }
            | SemType::Unknown
            | SemType::InferredEnum
            | SemType::Nil => return SemType::Unknown,
        };

        let substitute = |ret: SemType| -> SemType {
            if receiver_type_args.is_empty() {
                return ret;
            }
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
                    out = out.substitute_named(&param.name.name, arg);
                }
            }
            out
        };
        let wrap_if_optional = |ret: SemType| -> SemType {
            let ret = substitute(ret);
            // Don't double-wrap optional or wrap Nil — preserves prior behaviour.
            if is_optional && !ret.is_optional() && !matches!(ret, SemType::Nil) {
                SemType::optional_of(ret)
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
        SemType::Unknown
    }

    /// Search impl blocks in a file for `method_name` on `type_name`.
    fn find_method_return_in_file(
        type_name: &str,
        method_name: &str,
        file: &File,
    ) -> Option<SemType> {
        for stmt in &file.statements {
            if let Statement::Definition(def) = stmt {
                if let Definition::Impl(impl_def) = &**def {
                    if impl_def.name.name == type_name {
                        for func in &impl_def.functions {
                            if func.name.name == method_name {
                                return Some(
                                    func.return_type
                                        .as_ref()
                                        .map_or(SemType::Nil, SemType::from_ast),
                                );
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
    fn find_trait_method_return(&self, type_name: &str, method_name: &str) -> Option<SemType> {
        if let Some(trait_info) = self.symbols.get_trait(type_name) {
            for method in &trait_info.methods {
                if method.name.name == method_name {
                    return Some(
                        method
                            .return_type
                            .as_ref()
                            .map_or(SemType::Nil, SemType::from_ast),
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
                                .map_or(SemType::Nil, SemType::from_ast),
                        );
                    }
                }
            }
        }
        None
    }

    /// Get the type of a struct field. Returns [`SemType::Unknown`]
    /// when the struct or field cannot be resolved.
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
