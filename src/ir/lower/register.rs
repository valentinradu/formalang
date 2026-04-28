//! Definition registration for the IR lowering pass.
//!
//! These methods run before full type-resolved lowering: they walk the
//! AST and the symbol table to allocate IR ids for structs, enums, and
//! traits (including those imported from nested modules) so that later
//! lowering passes can reference them by id without forward-declaration
//! issues.

use super::IrLowerer;
use crate::ast::{self, Definition};
use crate::semantic::{EnumInfo, StructInfo, SymbolTable};

use crate::ir::{
    ImportedKind, IrEnum, IrEnumVariant, IrField, IrFunctionSig, IrStruct, IrTrait, TraitId,
};

impl IrLowerer<'_> {
    /// Register imported structs and enums from the symbol table.
    /// This ensures that imported types have struct/enum IDs in the IR module,
    /// so when we instantiate them, `struct_id` is populated correctly.
    pub(super) fn register_imported_types(&mut self) {
        // Register imported structs (top-level)
        for (name, struct_info) in &self.symbols.structs {
            // Check if this is an imported symbol
            if self.symbols.get_module_origin(name).is_some() {
                self.register_struct(name, struct_info);
                // Track this import for backend use (to find impl blocks)
                self.try_track_imported_type(name, ImportedKind::Struct);
            }
        }

        // Register imported enums (top-level)
        for (name, enum_info) in &self.symbols.enums {
            // Check if this is an imported symbol
            if self.symbols.get_module_origin(name).is_some() {
                self.register_enum(name, enum_info);
                // Track this import for backend use (to find impl blocks)
                self.try_track_imported_type(name, ImportedKind::Enum);
            }
        }

        // Register types from imported nested modules (e.g., fill::Solid)
        for (module_name, module_info) in &self.symbols.modules {
            self.register_module_types(module_name, &module_info.symbols);
        }
    }

    /// Register types from a nested module recursively
    fn register_module_types(&mut self, module_prefix: &str, module_symbols: &SymbolTable) {
        // Register traits from this module with their real shape. Composed
        // traits are filled in after all names exist, since composition can
        // forward-reference traits in the same module.
        let mut pending_trait_composition: Vec<(String, Vec<String>)> = Vec::new();
        for (name, trait_info) in &module_symbols.traits {
            let qualified_name = format!("{module_prefix}::{name}");
            let generic_params = self.lower_generic_params(&trait_info.generics);
            self.generic_scopes.push(generic_params.clone());
            let fields: Vec<IrField> = trait_info
                .fields
                .iter()
                .map(|f| IrField {
                    name: f.name.clone(),
                    ty: self.lower_type(&f.ty),
                    default: None,
                    optional: matches!(f.ty, ast::Type::Optional(_)),
                    mutable: false,
                    doc: f.doc.clone(),
                    convention: ast::ParamConvention::default(),
                })
                .collect();
            let methods: Vec<IrFunctionSig> = trait_info
                .methods
                .iter()
                .map(|m| self.lower_fn_sig(m))
                .collect();
            self.generic_scopes.pop();
            if let Err(e) = self.module.add_trait(
                qualified_name.clone(),
                IrTrait {
                    name: qualified_name.clone(),
                    visibility: trait_info.visibility,
                    composed_traits: Vec::new(),
                    fields,
                    methods,
                    generic_params,
                    doc: None,
                },
            ) {
                self.errors.push(e);
            }
            if !trait_info.composed_traits.is_empty() {
                pending_trait_composition
                    .push((qualified_name, trait_info.composed_traits.clone()));
            }
        }

        // Resolve composed-trait references after all traits from this module
        // have been registered.
        for (qualified_name, composed_names) in pending_trait_composition {
            let composed: Vec<TraitId> = composed_names
                .iter()
                .filter_map(|c| {
                    // Prefer the module-qualified lookup, fall back to simple
                    // name for traits composed from the enclosing scope.
                    self.module
                        .trait_id(&format!("{module_prefix}::{c}"))
                        .or_else(|| self.module.trait_id(c))
                })
                .collect();
            if let Some(id) = self.module.trait_id(&qualified_name) {
                if let Some(trait_def) = self.module.trait_mut(id) {
                    trait_def.composed_traits = composed;
                }
            }
        }

        // Register structs from this module
        for (name, struct_info) in &module_symbols.structs {
            let qualified_name = format!("{module_prefix}::{name}");
            self.register_struct(&qualified_name, struct_info);
        }

        // Register enums from this module
        for (name, enum_info) in &module_symbols.enums {
            let qualified_name = format!("{module_prefix}::{name}");
            self.register_enum(&qualified_name, enum_info);
        }

        // Recursively register nested modules
        for (nested_name, nested_module_info) in &module_symbols.modules {
            let nested_prefix = format!("{module_prefix}::{nested_name}");
            self.register_module_types(&nested_prefix, &nested_module_info.symbols);
        }
    }

    /// Helper method to register an enum using `EnumInfo::variant_fields`
    /// so imported-module enums carry real variant shapes into the IR.
    fn register_enum(&mut self, name: &str, enum_info: &EnumInfo) {
        let generic_params = self.lower_generic_params(&enum_info.generics);
        self.generic_scopes.push(generic_params.clone());

        let variants: Vec<IrEnumVariant> = enum_info
            .variants
            .keys()
            .map(|variant_name| {
                let fields = enum_info
                    .variant_fields
                    .get(variant_name)
                    .map(|fs| {
                        fs.iter()
                            .map(|f| IrField {
                                name: f.name.clone(),
                                ty: self.lower_type(&f.ty),
                                default: None,
                                optional: matches!(f.ty, ast::Type::Optional(_)),
                                mutable: false,
                                doc: f.doc.clone(),
                                convention: ast::ParamConvention::default(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                IrEnumVariant {
                    name: variant_name.clone(),
                    fields,
                }
            })
            .collect();

        self.generic_scopes.pop();

        if let Err(e) = self.module.add_enum(
            name.to_string(),
            IrEnum {
                name: name.to_string(),
                visibility: enum_info.visibility,
                variants,
                generic_params,
                doc: None,
            },
        ) {
            self.errors.push(e);
        }
    }

    /// Helper method to register a struct with full field information
    fn register_struct(&mut self, name: &str, struct_info: &StructInfo) {
        // Convert generic params first so field types referencing `T`
        // resolve as in-scope params instead of triggering an
        // `UndefinedType` from the tightened `lower_type` fallback.
        let generic_params = self.lower_generic_params(&struct_info.generics);
        self.generic_scopes.push(generic_params.clone());

        let fields: Vec<IrField> = struct_info
            .fields
            .iter()
            .map(|f| {
                let optional = matches!(f.ty, ast::Type::Optional(_));
                IrField {
                    name: f.name.clone(),
                    ty: self.lower_type(&f.ty),
                    mutable: false,
                    optional,
                    default: None,
                    doc: f.doc.clone(),
                    convention: ast::ParamConvention::default(),
                }
            })
            .collect();

        self.generic_scopes.pop();

        // Convert trait names to IrTraitRef. The symbol table's
        // get_all_traits_for_struct only carries trait names today,
        // so we always lower these as non-generic refs (empty args);
        // the impl-block path (lower_impl) is what produces
        // populated args via ImplDef.trait_args.
        let all_trait_names = self.symbols.get_all_traits_for_struct(name);
        let traits: Vec<crate::ir::IrTraitRef> = all_trait_names
            .iter()
            .filter_map(|trait_name| {
                self.module
                    .trait_id(trait_name)
                    .map(crate::ir::IrTraitRef::simple)
            })
            .collect();

        if let Err(e) = self.module.add_struct(
            name.to_string(),
            IrStruct {
                name: name.to_string(),
                visibility: struct_info.visibility,
                traits,
                fields,
                generic_params,
                doc: None,
            },
        ) {
            self.errors.push(e);
        }
    }

    /// First pass: register definitions to allocate IDs
    pub(super) fn register_definition(&mut self, def: &Definition) {
        match def {
            Definition::Trait(t) => {
                let name = t.name.name.clone();
                // Create placeholder, will be filled in second pass
                if let Err(e) = self.module.add_trait(
                    name,
                    IrTrait {
                        name: t.name.name.clone(),
                        visibility: t.visibility,
                        composed_traits: Vec::new(),
                        fields: Vec::new(),
                        methods: Vec::new(),
                        generic_params: Vec::new(),
                        doc: t.doc.clone(),
                    },
                ) {
                    self.errors.push(e);
                }
            }
            Definition::Struct(s) => {
                let name = s.name.name.clone();
                if let Err(e) = self.module.add_struct(
                    name,
                    IrStruct {
                        name: s.name.name.clone(),
                        visibility: s.visibility,
                        traits: Vec::new(),
                        fields: Vec::new(),
                        generic_params: Vec::new(),
                        doc: s.doc.clone(),
                    },
                ) {
                    self.errors.push(e);
                }
            }
            Definition::Enum(e) => {
                let name = e.name.name.clone();
                if let Err(e) = self.module.add_enum(
                    name,
                    IrEnum {
                        name: e.name.name.clone(),
                        visibility: e.visibility,
                        variants: Vec::new(),
                        generic_params: Vec::new(),
                        doc: e.doc.clone(),
                    },
                ) {
                    self.errors.push(e);
                }
            }
            Definition::Impl(_) | Definition::Module(_) | Definition::Function(_) => {
                // Impls are processed after structs.
                // Modules: nested definitions are registered by register_module_types
                //   (called from register_imported_types before the first pass).
                // Functions are processed in the second pass.
            }
        }
    }
}
