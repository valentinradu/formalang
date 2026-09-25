use super::super::module_resolver::ModuleResolver;
use super::super::sem_type::SemType;
use super::super::SemanticAnalyzer;
use super::generic_args::substitute_all;
use crate::ast::File;
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
        // Tuple receivers are matched by named field, not by struct
        // lookup. `(x: I32, y: I32).x` returns the I32 directly without
        // routing through `get_struct`.
        if let SemType::Tuple(fields) = &stripped {
            for (n, t) in fields {
                if n == field_name {
                    return if is_optional {
                        SemType::optional_of(t.clone())
                    } else {
                        t.clone()
                    };
                }
            }
            return SemType::Unknown;
        }
        // Strip generic args like Container<T> -> Container for struct lookup,
        // but keep the args so the field's declared type (which may reference
        // the struct's type parameters) gets substituted before being returned.
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
        let substitute = |ty: SemType| -> SemType {
            if receiver_type_args.is_empty() {
                return ty;
            }
            let generics = self.symbols.get_generics(lookup_name).unwrap_or_default();
            let mut out = ty;
            for (i, param) in generics.iter().enumerate() {
                if let Some(arg) = receiver_type_args.get(i) {
                    out = out.substitute_named(&param.name.name, arg);
                }
            }
            out
        };
        let wrap = |ty: SemType| -> SemType {
            let ty = substitute(ty);
            if is_optional {
                SemType::optional_of(ty)
            } else {
                ty
            }
        };
        // Top-level struct lookup, qualified-name aware so
        // `geometry::Point` field accesses resolve through the inline
        // module's symbol table.
        if let Some(struct_info) = self.symbols.get_struct_qualified(lookup_name) {
            for field in &struct_info.fields {
                if field.name == field_name {
                    let ty = self.qualify_for_owner(lookup_name, SemType::from_ast(&field.ty));
                    return wrap(ty);
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
        // Generic-parameter receiver: look at every trait the parameter
        // is bounded by and search its declared fields. So
        // `fn name_of<T: Tagged>(item: T) -> String { item.name }`
        // resolves `item.name` against `Tagged`'s `name: String`.
        if let Some(constraints) = self.get_type_parameter_constraints(lookup_name) {
            for trait_name in &constraints {
                if let Some(trait_info) = self.symbols.get_trait(trait_name) {
                    if let Some(field) = trait_info.fields.iter().find(|f| f.name == field_name) {
                        return wrap(SemType::from_ast(&field.ty));
                    }
                }
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
    #[expect(
        clippy::too_many_lines,
        reason = "single-pass receiver dispatch over the four built-in carriers + structural shapes; splitting would scatter the dispatch logic"
    )]
    pub(super) fn infer_method_return_type(
        &self,
        receiver_type: &SemType,
        method_name: &str,
        args: &[(Option<crate::ast::Ident>, crate::ast::Expr)],
        file: &File,
    ) -> SemType {
        if receiver_type.is_indeterminate() {
            return SemType::Unknown;
        }
        // A closure in a field of a tuple: `t.f(1)` answers what the
        // closure returns.
        if let Some(SemType::Closure { return_ty, .. }) =
            Self::tuple_closure_field(receiver_type, method_name)
        {
            return *return_ty;
        }
        // The four built-in compound shapes (`Array<T>`, `Optional<T>`,
        // `Dictionary<K, V>`, `Range<T>`) route to the prelude-defined
        // generic structs/enum so methods declared in `extern impl` for
        // those names dispatch uniformly with user types. Optional is
        // matched on the un-stripped value so `opt.is_some()` lands on
        // `Optional`, not on its inner `T`. See `src/prelude.fv`.
        let primitive_name_holder: String;
        let dispatch_match: Option<(&str, Vec<SemType>)> = match receiver_type {
            SemType::Optional(inner) => Some(("Optional", vec![(**inner).clone()])),
            SemType::Primitive(_)
            | SemType::Named(_)
            | SemType::Array(_)
            | SemType::Tuple(_)
            | SemType::Generic { .. }
            | SemType::Dictionary { .. }
            | SemType::Closure { .. }
            | SemType::Unknown
            | SemType::InferredEnum
            | SemType::Nil => None,
        };
        let (is_optional, stripped) = if dispatch_match.is_some() {
            // Optional is itself the dispatch receiver; don't post-wrap.
            (false, receiver_type.clone())
        } else {
            (receiver_type.is_optional(), receiver_type.strip_optional())
        };
        let (lookup_name, receiver_type_args): (&str, Vec<SemType>) =
            if let Some(d) = dispatch_match {
                d
            } else {
                match &stripped {
                    SemType::Generic { base, args } => (base.as_str(), args.clone()),
                    SemType::Named(base) => (base.as_str(), Vec::new()),
                    SemType::Primitive(p) => {
                        primitive_name_holder = format!("{p:?}");
                        (primitive_name_holder.as_str(), Vec::new())
                    }
                    SemType::Array(inner) => ("Array", vec![(**inner).clone()]),
                    SemType::Dictionary { key, value } => {
                        ("Dictionary", vec![(**key).clone(), (**value).clone()])
                    }
                    SemType::Optional(_)
                    | SemType::Tuple(_)
                    | SemType::Closure { .. }
                    | SemType::Unknown
                    | SemType::InferredEnum
                    | SemType::Nil => return SemType::Unknown,
                }
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
        let wrap = |ret: SemType| -> SemType {
            // Don't double-wrap optional or wrap Nil — preserves prior behaviour.
            if is_optional && !ret.is_optional() && !matches!(ret, SemType::Nil) {
                SemType::optional_of(ret)
            } else {
                ret
            }
        };
        let wrap_if_optional = |ret: SemType| -> SemType { wrap(substitute(ret)) };

        // Impl blocks: the current file, then the cached modules. A
        // type may declare several methods of one name, and the call's
        // labels and count say which one it means.
        let overloads = std::iter::once(file)
            .chain(
                self.module_cache
                    .values()
                    .map(|(cached_file, _)| cached_file),
            )
            .map(|f| Self::find_method_overloads(lookup_name, method_name, f))
            .find(|found| !found.is_empty());
        if let Some((fn_def, impl_generics)) = overloads
            .as_deref()
            .and_then(|found| self.choose_method_overload(found, args, file))
        {
            // The receiver's type arguments and the method's own type
            // parameters: `K` and `V` in
            // `fn collect<K, V>(sink self, key: (T) -> K, value: (T) -> V) -> [K: V]`
            // take the types that the call's arguments give.
            let (view, fresh) = self.method_view(
                lookup_name,
                impl_generics,
                &receiver_type_args,
                &fn_def.generics,
            );
            let views: Vec<_> = fn_def
                .params
                .iter()
                .map(crate::semantic::validation::invocation::overloads::ParamView::of_fn_param)
                .collect();
            let bindings = self.bind_method_generics(&view, &fresh, &views, args, file);
            let declared = fn_def
                .return_type
                .as_ref()
                .map_or(SemType::Nil, SemType::from_ast);
            let ret = substitute_all(&substitute_all(&declared, &view), &bindings);
            return wrap(ret);
        }
        // Trait method signatures
        if let Some(ret) = self.find_trait_method_return(lookup_name, method_name) {
            return wrap_if_optional(ret);
        }
        // Generic type parameter: the method of a bound, with the
        // trait's own type parameters read as the bound gives them.
        if let Some(bound) = self.bound_method(lookup_name, method_name) {
            let ret = bound
                .sig
                .return_type
                .as_ref()
                .map_or(SemType::Nil, |ty| bound.resolve(ty));
            return wrap_if_optional(ret);
        }
        if let Some(constraints) = self.get_type_parameter_constraints(lookup_name) {
            for trait_name in &constraints {
                if let Some(ret) = self.find_trait_method_return(trait_name, method_name) {
                    return wrap_if_optional(ret);
                }
            }
        }
        // Closure-typed struct field: `f.onPress()` where the struct
        // declares `onPress: () -> E`. The call's return type is the
        // closure's return type.
        if let Some(crate::ast::Type::Closure { ret, .. }) =
            self.find_struct_field_type(lookup_name, method_name)
        {
            return wrap_if_optional(SemType::from_ast(&ret));
        }
        SemType::Unknown
    }

    /// Find a field on the named struct, returning its declared AST
    /// type. Used by closure-field invocation inference.
    fn find_struct_field_type(
        &self,
        struct_name: &str,
        field_name: &str,
    ) -> Option<crate::ast::Type> {
        if let Some(info) = self.symbols.get_struct(struct_name) {
            for f in &info.fields {
                if f.name == field_name {
                    return Some(f.ty.clone());
                }
            }
        }
        for (_, symbols) in self.module_cache.values() {
            if let Some(info) = symbols.get_struct(struct_name) {
                for f in &info.fields {
                    if f.name == field_name {
                        return Some(f.ty.clone());
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
    ///    generic type parameter via its trait constraints. Walks the
    ///    trait's own methods plus every composed (super-)trait.
    /// 2. As a struct name — walks every trait the struct implements
    ///    (which, via `get_all_traits_for_struct`, already includes
    ///    inherited supertraits).
    fn find_trait_method_return(&self, type_name: &str, method_name: &str) -> Option<SemType> {
        if self.symbols.get_trait(type_name).is_some() {
            if let Some(ret) = self.find_in_trait_chain(
                type_name,
                method_name,
                &mut std::collections::HashSet::new(),
            ) {
                return Some(ret);
            }
        }
        let trait_names = self.symbols.get_all_traits_for_struct(type_name);
        for trait_name in trait_names {
            if let Some(ret) = self.find_in_trait_chain(
                &trait_name,
                method_name,
                &mut std::collections::HashSet::new(),
            ) {
                return Some(ret);
            }
        }
        None
    }

    fn find_in_trait_chain(
        &self,
        trait_name: &str,
        method_name: &str,
        visited: &mut std::collections::HashSet<String>,
    ) -> Option<SemType> {
        if !visited.insert(trait_name.to_string()) {
            return None;
        }
        let trait_info = self.symbols.get_trait(trait_name)?;
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
        for parent in &trait_info.composed_traits {
            if let Some(ret) = self.find_in_trait_chain(parent, method_name, visited) {
                return Some(ret);
            }
        }
        None
    }
}
