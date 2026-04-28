//! Pass 2: walk every definition and validate that the types it mentions
//! actually exist (and respect generic constraints / trait-as-value rules).

mod validate;

use super::module_resolver::ModuleResolver;
use super::symbol_table::{self, FieldInfo, SymbolTable};
use super::SemanticAnalyzer;
use crate::ast::{Definition, File, Statement, StructDef, TraitDef};
use crate::error::CompilerError;
use crate::location::Span;
use std::collections::HashMap;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Pass 2: ensure every type reference points to a defined type.
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
                        // Merge target struct/enum generics into impl scope
                        // so method bodies see trait bounds declared on T.
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
                    Definition::Module(module_def) => {
                        // Temporarily import module symbols so internal
                        // references resolve, then restore parent scope.
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
                                    self.resolve_module_types(nested_module, file);
                                }
                                Definition::Function(func_def) => {
                                    self.validate_standalone_function(func_def.as_ref(), file);
                                }
                            }
                        }

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
                        self.validate_standalone_function(func_def.as_ref(), file);
                    }
                }
            }
        }
    }

    /// Snapshot a module's defined symbols so the parent scope can pull them
    /// in temporarily during type resolution.
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
                        Vec::new(),
                        enum_def.doc.clone(),
                    );
                }
                Definition::Impl(_) | Definition::Module(_) | Definition::Function(_) => {}
            }
        }
        symbols
    }

    /// Recurse into a nested module: pulls its symbols into scope, walks its
    /// definitions, then restores the parent scope.
    pub(super) fn resolve_module_types(&mut self, module_def: &crate::ast::ModuleDef, file: &File) {
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
                    // Push impl-generic scope and run return-type validation —
                    // module-nested impl methods would otherwise escape Pass 2.
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
                    self.resolve_module_types(nested_module, file);
                }
                Definition::Function(func_def) => {
                    self.validate_standalone_function(func_def.as_ref(), file);
                }
            }
        }

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

    pub(super) fn resolve_trait_types(&mut self, trait_def: &TraitDef) {
        self.push_generic_scope(&trait_def.generics);

        for trait_ref in &trait_def.traits {
            if self.symbols.get_trait(&trait_ref.name).is_some() {
                // OK: trait exists
            } else if self.symbols.is_struct(&trait_ref.name) {
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

        for field in &trait_def.fields {
            self.validate_type(&field.ty);
        }

        self.pop_generic_scope();
    }

    pub(super) fn resolve_struct_types(&mut self, struct_def: &StructDef) {
        self.push_generic_scope(&struct_def.generics);
        for field in &struct_def.fields {
            self.validate_type(&field.ty);
        }
        self.pop_generic_scope();
    }
}
