use super::super::IrLowerer;
use crate::ast::PrimitiveType;
use crate::ir::ResolvedType;

/// A "simple" type name is a bare identifier (struct / enum / trait /
/// generic param). Composite stringifications produced by semantic's
/// `type_to_string` (`[T]`, `T?`, `(T) -> U`, `(a: T)`, `[K: V]`) contain
/// punctuation that disqualifies them.
fn is_simple_type_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':')
}

/// Replace each `TypeParam(name)` reference inside `ty` with the
/// matching entry from `subs`. Used during match-pattern lowering on a
/// generic-instantiated enum so binding payload types carry concrete
/// substitutions (e.g. `T -> I32`) before reaching the IR.
fn substitute_typeparams(
    ty: &mut ResolvedType,
    subs: &std::collections::HashMap<String, ResolvedType>,
) {
    match ty {
        ResolvedType::TypeParam(name) => {
            if let Some(concrete) = subs.get(name) {
                *ty = concrete.clone();
            }
        }
        ResolvedType::Tuple(fields) => {
            for (_, t) in fields {
                substitute_typeparams(t, subs);
            }
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            for (_, t) in param_tys {
                substitute_typeparams(t, subs);
            }
            substitute_typeparams(return_ty, subs);
        }
        ResolvedType::Generic { args, .. } => {
            for a in args {
                substitute_typeparams(a, subs);
            }
        }
        ResolvedType::External { type_args, .. } => {
            for a in type_args {
                substitute_typeparams(a, subs);
            }
        }
        ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_)
        | ResolvedType::Error => {}
    }
}

impl IrLowerer<'_> {
    /// Best-effort conversion of a stringified type from the symbol
    /// table into a `ResolvedType`.
    ///
    /// Semantic stores let / inference types via `type_to_string`, which
    /// produces full type expressions like `[I32]`, `(String) -> String`,
    /// or `(x: I32, y: I32)`. This helper only handles the
    /// *simple* name cases (primitives, named structs/enums/traits, in-
    /// scope generic params); for anything composite it returns `None`
    /// so callers can fall back to the value's already-lowered type.
    /// an unrecognised *simple identifier* now surfaces
    /// as `UndefinedType` rather than silently lowering to
    /// `TypeParam(name)`.
    pub(in crate::ir::lower) fn string_to_resolved_type(
        &mut self,
        type_str: &str,
    ) -> Option<ResolvedType> {
        match type_str {
            "String" => Some(ResolvedType::Primitive(PrimitiveType::String)),
            "I32" => Some(ResolvedType::Primitive(PrimitiveType::I32)),
            "I64" => Some(ResolvedType::Primitive(PrimitiveType::I64)),
            "F32" => Some(ResolvedType::Primitive(PrimitiveType::F32)),
            "F64" => Some(ResolvedType::Primitive(PrimitiveType::F64)),
            "Boolean" => Some(ResolvedType::Primitive(PrimitiveType::Boolean)),
            "Path" => Some(ResolvedType::Primitive(PrimitiveType::Path)),
            "Regex" => Some(ResolvedType::Primitive(PrimitiveType::Regex)),
            "Never" => Some(ResolvedType::Primitive(PrimitiveType::Never)),
            // Inference's stringified marker for the `nil` literal;
            // matches the IR representation in `lower_literal`. Produces
            // `Optional<Never>` against the prelude enum.
            "Nil" => self.optional_of(ResolvedType::Primitive(PrimitiveType::Never)),
            name if is_simple_type_name(name) => {
                if let Some(id) = self.module.struct_id(name) {
                    Some(ResolvedType::Struct(id))
                } else if let Some(id) = self.module.enum_id(name) {
                    Some(ResolvedType::Enum(id))
                } else if let Some(id) = self.module.trait_id(name) {
                    Some(ResolvedType::Trait(id))
                } else if self.is_generic_param_in_scope(name) {
                    Some(ResolvedType::TypeParam(name.to_string()))
                } else {
                    self.errors
                        .push(crate::error::CompilerError::UndefinedType {
                            name: name.to_string(),
                            span: self.current_span,
                        });
                    Some(ResolvedType::Error)
                }
            }
            // Composite stringification (e.g. "[T]", "T?", "(a: T)",
            // "T -> U", "[K: V]"). The lowerer doesn't reparse these;
            // callers fall back to the value expression's resolved type.
            _ => None,
        }
    }

    /// Get field type from a resolved type.
    pub(in crate::ir::lower) fn get_field_type_from_resolved(
        &mut self,
        ty: &ResolvedType,
        field_name: &str,
    ) -> ResolvedType {
        if let ResolvedType::Struct(id) = ty {
            if let Some(struct_def) = self.module.get_struct(*id) {
                if let Some(field) = struct_def.fields.iter().find(|f| f.name == field_name) {
                    return field.ty.clone();
                }
            }
        }
        let bad = ty.clone();
        self.internal_error_type_if_concrete(
            &bad,
            format!(
                "get_field_type_from_resolved: no field `{field_name}` on type {bad:?}; semantic should have caught this"
            ),
        )
    }

    /// Get the field types of a specific variant from an enum type.
    ///
    /// Handles direct `Enum(id)`, a `Generic` whose base is an enum (so a
    /// match over e.g. `Option<T>` still finds its variants), and the
    /// `TypeParam("self")` impl-context fallback.
    pub(in crate::ir::lower) fn get_variant_fields(
        &self,
        enum_ty: &ResolvedType,
        variant_name: &str,
    ) -> Vec<ResolvedType> {
        // Receiver-side type arguments for a `Generic` scrutinee; used to
        // substitute the variant payload's `TypeParam` slots with concrete
        // types so match-arm bindings carry the right type post-lowering.
        // Optional<T> hits this path naturally now: it's the prelude-defined
        // generic enum, so `.some(T)` / `.none` resolve through the normal
        // enum lookup with substitution.
        let receiver_args: &[ResolvedType] = match enum_ty {
            ResolvedType::Generic { args, .. } => args,
            ResolvedType::Primitive(_)
            | ResolvedType::Struct(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Enum(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::Closure { .. }
            | ResolvedType::Error => &[],
        };
        let enum_id = match enum_ty {
            ResolvedType::Enum(id) => Some(*id),
            ResolvedType::Generic { base, .. } => match base {
                crate::ir::GenericBase::Enum(id) => Some(*id),
                crate::ir::GenericBase::Struct(_) | crate::ir::GenericBase::Trait(_) => None,
            },
            ResolvedType::TypeParam(name) if name == "self" => self
                .current_impl_struct
                .as_ref()
                .and_then(|impl_name| self.module.enum_id(impl_name)),
            ResolvedType::Primitive(_)
            | ResolvedType::Struct(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::Closure { .. }
            | ResolvedType::External { .. }
            | ResolvedType::TypeParam(_)
            | ResolvedType::Error => None,
        };
        if let Some(id) = enum_id {
            if let Some(enum_def) = self.module.get_enum(id) {
                if let Some(variant) = enum_def.variants.iter().find(|v| v.name == variant_name) {
                    let param_names: Vec<String> = if receiver_args.is_empty() {
                        Vec::new()
                    } else {
                        enum_def
                            .generic_params
                            .iter()
                            .map(|p| p.name.clone())
                            .collect()
                    };
                    return variant
                        .fields
                        .iter()
                        .map(|f| {
                            let mut ty = f.ty.clone();
                            if !param_names.is_empty() {
                                let subs: std::collections::HashMap<String, ResolvedType> =
                                    param_names
                                        .iter()
                                        .cloned()
                                        .zip(receiver_args.iter().cloned())
                                        .collect();
                                substitute_typeparams(&mut ty, &subs);
                            }
                            ty
                        })
                        .collect();
                }
            }
        }
        Vec::new()
    }
}
