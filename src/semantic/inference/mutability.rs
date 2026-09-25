//! Mutability queries over expressions and bindings.
//!
//! Split out of `mod.rs` to keep each file under the project's
//! 500-line ceiling. Answers "may this be assigned to?" for a `mut`
//! argument check and for assignment validation.

use super::super::collect_bindings_from_pattern;
use super::super::module_resolver::ModuleResolver;
use super::super::SemanticAnalyzer;
use crate::ast::{Expr, File, Statement};

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Check if an expression is mutable
    /// An expression is mutable if:
    /// - It's a reference to a mutable let binding
    /// - It's a field access where the entire chain is mutable (upward propagation)
    /// - It's a context access that was marked as mutable
    ///
    /// An element of an array, an entry of a dictionary and a byte of a
    /// string are never mutable. [`Self::is_element_target`] names that
    /// rule on its own, because a caller must report it separately: no
    /// binding mutability makes such a write legal.
    #[expect(
        clippy::indexing_slicing,
        reason = "path[1..] is valid: path.len() >= 2 is guaranteed by the len==1 early return above"
    )]
    pub(in crate::semantic) fn is_expr_mutable(&self, expr: &Expr, file: &File) -> bool {
        match expr {
            // References can be mutable if they refer to mutable let bindings or fields
            Expr::Reference { path, .. } => {
                let Some(first) = path.first() else {
                    return false;
                };

                // Check if this is a reference to a let binding
                if path.len() == 1 {
                    return self.is_let_mutable(&first.name, file);
                }

                // For field access like `user.email`, check if:
                // 1. The root (user) is mutable
                // 2. The field (email) is mutable
                // Both must be true (upward propagation)
                let root_name = &first.name;
                let is_root_mutable = self.is_let_mutable(root_name, file);

                if !is_root_mutable {
                    return false;
                }

                // Check if all fields in the chain are mutable
                // For user.profile.email, we need: user is mut, profile field is mut, email field is mut
                Self::is_field_chain_mutable(&first.name, &path[1..], file)
            }

            // Literals, arrays, tuples, invocations, binary/unary ops,
            // for/if/match/closure/method-call expressions produce new values — not mutable
            Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::Literal { .. }
            | Expr::Invocation { .. }
            | Expr::EnumInstantiation { .. }
            | Expr::InferredEnumInstantiation { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::DictLiteral { .. }
            | Expr::DictAccess { .. }
            | Expr::ClosureExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Call { .. } => false,

            // Grouped expressions delegate to inner expression
            Expr::Group { expr, .. } => self.is_expr_mutable(expr, file),

            // Field access depends on the object
            Expr::FieldAccess { object, .. } => self.is_expr_mutable(object, file),

            // Let expressions delegate to their body
            Expr::LetExpr { body, .. } => self.is_expr_mutable(body, file),

            // Block expressions delegate to their result
            Expr::Block { result, .. } => self.is_expr_mutable(result, file),
        }
    }

    /// Report whether an assignment target names an element of a
    /// container.
    ///
    /// An array, a dictionary and a string are immutable in their
    /// elements, so `xs[0] = v`, `d[k] = v` and `s[0] = v` are all
    /// illegal, whatever the mutability of the binding. A field or a
    /// group above the index does not change the answer: `xs[0].f = v`
    /// still writes through the index.
    ///
    /// The rule is separate from [`Self::is_expr_mutable`] so that the
    /// caller reports the real cause. `let mut` is the fix for an
    /// immutable binding; it is not the fix for an element.
    pub(in crate::semantic) fn is_element_target(expr: &Expr) -> bool {
        match expr {
            Expr::DictAccess { .. } => true,

            // These three carry the target forward; the index below one
            // of them still governs the write.
            Expr::Group { expr: inner, .. } | Expr::FieldAccess { object: inner, .. } => {
                Self::is_element_target(inner)
            }
            Expr::LetExpr { body, .. } => Self::is_element_target(body),
            Expr::Block { result, .. } => Self::is_element_target(result),

            // Everything else is either a fresh value or a binding, and
            // neither reaches an element.
            Expr::Reference { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::Literal { .. }
            | Expr::Invocation { .. }
            | Expr::EnumInstantiation { .. }
            | Expr::InferredEnumInstantiation { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::DictLiteral { .. }
            | Expr::ClosureExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Call { .. } => false,
        }
    }

    /// Check if a let binding is mutable
    pub(in crate::semantic) fn is_let_mutable(&self, name: &str, file: &File) -> bool {
        // First check local let bindings (function params, block lets)
        if let Some((_, mutable)) = self.local_let_bindings.get(name) {
            return *mutable;
        }

        // Then check file-level let bindings
        for statement in &file.statements {
            if let Statement::Let(let_binding) = statement {
                // Check if the name is in any binding from this pattern
                for binding in collect_bindings_from_pattern(&let_binding.pattern) {
                    if binding.name == name {
                        return let_binding.mutable;
                    }
                }
            }
        }
        false
    }

    /// Check if a field access chain is mutable.
    ///
    /// Field-level mutability was removed; mutability lives entirely on
    /// the binding (`let mut`). A chain is mutable iff the root binding
    /// is — the field path itself adds no further restriction.
    pub(in crate::semantic) const fn is_field_chain_mutable(
        _root_name: &str,
        _field_path: &[crate::ast::Ident],
        _file: &File,
    ) -> bool {
        true
    }
}
