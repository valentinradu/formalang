mod insert;
mod kinds;
mod normalization;

pub use kinds::{
    EnumInfo, FieldInfo, FunctionInfo, ImplInfo, ImportError, LetInfo, ModuleInfo, ParamInfo,
    StructInfo, SymbolKind, TraitImplInfo, TraitInfo,
};

use crate::ast::{GenericParam, Type, Visibility};
use std::collections::HashMap;
use std::path::PathBuf;

/// Symbol table for tracking all definitions
#[derive(Debug, Clone)]
pub struct SymbolTable {
    /// Unified traits (no model/view distinction)
    pub traits: HashMap<String, TraitInfo>,
    /// Unified structs (replaces models and views)
    pub structs: HashMap<String, StructInfo>,
    /// Inherent impl blocks (impl Struct)
    pub impls: HashMap<String, ImplInfo>,
    /// Trait implementations (impl Trait for Struct), keyed by struct name
    pub trait_impls: HashMap<String, Vec<TraitImplInfo>>,
    /// Enums with variant information
    pub enums: HashMap<String, EnumInfo>,
    /// Let bindings with type information
    pub lets: HashMap<String, LetInfo>,
    /// Standalone functions (multiple overloads per name allowed)
    pub functions: HashMap<String, Vec<FunctionInfo>>,
    /// Modules with nested symbol tables
    pub modules: HashMap<String, ModuleInfo>,
    /// Track which module each symbol came from (None = current module)
    module_origins: HashMap<String, Option<PathBuf>>,
    /// Track the logical module path for imported symbols (e.g., `["utils", "helpers"]`)
    module_logical_paths: HashMap<String, Vec<String>>,
}

impl SymbolTable {
    #[must_use]
    pub fn new() -> Self {
        Self {
            traits: HashMap::new(),
            structs: HashMap::new(),
            impls: HashMap::new(),
            trait_impls: HashMap::new(),
            enums: HashMap::new(),
            lets: HashMap::new(),
            functions: HashMap::new(),
            modules: HashMap::new(),
            module_origins: HashMap::new(),
            module_logical_paths: HashMap::new(),
        }
    }

    /// Get all traits implemented by a struct (via impl Trait for Struct blocks)
    #[must_use]
    pub fn get_all_traits_for_struct(&self, struct_name: &str) -> Vec<String> {
        let mut traits = Vec::new();

        // Get traits from trait impl blocks (impl Trait for Struct)
        if let Some(impls) = self.trait_impls.get(struct_name) {
            for impl_info in impls {
                if !traits.contains(&impl_info.trait_name) {
                    traits.push(impl_info.trait_name.clone());
                }
            }
        }

        traits
    }

    /// Get all traits implemented by an enum (from : Trait syntax and impl Trait for Enum)
    #[must_use]
    pub fn get_all_traits_for_enum(&self, enum_name: &str) -> Vec<String> {
        let mut all_traits = Vec::new();

        // Get traits from enum definition (: Trait syntax)
        if let Some(enum_info) = self.enums.get(enum_name) {
            all_traits.extend(enum_info.traits.clone());
        }

        // Get traits from trait_impls (impl Trait for Enum)
        if let Some(impls) = self.trait_impls.get(enum_name) {
            for impl_info in impls {
                if !all_traits.contains(&impl_info.trait_name) {
                    all_traits.push(impl_info.trait_name.clone());
                }
            }
        }

        all_traits
    }

    /// Get enum variants
    #[must_use]
    pub fn get_enum_variants(
        &self,
        name: &str,
    ) -> Option<&HashMap<String, (usize, crate::location::Span)>> {
        self.enums.get(name).map(|info| &info.variants)
    }

    /// Get the first function overload by name (for backward compatibility)
    #[must_use]
    pub fn get_function(&self, name: &str) -> Option<&FunctionInfo> {
        self.functions.get(name).and_then(|v| v.first())
    }

    /// Get all overloads for a function name
    #[must_use]
    pub fn get_function_overloads(&self, name: &str) -> &[FunctionInfo] {
        self.functions.get(name).map_or(&[], |v| v.as_slice())
    }

    /// Get the inferred type of a let binding
    #[must_use]
    pub fn get_let_type(&self, name: &str) -> Option<&str> {
        self.lets
            .get(name)
            .and_then(|info| info.inferred_type.as_deref())
    }

    /// Find a symbol in any table (functions are excluded — they allow overloads)
    pub(super) fn find_any(&self, name: &str) -> Option<(SymbolKind, crate::location::Span)> {
        if let Some(info) = self.traits.get(name) {
            return Some((SymbolKind::Trait, info.span));
        }
        if let Some(info) = self.structs.get(name) {
            return Some((SymbolKind::Struct, info.span));
        }
        if let Some(info) = self.impls.get(name) {
            return Some((SymbolKind::Impl, info.span));
        }
        if let Some(info) = self.enums.get(name) {
            return Some((SymbolKind::Enum, info.span));
        }
        if let Some(info) = self.lets.get(name) {
            return Some((SymbolKind::Let, info.span));
        }
        if let Some(info) = self.modules.get(name) {
            return Some((SymbolKind::Module, info.span));
        }
        None
    }

    /// Get trait info
    #[must_use]
    pub fn get_trait(&self, name: &str) -> Option<&TraitInfo> {
        self.traits.get(name)
    }

    /// Get struct info
    #[must_use]
    pub fn get_struct(&self, name: &str) -> Option<&StructInfo> {
        self.structs.get(name)
    }

    /// Get struct info, supporting module-qualified names like "`fill::Solid`"
    #[must_use]
    pub fn get_struct_qualified(&self, name: &str) -> Option<&StructInfo> {
        // Try direct lookup first
        if let Some(info) = self.structs.get(name) {
            return Some(info);
        }

        // Try module-qualified lookup (e.g., "fill::Solid")
        if let Some((module_name, struct_name)) = name.split_once("::") {
            if let Some(module_info) = self.modules.get(module_name) {
                return module_info.symbols.get_struct_qualified(struct_name);
            }
        }

        None
    }

    /// Get enum info, supporting module-qualified names like "`fill::PatternRepeat`"
    #[must_use]
    pub fn get_enum_qualified(&self, name: &str) -> Option<&EnumInfo> {
        // Try direct lookup first
        if let Some(info) = self.enums.get(name) {
            return Some(info);
        }

        // Try module-qualified lookup (e.g., "fill::PatternRepeat")
        if let Some((module_name, enum_name)) = name.split_once("::") {
            if let Some(module_info) = self.modules.get(module_name) {
                return module_info.symbols.get_enum_qualified(enum_name);
            }
        }

        None
    }

    /// Check if a name is a struct
    #[must_use]
    pub fn is_struct(&self, name: &str) -> bool {
        self.structs.contains_key(name)
    }

    /// Check if a name is an enum
    #[must_use]
    pub fn is_enum(&self, name: &str) -> bool {
        self.enums.contains_key(name)
    }

    /// Check if a name is a let binding
    #[must_use]
    pub fn is_let(&self, name: &str) -> bool {
        self.lets.contains_key(name)
    }

    /// Check if a name is a type (struct or enum)
    #[must_use]
    pub fn is_type(&self, name: &str) -> bool {
        self.structs.contains_key(name) || self.enums.contains_key(name)
    }

    /// Get all required fields from a trait (including composed traits)
    ///
    /// Returns `name -> Type`. Doc comments are not propagated through the
    /// composition flattening — callers needing per-field docs should walk
    /// `TraitInfo.fields` directly on the leaf trait.
    #[must_use]
    pub fn get_all_trait_fields(&self, name: &str) -> HashMap<String, Type> {
        let mut all_fields = HashMap::new();

        if let Some(trait_info) = self.get_trait(name) {
            // Add fields from composed traits first
            for composed_trait in &trait_info.composed_traits {
                let composed_fields = self.get_all_trait_fields(composed_trait);
                all_fields.extend(composed_fields);
            }

            // Add fields from this trait (can override composed trait fields)
            for f in &trait_info.fields {
                all_fields.insert(f.name.clone(), f.ty.clone());
            }
        }

        all_fields
    }

    /// Get generic parameters for a type (trait, struct, or enum)
    #[must_use]
    pub fn get_generics(&self, type_name: &str) -> Option<Vec<GenericParam>> {
        // Check traits
        if let Some(info) = self.traits.get(type_name) {
            return Some(info.generics.clone());
        }

        // Check structs
        if let Some(info) = self.structs.get(type_name) {
            return Some(info.generics.clone());
        }

        // Check enums
        if let Some(info) = self.enums.get(type_name) {
            return Some(info.generics.clone());
        }

        None
    }

    /// Check if a name is a trait
    #[must_use]
    pub fn is_trait(&self, name: &str) -> bool {
        self.traits.contains_key(name)
    }

    /// Get all public symbols in this table
    #[must_use]
    pub fn all_public_symbols(&self) -> Vec<String> {
        let mut symbols = Vec::new();

        for (name, info) in &self.traits {
            if info.visibility == Visibility::Public {
                symbols.push(name.clone());
            }
        }
        for (name, info) in &self.structs {
            if info.visibility == Visibility::Public {
                symbols.push(name.clone());
            }
        }
        for (name, info) in &self.enums {
            if info.visibility == Visibility::Public {
                symbols.push(name.clone());
            }
        }
        for (name, info) in &self.lets {
            if info.visibility == Visibility::Public {
                symbols.push(name.clone());
            }
        }
        for (name, info) in &self.modules {
            if info.visibility == Visibility::Public {
                symbols.push(name.clone());
            }
        }
        for (name, overloads) in &self.functions {
            if overloads.iter().any(|f| f.visibility == Visibility::Public) {
                symbols.push(name.clone());
            }
        }

        symbols.sort();
        symbols
    }

    /// Get the module origin for a symbol.
    ///
    /// Returns `Some(path)` if the symbol was imported from another module,
    /// or `None` if the symbol is defined locally.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the symbol to look up
    ///
    /// # Returns
    ///
    /// * `Some(&PathBuf)` - The filesystem path of the module the symbol was imported from
    /// * `None` - The symbol is local or not found
    #[must_use]
    pub fn get_module_origin(&self, name: &str) -> Option<&PathBuf> {
        self.module_origins.get(name).and_then(|opt| opt.as_ref())
    }

    /// Get the logical module path for an imported symbol.
    ///
    /// Returns `Some(path)` if the symbol was imported from another module,
    /// or `None` if the symbol is local.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the symbol to look up
    ///
    /// # Returns
    ///
    /// * `Some(&Vec<String>)` - The logical module path (e.g., `["utils", "helpers"]`)
    /// * `None` - The symbol is local or not found
    #[must_use]
    pub fn get_module_logical_path(&self, name: &str) -> Option<&Vec<String>> {
        self.module_logical_paths.get(name)
    }

    /// Get the kind of a symbol (struct, trait, enum, etc.)
    #[must_use]
    pub fn get_symbol_kind(&self, name: &str) -> Option<SymbolKind> {
        if self.structs.contains_key(name) {
            Some(SymbolKind::Struct)
        } else if self.traits.contains_key(name) {
            Some(SymbolKind::Trait)
        } else if self.enums.contains_key(name) {
            Some(SymbolKind::Enum)
        } else if self.lets.contains_key(name) {
            Some(SymbolKind::Let)
        } else if self.modules.contains_key(name) {
            Some(SymbolKind::Module)
        } else if self.impls.contains_key(name) {
            Some(SymbolKind::Impl)
        } else if self.functions.contains_key(name) {
            Some(SymbolKind::Function)
        } else {
            None
        }
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}
