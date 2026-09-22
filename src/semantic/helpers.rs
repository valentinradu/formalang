//! Free helper functions used throughout the semantic analyzer.
//!
//! These functions have no dependency on `SemanticAnalyzer` state and are
//! split out so the orchestrator module stays focused on pass dispatch.

use crate::ast::{ArrayPatternElement, BindingPattern};
use crate::location::Span;

/// Represents a binding extracted from a pattern
#[derive(Debug, Clone)]
pub(crate) struct PatternBinding {
    pub(crate) name: String,
    pub(crate) span: Span,
}

/// Collect all binding names from a pattern recursively
pub(crate) fn collect_bindings_from_pattern(pattern: &BindingPattern) -> Vec<PatternBinding> {
    let mut bindings = Vec::new();
    collect_bindings_recursive(pattern, &mut bindings);
    bindings
}

/// Return true if `name` is the name of a built-in primitive type.
///
/// Primitive names are not lexer keywords — they parse as regular identifiers
/// and are mapped to `Type::Primitive` at type position by the parser. User
/// definitions that reuse these names must be rejected here with
/// `PrimitiveRedefinition` rather than silently shadowing the built-in.
pub(crate) fn is_primitive_name(name: &str) -> bool {
    matches!(
        name,
        "String" | "I32" | "I64" | "F32" | "F64" | "Boolean" | "Never"
    )
}

/// If `ty` is `[T]`, return `T`. Returns `None` for non-array shapes
/// and for dictionary types `[K: V]`.
///
/// previously used `!ty.contains(':')` to reject dict types,
/// which mis-classified `[[K: V]]` (an array of dicts) as a dict because
/// the colon lives inside a nested type. Now walks at depth 0 only, so
/// only a top-level `:` disqualifies the input as an array.
pub(crate) fn strip_array_type(ty: &str) -> Option<&str> {
    let trimmed = ty.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))?;
    if has_depth_zero_colon(inner) {
        None
    } else {
        Some(inner.trim())
    }
}

/// Return true if `s` contains a `:` at bracket-depth 0 (i.e. not nested
/// inside `( ) [ ] < >`).
pub(crate) fn has_depth_zero_colon(s: &str) -> bool {
    depth_zero_colon_index(s).is_some()
}

/// Find the byte index of the first `:` in `s` at bracket-depth 0.
/// Tracks `( ) [ ] < >` symmetrically.
pub(crate) fn depth_zero_colon_index(s: &str) -> Option<usize> {
    let mut depth: u32 = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' | '[' | '<' => depth = depth.saturating_add(1),
            ')' | ']' | '>' => depth = depth.saturating_sub(1),
            ':' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

fn collect_bindings_recursive(pattern: &BindingPattern, bindings: &mut Vec<PatternBinding>) {
    match pattern {
        BindingPattern::Simple(ident) => {
            bindings.push(PatternBinding {
                name: ident.name.clone(),
                span: ident.span,
            });
        }
        BindingPattern::Array { elements, .. } => {
            for element in elements {
                match element {
                    ArrayPatternElement::Binding(inner) => {
                        collect_bindings_recursive(inner, bindings);
                    }
                    ArrayPatternElement::Rest(Some(ident)) => {
                        bindings.push(PatternBinding {
                            name: ident.name.clone(),
                            span: ident.span,
                        });
                    }
                    ArrayPatternElement::Rest(None) | ArrayPatternElement::Wildcard => {
                        // No binding for anonymous rest or wildcard
                    }
                }
            }
        }
        BindingPattern::Struct { fields, .. } => {
            for field in fields {
                // Use alias if present, otherwise use field name
                let binding_ident = field.alias.as_ref().unwrap_or(&field.name);
                bindings.push(PatternBinding {
                    name: binding_ident.name.clone(),
                    span: binding_ident.span,
                });
            }
        }
        BindingPattern::Tuple { elements, .. } => {
            for element in elements {
                collect_bindings_recursive(element, bindings);
            }
        }
    }
}
