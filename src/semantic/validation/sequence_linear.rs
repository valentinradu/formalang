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
    /// branch, not from what the branch before it left behind. Walking
    /// them in sequence against shared state made a sequence read in
    /// two arms of one `match` — or in both halves of one `if` — look
    /// like two reads of the same value, and the second was rejected as
    /// `SeqUsedTwice`.
    ///
    /// The results merge the other way: a binding counts as consumed if
    /// any branch consumed it. That keeps the "never consumed" report
    /// quiet for a sequence that only one arm reads, which is the
    /// conservative direction — the rule is about a sequence nothing
    /// reads at all.
    fn walk_alternatives(&mut self, branches: &[&Expr], scopes: &mut Scopes, file: &File) {
        let before = scopes.clone();
        let mut merged = before.clone();

        for branch in branches {
            let mut state = before.clone();
            self.walk_linear(branch, &mut state, file);
            for (frame, after) in merged.iter_mut().zip(state.iter()) {
                for (name, binding) in frame.iter_mut() {
                    if binding.consumed_at.is_none() {
                        binding.consumed_at = after.get(name).and_then(|b| b.consumed_at);
                    }
                }
            }
        }

        *scopes = merged;
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
                let mut branches: Vec<&Expr> = vec![then_branch];
                if let Some(else_expr) = else_branch {
                    branches.push(else_expr);
                }
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
                let bodies: Vec<&Expr> = arms.iter().map(|arm| &arm.body).collect();
                self.walk_alternatives(&bodies, scopes, file);
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
