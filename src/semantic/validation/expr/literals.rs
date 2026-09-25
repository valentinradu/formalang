//! Dictionary literals, and the array- and dict-literal homogeneity
//! checks: every entry must be type-compatible with the first; mixes
//! like `[1, "two"]` are rejected.

use super::super::super::literal_types;
use super::super::super::module_resolver::ModuleResolver;
use super::super::super::SemanticAnalyzer;
use crate::ast::{Expr, File, Literal, NumberLiteral, NumberValue, PrimitiveType};
use crate::error::CompilerError;
use crate::location::Span;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Validate a dictionary literal in a position that expects
    /// `expected`. `key_reported` is true when the expected type holds a
    /// float key that its annotation reports already.
    pub(super) fn validate_dict_literal(
        &mut self,
        entries: &[(Expr, Expr)],
        span: Span,
        expected: Option<&crate::semantic::sem_type::SemType>,
        key_reported: bool,
        file: &File,
    ) {
        let (key_expected, value_expected) = Self::expected_entry(expected);
        for (key, value) in entries {
            self.validate_expr_expecting(key, key_expected.clone(), file);
            self.validate_expr_expecting(value, value_expected.clone(), file);
        }
        // Escape analysis: closure values stored as dict keys/values escape.
        for (key, value) in entries {
            self.escape_closure_value(key);
            self.escape_closure_value(value);
        }
        // unify key types and value types across
        // entries so a heterogeneous dict literal
        // (`["a": 1, "b": "two"]`) surfaces as a real
        // TypeMismatch instead of silently using the first
        // entry's type.
        self.validate_dict_homogeneity(entries, span, file);
        if !key_reported {
            self.check_literal_float_key(entries, span, file);
        }
    }

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
        let mut joined_key = self.infer_type_sem(first_k, file);
        let mut joined_val = self.infer_type_sem(first_v, file);
        let key_indeterminate = joined_key.is_unknown();
        let val_indeterminate = joined_val.is_unknown();

        // Keys and values each fold with the join, the same way an
        // array's elements do.
        for (k, v) in iter {
            if !key_indeterminate {
                let kty_sem = self.infer_type_sem(k, file);
                if !kty_sem.is_unknown() {
                    let kty = kty_sem.display();
                    let known = !kty_sem.is_indeterminate() && !joined_key.is_indeterminate();
                    let next = joined_key.clone().join(kty_sem);
                    if next.is_unknown() || (known && next.is_indeterminate()) {
                        self.errors.push(CompilerError::TypeMismatch {
                            expected: format!(
                                "[{}: {}]",
                                joined_key.display(),
                                joined_val.display()
                            ),
                            found: format!("key of type {kty}"),
                            span,
                        });
                        return;
                    }
                    joined_key = next;
                }
            }
            if !val_indeterminate {
                let vty_sem = self.infer_type_sem(v, file);
                if !vty_sem.is_unknown() {
                    let vty = vty_sem.display();
                    let known = !vty_sem.is_indeterminate() && !joined_val.is_indeterminate();
                    let next = joined_val.clone().join(vty_sem);
                    if next.is_unknown() || (known && next.is_indeterminate()) {
                        self.errors.push(CompilerError::TypeMismatch {
                            expected: format!(
                                "[{}: {}]",
                                joined_key.display(),
                                joined_val.display()
                            ),
                            found: format!("value of type {vty}"),
                            span,
                        });
                        return;
                    }
                    joined_val = next;
                }
            }
        }
    }

    /// Record the type that `lit` takes from its position, when it is
    /// an unsuffixed number and the position expects a numeric type of
    /// the same kind. Return the type of the literal.
    pub(super) fn record_literal_type(
        &mut self,
        expr: &Expr,
        lit: &Literal,
        expected: Option<&crate::semantic::sem_type::SemType>,
    ) -> Option<PrimitiveType> {
        let Literal::Number(n) = lit else {
            return None;
        };
        if n.suffix.is_none() {
            if let Some(target) = literal_types::contextual_type(n.kind, expected) {
                // The default needs no record: the AST gives it already.
                if target != n.kind.default_primitive() {
                    self.literal_types
                        .insert(literal_types::node_key(expr), target);
                }
                return Some(target);
            }
        }
        Some(n.primitive_type())
    }

    /// Validate that a numeric literal fits in `target`, its type.
    ///
    /// An integer value must be in the range of an integer target. A
    /// value with a fraction cannot have an integer type. A float value
    /// must be finite in its float target: `3.5e38F32` is too large.
    pub(in crate::semantic) fn validate_numeric_literal(
        &mut self,
        lit: &Literal,
        target: Option<PrimitiveType>,
        span: Span,
    ) {
        let (Literal::Number(n), Some(target)) = (lit, target) else {
            return;
        };
        let NumberLiteral { value, .. } = *n;
        match value {
            NumberValue::Integer(v) => {
                let in_range = match target {
                    PrimitiveType::I32 => i32::try_from(v).is_ok(),
                    PrimitiveType::I64 => i64::try_from(v).is_ok(),
                    // An integer-syntax literal with a float suffix is
                    // a float value, and every such value has a float
                    // form. A `Number` literal has no other type.
                    PrimitiveType::F32
                    | PrimitiveType::F64
                    | PrimitiveType::String
                    | PrimitiveType::Boolean
                    | PrimitiveType::Never => true,
                };
                if !in_range {
                    self.errors.push(CompilerError::NumericOverflow {
                        written: v.to_string(),
                        target,
                        span,
                    });
                }
            }
            NumberValue::Float(v) => {
                let fits = match target {
                    PrimitiveType::F64 => v.is_finite(),
                    PrimitiveType::F32 => v.is_finite() && v.abs() <= f64::from(f32::MAX),
                    PrimitiveType::I32
                    | PrimitiveType::I64
                    | PrimitiveType::String
                    | PrimitiveType::Boolean
                    | PrimitiveType::Never => false,
                };
                if !fits {
                    let written = if v.abs() >= 1e16 {
                        format!("{v:e}")
                    } else {
                        v.to_string()
                    };
                    self.errors.push(CompilerError::InvalidNumber {
                        value: format!("{written} does not fit in {target:?}"),
                        span,
                    });
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

        // Fold the elements with the same join inference uses, so the
        // two agree on what the literal's element type is. The first
        // element is not privileged: `[1, nil]` and `[nil, 1]` are both
        // arrays of `I32?`, and neither element names that type alone.
        // The fold reaching `Unknown` is the mismatch.
        let mut joined = first_sem;
        for elem in iter {
            let elem_sem = self.infer_type_sem(elem, file);
            if elem_sem.is_unknown() {
                continue;
            }
            let elem_ty = elem_sem.display();
            // Two known types that join to a type with an unknown part
            // do not agree: `[[1], ["a"]]` joins to `[[Unknown]]`.
            let known = !elem_sem.is_indeterminate() && !joined.is_indeterminate();
            let next = joined.clone().join(elem_sem);
            if next.is_unknown() || (known && next.is_indeterminate()) {
                self.errors.push(CompilerError::TypeMismatch {
                    expected: format!("[{}]", joined.display()),
                    found: format!("element of type {elem_ty}"),
                    span,
                });
                // Stop after the first mismatch so a single typo doesn't
                // cascade into N copies of the same diagnostic.
                break;
            }
            joined = next;
        }
    }
}
