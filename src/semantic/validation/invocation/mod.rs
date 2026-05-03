//! Invocation validation: struct instantiation, function calls (single +
//! overload resolution), closure-binding calls, and the `mod::item` module
//! visibility check used at every qualified call/reference site.

mod functions;
mod overloads;
mod structs;

use super::super::module_resolver::ModuleResolver;
use super::super::SemanticAnalyzer;
use crate::ast::{Expr, File};
use crate::error::CompilerError;
use crate::location::Span;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Check module visibility for a multi-segment path (`mod::item`,
    /// `outer::inner::item`, etc.).
    ///
    /// Walks the full module path, checking:
    /// 1. Each intermediate module segment must be `pub` to be accessible
    ///    across module boundaries.
    /// 2. The final item must be `pub` when accessed across any module boundary.
    ///
    /// Returns true if access is allowed, false if a `VisibilityViolation`
    /// was emitted.
    pub(in crate::semantic::validation) fn check_module_visibility(
        &mut self,
        path: &[crate::ast::Ident],
        span: Span,
    ) -> bool {
        let Some((first, rest)) = path.split_first() else {
            return true;
        };
        if rest.is_empty() {
            return true;
        }
        let Some(root_module) = self.symbols.modules.get(first.name.as_str()) else {
            return true;
        };
        // Walk intermediate modules (all rest segments except the last).
        // Each intermediate module must itself be `pub`.
        let mut current = &root_module.symbols;
        let Some((item_ident, middle)) = rest.split_last() else {
            return true;
        };
        for seg in middle {
            let name = seg.name.as_str();
            let Some(next) = current.modules.get(name) else {
                // Unknown module: leave error reporting to the caller.
                return true;
            };
            if matches!(next.visibility, crate::ast::Visibility::Private) {
                self.errors.push(CompilerError::VisibilityViolation {
                    name: name.to_string(),
                    span,
                });
                return false;
            }
            current = &next.symbols;
        }
        // Final segment is the item name
        let item_name = item_ident.name.as_str();
        let item_visibility = current
            .structs
            .get(item_name)
            .map(|s| s.visibility)
            .or_else(|| {
                current
                    .functions
                    .get(item_name)
                    .and_then(|overloads| overloads.first().map(|f| f.visibility))
            })
            .or_else(|| current.enums.get(item_name).map(|e| e.visibility))
            .or_else(|| current.traits.get(item_name).map(|t| t.visibility))
            .or_else(|| current.lets.get(item_name).map(|l| l.visibility))
            .or_else(|| current.modules.get(item_name).map(|m| m.visibility));

        if matches!(item_visibility, Some(crate::ast::Visibility::Private)) {
            self.errors.push(CompilerError::VisibilityViolation {
                name: item_name.to_string(),
                span,
            });
            return false;
        }
        true
    }

    /// Validate an invocation expression (struct instantiation or function call)
    pub(in crate::semantic::validation) fn validate_expr_invocation(
        &mut self,
        path: &[crate::ast::Ident],
        type_args: &[crate::ast::Type],
        args: &[(Option<crate::ast::Ident>, Expr)],
        span: Span,
        file: &File,
    ) {
        let name = path
            .iter()
            .map(|id| id.name.as_str())
            .collect::<Vec<_>>()
            .join("::");

        for (_, arg_expr) in args {
            self.validate_expr(arg_expr, file);
        }
        for type_arg in type_args {
            self.validate_type(type_arg);
        }

        // Check module visibility for qualified paths (mod::item)
        if !self.check_module_visibility(path, span) {
            return;
        }

        let is_struct = self.symbols.get_struct_qualified(&name).is_some();
        if is_struct {
            self.validate_expr_invocation_struct(&name, type_args, args, span, file);
        } else {
            self.validate_expr_invocation_function(&name, type_args, args, span, file);
        }
    }
}
