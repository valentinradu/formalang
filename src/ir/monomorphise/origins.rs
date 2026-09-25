//! Where each specialised struct and enum came from.
//!
//! Phase 2 rewrites `Box<I32>` to `Struct(Box__I32)` everywhere before
//! Phase 2d infers the type arguments of a generic call. A parameter
//! declared as `Box<T>` then meets an argument of type
//! `Struct(Box__I32)`, and the two have no common shape. This table
//! gives the unification the `(Box, [I32])` that `Box__I32` was made
//! from, so `T` still binds to `I32`.

use std::collections::HashMap;

use crate::error::CompilerError;
use crate::ir::{GenericBase, IrModule, ResolvedType};

use super::rewrite::rewrite_type;
use super::specialise::Instantiation;

/// The deepest nesting of type arguments that a specialisation may have.
///
/// A generic that uses itself with a larger type, as `grow<T>` does
/// with `grow(x: Box(value: x))`, needs a new copy at each level, so
/// the worklist never empties. Real programs nest a few levels deep; a
/// type past this depth comes from that kind of recursion.
pub(super) const MAX_INSTANTIATION_DEPTH: usize = 32;

/// The instantiation behind each specialisation, and the reverse.
#[derive(Default)]
pub(super) struct Origins {
    forward: HashMap<Instantiation, GenericBase>,
    inverse: HashMap<GenericBase, Instantiation>,
}

impl Origins {
    /// Record that `spec` was made from `inst`.
    pub(super) fn insert(&mut self, inst: Instantiation, spec: GenericBase) {
        self.inverse.insert(spec, inst.clone());
        self.forward.insert(inst, spec);
    }

    /// The error for `inst` when its type arguments nest deeper than
    /// [`MAX_INSTANTIATION_DEPTH`], or `None` when it may be made.
    /// `written` tells that the type comes from the program text, and not
    /// from a generic that calls itself.
    pub(super) fn too_deep(
        &self,
        inst: &Instantiation,
        module: &IrModule,
        written: bool,
    ) -> Option<CompilerError> {
        let (base, args) = inst;
        let depth = args.iter().map(|a| self.depth(a)).max().unwrap_or(0);
        if depth <= MAX_INSTANTIATION_DEPTH {
            return None;
        }
        let (name, span) = match *base {
            GenericBase::Struct(id) => module.get_struct(id).map(|s| (&s.name, s.span)),
            GenericBase::Enum(id) => module.get_enum(id).map(|e| (&e.name, e.span)),
            GenericBase::Trait(id) => module.get_trait(id).map(|t| (&t.name, t.span)),
        }?;
        Some(CompilerError::InstantiationDepthExceeded {
            name: name.clone(),
            limit: MAX_INSTANTIATION_DEPTH,
            written,
            span: span.span,
        })
    }

    /// The instantiation that the concrete type `ty` was made from, or
    /// `None` when `ty` is not a specialisation.
    pub(super) fn of(&self, ty: &ResolvedType) -> Option<&Instantiation> {
        let base = if let ResolvedType::Struct(id) = ty {
            GenericBase::Struct(*id)
        } else if let ResolvedType::Enum(id) = ty {
            GenericBase::Enum(*id)
        } else {
            return None;
        };
        self.inverse.get(&base)
    }

    /// How deep the type arguments nest in `ty`. A specialisation counts
    /// as the instantiation that it was made from, so `Box__I32` and
    /// `Box<I32>` have one depth.
    pub(super) fn depth(&self, ty: &ResolvedType) -> usize {
        let deepest = |tys: &mut dyn Iterator<Item = &ResolvedType>| {
            tys.map(|t| self.depth(t)).max().unwrap_or(0)
        };
        let inner = match ty {
            ResolvedType::Generic { args, .. }
            | ResolvedType::External {
                type_args: args, ..
            } => deepest(&mut args.iter()),
            ResolvedType::Tuple(fields) => deepest(&mut fields.iter().map(|(_, t)| t)),
            ResolvedType::Closure {
                param_tys,
                return_ty,
            } => deepest(&mut param_tys.iter().map(|(_, t)| t).chain([&**return_ty])),
            ResolvedType::Struct(_) | ResolvedType::Enum(_) => match self.of(ty) {
                Some((_, args)) => deepest(&mut args.iter()),
                None => 0,
            },
            ResolvedType::Primitive(_)
            | ResolvedType::Trait(_)
            | ResolvedType::TypeParam(_)
            | ResolvedType::Error => 0,
        };
        inner.saturating_add(1)
    }

    /// `ty` with each instantiation that has a specialisation replaced
    /// by it. Two calls that give one type in two forms then give the
    /// same type arguments, and share one copy of the function.
    pub(super) fn canonical(&self, ty: &ResolvedType) -> ResolvedType {
        let mut out = ty.clone();
        rewrite_type(&mut out, &self.forward);
        out
    }
}
