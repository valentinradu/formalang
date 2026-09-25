//! Lowering for references: a local binding, a parameter, a module
//! `let`, a field path, and `self`. Split out of `operators.rs` to keep
//! each file under the line ceiling that `scripts/check_file_sizes.sh`
//! enforces.

use crate::error::CompilerError;
use crate::ir::lower::IrLowerer;
use crate::ir::{IrExpr, ResolvedType};

impl IrLowerer<'_> {
    pub(super) fn lower_reference(&mut self, path: &[crate::ast::Ident]) -> IrExpr {
        let path_strs: Vec<String> = path.iter().map(|i| i.name.clone()).collect();

        // Check for self.field pattern — bounds verified by len() == 2 check
        #[expect(
            clippy::indexing_slicing,
            reason = "len == 2 check above guarantees indices 0 and 1"
        )]
        if path_strs.len() == 2 && path_strs[0] == "self" {
            let field_name = &path_strs[1];
            let ty = self.resolve_self_field_type(field_name);
            return IrExpr::SelfFieldRef {
                field: field_name.clone(),
                field_idx: crate::ir::FieldIdx(0),
                ty,
                span: self.current_ir_span(),
            };
        }

        // Check for bare "self" in impl context — bounds verified by len() == 1 check
        #[expect(
            clippy::indexing_slicing,
            reason = "len == 1 check above guarantees index 0"
        )]
        if path_strs.len() == 1 && path_strs[0] == "self" {
            if let Some(impl_name) = self.current_impl_struct.clone() {
                let ty = self.resolve_impl_self_type(&impl_name);
                return IrExpr::Reference {
                    path: path_strs,
                    target: crate::ir::ReferenceTarget::Unresolved,
                    ty,
                    span: self.current_ir_span(),
                };
            }
        }

        // A module-level `let`, unless a local binding shadows it.
        if path_strs.len() == 1 {
            #[expect(
                clippy::indexing_slicing,
                reason = "len == 1 check above guarantees index 0"
            )]
            let name = &path_strs[0];
            if self.lookup_local_binding(name).is_none() {
                if let Some(ty) = self.module_let_type(name) {
                    return IrExpr::LetRef {
                        name: self.module_let_ir_name(name),
                        binding_id: crate::ir::BindingId(0),
                        ty,
                        span: self.current_ir_span(),
                    };
                }
            }
        }

        // For a multi-segment path whose root is a local binding or a
        // module-level `let`, emit a chain of `FieldAccess` over a
        // `LetRef` rather than keeping the joined path on
        // `IrExpr::Reference`. The resolve-references pass only matches
        // `Reference` paths against module-level symbols, so a
        // multi-segment `b.value` would otherwise surface as
        // `UndefinedReference("b::value")` even when `b` is a local.
        let root_binding = if path_strs.len() > 1 {
            path_strs.first().and_then(|n| {
                if let Some(ty) = self.lookup_local_binding(n).cloned() {
                    return Some((n.clone(), ty));
                }
                self.module_let_type(n)
                    .map(|ty| (self.module_let_ir_name(n), ty))
            })
        } else {
            None
        };
        if let Some((root_name, root_ty)) = root_binding {
            let mut current_expr = IrExpr::LetRef {
                name: root_name,
                binding_id: crate::ir::BindingId(0),
                ty: root_ty,
                span: self.current_ir_span(),
            };
            for seg in path_strs.iter().skip(1) {
                let field_ty = self.resolve_field_type(current_expr.ty(), seg);
                current_expr = IrExpr::FieldAccess {
                    object: Box::new(current_expr),
                    field: seg.clone(),
                    field_idx: crate::ir::FieldIdx(0),
                    ty: field_ty,
                    span: self.current_ir_span(),
                };
            }
            return current_expr;
        }
        // A single local binding. A module-level `let` and every
        // multi-segment path with a known root returned above.
        let root = path_strs
            .first()
            .and_then(|n| self.lookup_local_binding(n).cloned());
        let ty = if let Some(root_ty) = root {
            let mut current = root_ty;
            for seg in path_strs.iter().skip(1) {
                current = self.resolve_field_type(&current, seg);
            }
            current
        } else {
            let span = path.first().map_or(self.current_span, |i| i.span);
            self.errors.push(CompilerError::UndefinedReference {
                name: path_strs.join("."),
                span,
            });
            ResolvedType::Error
        };
        IrExpr::Reference {
            path: path_strs,
            target: crate::ir::ReferenceTarget::Unresolved,
            ty,
            span: self.current_ir_span(),
        }
    }

    /// The name that a reference to the module-level `let` `name`
    /// carries. It is `name`, unless the linker gives the `let` another
    /// name in the finished module.
    fn module_let_ir_name(&self, name: &str) -> String {
        self.linked_let_names
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string())
    }

    /// The type of the module-level `let` with the name `name`, or
    /// `None` if no module-level `let` has that name.
    ///
    /// The pre-pass records each type that semantic analysis settled.
    /// Any other type comes from the lowered value, so it is known only
    /// after the lowering of the `let`. A reference before that is an
    /// internal error, not a silent `Error` type.
    pub(in crate::ir::lower) fn module_let_type(&mut self, name: &str) -> Option<ResolvedType> {
        if let Some(ty) = self.module_let_types.get(name) {
            return Some(ty.clone());
        }
        if let Some(ty) = self.deferred_module_let_type(name) {
            return Some(ty);
        }
        self.symbols.get_let_type(name)?;
        let lowered = self
            .module
            .lets
            .iter()
            .find(|l| l.name == name)
            .map(|l| l.value.ty().clone());
        Some(lowered.unwrap_or_else(|| {
            self.internal_error_type(format!(
                "IR lowering: the type of module-level let `{name}` is not known before its value is lowered"
            ))
        }))
    }
}
