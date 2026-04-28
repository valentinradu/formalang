//! Static helpers that build a foreign module's symbol table.
//!
//! These functions are used by Pass 0 (`parse_and_analyze_module`) to
//! populate a fresh `SymbolTable` for an imported module without holding
//! `&mut self` on the analyzer. They mirror the `collect_definition_*`
//! family in [`pass1_symbols`](super::pass1_symbols), but accept the
//! target table and error list as parameters.
//!
//! Also hosts [`SemanticAnalyzer::import_symbol`], which routes a single
//! item from a module's table into the analyzer's own table.

use super::helpers::is_primitive_name;
use super::module_resolver::ModuleResolver;
use super::symbol_table::{self, SymbolTable};
use super::SemanticAnalyzer;
use crate::ast::Definition;
use crate::error::CompilerError;
use crate::location::Span;
use std::collections::HashMap;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Helper to collect a definition into a symbol table (static version for module parsing)
    pub(super) fn collect_definition_into(
        symbols: &mut SymbolTable,
        errors: &mut Vec<CompilerError>,
        def: &Definition,
    ) {
        match def {
            Definition::Trait(trait_def) => {
                Self::collect_trait_into(symbols, errors, trait_def);
            }
            Definition::Struct(struct_def) => {
                Self::collect_struct_into(symbols, errors, struct_def);
            }
            Definition::Impl(impl_def) => {
                Self::collect_impl_into(symbols, errors, impl_def);
            }
            Definition::Enum(enum_def) => {
                Self::collect_enum_into(symbols, errors, enum_def);
            }
            Definition::Module(module_def) => {
                Self::collect_module_into(symbols, errors, module_def);
            }
            Definition::Function(func_def) => {
                Self::collect_function_into(symbols, errors, func_def);
            }
        }
    }

    fn collect_trait_into(
        symbols: &mut SymbolTable,
        errors: &mut Vec<CompilerError>,
        trait_def: &crate::ast::TraitDef,
    ) {
        use symbol_table::FieldInfo;

        if is_primitive_name(&trait_def.name.name) {
            errors.push(CompilerError::PrimitiveRedefinition {
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

        if let Some((kind, _)) = symbols.define_trait(
            trait_def.name.name.clone(),
            trait_def.visibility,
            trait_def.span,
            trait_def.generics.clone(),
            fields,
            composed_traits,
            trait_def.methods.clone(),
            trait_def.doc.clone(),
        ) {
            errors.push(CompilerError::DuplicateDefinition {
                name: format!(
                    "{} (already defined as {})",
                    trait_def.name.name,
                    kind.as_str()
                ),
                span: trait_def.name.span,
            });
        }
    }

    fn collect_struct_into(
        symbols: &mut SymbolTable,
        errors: &mut Vec<CompilerError>,
        struct_def: &crate::ast::StructDef,
    ) {
        use symbol_table::FieldInfo;

        if is_primitive_name(&struct_def.name.name) {
            errors.push(CompilerError::PrimitiveRedefinition {
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

        if let Some((kind, _)) = symbols.define_struct(
            struct_def.name.name.clone(),
            struct_def.visibility,
            struct_def.span,
            struct_def.generics.clone(),
            fields,
            struct_def.doc.clone(),
        ) {
            errors.push(CompilerError::DuplicateDefinition {
                name: format!(
                    "{} (already defined as {})",
                    struct_def.name.name,
                    kind.as_str()
                ),
                span: struct_def.name.span,
            });
        }
    }

    fn collect_impl_into(
        symbols: &mut SymbolTable,
        errors: &mut Vec<CompilerError>,
        impl_def: &crate::ast::ImplDef,
    ) {
        use symbol_table::ImplInfo;
        if let Some(trait_ident) = &impl_def.trait_name {
            // Trait implementation: impl Trait for Struct
            // Existence validation is deferred to collect_definition.
            symbols
                .trait_impls
                .entry(impl_def.name.name.clone())
                .or_default()
                .push(symbol_table::TraitImplInfo {
                    trait_name: trait_ident.name.clone(),
                    struct_name: impl_def.name.name.clone(),
                    generics: impl_def.generics.clone(),
                    span: impl_def.span,
                });
        } else {
            let info = ImplInfo {
                struct_name: impl_def.name.name.clone(),
                generics: impl_def.generics.clone(),
                span: impl_def.span,
            };
            if let Some((kind, _)) =
                symbols.define_impl(&impl_def.name.name, info, impl_def.is_extern)
            {
                errors.push(CompilerError::DuplicateDefinition {
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

    fn collect_enum_into(
        symbols: &mut SymbolTable,
        errors: &mut Vec<CompilerError>,
        enum_def: &crate::ast::EnumDef,
    ) {
        use symbol_table::FieldInfo;
        if is_primitive_name(&enum_def.name.name) {
            errors.push(CompilerError::PrimitiveRedefinition {
                name: enum_def.name.name.clone(),
                span: enum_def.name.span,
            });
            return;
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

        if let Some((kind, _)) = symbols.define_enum(
            enum_def.name.name.clone(),
            enum_def.visibility,
            enum_def.span,
            enum_def.generics.clone(),
            variants,
            variant_fields,
            Vec::new(),
            enum_def.doc.clone(),
        ) {
            errors.push(CompilerError::DuplicateDefinition {
                name: format!(
                    "{} (already defined as {})",
                    enum_def.name.name,
                    kind.as_str()
                ),
                span: enum_def.name.span,
            });
        }
    }

    fn collect_module_into(
        symbols: &mut SymbolTable,
        errors: &mut Vec<CompilerError>,
        module_def: &crate::ast::ModuleDef,
    ) {
        let mut module_symbols = SymbolTable::new();
        for nested_def in &module_def.definitions {
            Self::collect_definition_into(&mut module_symbols, errors, nested_def);
        }
        if let Some((kind, _)) = symbols.define_module(
            module_def.name.name.clone(),
            module_def.visibility,
            module_def.span,
            module_symbols,
        ) {
            errors.push(CompilerError::DuplicateDefinition {
                name: format!(
                    "{} (already defined as {})",
                    module_def.name.name,
                    kind.as_str()
                ),
                span: module_def.name.span,
            });
        }
    }

    fn collect_function_into(
        symbols: &mut SymbolTable,
        errors: &mut Vec<CompilerError>,
        func_def: &crate::ast::FunctionDef,
    ) {
        use symbol_table::ParamInfo;

        if is_primitive_name(&func_def.name.name) {
            errors.push(CompilerError::PrimitiveRedefinition {
                name: func_def.name.name.clone(),
                span: func_def.name.span,
            });
            return;
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

        if let Some((kind, _)) = symbols.define_function(
            func_def.name.name.clone(),
            func_def.visibility,
            func_def.span,
            params,
            func_def.return_type.clone(),
            func_def.generics.clone(),
            func_def.doc.clone(),
        ) {
            errors.push(CompilerError::DuplicateDefinition {
                name: format!(
                    "{} (already defined as {})",
                    func_def.name.name,
                    kind.as_str()
                ),
                span: func_def.name.span,
            });
        }
    }

    /// Import a single symbol from a module
    pub(super) fn import_symbol(
        &mut self,
        name: &str,
        module_symbols: &SymbolTable,
        module_path: &std::path::Path,
        logical_path: Vec<String>,
        span: Span,
    ) {
        use symbol_table::ImportError;

        match self.symbols.import_symbol(
            name,
            module_symbols,
            module_path.to_path_buf(),
            logical_path,
        ) {
            Ok(()) => {
                // Success
            }
            Err(ImportError::PrivateItem { name, kind: _ }) => {
                self.errors.push(CompilerError::PrivateImport {
                    name: format!("{} from module {}", name, module_path.display()),
                    span,
                });
            }
            Err(ImportError::ItemNotFound { name, available }) => {
                self.errors.push(CompilerError::ImportItemNotFound {
                    item: name,
                    module: module_path.display().to_string(),
                    available: available.join(", "),
                    span,
                });
            }
        }
    }
}
