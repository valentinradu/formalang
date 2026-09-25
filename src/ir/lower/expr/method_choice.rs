//! Choosing which method a call means, and reading its signature.
//!
//! A type may declare several methods of one name, and the call's
//! labels and count say which. Split out of `helpers.rs` and
//! `operators.rs` to keep each file under the line ceiling that
//! `scripts/check_file_sizes.sh` enforces.

use crate::ir::lower::expr::operators::signature_of;
use crate::ir::lower::IrLowerer;
use crate::ir::{IrExpr, ResolvedType};
use std::collections::HashMap;

impl IrLowerer<'_> {
    /// Which method inside its impl block or trait the call means.
    ///
    /// The same index `ResolveReferencesPass` computes, written at
    /// lowering time so the module is right the moment it is built.
    pub(in crate::ir::lower::expr) fn method_index(
        &self,
        dispatch: &crate::ir::DispatchKind,
        method_name: &str,
        call_args: &[(Option<String>, IrExpr)],
    ) -> crate::ir::MethodIdx {
        let labels: Vec<Option<String>> =
            call_args.iter().map(|(label, _)| label.clone()).collect();
        let arg_types: Vec<ResolvedType> =
            call_args.iter().map(|(_, arg)| arg.ty().clone()).collect();

        #[expect(
            clippy::cast_possible_truncation,
            reason = "method count is bounded upstream"
        )]
        let index = match dispatch {
            crate::ir::DispatchKind::Static { impl_id } => self
                .module
                .impls
                .get(impl_id.0 as usize)
                .and_then(|imp| {
                    crate::ir::overload::method_index(
                        &imp.functions,
                        |f| f.name.as_str(),
                        |f| f.params.as_slice(),
                        method_name,
                        &labels,
                        &arg_types,
                    )
                })
                .unwrap_or(0) as u32,
            crate::ir::DispatchKind::Virtual { trait_id, .. } => {
                self.module
                    .get_trait(*trait_id)
                    .and_then(|t| {
                        crate::ir::overload::method_index(
                            &t.methods,
                            |m| m.name.as_str(),
                            |m| m.params.as_slice(),
                            method_name,
                            &labels,
                            &arg_types,
                        )
                    })
                    .unwrap_or(0) as u32
            }
        };
        crate::ir::MethodIdx(index)
    }

    /// The method of `impl_block` that a call of this name and shape
    /// means.
    ///
    /// A type may declare several methods of one name. Taking the
    /// first of the right name made the second unreachable; this takes
    /// the one whose labels and count the call fits, by the rule of
    /// `crate::ir::overload`, and falls back to the first so a call
    /// that fits none still resolves to something the diagnostics can
    /// name. `labels` holds one entry for each argument.
    pub(super) fn method_for_call<'b>(
        impl_block: &'b crate::ir::IrImpl,
        method_name: &str,
        labels: &[Option<String>],
    ) -> Option<&'b crate::ir::IrFunction> {
        let named: Vec<&crate::ir::IrFunction> = impl_block
            .functions
            .iter()
            .filter(|f| f.name == method_name)
            .collect();
        crate::ir::overload::choose(
            named.iter().copied().enumerate(),
            |f| f.params.as_slice(),
            labels,
            labels.len(),
        )
        .and_then(|index| named.get(index).copied())
        .or_else(|| named.first().copied())
    }

    /// The method of `impl_block` that a call with these lowered
    /// arguments means: the labels, the count and the argument types
    /// decide, by the rule of `crate::ir::overload::method_index`.
    pub(super) fn method_for_typed_call<'b>(
        impl_block: &'b crate::ir::IrImpl,
        method_name: &str,
        labels: &[Option<String>],
        arg_types: &[ResolvedType],
    ) -> Option<&'b crate::ir::IrFunction> {
        crate::ir::overload::method_index(
            &impl_block.functions,
            |f| f.name.as_str(),
            |f| f.params.as_slice(),
            method_name,
            labels,
            arg_types,
        )
        .and_then(|index| impl_block.functions.get(index))
    }

    /// The signature of the impl method that a call on `receiver_ty`
    /// means, and the map from the type parameters of the receiver's
    /// type to the receiver's type arguments.
    ///
    /// `None` when the method cannot be resolved here (generic dispatch
    /// through a trait): the caller then gives its arguments no
    /// expected type.
    pub(super) fn method_callee(
        &self,
        receiver_ty: &ResolvedType,
        method_name: &str,
        labels: &[Option<String>],
    ) -> Option<(crate::ir::IrFunction, HashMap<String, ResolvedType>)> {
        let target = match receiver_ty {
            ResolvedType::Generic { base, .. } => match base {
                crate::ir::GenericBase::Struct(id) => Some(crate::ir::ImplTarget::Struct(*id)),
                crate::ir::GenericBase::Enum(id) => Some(crate::ir::ImplTarget::Enum(*id)),
                // A generic trait base can't be a method-call
                // receiver (FormaLang has no dynamic dispatch). Phase
                // E2 rejects trait values; this branch is here only
                // to keep the match exhaustive.
                crate::ir::GenericBase::Trait(_) => None,
            },
            ResolvedType::Struct(id) => Some(crate::ir::ImplTarget::Struct(*id)),
            ResolvedType::Enum(id) => Some(crate::ir::ImplTarget::Enum(*id)),
            // `extern impl String { ... }`: a method of a primitive may
            // have type parameters of its own too.
            ResolvedType::Primitive(p) => Some(crate::ir::ImplTarget::Primitive(*p)),
            ResolvedType::Trait(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::Closure { .. }
            | ResolvedType::Error => None,
        }?;
        let impl_block = self
            .module
            .impls
            .iter()
            .chain(&self.declared_impls)
            .find(|b| b.target == target && b.functions.iter().any(|f| f.name == method_name))?;
        let func = Self::method_for_call(impl_block, method_name, labels)?;
        Some((
            signature_of(func),
            self.receiver_subs(receiver_ty, impl_block),
        ))
    }

    /// Map the type parameters of the receiver's type to the receiver's
    /// type arguments.
    ///
    /// A method on a generic type declares its parameters against the
    /// type's parameters, so `Seq<I32>::filter` takes `(I32) -> Boolean`
    /// rather than `(T) -> Boolean`. The type's own declaration names
    /// them. A target that is not lowered yet has none in the module;
    /// the impl declares the same names, in the same order. Empty when
    /// the receiver carries no type arguments.
    pub(super) fn receiver_subs(
        &self,
        receiver_ty: &ResolvedType,
        impl_block: &crate::ir::IrImpl,
    ) -> HashMap<String, ResolvedType> {
        let ResolvedType::Generic { args, .. } = receiver_ty else {
            return HashMap::new();
        };
        let declared = match impl_block.target {
            crate::ir::ImplTarget::Struct(id) => {
                self.module.get_struct(id).map(|s| s.generic_params.clone())
            }
            crate::ir::ImplTarget::Enum(id) => {
                self.module.get_enum(id).map(|e| e.generic_params.clone())
            }
            crate::ir::ImplTarget::Primitive(_) => None,
        }
        .filter(|params| !params.is_empty())
        .unwrap_or_else(|| impl_block.generic_params.clone());
        declared
            .iter()
            .zip(args)
            .map(|(p, a)| (p.name.clone(), a.clone()))
            .collect()
    }
}
