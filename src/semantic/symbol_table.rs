use crate::ast::{Expr, GenericParam, Type, Visibility};
use crate::location::Span;
use std::collections::HashMap;
use std::path::PathBuf;

/// Symbol table for tracking all definitions
#[derive(Debug, Clone)]
pub struct SymbolTable {
    /// Unified traits (no model/view distinction)
    pub traits: HashMap<String, TraitInfo>,
    /// Unified structs (replaces models and views)
    pub structs: HashMap<String, StructInfo>,
    /// Impl blocks
    pub impls: HashMap<String, ImplInfo>,
    /// Enums with variant information
    pub enums: HashMap<String, EnumInfo>,
    /// Let bindings with type information
    pub lets: HashMap<String, LetInfo>,
    /// Modules with nested symbol tables
    pub modules: HashMap<String, ModuleInfo>,
    /// Track which module each symbol came from (None = current module)
    module_origins: HashMap<String, Option<PathBuf>>,
}

/// Information about a symbol
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SymbolInfo {
    pub visibility: Visibility,
    pub span: Span,
}

/// Information about a trait with field and mounting point requirements
#[derive(Debug, Clone)]
pub struct TraitInfo {
    pub visibility: Visibility,
    pub span: Span,
    /// Generic parameters
    pub generics: Vec<GenericParam>,
    /// Required fields (name -> type)
    pub fields: HashMap<String, Type>,
    /// Required mounting points (name -> type)
    pub mount_fields: HashMap<String, Type>,
    /// Trait composition list (trait names this trait extends)
    pub composed_traits: Vec<String>,
}

/// Information about a let binding with type
#[derive(Debug, Clone)]
pub struct LetInfo {
    pub visibility: Visibility,
    pub span: Span,
    /// Inferred type of the binding (optional, computed during semantic analysis)
    pub inferred_type: Option<String>,
}

/// Information about a struct (unified, replaces ModelInfo and ViewInfo)
#[derive(Debug, Clone)]
pub struct StructInfo {
    pub visibility: Visibility,
    pub span: Span,
    /// Generic parameters
    pub generics: Vec<GenericParam>,
    /// Implemented trait names
    #[allow(dead_code)]
    pub traits: Vec<String>,
    /// Regular fields
    #[allow(dead_code)]
    pub fields: Vec<FieldInfo>,
    /// Mount fields
    #[allow(dead_code)]
    pub mount_fields: Vec<FieldInfo>,
    /// Track if impl block exists
    pub has_impl: bool,
}

/// Information about a field
#[derive(Debug, Clone)]
pub struct FieldInfo {
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub ty: Type,
}

/// Information about an impl block
#[derive(Debug, Clone)]
pub struct ImplInfo {
    #[allow(dead_code)]
    pub struct_name: String,
    #[allow(dead_code)]
    pub generics: Vec<GenericParam>,
    #[allow(dead_code)]
    pub body: Vec<Expr>,
    pub span: Span,
}

/// Information about an enum with its variants
#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub visibility: Visibility,
    pub span: Span,
    /// Generic parameters
    pub generics: Vec<GenericParam>,
    /// Variant name -> (arity, span)
    pub variants: HashMap<String, (usize, Span)>,
}

/// Information about a module with its nested symbol table
#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub visibility: Visibility,
    pub span: Span,
    /// Nested symbol table containing the module's definitions
    pub symbols: SymbolTable,
}

/// Kind of symbol (for error reporting)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Trait,
    Struct,
    Impl,
    Enum,
    Let,
    Module,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            traits: HashMap::new(),
            structs: HashMap::new(),
            impls: HashMap::new(),
            enums: HashMap::new(),
            lets: HashMap::new(),
            modules: HashMap::new(),
            module_origins: HashMap::new(),
        }
    }

    /// Define a trait (unified)
    #[allow(clippy::too_many_arguments)]
    pub fn define_trait(
        &mut self,
        name: String,
        visibility: Visibility,
        span: Span,
        generics: Vec<GenericParam>,
        fields: HashMap<String, Type>,
        mount_fields: HashMap<String, Type>,
        composed_traits: Vec<String>,
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
                mount_fields,
                composed_traits,
            },
        );
        None
    }

    /// Define a struct (unified)
    #[allow(clippy::too_many_arguments)]
    pub fn define_struct(
        &mut self,
        name: String,
        visibility: Visibility,
        span: Span,
        generics: Vec<GenericParam>,
        traits: Vec<String>,
        fields: Vec<FieldInfo>,
        mount_fields: Vec<FieldInfo>,
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
                traits,
                fields,
                mount_fields,
                has_impl: false,
            },
        );
        None
    }

    /// Define an impl block
    pub fn define_impl(
        &mut self,
        struct_name: String,
        info: ImplInfo,
    ) -> Option<(SymbolKind, Span)> {
        // Check if impl already exists
        if let Some(existing) = self.impls.get(&struct_name) {
            return Some((SymbolKind::Impl, existing.span));
        }

        // Mark struct as having impl
        if let Some(struct_info) = self.structs.get_mut(&struct_name) {
            struct_info.has_impl = true;
        }

        self.impls.insert(struct_name, info);
        None
    }

    /// Define an enum with variants
    pub fn define_enum(
        &mut self,
        name: String,
        visibility: Visibility,
        span: Span,
        generics: Vec<GenericParam>,
        variants: HashMap<String, (usize, Span)>,
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
            },
        );
        None
    }

    /// Get enum variants
    pub fn get_enum_variants(&self, name: &str) -> Option<&HashMap<String, (usize, Span)>> {
        self.enums.get(name).map(|info| &info.variants)
    }

    /// Define a let binding
    pub fn define_let(
        &mut self,
        name: String,
        visibility: Visibility,
        span: Span,
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
            },
        );
        None
    }

    /// Define a module
    pub fn define_module(
        &mut self,
        name: String,
        visibility: Visibility,
        span: Span,
        symbols: SymbolTable,
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

    /// Get the inferred type of a let binding
    pub fn get_let_type(&self, name: &str) -> Option<&str> {
        self.lets
            .get(name)
            .and_then(|info| info.inferred_type.as_deref())
    }

    /// Find a symbol in any table
    fn find_any(&self, name: &str) -> Option<(SymbolKind, Span)> {
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
    pub fn get_trait(&self, name: &str) -> Option<&TraitInfo> {
        self.traits.get(name)
    }

    /// Get struct info
    pub fn get_struct(&self, name: &str) -> Option<&StructInfo> {
        self.structs.get(name)
    }

    /// Get impl info
    #[allow(dead_code)]
    pub fn get_impl(&self, struct_name: &str) -> Option<&ImplInfo> {
        self.impls.get(struct_name)
    }

    /// Check if a name is a struct
    pub fn is_struct(&self, name: &str) -> bool {
        self.structs.contains_key(name)
    }

    /// Check if a name is an enum
    pub fn is_enum(&self, name: &str) -> bool {
        self.enums.contains_key(name)
    }

    /// Check if a name is a let binding
    pub fn is_let(&self, name: &str) -> bool {
        self.lets.contains_key(name)
    }

    /// Check if a name is a type (struct or enum)
    pub fn is_type(&self, name: &str) -> bool {
        self.structs.contains_key(name) || self.enums.contains_key(name)
    }

    /// Get all required fields from a trait (including composed traits)
    pub fn get_all_trait_fields(&self, name: &str) -> HashMap<String, Type> {
        let mut all_fields = HashMap::new();

        if let Some(trait_info) = self.get_trait(name) {
            // Add fields from composed traits first
            for composed_trait in &trait_info.composed_traits {
                let composed_fields = self.get_all_trait_fields(composed_trait);
                all_fields.extend(composed_fields);
            }

            // Add fields from this trait (can override composed trait fields)
            all_fields.extend(trait_info.fields.clone());
        }

        all_fields
    }

    /// Get all required mounting points from a trait (including composed traits)
    pub fn get_all_trait_mounting_points(&self, name: &str) -> HashMap<String, Type> {
        let mut all_mounts = HashMap::new();

        if let Some(trait_info) = self.get_trait(name) {
            // Add mounting points from composed traits first
            for composed_trait in &trait_info.composed_traits {
                let composed_mounts = self.get_all_trait_mounting_points(composed_trait);
                all_mounts.extend(composed_mounts);
            }

            // Add mounting points from this trait
            all_mounts.extend(trait_info.mount_fields.clone());
        }

        all_mounts
    }

    /// Import a symbol from another module
    /// Returns an error if the symbol is private or doesn't exist
    pub fn import_symbol(
        &mut self,
        name: &str,
        module_table: &SymbolTable,
        module_path: PathBuf,
    ) -> Result<(), ImportError> {
        // Check if symbol exists in the module
        let (kind, visibility) = if let Some(info) = module_table.traits.get(name) {
            (SymbolKind::Trait, info.visibility)
        } else if let Some(info) = module_table.structs.get(name) {
            (SymbolKind::Struct, info.visibility)
        } else if let Some(_info) = module_table.impls.get(name) {
            (SymbolKind::Impl, Visibility::Private) // Impls are not importable
        } else if let Some(info) = module_table.enums.get(name) {
            (SymbolKind::Enum, info.visibility)
        } else if let Some(info) = module_table.lets.get(name) {
            (SymbolKind::Let, info.visibility)
        } else if let Some(info) = module_table.modules.get(name) {
            (SymbolKind::Module, info.visibility)
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
                }
            }
            SymbolKind::Impl => {
                // Impls are not importable, skip
            }
            SymbolKind::Enum => {
                if let Some(enum_info) = module_table.enums.get(name) {
                    self.enums.insert(name.to_string(), enum_info.clone());
                }
            }
            SymbolKind::Let => {
                if let Some(let_info) = module_table.lets.get(name) {
                    self.lets.insert(name.to_string(), let_info.clone());
                }
            }
            SymbolKind::Module => {
                if let Some(module_info) = module_table.modules.get(name) {
                    self.modules.insert(name.to_string(), module_info.clone());
                }
            }
        }

        // Track the module origin
        self.module_origins
            .insert(name.to_string(), Some(module_path));

        Ok(())
    }

    /// Get generic parameters for a type (trait, struct, or enum)
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
    pub fn is_trait(&self, name: &str) -> bool {
        self.traits.contains_key(name)
    }

    /// Get all public symbols in this table
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

        symbols.sort();
        symbols
    }
}

/// Errors that can occur during symbol import
#[derive(Debug, Clone, PartialEq)]
pub enum ImportError {
    /// Imported item is not public
    PrivateItem { name: String, kind: SymbolKind },
    /// Imported item not found in module
    ItemNotFound {
        name: String,
        available: Vec<String>,
    },
}

impl SymbolKind {
    pub fn as_str(&self) -> &str {
        match self {
            SymbolKind::Trait => "trait",
            SymbolKind::Struct => "struct",
            SymbolKind::Impl => "impl",
            SymbolKind::Enum => "enum",
            SymbolKind::Let => "let binding",
            SymbolKind::Module => "module",
        }
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}
