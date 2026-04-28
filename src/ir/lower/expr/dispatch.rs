//! Method-dispatch resolution: maps a `(receiver_ty, method_name)` to a
//! `DispatchKind` and provides the impl-/trait-lookup helpers used by
//! both `resolve_dispatch_kind` and `resolve_method_return_type`.

use crate::error::CompilerError;
use crate::ir::lower::IrLowerer;
use crate::ir::{DispatchKind, ImplId, ResolvedType, TraitId};

impl IrLowerer<'_> {
    /// Resolve the dispatch kind for a method call.
    ///
    /// * Concrete struct/enum receivers resolve to `Static` dispatch pointing
    ///   at the impl block that provides the method body. When the call site
    ///   is inside the impl that is still being lowered, the `ImplId` refers
    ///   to the slot that impl will occupy in `module.impls` once finalized.
    /// * Type-parameter receivers (`T: Trait`) and trait-object receivers
    ///   resolve to `Virtual` dispatch through the relevant trait.
    /// * Other receiver shapes (primitives, arrays, tuples, etc.) are a
    ///   compiler bug at this layer — semantic analysis should have rejected
    ///   them. We record an `InternalError` and return a sentinel
    ///   `Virtual` dispatch pointing at `TraitId(u32::MAX)` so downstream
    ///   code never silently emits against a bogus trait id.
    pub(super) fn resolve_dispatch_kind(
        &mut self,
        receiver_ty: &ResolvedType,
        method_name: &str,
    ) -> DispatchKind {
        // Unwrap a Generic wrapper to its base so `Box<T>.method()` and
        // `Option<T>.method()` dispatch the same way a concrete Struct/Enum
        // receiver would.
        let concrete = match receiver_ty {
            ResolvedType::Generic { base, .. } => match base {
                crate::ir::GenericBase::Struct(id) => Some(ResolvedType::Struct(*id)),
                crate::ir::GenericBase::Enum(id) => Some(ResolvedType::Enum(*id)),
                // A trait base wouldn't appear here for a method
                // call receiver post item E2. Stay None and let the
                // resolver fall through to the existing error path.
                crate::ir::GenericBase::Trait(_) => None,
            },
            ResolvedType::Primitive(_)
            | ResolvedType::Struct(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Enum(_)
            | ResolvedType::Array(_)
            | ResolvedType::Range(_)
            | ResolvedType::Optional(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::Dictionary { .. }
            | ResolvedType::Closure { .. }
            | ResolvedType::Error => None,
        };
        let effective_ty = concrete.as_ref().unwrap_or(receiver_ty);

        if let ResolvedType::Struct(struct_id) = effective_ty {
            if let Some(impl_id) = self.find_impl_for_struct(*struct_id, method_name) {
                return DispatchKind::Static { impl_id };
            }
            return DispatchKind::Static {
                impl_id: self.next_impl_id_or_record(),
            };
        }

        if let ResolvedType::Enum(enum_id) = effective_ty {
            if let Some(impl_id) = self.find_impl_for_enum(*enum_id, method_name) {
                return DispatchKind::Static { impl_id };
            }
            return DispatchKind::Static {
                impl_id: self.next_impl_id_or_record(),
            };
        }

        if let ResolvedType::TypeParam(param_name) = receiver_ty {
            if let Some(trait_id) = self.find_trait_for_method(param_name, method_name) {
                return DispatchKind::Virtual {
                    trait_id,
                    method_name: method_name.to_string(),
                };
            }
        }

        if let ResolvedType::Trait(trait_id) = receiver_ty {
            // Tier-1 item E2: trait values are banned at semantic time
            // (TraitUsedAsValueType). A receiver of `ResolvedType::Trait`
            // means semantic let one through — surface as an
            // InternalError instead of silently emitting Virtual
            // dispatch that the language doesn't otherwise permit.
            self.errors.push(CompilerError::InternalError {
                detail: format!(
                    "IR lowering: receiver type `Trait({})` reached method dispatch — \
                     semantic should have rejected the trait value at the call site",
                    trait_id.0
                ),
                span: self.current_span,
            });
            return DispatchKind::Virtual {
                trait_id: *trait_id,
                method_name: method_name.to_string(),
            };
        }

        self.errors.push(CompilerError::InternalError {
            detail: format!(
                "IR lowering: cannot resolve dispatch for method `{method_name}` on receiver {receiver_ty:?}"
            ),
            span: self.current_span,
        });
        DispatchKind::Virtual {
            trait_id: TraitId(u32::MAX),
            method_name: method_name.to_string(),
        }
    }

    /// Return the `ImplId` that will be assigned to the next impl block added.
    /// On u32 overflow, records a `TooManyDefinitions` error and returns a
    /// sentinel ID so compilation fails loudly rather than producing wrong dispatch.
    fn next_impl_id_or_record(&mut self) -> ImplId {
        self.module.next_impl_id().unwrap_or_else(|| {
            self.errors.push(CompilerError::TooManyDefinitions {
                kind: "impl",
                span: self.current_span,
            });
            ImplId(u32::MAX)
        })
    }

    /// Record `TooManyDefinitions` for an impl index that does not fit in `u32`
    /// and return a sentinel `ImplId`. Callers should have already established
    /// an `add_impl`-enforced invariant; this path exists purely to keep the
    /// compiler type-safe without an unchecked cast.
    fn impl_id_from_idx(&mut self, idx: usize) -> ImplId {
        if let Ok(v) = u32::try_from(idx) {
            ImplId(v)
        } else {
            self.errors.push(CompilerError::TooManyDefinitions {
                kind: "impl",
                span: self.current_span,
            });
            ImplId(u32::MAX)
        }
    }

    fn trait_id_from_idx(&mut self, idx: usize) -> TraitId {
        if let Ok(v) = u32::try_from(idx) {
            TraitId(v)
        } else {
            self.errors.push(CompilerError::TooManyDefinitions {
                kind: "trait",
                span: self.current_span,
            });
            TraitId(u32::MAX)
        }
    }

    fn find_impl_for_struct(
        &mut self,
        id: crate::ir::StructId,
        method_name: &str,
    ) -> Option<ImplId> {
        let found_idx = self.module.impls.iter().enumerate().find_map(|(idx, b)| {
            if b.struct_id() == Some(id) && b.functions.iter().any(|f| f.name == method_name) {
                Some(idx)
            } else {
                None
            }
        })?;
        Some(self.impl_id_from_idx(found_idx))
    }

    fn find_impl_for_enum(&mut self, id: crate::ir::EnumId, method_name: &str) -> Option<ImplId> {
        let found_idx = self.module.impls.iter().enumerate().find_map(|(idx, b)| {
            if b.enum_id() == Some(id) && b.functions.iter().any(|f| f.name == method_name) {
                Some(idx)
            } else {
                None
            }
        })?;
        Some(self.impl_id_from_idx(found_idx))
    }

    /// Look up the trait that declares `method_name` among the constraints
    /// attached to generic parameter `param_name`. Walks the innermost
    /// generic scope outwards, finds the param by name, then scans its
    /// trait constraints. Falls back to a module-wide search only when the
    /// param is not in any active scope (e.g. a lowering invariant was
    /// violated upstream) — this matches the pre-#12 behaviour so we don't
    /// regress on cases where the scope hasn't been populated.
    pub(super) fn find_trait_for_method(
        &mut self,
        param_name: &str,
        method_name: &str,
    ) -> Option<TraitId> {
        for frame in self.generic_scopes.iter().rev() {
            if let Some(param) = frame.iter().find(|p| p.name == param_name) {
                for constraint in &param.constraints {
                    let idx = constraint.trait_id.0 as usize;
                    if let Some(trait_def) = self.module.traits.get(idx) {
                        if trait_def.methods.iter().any(|m| m.name == method_name) {
                            return Some(constraint.trait_id);
                        }
                    }
                }
                // Param is in scope but none of its constraints declare the
                // method — the semantic analyser should already have flagged
                // this; return None rather than picking an unrelated trait.
                return None;
            }
        }
        // Fallback: no matching scope frame — behave as before.
        let found_idx = self
            .module
            .traits
            .iter()
            .enumerate()
            .find_map(|(idx, trait_def)| {
                trait_def
                    .methods
                    .iter()
                    .any(|m| m.name == method_name)
                    .then_some(idx)
            })?;
        Some(self.trait_id_from_idx(found_idx))
    }
}
