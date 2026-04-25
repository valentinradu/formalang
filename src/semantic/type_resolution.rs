use super::module_resolver::ModuleResolver;
use super::symbol_table::{self, FieldInfo, SymbolTable};
use super::type_graph::TypeGraph;
use super::SemanticAnalyzer;
use crate::ast::{Definition, File, Statement, StructDef, TraitDef, Type};
use crate::error::CompilerError;
use crate::location::Span;
use std::collections::HashMap;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Pass 2: Resolve type references
    /// Ensure all type references point to defined types
    pub(super) fn resolve_types(&mut self, file: &File) {
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                match &**def {
                    Definition::Trait(trait_def) => {
                        self.resolve_trait_types(trait_def);
                    }
                    Definition::Struct(struct_def) => {
                        self.resolve_struct_types(struct_def);
                    }
                    Definition::Impl(impl_def) => {
                        // Audit #12/#27: merge target struct/enum generics into the
                        // impl scope so method bodies see trait bounds declared on T.
                        self.push_impl_generic_scope(&impl_def.generics, &impl_def.name.name);

                        // Set current impl struct for field type resolution
                        self.current_impl_struct = Some(impl_def.name.name.clone());

                        // Clear local let bindings for this impl block
                        self.local_let_bindings.clear();

                        // Validate functions in impl block
                        for func in &impl_def.functions {
                            self.validate_function_return_type(func, file);
                        }

                        // Clear impl struct context and local bindings
                        self.current_impl_struct = None;
                        self.local_let_bindings.clear();

                        // Pop generic scope
                        self.pop_generic_scope();
                    }
                    Definition::Enum(enum_def) => {
                        // Push generic scope for this definition
                        self.push_generic_scope(&enum_def.generics);

                        // Resolve associated data types (named fields)
                        for variant in &enum_def.variants {
                            for field in &variant.fields {
                                self.validate_type(&field.ty);
                            }
                        }

                        // Pop generic scope
                        self.pop_generic_scope();
                    }
                    Definition::Module(module_def) => {
                        // Temporarily import module symbols into parent for type resolution
                        // This allows module-internal references to resolve correctly
                        let module_symbols = Self::collect_module_symbols(module_def);

                        // Import module symbols temporarily
                        for (name, trait_info) in &module_symbols.traits {
                            self.symbols.traits.insert(name.clone(), trait_info.clone());
                        }
                        for (name, struct_info) in &module_symbols.structs {
                            self.symbols
                                .structs
                                .insert(name.clone(), struct_info.clone());
                        }
                        for (name, enum_info) in &module_symbols.enums {
                            self.symbols.enums.insert(name.clone(), enum_info.clone());
                        }

                        // Recursively validate types in nested definitions
                        for nested_def in &module_def.definitions {
                            match nested_def {
                                Definition::Trait(trait_def) => {
                                    self.resolve_trait_types(trait_def);
                                }
                                Definition::Struct(struct_def) => {
                                    self.resolve_struct_types(struct_def);
                                }
                                Definition::Impl(impl_def) => {
                                    self.push_impl_generic_scope(
                                        &impl_def.generics,
                                        &impl_def.name.name,
                                    );
                                    self.current_impl_struct = Some(impl_def.name.name.clone());
                                    self.local_let_bindings.clear();
                                    for func in &impl_def.functions {
                                        self.validate_function_return_type(func, file);
                                    }
                                    self.current_impl_struct = None;
                                    self.local_let_bindings.clear();
                                    self.pop_generic_scope();
                                }
                                Definition::Enum(enum_def) => {
                                    self.push_generic_scope(&enum_def.generics);
                                    for variant in &enum_def.variants {
                                        for field in &variant.fields {
                                            self.validate_type(&field.ty);
                                        }
                                    }
                                    self.pop_generic_scope();
                                }
                                Definition::Module(nested_module) => {
                                    // Recursively process nested modules
                                    self.resolve_module_types(nested_module, file);
                                }
                                Definition::Function(func_def) => {
                                    // Validate function parameter and return types
                                    self.validate_standalone_function(func_def.as_ref(), file);
                                }
                            }
                        }

                        // Remove temporarily imported symbols (restore parent scope)
                        for name in module_symbols.traits.keys() {
                            self.symbols.traits.remove(name);
                        }
                        for name in module_symbols.structs.keys() {
                            self.symbols.structs.remove(name);
                        }
                        for name in module_symbols.enums.keys() {
                            self.symbols.enums.remove(name);
                        }
                    }
                    Definition::Function(func_def) => {
                        // Validate standalone function parameter and return types
                        self.validate_standalone_function(func_def.as_ref(), file);
                    }
                }
            }
        }
    }

    /// Collect symbols from a module definition for temporary import
    pub(super) fn collect_module_symbols(module_def: &crate::ast::ModuleDef) -> SymbolTable {
        let mut symbols = SymbolTable::new();
        for def in &module_def.definitions {
            match def {
                Definition::Trait(trait_def) => {
                    let fields: Vec<symbol_table::FieldInfo> = trait_def
                        .fields
                        .iter()
                        .map(|f| symbol_table::FieldInfo {
                            name: f.name.name.clone(),
                            ty: f.ty.clone(),
                            doc: f.doc.clone(),
                        })
                        .collect();
                    let composed_traits: Vec<String> =
                        trait_def.traits.iter().map(|t| t.name.clone()).collect();
                    symbols.define_trait(
                        trait_def.name.name.clone(),
                        trait_def.visibility,
                        trait_def.span,
                        trait_def.generics.clone(),
                        fields,
                        composed_traits,
                        trait_def.methods.clone(),
                        trait_def.doc.clone(),
                    );
                }
                Definition::Struct(struct_def) => {
                    let fields: Vec<_> = struct_def
                        .fields
                        .iter()
                        .map(|f| symbol_table::FieldInfo {
                            name: f.name.name.clone(),
                            ty: f.ty.clone(),
                            doc: f.doc.clone(),
                        })
                        .collect();
                    symbols.define_struct(
                        struct_def.name.name.clone(),
                        struct_def.visibility,
                        struct_def.span,
                        struct_def.generics.clone(),
                        fields,
                        struct_def.doc.clone(),
                    );
                }
                Definition::Enum(enum_def) => {
                    let variants: HashMap<String, (usize, Span)> = enum_def
                        .variants
                        .iter()
                        .map(|v| (v.name.name.clone(), (v.fields.len(), v.span)))
                        .collect();
                    let variant_fields: HashMap<String, Vec<FieldInfo>> = enum_def
                        .variants
                        .iter()
                        .map(|v| {
                            (
                                v.name.name.clone(),
                                v.fields
                                    .iter()
                                    .map(|f| FieldInfo {
                                        name: f.name.name.clone(),
                                        ty: f.ty.clone(),
                                        doc: f.doc.clone(),
                                    })
                                    .collect(),
                            )
                        })
                        .collect();
                    symbols.define_enum(
                        enum_def.name.name.clone(),
                        enum_def.visibility,
                        enum_def.span,
                        enum_def.generics.clone(),
                        variants,
                        variant_fields,
                        Vec::new(), // Enums don't support inline trait syntax yet
                        enum_def.doc.clone(),
                    );
                }
                Definition::Impl(_) | Definition::Module(_) | Definition::Function(_) => {}
            }
        }
        symbols
    }

    /// Resolve types in a module definition (recursive)
    pub(super) fn resolve_module_types(&mut self, module_def: &crate::ast::ModuleDef, file: &File) {
        // Temporarily import module symbols
        let module_symbols = Self::collect_module_symbols(module_def);
        for (name, trait_info) in &module_symbols.traits {
            self.symbols.traits.insert(name.clone(), trait_info.clone());
        }
        for (name, struct_info) in &module_symbols.structs {
            self.symbols
                .structs
                .insert(name.clone(), struct_info.clone());
        }
        for (name, enum_info) in &module_symbols.enums {
            self.symbols.enums.insert(name.clone(), enum_info.clone());
        }

        for nested_def in &module_def.definitions {
            match nested_def {
                Definition::Trait(trait_def) => {
                    self.resolve_trait_types(trait_def);
                }
                Definition::Struct(struct_def) => {
                    self.resolve_struct_types(struct_def);
                }
                Definition::Impl(impl_def) => {
                    // Audit #37: previously this arm pushed the generic
                    // scope and immediately cleared it without running
                    // return-type validation, so module-nested impl
                    // methods (e.g. `pub mod m { impl Foo { fn bar() -> X
                    // { ... } } }`) escaped Pass 2.
                    self.push_impl_generic_scope(&impl_def.generics, &impl_def.name.name);
                    self.current_impl_struct = Some(impl_def.name.name.clone());
                    self.local_let_bindings.clear();
                    for func in &impl_def.functions {
                        self.validate_function_return_type(func, file);
                    }
                    self.current_impl_struct = None;
                    self.local_let_bindings.clear();
                    self.pop_generic_scope();
                }
                Definition::Enum(enum_def) => {
                    self.push_generic_scope(&enum_def.generics);
                    for variant in &enum_def.variants {
                        for field in &variant.fields {
                            self.validate_type(&field.ty);
                        }
                    }
                    self.pop_generic_scope();
                }
                Definition::Module(nested_module) => {
                    // Recursively process nested modules
                    self.resolve_module_types(nested_module, file);
                }
                Definition::Function(func_def) => {
                    // Validate function parameter and return types
                    self.validate_standalone_function(func_def.as_ref(), file);
                }
            }
        }

        // Remove temporarily imported symbols
        for name in module_symbols.traits.keys() {
            self.symbols.traits.remove(name);
        }
        for name in module_symbols.structs.keys() {
            self.symbols.structs.remove(name);
        }
        for name in module_symbols.enums.keys() {
            self.symbols.enums.remove(name);
        }
    }

    /// Resolve types in a trait definition
    pub(super) fn resolve_trait_types(&mut self, trait_def: &TraitDef) {
        // Push generic scope for this definition
        self.push_generic_scope(&trait_def.generics);

        // Validate trait composition (traits list)
        for trait_ref in &trait_def.traits {
            if self.symbols.get_trait(&trait_ref.name).is_some() {
                // OK: trait exists
            } else {
                // Check if it's defined as something else
                if self.symbols.is_struct(&trait_ref.name) {
                    self.errors.push(CompilerError::NotATrait {
                        name: trait_ref.name.clone(),
                        actual_kind: "struct".to_string(),
                        span: trait_ref.span,
                    });
                } else if self.symbols.is_enum(&trait_ref.name) {
                    self.errors.push(CompilerError::NotATrait {
                        name: trait_ref.name.clone(),
                        actual_kind: "enum".to_string(),
                        span: trait_ref.span,
                    });
                } else {
                    self.errors.push(CompilerError::UndefinedTrait {
                        name: trait_ref.name.clone(),
                        span: trait_ref.span,
                    });
                }
            }
        }

        // Validate field types
        for field in &trait_def.fields {
            self.validate_type(&field.ty);
        }

        // Pop generic scope
        self.pop_generic_scope();
    }

    /// Resolve types in a struct definition
    pub(super) fn resolve_struct_types(&mut self, struct_def: &StructDef) {
        // Push generic scope for this definition
        self.push_generic_scope(&struct_def.generics);

        // Validate field types
        for field in &struct_def.fields {
            self.validate_type(&field.ty);
        }

        // Pop generic scope
        self.pop_generic_scope();
    }

    /// Validate a type reference
    pub(super) fn validate_type(&mut self, ty: &Type) {
        match ty {
            Type::Primitive(_) => {}
            Type::Ident(ident) => self.validate_ident_type(ident),
            Type::Array(element_ty) => self.validate_type(element_ty),
            Type::Optional(inner_ty) => self.validate_type(inner_ty),
            Type::Tuple(fields) => {
                for field in fields {
                    self.validate_type(&field.ty);
                }
            }
            Type::Generic { name, args, span } => self.validate_generic_type(name, args, *span),
            Type::Dictionary { key, value } => {
                self.validate_type(key);
                self.validate_type(value);
            }
            Type::Closure { params, ret } => {
                for (_, param) in params {
                    self.validate_type(param);
                }
                self.validate_type(ret);
            }
        }
    }

    /// Validate a simple identifier type reference (handles module paths and plain names).
    fn validate_ident_type(&mut self, ident: &crate::ast::Ident) {
        if ident.name.contains("::") {
            let parts: Vec<&str> = ident.name.split("::").collect();
            if parts.len() >= 2 {
                if let Some(error_msg) = self.resolve_nested_module_type(&parts, ident.span) {
                    self.errors.push(CompilerError::UndefinedType {
                        name: error_msg,
                        span: ident.span,
                    });
                }
            } else {
                self.errors.push(CompilerError::UndefinedType {
                    name: format!("invalid module path: {}", ident.name),
                    span: ident.span,
                });
            }
        } else if self.symbols.is_trait(&ident.name) {
            // FormaLang has no dynamic dispatch: a trait name in a
            // value-producing type position (param, return, field,
            // let annotation) means the user expected a trait object
            // value, which the IR does not represent. Tier-1 audit:
            // require `<T: Trait>` instead of `: Trait`.
            self.errors.push(CompilerError::TraitUsedAsValueType {
                trait_name: ident.name.clone(),
                span: ident.span,
            });
        } else if self.symbols.is_type(&ident.name) || self.is_type_parameter(&ident.name) {
            // Valid struct/enum type or generic type parameter — OK.
        } else if ident.name.len() == 1 && ident.name.chars().next().is_some_and(char::is_uppercase)
        {
            self.errors.push(CompilerError::OutOfScopeTypeParameter {
                param: ident.name.clone(),
                span: ident.span,
            });
        } else {
            self.errors.push(CompilerError::UndefinedType {
                name: ident.name.clone(),
                span: ident.span,
            });
        }
    }

    /// Validate a generic type application (e.g., `Container<T, U>`).
    ///
    /// Recurses into nested generic arguments so that constraint violations at
    /// any depth are reported (e.g., `S<S<BadType>>` checks `BadType` too).
    fn validate_generic_type(
        &mut self,
        name: &crate::ast::Ident,
        args: &[Type],
        span: crate::location::Span,
    ) {
        if self.symbols.is_trait(&name.name) {
            // `Trait<X>` in a value position is also banned — same
            // reason as `Trait` (no dynamic dispatch). The fix is the
            // same: `<T: Trait<X>>` (currently unsupported, see
            // generic-trait deferred PR).
            self.errors.push(CompilerError::TraitUsedAsValueType {
                trait_name: name.name.clone(),
                span: name.span,
            });
            return;
        }
        if !self.symbols.is_type(&name.name) {
            self.errors.push(CompilerError::UndefinedType {
                name: name.name.clone(),
                span: name.span,
            });
            return;
        }
        if let Some(expected_params) = self.symbols.get_generics(&name.name) {
            let expected = expected_params.len();
            let actual = args.len();
            if expected != actual {
                self.errors.push(CompilerError::GenericArityMismatch {
                    name: name.name.clone(),
                    expected,
                    actual,
                    span,
                });
            }
        }
        if let Some(expected_params) = self.symbols.get_generics(&name.name) {
            for (i, arg) in args.iter().enumerate() {
                if let Some(param) = expected_params.get(i) {
                    for constraint in &param.constraints {
                        let crate::ast::GenericConstraint::Trait(trait_ref) = constraint;
                        if !self.type_satisfies_trait_constraint(arg, &trait_ref.name) {
                            self.errors.push(CompilerError::GenericConstraintViolation {
                                arg: Self::type_to_string(arg),
                                constraint: trait_ref.name.clone(),
                                span,
                            });
                        }
                    }
                }
            }
        }
        // Recurse into each argument. `validate_type` will re-enter
        // `validate_generic_type` for any nested Type::Generic, so inner
        // constraints are checked too.
        for arg in args {
            self.validate_type(arg);
        }
    }

    /// Resolve a nested module type path (e.g., `["outer", "inner", "Type"]`)
    /// Returns `Some(error_message)` if the type doesn't exist, None if valid
    pub(super) fn resolve_nested_module_type(&self, parts: &[&str], _span: Span) -> Option<String> {
        if parts.is_empty() {
            return Some("empty module path".to_string());
        }

        // The last part is the type name, the rest are module names
        let Some((type_name, module_parts)) = parts.split_last() else {
            return Some("empty module path".to_string());
        };

        // Traverse nested modules
        let mut current_symbols = &self.symbols;
        let mut path_so_far = String::new();

        for (i, module_name) in module_parts.iter().enumerate() {
            if i > 0 {
                path_so_far.push_str("::");
            }
            path_so_far.push_str(module_name);

            if let Some(module_info) = current_symbols.modules.get(*module_name) {
                current_symbols = &module_info.symbols;
            } else {
                return Some(format!("module '{path_so_far}' not found"));
            }
        }

        // Check if type exists in the final module
        if !current_symbols.is_type(type_name) && !current_symbols.is_trait(type_name) {
            return Some(format!(
                "type '{type_name}' not found in module '{path_so_far}'"
            ));
        }

        None // Type is valid
    }

    /// Add type dependencies from a type to the graph
    /// Recursively extracts type names from arrays and optionals
    pub(super) fn add_type_dependencies(graph: &mut TypeGraph, from: &str, ty: &Type) {
        match ty {
            // Primitive types don't create dependencies
            Type::Primitive(_) => {}
            Type::Ident(ident) => {
                // Direct type reference creates a dependency
                graph.add_dependency(from.to_string(), ident.name.clone());
            }
            Type::Array(element_ty) => {
                // Array element type creates a dependency
                // Note: Currently arrays don't break cycles, so [Node] still creates Node -> Node
                Self::add_type_dependencies(graph, from, element_ty);
            }
            Type::Optional(inner_ty) => {
                // Optional inner type creates a dependency
                Self::add_type_dependencies(graph, from, inner_ty);
            }
            Type::Tuple(fields) => {
                // Tuple field types create dependencies
                for field in fields {
                    Self::add_type_dependencies(graph, from, &field.ty);
                }
            }
            Type::Generic { name, args, .. } => {
                // Generic type: Container<T, U> creates dependency on the base type
                // and recursively on all type arguments
                graph.add_dependency(from.to_string(), name.name.clone());
                for arg in args {
                    Self::add_type_dependencies(graph, from, arg);
                }
            }
            Type::Dictionary { key, value } => {
                // Recursively add dependencies for key and value types
                Self::add_type_dependencies(graph, from, key);
                Self::add_type_dependencies(graph, from, value);
            }
            Type::Closure { params, ret } => {
                // Recursively add dependencies for parameter and return types
                for (_, param) in params {
                    Self::add_type_dependencies(graph, from, param);
                }
                Self::add_type_dependencies(graph, from, ret);
            }
        }
    }
}
