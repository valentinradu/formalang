use crate::ast::{FnSig, GenericParam, Type, Visibility};
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

/// Information about a trait with field requirements
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TraitInfo {
    pub visibility: Visibility,
    pub span: Span,
    /// Generic parameters
    pub generics: Vec<GenericParam>,
    /// Required fields, in source order. `Vec` (not map) so order and
    /// doc-comments survive to the IR.
    pub fields: Vec<FieldInfo>,
    /// Trait composition list (trait names this trait extends)
    pub composed_traits: Vec<String>,
    /// Required method signatures declared in the trait body
    pub methods: Vec<FnSig>,
    /// Joined `///` doc comments preceding this trait.
    pub doc: Option<String>,
}

/// Information about a let binding with type
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct LetInfo {
    pub visibility: Visibility,
    pub span: Span,
    /// Inferred type of the binding (optional, computed during semantic analysis)
    pub inferred_type: Option<String>,
    /// Joined `///` doc comments preceding this binding.
    pub doc: Option<String>,
}

/// Information about a struct
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct StructInfo {
    pub visibility: Visibility,
    pub span: Span,
    /// Generic parameters
    pub generics: Vec<GenericParam>,
    /// Regular fields
    pub fields: Vec<FieldInfo>,
    /// Track if impl block exists
    pub has_impl: bool,
    /// Joined `///` doc comments preceding this struct.
    pub doc: Option<String>,
}

/// Information about a field
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FieldInfo {
    pub name: String,
    pub ty: Type,
    /// Joined `///` doc comments preceding this field.
    pub doc: Option<String>,
}

/// Information about an inherent impl block (impl Struct)
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ImplInfo {
    pub struct_name: String,
    pub generics: Vec<GenericParam>,
    pub span: Span,
}

/// Information about a trait implementation (impl Trait for Struct)
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TraitImplInfo {
    /// The trait being implemented
    pub trait_name: String,
    /// The struct implementing the trait
    pub struct_name: String,
    /// Generic parameters
    pub generics: Vec<GenericParam>,
    /// Span for error reporting
    pub span: Span,
}

/// Information about an enum with its variants
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct EnumInfo {
    pub visibility: Visibility,
    pub span: Span,
    /// Generic parameters
    pub generics: Vec<GenericParam>,
    /// Variant name -> (arity, span)
    pub variants: HashMap<String, (usize, Span)>,
    /// Variant name -> ordered field definitions.
    ///
    /// Populated alongside `variants` so IR lowering of imported module
    /// enums can emit the full variant shape instead of empty placeholders.
    pub variant_fields: HashMap<String, Vec<FieldInfo>>,
    /// Traits this enum implements (from : Trait syntax)
    pub traits: Vec<String>,
    /// Joined `///` doc comments preceding this enum.
    pub doc: Option<String>,
}

/// Information about a module with its nested symbol table
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ModuleInfo {
    pub visibility: Visibility,
    pub span: Span,
    /// Nested symbol table containing the module's definitions
    pub symbols: SymbolTable,
}

/// Information about a single parameter in a function (stored in symbol table)
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ParamInfo {
    /// Parameter passing convention
    pub convention: crate::ast::ParamConvention,
    /// External call-site label (if specified separately from the internal name)
    pub external_label: Option<crate::ast::Ident>,
    pub name: crate::ast::Ident,
    pub ty: Option<Type>,
}

/// Information about a standalone function
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FunctionInfo {
    pub visibility: Visibility,
    pub span: Span,
    /// Parameter information including external labels
    pub params: Vec<ParamInfo>,
    /// Return type (None for unit/void)
    pub return_type: Option<Type>,
    /// Generic parameters declared on this function
    pub generics: Vec<GenericParam>,
    /// Joined `///` doc comments preceding this function.
    pub doc: Option<String>,
}

/// Kind of symbol (for error reporting)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SymbolKind {
    Trait,
    Struct,
    Impl,
    Enum,
    Let,
    Module,
    Function,
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
    pub fn get_enum_variants(&self, name: &str) -> Option<&HashMap<String, (usize, Span)>> {
        self.enums.get(name).map(|info| &info.variants)
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

    /// Get the inferred type of a let binding
    #[must_use]
    pub fn get_let_type(&self, name: &str) -> Option<&str> {
        self.lets
            .get(name)
            .and_then(|info| info.inferred_type.as_deref())
    }

    /// Find a symbol in any table (functions are excluded — they allow overloads)
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

/// Errors that can occur during symbol import
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Trait => "trait",
            Self::Struct => "struct",
            Self::Impl => "impl",
            Self::Enum => "enum",
            Self::Let => "let binding",
            Self::Function => "fn",
            Self::Module => "mod",
        }
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalised, span-free signature string for overload deduplication.
///
/// Includes the call-site label (external label if present, else the internal
/// name) so that overloads distinguished purely by label are not treated as
/// duplicates, matching the overload resolution rules.
fn param_signature(params: &[ParamInfo]) -> String {
    let mut out = String::new();
    for (i, p) in params.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let label = p
            .external_label
            .as_ref()
            .map_or(p.name.name.as_str(), |l| l.name.as_str());
        out.push_str(label);
        out.push(':');
        match &p.ty {
            Some(t) => out.push_str(&ty_shape(t)),
            None => out.push('_'),
        }
    }
    out
}

fn ty_shape(ty: &Type) -> String {
    match ty {
        Type::Primitive(p) => format!("{p:?}"),
        Type::Ident(i) => i.name.clone(),
        Type::Generic { name, args, .. } => {
            let parts: Vec<String> = args.iter().map(ty_shape).collect();
            format!("{}<{}>", name.name, parts.join(","))
        }
        Type::Array(inner) => format!("[{}]", ty_shape(inner)),
        Type::Optional(inner) => format!("{}?", ty_shape(inner)),
        Type::Tuple(fields) => {
            let parts: Vec<String> = fields
                .iter()
                .map(|f| format!("{}:{}", f.name.name, ty_shape(&f.ty)))
                .collect();
            format!("({})", parts.join(","))
        }
        Type::Dictionary { key, value } => {
            format!("[{}:{}]", ty_shape(key), ty_shape(value))
        }
        Type::Closure { params, ret } => {
            let parts: Vec<String> = params
                .iter()
                .map(|(c, t)| format!("{c:?}_{}", ty_shape(t)))
                .collect();
            format!("({})->{}", parts.join(","), ty_shape(ret))
        }
    }
}
