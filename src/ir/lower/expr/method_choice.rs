//! Choosing which method a call means.
//!
//! A type may declare several methods of one name, and the call's
//! labels and count say which. Split out of `helpers.rs` to keep each
//! file under the line ceiling that `scripts/check_file_sizes.sh`
//! enforces.

use crate::ir::lower::IrLowerer;
use crate::ir::IrExpr;

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
                    let named = imp
                        .functions
                        .iter()
                        .enumerate()
                        .filter(|(_, f)| f.name == method_name);
                    crate::ir::overload::choose(
                        named,
                        |f| f.params.as_slice(),
                        &labels,
                        call_args.len(),
                    )
                    .or_else(|| imp.functions.iter().position(|f| f.name == method_name))
                })
                .unwrap_or(0) as u32,
            crate::ir::DispatchKind::Virtual { trait_id, .. } => {
                self.module
                    .get_trait(*trait_id)
                    .and_then(|t| {
                        let named = t
                            .methods
                            .iter()
                            .enumerate()
                            .filter(|(_, m)| m.name == method_name);
                        crate::ir::overload::choose(
                            named,
                            |m| m.params.as_slice(),
                            &labels,
                            call_args.len(),
                        )
                        .or_else(|| t.methods.iter().position(|m| m.name == method_name))
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
    /// the one whose labels and count the call fits, and falls back to
    /// the first so a call that fits none still resolves to something
    /// the diagnostics can name.
    pub(super) fn method_for_call<'b>(
        impl_block: &'b crate::ir::IrImpl,
        method_name: &str,
        args: &[(Option<String>, IrExpr)],
    ) -> Option<&'b crate::ir::IrFunction> {
        let labels: Vec<Option<String>> = args.iter().map(|(label, _)| label.clone()).collect();
        let named: Vec<&crate::ir::IrFunction> = impl_block
            .functions
            .iter()
            .filter(|f| f.name == method_name)
            .collect();
        crate::ir::overload::choose(
            named.iter().copied().enumerate(),
            |f| f.params.as_slice(),
            &labels,
            args.len(),
        )
        .and_then(|index| named.get(index).copied())
        .or_else(|| named.first().copied())
    }
}
