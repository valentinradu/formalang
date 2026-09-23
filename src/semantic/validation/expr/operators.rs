//! Per-variant validators for binary operators, `for`-loop iteration,
//! `if`-condition typing, and let-pattern destructuring shape checks.

use super::super::super::module_resolver::ModuleResolver;
use super::super::super::sem_type::SemType;
use super::super::super::SemanticAnalyzer;
use crate::ast::{BinaryOperator, BindingPattern, Expr, File};
use crate::error::CompilerError;
use crate::location::Span;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Validate binary operator type compatibility
    pub(super) fn validate_binary_op(
        &mut self,
        left: &Expr,
        op: BinaryOperator,
        right: &Expr,
        span: Span,
        file: &File,
    ) {
        let left_sem = self.infer_type_sem(left, file);
        let right_sem = self.infer_type_sem(right, file);

        // Skip validation when either operand type is unknown (field access, method calls, etc.)
        if left_sem.is_unknown() || right_sem.is_unknown() {
            return;
        }
        let left_type = left_sem.display();
        let right_type = right_sem.display();

        // Numeric primitives accepted for arithmetic/comparison/range;
        // both operands must agree (no implicit width promotion).
        // Backend-specific scalar/vector types are the codegen pass's job.
        let is_numeric = |s: &str| matches!(s, "I32" | "I64" | "F32" | "F64");
        let valid = match op {
            // Add: matched-numeric pair, or String + String (concatenation)
            BinaryOperator::Add => {
                (is_numeric(&left_type) && left_type == right_type)
                    || (left_type == "String" && right_type == "String")
            }
            // Arithmetic and comparison operators: matched-numeric pair
            BinaryOperator::Sub
            | BinaryOperator::Mul
            | BinaryOperator::Div
            | BinaryOperator::Mod
            | BinaryOperator::Lt
            | BinaryOperator::Gt
            | BinaryOperator::Le
            | BinaryOperator::Ge => is_numeric(&left_type) && left_type == right_type,
            // Range: a matched pair of integers. A range counts from
            // the start to the end in steps of one, so a float bound
            // has no meaning. Every consumer models a range as two
            // integers, so accepting `1.5..3.5` here produced a
            // program no backend can run.
            BinaryOperator::Range => {
                matches!(left_type.as_str(), "I32" | "I64") && left_type == right_type
            }
            // Equality operators: the same type, and a type that can
            // be compared.
            //
            // Comparing an optional to `nil` is the exception, and the
            // plainest way to ask whether one holds anything. The two
            // sides have different types by construction — `I32?` and
            // `Nil` — so a same-type rule rejected `x == nil` and left
            // `.is_none()` as the only spelling, without saying so.
            BinaryOperator::Eq | BinaryOperator::Ne => {
                if Self::compares_against_nil(&left_sem, &right_sem) {
                    true
                } else {
                    left_type == right_type && self.is_equatable(&left_sem)
                }
            }
            // Logical operators: Boolean + Boolean
            BinaryOperator::And | BinaryOperator::Or => {
                left_type == "Boolean" && right_type == "Boolean"
            }
        };

        if !valid {
            self.errors.push(CompilerError::InvalidBinaryOp {
                op: format!("{op:?}"),
                left_type,
                right_type,
                span,
            });
        }
    }

    /// Whether this comparison is an optional against `nil`.
    ///
    /// Either side may be the `nil`. Two `nil`s compare too: both are
    /// empty, so the answer is true, and refusing to say so would be
    /// a special case with nothing behind it.
    fn compares_against_nil(left: &SemType, right: &SemType) -> bool {
        let optional_or_nil = |ty: &SemType| matches!(ty, SemType::Optional(_) | SemType::Nil);
        (matches!(left, SemType::Nil) && optional_or_nil(right))
            || (matches!(right, SemType::Nil) && optional_or_nil(left))
    }

    /// Whether `==` and `!=` have an answer for this type.
    ///
    /// Equality is structural, so it reaches every field of a struct
    /// and every element of a container. A closure has no structure to
    /// compare, so a type that holds one anywhere is not equatable.
    /// Unresolved types stay equatable: an unknown type is reported
    /// elsewhere, and a second diagnostic for it only adds noise.
    fn is_equatable(&self, ty: &SemType) -> bool {
        let mut seen = Vec::new();
        !self.holds_a_closure(ty, &mut seen)
    }

    /// The recursive half of [`Self::is_equatable`].
    ///
    /// `seen` holds the struct names already on the stack, so a struct
    /// that contains itself through an optional or an array terminates
    /// instead of recursing without end.
    fn holds_a_closure(&self, ty: &SemType, seen: &mut Vec<String>) -> bool {
        if ty.holds_a_closure() {
            return true;
        }

        // `SemType::holds_a_closure` stops at a name because it has no
        // symbol table. Look the name up and walk the fields.
        let mut found = false;
        Self::for_each_inner_type(ty, &mut |inner| {
            if let SemType::Named(name) = inner {
                if seen.iter().any(|s| s == name) {
                    return;
                }
                let fields: Vec<SemType> = if let Some(info) = self.symbols.get_struct(name) {
                    info.fields
                        .iter()
                        .map(|f| SemType::from_ast(&f.ty))
                        .collect()
                } else if let Some(info) = self.symbols.get_enum_qualified(name) {
                    // An enum holds a closure when one of its
                    // variants carries one in a payload field.
                    info.variant_fields
                        .values()
                        .flatten()
                        .map(|f| SemType::from_ast(&f.ty))
                        .collect()
                } else {
                    return;
                };
                seen.push(name.clone());
                for field in &fields {
                    if self.holds_a_closure(field, seen) {
                        found = true;
                    }
                }
                seen.pop();
            }
        });
        found
    }

    /// Call `visit` on this type and on every type nested inside it.
    fn for_each_inner_type(ty: &SemType, visit: &mut impl FnMut(&SemType)) {
        visit(ty);
        match ty {
            SemType::Array(inner) | SemType::Optional(inner) => {
                Self::for_each_inner_type(inner, visit);
            }
            SemType::Dictionary { key, value } => {
                Self::for_each_inner_type(key, visit);
                Self::for_each_inner_type(value, visit);
            }
            SemType::Tuple(fields) => {
                for (_, field) in fields {
                    Self::for_each_inner_type(field, visit);
                }
            }
            SemType::Generic { args, .. } => {
                for arg in args {
                    Self::for_each_inner_type(arg, visit);
                }
            }
            SemType::Closure { params, return_ty } => {
                for (_, param) in params {
                    Self::for_each_inner_type(param, visit);
                }
                Self::for_each_inner_type(return_ty, visit);
            }
            SemType::Primitive(_)
            | SemType::Named(_)
            | SemType::Unknown
            | SemType::InferredEnum
            | SemType::Nil => {}
        }
    }

    /// Validate for loop collection is an array or range
    pub(super) fn validate_for_loop(&mut self, collection: &Expr, span: Span, file: &File) {
        let collection_sem = self.infer_type_sem(collection, file);

        let is_iterable = matches!(collection_sem, SemType::Array(_) | SemType::Unknown)
            || matches!(&collection_sem, SemType::Generic { base, .. } if base == "Range" || base == "Seq");

        if !is_iterable {
            self.errors.push(CompilerError::ForLoopNotArray {
                actual: collection_sem.display(),
                span,
            });
        }
    }

    /// Validate destructuring pattern matches the value type
    pub(in crate::semantic::validation) fn validate_destructuring_pattern(
        &mut self,
        pattern: &BindingPattern,
        value: &Expr,
        span: Span,
        file: &File,
    ) {
        let value_sem = self.infer_type_sem(value, file);

        // Skip destructuring validation when value type is unknown (field access, etc.)
        if value_sem.is_unknown() {
            return;
        }

        match pattern {
            BindingPattern::Array { elements, .. } => {
                // Array destructuring requires an array type
                if !matches!(value_sem, SemType::Array(_)) {
                    self.errors.push(CompilerError::ArrayDestructuringNotArray {
                        actual: value_sem.display(),
                        span,
                    });
                } else if let Expr::Array {
                    elements: literal_elems,
                    ..
                } = value
                {
                    // Known array length: pattern must not demand more fixed
                    // elements than the array provides. Partial patterns that
                    // cover fewer positions than the array (e.g.,
                    // `let [a, b] = [1, 2, 3]`) are permitted — extra values
                    // are simply unbound. A rest element accepts any tail.
                    let pattern_fixed = elements
                        .iter()
                        .filter(|e| !matches!(e, crate::ast::ArrayPatternElement::Rest(_)))
                        .count();
                    let actual = literal_elems.len();
                    if pattern_fixed > actual {
                        self.errors.push(CompilerError::TypeMismatch {
                            expected: format!("array with at least {pattern_fixed} element(s)"),
                            found: format!("array with {actual} element(s)"),
                            span,
                        });
                    }
                }
            }
            BindingPattern::Struct { fields, .. } => {
                // Struct destructuring requires a struct type.
                // The type may also be `Generic { base, .. }` for instantiated
                // generic structs — strip args for the lookup.
                let lookup_name = match &value_sem {
                    SemType::Generic { base, .. } | SemType::Named(base) => Some(base.as_str()),
                    SemType::Primitive(_)
                    | SemType::Array(_)
                    | SemType::Optional(_)
                    | SemType::Tuple(_)
                    | SemType::Dictionary { .. }
                    | SemType::Closure { .. }
                    | SemType::Unknown
                    | SemType::InferredEnum
                    | SemType::Nil => None,
                };
                if let Some(struct_info) = lookup_name.and_then(|n| self.symbols.get_struct(n)) {
                    let field_names: Vec<&str> =
                        struct_info.fields.iter().map(|f| f.name.as_str()).collect();
                    for field in fields {
                        if !field_names.contains(&field.name.name.as_str()) {
                            self.errors.push(CompilerError::UnknownField {
                                field: field.name.name.clone(),
                                type_name: value_sem.display(),
                                span: field.name.span,
                            });
                        }
                    }
                } else {
                    // Not a known struct - report error (includes primitives)
                    self.errors
                        .push(CompilerError::StructDestructuringNotStruct {
                            actual: value_sem.display(),
                            span,
                        });
                }
            }
            BindingPattern::Tuple { elements, .. } => {
                // Validate tuple pattern arity against tuple type "(x: T, y: U, ...)"
                if let SemType::Tuple(fields) = &value_sem {
                    let field_count = fields.len();
                    let pattern_count = elements.len();
                    if pattern_count > field_count && field_count > 0 {
                        self.errors.push(CompilerError::TypeMismatch {
                            expected: format!("tuple with {field_count} field(s)"),
                            found: value_sem.display(),
                            span,
                        });
                    }
                }
            }
            BindingPattern::Simple(_) => {
                // Simple patterns don't require type validation here
            }
        }
    }

    /// Validate if condition is boolean or optional
    pub(super) fn validate_if_condition(&mut self, condition: &Expr, span: Span, file: &File) {
        use crate::ast::PrimitiveType;
        let condition_sem = self.infer_type_sem(condition, file);

        // Skip when type is unknown (field access, method calls — IR lowering handles these)
        if condition_sem.is_unknown() {
            return;
        }

        // Condition must be Boolean or optional
        let is_valid = matches!(
            condition_sem,
            SemType::Primitive(PrimitiveType::Boolean) | SemType::Optional(_)
        );
        if !is_valid {
            self.errors.push(CompilerError::InvalidIfCondition {
                actual: condition_sem.display(),
                span,
            });
        }
    }
}
