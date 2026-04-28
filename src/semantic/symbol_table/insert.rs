use super::normalization::param_signature;
use super::{
    EnumInfo, FieldInfo, FunctionInfo, ImplInfo, ImportError, LetInfo, ModuleInfo, ParamInfo,
    StructInfo, SymbolKind, SymbolTable, TraitImplInfo, TraitInfo,
};
use crate::ast::{FnSig, GenericParam, Type, Visibility};
use crate::location::Span;
use std::collections::HashMap;
use std::path::PathBuf;

impl SymbolTable {
    /// Define a trait
    #[expect(
        clippy::too_many_arguments,
        reason = "trait definition has many independent fields"
    )]
    pub fn define_trait(
        &mut self,
        name: String,
        visibility: Visibility,
        span: Span,
        generics: Vec<GenericParam>,
        fields: Vec<FieldInfo>,
        composed_traits: Vec<String>,
        methods: Vec<FnSig>,
        doc: Option<String>,
    ) -> Option<(SymbolKind, Span)> {
        // Check for duplicates across all symbol types
        if let Some(existing) = self.find_any(&name) {
            return Some(existing);
        }

        self.traits.insert(
            name,
            TraitInfo {
                visibility,
                span,
                generics,
                fields,
                composed_traits,
                methods,
                doc,
            },
        );
        None
    }

    /// Define a struct
    pub fn define_struct(
        &mut self,
        name: String,
        visibility: Visibility,
        span: Span,
        generics: Vec<GenericParam>,
        fields: Vec<FieldInfo>,
        doc: Option<String>,
    ) -> Option<(SymbolKind, Span)> {
        // Check for duplicates across all symbol types
        if let Some(existing) = self.find_any(&name) {
            return Some(existing);
        }

        self.structs.insert(
            name,
            StructInfo {
                visibility,
                span,
                generics,
                fields,
                has_impl: false,
                doc,
            },
        );
        None
    }

    /// Define an impl block.
    ///
    /// `is_extern` distinguishes `extern impl T` (no function bodies) from a
    /// regular `impl T` block. Both are allowed to coexist for the same type.
    pub fn define_impl(
        &mut self,
        struct_name: &str,
        info: ImplInfo,
        is_extern: bool,
    ) -> Option<(SymbolKind, Span)> {
        // Use a distinct key so extern impl and regular impl can coexist.
        let key = if is_extern {
            format!("{struct_name}::extern")
        } else {
            struct_name.to_string()
        };

        // Check if an impl of the same kind already exists
        if let Some(existing) = self.impls.get(&key) {
            return Some((SymbolKind::Impl, existing.span));
        }

        // Mark struct as having (at least one) impl
        if let Some(struct_info) = self.structs.get_mut(struct_name) {
            struct_info.has_impl = true;
        }

        self.impls.insert(key, info);
        None
    }

    /// Register a trait implementation (impl Trait for Struct)
    ///
    /// Returns an error if the trait or struct doesn't exist, or if this
    /// implementation already exists.
    ///
    /// # Errors
    ///
    /// Returns `Err((SymbolKind::Trait, span))` if the trait does not exist,
    /// `Err((SymbolKind::Struct, span))` if the implementing type does not exist,
    /// or `Err((SymbolKind::Impl, existing_span))` if this trait is already implemented.
    pub fn define_trait_impl(
        &mut self,
        trait_name: String,
        struct_name: String,
        generics: Vec<GenericParam>,
        span: Span,
    ) -> Result<(), (SymbolKind, Span)> {
        // Check if trait exists
        if !self.traits.contains_key(&trait_name) {
            // Trait not found - we don't have a span for it, so return the impl span
            return Err((SymbolKind::Trait, span));
        }

        // Check if struct/enum exists
        let type_exists =
            self.structs.contains_key(&struct_name) || self.enums.contains_key(&struct_name);
        if !type_exists {
            return Err((SymbolKind::Struct, span));
        }

        // Check for duplicate implementation
        let existing_impls = self.trait_impls.entry(struct_name.clone()).or_default();
        if let Some(existing) = existing_impls.iter().find(|i| i.trait_name == trait_name) {
            return Err((SymbolKind::Impl, existing.span));
        }

        // Register the implementation
        existing_impls.push(TraitImplInfo {
            trait_name,
            struct_name,
            generics,
            span,
        });

        Ok(())
    }

    /// Define an enum with variants
    #[expect(
        clippy::too_many_arguments,
        reason = "each field captures distinct EnumInfo state; grouping into a struct would add a parallel type with no semantic benefit"
    )]
    pub fn define_enum(
        &mut self,
        name: String,
        visibility: Visibility,
        span: Span,
        generics: Vec<GenericParam>,
        variants: HashMap<String, (usize, Span)>,
        variant_fields: HashMap<String, Vec<FieldInfo>>,
        traits: Vec<String>,
        doc: Option<String>,
    ) -> Option<(SymbolKind, Span)> {
        // Check for duplicates across all symbol types
        if let Some(existing) = self.find_any(&name) {
            return Some(existing);
        }

        self.enums.insert(
            name,
            EnumInfo {
                visibility,
                span,
                generics,
                variants,
                variant_fields,
                traits,
                doc,
            },
        );
        None
    }

    /// Define a let binding
    pub fn define_let(
        &mut self,
        name: String,
        visibility: Visibility,
        span: Span,
        doc: Option<String>,
    ) -> Option<(SymbolKind, Span)> {
        // Check for duplicates across all symbol types
        if let Some(existing) = self.find_any(&name) {
            return Some(existing);
        }

        self.lets.insert(
            name,
            LetInfo {
                visibility,
                span,
                inferred_type: None,
                doc,
            },
        );
        None
    }

    /// Define a standalone function. Multiple definitions with the same name are
    /// stored as overloads; only conflicts with non-function symbols or with an
    /// existing overload of identical signature are rejected.
    #[expect(
        clippy::too_many_arguments,
        reason = "function definition has many independent fields and a builder would just push the boilerplate elsewhere"
    )]
    pub fn define_function(
        &mut self,
        name: String,
        visibility: Visibility,
        span: Span,
        params: Vec<ParamInfo>,
        return_type: Option<Type>,
        generics: Vec<GenericParam>,
        doc: Option<String>,
    ) -> Option<(SymbolKind, Span)> {
        // Only check for conflicts with non-function symbols
        if let Some(info) = self.traits.get(&name) {
            return Some((SymbolKind::Trait, info.span));
        }
        if let Some(info) = self.structs.get(&name) {
            return Some((SymbolKind::Struct, info.span));
        }
        if let Some(info) = self.enums.get(&name) {
            return Some((SymbolKind::Enum, info.span));
        }
        if let Some(info) = self.lets.get(&name) {
            return Some((SymbolKind::Let, info.span));
        }
        if let Some(info) = self.modules.get(&name) {
            return Some((SymbolKind::Module, info.span));
        }

        // Reject identical-signature duplicates (valid overloads must differ in
        // arity or parameter types).
        if let Some(existing) = self.functions.get(&name) {
            let new_sig = param_signature(&params);
            for prior in existing {
                if param_signature(&prior.params) == new_sig {
                    return Some((SymbolKind::Function, prior.span));
                }
            }
        }

        self.functions.entry(name).or_default().push(FunctionInfo {
            visibility,
            span,
            params,
            return_type,
            generics,
            doc,
        });
        None
    }

    /// Define a module
    pub fn define_module(
        &mut self,
        name: String,
        visibility: Visibility,
        span: Span,
        symbols: Self,
    ) -> Option<(SymbolKind, Span)> {
        // Check for duplicates across all symbol types
        if let Some(existing) = self.find_any(&name) {
            return Some(existing);
        }

        self.modules.insert(
            name,
            ModuleInfo {
                visibility,
                span,
                symbols,
            },
        );
        None
    }

    /// Update the inferred type of a let binding
    pub fn set_let_type(&mut self, name: &str, inferred_type: String) {
        if let Some(let_info) = self.lets.get_mut(name) {
            let_info.inferred_type = Some(inferred_type);
        }
    }

    /// Import a symbol from another module
    /// Returns an error if the symbol is private or doesn't exist
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the symbol to import
    /// * `module_table` - The symbol table of the module to import from
    /// * `module_path` - The filesystem path of the module
    /// * `logical_path` - The logical module path (e.g., `["utils", "helpers"]`)
    ///
    /// # Errors
    ///
    /// Returns `Err(ImportError::PrivateItem)` if the symbol exists but is not public,
    /// or `Err(ImportError::ItemNotFound)` if the symbol does not exist in the module.
    pub fn import_symbol(
        &mut self,
        name: &str,
        module_table: &Self,
        module_path: PathBuf,
        logical_path: Vec<String>,
    ) -> Result<(), ImportError> {
        // Check if symbol exists in the module
        // Note: We check structs and enums BEFORE impls because impls are keyed by the same name
        // as their associated type. Impls themselves are not directly importable - they're
        // automatically brought in when you import their struct/enum.
        let (kind, visibility) = if let Some(info) = module_table.traits.get(name) {
            (SymbolKind::Trait, info.visibility)
        } else if let Some(info) = module_table.structs.get(name) {
            (SymbolKind::Struct, info.visibility)
        } else if let Some(info) = module_table.enums.get(name) {
            (SymbolKind::Enum, info.visibility)
        } else if let Some(info) = module_table.lets.get(name) {
            (SymbolKind::Let, info.visibility)
        } else if let Some(info) = module_table.modules.get(name) {
            (SymbolKind::Module, info.visibility)
        } else if let Some(overloads) = module_table.functions.get(name) {
            // Report as public if any overload is public, mirroring how the
            // parser attaches `pub` per declaration.
            let any_public = overloads.iter().any(|o| o.visibility == Visibility::Public);
            let visibility = if any_public {
                Visibility::Public
            } else {
                Visibility::Private
            };
            (SymbolKind::Function, visibility)
        } else {
            return Err(ImportError::ItemNotFound {
                name: name.to_string(),
                available: module_table.all_public_symbols(),
            });
        };

        // Check if symbol is public
        if visibility != Visibility::Public {
            return Err(ImportError::PrivateItem {
                name: name.to_string(),
                kind,
            });
        }

        // Import the symbol (clone it into this table)
        match kind {
            SymbolKind::Trait => {
                if let Some(trait_info) = module_table.traits.get(name) {
                    self.traits.insert(name.to_string(), trait_info.clone());
                }
            }
            SymbolKind::Struct => {
                if let Some(struct_info) = module_table.structs.get(name) {
                    self.structs.insert(name.to_string(), struct_info.clone());
                    // Also import the impl block if it exists
                    if let Some(impl_info) = module_table.impls.get(name) {
                        self.impls.insert(name.to_string(), impl_info.clone());
                    }
                }
            }
            SymbolKind::Impl => {
                // Impls are not importable directly, but are imported with their structs
            }
            SymbolKind::Enum => {
                if let Some(enum_info) = module_table.enums.get(name) {
                    self.enums.insert(name.to_string(), enum_info.clone());
                    // Also import the impl block if it exists
                    if let Some(impl_info) = module_table.impls.get(name) {
                        self.impls.insert(name.to_string(), impl_info.clone());
                    }
                }
            }
            SymbolKind::Let => {
                if let Some(let_info) = module_table.lets.get(name) {
                    self.lets.insert(name.to_string(), let_info.clone());
                }
            }
            SymbolKind::Function => {
                if let Some(overloads) = module_table.functions.get(name) {
                    self.functions.entry(name.to_string()).or_default().extend(
                        overloads
                            .iter()
                            .filter(|o| o.visibility == Visibility::Public)
                            .cloned(),
                    );
                }
            }
            SymbolKind::Module => {
                if let Some(module_info) = module_table.modules.get(name) {
                    self.modules.insert(name.to_string(), module_info.clone());
                }
            }
        }

        // Track the module origin
        // If the symbol was itself imported into the source module (re-export chain),
        // preserve the original origin rather than using the intermediate module.
        let actual_origin = module_table
            .get_module_origin(name)
            .cloned()
            .unwrap_or(module_path);
        self.module_origins
            .insert(name.to_string(), Some(actual_origin));

        // Same for logical path - preserve original if this is a re-export
        let actual_logical_path = module_table
            .get_module_logical_path(name)
            .map_or(logical_path, std::clone::Clone::clone);
        self.module_logical_paths
            .insert(name.to_string(), actual_logical_path);

        Ok(())
    }
}
