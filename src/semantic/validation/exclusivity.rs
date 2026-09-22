//! Exclusive access at a call site.
//!
//! The language uses Mutable Value Semantics. A `mut` argument and a
//! `sink` argument each need sole access to the value for the whole
//! call: the callee may change the first and takes the second away. A
//! default argument goes by pointer too, so it sees any change the
//! callee makes through a `mut` argument that reaches the same value.
//!
//! So two arguments of one call must not reach the same place when one
//! of them is `mut` or `sink`. `swap(a: x, b: x)` breaks the rule, and
//! so does `f(a: p, b: p.x)`. The check is field-sensitive:
//! `f(a: p.x, b: p.y)` reaches two different places, and it is legal.
//!
//! The receiver of a method counts as an argument, with the convention
//! of its `self` parameter.

use super::super::module_resolver::ModuleResolver;
use super::super::SemanticAnalyzer;
use crate::ast::{Expr, ParamConvention};
use crate::error::CompilerError;
use crate::location::Span;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Report each pair of arguments that reach the same place when one
    /// of the two is `mut` or `sink`.
    ///
    /// `accesses` holds every argument of one call, the receiver too,
    /// each with the convention of the parameter it fills. An argument
    /// that fills no known parameter goes in as `Let`.
    pub(super) fn validate_exclusive_access(
        &mut self,
        accesses: &[(ParamConvention, &Expr)],
        span: Span,
    ) {
        let paths: Vec<(ParamConvention, Vec<String>)> = accesses
            .iter()
            .filter_map(|(convention, expr)| {
                Self::access_path(expr).map(|path| (*convention, path))
            })
            .collect();

        let mut reported: Vec<Vec<String>> = Vec::new();
        for (i, (first_convention, first_path)) in paths.iter().enumerate() {
            for (second_convention, second_path) in paths.iter().skip(i.saturating_add(1)) {
                let exclusive =
                    Self::is_exclusive(*first_convention) || Self::is_exclusive(*second_convention);
                if !exclusive {
                    continue;
                }
                let Some(shared) = Self::overlap(first_path, second_path) else {
                    continue;
                };
                if reported.contains(&shared) {
                    continue;
                }
                self.errors.push(CompilerError::OverlappingArguments {
                    path: shared.join("."),
                    span,
                });
                reported.push(shared);
            }
        }
    }

    /// Whether a convention needs sole access to its argument.
    const fn is_exclusive(convention: ParamConvention) -> bool {
        match convention {
            ParamConvention::Mut | ParamConvention::Sink => true,
            ParamConvention::Let => false,
        }
    }

    /// The shorter of two paths when one is a prefix of the other.
    ///
    /// `p` and `p.x` overlap in `p`. `p.x` and `p.x.z` overlap in
    /// `p.x`. `p.x` and `p.y` share a root but no place, so they do not
    /// overlap.
    fn overlap(first: &[String], second: &[String]) -> Option<Vec<String>> {
        let (shorter, longer) = if first.len() <= second.len() {
            (first, second)
        } else {
            (second, first)
        };
        longer.starts_with(shorter).then(|| shorter.to_vec())
    }

    /// The place an argument reaches: its root binding, then each field
    /// on the way down.
    ///
    /// `None` means the argument is a fresh value — a literal, a call,
    /// an operator — so it shares a place with nothing. An index reads
    /// from inside its container, so `xs[0]` reaches `xs`.
    fn access_path(expr: &Expr) -> Option<Vec<String>> {
        match expr {
            Expr::Reference { path, .. } => {
                if path.is_empty() {
                    None
                } else {
                    Some(path.iter().map(|segment| segment.name.clone()).collect())
                }
            }
            Expr::FieldAccess { object, field, .. } => {
                let mut path = Self::access_path(object)?;
                path.push(field.name.clone());
                Some(path)
            }
            Expr::Group { expr, .. } => Self::access_path(expr),
            Expr::DictAccess { dict, .. } => Self::access_path(dict),
            Expr::Literal { .. }
            | Expr::Invocation { .. }
            | Expr::EnumInstantiation { .. }
            | Expr::InferredEnumInstantiation { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::DictLiteral { .. }
            | Expr::ClosureExpr { .. }
            | Expr::LetExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Block { .. } => None,
        }
    }
}
