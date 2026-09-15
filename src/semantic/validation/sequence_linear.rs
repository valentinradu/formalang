//! A sequence must be consumed exactly once.
//!
//! Two halves, travelling together as linearity:
//!
//! **At most once** is what buys fusion. A sequence has at most one
//! reader, so there is never a second consumer to materialise for, and
//! the backend never has to decide whether joining two loops is safe.
//!
//! **At least once** is what stops silent dead code. A dropped
//! sequence is not like a dropped number: drop an `I32` and a value
//! was computed and ignored, but drop a `Seq` and **nothing ran at
//! all**. That holds whether the body is pure or effectful, which is
//! why one unconditional rule covers both and no effect analysis is
//! needed.
//!
//! This is a separate walk rather than a hook in the general
//! validator, because that validator visits one expression more than
//! once — argument checking and overload resolution both descend into
//! the same nodes — and counting reads needs each one seen exactly
//! once.

use std::collections::HashMap;

use super::super::module_resolver::ModuleResolver;
use super::super::sem_type::SemType;
use super::super::SemanticAnalyzer;
use crate::ast::{
    BindingPattern, BlockStatement, Definition, Expr, File, FnParam, ParamConvention, Statement,
};
use crate::error::CompilerError;
use crate::location::Span;

/// One sequence binding in scope, and whether anything has read it.
struct Binding {
    /// Where the sequence was made, for the "never consumed" report.
    origin: Span,
    /// Where the first read was, once one happens.
    consumed_at: Option<Span>,
}

/// Scope stack for one function body. The innermost frame is last.
type Scopes = Vec<HashMap<String, Binding>>;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    pub(in crate::semantic) fn validate_sequence_linearity(&mut self, file: &File) {
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                self.check_linear_definition(def, file);
            }
        }
    }

    fn check_linear_definition(&mut self, def: &Definition, file: &File) {
        match def {
            Definition::Function(f) => {
                if let Some(body) = &f.body {
                    self.check_linear_body(&f.params, body, file);
                }
            }
            Definition::Impl(i) => {
                for method in &i.functions {
                    if let Some(body) = &method.body {
                        self.check_linear_body(&method.params, body, file);
                    }
                }
            }
            Definition::Module(m) => {
                for nested in &m.definitions {
                    self.check_linear_definition(nested, file);
                }
            }
            Definition::Struct(_) | Definition::Enum(_) | Definition::Trait(_) => {}
        }
    }

    /// Walk one function body, tracking every sequence binding.
    ///
    /// A `sink` parameter of sequence type is a binding like any
    /// other: the callee took ownership, so the callee has to consume
    /// it.
    fn check_linear_body(&mut self, params: &[FnParam], body: &Expr, file: &File) {
        let mut scopes: Scopes = vec![HashMap::new()];
        for param in params {
            let Some(ty) = &param.ty else { continue };
            if param.convention == ParamConvention::Sink
                && Self::is_sequence(&SemType::from_ast(ty))
            {
                Self::bind(&mut scopes, &param.name.name, param.span);
            }
        }
        self.walk_linear(body, &mut scopes, file);
        self.close_scope(&mut scopes);
    }

    fn bind(scopes: &mut Scopes, name: &str, origin: Span) {
        if let Some(frame) = scopes.last_mut() {
            frame.insert(
                name.to_string(),
                Binding {
                    origin,
                    consumed_at: None,
                },
            );
        }
    }

    /// Report every binding the closing frame introduced that nothing
    /// read.
    fn close_scope(&mut self, scopes: &mut Scopes) {
        let Some(frame) = scopes.pop() else { return };
        for binding in frame.into_values() {
            if binding.consumed_at.is_none() {
                self.errors.push(CompilerError::SeqNotConsumed {
                    span: binding.origin,
                });
            }
        }
    }

    /// Record a read of `name`, and report a second one.
    fn read(&mut self, scopes: &mut Scopes, name: &str, span: Span) {
        for frame in scopes.iter_mut().rev() {
            let Some(binding) = frame.get_mut(name) else {
                continue;
            };
            if binding.consumed_at.is_some() {
                self.errors.push(CompilerError::SeqUsedTwice {
                    name: name.to_string(),
                    span,
                });
            } else {
                binding.consumed_at = Some(span);
            }
            return;
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one arm per expression shape; splitting the walk hides the traversal order"
    )]
    fn walk_linear(&mut self, expr: &Expr, scopes: &mut Scopes, file: &File) {
        match expr {
            Expr::Reference { path, span } => {
                if let [only] = path.as_slice() {
                    self.read(scopes, &only.name, *span);
                }
            }
            Expr::Block {
                statements, result, ..
            } => {
                scopes.push(HashMap::new());
                for stmt in statements {
                    self.walk_linear_statement(stmt, scopes, file);
                }
                self.walk_linear(result, scopes, file);
                self.close_scope(scopes);
            }
            Expr::ForExpr {
                collection, body, ..
            } => {
                // The source is consumed by the loop; the body runs
                // per element and cannot hold a sequence across steps.
                self.walk_linear(collection, scopes, file);
                scopes.push(HashMap::new());
                self.walk_linear(body, scopes, file);
                self.close_scope(scopes);
            }
            Expr::MethodCall { receiver, args, .. } => {
                self.walk_linear(receiver, scopes, file);
                for (_, arg) in args {
                    self.walk_linear(arg, scopes, file);
                }
            }
            Expr::Invocation { args, .. } => {
                for (_, arg) in args {
                    self.walk_linear(arg, scopes, file);
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                self.walk_linear(left, scopes, file);
                self.walk_linear(right, scopes, file);
            }
            Expr::UnaryOp { operand, .. } => self.walk_linear(operand, scopes, file),
            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.walk_linear(condition, scopes, file);
                // Branches are alternatives, so a sequence read in
                // both is read once on any path. Walk each against the
                // same state and keep whichever read happened.
                self.walk_linear(then_branch, scopes, file);
                if let Some(else_expr) = else_branch {
                    self.walk_linear(else_expr, scopes, file);
                }
            }
            // `let pat = value { body }` — the optional-unwrap form.
            Expr::LetExpr {
                pattern,
                value,
                body,
                ty,
                span,
                ..
            } => {
                self.walk_linear(value, scopes, file);
                scopes.push(HashMap::new());
                let declared = ty
                    .as_ref()
                    .map_or_else(|| self.infer_type_sem(value, file), SemType::from_ast);
                if Self::is_sequence(&declared) {
                    if let BindingPattern::Simple(ident) = pattern {
                        Self::bind(scopes, &ident.name.clone(), *span);
                    }
                }
                self.walk_linear(body, scopes, file);
                self.close_scope(scopes);
            }
            Expr::Group { expr: inner, .. } => self.walk_linear(inner, scopes, file),
            Expr::MatchExpr {
                scrutinee, arms, ..
            } => {
                self.walk_linear(scrutinee, scopes, file);
                for arm in arms {
                    self.walk_linear(&arm.body, scopes, file);
                }
            }
            Expr::Array { elements, .. } => {
                for element in elements {
                    self.walk_linear(element, scopes, file);
                }
            }
            Expr::Tuple { fields, .. } => {
                for (_, value) in fields {
                    self.walk_linear(value, scopes, file);
                }
            }
            Expr::DictLiteral { entries, .. } => {
                for (key, value) in entries {
                    self.walk_linear(key, scopes, file);
                    self.walk_linear(value, scopes, file);
                }
            }
            Expr::DictAccess { dict, key, .. } => {
                self.walk_linear(dict, scopes, file);
                self.walk_linear(key, scopes, file);
            }
            Expr::FieldAccess { object, .. } => self.walk_linear(object, scopes, file),
            Expr::EnumInstantiation { data, .. } | Expr::InferredEnumInstantiation { data, .. } => {
                for (_, value) in data {
                    self.walk_linear(value, scopes, file);
                }
            }
            Expr::ClosureExpr { body, .. } => self.walk_linear(body, scopes, file),
            Expr::Literal { .. } => {}
        }
    }

    fn walk_linear_statement(&mut self, stmt: &BlockStatement, scopes: &mut Scopes, file: &File) {
        match stmt {
            BlockStatement::Let {
                pattern,
                value,
                ty,
                span,
                ..
            } => {
                self.walk_linear(value, scopes, file);
                let declared = ty
                    .as_ref()
                    .map_or_else(|| self.infer_type_sem(value, file), SemType::from_ast);
                if Self::is_sequence(&declared) {
                    if let BindingPattern::Simple(ident) = pattern {
                        Self::bind(scopes, &ident.name.clone(), *span);
                    }
                }
            }
            BlockStatement::Assign { target, value, .. } => {
                self.walk_linear(target, scopes, file);
                self.walk_linear(value, scopes, file);
            }
            BlockStatement::Expr(expr) => {
                self.walk_linear(expr, scopes, file);
                // Nothing takes this value, so nothing consumes the
                // sequence and the loop never runs.
                if Self::is_sequence(&self.infer_type_sem(expr, file)) {
                    self.errors
                        .push(CompilerError::SeqNotConsumed { span: expr.span() });
                }
            }
        }
    }
}
