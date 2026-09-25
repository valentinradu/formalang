//! Destructuring let lowering: array, struct, and tuple patterns.
//!
//! Each destructuring pattern is expanded into one [`IrLet`] per
//! introduced name, at any depth, with the synthesised access
//! expression (`arr[0]` / `tuple.field` / `struct.field`) as the value.
//! [`IrLowerer::destructure`] builds the accesses; this file adds them
//! to the module.
//!
//! The annotation-threading rule mirrors `lower_simple_let`: when the
//! let carries a type annotation, the value is lowered with the
//! annotation as its expected type, so closure literals inside the
//! value pick up their param types from the annotation instead of
//! falling back to [`crate::ir::ResolvedType::Error`].

use super::IrLowerer;
use crate::ast::LetBinding;
use crate::ir::IrLet;

impl IrLowerer<'_> {
    /// Lower a destructuring module-level let: `let [a, b] = value`,
    /// `let { field, other as alias } = value`, `let (a, b) = value`,
    /// and any nesting of them.
    pub(super) fn lower_destructuring_let(&mut self, let_binding: &LetBinding) {
        let annotation = let_binding
            .type_annotation
            .as_ref()
            .map(|t| self.lower_type(t));
        let value = self.lower_with_expected_value(&let_binding.value, annotation.as_ref());
        for (name, ty, access) in self.destructure(&let_binding.pattern, value) {
            self.module.add_let(IrLet {
                name,
                visibility: let_binding.visibility,
                mutable: let_binding.mutable,
                ty,
                value: access,
                doc: let_binding.doc.clone(),
                span: self.ir_span(let_binding.span),
            });
        }
    }
}
