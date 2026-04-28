//! Pass 1 — symbol-table construction.
//!
//! Walks every top-level `Statement` in the file and registers each
//! definition (struct / trait / enum / impl / module / function) plus
//! every let-binding into [`SymbolTable`]. Also runs nested-module
//! collection via the static [`SemanticAnalyzer::collect_definition_into`]
//! helper in [`module_collect`](super::module_collect).

use super::helpers::{collect_bindings_from_pattern, is_primitive_name};
use super::module_resolver::ModuleResolver;
use super::symbol_table::{self, SymbolTable};
use super::SemanticAnalyzer;
use crate::ast::{Definition, File, Statement};
use crate::error::CompilerError;
use std::collections::HashMap;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Pass 1: Build symbol table
    /// Collect all definitions and detect duplicates
    pub(super) fn build_symbol_table(&mut self, file: &File) {
        for statement in &file.statements {
            match statement {
                Statement::Use(_) => {
                    // Module resolution handled in Pass 0
                }
                Statement::Let(let_binding) => {
                    // Register all bindings from the pattern (simple, array, struct, tuple)
                    for binding in collect_bindings_from_pattern(&let_binding.pattern) {
                        if is_primitive_name(&binding.name) {
                            self.errors.push(CompilerError::PrimitiveRedefinition {
                                name: binding.name.clone(),
                                span: binding.span,
                            });
                            continue;
                        }
                        if let Some((kind, _)) = self.symbols.define_let(
                            binding.name.clone(),
                            let_binding.visibility,
                            let_binding.span,
                            let_binding.doc.clone(),
                        ) {
                            self.errors.push(CompilerError::DuplicateDefinition {
                                name: format!(
                                    "{} (already defined as {})",
                                    binding.name,
                                    kind.as_str()
                                ),
                                span: binding.span,
                            });
                        }
                    }
                }
                Statement::Definition(def) => {
                    self.collect_definition(def);
                }
            }
        }
    }

    /// Collect a single definition
    fn collect_definition(&mut self, def: &Definition) {
        match def {
            Definition::Trait(trait_def) => self.collect_definition_trait(trait_def),
            Definition::Struct(struct_def) => self.collect_definition_struct(struct_def),
            Definition::Impl(impl_def) => self.collect_definition_impl(impl_def),
            Definition::Enum(enum_def) => self.collect_definition_enum(enum_def),
            Definition::Module(module_def) => self.collect_definition_module(module_def),
            Definition::Function(func_def) => self.collect_definition_function(func_def),
        }
    }

    fn collect_definition_trait(&mut self, trait_def: &crate::ast::TraitDef) {
        use symbol_table::FieldInfo;

        if is_primitive_name(&trait_def.name.name) {
            self.errors.push(CompilerError::PrimitiveRedefinition {
                name: trait_def.name.name.clone(),
                span: trait_def.name.span,
            });
            return;
        }

        let fields: Vec<FieldInfo> = trait_def
            .fields
            .iter()
            .map(|f| FieldInfo {
                name: f.name.name.clone(),
                ty: f.ty.clone(),
                doc: f.doc.clone(),
            })
            .collect();
        let composed_traits: Vec<String> =
            trait_def.traits.iter().map(|t| t.name.clone()).collect();

        if let Some((kind, _)) = self.symbols.define_trait(
            trait_def.name.name.clone(),
            trait_def.visibility,
            trait_def.span,
            trait_def.generics.clone(),
            fields,
            composed_traits,
            trait_def.methods.clone(),
            trait_def.doc.clone(),
        ) {
            self.errors.push(CompilerError::DuplicateDefinition {
                name: format!(
                    "{} (already defined as {})",
                    trait_def.name.name,
                    kind.as_str()
                ),
                span: trait_def.name.span,
            });
        }
    }

    fn collect_definition_struct(&mut self, struct_def: &crate::ast::StructDef) {
        use symbol_table::FieldInfo;

        if is_primitive_name(&struct_def.name.name) {
            self.errors.push(CompilerError::PrimitiveRedefinition {
                name: struct_def.name.name.clone(),
                span: struct_def.name.span,
            });
            return;
        }

        let fields: Vec<FieldInfo> = struct_def
            .fields
            .iter()
            .map(|f| FieldInfo {
                name: f.name.name.clone(),
                ty: f.ty.clone(),
                doc: f.doc.clone(),
            })
            .collect();

        if let Some((kind, _)) = self.symbols.define_struct(
            struct_def.name.name.clone(),
            struct_def.visibility,
            struct_def.span,
            struct_def.generics.clone(),
            fields,
            struct_def.doc.clone(),
        ) {
            self.errors.push(CompilerError::DuplicateDefinition {
                name: format!(
                    "{} (already defined as {})",
                    struct_def.name.name,
                    kind.as_str()
                ),
                span: struct_def.name.span,
            });
        }
    }

    fn collect_definition_impl(&mut self, impl_def: &crate::ast::ImplDef) {
        use symbol_table::ImplInfo;

        // Validate function bodies vs extern status
        for func in &impl_def.functions {
            if impl_def.is_extern && func.body.is_some() {
                self.errors.push(CompilerError::ExternImplWithBody {
                    name: impl_def.name.name.clone(),
                    span: func.name.span,
                });
            } else if !impl_def.is_extern && func.body.is_none() {
                self.errors.push(CompilerError::RegularFnWithoutBody {
                    function: func.name.name.clone(),
                    span: func.name.span,
                });
            }
        }

        let type_exists = self.symbols.get_struct(&impl_def.name.name).is_some()
            || self
                .symbols
                .get_enum_variants(&impl_def.name.name)
                .is_some();

        if !type_exists {
            self.errors.push(CompilerError::UndefinedType {
                name: impl_def.name.name.clone(),
                span: impl_def.span,
            });
            return;
        }

        if let Some(trait_ident) = &impl_def.trait_name {
            if !self.symbols.traits.contains_key(&trait_ident.name) {
                self.errors.push(CompilerError::UndefinedType {
                    name: trait_ident.name.clone(),
                    span: trait_ident.span,
                });
                return;
            }

            if let Err((kind, _)) = self.symbols.define_trait_impl(
                trait_ident.name.clone(),
                impl_def.name.name.clone(),
                impl_def.generics.clone(),
                impl_def.span,
            ) {
                self.errors.push(CompilerError::DuplicateDefinition {
                    name: format!(
                        "impl {} for {} (already defined as {})",
                        trait_ident.name,
                        impl_def.name.name,
                        kind.as_str()
                    ),
                    span: impl_def.span,
                });
            }
        } else {
            let info = ImplInfo {
                struct_name: impl_def.name.name.clone(),
                generics: impl_def.generics.clone(),
                span: impl_def.span,
            };
            if let Some((kind, _)) =
                self.symbols
                    .define_impl(&impl_def.name.name, info, impl_def.is_extern)
            {
                self.errors.push(CompilerError::DuplicateDefinition {
                    name: format!(
                        "{} (already defined as {})",
                        impl_def.name.name,
                        kind.as_str()
                    ),
                    span: impl_def.name.span,
                });
            }
        }
    }

    fn collect_definition_enum(&mut self, enum_def: &crate::ast::EnumDef) {
        use symbol_table::FieldInfo;
        if is_primitive_name(&enum_def.name.name) {
            self.errors.push(CompilerError::PrimitiveRedefinition {
                name: enum_def.name.name.clone(),
                span: enum_def.name.span,
            });
            return;
        }

        let mut seen_variants = std::collections::HashSet::new();
        for variant in &enum_def.variants {
            if !seen_variants.insert(&variant.name.name) {
                self.errors.push(CompilerError::DuplicateDefinition {
                    name: format!(
                        "enum variant '{}' in enum '{}'",
                        variant.name.name, enum_def.name.name
                    ),
                    span: variant.name.span,
                });
            }
        }

        let variants = enum_def
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

        if let Some((kind, _)) = self.symbols.define_enum(
            enum_def.name.name.clone(),
            enum_def.visibility,
            enum_def.span,
            enum_def.generics.clone(),
            variants,
            variant_fields,
            Vec::new(),
            enum_def.doc.clone(),
        ) {
            self.errors.push(CompilerError::DuplicateDefinition {
                name: format!(
                    "{} (already defined as {})",
                    enum_def.name.name,
                    kind.as_str()
                ),
                span: enum_def.name.span,
            });
        }
    }

    fn collect_definition_module(&mut self, module_def: &crate::ast::ModuleDef) {
        let mut module_symbols = SymbolTable::new();
        for nested_def in &module_def.definitions {
            Self::collect_definition_into(&mut module_symbols, &mut self.errors, nested_def);
        }
        if let Some((kind, _)) = self.symbols.define_module(
            module_def.name.name.clone(),
            module_def.visibility,
            module_def.span,
            module_symbols,
        ) {
            self.errors.push(CompilerError::DuplicateDefinition {
                name: format!(
                    "{} (already defined as {})",
                    module_def.name.name,
                    kind.as_str()
                ),
                span: module_def.name.span,
            });
        }
    }

    fn collect_definition_function(&mut self, func_def: &crate::ast::FunctionDef) {
        use symbol_table::ParamInfo;

        if is_primitive_name(&func_def.name.name) {
            self.errors.push(CompilerError::PrimitiveRedefinition {
                name: func_def.name.name.clone(),
                span: func_def.name.span,
            });
            return;
        }

        // `FunctionDef` carries an explicit
        // `is_extern` flag set by the parser (`extern_fn_parser` →
        // `true`, `function_def_parser` → `false`). Cross-check it
        // against `body` so a mismatch — which can happen under parser
        // error recovery, even though the happy-path parsers preserve
        // the invariant — surfaces a meaningful semantic error.
        if func_def.is_extern() && func_def.body.is_some() {
            self.errors.push(CompilerError::ExternFnWithBody {
                function: func_def.name.name.clone(),
                span: func_def.name.span,
            });
        } else if !func_def.is_extern() && func_def.body.is_none() {
            self.errors.push(CompilerError::RegularFnWithoutBody {
                function: func_def.name.name.clone(),
                span: func_def.name.span,
            });
        }

        let params: Vec<ParamInfo> = func_def
            .params
            .iter()
            .map(|p| ParamInfo {
                convention: p.convention,
                external_label: p.external_label.clone(),
                name: p.name.clone(),
                ty: p.ty.clone(),
            })
            .collect();

        if let Some((kind, _)) = self.symbols.define_function(
            func_def.name.name.clone(),
            func_def.visibility,
            func_def.span,
            params,
            func_def.return_type.clone(),
            func_def.generics.clone(),
            func_def.doc.clone(),
        ) {
            self.errors.push(CompilerError::DuplicateDefinition {
                name: format!(
                    "{} (already defined as {})",
                    func_def.name.name,
                    kind.as_str()
                ),
                span: func_def.name.span,
            });
        }
    }
}
