//! AST node finder for position-based queries
//!
//! This module provides utilities for finding AST nodes at a given position in the source code.
//! Used for LSP features like hover, go-to-definition, completion, etc.

mod defs;
mod exprs;
mod patterns;
#[cfg(test)]
mod tests;

use super::position::span_contains_offset;
use crate::ast::{
    EnumDef, EnumVariant, Expr, FieldDef, File, FnDef, FnParam, FunctionDef, Ident, ImplDef,
    LetBinding, ModuleDef, Statement, StructDef, StructField, TraitDef, Type, UseItems, UseStmt,
};

/// Result of finding a node at a position
#[expect(
    clippy::exhaustive_enums,
    reason = "matched exhaustively by consumer code"
)]
#[derive(Debug, Clone)]
pub enum NodeAtPosition<'ast> {
    /// Top-level file (no specific node found)
    File,

    /// Use statement
    UseStatement(&'ast UseStmt),

    /// Let binding
    LetBinding(&'ast LetBinding),

    /// Trait definition
    TraitDef(&'ast TraitDef),

    /// Struct definition (unified model/view)
    StructDef(&'ast StructDef),

    /// Enum definition
    EnumDef(&'ast EnumDef),

    /// Enum variant
    EnumVariant(&'ast EnumVariant),

    /// Field definition (in trait)
    FieldDef(&'ast FieldDef),

    /// Struct field (unified)
    StructField(&'ast StructField),

    /// Mount field (also `StructField`)
    MountField(&'ast StructField),

    /// Type reference
    Type(&'ast Type),

    /// Expression
    Expression(&'ast Expr),

    /// Identifier
    Identifier(&'ast Ident),

    /// Impl block definition
    ImplDef(&'ast ImplDef),

    /// Module definition
    ModuleDef(&'ast ModuleDef),

    /// Standalone function definition
    FunctionDef(&'ast FunctionDef),

    /// Function definition inside impl block
    FnDef(&'ast FnDef),

    /// Function parameter
    FunctionParam(&'ast FnParam),

    /// No node found at position
    None,
}

/// Context information about where a position is in the AST
#[expect(clippy::exhaustive_structs, reason = "public API type")]
#[derive(Debug, Clone)]
pub struct PositionContext<'ast> {
    /// The most specific node at the position
    pub node: NodeAtPosition<'ast>,

    /// Parent nodes from innermost to outermost
    pub parents: Vec<NodeAtPosition<'ast>>,

    /// The byte offset of the position
    pub offset: usize,
}

impl<'ast> PositionContext<'ast> {
    /// Find the enclosing definition (trait, struct, enum, impl, module, function)
    #[must_use]
    pub fn enclosing_definition(&self) -> Option<&NodeAtPosition<'ast>> {
        self.parents.iter().find(|node| {
            matches!(
                node,
                NodeAtPosition::TraitDef(_)
                    | NodeAtPosition::StructDef(_)
                    | NodeAtPosition::EnumDef(_)
                    | NodeAtPosition::ImplDef(_)
                    | NodeAtPosition::ModuleDef(_)
                    | NodeAtPosition::FunctionDef(_)
                    | NodeAtPosition::FnDef(_)
            )
        })
    }

    /// Check if we're inside an expression context
    #[must_use]
    pub fn is_in_expression(&self) -> bool {
        matches!(self.node, NodeAtPosition::Expression(_))
            || self
                .parents
                .iter()
                .any(|n| matches!(n, NodeAtPosition::Expression(_)))
    }

    /// Check if we're in a type position
    #[must_use]
    pub const fn is_in_type_position(&self) -> bool {
        matches!(self.node, NodeAtPosition::Type(_))
    }
}

/// Find the node at a given byte offset in the file
#[must_use]
pub fn find_node_at_offset(file: &File, offset: usize) -> PositionContext<'_> {
    let mut finder = NodeFinder {
        offset,
        parents: Vec::new(),
        found_node: None,
    };

    finder.visit_file(file);

    PositionContext {
        node: finder.found_node.unwrap_or(NodeAtPosition::File),
        parents: finder.parents,
        offset,
    }
}

/// Internal node finder visitor
struct NodeFinder<'ast> {
    offset: usize,
    parents: Vec<NodeAtPosition<'ast>>,
    found_node: Option<NodeAtPosition<'ast>>,
}

impl<'ast> NodeFinder<'ast> {
    /// Visit a file
    fn visit_file(&mut self, file: &'ast File) {
        if !span_contains_offset(&file.span, self.offset) {
            return;
        }

        for statement in &file.statements {
            self.visit_statement(statement);
            if self.found_node.is_some() {
                return;
            }
        }
    }

    /// Visit a statement
    fn visit_statement(&mut self, statement: &'ast Statement) {
        match statement {
            Statement::Use(use_stmt) => {
                if span_contains_offset(&use_stmt.span, self.offset) {
                    self.found_node = Some(NodeAtPosition::UseStatement(use_stmt));
                    self.visit_use_stmt(use_stmt);
                }
            }
            Statement::Let(let_binding) => {
                if span_contains_offset(&let_binding.span, self.offset) {
                    self.parents.push(NodeAtPosition::LetBinding(let_binding));
                    self.visit_let_binding(let_binding);
                    // Don't pop if we found the node within this let binding
                    if self.found_node.is_none() {
                        self.parents.pop();
                    }
                }
            }
            Statement::Definition(definition) => {
                self.visit_definition(definition.as_ref());
            }
        }
    }

    /// Visit a use statement
    fn visit_use_stmt(&mut self, use_stmt: &'ast UseStmt) {
        // Check path identifiers
        for ident in &use_stmt.path {
            if span_contains_offset(&ident.span, self.offset) {
                self.found_node = Some(NodeAtPosition::Identifier(ident));
                return;
            }
        }

        // Check imported items
        match &use_stmt.items {
            UseItems::Single(ident) => {
                if span_contains_offset(&ident.span, self.offset) {
                    self.found_node = Some(NodeAtPosition::Identifier(ident));
                }
            }
            UseItems::Multiple(idents) => {
                for ident in idents {
                    if span_contains_offset(&ident.span, self.offset) {
                        self.found_node = Some(NodeAtPosition::Identifier(ident));
                        return;
                    }
                }
            }
            UseItems::Glob => {} // No identifiers to check for glob imports
        }
    }

    /// Visit a let binding
    fn visit_let_binding(&mut self, let_binding: &'ast LetBinding) {
        // Check pattern
        self.visit_binding_pattern(&let_binding.pattern);
        if self.found_node.is_some() {
            return;
        }

        // Check value expression
        self.visit_expr(&let_binding.value);
    }
}
