use super::super::module_resolver::ModuleResolver;
use super::super::sem_type::SemType;
use super::super::SemanticAnalyzer;
use crate::ast::{Definition, File, Statement};
// HashMap unused after split

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Infer the type of a field access given the receiver's type.
    ///
    /// Handles optional receiver types by stripping `?`, looking up the struct,
    /// and re-wrapping the result as `T?`. Returns [`SemType::Unknown`] when
    /// the receiver is not a known struct or the field doesn't exist.
    pub(super) fn infer_field_type(&self, obj_type: &SemType, field_name: &str) -> SemType {
        if obj_type.is_indeterminate() {
            return SemType::Unknown;
        }
        let is_optional = obj_type.is_optional();
        let stripped = obj_type.strip_optional();
        // Strip generic args like Container<T> -> Container for struct lookup.
        let lookup_name: &str = match &stripped {
            SemType::Generic { base, .. } | SemType::Named(base) => base.as_str(),
            SemType::Primitive(_)
            | SemType::Array(_)
            | SemType::Optional(_)
            | SemType::Tuple(_)
            | SemType::Dictionary { .. }
            | SemType::Closure { .. }
            | SemType::Unknown
            | SemType::InferredEnum
            | SemType::Nil => return SemType::Unknown,
        };
        let wrap = |ty: SemType| -> SemType {
            if is_optional {
                SemType::optional_of(ty)
            } else {
                ty
            }
        };
        // Top-level struct lookup.
        if let Some(struct_info) = self.symbols.get_struct(lookup_name) {
            for field in &struct_info.fields {
                if field.name == field_name {
                    return wrap(SemType::from_ast(&field.ty));
                }
            }
        }
        // Trait-used-as-type field lookup. Traits in FormaLang declare
        // required fields; an `item: SomeTrait` parameter must allow
        // `item.field` access.
        if let Some(trait_info) = self.symbols.get_trait(lookup_name) {
            if let Some(field) = trait_info.fields.iter().find(|f| f.name == field_name) {
                return wrap(SemType::from_ast(&field.ty));
            }
        }
        // Module-nested struct: walk the symbol table's modules.
        if let Some(ty) = Self::lookup_field_in_modules(&self.symbols, lookup_name, field_name) {
            return wrap(ty);
        }
        // Imported-module struct: walk the analyser's module cache.
        for (_, symbols) in self.module_cache.values() {
            if let Some(struct_info) = symbols.get_struct(lookup_name) {
                for field in &struct_info.fields {
                    if field.name == field_name {
                        return wrap(SemType::from_ast(&field.ty));
                    }
                }
            }
            if let Some(ty) = Self::lookup_field_in_modules(symbols, lookup_name, field_name) {
                return wrap(ty);
            }
        }
        SemType::Unknown
    }

    /// Walk a `SymbolTable`'s module hierarchy looking for a struct by
    /// (unqualified) name; if found, return the type-string of its
    /// `field_name` field. Used by `infer_field_type_from_string` so a
    /// struct nested inside `pub mod m { struct S { ... } }` resolves
    /// even when the impl method body refers to it as just `S`.
    fn lookup_field_in_modules(
        symbols: &super::super::symbol_table::SymbolTable,
        struct_name: &str,
        field_name: &str,
    ) -> Option<SemType> {
        for module_info in symbols.modules.values() {
            if let Some(struct_info) = module_info.symbols.get_struct(struct_name) {
                for field in &struct_info.fields {
                    if field.name == field_name {
                        return Some(SemType::from_ast(&field.ty));
                    }
                }
            }
            if let Some(ty) =
                Self::lookup_field_in_modules(&module_info.symbols, struct_name, field_name)
            {
                return Some(ty);
            }
        }
        None
    }

    /// Infer the return type of a method call given the receiver's type.
    ///
    /// Searches impl blocks in the current file and module cache for a matching
    /// method. Falls back to trait method signatures for types that implement the
    /// trait. Returns "Unknown" when the method cannot be resolved.
    pub(super) fn infer_method_return_type(
        &self,
        receiver_type: &SemType,
        method_name: &str,
        file: &File,
    ) -> SemType {
        if receiver_type.is_indeterminate() {
            return SemType::Unknown;
        }
        let is_optional = receiver_type.is_optional();
        let stripped = receiver_type.strip_optional();
        // Receiver-side generic args (`Box<I32>` → `["I32"]`)
        // for substituting the impl method's `TypeParam` references
        // with concrete types.
        let (lookup_name, receiver_type_args): (&str, Vec<SemType>) = match &stripped {
            SemType::Generic { base, args } => (base.as_str(), args.clone()),
            SemType::Named(base) => (base.as_str(), Vec::new()),
            SemType::Primitive(_)
            | SemType::Array(_)
            | SemType::Optional(_)
            | SemType::Tuple(_)
            | SemType::Dictionary { .. }
            | SemType::Closure { .. }
            | SemType::Unknown
            | SemType::InferredEnum
            | SemType::Nil => return SemType::Unknown,
        };

        let substitute = |ret: SemType| -> SemType {
            if receiver_type_args.is_empty() {
                return ret;
            }
            let generics = self
                .symbols
                .structs
                .get(lookup_name)
                .map(|s| s.generics.clone())
                .or_else(|| {
                    self.symbols
                        .enums
                        .get(lookup_name)
                        .map(|e| e.generics.clone())
                })
                .unwrap_or_default();
            let mut out = ret;
            for (i, param) in generics.iter().enumerate() {
                if let Some(arg) = receiver_type_args.get(i) {
                    out = out.substitute_named(&param.name.name, arg);
                }
            }
            out
        };
        let wrap_if_optional = |ret: SemType| -> SemType {
            let ret = substitute(ret);
            // Don't double-wrap optional or wrap Nil — preserves prior behaviour.
            if is_optional && !ret.is_optional() && !matches!(ret, SemType::Nil) {
                SemType::optional_of(ret)
            } else {
                ret
            }
        };

        // Current file impl blocks
        if let Some(ret) = Self::find_method_return_in_file(lookup_name, method_name, file) {
            return wrap_if_optional(ret);
        }
        // Module cache impl blocks
        for (cached_file, _) in self.module_cache.values() {
            if let Some(ret) =
                Self::find_method_return_in_file(lookup_name, method_name, cached_file)
            {
                return wrap_if_optional(ret);
            }
        }
        // Trait method signatures
        if let Some(ret) = self.find_trait_method_return(lookup_name, method_name) {
            return wrap_if_optional(ret);
        }
        // Generic type parameter: look up its trait bounds in the active
        // generic-scope stack, then search those traits for the method.
        if let Some(constraints) = self.get_type_parameter_constraints(lookup_name) {
            for trait_name in &constraints {
                if let Some(ret) = self.find_trait_method_return(trait_name, method_name) {
                    return wrap_if_optional(ret);
                }
            }
        }
        SemType::Unknown
    }

    /// Search impl blocks in a file for `method_name` on `type_name`.
    fn find_method_return_in_file(
        type_name: &str,
        method_name: &str,
        file: &File,
    ) -> Option<SemType> {
        for stmt in &file.statements {
            if let Statement::Definition(def) = stmt {
                if let Definition::Impl(impl_def) = &**def {
                    if impl_def.name.name == type_name {
                        for func in &impl_def.functions {
                            if func.name.name == method_name {
                                return Some(
                                    func.return_type
                                        .as_ref()
                                        .map_or(SemType::Nil, SemType::from_ast),
                                );
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Look up a trait method signature.
    ///
    /// Tries two interpretations of `type_name`:
    /// 1. As a trait name itself — used when resolving methods on a
    ///    generic type parameter via its trait constraints.
    /// 2. As a struct name — walks every trait the struct implements
    ///    and searches their methods.
    fn find_trait_method_return(&self, type_name: &str, method_name: &str) -> Option<SemType> {
        if let Some(trait_info) = self.symbols.get_trait(type_name) {
            for method in &trait_info.methods {
                if method.name.name == method_name {
                    return Some(
                        method
                            .return_type
                            .as_ref()
                            .map_or(SemType::Nil, SemType::from_ast),
                    );
                }
            }
        }
        let trait_names = self.symbols.get_all_traits_for_struct(type_name);
        for trait_name in trait_names {
            if let Some(trait_info) = self.symbols.get_trait(&trait_name) {
                for method in &trait_info.methods {
                    if method.name.name == method_name {
                        return Some(
                            method
                                .return_type
                                .as_ref()
                                .map_or(SemType::Nil, SemType::from_ast),
                        );
                    }
                }
            }
        }
        None
    }
}
