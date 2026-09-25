//! Enum-instantiation lowering: explicit `Enum::Variant {..}` and the
//! inferred `.Variant {..}` form whose enum target comes from the
//! current return-type context.

use std::collections::HashMap;

use crate::ast::Expr;
use crate::error::CompilerError;
use crate::ir::lower::expr::type_params::{holds_a_type_param, substitute_typeparam_in_resolved};
use crate::ir::lower::IrLowerer;
use crate::ir::{IrExpr, ResolvedType};

impl IrLowerer<'_> {
    pub(in crate::ir::lower::expr) fn lower_enum_instantiation(
        &mut self,
        enum_name: &str,
        variant: &str,
        data: &[(crate::ast::Ident, Expr)],
    ) -> IrExpr {
        let scoped = self.scoped_type_name(enum_name);
        let (enum_id, ty) = self.module.enum_id(&scoped).map_or_else(
            || {
                let ty = self
                    .try_external_type(enum_name, vec![])
                    .unwrap_or_else(|| {
                        // Semantic analysis refuses an enum name that no
                        // enum has, so this is a compiler bug, not a user
                        // error.
                        self.internal_error_type(format!(
                            "IR lowering: enum `{enum_name}` of `.{variant}` is not defined"
                        ))
                    });
                (None, ty)
            },
            |id| (Some(id), ResolvedType::Enum(id)),
        );
        // A context that expects an instance of this enum gives the
        // type arguments, and each payload expects its field type with
        // them put in: `let m: Maybe<I32?> = Maybe.some(v: nil)`.
        // An expected type that holds a type parameter can come from the
        // callee's signature, `fn get<T>(m: Maybe<T>)`, where `T` is not a
        // type of this scope. The payload gives the better type there.
        let expected = enum_id
            .and_then(|id| self.expected_instance_of(id))
            .filter(|ty| !holds_a_type_param(ty));
        let field_tys: HashMap<String, ResolvedType> = match (enum_id, &expected) {
            (Some(id), Some(ResolvedType::Generic { args, .. })) => {
                self.variant_field_types(id, variant, args)
            }
            _ => HashMap::new(),
        };
        let fields: Vec<(String, crate::ir::FieldIdx, IrExpr)> = data
            .iter()
            .map(|(n, e)| {
                let lowered = self.lower_with_expected_value(e, field_tys.get(&n.name));
                (n.name.clone(), crate::ir::FieldIdx(0), lowered)
            })
            .collect();
        let ty = expected
            .or_else(|| enum_id.and_then(|id| self.enum_type_from_payload(id, variant, &fields)))
            .unwrap_or(ty);
        IrExpr::EnumInst {
            enum_id,
            variant: variant.to_string(),
            variant_idx: crate::ir::VariantIdx(0),
            fields,
            ty,
            span: self.current_ir_span(),
        }
    }

    /// The expected type of the value when it is an instance of the
    /// generic enum `id`, with an optional peeled.
    fn expected_instance_of(&self, id: crate::ir::EnumId) -> Option<ResolvedType> {
        let expected = self.expected_value_type.as_ref()?;
        let inner = self.module.optional_inner_ty(expected).unwrap_or(expected);
        matches!(
            inner,
            ResolvedType::Generic {
                base: crate::ir::GenericBase::Enum(e),
                ..
            } if *e == id
        )
        .then(|| inner.clone())
    }

    /// The field types of `variant` of the generic enum `id`, with the
    /// type arguments `args` put in.
    fn variant_field_types(
        &self,
        id: crate::ir::EnumId,
        variant: &str,
        args: &[ResolvedType],
    ) -> HashMap<String, ResolvedType> {
        let Some(def) = self.module.get_enum(id) else {
            return HashMap::new();
        };
        let subs: HashMap<String, ResolvedType> = def
            .generic_params
            .iter()
            .zip(args)
            .map(|(p, a)| (p.name.clone(), a.clone()))
            .collect();
        def.variants
            .iter()
            .find(|v| v.name == variant)
            .map(|v| {
                v.fields
                    .iter()
                    .map(|f| {
                        let mut ty = f.ty.clone();
                        substitute_typeparam_in_resolved(&mut ty, &subs);
                        (f.name.clone(), ty)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The type of a generic enum's variant from the types of its
    /// payload: `Opt.some(value: 4)` is an `Opt<I32>`. `None` when the
    /// enum is not generic, or when the payload does not give every
    /// type parameter a type.
    fn enum_type_from_payload(
        &self,
        id: crate::ir::EnumId,
        variant: &str,
        fields: &[(String, crate::ir::FieldIdx, IrExpr)],
    ) -> Option<ResolvedType> {
        let def = self.module.get_enum(id)?;
        if def.generic_params.is_empty() {
            return None;
        }
        let declared = def.variants.iter().find(|v| v.name == variant)?;
        let mut bindings = std::collections::HashMap::new();
        for (name, _, value) in fields {
            if let Some(field) = declared.fields.iter().find(|f| &f.name == name) {
                super::invocation::unify_type_args(&field.ty, value.ty(), &mut bindings);
            }
        }
        let args: Option<Vec<ResolvedType>> = def
            .generic_params
            .iter()
            .map(|p| bindings.get(&p.name).cloned())
            .collect();
        Some(ResolvedType::Generic {
            base: crate::ir::GenericBase::Enum(id),
            args: args?,
        })
    }

    /// Give a lowered variant the type arguments that its path writes:
    /// `Maybe<I32>.none` has the type `Maybe<I32>`.
    pub(in crate::ir::lower::expr) fn apply_enum_path_type_args(
        &mut self,
        mut inst: IrExpr,
        type_args: &[crate::ast::Type],
    ) -> IrExpr {
        if type_args.is_empty() {
            return inst;
        }
        let args: Vec<ResolvedType> = type_args.iter().map(|t| self.lower_type(t)).collect();
        if let IrExpr::EnumInst {
            enum_id: Some(id),
            ty,
            ..
        } = &mut inst
        {
            *ty = ResolvedType::Generic {
                base: crate::ir::GenericBase::Enum(*id),
                args,
            };
        }
        inst
    }

    /// Peel `Optional<T>` and report the enum a `.variant` literal
    /// should build, given the type the context expects.
    ///
    /// `Some` for a local enum, a generic enum instantiation, or an
    /// external enum. `None` when the expected type is not an enum at
    /// all — a mismatch the semantic analyzer owns, so lowering must
    /// not raise a second diagnostic for it.
    fn enum_target_of(
        &self,
        expected: &ResolvedType,
    ) -> Option<(Option<crate::ir::EnumId>, ResolvedType)> {
        match expected {
            ResolvedType::Enum(id) => Some((Some(*id), ResolvedType::Enum(*id))),
            ResolvedType::Generic {
                base: crate::ir::GenericBase::Enum(id),
                args,
            } => {
                // `T?` is `Optional<T>`; peel it so `.variant` on an
                // optional-typed slot reaches the inner enum.
                if Some(*id) == self.module.prelude_optional_id() {
                    if let [inner] = args.as_slice() {
                        return self.enum_target_of(inner);
                    }
                }
                Some((Some(*id), expected.clone()))
            }
            ResolvedType::External {
                kind: crate::ir::ImportedKind::Enum,
                ..
            } => Some((None, expected.clone())),
            ResolvedType::Primitive(_)
            | ResolvedType::Struct(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::Generic { .. }
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::Closure { .. }
            | ResolvedType::Error => None,
        }
    }

    pub(in crate::ir::lower::expr) fn lower_inferred_enum_instantiation(
        &mut self,
        variant: &str,
        data: &[(crate::ast::Ident, Expr)],
    ) -> IrExpr {
        // The context that knows which enum `.variant` belongs to is
        // whatever type the surrounding expression expects: a `let`
        // annotation, an array element slot, a call argument, a struct
        // or enum field, or the enclosing function's return type. Take
        // it — the site that set it restores the previous value.
        let expected = self
            .expected_value_type
            .take()
            .or_else(|| self.current_function_return_type.clone());
        let (enum_id, ty) =
            if let Some(target) = expected.as_ref().and_then(|t| self.enum_target_of(t)) {
                target
            } else {
                // No expected type reaches here, so the enum is genuinely
                // unknowable and the author must annotate. An expected
                // type that is not an enum is a type mismatch the semantic
                // analyzer already reports, so stay quiet about that one
                // and let its diagnostic stand alone.
                if expected.is_none() {
                    self.errors.push(CompilerError::CannotInferEnumType {
                        variant: variant.to_string(),
                        span: self.current_span,
                    });
                }
                (None, ResolvedType::Error)
            };

        // Each variant field lowers against its own declared type, so a
        // nested `.variant` inside the payload resolves too.
        let field_types: HashMap<String, ResolvedType> = enum_id
            .and_then(|id| self.module.get_enum(id))
            .and_then(|e| e.variants.iter().find(|v| v.name == variant))
            .map(|v| {
                v.fields
                    .iter()
                    .map(|f| (f.name.clone(), f.ty.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let fields = data
            .iter()
            .map(|(n, e)| {
                let lowered =
                    self.lower_with_expected_value(e, field_types.get(&n.name).cloned().as_ref());
                (n.name.clone(), crate::ir::FieldIdx(0), lowered)
            })
            .collect();

        IrExpr::EnumInst {
            enum_id,
            variant: variant.to_string(),
            variant_idx: crate::ir::VariantIdx(0),
            fields,
            ty,
            span: self.current_ir_span(),
        }
    }
}
