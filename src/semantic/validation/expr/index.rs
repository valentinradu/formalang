//! Index expressions: `xs[i]`, `s[i]` and `d[key]`.
//!
//! Three shapes take an index. An array and a string take an `I32`
//! position. A dictionary takes a key of its key type.

use super::super::super::module_resolver::ModuleResolver;
use super::super::super::sem_type::SemType;
use super::super::super::SemanticAnalyzer;
use crate::ast::{Expr, File};
use crate::error::CompilerError;
use crate::location::Span;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Validate `dict[key]`.
    pub(super) fn validate_index_expr(&mut self, dict: &Expr, key: &Expr, span: Span, file: &File) {
        self.validate_expr(dict, file);
        // The receiver gives the key its expected type: the key
        // type of a dictionary, and `I32` for a position in an
        // array or a string.
        let key_expected = match self.infer_type_sem(dict, file) {
            SemType::Dictionary { key, .. } => Some(*key),
            SemType::Array(_) | SemType::Primitive(crate::ast::PrimitiveType::String) => {
                Some(SemType::Primitive(crate::ast::PrimitiveType::I32))
            }
            SemType::Primitive(_)
            | SemType::Named(_)
            | SemType::Optional(_)
            | SemType::Tuple(_)
            | SemType::Generic { .. }
            | SemType::Closure { .. }
            | SemType::Unknown
            | SemType::InferredEnum
            | SemType::Nil => None,
        };
        self.validate_expr_expecting(key, key_expected, file);
        self.check_position_index(dict, key, span, file);
        // Only three shapes take an index. Nothing checked
        // this, so `true[0]` reached IR lowering, which
        // reported an internal error and asked the user to
        // file a bug for a mistake in their own program.
        let receiver = self.infer_type_sem(dict, file);
        if !receiver.is_indeterminate() && !Self::is_indexable(&receiver) {
            self.errors.push(CompilerError::NotIndexable {
                actual: receiver.display(),
                span,
            });
        }
        // Validate key type against declared dict type.
        // Structural unpacking — no string scanning needed.
        if let SemType::Dictionary {
            key: expected_key, ..
        } = self.infer_type_sem(dict, file)
        {
            let actual_key_sem = self.infer_type_sem(key, file);
            // `value_satisfies_declared` waves through every
            // indeterminate type, which includes a `.variant`
            // whose enum comes from context. Comparing for
            // equality instead rejected `m[.pending]` against
            // `[Status: I32]`.
            if !self.value_satisfies_declared(&expected_key.display(), &actual_key_sem) {
                self.errors.push(CompilerError::TypeMismatch {
                    expected: expected_key.display(),
                    found: actual_key_sem.display(),
                    span,
                });
            }
        }
    }

    /// An array and a string take an `I32` position as the index.
    fn check_position_index(&mut self, receiver: &Expr, index: &Expr, span: Span, file: &File) {
        let receiver_sem = self.infer_type_sem(receiver, file);
        if !matches!(
            receiver_sem,
            SemType::Array(_) | SemType::Primitive(crate::ast::PrimitiveType::String)
        ) {
            return;
        }
        let index_sem = self.infer_type_sem(index, file);
        if index_sem.is_indeterminate() {
            return;
        }
        if index_sem != SemType::Primitive(crate::ast::PrimitiveType::I32) {
            self.errors.push(CompilerError::TypeMismatch {
                expected: "I32".to_string(),
                found: index_sem.display(),
                span,
            });
        }
    }

    const fn is_indexable(ty: &SemType) -> bool {
        matches!(
            ty,
            SemType::Array(_)
                | SemType::Dictionary { .. }
                | SemType::Primitive(crate::ast::PrimitiveType::String)
        )
    }
}
