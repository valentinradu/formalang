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
        // Collect the (name, source_module_path) pairs first so the
        // borrow on `self.symbols` doesn't overlap the mutable borrow
        // of `self.imported_source_context` below.
        let mut imported_struct_pairs: Vec<(String, Vec<String>)> = self
            .symbols
            .structs
            .keys()
            .filter_map(|name| {
                self.symbols
                    .get_module_logical_path(name)
                    .map(|path| (name.clone(), path.clone()))
            })
            .collect();
        // `SymbolTable` stores its definitions in `HashMap`s, whose
        // iteration order is randomised per process. The loop below
        // hands out `StructId`s in that order, so without this sort
        // two compiles of one source produce different ids and a
        // different `IrModule.structs` order. Every such loop in this
        // file sorts for the same reason.
        imported_struct_pairs.sort_by(|a, b| a.0.cmp(&b.0));
        // With the linker, the imported type is in the module already.
        for (name, source_path) in imported_struct_pairs {
            let info = self.symbols.structs.get(&name).filter(|_| !self.linked);
            if let Some(struct_info) = info.cloned() {
                self.imported_source_context = Some(source_path);
                self.register_struct(&name, &struct_info);
                self.imported_source_context = None;
            }
            self.try_track_imported_type(&name, ImportedKind::Struct);
        }

        let mut imported_enum_pairs: Vec<(String, Vec<String>)> = self
            .symbols
            .enums
            .keys()
            .filter_map(|name| {
                self.symbols
                    .get_module_logical_path(name)
                    .map(|path| (name.clone(), path.clone()))
            })
            .collect();
        imported_enum_pairs.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, source_path) in imported_enum_pairs {
            let info = self.symbols.enums.get(&name).filter(|_| !self.linked);
            if let Some(enum_info) = info.cloned() {
                self.imported_source_context = Some(source_path);
                self.register_enum(&name, &enum_info);
                self.imported_source_context = None;
            }
            self.try_track_imported_type(&name, ImportedKind::Enum);
        }

        // CM-J: Track imported standalone-function and module-let
        // imports in IrImport.items so the cross-module qualification
        // pass can route bare-name references to their qualified
        // forms after inlining.
        let mut function_names: Vec<String> = self
            .symbols
            .functions
            .keys()
            .filter(|name| self.symbols.get_module_origin(name).is_some())
            .cloned()
            .collect();
        function_names.sort();
        for name in function_names {
            self.try_track_imported_type(&name, ImportedKind::Function);
        }
        let mut let_names: Vec<String> = self
            .symbols
            .lets
            .keys()
            .filter(|name| self.symbols.get_module_origin(name).is_some())
            .cloned()
            .collect();
        let_names.sort();
        for name in let_names {
            self.try_track_imported_type(&name, ImportedKind::ModuleLet);
        }

        // Register types from imported nested modules (e.g., fill::Solid)
        let mut module_names: Vec<&String> = self.symbols.modules.keys().collect();
        // With the linker, an imported module is in the module already.
        module_names.retain(|name| !self.linked || self.symbols.get_module_origin(name).is_none());
        module_names.sort();
        let modules: Vec<(String, SymbolTable)> = module_names
            .into_iter()
            .filter_map(|name| {
                self.symbols
                    .modules
                    .get(name)
                    .map(|info| (name.clone(), info.symbols.clone()))
            })
            .collect();
        // Step 1 registers every item of each module tree. A field type
        // can name an item that registers later in the same step, so
        // this step's errors are dropped. Step 2 lowers the field types
        // again, when every name exists, and reports its errors.
        let errors_before = self.errors.len();
        for (module_name, module_symbols) in &modules {
            self.register_module_types(module_name, module_symbols);
        }
        self.errors.truncate(errors_before);
        for (module_name, module_symbols) in &modules {
            self.refresh_module_field_types(module_name, module_symbols);
        }
    }

    /// Lower the field types of each struct, enum variant and trait in
    /// a module tree again, and store them on the registered items.
    ///
    /// The code of a module names the module's types without the
    /// prefix, so the types lower with `current_module_prefix` set to
    /// the module.
    fn refresh_module_field_types(&mut self, module_prefix: &str, module_symbols: &SymbolTable) {
        let saved_prefix =
            std::mem::replace(&mut self.current_module_prefix, module_prefix.to_string());
        for (name, info) in sorted_by_name(&module_symbols.structs) {
            let Some(id) = self.module.struct_id(&format!("{module_prefix}::{name}")) else {
                continue;
            };
            let generics = self.lower_generic_params(&info.generics);
            self.generic_scopes.push(generics);
            let types: Vec<_> = info.fields.iter().map(|f| self.lower_type(&f.ty)).collect();
            self.generic_scopes.pop();
            if let Some(ir) = self.module.struct_mut(id) {
                for (field, ty) in ir.fields.iter_mut().zip(types) {
                    field.ty = ty;
                }
            }
        }
        for (name, info) in sorted_by_name(&module_symbols.enums) {
            let Some(id) = self.module.enum_id(&format!("{module_prefix}::{name}")) else {
                continue;
            };
            let generics = self.lower_generic_params(&info.generics);
            self.generic_scopes.push(generics);
            let variant_names: Vec<String> = self
                .module
                .get_enum(id)
                .map(|e| e.variants.iter().map(|v| v.name.clone()).collect())
                .unwrap_or_default();
            let types: Vec<Vec<_>> = variant_names
                .iter()
                .map(|variant| {
                    info.variant_fields
                        .get(variant)
                        .map_or_else(Vec::new, |fields| {
                            fields.iter().map(|f| self.lower_type(&f.ty)).collect()
                        })
                })
                .collect();
            self.generic_scopes.pop();
            if let Some(ir) = self.module.enum_mut(id) {
                for (variant, variant_types) in ir.variants.iter_mut().zip(types) {
                    for (field, ty) in variant.fields.iter_mut().zip(variant_types) {
                        field.ty = ty;
                    }
                }
            }
        }
        for (name, info) in sorted_by_name(&module_symbols.traits) {
            let Some(id) = self.module.trait_id(&format!("{module_prefix}::{name}")) else {
                continue;
            };
            let generics = self.lower_generic_params(&info.generics);
            self.generic_scopes.push(generics);
            let types: Vec<_> = info.fields.iter().map(|f| self.lower_type(&f.ty)).collect();
            self.generic_scopes.pop();
            if let Some(ir) = self.module.trait_mut(id) {
                for (field, ty) in ir.fields.iter_mut().zip(types) {
                    field.ty = ty;
                }
            }
        }
        for (nested_name, nested) in sorted_by_name(&module_symbols.modules) {
            let nested_prefix = format!("{module_prefix}::{nested_name}");
            self.refresh_module_field_types(&nested_prefix, &nested.symbols);
        }
        self.current_module_prefix = saved_prefix;
    }

    /// Register types from a nested module recursively
    fn register_module_types(&mut self, module_prefix: &str, module_symbols: &SymbolTable) {
        // The module's own names resolve with its prefix.
        let saved_prefix =
            std::mem::replace(&mut self.current_module_prefix, module_prefix.to_string());
        // Register traits from this module with their real shape. Composed
        // traits are filled in after all names exist, since composition can
        // forward-reference traits in the same module.
        let mut pending_trait_composition: Vec<(String, Vec<String>)> = Vec::new();
        for (name, trait_info) in sorted_by_name(&module_symbols.traits) {
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
                    span: self.current_ir_span(),
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
                    span: self.ir_span(trait_info.span),
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
        for (name, struct_info) in sorted_by_name(&module_symbols.structs) {
            let qualified_name = format!("{module_prefix}::{name}");
            self.register_struct(&qualified_name, struct_info);
        }

        // Register enums from this module
        for (name, enum_info) in sorted_by_name(&module_symbols.enums) {
            let qualified_name = format!("{module_prefix}::{name}");
            self.register_enum(&qualified_name, enum_info);
        }

        // Recursively register nested modules
        for (nested_name, nested_module_info) in sorted_by_name(&module_symbols.modules) {
            let nested_prefix = format!("{module_prefix}::{nested_name}");
            self.register_module_types(&nested_prefix, &nested_module_info.symbols);
        }
        self.current_module_prefix = saved_prefix;
    }

    /// Helper method to register an enum using `EnumInfo::variant_fields`
    /// so imported-module enums carry real variant shapes into the IR.
    fn register_enum(&mut self, name: &str, enum_info: &EnumInfo) {
        let generic_params = self.lower_generic_params(&enum_info.generics);
        self.generic_scopes.push(generic_params.clone());

        // `EnumInfo::variants` is a `HashMap`, so iterating it directly
        // ordered the variants by hash. A variant's index is its
        // discriminant, so that made an imported enum's tags differ
        // between two builds of the same program. Each entry carries
        // the span it was declared at, so sorting by that restores the
        // order the user wrote.
        let mut variant_names: Vec<(&String, crate::location::Span)> = enum_info
            .variants
            .iter()
            .map(|(name, (_, span))| (name, *span))
            .collect();
        variant_names.sort_by_key(|(name, span)| (span.start.offset, name.as_str()));

        let variants: Vec<IrEnumVariant> = variant_names
            .into_iter()
            .map(|(variant_name, variant_span)| {
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
                                span: self.current_ir_span(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                IrEnumVariant {
                    name: variant_name.clone(),
                    fields,
                    span: self.ir_span(variant_span),
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
                span: self.ir_span(enum_info.span),
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
                    span: self.current_ir_span(),
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
                span: self.ir_span(struct_info.span),
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
                        span: self.ir_span(t.span),
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
                        span: self.ir_span(s.span),
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
                        span: self.ir_span(e.span),
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

/// The entries of `map`, ordered by key.
///
/// `SymbolTable` stores its definitions in `HashMap`s. Iterating one
/// directly hands out IR ids in an order that is randomised per
/// process, which makes the whole `IrModule` non-reproducible. Every
/// loop that assigns an id goes through here.
fn sorted_by_name<V>(map: &std::collections::HashMap<String, V>) -> Vec<(&String, &V)> {
    let mut entries: Vec<(&String, &V)> = map.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    entries
}
