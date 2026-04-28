//! Definition lowering for the IR lowering pass.
//!
//! Covers the second-pass lowering of struct/enum/trait definitions
//! (with and without an enclosing module prefix), `impl` blocks
//! (including impl-context helpers like `self.field` resolution), and
//! the `lower_definition` dispatcher.

use super::IrLowerer;
use crate::ast::{self, Definition, EnumDef, ImplDef, StructDef, TraitDef};
use crate::error::CompilerError;
use crate::ir::{
    ImportedKind, IrEnumVariant, IrField, IrFunction, IrFunctionSig, IrGenericParam, IrImpl,
    ResolvedType, TraitId,
};
use crate::semantic::SymbolTable;
use std::collections::HashMap;

impl IrLowerer<'_> {
    /// Second pass: lower definitions with full type resolution
    pub(super) fn lower_definition(&mut self, def: &Definition) {
        match def {
            Definition::Trait(t) => self.lower_trait(t),
            Definition::Struct(s) => self.lower_struct(s),
            Definition::Enum(e) => self.lower_enum(e),
            Definition::Impl(i) => self.lower_impl(i),
            Definition::Function(f) => self.lower_function(f.as_ref()),
            Definition::Module(m) => {
                // Lower nested definitions within the module
                self.lower_module(&m.name.name, &m.definitions);
            }
        }
    }

    /// Resolve the type of a self.field reference using current impl context.
    /// Searches the top-level symbol table first, then any current module
    /// prefix (so impl blocks inside `pub mod foo { ... }` resolve correctly).
    pub(super) fn resolve_self_field_type(&mut self, field_name: &str) -> ResolvedType {
        if let Some(struct_name) = self.current_impl_struct.clone() {
            if let Some(ty) = self.find_struct_field_ty(&struct_name, field_name) {
                return ty;
            }
        }
        self.internal_error_type(format!(
            "`self.{field_name}` has no matching field on the current impl target; semantic should have caught this"
        ))
    }

    /// Look up a struct's field type by searching the top-level symbol
    /// table, then walking the current module prefix if any. Returns the
    /// lowered `ResolvedType` if found.
    fn find_struct_field_ty(
        &mut self,
        struct_name: &str,
        field_name: &str,
    ) -> Option<ResolvedType> {
        if let Some(struct_info) = self.symbols.structs.get(struct_name) {
            if let Some(field) = struct_info.fields.iter().find(|f| f.name == field_name) {
                let ty = field.ty.clone();
                return Some(self.lower_type(&ty));
            }
        }
        if !self.current_module_prefix.is_empty() {
            let parts: Vec<&str> = self.current_module_prefix.split("::").collect();
            let mut current = self.symbols;
            for part in &parts {
                match current.modules.get(*part) {
                    Some(info) => current = &info.symbols,
                    None => return None,
                }
            }
            if let Some(struct_info) = current.structs.get(struct_name) {
                if let Some(field) = struct_info.fields.iter().find(|f| f.name == field_name) {
                    let ty = field.ty.clone();
                    return Some(self.lower_type(&ty));
                }
            }
        }
        None
    }

    /// Resolve the type of `self` in an impl block context.
    /// Returns the `ResolvedType` for the struct or enum being implemented.
    pub(super) fn resolve_impl_self_type(&mut self, impl_name: &str) -> ResolvedType {
        // First try as a struct
        if let Some(id) = self.module.struct_id(impl_name) {
            return ResolvedType::Struct(id);
        }
        // Then try as an enum
        if let Some(id) = self.module.enum_id(impl_name) {
            return ResolvedType::Enum(id);
        }
        self.internal_error_type(format!(
            "impl-self type `{impl_name}` was not registered in the module before lowering referenced it",
        ))
    }

    /// Look up all traits for a struct that lives inside a module.
    ///
    /// `module_prefix` is a `"::"` separated path (e.g. `"shapes"` or `"a::b"`).
    /// Returns the trait names as stored in the nested symbol table, which are
    /// the *unqualified* trait names as written in source.
    fn get_traits_for_struct_in_module(
        &self,
        module_prefix: &str,
        struct_name: &str,
    ) -> Vec<String> {
        // Walk the module hierarchy following the prefix segments.
        let parts: Vec<&str> = module_prefix.split("::").collect();
        let mut current: &SymbolTable = self.symbols;
        for part in &parts {
            match current.modules.get(*part) {
                Some(info) => current = &info.symbols,
                None => return Vec::new(),
            }
        }
        current.get_all_traits_for_struct(struct_name)
    }

    /// Lower trait with module prefix
    pub(super) fn lower_trait_with_prefix(&mut self, t: &TraitDef, prefix: &str) {
        let qualified_name = format!("{}::{}", prefix, t.name.name);
        let Some(id) = self
            .module
            .trait_id(&qualified_name)
            .or_else(|| self.module.trait_id(&t.name.name))
        else {
            return; // Trait not registered, skip
        };

        let composed_traits: Vec<TraitId> = t
            .traits
            .iter()
            .filter_map(|ident| self.module.trait_id(&ident.name))
            .collect();

        let generic_params = self.lower_generic_params(&t.generics);
        self.generic_scopes.push(generic_params.clone());
        let fields: Vec<IrField> = t.fields.iter().map(|f| self.lower_field_def(f)).collect();
        let methods: Vec<IrFunctionSig> = t.methods.iter().map(|m| self.lower_fn_sig(m)).collect();
        self.generic_scopes.pop();

        let Some(trait_def) = self.module.trait_mut(id) else {
            self.record_missing_id("trait", id.0);
            return;
        };
        trait_def.name = qualified_name;
        trait_def.visibility = t.visibility;
        trait_def.composed_traits = composed_traits;
        trait_def.generic_params = generic_params;
        trait_def.fields = fields;
        trait_def.methods = methods;

        if let Some(node) = self.module_node_stack.last_mut() {
            node.traits.push(id);
        }
    }

    /// Lower struct with module prefix
    pub(super) fn lower_struct_with_prefix(&mut self, s: &StructDef, prefix: &str) {
        let qualified_name = format!("{}::{}", prefix, s.name.name);
        let Some(id) = self
            .module
            .struct_id(&qualified_name)
            .or_else(|| self.module.struct_id(&s.name.name))
        else {
            return; // Struct not registered, skip
        };

        // Look up the struct's traits from the correct (nested) symbol table.
        let all_trait_names = self.get_traits_for_struct_in_module(prefix, &s.name.name);
        let traits: Vec<crate::ir::IrTraitRef> = all_trait_names
            .iter()
            .filter_map(|trait_name| {
                // The trait name from source is unqualified (e.g. "Drawable").
                // It was registered in the IR as a qualified name (e.g. "shapes::Drawable").
                // Try the qualified form first, fall back to unqualified.
                let qualified = format!("{prefix}::{trait_name}");
                self.module
                    .trait_id(&qualified)
                    .or_else(|| self.module.trait_id(trait_name))
                    .map(crate::ir::IrTraitRef::simple)
            })
            .collect();

        let generic_params = self.lower_generic_params(&s.generics);
        self.generic_scopes.push(generic_params.clone());
        let fields: Vec<IrField> = s
            .fields
            .iter()
            .map(|f| self.lower_struct_field(f))
            .collect();
        self.generic_scopes.pop();

        let Some(struct_def) = self.module.struct_mut(id) else {
            self.record_missing_id("struct", id.0);
            return;
        };
        struct_def.name = qualified_name;
        struct_def.visibility = s.visibility;
        struct_def.traits = traits;
        struct_def.generic_params = generic_params;
        struct_def.fields = fields;

        if let Some(node) = self.module_node_stack.last_mut() {
            node.structs.push(id);
        }
    }

    /// Lower enum with module prefix
    pub(super) fn lower_enum_with_prefix(&mut self, e: &EnumDef, prefix: &str) {
        let qualified_name = format!("{}::{}", prefix, e.name.name);
        let Some(id) = self
            .module
            .enum_id(&qualified_name)
            .or_else(|| self.module.enum_id(&e.name.name))
        else {
            return; // Enum not registered, skip
        };

        let generic_params = self.lower_generic_params(&e.generics);
        self.generic_scopes.push(generic_params.clone());
        let variants: Vec<IrEnumVariant> = e
            .variants
            .iter()
            .map(|v| IrEnumVariant {
                name: v.name.name.clone(),
                fields: v
                    .fields
                    .iter()
                    .map(|f| IrField {
                        name: f.name.name.clone(),
                        ty: self.lower_type(&f.ty),
                        default: None,
                        optional: false,
                        mutable: false,
                        doc: f.doc.clone(),
                        convention: ast::ParamConvention::default(),
                    })
                    .collect(),
            })
            .collect();
        self.generic_scopes.pop();

        let Some(enum_def) = self.module.enum_mut(id) else {
            self.record_missing_id("enum", id.0);
            return;
        };
        enum_def.name = qualified_name;
        enum_def.visibility = e.visibility;
        enum_def.generic_params = generic_params;
        enum_def.variants = variants;

        if let Some(node) = self.module_node_stack.last_mut() {
            node.enums.push(id);
        }
    }

    fn lower_trait(&mut self, t: &TraitDef) {
        let Some(id) = self.module.trait_id(&t.name.name) else {
            self.errors.push(CompilerError::UndefinedType {
                name: t.name.name.clone(),
                span: t.span,
            });
            return;
        };

        let composed_traits: Vec<TraitId> = t
            .traits
            .iter()
            .filter_map(|ident| self.module.trait_id(&ident.name))
            .collect();

        let generic_params = self.lower_generic_params(&t.generics);
        self.generic_scopes.push(generic_params.clone());

        let fields: Vec<IrField> = t.fields.iter().map(|f| self.lower_field_def(f)).collect();

        let methods: Vec<IrFunctionSig> = t.methods.iter().map(|m| self.lower_fn_sig(m)).collect();

        self.generic_scopes.pop();

        let Some(trait_def) = self.module.trait_mut(id) else {
            self.record_missing_id("trait", id.0);
            return;
        };
        trait_def.composed_traits = composed_traits;
        trait_def.fields = fields;
        trait_def.methods = methods;
        trait_def.generic_params = generic_params;
    }

    fn lower_struct(&mut self, s: &StructDef) {
        let Some(id) = self.module.struct_id(&s.name.name) else {
            self.errors.push(CompilerError::UndefinedType {
                name: s.name.name.clone(),
                span: s.span,
            });
            return;
        };

        // Get all traits from both inline definition and impl blocks
        let all_trait_names = self.symbols.get_all_traits_for_struct(&s.name.name);
        let traits: Vec<crate::ir::IrTraitRef> = all_trait_names
            .iter()
            .filter_map(|trait_name| {
                // Check if this is an external trait and track the import
                self.try_track_imported_type(trait_name, ImportedKind::Trait);
                self.module
                    .trait_id(trait_name)
                    .map(crate::ir::IrTraitRef::simple)
            })
            .collect();

        let generic_params = self.lower_generic_params(&s.generics);
        self.generic_scopes.push(generic_params.clone());

        let fields: Vec<IrField> = s
            .fields
            .iter()
            .map(|f| self.lower_struct_field(f))
            .collect();

        self.generic_scopes.pop();

        let Some(struct_def) = self.module.struct_mut(id) else {
            self.record_missing_id("struct", id.0);
            return;
        };
        struct_def.traits = traits;
        struct_def.fields = fields;
        struct_def.generic_params = generic_params;
    }

    fn lower_enum(&mut self, e: &EnumDef) {
        let Some(id) = self.module.enum_id(&e.name.name) else {
            self.errors.push(CompilerError::UndefinedType {
                name: e.name.name.clone(),
                span: e.span,
            });
            return;
        };

        let generic_params = self.lower_generic_params(&e.generics);
        self.generic_scopes.push(generic_params.clone());

        let variants: Vec<IrEnumVariant> = e
            .variants
            .iter()
            .map(|v| IrEnumVariant {
                name: v.name.name.clone(),
                fields: v.fields.iter().map(|f| self.lower_field_def(f)).collect(),
            })
            .collect();

        self.generic_scopes.pop();

        let Some(enum_def) = self.module.enum_mut(id) else {
            self.record_missing_id("enum", id.0);
            return;
        };
        enum_def.variants = variants;
        enum_def.generic_params = generic_params;
    }

    pub(super) fn lower_impl(&mut self, i: &ImplDef) {
        use crate::ir::ImplTarget;

        // Build qualified name if we're inside a module
        let qualified_name = if self.current_module_prefix.is_empty() {
            i.name.name.clone()
        } else {
            format!("{}::{}", self.current_module_prefix, i.name.name)
        };

        // Try to find struct first (qualified then unqualified), then enum
        let target = if let Some(id) = self.module.struct_id(&qualified_name) {
            ImplTarget::Struct(id)
        } else if let Some(id) = self.module.struct_id(&i.name.name) {
            ImplTarget::Struct(id)
        } else if let Some(id) = self.module.enum_id(&qualified_name) {
            ImplTarget::Enum(id)
        } else if let Some(id) = self.module.enum_id(&i.name.name) {
            ImplTarget::Enum(id)
        } else {
            return; // Error would have been caught in semantic analysis
        };

        // Set current impl struct/enum for self reference resolution
        self.current_impl_struct = Some(i.name.name.clone());

        let generic_params = self.lower_generic_params(&i.generics);
        // The impl's methods reference type parameters whose trait
        // constraints are declared on the *target* struct/enum
        // (e.g. `struct Box<T: Foo>` plus `impl Box<T> { ... }`). The impl
        // header itself carries the param names without constraints, so we
        // merge constraints from the target definition under each matching
        // name so `find_trait_for_method` resolves correctly.
        let mut scope = generic_params.clone();
        let target_params: Vec<IrGenericParam> = match target {
            ImplTarget::Struct(id) => self
                .module
                .get_struct(id)
                .map(|s| s.generic_params.clone())
                .unwrap_or_default(),
            ImplTarget::Enum(id) => self
                .module
                .get_enum(id)
                .map(|e| e.generic_params.clone())
                .unwrap_or_default(),
        };
        for target_param in target_params {
            if let Some(existing) = scope.iter_mut().find(|q| q.name == target_param.name) {
                for c in target_param.constraints {
                    if !existing.constraints.contains(&c) {
                        existing.constraints.push(c);
                    }
                }
            } else {
                scope.push(target_param);
            }
        }
        self.generic_scopes.push(scope);

        // Pre-compute method return types so lowering a body can resolve
        // forward references like `self.other_method()` without needing
        // the impl to already be in `module.impls`. Must run *after* the
        // generic-scope push so a method returning `T` resolves the type
        // param against the impl/target scope instead of failing
        // `UndefinedType` lookup.
        let saved_impl_returns = self.current_impl_method_returns.take();
        let mut impl_returns: HashMap<String, Option<ResolvedType>> = HashMap::new();
        for f in &i.functions {
            let ret = f.return_type.as_ref().map(|t| self.lower_type(t));
            impl_returns.insert(f.name.name.clone(), ret);
        }
        self.current_impl_method_returns = Some(impl_returns);
        // Tier-1 item E: extern impl methods inherit the C ABI by
        // default. Until the parser accepts `extern "system" impl ...`,
        // there's only one possible value to propagate.
        let enclosing_extern: Option<ast::ExternAbi> = i.is_extern.then_some(ast::ExternAbi::C);
        let functions: Vec<IrFunction> = i
            .functions
            .iter()
            .map(|f| self.lower_fn_def(f, enclosing_extern))
            .collect();
        self.generic_scopes.pop();
        // Phase C: lower the trait reference together with any
        // generic-trait args (`impl Foo<X> for Y`).
        let trait_ref = i.trait_name.as_ref().and_then(|tname| {
            self.module.trait_id(&tname.name).map(|trait_id| {
                let args = i.trait_args.iter().map(|t| self.lower_type(t)).collect();
                crate::ir::IrTraitRef { trait_id, args }
            })
        });

        // Clear the context
        self.current_impl_struct = None;
        self.current_impl_method_returns = saved_impl_returns;

        if let Err(err) = self.module.add_impl(IrImpl {
            target,
            trait_ref,
            is_extern: i.is_extern,
            generic_params,
            functions,
        }) {
            self.errors.push(err);
        }
    }
}
