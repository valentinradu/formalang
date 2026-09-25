//! The shape of the arguments of a call: the labels and the count.
//!
//! A call gives each argument by position or by label. A position fills
//! the parameter at that position. A label names a parameter by its
//! external label or by its name. The call must give each parameter
//! that has no default, it must not give a parameter twice, and each
//! label must name a parameter.

use super::super::super::module_resolver::ModuleResolver;
use super::super::super::SemanticAnalyzer;
use super::overloads::ParamView;
use crate::ast::{Expr, Ident};
use crate::error::CompilerError;
use crate::location::Span;
use std::collections::HashSet;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Check the labels and the count of `args` against `params`.
    /// `callee` names the function in a message, for example
    /// `Function 'f'`. `name` is the bare name of the function.
    ///
    /// Return false when the shape is wrong. The check reports the
    /// first problem only, because the later ones follow from it.
    pub(in crate::semantic) fn validate_call_shape(
        &mut self,
        callee: &str,
        name: &str,
        params: &[ParamView<'_>],
        args: &[(Option<Ident>, Expr)],
        span: Span,
    ) -> bool {
        let non_self: Vec<&ParamView<'_>> = params.iter().filter(|p| p.name != "self").collect();

        if !self.check_repeated_labels(args) {
            return false;
        }

        // Too many arguments is the first problem, whatever the labels.
        if args.len() > non_self.len() {
            self.errors.push(CompilerError::ArgumentCountMismatch {
                callee: callee.to_string(),
                expected: non_self.len(),
                actual: args.len(),
                span,
            });
            return false;
        }

        // A position fills the parameter at that position, and a label
        // fills the parameter that it names.
        let mut filled: HashSet<usize> = HashSet::new();
        for (position, (label, _)) in args.iter().enumerate() {
            let index = match label {
                None => position,
                Some(label) => {
                    let Some(index) = non_self.iter().position(|p| {
                        p.external_label == Some(label.name.as_str()) || p.name == label.name
                    }) else {
                        self.errors.push(CompilerError::NoMatchingOverload {
                            function: name.to_string(),
                            span,
                        });
                        return false;
                    };
                    index
                }
            };
            if index >= non_self.len() {
                self.errors.push(CompilerError::ArgumentCountMismatch {
                    callee: callee.to_string(),
                    expected: non_self.len(),
                    actual: args.len(),
                    span,
                });
                return false;
            }
            if !filled.insert(index) {
                let param = non_self.get(index).map_or("", |p| p.name);
                self.errors.push(CompilerError::DuplicateDefinition {
                    name: format!("argument '{param}'"),
                    span,
                });
                return false;
            }
        }

        // Each parameter without a default needs an argument.
        let required = non_self.iter().filter(|p| !p.has_default).count();
        let missing = non_self
            .iter()
            .enumerate()
            .any(|(index, p)| !p.has_default && !filled.contains(&index));
        if missing {
            self.errors.push(CompilerError::ArgumentCountMismatch {
                callee: callee.to_string(),
                expected: required,
                actual: args.len(),
                span,
            });
            return false;
        }
        true
    }

    /// A label may appear once in a call. Return false, and report
    /// the label, when one appears twice.
    pub(in crate::semantic) fn check_repeated_labels(
        &mut self,
        args: &[(Option<Ident>, Expr)],
    ) -> bool {
        let mut labels_seen: HashSet<&str> = HashSet::new();
        for label in args.iter().filter_map(|(label, _)| label.as_ref()) {
            if !labels_seen.insert(label.name.as_str()) {
                self.errors.push(CompilerError::DuplicateDefinition {
                    name: format!("argument '{}'", label.name),
                    span: label.span,
                });
                return false;
            }
        }
        true
    }
}
