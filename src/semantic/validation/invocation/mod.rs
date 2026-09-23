//! Invocation validation: struct instantiation, function calls (single +
//! overload resolution), closure-binding calls, and the `mod::item` module
//! visibility check used at every qualified call/reference site.

mod functions;
pub(in crate::semantic) mod overloads;
mod structs;

use std::borrow::Cow;

use super::super::module_resolver::ModuleResolver;
use super::super::symbol_table::{FunctionInfo, SymbolTable};
use super::super::SemanticAnalyzer;
use crate::ast::{Expr, File, Ident, Type};
use crate::error::CompilerError;
use crate::location::Span;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// The overloads of the function that a call names.
    ///
    /// The name as written first. A qualified name such as `m::id`
    /// then names `id` in the module `m`, in this file or in an
    /// imported module. The last segment alone was the fallback
    /// before, so `m::id<I32>(...)` found no function (or a top-level
    /// `id` that is another function): the call reported the wrong
    /// number of type arguments, and its arguments had no type check.
    ///
    /// A function of a module names the module's types without the
    /// prefix. The copy returned for one reads them as the caller does,
    /// `m::E`, so its parameter types compare with the caller's.
    pub(in crate::semantic::validation) fn function_overloads(
        &self,
        name: &str,
    ) -> Cow<'_, [FunctionInfo]> {
        let direct = self.symbols.get_function_overloads(name);
        if !direct.is_empty() {
            return Cow::Borrowed(direct);
        }
        let Some((module, last)) = name.rsplit_once("::") else {
            return Cow::Borrowed(direct);
        };
        let segments: Vec<&str> = module.split("::").collect();
        let found = overloads_in(&self.symbols, &segments, last).or_else(|| {
            self.module_cache
                .values()
                .find_map(|(_, symbols)| overloads_in(symbols, &segments, last))
        });
        found.map_or(Cow::Borrowed(direct), |(symbols, overloads)| {
            Cow::Owned(
                overloads
                    .iter()
                    .map(|info| qualify_function(info, symbols, module))
                    .collect(),
            )
        })
    }

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

        let expected = self.invocation_argument_types(&name, type_args, args, file);
        for ((_, arg_expr), arg_expected) in args.iter().zip(expected) {
            self.validate_expr_expecting(arg_expr, arg_expected, file);
        }
        for type_arg in type_args {
            self.validate_type(type_arg, span);
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

/// The overloads of `last` in the module that `segments` names, inside
/// `symbols`, with the symbol table of that module. `None` when the
/// module or the function is not there.
fn overloads_in<'s>(
    symbols: &'s SymbolTable,
    segments: &[&str],
    last: &str,
) -> Option<(&'s SymbolTable, &'s [FunctionInfo])> {
    let mut current = symbols;
    for part in segments {
        current = &current.modules.get(*part)?.symbols;
    }
    let found = current.get_function_overloads(last);
    (!found.is_empty()).then_some((current, found))
}

/// `info` with each type that the module `module` declares written with
/// the module's prefix, as code outside the module names it.
fn qualify_function(info: &FunctionInfo, module: &SymbolTable, prefix: &str) -> FunctionInfo {
    let mut out = info.clone();
    for param in &mut out.params {
        if let Some(ty) = &mut param.ty {
            *ty = qualify_type(ty, module, prefix);
        }
    }
    if let Some(ty) = &mut out.return_type {
        *ty = qualify_type(ty, module, prefix);
    }
    out
}

/// `ty` with each name that `module` declares as a type or a trait
/// written as `prefix::name`.
fn qualify_type(ty: &Type, module: &SymbolTable, prefix: &str) -> Type {
    let qualify = |ident: &Ident| {
        if module.is_type(&ident.name) || module.is_trait(&ident.name) {
            Ident::new(format!("{prefix}::{}", ident.name), ident.span)
        } else {
            ident.clone()
        }
    };
    let inner = |t: &Type| Box::new(qualify_type(t, module, prefix));
    match ty {
        Type::Ident(ident) => Type::Ident(qualify(ident)),
        Type::Generic { name, args, span } => Type::Generic {
            name: qualify(name),
            args: args
                .iter()
                .map(|a| qualify_type(a, module, prefix))
                .collect(),
            span: *span,
        },
        Type::Array(t) => Type::Array(inner(t)),
        Type::Optional(t) => Type::Optional(inner(t)),
        Type::Tuple(fields) => Type::Tuple(
            fields
                .iter()
                .map(|f| {
                    let mut field = f.clone();
                    field.ty = qualify_type(&f.ty, module, prefix);
                    field
                })
                .collect(),
        ),
        Type::Dictionary { key, value } => Type::Dictionary {
            key: inner(key),
            value: inner(value),
        },
        Type::Closure { params, ret } => Type::Closure {
            params: params
                .iter()
                .map(|(c, t)| (*c, qualify_type(t, module, prefix)))
                .collect(),
            ret: inner(ret),
        },
        Type::Primitive(_) => ty.clone(),
    }
}
