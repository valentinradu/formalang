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
//! Two rules follow from this. A read of an outer sequence in a body
//! that can run more than once, a `for` body or a closure argument such
//! as the closure of `fold`, is a read per pass: `SeqUsedTwice`. And a
//! sequence must be read on every path: when one branch of an `if`, one
//! arm of a `match` or the right side of `&&` or `||` reads it and
//! another path does not, the walk reports `SeqNotConsumed`.
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
    BinaryOperator, BindingPattern, BlockStatement, Definition, Expr, File, FnParam, Ident,
    ParamConvention, Statement,
};
use crate::error::CompilerError;
use crate::location::Span;

/// One binding in scope, and whether anything has read it.
///
/// Every binding is recorded, not only the sequences. A plain binding
/// that reuses a name has to hide the sequence above it, or reading the
/// inner name twice — which is fine, it holds an `I32` — is reported
/// against the outer sequence.
#[derive(Clone)]
struct Binding {
    /// Where the sequence was made, for the "never consumed" report.
    origin: Span,
    /// Where the first read was, once one happens.
    consumed_at: Option<Span>,
    /// Whether this binding holds a sequence. A binding that does not
    /// is here only to shadow.
    holds_a_sequence: bool,
    /// True while the walk is in a body that can run more than once,
    /// such as a loop body or a `fold` closure, and the binding is
    /// from outside that body. A read there is a read per pass.
    outside_repeat: bool,
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
                    self.check_linear_body(&f.params, body, f.return_type.is_some(), file);
                }
            }
            Definition::Impl(i) => {
                for method in &i.functions {
                    if let Some(body) = &method.body {
                        self.check_linear_body(
                            &method.params,
                            body,
                            method.return_type.is_some(),
                            file,
                        );
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
    fn check_linear_body(
        &mut self,
        params: &[FnParam],
        body: &Expr,
        returns_a_value: bool,
        file: &File,
    ) {
        let mut scopes: Scopes = vec![HashMap::new()];
        for param in params {
            let holds_a_sequence = param.ty.as_ref().is_some_and(|ty| {
                param.convention == ParamConvention::Sink
                    && Self::is_sequence(&SemType::from_ast(ty))
            });
            Self::bind(&mut scopes, &param.name.name, param.span, holds_a_sequence);
        }
        self.walk_linear(body, &mut scopes, file);
        self.close_scope(&mut scopes);

        // A function that declares no return type discards whatever
        // its body ends with. If that is a sequence, nothing consumes
        // it and the loop never runs. The statement-position check
        // covers a loop written above the last line; this covers the
        // last line itself, which it did not reach.
        if !returns_a_value {
            let tail = Self::tail_expression(body);
            if Self::is_sequence(&self.infer_type_sem(tail, file)) {
                self.errors
                    .push(CompilerError::SeqNotConsumed { span: tail.span() });
            }
        }
    }

    /// The expression whose value a body produces.
    ///
    /// A block's value is its result, however many blocks deep.
    fn tail_expression(expr: &Expr) -> &Expr {
        if let Expr::Block { result, .. } = expr {
            Self::tail_expression(result)
        } else {
            expr
        }
    }

    fn bind(scopes: &mut Scopes, name: &str, origin: Span, holds_a_sequence: bool) {
        if let Some(frame) = scopes.last_mut() {
            frame.insert(
                name.to_string(),
                Binding {
                    origin,
                    consumed_at: None,
                    holds_a_sequence,
                    outside_repeat: false,
                },
            );
        }
    }

    /// Report every binding the closing frame introduced that nothing
    /// read.
    fn close_scope(&mut self, scopes: &mut Scopes) {
        let Some(frame) = scopes.pop() else { return };
        for binding in frame.into_values() {
            if binding.holds_a_sequence && binding.consumed_at.is_none() {
                self.errors.push(CompilerError::SeqNotConsumed {
                    span: binding.origin,
                });
            }
        }
    }

    /// Walk a set of branches that are alternatives to each other.
    ///
    /// Only one of them runs, so each starts from the state before the
    /// branch, not from what the branch before it left behind. `None`
    /// is a branch that reads nothing, such as a missing `else` or the
    /// skipped right side of `&&`.
    ///
    /// A sequence must be consumed on every path. When some branches
    /// read an outer sequence and others do not, the paths that do not
    /// drop it, so the walk reports `SeqNotConsumed` at its origin.
    fn walk_alternatives(&mut self, branches: &[Option<&Expr>], scopes: &mut Scopes, file: &File) {
        let before = scopes.clone();
        let mut states = Vec::with_capacity(branches.len());
        for branch in branches {
            let mut state = before.clone();
            if let Some(branch) = branch {
                self.walk_linear(branch, &mut state, file);
            }
            states.push(state);
        }

        let mut merged = before;
        for (depth, frame) in merged.iter_mut().enumerate() {
            for (name, binding) in frame.iter_mut() {
                if binding.consumed_at.is_some() {
                    continue;
                }
                let reads: Vec<Option<Span>> = states
                    .iter()
                    .map(|state| {
                        state
                            .get(depth)
                            .and_then(|f| f.get(name))
                            .and_then(|b| b.consumed_at)
                    })
                    .collect();
                let first_read = reads.iter().flatten().next().copied();
                if first_read.is_some() && reads.iter().any(Option::is_none) {
                    self.errors.push(CompilerError::SeqNotConsumed {
                        span: binding.origin,
                    });
                }
                binding.consumed_at = first_read;
            }
        }
        *scopes = merged;
    }

    /// Walk `body` as code that can run more than once.
    ///
    /// Every binding in scope is outside the body, so a read of an
    /// outer sequence in the body is a read per pass.
    fn walk_repeated(&mut self, body: &Expr, scopes: &mut Scopes, file: &File) {
        let mut marked = Vec::new();
        for (depth, frame) in scopes.iter_mut().enumerate() {
            for (name, binding) in frame.iter_mut() {
                if !binding.outside_repeat {
                    binding.outside_repeat = true;
                    marked.push((depth, name.clone()));
                }
            }
        }
        scopes.push(HashMap::new());
        self.walk_linear(body, scopes, file);
        self.close_scope(scopes);
        for (depth, name) in marked {
            if let Some(binding) = scopes.get_mut(depth).and_then(|f| f.get_mut(&name)) {
                binding.outside_repeat = false;
            }
        }
    }

    /// Walk the arguments of a call. A closure literal argument can run
    /// once per element, as the closure of `fold` does.
    fn walk_linear_args(
        &mut self,
        args: &[(Option<Ident>, Expr)],
        scopes: &mut Scopes,
        file: &File,
    ) {
        for (_, arg) in args {
            if matches!(arg, Expr::ClosureExpr { .. }) {
                self.walk_repeated(arg, scopes, file);
            } else {
                self.walk_linear(arg, scopes, file);
            }
        }
    }

    /// Record a read of `name`, and report a second one.
    fn read(&mut self, scopes: &mut Scopes, name: &str, span: Span) {
        for frame in scopes.iter_mut().rev() {
            let Some(binding) = frame.get_mut(name) else {
                continue;
            };
            // The innermost binding of the name is the one being read,
            // whatever it holds. A plain binding ends the search
            // without reporting anything: it is not a sequence, so
            // reading it twice is ordinary.
            if !binding.holds_a_sequence {
                return;
            }
            if binding.consumed_at.is_some() || binding.outside_repeat {
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
                self.walk_repeated(body, scopes, file);
            }
            Expr::MethodCall { receiver, args, .. }
            | Expr::Call {
                callee: receiver,
                args,
                ..
            } => {
                self.walk_linear(receiver, scopes, file);
                self.walk_linear_args(args, scopes, file);
            }
            Expr::Invocation { args, .. } => self.walk_linear_args(args, scopes, file),
            Expr::BinaryOp {
                left, op, right, ..
            } => {
                self.walk_linear(left, scopes, file);
                if matches!(op, BinaryOperator::And | BinaryOperator::Or) {
                    // The right side runs only on one path.
                    self.walk_alternatives(&[Some(right), None], scopes, file);
                } else {
                    self.walk_linear(right, scopes, file);
                }
            }
            Expr::UnaryOp { operand, .. } => self.walk_linear(operand, scopes, file),
            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.walk_linear(condition, scopes, file);
                let branches = [Some(&**then_branch), else_branch.as_deref()];
                self.walk_alternatives(&branches, scopes, file);
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
                if let BindingPattern::Simple(ident) = pattern {
                    let holds_a_sequence = Self::is_sequence(&declared);
                    Self::bind(scopes, &ident.name.clone(), *span, holds_a_sequence);
                }
                self.walk_linear(body, scopes, file);
                self.close_scope(scopes);
            }
            Expr::Group { expr: inner, .. } => self.walk_linear(inner, scopes, file),
            Expr::MatchExpr {
                scrutinee, arms, ..
            } => {
                self.walk_linear(scrutinee, scopes, file);
                let bodies: Vec<Option<&Expr>> = arms.iter().map(|arm| Some(&arm.body)).collect();
                self.walk_alternatives(&bodies, scopes, file);
            }
            Expr::Array { elements, .. } => {
                for element in elements {
                    self.reject_stored_sequence(element, "an array element", file);
                    self.walk_linear(element, scopes, file);
                }
            }
            Expr::Tuple { fields, .. } => {
                for (_, value) in fields {
                    self.reject_stored_sequence(value, "a tuple field", file);
                    self.walk_linear(value, scopes, file);
                }
            }
            Expr::DictLiteral { entries, .. } => {
                for (key, value) in entries {
                    self.reject_stored_sequence(key, "a dictionary key", file);
                    self.reject_stored_sequence(value, "a dictionary value", file);
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
            Expr::ClosureExpr {
                body, return_type, ..
            } => {
                self.reject_closure_sequence(return_type.as_ref(), body, file);
                self.walk_linear(body, scopes, file);
            }
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
                if let Some(ty) = ty {
                    self.reject_nested_sequence(ty, *span);
                }
                let declared = ty
                    .as_ref()
                    .map_or_else(|| self.infer_type_sem(value, file), SemType::from_ast);
                if let BindingPattern::Simple(ident) = pattern {
                    let holds_a_sequence = Self::is_sequence(&declared);
                    Self::bind(scopes, &ident.name.clone(), *span, holds_a_sequence);
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
