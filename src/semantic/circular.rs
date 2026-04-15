use super::module_resolver::ModuleResolver;
use super::type_graph::TypeGraph;
use super::SemanticAnalyzer;
use crate::ast::{BlockStatement, Definition, Expr, File, Statement};
use crate::error::CompilerError;
use crate::location::Span;
use std::collections::{HashMap, HashSet};

use super::collect_bindings_from_pattern;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Pass 5: Detect circular dependencies
    /// Build dependency graphs and detect cycles
    pub(super) fn detect_circular_dependencies(&mut self, file: &File) {
        // 5.1: Detect circular type dependencies
        self.detect_circular_type_dependencies(file);

        // 5.2: Detect circular let binding dependencies
        self.detect_circular_let_dependencies(file);
    }

    /// Pass 5.1: Detect circular type dependencies
    /// Build a type dependency graph and detect cycles
    pub(super) fn detect_circular_type_dependencies(&mut self, file: &File) {
        let mut type_graph = TypeGraph::new();
        let mut type_spans: HashMap<String, Span> = HashMap::new();

        // Build the type dependency graph
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                match &**def {
                    Definition::Trait(trait_def) => {
                        let trait_name = trait_def.name.name.clone();
                        type_spans.insert(trait_name.clone(), trait_def.span);

                        // Add dependencies from trait inheritance (trait A: B)
                        for parent_trait in &trait_def.traits {
                            type_graph
                                .add_dependency(trait_name.clone(), parent_trait.name.clone());
                        }

                        // Add dependencies from trait fields
                        for field in &trait_def.fields {
                            Self::add_type_dependencies(&mut type_graph, &trait_name, &field.ty);
                        }

                        // Note: Mount points are NOT added to the dependency graph.
                        // Mount points are "slots" filled at runtime with child content,
                        // so self-referential mount points (e.g., `mount body: View` in View trait)
                        // are valid and don't create impossible-to-construct types.
                        // The recursion is always broken by terminal types like Empty.
                    }
                    Definition::Struct(struct_def) => {
                        let struct_name = struct_def.name.name.clone();
                        type_spans.insert(struct_name.clone(), struct_def.span);

                        // Add dependencies from struct fields
                        for field in &struct_def.fields {
                            Self::add_type_dependencies(&mut type_graph, &struct_name, &field.ty);
                        }

                        // Note: Mount points are NOT added to the dependency graph.
                        // See comment above in trait handling for rationale.
                    }
                    Definition::Enum(_)
                    | Definition::Impl(_)
                    | Definition::Module(_)
                    | Definition::Function(_)
                    | Definition::ExternType(_) => {
                        // Enums, impl blocks, modules, standalone functions, and extern types
                        // don't create type dependencies directly
                    }
                }
            }
        }

        // Detect cycles
        let cycles = type_graph.find_cycles();

        // Report errors for each cycle found
        for cycle in cycles {
            if !cycle.is_empty() {
                // Get the span of the first type in the cycle
                let span = cycle
                    .first()
                    .and_then(|t| type_spans.get(t))
                    .copied()
                    .unwrap_or_default();

                // Format the cycle as "A -> B -> C -> A"
                let cycle_str = cycle.join(" -> ");

                self.errors.push(CompilerError::CircularDependency {
                    cycle: cycle_str,
                    span,
                });
            }
        }
    }

    /// Pass 5.2: Detect circular let binding dependencies
    /// Build a let binding dependency graph and detect cycles
    pub(super) fn detect_circular_let_dependencies(&mut self, file: &File) {
        let mut let_graph = TypeGraph::new();
        let mut let_spans: HashMap<String, Span> = HashMap::new();

        // Build the let binding dependency graph
        for statement in &file.statements {
            if let Statement::Let(let_binding) = statement {
                // Get all bindings from the pattern
                let bindings = collect_bindings_from_pattern(&let_binding.pattern);
                if bindings.is_empty() {
                    continue;
                }

                // Register each binding and store its span
                for binding in &bindings {
                    let_spans.insert(binding.name.clone(), binding.span);
                }

                // Extract all let binding references from the value expression
                let references = self.extract_let_references(&let_binding.value);

                // Add dependencies for each binding from the pattern
                // All bindings from a single let share the same dependencies
                for binding in &bindings {
                    for referenced_let in &references {
                        let_graph.add_dependency(&binding.name, referenced_let.clone());
                    }
                }
            }
        }

        // Detect cycles
        let cycles = let_graph.find_cycles();

        // Report errors for each cycle found
        for cycle in cycles {
            if !cycle.is_empty() {
                // Get the span of the first let binding in the cycle
                let span = cycle
                    .first()
                    .and_then(|l| let_spans.get(l))
                    .copied()
                    .unwrap_or_default();

                // Format the cycle as "a -> b -> c -> a"
                let cycle_str = cycle.join(" -> ");

                self.errors.push(CompilerError::CircularDependency {
                    cycle: cycle_str,
                    span,
                });
            }
        }
    }

    /// Extract all let binding references from an expression
    /// Returns a set of let binding names that this expression depends on
    pub(super) fn extract_let_references(&self, expr: &Expr) -> HashSet<String> {
        let mut refs = HashSet::new();
        self.collect_let_references(expr, &mut refs);
        refs
    }

    /// Recursive worker for `extract_let_references` — accumulates into an existing set
    fn collect_let_references(&self, expr: &Expr, refs: &mut HashSet<String>) {
        match expr {
            Expr::Literal(_) => {}
            Expr::Array { elements, .. } => {
                for elem in elements {
                    self.collect_let_references(elem, refs);
                }
            }
            Expr::Tuple { fields, .. } => {
                for (_, field_expr) in fields {
                    self.collect_let_references(field_expr, refs);
                }
            }
            Expr::Reference { path, .. } => {
                if let Some(first) = path.first().filter(|_| path.len() == 1) {
                    if self.symbols.is_let(&first.name) {
                        refs.insert(first.name.clone());
                    }
                }
            }
            Expr::Invocation { args, .. } => {
                for (_, arg_expr) in args {
                    self.collect_let_references(arg_expr, refs);
                }
            }
            Expr::EnumInstantiation { data, .. } | Expr::InferredEnumInstantiation { data, .. } => {
                for (_, data_expr) in data {
                    self.collect_let_references(data_expr, refs);
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                self.collect_let_references(left, refs);
                self.collect_let_references(right, refs);
            }
            Expr::UnaryOp { operand, .. } => {
                self.collect_let_references(operand, refs);
            }
            Expr::ForExpr {
                collection, body, ..
            } => {
                self.collect_let_references(collection, refs);
                self.collect_let_references(body, refs);
            }
            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_let_references(condition, refs);
                self.collect_let_references(then_branch, refs);
                if let Some(else_expr) = else_branch {
                    self.collect_let_references(else_expr, refs);
                }
            }
            Expr::MatchExpr {
                scrutinee, arms, ..
            } => {
                self.collect_let_references(scrutinee, refs);
                for arm in arms {
                    self.collect_let_references(&arm.body, refs);
                }
            }
            Expr::Group { expr, .. } => self.collect_let_references(expr, refs),
            Expr::DictLiteral { entries, .. } => {
                for (key, value) in entries {
                    self.collect_let_references(key, refs);
                    self.collect_let_references(value, refs);
                }
            }
            Expr::DictAccess { dict, key, .. } => {
                self.collect_let_references(dict, refs);
                self.collect_let_references(key, refs);
            }
            Expr::FieldAccess { object, .. } => self.collect_let_references(object, refs),
            Expr::ClosureExpr { body, .. } => self.collect_let_references(body, refs),
            Expr::LetExpr { value, body, .. } => {
                self.collect_let_references(value, refs);
                self.collect_let_references(body, refs);
            }
            Expr::MethodCall { receiver, args, .. } => {
                self.collect_let_references(receiver, refs);
                for arg in args {
                    self.collect_let_references(arg, refs);
                }
            }
            Expr::Block {
                statements, result, ..
            } => {
                self.collect_let_references_block(statements, result, refs);
            }
        }
    }

    /// Extract let references from a block's statements and result expression
    fn collect_let_references_block(
        &self,
        statements: &[BlockStatement],
        result: &Expr,
        refs: &mut HashSet<String>,
    ) {
        for stmt in statements {
            match stmt {
                BlockStatement::Let { value, .. } => {
                    self.collect_let_references(value, refs);
                }
                BlockStatement::Assign { target, value, .. } => {
                    self.collect_let_references(target, refs);
                    self.collect_let_references(value, refs);
                }
                BlockStatement::Expr(expr) => {
                    self.collect_let_references(expr, refs);
                }
            }
        }
        self.collect_let_references(result, refs);
    }
}
