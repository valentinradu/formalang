//! Pass 2: walk every definition and validate that the types it mentions
//! actually exist (and respect generic constraints / trait-as-value rules).

mod key_types;
mod validate;

pub(in crate::semantic) use validate::float_key_in;

use super::module_resolver::ModuleResolver;
use super::symbol_table::{self, SymbolTable};
use super::SemanticAnalyzer;
use crate::ast::{Definition, File, Statement, StructDef, TraitDef};
use crate::error::CompilerError;
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
                                self.validate_type(&field.ty, field.span);
                            }
                        }
                        self.pop_generic_scope();
                    }
                    Definition::Module(module_def) => {
                        self.resolve_module_types(module_def, file);
                    }
                    Definition::Function(func_def) => {
                        self.validate_standalone_function(func_def.as_ref(), file);
                    }
                }
            }
        }
    }

    /// Recurse into a nested module: pulls its symbols into scope, walks its
    /// definitions, then restores the parent scope.
    pub(super) fn resolve_module_types(&mut self, module_def: &crate::ast::ModuleDef, file: &File) {
        let shadowed = self.enter_module_scope(module_def);

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
                            self.validate_type(&field.ty, field.span);
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

        self.leave_module_scope(shadowed);
    }

    /// Bring the traits, structs, enums and functions of `module_def`
    /// into scope, so the code of the module names them without the
    /// module prefix. A module name shadows an outer name. Returns the
    /// outer entries that the module shadows, for
    /// [`Self::leave_module_scope`].
    pub(in crate::semantic) fn enter_module_scope(
        &mut self,
        module_def: &crate::ast::ModuleDef,
    ) -> ShadowedNames {
        let mut module_symbols = SymbolTable::new();
        // Pass 1 has reported the errors of these definitions already.
        let mut already_reported = Vec::new();
        for def in &module_def.definitions {
            Self::collect_definition_into(&mut module_symbols, &mut already_reported, def);
        }
        self.module_path.push(module_def.name.name.clone());
        let mut shadowed = ShadowedNames::default();
        for (name, info) in module_symbols.traits {
            let outer = self.symbols.traits.insert(name.clone(), info);
            shadowed.traits.push((name, outer));
        }
        for (name, info) in module_symbols.structs {
            let outer = self.symbols.structs.insert(name.clone(), info);
            shadowed.structs.push((name, outer));
        }
        for (name, info) in module_symbols.enums {
            let outer = self.symbols.enums.insert(name.clone(), info);
            shadowed.enums.push((name, outer));
        }
        for (name, overloads) in module_symbols.functions {
            let outer = self.symbols.functions.insert(name.clone(), overloads);
            shadowed.functions.push((name, outer));
        }
        shadowed
    }

    /// Restore the outer entries that [`Self::enter_module_scope`]
    /// shadowed, and remove the names that only the module declares.
    pub(in crate::semantic) fn leave_module_scope(&mut self, shadowed: ShadowedNames) {
        fn restore<V>(map: &mut HashMap<String, V>, entries: Vec<(String, Option<V>)>) {
            for (name, outer) in entries {
                match outer {
                    Some(outer) => {
                        map.insert(name, outer);
                    }
                    None => {
                        map.remove(&name);
                    }
                }
            }
        }
        restore(&mut self.symbols.traits, shadowed.traits);
        restore(&mut self.symbols.structs, shadowed.structs);
        restore(&mut self.symbols.enums, shadowed.enums);
        restore(&mut self.symbols.functions, shadowed.functions);
        self.module_path.pop();
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
            self.validate_type(&field.ty, field.span);
        }

        self.pop_generic_scope();
    }

    pub(super) fn resolve_struct_types(&mut self, struct_def: &StructDef) {
        self.push_generic_scope(&struct_def.generics);
        for field in &struct_def.fields {
            self.validate_type(&field.ty, field.span);
        }
        self.pop_generic_scope();
    }
}

/// The outer symbol-table entries that the names of an inline module
/// shadow while the code of that module is checked. `None` marks a
/// name that only the module declares.
#[derive(Default)]
pub(in crate::semantic) struct ShadowedNames {
    traits: Vec<(String, Option<symbol_table::TraitInfo>)>,
    structs: Vec<(String, Option<symbol_table::StructInfo>)>,
    enums: Vec<(String, Option<symbol_table::EnumInfo>)>,
    functions: Vec<(String, Option<Vec<symbol_table::FunctionInfo>>)>,
}
