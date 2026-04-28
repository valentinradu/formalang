//! Free helpers for traversing qualified type names (`m1::m2::Foo`).
//!
//! Used by `method_exists_on_type` and the qualified-type fallback in
//! `validate_expr_invocation` to resolve impl methods that live inside
//! nested `module { ... }` definitions or imported-module ASTs.

use crate::ast::{Definition, Statement};

/// Split a qualified type name `m1::m2::Foo` into module
/// segments `["m1", "m2"]` and bare name `"Foo"`. Returns `None` if the
/// name has no `::`.
pub(super) fn split_qualified_type(name: &str) -> Option<(Vec<&str>, &str)> {
    if !name.contains("::") {
        return None;
    }
    let mut parts: Vec<&str> = name.split("::").collect();
    let last = parts.pop()?;
    Some((parts, last))
}

/// Walk a slice of `Statement`s looking for the nested
/// module path `segments`, returning that module's `definitions`.
/// Recurses into nested `Definition::Module` matches.
pub(super) fn find_nested_module_definitions<'a>(
    statements: &'a [Statement],
    segments: &[&str],
) -> Option<&'a [Definition]> {
    let (head, rest) = segments.split_first()?;
    for stmt in statements {
        if let Statement::Definition(def) = stmt {
            if let Definition::Module(module_def) = &**def {
                if module_def.name.name == *head {
                    return if rest.is_empty() {
                        Some(&module_def.definitions)
                    } else {
                        find_nested_module_definitions_in_defs(&module_def.definitions, rest)
                    };
                }
            }
        }
    }
    None
}

fn find_nested_module_definitions_in_defs<'a>(
    definitions: &'a [Definition],
    segments: &[&str],
) -> Option<&'a [Definition]> {
    let (head, rest) = segments.split_first()?;
    for def in definitions {
        if let Definition::Module(module_def) = def {
            if module_def.name.name == *head {
                return if rest.is_empty() {
                    Some(&module_def.definitions)
                } else {
                    find_nested_module_definitions_in_defs(&module_def.definitions, rest)
                };
            }
        }
    }
    None
}

/// Scan a slice of definitions for `impl <bare> { fn <method> }`.
pub(super) fn impl_method_in_definitions(
    definitions: &[Definition],
    bare: &str,
    method: &str,
) -> bool {
    for def in definitions {
        if let Definition::Impl(impl_def) = def {
            if impl_def.name.name == bare {
                for func in &impl_def.functions {
                    if func.name.name == method {
                        return true;
                    }
                }
            }
        }
    }
    false
}
