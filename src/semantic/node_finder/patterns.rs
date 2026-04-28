//! Walkers for binding patterns inside `let` statements.

use super::{NodeAtPosition, NodeFinder};
use crate::ast::{ArrayPatternElement, BindingPattern};
use crate::semantic::position::span_contains_offset;

impl<'ast> NodeFinder<'ast> {
    /// Visit a binding pattern (for destructuring support)
    pub(super) fn visit_binding_pattern(&mut self, pattern: &'ast BindingPattern) {
        match pattern {
            BindingPattern::Simple(ident) => {
                if span_contains_offset(&ident.span, self.offset) {
                    self.found_node = Some(NodeAtPosition::Identifier(ident));
                }
            }
            BindingPattern::Array { elements, .. } => {
                for element in elements {
                    self.visit_array_pattern_element(element);
                    if self.found_node.is_some() {
                        return;
                    }
                }
            }
            BindingPattern::Struct { fields, .. } => {
                for field in fields {
                    if span_contains_offset(&field.name.span, self.offset) {
                        self.found_node = Some(NodeAtPosition::Identifier(&field.name));
                        return;
                    }
                    // Check alias if present
                    if let Some(alias) = &field.alias {
                        if span_contains_offset(&alias.span, self.offset) {
                            self.found_node = Some(NodeAtPosition::Identifier(alias));
                            return;
                        }
                    }
                }
            }
            BindingPattern::Tuple { elements, .. } => {
                for element in elements {
                    self.visit_binding_pattern(element);
                    if self.found_node.is_some() {
                        return;
                    }
                }
            }
        }
    }

    /// Visit an array pattern element
    fn visit_array_pattern_element(&mut self, element: &'ast ArrayPatternElement) {
        match element {
            ArrayPatternElement::Binding(pattern) => {
                self.visit_binding_pattern(pattern);
            }
            ArrayPatternElement::Rest(Some(ident)) => {
                if span_contains_offset(&ident.span, self.offset) {
                    self.found_node = Some(NodeAtPosition::Identifier(ident));
                }
            }
            ArrayPatternElement::Rest(None) | ArrayPatternElement::Wildcard => {
                // No identifier to check
            }
        }
    }
}
