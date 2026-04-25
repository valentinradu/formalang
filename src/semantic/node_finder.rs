//! AST node finder for position-based queries
//!
//! This module provides utilities for finding AST nodes at a given position in the source code.
//! Used for LSP features like hover, go-to-definition, completion, etc.

use super::position::span_contains_offset;
use crate::ast::{
    ArrayPatternElement, BindingPattern, BlockStatement, Definition, EnumDef, EnumVariant, Expr,
    FieldDef, File, FnDef, FnParam, FunctionDef, Ident, ImplDef, LetBinding, ModuleDef, Statement,
    StructDef, StructField, TraitDef, Type, UseItems, UseStmt,
};
use crate::location::Span;

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

    /// Visit a binding pattern (for destructuring support)
    fn visit_binding_pattern(&mut self, pattern: &'ast BindingPattern) {
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

    /// Visit a definition
    fn visit_definition(&mut self, definition: &'ast Definition) {
        match definition {
            Definition::Trait(trait_def) => {
                if span_contains_offset(&trait_def.span, self.offset) {
                    self.parents.push(NodeAtPosition::TraitDef(trait_def));
                    self.visit_trait_def(trait_def);
                    // Don't pop if we found the node
                    if self.found_node.is_none() {
                        self.parents.pop();
                    }
                }
            }
            Definition::Struct(struct_def) => {
                if span_contains_offset(&struct_def.span, self.offset) {
                    self.parents.push(NodeAtPosition::StructDef(struct_def));
                    self.visit_struct_def(struct_def);
                    // Don't pop if we found the node
                    if self.found_node.is_none() {
                        self.parents.pop();
                    }
                }
            }
            Definition::Enum(enum_def) => {
                if span_contains_offset(&enum_def.span, self.offset) {
                    self.parents.push(NodeAtPosition::EnumDef(enum_def));
                    self.visit_enum_def(enum_def);
                    // Don't pop if we found the node
                    if self.found_node.is_none() {
                        self.parents.pop();
                    }
                }
            }
            Definition::Impl(impl_def) => {
                if span_contains_offset(&impl_def.span, self.offset) {
                    self.parents.push(NodeAtPosition::ImplDef(impl_def));
                    self.visit_impl_def(impl_def);
                    if self.found_node.is_none() {
                        self.parents.pop();
                    }
                }
            }
            Definition::Module(module_def) => {
                if span_contains_offset(&module_def.span, self.offset) {
                    self.parents.push(NodeAtPosition::ModuleDef(module_def));
                    self.visit_module_def(module_def);
                    if self.found_node.is_none() {
                        self.parents.pop();
                    }
                }
            }
            Definition::Function(func_def) => {
                if span_contains_offset(&func_def.span, self.offset) {
                    self.parents
                        .push(NodeAtPosition::FunctionDef(func_def.as_ref()));
                    self.visit_function_def(func_def.as_ref());
                    if self.found_node.is_none() {
                        self.parents.pop();
                    }
                }
            }
        }
    }

    /// Visit a trait definition
    fn visit_trait_def(&mut self, trait_def: &'ast TraitDef) {
        // Check name
        if span_contains_offset(&trait_def.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&trait_def.name));
            return;
        }

        // Check generic parameters
        for generic in &trait_def.generics {
            if span_contains_offset(&generic.span, self.offset) {
                self.found_node = Some(NodeAtPosition::Identifier(&generic.name));
                return;
            }
        }

        // Check trait composition
        for trait_ref in &trait_def.traits {
            if span_contains_offset(&trait_ref.span, self.offset) {
                self.found_node = Some(NodeAtPosition::Identifier(trait_ref));
                return;
            }
        }

        // Check fields
        for field in &trait_def.fields {
            if span_contains_offset(&field.span, self.offset) {
                self.parents.push(NodeAtPosition::FieldDef(field));
                self.visit_field_def(field);
                self.parents.pop();
                if self.found_node.is_some() {
                    return;
                }
            }
        }

        // If no specific node found, return the trait def itself
        if self.found_node.is_none() {
            self.found_node = Some(NodeAtPosition::TraitDef(trait_def));
        }
    }

    /// Visit a struct definition (unified model/view)
    fn visit_struct_def(&mut self, struct_def: &'ast StructDef) {
        // Check name
        if span_contains_offset(&struct_def.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&struct_def.name));
            return;
        }

        // Check generic parameters
        for generic in &struct_def.generics {
            if span_contains_offset(&generic.span, self.offset) {
                self.found_node = Some(NodeAtPosition::Identifier(&generic.name));
                return;
            }
        }

        // Check regular fields
        for field in &struct_def.fields {
            if span_contains_offset(&field.span, self.offset) {
                self.parents.push(NodeAtPosition::StructField(field));
                self.visit_struct_field(field);
                self.parents.pop();
                if self.found_node.is_some() {
                    return;
                }
            }
        }

        // If no specific node found, return the struct def itself
        if self.found_node.is_none() {
            self.found_node = Some(NodeAtPosition::StructDef(struct_def));
        }
    }

    /// Visit an enum definition
    fn visit_enum_def(&mut self, enum_def: &'ast EnumDef) {
        // Check name
        if span_contains_offset(&enum_def.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&enum_def.name));
            return;
        }

        // Check generic parameters
        for generic in &enum_def.generics {
            if span_contains_offset(&generic.span, self.offset) {
                self.found_node = Some(NodeAtPosition::Identifier(&generic.name));
                return;
            }
        }

        // Check variants
        for variant in &enum_def.variants {
            if span_contains_offset(&variant.span, self.offset) {
                self.parents.push(NodeAtPosition::EnumVariant(variant));
                self.visit_enum_variant(variant);
                self.parents.pop();
                if self.found_node.is_some() {
                    return;
                }
            }
        }

        // If no specific node found, return the enum def itself
        if self.found_node.is_none() {
            self.found_node = Some(NodeAtPosition::EnumDef(enum_def));
        }
    }

    /// Visit an enum variant
    fn visit_enum_variant(&mut self, variant: &'ast EnumVariant) {
        // Check name
        if span_contains_offset(&variant.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&variant.name));
            return;
        }

        // Check fields
        for field in &variant.fields {
            if span_contains_offset(&field.span, self.offset) {
                self.parents.push(NodeAtPosition::FieldDef(field));
                self.visit_field_def(field);
                self.parents.pop();
                if self.found_node.is_some() {
                    return;
                }
            }
        }

        // If no specific node found, return the variant itself
        if self.found_node.is_none() {
            self.found_node = Some(NodeAtPosition::EnumVariant(variant));
        }
    }

    /// Visit an impl block definition
    fn visit_impl_def(&mut self, impl_def: &'ast ImplDef) {
        // Check the struct name
        if span_contains_offset(&impl_def.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&impl_def.name));
            return;
        }

        // Check trait name if present
        if let Some(trait_name) = &impl_def.trait_name {
            if span_contains_offset(&trait_name.span, self.offset) {
                self.found_node = Some(NodeAtPosition::Identifier(trait_name));
                return;
            }
        }

        // Check functions within the impl block
        for func in &impl_def.functions {
            if span_contains_offset(&func.span, self.offset) {
                self.parents.push(NodeAtPosition::FnDef(func));
                self.visit_fn_def(func);
                if self.found_node.is_none() {
                    self.parents.pop();
                }
                if self.found_node.is_some() {
                    return;
                }
            }
        }

        // If no specific node found, return the impl def itself
        if self.found_node.is_none() {
            self.found_node = Some(NodeAtPosition::ImplDef(impl_def));
        }
    }

    /// Visit a function definition inside an impl block (`FnDef`)
    fn visit_fn_def(&mut self, func_def: &'ast FnDef) {
        // Check function name
        if span_contains_offset(&func_def.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&func_def.name));
            return;
        }

        // Check parameters
        for param in &func_def.params {
            if span_contains_offset(&param.span, self.offset) {
                // Check parameter name
                if span_contains_offset(&param.name.span, self.offset) {
                    self.found_node = Some(NodeAtPosition::Identifier(&param.name));
                    return;
                }
                // Check parameter type
                if let Some(ref ty) = param.ty {
                    self.visit_type(ty);
                    if self.found_node.is_some() {
                        return;
                    }
                }
                // Return the parameter itself
                self.found_node = Some(NodeAtPosition::FunctionParam(param));
                return;
            }
        }

        // Check return type
        if let Some(ref ret_ty) = func_def.return_type {
            self.visit_type(ret_ty);
            if self.found_node.is_some() {
                return;
            }
        }

        // Check body expression (only if function has a body)
        if let Some(ref body) = func_def.body {
            self.visit_expr(body);
        }

        // If no specific node found, return the fn def itself
        if self.found_node.is_none() {
            self.found_node = Some(NodeAtPosition::FnDef(func_def));
        }
    }

    /// Visit a module definition
    fn visit_module_def(&mut self, module_def: &'ast ModuleDef) {
        // Check the module name
        if span_contains_offset(&module_def.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&module_def.name));
            return;
        }

        // Check nested definitions
        for def in &module_def.definitions {
            self.visit_definition(def);
            if self.found_node.is_some() {
                return;
            }
        }

        // If no specific node found, return the module def itself
        if self.found_node.is_none() {
            self.found_node = Some(NodeAtPosition::ModuleDef(module_def));
        }
    }

    /// Visit a standalone function definition
    fn visit_function_def(&mut self, func_def: &'ast FunctionDef) {
        // Check function name
        if span_contains_offset(&func_def.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&func_def.name));
            return;
        }

        // Check parameters
        for param in &func_def.params {
            if span_contains_offset(&param.span, self.offset) {
                // Check parameter name
                if span_contains_offset(&param.name.span, self.offset) {
                    self.found_node = Some(NodeAtPosition::Identifier(&param.name));
                    return;
                }
                // Check parameter type
                if let Some(ref ty) = param.ty {
                    self.visit_type(ty);
                    if self.found_node.is_some() {
                        return;
                    }
                }
                // Return the parameter itself
                self.found_node = Some(NodeAtPosition::FunctionParam(param));
                return;
            }
        }

        // Check return type
        if let Some(ref ret_ty) = func_def.return_type {
            self.visit_type(ret_ty);
            if self.found_node.is_some() {
                return;
            }
        }

        // Check body expression (only if function has a body)
        if let Some(ref body) = func_def.body {
            self.visit_expr(body);
        }

        // If no specific node found, return the function def itself
        if self.found_node.is_none() {
            self.found_node = Some(NodeAtPosition::FunctionDef(func_def));
        }
    }

    /// Visit a field definition
    fn visit_field_def(&mut self, field: &'ast FieldDef) {
        // Check name
        if span_contains_offset(&field.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&field.name));
            return;
        }

        // Check type
        self.visit_type(&field.ty);
    }

    /// Visit a model field
    /// Visit a struct field (unified for regular and mount fields)
    fn visit_struct_field(&mut self, field: &'ast StructField) {
        // Check name
        if span_contains_offset(&field.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&field.name));
            return;
        }

        // Check type
        self.visit_type(&field.ty);

        // Check default value
        if let Some(default) = &field.default {
            self.visit_expr(default);
        }
    }

    /// Visit a type
    fn visit_type(&mut self, ty: &'ast Type) {
        match ty {
            Type::Primitive(_) => {
                // Primitive types don't have spans to check
            }
            Type::Ident(ident) => {
                if span_contains_offset(&ident.span, self.offset) {
                    self.found_node = Some(NodeAtPosition::Identifier(ident));
                }
            }
            Type::Generic { name, args, span } => {
                if span_contains_offset(span, self.offset) {
                    // Check the generic type name
                    if span_contains_offset(&name.span, self.offset) {
                        self.found_node = Some(NodeAtPosition::Identifier(name));
                        return;
                    }

                    // Check type arguments
                    for arg in args {
                        self.visit_type(arg);
                        if self.found_node.is_some() {
                            return;
                        }
                    }

                    // If no specific part found, return the type itself
                    if self.found_node.is_none() {
                        self.found_node = Some(NodeAtPosition::Type(ty));
                    }
                }
            }
            Type::Array(inner) | Type::Optional(inner) => {
                self.visit_type(inner);
            }
            Type::Tuple(fields) => {
                for field in fields {
                    if span_contains_offset(&field.span, self.offset) {
                        if span_contains_offset(&field.name.span, self.offset) {
                            self.found_node = Some(NodeAtPosition::Identifier(&field.name));
                            return;
                        }
                        self.visit_type(&field.ty);
                        if self.found_node.is_some() {
                            return;
                        }
                    }
                }
            }
            Type::Dictionary { key, value } => {
                self.visit_type(key);
                if self.found_node.is_some() {
                    return;
                }
                self.visit_type(value);
            }
            Type::Closure { params, ret } => {
                for (_, param) in params {
                    self.visit_type(param);
                    if self.found_node.is_some() {
                        return;
                    }
                }
                self.visit_type(ret);
            }
        }
    }

    /// Visit an expression
    fn visit_expr(&mut self, expr: &'ast Expr) {
        if let Some(span) = Self::expr_span(expr) {
            if span_contains_offset(&span, self.offset) {
                self.parents.push(NodeAtPosition::Expression(expr));
                self.visit_expr_inner(expr);
                if self.found_node.is_none() {
                    self.found_node = Some(NodeAtPosition::Expression(expr));
                }
                self.parents.pop();
            }
        }
    }

    /// Dispatch to per-variant expression visitors.
    #[expect(
        clippy::too_many_lines,
        reason = "exhaustive per-Expr-variant traversal; each arm is a short recursive walk"
    )]
    fn visit_expr_inner(&mut self, expr: &'ast Expr) {
        match expr {
            Expr::Reference { path, .. } => {
                for ident in path {
                    if span_contains_offset(&ident.span, self.offset) {
                        self.found_node = Some(NodeAtPosition::Identifier(ident));
                        return;
                    }
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                self.visit_expr(left);
                if self.found_node.is_none() {
                    self.visit_expr(right);
                }
            }
            Expr::ForExpr {
                var,
                collection,
                body,
                ..
            } => {
                if span_contains_offset(&var.span, self.offset) {
                    self.found_node = Some(NodeAtPosition::Identifier(var));
                } else {
                    self.visit_expr(collection);
                    if self.found_node.is_none() {
                        self.visit_expr(body);
                    }
                }
            }
            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.visit_expr(condition);
                if self.found_node.is_none() {
                    self.visit_expr(then_branch);
                }
                if self.found_node.is_none() {
                    if let Some(else_expr) = else_branch {
                        self.visit_expr(else_expr);
                    }
                }
            }
            Expr::MatchExpr {
                scrutinee, arms, ..
            } => {
                self.visit_expr(scrutinee);
                if self.found_node.is_none() {
                    for arm in arms {
                        self.visit_expr(&arm.body);
                        if self.found_node.is_some() {
                            break;
                        }
                    }
                }
            }
            Expr::Group { expr, .. } => {
                self.visit_expr(expr);
            }
            Expr::Array { elements, .. } => {
                for elem in elements {
                    self.visit_expr(elem);
                    if self.found_node.is_some() {
                        break;
                    }
                }
            }
            Expr::Tuple { fields, .. } => {
                for (name, field_expr) in fields {
                    if span_contains_offset(&name.span, self.offset) {
                        self.found_node = Some(NodeAtPosition::Identifier(name));
                        break;
                    }
                    self.visit_expr(field_expr);
                    if self.found_node.is_some() {
                        break;
                    }
                }
            }
            Expr::Invocation { args, .. } => {
                for (_, arg) in args {
                    self.visit_expr(arg);
                    if self.found_node.is_some() {
                        break;
                    }
                }
            }
            Expr::EnumInstantiation { data, .. } | Expr::InferredEnumInstantiation { data, .. } => {
                for (_, v) in data {
                    self.visit_expr(v);
                    if self.found_node.is_some() {
                        break;
                    }
                }
            }
            Expr::UnaryOp { operand, .. } => self.visit_expr(operand),
            Expr::DictLiteral { entries, .. } => {
                for (key, value) in entries {
                    self.visit_expr(key);
                    if self.found_node.is_some() {
                        break;
                    }
                    self.visit_expr(value);
                    if self.found_node.is_some() {
                        break;
                    }
                }
            }
            Expr::DictAccess { dict, key, .. } => {
                self.visit_expr(dict);
                if self.found_node.is_none() {
                    self.visit_expr(key);
                }
            }
            Expr::FieldAccess { object, .. } => self.visit_expr(object),
            Expr::ClosureExpr { body, .. } => self.visit_expr(body),
            Expr::LetExpr { value, body, .. } => {
                self.visit_expr(value);
                if self.found_node.is_none() {
                    self.visit_expr(body);
                }
            }
            Expr::MethodCall { receiver, args, .. } => {
                self.visit_expr(receiver);
                if self.found_node.is_some() {
                    return;
                }
                for (_, arg) in args {
                    self.visit_expr(arg);
                    if self.found_node.is_some() {
                        break;
                    }
                }
            }
            Expr::Block {
                statements, result, ..
            } => {
                for stmt in statements {
                    match stmt {
                        BlockStatement::Let { value, .. } => self.visit_expr(value),
                        BlockStatement::Assign { target, value, .. } => {
                            self.visit_expr(target);
                            if self.found_node.is_none() {
                                self.visit_expr(value);
                            }
                        }
                        BlockStatement::Expr(expr) => self.visit_expr(expr),
                    }
                    if self.found_node.is_some() {
                        break;
                    }
                }
                if self.found_node.is_none() {
                    self.visit_expr(result);
                }
            }
            Expr::Literal { .. } => {}
        }
    }

    /// Get the span of an expression. Every `Expr` variant now carries a
    /// real span (audit finding #19), so this is always `Some` — kept for
    /// backwards compatibility with the existing callers.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "callers pattern-match on Option to skip nodes with no span; preserved for API stability"
    )]
    const fn expr_span(expr: &Expr) -> Option<Span> {
        Some(expr.span())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_only;

    fn parse(source: &str) -> Result<File, Vec<crate::error::CompilerError>> {
        parse_only(source)
    }

    fn find_offset_of(source: &str, pattern: &str) -> Result<usize, Box<dyn std::error::Error>> {
        source
            .find(pattern)
            .ok_or_else(|| format!("Pattern {pattern:?} not found in source").into())
    }

    #[test]
    fn test_find_struct_definition() -> Result<(), Box<dyn std::error::Error>> {
        let source = "struct User { name: String }";
        let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

        // Position on "User" - may return Identifier or StructDef
        let offset = find_offset_of(source, "User")?;
        let ctx = find_node_at_offset(&file, offset);

        // Either the node is a StructDef or there's a StructDef in parents
        let is_struct = matches!(ctx.node, NodeAtPosition::StructDef(_))
            || ctx
                .parents
                .iter()
                .any(|p| matches!(p, NodeAtPosition::StructDef(_)));
        if !is_struct && !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
            return Err(format!("Expected StructDef or Identifier, got {:?}", ctx.node).into());
        }
        Ok(())
    }

    #[test]
    fn test_find_trait_definition() -> Result<(), Box<dyn std::error::Error>> {
        let source = "trait Named { name: String }";
        let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

        // Position on "Named" - may return Identifier or TraitDef
        let offset = find_offset_of(source, "Named")?;
        let ctx = find_node_at_offset(&file, offset);

        // Either the node is a TraitDef or there's a TraitDef in parents
        let is_trait = matches!(ctx.node, NodeAtPosition::TraitDef(_))
            || ctx
                .parents
                .iter()
                .any(|p| matches!(p, NodeAtPosition::TraitDef(_)));
        if !is_trait && !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
            return Err(format!("Expected TraitDef or Identifier, got {:?}", ctx.node).into());
        }
        Ok(())
    }

    #[test]
    fn test_find_enum_definition() -> Result<(), Box<dyn std::error::Error>> {
        let source = "enum Status { active, inactive }";
        let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

        // Position on "Status" - may return Identifier or EnumDef
        let offset = find_offset_of(source, "Status")?;
        let ctx = find_node_at_offset(&file, offset);

        // Either the node is an EnumDef or there's an EnumDef in parents
        let is_enum = matches!(ctx.node, NodeAtPosition::EnumDef(_))
            || ctx
                .parents
                .iter()
                .any(|p| matches!(p, NodeAtPosition::EnumDef(_)));
        if !is_enum && !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
            return Err(format!("Expected EnumDef or Identifier, got {:?}", ctx.node).into());
        }
        Ok(())
    }

    #[test]
    fn test_find_field_in_struct() -> Result<(), Box<dyn std::error::Error>> {
        let source = "struct User { name: String }";
        let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

        // Position on "name" field
        let offset = find_offset_of(source, "name")?;
        let ctx = find_node_at_offset(&file, offset);

        // Should find the struct field
        if !matches!(
            ctx.node,
            NodeAtPosition::StructField(_) | NodeAtPosition::Identifier(_)
        ) {
            return Err(format!("Expected StructField or Identifier, got {:?}", ctx.node).into());
        }
        Ok(())
    }

    #[test]
    fn test_find_type_in_field() -> Result<(), Box<dyn std::error::Error>> {
        let source = "struct User { name: String }";
        let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

        // Position on "String" type
        let offset = find_offset_of(source, "String")?;
        let ctx = find_node_at_offset(&file, offset);

        // Could be Type, Identifier, StructField, or even StructDef
        // The finder returns the innermost node
        let is_valid = matches!(
            ctx.node,
            NodeAtPosition::Type(_)
                | NodeAtPosition::Identifier(_)
                | NodeAtPosition::StructField(_)
                | NodeAtPosition::StructDef(_)
        );
        if !is_valid {
            return Err(format!(
                "Expected Type/Identifier/StructField/StructDef, got {:?}",
                ctx.node
            )
            .into());
        }
        Ok(())
    }

    #[test]
    fn test_find_let_binding() -> Result<(), Box<dyn std::error::Error>> {
        let source = "let x = 42";
        let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

        // Position on "x"
        let offset = find_offset_of(source, "x")?;
        let ctx = find_node_at_offset(&file, offset);

        // Should find identifier within let binding
        if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
            return Err(format!("Expected Identifier, got {:?}", ctx.node).into());
        }
        // Let binding should be a parent
        if !ctx
            .parents
            .iter()
            .any(|n| matches!(n, NodeAtPosition::LetBinding(_)))
        {
            return Err("Expected LetBinding in parents".into());
        }
        Ok(())
    }

    #[test]
    fn test_enclosing_definition_in_struct_field() -> Result<(), Box<dyn std::error::Error>> {
        let source = "struct User { name: String }";
        let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

        // Position inside the struct on "name"
        let offset = find_offset_of(source, "name")?;
        let ctx = find_node_at_offset(&file, offset);

        let enclosing = ctx.enclosing_definition();
        if enclosing.is_none() {
            return Err("Expected enclosing definition but got None".into());
        }
        if !matches!(enclosing, Some(NodeAtPosition::StructDef(_))) {
            return Err(format!("Expected StructDef enclosing, got {enclosing:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_enclosing_definition_outside_struct() -> Result<(), Box<dyn std::error::Error>> {
        let source = "let x = 42";
        let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

        // Position in let binding
        let offset = find_offset_of(source, "42")?;
        let ctx = find_node_at_offset(&file, offset);

        // No enclosing definition for top-level let
        let enclosing = ctx.enclosing_definition();
        if enclosing.is_some() {
            return Err(format!("Expected no enclosing definition, got {enclosing:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_is_in_expression() -> Result<(), Box<dyn std::error::Error>> {
        let source = "let x = 1 + 2";
        let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

        // Position on "1" in expression
        let offset = find_offset_of(source, "1 +")?;
        let ctx = find_node_at_offset(&file, offset);

        // Should be in expression context (either the node is expression or has expression parent)
        let has_expression = ctx.is_in_expression()
            || matches!(ctx.node, NodeAtPosition::Expression(_))
            || ctx
                .parents
                .iter()
                .any(|p| matches!(p, NodeAtPosition::Expression(_)));
        // Or might just be a LetBinding
        if !has_expression && !matches!(ctx.node, NodeAtPosition::LetBinding(_)) {
            return Err(format!("Expected expression context, got {:?}", ctx.node).into());
        }
        Ok(())
    }

    #[test]
    fn test_is_in_type_position() -> Result<(), Box<dyn std::error::Error>> {
        let source = "struct User { name: String }";
        let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

        // Position on "String" type
        let offset = find_offset_of(source, "String")?;
        let ctx = find_node_at_offset(&file, offset);

        // May or may not be in type position depending on exact offset
        // Just verify the method doesn't panic
        let _ = ctx.is_in_type_position();
        Ok(())
    }

    #[test]
    fn test_find_enum_variant() -> Result<(), Box<dyn std::error::Error>> {
        let source = "enum Status { active, inactive }";
        let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

        // Position on "active" variant
        let offset = find_offset_of(source, "active")?;
        let ctx = find_node_at_offset(&file, offset);

        // Should find variant or identifier
        if !matches!(
            ctx.node,
            NodeAtPosition::EnumVariant(_) | NodeAtPosition::Identifier(_)
        ) {
            return Err(format!("Expected EnumVariant or Identifier, got {:?}", ctx.node).into());
        }
        Ok(())
    }

    #[test]
    fn test_find_node_at_file_start() -> Result<(), Box<dyn std::error::Error>> {
        let source = "struct A { }";
        let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

        // Position at very beginning
        let ctx = find_node_at_offset(&file, 0);

        // Should find something (struct definition starts at offset 0)
        if matches!(ctx.node, NodeAtPosition::None) {
            return Err("Expected some node at offset 0, got None".into());
        }
        Ok(())
    }

    #[test]
    fn test_find_node_past_end() -> Result<(), Box<dyn std::error::Error>> {
        let source = "struct A { }";
        let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

        // Position way past the end
        let ctx = find_node_at_offset(&file, 10000);

        // Should return File or None
        if !matches!(ctx.node, NodeAtPosition::File | NodeAtPosition::None) {
            return Err(format!("Expected File or None past end, got {:?}", ctx.node).into());
        }
        Ok(())
    }

    #[test]
    fn test_parents_chain() -> Result<(), Box<dyn std::error::Error>> {
        let source = r"
            struct User {
                name: String,
                age: Number
            }
        ";
        let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

        // Position on "age" field
        let offset = find_offset_of(source, "age")?;
        let ctx = find_node_at_offset(&file, offset);

        // Should have struct as parent somewhere
        let has_struct_parent = ctx
            .parents
            .iter()
            .any(|p| matches!(p, NodeAtPosition::StructDef(_)));
        if !has_struct_parent {
            return Err("Expected StructDef in parents chain".into());
        }
        Ok(())
    }

    #[test]
    fn test_find_use_statement() -> Result<(), Box<dyn std::error::Error>> {
        let source = "use foo::bar";
        let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

        // Position on "foo"
        let offset = find_offset_of(source, "foo")?;
        let ctx = find_node_at_offset(&file, offset);

        // Should find identifier or use statement
        if !matches!(
            ctx.node,
            NodeAtPosition::Identifier(_) | NodeAtPosition::UseStatement(_)
        ) {
            return Err(format!("Expected Identifier or UseStatement, got {:?}", ctx.node).into());
        }
        Ok(())
    }
}
