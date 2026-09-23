//! Building the bindings a `match` arm brings into scope.
//!
//! Split out of `calls.rs` to keep each file under the line ceiling
//! that `scripts/check_file_sizes.sh` enforces.

use super::super::module_resolver::ModuleResolver;
use super::super::sem_type::SemType;
use super::super::SemanticAnalyzer;
use std::collections::HashMap;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Type-driven entry point for match-arm scope construction. Routes
    /// `Optional<T>` scrutinees to the synthetic `.some(T)` / `.none`
    /// arms used by the Rust-style `if let` desugaring; everything else
    /// falls back to `build_match_arm_scope` keyed on the bare enum
    /// name plus the receiver's generic args.
    pub(in crate::semantic) fn build_match_arm_scope_for_type(
        &self,
        scrutinee_ty: &SemType,
        pattern: &crate::ast::Pattern,
    ) -> HashMap<String, SemType> {
        use crate::ast::Pattern;
        if let SemType::Optional(inner) = scrutinee_ty {
            let mut frame = HashMap::new();
            if let Pattern::Variant { name, bindings } = pattern {
                if name.name == "some" {
                    if let Some(b) = bindings.first() {
                        frame.insert(b.name.clone(), (**inner).clone());
                    }
                }
            }
            return frame;
        }
        let stripped = scrutinee_ty.strip_optional();
        let (enum_name, receiver_args): (String, Vec<SemType>) = match &stripped {
            SemType::Generic { base, args } => (base.clone(), args.clone()),
            SemType::Named(n) => (n.clone(), Vec::new()),
            SemType::Primitive(_)
            | SemType::Array(_)
            | SemType::Optional(_)
            | SemType::Tuple(_)
            | SemType::Dictionary { .. }
            | SemType::Closure { .. }
            | SemType::Unknown
            | SemType::InferredEnum
            | SemType::Nil => (stripped.display(), Vec::new()),
        };
        self.build_match_arm_scope(&enum_name, pattern, &receiver_args)
    }

    /// Build a per-arm inference scope from a match pattern's bindings.
    /// `enum_name` is the (optionally optional-stripped) name of the
    /// scrutinee's type. For a `Variant { name, bindings }` pattern with
    /// `n` bindings, looks up the variant's field types on the named
    /// enum and zips them with the binding identifiers. Variants on
    /// imported enums fall back through the module cache. Returns an
    /// empty map for `Wildcard` and for variants that can't be resolved
    /// (the body then falls back to existing inference behaviour).
    ///
    /// `receiver_args` carries the scrutinee's generic instantiation
    /// (e.g. `[I32, I32]` for `Result<I32, I32>`); when non-empty, the
    /// variant payload types have their `T`/`E` references substituted
    /// against the matching enum's generic parameter names so binding
    /// references inside the arm body resolve to concrete types.
    pub(super) fn build_match_arm_scope(
        &self,
        enum_name: &str,
        pattern: &crate::ast::Pattern,
        receiver_args: &[SemType],
    ) -> HashMap<String, SemType> {
        use crate::ast::Pattern;
        let mut frame = HashMap::new();
        let Pattern::Variant { name, bindings } = pattern else {
            return frame;
        };
        let variant_field_tys = self
            .lookup_enum_variant_field_types(enum_name, &name.name)
            .unwrap_or_default();
        let generic_param_names: Vec<String> = if receiver_args.is_empty() {
            Vec::new()
        } else {
            self.symbols
                .get_generics(enum_name)
                .map(|g| g.iter().map(|p| p.name.name.clone()).collect())
                .unwrap_or_default()
        };
        for (i, ident) in bindings.iter().enumerate() {
            if let Some(ty) = variant_field_tys.get(i) {
                let mut sem = ty.clone();
                if !generic_param_names.is_empty() {
                    for (param, arg) in generic_param_names.iter().zip(receiver_args.iter()) {
                        sem = sem.substitute_named(param, arg);
                    }
                }
                frame.insert(ident.name.clone(), sem);
            }
        }
        frame
    }

    /// Look up an enum variant's field types as `SemType`, in the
    /// current symbol table first, then through any imported module
    /// cache. Returns `None` if the enum or variant isn't found.
    fn lookup_enum_variant_field_types(
        &self,
        enum_name: &str,
        variant_name: &str,
    ) -> Option<Vec<SemType>> {
        if let Some(info) = self.symbols.get_enum_qualified(enum_name) {
            if let Some(fields) = info.variant_fields.get(variant_name) {
                return Some(
                    fields
                        .iter()
                        .map(|f| self.qualify_for_owner(enum_name, SemType::from_ast(&f.ty)))
                        .collect(),
                );
            }
        }
        for (_, symbols) in self.module_cache.values() {
            if let Some(info) = symbols.enums.get(enum_name) {
                if let Some(fields) = info.variant_fields.get(variant_name) {
                    return Some(fields.iter().map(|f| SemType::from_ast(&f.ty)).collect());
                }
            }
        }
        None
    }
}
