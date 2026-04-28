//! Array- and dict-literal homogeneity checks: every entry must be
//! type-compatible with the first; mixes like `[1, "two"]` are rejected.

use super::super::super::module_resolver::ModuleResolver;
use super::super::super::SemanticAnalyzer;
use crate::ast::{Expr, File};
use crate::error::CompilerError;
use crate::location::Span;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Validate that every key (and value) in a dict literal is compatible
    /// with the first entry. A mix like `["a": 1, "b": "two"]` is rejected.
    pub(super) fn validate_dict_homogeneity(
        &mut self,
        entries: &[(Expr, Expr)],
        span: Span,
        file: &File,
    ) {
        let mut iter = entries.iter();
        let Some((first_k, first_v)) = iter.next() else {
            return; // empty dict: nothing to unify
        };
        let first_key_sem = self.infer_type_sem(first_k, file);
        let first_val_sem = self.infer_type_sem(first_v, file);
        let key_indeterminate = first_key_sem.is_unknown();
        let val_indeterminate = first_val_sem.is_unknown();
        let first_key_ty = first_key_sem.display();
        let first_val_ty = first_val_sem.display();
        for (k, v) in iter {
            if !key_indeterminate {
                let kty_sem = self.infer_type_sem(k, file);
                if !kty_sem.is_unknown() {
                    let kty = kty_sem.display();
                    if !self.type_strings_compatible(&first_key_ty, &kty) {
                        self.errors.push(CompilerError::TypeMismatch {
                            expected: format!("[{first_key_ty}: {first_val_ty}]"),
                            found: format!("key of type {kty}"),
                            span,
                        });
                        return;
                    }
                }
            }
            if !val_indeterminate {
                let vty_sem = self.infer_type_sem(v, file);
                if !vty_sem.is_unknown() {
                    let vty = vty_sem.display();
                    if !self.type_strings_compatible(&first_val_ty, &vty) {
                        self.errors.push(CompilerError::TypeMismatch {
                            expected: format!("[{first_key_ty}: {first_val_ty}]"),
                            found: format!("value of type {vty}"),
                            span,
                        });
                        return;
                    }
                }
            }
        }
    }

    /// Validate every element of an array literal is type-compatible with
    /// the first. Rejects mixes like `[1, "two"]`.
    pub(super) fn validate_array_homogeneity(
        &mut self,
        elements: &[Expr],
        span: Span,
        file: &File,
    ) {
        let mut iter = elements.iter();
        let Some(first) = iter.next() else {
            return; // empty array: nothing to unify
        };
        let first_sem = self.infer_type_sem(first, file);
        if first_sem.is_unknown() {
            // Can't trust the inference; skip rather than emit noise.
            return;
        }
        let first_ty = first_sem.display();
        for elem in iter {
            let elem_sem = self.infer_type_sem(elem, file);
            if elem_sem.is_unknown() {
                continue;
            }
            let elem_ty = elem_sem.display();
            if !self.type_strings_compatible(&first_ty, &elem_ty) {
                self.errors.push(CompilerError::TypeMismatch {
                    expected: format!("[{first_ty}]"),
                    found: format!("element of type {elem_ty}"),
                    span,
                });
                // Stop after the first mismatch so a single typo doesn't
                // cascade into N copies of the same diagnostic.
                break;
            }
        }
    }
}
