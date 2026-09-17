//! A reference interpreter for the lowered IR.
//!
//! The compiler is a frontend: it hands a backend an `IrModule` and
//! stops. Nothing in the crate ever runs a `FormaLang` program, so
//! nothing checks what one *means* — only that it compiles and that
//! the IR is shaped right. The twenty example programs each end in a
//! `run_checks()` full of `assert(condition: ...)` calls that state
//! real expected values, and none of them ever executed.
//!
//! This evaluator closes that gap. It is test-only: it is not shipped,
//! it is not fast, and it is not a specification. It is a second
//! opinion. When it disagrees with an assertion a program makes, one
//! of the two is wrong and both are worth reading.
//!
//! It walks the IR that `compile_to_ir` produces, before any pass
//! runs. That IR still carries `IrExpr::Closure` with its capture
//! list, which a tree-walker handles directly; after closure
//! conversion the same program is harder to read, not easier.
//!
//! Lookups go by name rather than by id. Ids are what a real backend
//! uses, and `tests/metamorphic.rs` already checks that every one of
//! them is in range; repeating that here would only couple this file
//! to a pass it does not run.

#![allow(
    clippy::wildcard_enum_match_arm,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::arithmetic_side_effects,
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    clippy::option_if_let_else,
    clippy::match_same_arms,
    clippy::single_match_else,
    clippy::match_wildcard_for_single_variants,
    clippy::unused_self,
    clippy::float_cmp,
    // A reference evaluator: the value and operator matches end in a
    // catch-all that reports the shape it met, mixed arithmetic
    // promotes to f64 the way the constant folder does, and lengths
    // come back as the language's I32.
    //
    // `allow` rather than `expect`: this module is included into
    // several test binaries and a given lint fires in only some of
    // them, so `expect` reports itself unfulfilled in the rest.
)]

use std::collections::HashMap;
use std::rc::Rc;

use formalang::ast::{BinaryOperator, Literal, NumberValue, UnaryOperator};
use formalang::ir::{
    GenericBase, ImplTarget, IrBlockStatement, IrExpr, IrFunction, IrModule, ResolvedType,
};

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

/// A runtime value.
#[derive(Clone, Debug)]
pub enum Value {
    /// An integer of any width. Widths are a backend concern; the
    /// semantic analyser has already checked the literal fits.
    Int(i128),
    Float(f64),
    Bool(bool),
    Str(String),
    /// The absent value: `nil`, and the `none` an optional holds.
    Nil,
    Array(Vec<Self>),
    /// A dictionary, kept as pairs so any value can be a key.
    Dict(Vec<(Self, Self)>),
    /// A half-open integer range, `start..end`.
    Range(i128, i128),
    /// A sequence, materialised. The language says a sequence is lazy
    /// and consumed once; laziness is an efficiency property, and the
    /// linearity rule is enforced at compile time, so evaluating
    /// eagerly gives the same answers.
    Seq(Vec<Self>),
    Tuple(Vec<(String, Self)>),
    Struct {
        name: String,
        fields: Vec<(String, Self)>,
    },
    Enum {
        enum_name: String,
        variant: String,
        fields: Vec<(String, Self)>,
    },
    Closure(Rc<ClosureValue>),
    /// A closure after `ClosureConversionPass` has lifted it: the name
    /// of a top-level function, and the environment struct holding
    /// what it captured. Calling it calls that function with the
    /// environment as its first argument.
    Lifted {
        funcref: String,
        env: Rc<Self>,
    },
    /// The value of a call to a function with no return type.
    Unit,
}

/// A closure, with the values it captured.
#[derive(Debug)]
pub struct ClosureValue {
    params: Vec<String>,
    body: IrExpr,
    captured: Vec<(String, Value)>,
}

impl Value {
    /// The truth value, for a condition.
    fn as_bool(&self) -> Result<bool, Fault> {
        match self {
            Self::Bool(b) => Ok(*b),
            other => Err(Fault::Type(format!("expected a boolean, found {other:?}"))),
        }
    }

    pub fn as_int(&self) -> Result<i128, Fault> {
        match self {
            Self::Int(i) => Ok(*i),
            other => Err(Fault::Type(format!("expected an integer, found {other:?}"))),
        }
    }

    fn as_str(&self) -> Result<&str, Fault> {
        match self {
            Self::Str(s) => Ok(s.as_str()),
            other => Err(Fault::Type(format!("expected a string, found {other:?}"))),
        }
    }

    /// The elements of anything that can be walked.
    fn as_elements(&self) -> Result<Vec<Self>, Fault> {
        match self {
            Self::Array(items) | Self::Seq(items) => Ok(items.clone()),
            Self::Range(start, end) => Ok((*start..*end).map(Self::Int).collect()),
            Self::Dict(entries) => Ok(entries.iter().map(|(k, _)| k.clone()).collect()),
            other => Err(Fault::Type(format!("{other:?} cannot be iterated"))),
        }
    }

    /// Structural equality. Two values of different shapes are never
    /// equal rather than an error: `==` is total in the language.
    fn equals(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a == b,
            (Self::Int(a), Self::Float(b)) | (Self::Float(b), Self::Int(a)) => {
                (*a as f64 - *b).abs() < f64::EPSILON
            }
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Str(a), Self::Str(b)) => a == b,
            (Self::Nil, Self::Nil) | (Self::Unit, Self::Unit) => true,
            (Self::Array(a), Self::Array(b)) | (Self::Seq(a), Self::Seq(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.equals(y))
            }
            // A dictionary is a list of pairs here, and two literals
            // that spell the same mapping may list it in any order, so
            // the comparison is by lookup rather than position.
            (Self::Dict(a), Self::Dict(b)) => {
                a.len() == b.len()
                    && a.iter().all(|(key, value)| {
                        b.iter()
                            .any(|(other, w)| key.equals(other) && value.equals(w))
                    })
            }
            (Self::Range(a, b), Self::Range(c, d)) => a == c && b == d,
            (Self::Tuple(a), Self::Tuple(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|((n, x), (m, y))| n == m && x.equals(y))
            }
            (Self::Struct { name: a, fields: x }, Self::Struct { name: b, fields: y }) => {
                a == b
                    && x.len() == y.len()
                    && x.iter()
                        .zip(y.iter())
                        .all(|((n, v), (m, w))| n == m && v.equals(w))
            }
            (
                Self::Enum {
                    variant: a,
                    fields: x,
                    ..
                },
                Self::Enum {
                    variant: b,
                    fields: y,
                    ..
                },
            ) => {
                a == b
                    && x.len() == y.len()
                    && x.iter()
                        .zip(y.iter())
                        .all(|((n, v), (m, w))| n == m && v.equals(w))
            }
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Faults
// ---------------------------------------------------------------------------

/// Why evaluation stopped.
#[derive(Debug)]
pub enum Fault {
    /// The program's own `assert` failed. This is the interesting one:
    /// the compiler produced an IR that computes the wrong answer.
    AssertFailed,
    /// A type the evaluator did not expect. Usually the evaluator's
    /// gap, occasionally an ill-typed IR.
    Type(String),
    /// A name the evaluator could not resolve.
    Unresolved(String),
    /// A shape the evaluator does not implement yet.
    Unsupported(String),
}

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AssertFailed => write!(f, "an assert in the program failed"),
            Self::Type(m) => write!(f, "type fault: {m}"),
            Self::Unresolved(m) => write!(f, "unresolved: {m}"),
            Self::Unsupported(m) => write!(f, "not implemented by the interpreter: {m}"),
        }
    }
}

// ---------------------------------------------------------------------------
// The interpreter
// ---------------------------------------------------------------------------

/// A stack of name-to-value scopes.
#[derive(Default, Debug)]
struct Env {
    scopes: Vec<HashMap<String, Value>>,
}

impl Env {
    fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn define(&mut self, name: &str, value: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), value);
        }
    }

    fn get(&self, name: &str) -> Option<&Value> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    fn assign(&mut self, name: &str, value: Value) -> bool {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(slot) = scope.get_mut(name) {
                *slot = value;
                return true;
            }
        }
        false
    }
}

/// Evaluates a module.
pub struct Interpreter<'m> {
    module: &'m IrModule,
    env: Env,
    /// How many `assert` calls succeeded. A `run_checks` that asserts
    /// nothing is a test that proves nothing, so the caller checks
    /// this.
    pub asserts_passed: usize,
    /// Guards against a runaway recursion in the interpreted program.
    depth: usize,
}

/// The deepest call chain the evaluator follows. Recursion in a test
/// program is bounded; anything past this is a loop the interpreter
/// would not escape.
const MAX_DEPTH: usize = 512;

impl<'m> Interpreter<'m> {
    pub fn new(module: &'m IrModule) -> Self {
        Self {
            module,
            env: Env::default(),
            asserts_passed: 0,
            depth: 0,
        }
    }

    /// Run the named function with no arguments and return its value.
    pub fn run(&mut self, function: &str) -> Result<Value, Fault> {
        let Some(f) = self.module.functions.iter().find(|f| f.name == function) else {
            return Err(Fault::Unresolved(format!("function `{function}`")));
        };
        self.call_function(f, Vec::new())
    }

    /// Whether the module declares a function by that name with a body.
    pub fn has_function(&self, name: &str) -> bool {
        self.module
            .functions
            .iter()
            .any(|f| f.name == name && f.body.is_some())
    }

    fn call_function(
        &mut self,
        f: &IrFunction,
        args: Vec<(String, Value)>,
    ) -> Result<Value, Fault> {
        let Some(body) = f.body.clone() else {
            // An `extern fn` is the host's to provide. The examples
            // that declare one assert only that the call path works —
            // `call_host(x: 21) > 0` — so a plausible host is enough.
            return Ok(host_function(&f.name, &args, f.return_type.as_ref()));
        };

        self.depth = self.depth.saturating_add(1);
        if self.depth > MAX_DEPTH {
            self.depth = self.depth.saturating_sub(1);
            return Err(Fault::Unsupported(format!(
                "recursion deeper than {MAX_DEPTH} calls in `{}`",
                f.name
            )));
        }

        self.env.push();
        for (i, param) in f.params.iter().enumerate() {
            // Match by label first, then by position.
            let value = args
                .iter()
                .find(|(name, _)| {
                    *name == param.name
                        || param.external_label.as_ref().is_some_and(|l| *l == *name)
                })
                .map(|(_, v)| v.clone())
                .or_else(|| args.get(i).map(|(_, v)| v.clone()));

            let value = match value {
                Some(v) => v,
                None => match param.default.clone() {
                    Some(default) => self.eval(&default)?,
                    None => Value::Nil,
                },
            };
            self.env.define(&param.name, value);
        }

        let result = self.eval(&body);
        self.env.pop();
        self.depth = self.depth.saturating_sub(1);
        result
    }

    // -----------------------------------------------------------------
    // Expressions
    // -----------------------------------------------------------------
    fn eval(&mut self, expr: &IrExpr) -> Result<Value, Fault> {
        match expr {
            IrExpr::Literal { value, ty, .. } => Ok(literal_value(value, ty)),

            IrExpr::Array { elements, .. } => {
                let mut out = Vec::with_capacity(elements.len());
                for e in elements {
                    out.push(self.eval(e)?);
                }
                Ok(Value::Array(out))
            }

            IrExpr::DictLiteral { entries, .. } => {
                let mut out = Vec::with_capacity(entries.len());
                for (k, v) in entries {
                    out.push((self.eval(k)?, self.eval(v)?));
                }
                Ok(Value::Dict(out))
            }

            IrExpr::Tuple { fields, .. } => {
                let mut out = Vec::with_capacity(fields.len());
                for (name, e) in fields {
                    out.push((name.clone(), self.eval(e)?));
                }
                Ok(Value::Tuple(out))
            }

            IrExpr::StructInst {
                struct_id,
                fields,
                ty,
                ..
            } => {
                let mut out = Vec::with_capacity(fields.len());
                for (name, _, e) in fields {
                    out.push((name.clone(), self.eval(e)?));
                }

                // A field the instantiation leaves out takes the
                // default from the struct's own definition:
                // `StructInst.fields` lists only what the call site
                // wrote, so `Config()` arrives here with none at all.
                let defaults: Vec<(String, IrExpr)> = struct_id
                    .and_then(|id| self.module.get_struct(id))
                    .map(|s| {
                        s.fields
                            .iter()
                            .filter(|f| out.iter().all(|(n, _)| *n != f.name))
                            .filter_map(|f| f.default.clone().map(|d| (f.name.clone(), d)))
                            .collect()
                    })
                    .unwrap_or_default();
                for (name, default) in defaults {
                    let value = self.eval(&default)?;
                    out.push((name, value));
                }

                Ok(Value::Struct {
                    name: self.type_name(ty),
                    fields: out,
                })
            }

            IrExpr::EnumInst {
                variant,
                fields,
                ty,
                ..
            } => {
                let mut out = Vec::with_capacity(fields.len());
                for (name, _, e) in fields {
                    out.push((name.clone(), self.eval(e)?));
                }
                Ok(Value::Enum {
                    enum_name: self.type_name(ty),
                    variant: variant.clone(),
                    fields: out,
                })
            }

            IrExpr::Reference { path, .. } => {
                let name = path.last().map_or("", String::as_str);
                self.env
                    .get(name)
                    .cloned()
                    .map_or_else(|| Err(Fault::Unresolved(format!("reference `{name}`"))), Ok)
            }

            IrExpr::LetRef { name, .. } => self
                .env
                .get(name)
                .cloned()
                .map_or_else(|| Err(Fault::Unresolved(format!("binding `{name}`"))), Ok),

            IrExpr::SelfFieldRef { field, .. } => {
                let Some(Value::Struct { fields, .. }) = self.env.get("self").cloned() else {
                    return Err(Fault::Unresolved(format!("self.{field}")));
                };
                field_of(&fields, field)
            }

            IrExpr::FieldAccess { object, field, .. } => {
                let value = self.eval(object)?;
                match value {
                    Value::Struct { fields, .. } | Value::Enum { fields, .. } => {
                        field_of(&fields, field)
                    }
                    Value::Tuple(fields) => field_of(&fields, field),
                    other => Err(Fault::Type(format!(
                        "cannot read field `{field}` of {other:?}"
                    ))),
                }
            }

            IrExpr::BinaryOp {
                left, op, right, ..
            } => self.binary(left, *op, right),

            IrExpr::UnaryOp { op, operand, .. } => {
                let v = self.eval(operand)?;
                match (op, v) {
                    (UnaryOperator::Neg, Value::Int(i)) => Ok(Value::Int(-i)),
                    (UnaryOperator::Neg, Value::Float(f)) => Ok(Value::Float(-f)),
                    (UnaryOperator::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
                    (op, other) => Err(Fault::Type(format!("cannot apply {op:?} to {other:?}"))),
                }
            }

            IrExpr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                if self.eval(condition)?.as_bool()? {
                    self.eval(then_branch)
                } else {
                    match else_branch {
                        Some(e) => self.eval(e),
                        // An `if` with no `else` produces nothing when
                        // the condition is false, and "nothing" here is
                        // `nil`, not a unit value: the type of such an
                        // expression is optional. See
                        // `docs/user/control-flow.md`.
                        None => Ok(Value::Nil),
                    }
                }
            }

            IrExpr::For {
                var,
                collection,
                body,
                ..
            } => {
                let items = self.eval(collection)?.as_elements()?;
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    self.env.push();
                    self.env.define(var, item);
                    let produced = self.eval(body);
                    self.env.pop();
                    out.push(produced?);
                }
                Ok(Value::Seq(out))
            }

            IrExpr::Match {
                scrutinee, arms, ..
            } => {
                let value = self.eval(scrutinee)?;

                // An optional is not represented as an enum here: a
                // `T?` is either the value or `Nil`. `if let`
                // desugars to a match on `.some` / `.none`, so give
                // those arms the shape they expect.
                //
                // Which shape to use is decided by the scrutinee's
                // type. Reading the value could not tell
                // `Some(Colour.red)` from `Colour.red` — both are a
                // `Value::Enum` — and reading the arm names could not
                // tell the prelude's `Optional` from a user enum whose
                // variants happen to be called `some` and `none`.
                let matching_an_optional = self
                    .module
                    .prelude_optional_id()
                    .is_some_and(|optional| enum_id_of(scrutinee.ty()) == Some(optional));
                let (variant, fields) = if matching_an_optional {
                    match &value {
                        Value::Nil => ("none".to_string(), Vec::new()),
                        other => (
                            "some".to_string(),
                            vec![("value".to_string(), other.clone())],
                        ),
                    }
                } else {
                    match &value {
                        Value::Enum {
                            variant, fields, ..
                        } => (variant.clone(), fields.clone()),
                        Value::Nil => ("none".to_string(), Vec::new()),
                        other => (
                            "some".to_string(),
                            vec![("value".to_string(), other.clone())],
                        ),
                    }
                };
                let (variant, fields) = (&variant, &fields);

                for arm in arms {
                    if !arm.is_wildcard && arm.variant != *variant {
                        continue;
                    }
                    self.env.push();
                    // Payload bindings take the variant's fields in
                    // declaration order, which is how the pattern
                    // `.circle(r)` names them.
                    for (i, (name, _, _)) in arm.bindings.iter().enumerate() {
                        if let Some((_, value)) = fields.get(i) {
                            self.env.define(name, value.clone());
                        }
                    }
                    let result = self.eval(&arm.body);
                    self.env.pop();
                    return result;
                }
                Err(Fault::Unresolved(format!("no match arm for `{variant}`")))
            }

            IrExpr::Block {
                statements, result, ..
            } => {
                self.env.push();
                let outcome = self.block(statements, result);
                self.env.pop();
                outcome
            }

            IrExpr::Closure {
                params,
                captures,
                body,
                ..
            } => {
                let mut captured = Vec::with_capacity(captures.len());
                for (_, name, _, _) in captures {
                    if let Some(v) = self.env.get(name) {
                        captured.push((name.clone(), v.clone()));
                    }
                }
                Ok(Value::Closure(Rc::new(ClosureValue {
                    params: params.iter().map(|(_, _, n, _)| n.clone()).collect(),
                    body: (**body).clone(),
                    captured,
                })))
            }

            IrExpr::CallClosure { closure, args, .. } => {
                let callee = self.eval(closure)?;
                let mut values = Vec::with_capacity(args.len());
                for (name, e) in args {
                    values.push((name.clone().unwrap_or_default(), self.eval(e)?));
                }
                self.call_closure(&callee, values)
            }

            IrExpr::FunctionCall {
                path,
                function_id,
                args,
                ..
            } => {
                let name = path.last().map_or("", String::as_str);
                let mut values = Vec::with_capacity(args.len());
                for (label, e) in args {
                    values.push((label.clone().unwrap_or_default(), self.eval(e)?));
                }
                // Call through the id the compiler resolved, not by
                // name. That is what a backend does, so a call that
                // names the wrong overload shows up here instead of
                // being papered over by a second name lookup.
                if let Some(f) = function_id.and_then(|id| self.module.get_function(id)) {
                    if name != "assert" {
                        let f = f.clone();
                        return self.call_function(&f, values);
                    }
                }
                self.call_named(name, values)
            }

            IrExpr::MethodCall {
                receiver,
                method,
                args,
                ..
            } => {
                let recv = self.eval(receiver)?;
                let mut values = Vec::with_capacity(args.len());
                for (label, e) in args {
                    values.push((label.clone().unwrap_or_default(), self.eval(e)?));
                }
                self.call_method(recv, method, values)
            }

            IrExpr::DictAccess { dict, key, .. } => {
                let container = self.eval(dict)?;
                let k = self.eval(key)?;
                Ok(index_into(&container, &k))
            }

            IrExpr::ClosureRef {
                funcref,
                env_struct,
                ..
            } => {
                let env = self.eval(env_struct)?;
                let Some(name) = funcref.last() else {
                    return Err(Fault::Unresolved("a ClosureRef with no name".to_string()));
                };
                Ok(Value::Lifted {
                    funcref: name.clone(),
                    env: Rc::new(env),
                })
            }
        }
    }

    fn block(&mut self, statements: &[IrBlockStatement], result: &IrExpr) -> Result<Value, Fault> {
        for stmt in statements {
            match stmt {
                IrBlockStatement::Let { name, value, .. } => {
                    let v = self.eval(value)?;
                    self.env.define(name, v);
                }
                IrBlockStatement::Assign { target, value, .. } => {
                    let v = self.eval(value)?;
                    match target {
                        IrExpr::LetRef { name, .. } => {
                            if !self.env.assign(name, v) {
                                return Err(Fault::Unresolved(format!("assignment to `{name}`")));
                            }
                        }
                        IrExpr::Reference { path, .. } => {
                            let name = path.last().map_or("", String::as_str);
                            if !self.env.assign(name, v) {
                                return Err(Fault::Unresolved(format!("assignment to `{name}`")));
                            }
                        }
                        other => {
                            return Err(Fault::Unsupported(format!("assignment target {other:?}")))
                        }
                    }
                }
                IrBlockStatement::Expr(e) => {
                    self.eval(e)?;
                }
            }
        }
        self.eval(result)
    }

    fn binary(
        &mut self,
        left: &IrExpr,
        op: BinaryOperator,
        right: &IrExpr,
    ) -> Result<Value, Fault> {
        // Short-circuit before evaluating the right side.
        if matches!(op, BinaryOperator::And | BinaryOperator::Or) {
            let l = self.eval(left)?.as_bool()?;
            return match op {
                BinaryOperator::And if !l => Ok(Value::Bool(false)),
                BinaryOperator::Or if l => Ok(Value::Bool(true)),
                _ => Ok(Value::Bool(self.eval(right)?.as_bool()?)),
            };
        }

        let l = self.eval(left)?;
        let r = self.eval(right)?;

        match op {
            BinaryOperator::Eq => return Ok(Value::Bool(l.equals(&r))),
            BinaryOperator::Ne => return Ok(Value::Bool(!l.equals(&r))),
            BinaryOperator::Range => {
                return Ok(Value::Range(l.as_int()?, r.as_int()?));
            }
            _ => {}
        }

        // String concatenation is the one non-numeric arithmetic.
        if let (Value::Str(a), Value::Str(b), BinaryOperator::Add) = (&l, &r, op) {
            return Ok(Value::Str(format!("{a}{b}")));
        }

        if let (Value::Int(a), Value::Int(b)) = (&l, &r) {
            integer_op(*a, op, *b)
        } else {
            let a = numeric(&l)?;
            let b = numeric(&r)?;
            float_op(a, op, b)
        }
    }

    // -----------------------------------------------------------------
    // Calls
    // -----------------------------------------------------------------

    fn call_named(&mut self, name: &str, args: Vec<(String, Value)>) -> Result<Value, Fault> {
        if name == "assert" {
            let condition = args.first().map_or(Value::Bool(false), |(_, v)| v.clone());
            return if condition.as_bool()? {
                self.asserts_passed = self.asserts_passed.saturating_add(1);
                Ok(Value::Unit)
            } else {
                Err(Fault::AssertFailed)
            };
        }

        // A local binding holding a closure shadows a function of the
        // same name: `let f = (n: I32) -> n` then `f(1)`.
        if let Some(value) = self.env.get(name).cloned() {
            if matches!(value, Value::Closure(_)) {
                return self.call_closure(&value, args);
            }
        }

        let Some(f) = self
            .module
            .functions
            .iter()
            .find(|f| f.name == name || f.name.ends_with(&format!("::{name}")))
        else {
            return Err(Fault::Unresolved(format!("function `{name}`")));
        };
        let f = f.clone();
        self.call_function(&f, args)
    }

    fn call_closure(&mut self, callee: &Value, args: Vec<(String, Value)>) -> Result<Value, Fault> {
        // A closure that `ClosureConversionPass` has lifted is an
        // ordinary function plus an environment. Calling it means
        // calling that function with the environment first, which is
        // the convention the pass documents on the generated
        // parameter: "The first parameter `__env` carries the
        // closure's captures."
        // A closure that `DefunctionalisePass` has replaced is an enum
        // tag: `__Fn0` with one variant per lifted body, each carrying
        // its environment. The pass also generates the dispatcher that
        // reads the tag — `__Fn0` is called through `__call_Fn0` — so
        // calling the value means calling that function with the tag
        // first. The dispatcher names its own parameters `f`, `p0`,
        // `p1`, which need not match the lifted body's, so the
        // arguments are bound in its order rather than by name.
        if let Value::Enum { enum_name, .. } = callee {
            if let Some(shape) = enum_name.strip_prefix("__") {
                let dispatcher = format!("__call_{shape}");
                let Some(f) = self
                    .module
                    .functions
                    .iter()
                    .find(|f| f.name == dispatcher)
                    .cloned()
                else {
                    return Err(Fault::Unresolved(format!(
                        "dispatcher `{dispatcher}` for a defunctionalised closure"
                    )));
                };
                let values =
                    std::iter::once(callee.clone()).chain(args.into_iter().map(|(_, v)| v));
                let bound: Vec<(String, Value)> = f
                    .params
                    .iter()
                    .map(|p| p.name.clone())
                    .zip(values)
                    .collect();
                return self.call_function(&f, bound);
            }
        }
        if let Value::Lifted { funcref, env } = callee {
            let name = funcref.clone();
            let environment = Rc::clone(env);
            let Some(f) = self
                .module
                .functions
                .iter()
                .find(|f| f.name == name)
                .cloned()
            else {
                return Err(Fault::Unresolved(format!("lifted closure `{name}`")));
            };
            let mut full = vec![("__env".to_string(), (*environment).clone())];
            full.extend(args);
            return self.call_function(&f, full);
        }
        let Value::Closure(c) = callee else {
            return Err(Fault::Type(format!("{callee:?} is not callable")));
        };
        let c = Rc::clone(c);

        self.depth = self.depth.saturating_add(1);
        if self.depth > MAX_DEPTH {
            self.depth = self.depth.saturating_sub(1);
            return Err(Fault::Unsupported(format!(
                "recursion deeper than {MAX_DEPTH} calls"
            )));
        }

        self.env.push();
        for (name, value) in &c.captured {
            self.env.define(name, value.clone());
        }
        for (i, param) in c.params.iter().enumerate() {
            let value = args
                .iter()
                .find(|(name, _)| name == param)
                .map(|(_, v)| v.clone())
                .or_else(|| args.get(i).map(|(_, v)| v.clone()))
                .unwrap_or(Value::Nil);
            self.env.define(param, value);
        }
        let result = self.eval(&c.body);
        self.env.pop();
        self.depth = self.depth.saturating_sub(1);
        result
    }

    fn call_method(
        &mut self,
        receiver: Value,
        method: &str,
        args: Vec<(String, Value)>,
    ) -> Result<Value, Fault> {
        // A closure held in a struct field is called through the
        // field: `form.onPress()`.
        if let Value::Struct { fields, .. } = &receiver {
            if let Some((_, value)) = fields.iter().find(|(n, _)| n == method) {
                if matches!(value, Value::Closure(_)) {
                    let value = value.clone();
                    return self.call_closure(&value, args);
                }
            }
        }

        if let Some(result) = self.builtin_method(&receiver, method, &args)? {
            return Ok(result);
        }

        let type_name = self.value_type_name(&receiver);
        let found = self.module.impls.iter().find_map(|imp| {
            if self.impl_target_name(imp.target) != type_name {
                return None;
            }
            // A bodyless method is an `extern impl` declaration; it
            // still resolves, and the host stands in for the body.
            imp.functions.iter().find(|f| f.name == method).cloned()
        });

        let Some(f) = found else {
            return Err(Fault::Unresolved(format!(
                "method `{method}` on `{type_name}`"
            )));
        };

        self.depth = self.depth.saturating_add(1);
        if self.depth > MAX_DEPTH {
            self.depth = self.depth.saturating_sub(1);
            return Err(Fault::Unsupported("recursion too deep".to_string()));
        }
        self.env.push();
        self.env.define("self", receiver);
        for (i, param) in f.params.iter().filter(|p| p.name != "self").enumerate() {
            let value = args
                .iter()
                .find(|(name, _)| {
                    *name == param.name
                        || param.external_label.as_ref().is_some_and(|l| *l == *name)
                })
                .map(|(_, v)| v.clone())
                .or_else(|| args.get(i).map(|(_, v)| v.clone()));
            let value = match value {
                Some(v) => v,
                None => match param.default.clone() {
                    Some(default) => self.eval(&default)?,
                    None => Value::Nil,
                },
            };
            self.env.define(&param.name, value);
        }
        let body = f.body.clone();
        let result = match body {
            Some(b) => self.eval(&b),
            // An `extern impl` method is the host's, like an
            // `extern fn`.
            None => Ok(host_function(method, &args, f.return_type.as_ref())),
        };
        self.env.pop();
        self.depth = self.depth.saturating_sub(1);
        result
    }

    // -----------------------------------------------------------------
    // The prelude's extern methods, implemented as host builtins
    // -----------------------------------------------------------------
    fn builtin_method(
        &mut self,
        receiver: &Value,
        method: &str,
        args: &[(String, Value)],
    ) -> Result<Option<Value>, Fault> {
        let arg = |i: usize| args.get(i).map(|(_, v)| v.clone());
        let named = |want: &str| args.iter().find(|(n, _)| n == want).map(|(_, v)| v.clone());

        let result = match (receiver, method) {
            // --- String ---
            (Value::Str(s), "len") => Value::Int(s.len() as i128),
            (Value::Str(s), "is_empty") => Value::Bool(s.is_empty()),
            (Value::Str(s), "slice") => {
                let start = named("start").or_else(|| arg(0)).unwrap_or(Value::Int(0));
                let end = named("end").or_else(|| arg(1)).unwrap_or(Value::Int(0));
                let (start, end) = (start.as_int()?, end.as_int()?);
                let from = usize::try_from(start.max(0)).unwrap_or(0).min(s.len());
                let to = usize::try_from(end.max(0)).unwrap_or(0).min(s.len());
                Value::Str(s.get(from..to.max(from)).unwrap_or("").to_string())
            }
            (Value::Str(s), "starts_with") => {
                let prefix = named("prefix")
                    .or_else(|| arg(0))
                    .unwrap_or(Value::Str(String::new()));
                Value::Bool(s.starts_with(prefix.as_str()?))
            }
            (Value::Str(s), "contains") => {
                let needle = named("needle")
                    .or_else(|| arg(0))
                    .unwrap_or(Value::Str(String::new()));
                Value::Bool(s.contains(needle.as_str()?))
            }
            (Value::Str(s), "byte_at") => {
                let i = named("i")
                    .or_else(|| arg(0))
                    .unwrap_or(Value::Int(0))
                    .as_int()?;
                let index = usize::try_from(i.max(0)).unwrap_or(0);
                Value::Int(s.as_bytes().get(index).map_or(-1, |b| i128::from(*b)))
            }

            // --- Array, Dictionary, Range: len / is_empty ---
            (Value::Array(items) | Value::Seq(items), "len") => Value::Int(items.len() as i128),
            (Value::Array(items) | Value::Seq(items), "is_empty") => Value::Bool(items.is_empty()),
            (Value::Dict(entries), "len") => Value::Int(entries.len() as i128),
            (Value::Dict(entries), "is_empty") => Value::Bool(entries.is_empty()),
            (Value::Range(a, b), "len") => Value::Int((b - a).max(0)),
            (Value::Range(a, b), "is_empty") => Value::Bool(b <= a),

            // --- Optional ---
            (Value::Nil, "is_some") => Value::Bool(false),
            (Value::Nil, "is_none") => Value::Bool(true),
            (_, "is_some") => Value::Bool(true),
            (_, "is_none") => Value::Bool(false),

            // --- Seq combinators ---
            (value, "collect") => Value::Array(value.as_elements()?),
            (value, "count") => Value::Int(value.as_elements()?.len() as i128),
            (value, "run") => {
                let _ = value.as_elements()?;
                Value::Unit
            }
            (value, "first") => value.as_elements()?.first().cloned().unwrap_or(Value::Nil),
            (value, "take") => {
                let n = named("count")
                    .or_else(|| arg(0))
                    .unwrap_or(Value::Int(0))
                    .as_int()?;
                let items = value.as_elements()?;
                let n = usize::try_from(n.max(0)).unwrap_or(0);
                Value::Seq(items.into_iter().take(n).collect())
            }
            (value, "skip") => {
                let n = named("count")
                    .or_else(|| arg(0))
                    .unwrap_or(Value::Int(0))
                    .as_int()?;
                let items = value.as_elements()?;
                let n = usize::try_from(n.max(0)).unwrap_or(0);
                Value::Seq(items.into_iter().skip(n).collect())
            }
            (value, "map") => {
                let f = named("f").or_else(|| arg(0)).unwrap_or(Value::Nil);
                let mut out = Vec::new();
                for item in value.as_elements()? {
                    out.push(self.call_closure(&f, vec![(String::new(), item)])?);
                }
                Value::Seq(out)
            }
            (value, "filter") => {
                let f = named("f").or_else(|| arg(0)).unwrap_or(Value::Nil);
                let mut out = Vec::new();
                for item in value.as_elements()? {
                    if self
                        .call_closure(&f, vec![(String::new(), item.clone())])?
                        .as_bool()?
                    {
                        out.push(item);
                    }
                }
                Value::Seq(out)
            }
            (value, "fold") => {
                let initial = named("initial").or_else(|| arg(0)).unwrap_or(Value::Int(0));
                let f = named("f").or_else(|| arg(1)).unwrap_or(Value::Nil);
                let mut acc = initial;
                for item in value.as_elements()? {
                    acc =
                        self.call_closure(&f, vec![(String::new(), acc), (String::new(), item)])?;
                }
                acc
            }
            (value, "any") => {
                let f = named("f").or_else(|| arg(0)).unwrap_or(Value::Nil);
                let mut found = false;
                for item in value.as_elements()? {
                    if self
                        .call_closure(&f, vec![(String::new(), item)])?
                        .as_bool()?
                    {
                        found = true;
                        break;
                    }
                }
                Value::Bool(found)
            }
            (value, "all") => {
                let f = named("f").or_else(|| arg(0)).unwrap_or(Value::Nil);
                let mut holds = true;
                for item in value.as_elements()? {
                    if !self
                        .call_closure(&f, vec![(String::new(), item)])?
                        .as_bool()?
                    {
                        holds = false;
                        break;
                    }
                }
                Value::Bool(holds)
            }

            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    // -----------------------------------------------------------------
    // Names
    // -----------------------------------------------------------------

    fn type_name(&self, ty: &ResolvedType) -> String {
        match ty {
            ResolvedType::Struct(id) => self
                .module
                .get_struct(*id)
                .map_or_else(|| "?".to_string(), |s| s.name.clone()),
            ResolvedType::Enum(id) => self
                .module
                .get_enum(*id)
                .map_or_else(|| "?".to_string(), |e| e.name.clone()),
            ResolvedType::Primitive(p) => format!("{p:?}"),
            ResolvedType::Generic { base, .. } => match base {
                formalang::ir::GenericBase::Struct(id) => self
                    .module
                    .get_struct(*id)
                    .map_or_else(|| "?".to_string(), |s| s.name.clone()),
                formalang::ir::GenericBase::Enum(id) => self
                    .module
                    .get_enum(*id)
                    .map_or_else(|| "?".to_string(), |e| e.name.clone()),
                other => format!("{other:?}"),
            },
            other => format!("{other:?}"),
        }
    }

    fn value_type_name(&self, value: &Value) -> String {
        match value {
            Value::Struct { name, .. } => name.clone(),
            Value::Enum { enum_name, .. } => enum_name.clone(),
            Value::Str(_) => "String".to_string(),
            Value::Int(_) => "I32".to_string(),
            Value::Float(_) => "F64".to_string(),
            Value::Bool(_) => "Boolean".to_string(),
            Value::Array(_) => "Array".to_string(),
            Value::Dict(_) => "Dictionary".to_string(),
            Value::Lifted { .. } => "Closure".to_string(),
            Value::Seq(_) => "Seq".to_string(),
            Value::Range(_, _) => "Range".to_string(),
            Value::Nil => "Optional".to_string(),
            Value::Tuple(_) => "Tuple".to_string(),
            Value::Closure(_) => "Closure".to_string(),
            Value::Unit => "Unit".to_string(),
        }
    }

    fn impl_target_name(&self, target: ImplTarget) -> String {
        match target {
            ImplTarget::Struct(id) => self
                .module
                .get_struct(id)
                .map_or_else(|| "?".to_string(), |s| s.name.clone()),
            ImplTarget::Enum(id) => self
                .module
                .get_enum(id)
                .map_or_else(|| "?".to_string(), |e| e.name.clone()),
            ImplTarget::Primitive(p) => format!("{p:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Stand in for a host-provided `extern` function.
///
/// The examples that declare one assert only that the call path works
/// — `assert(condition: call_host(x: 21) > 0)`, `area(c) >= 0` — so a
/// plausible answer of the declared type is enough. Returning the
/// first integer argument doubled, a true boolean, or an empty string
/// satisfies every such assertion without pretending to model a real
/// host.
fn host_function(
    name: &str,
    args: &[(String, Value)],
    return_type: Option<&ResolvedType>,
) -> Value {
    use formalang::ast::PrimitiveType;

    let first_int = args.iter().find_map(|(_, v)| match v {
        Value::Int(i) => Some(*i),
        _ => None,
    });

    match return_type {
        None => Value::Unit,
        Some(ResolvedType::Primitive(PrimitiveType::Boolean)) => Value::Bool(true),
        Some(ResolvedType::Primitive(PrimitiveType::String)) => Value::Str(String::new()),
        Some(ResolvedType::Primitive(PrimitiveType::F32 | PrimitiveType::F64)) => Value::Float(1.0),
        Some(ResolvedType::Primitive(_)) => {
            // `host_double` is the one the examples name, so doubling
            // the argument keeps its intent readable.
            Value::Int(first_int.map_or(1, |i| i.saturating_mul(2)))
        }
        Some(_) => {
            let _ = name;
            Value::Unit
        }
    }
}

fn literal_value(value: &Literal, ty: &ResolvedType) -> Value {
    match value {
        Literal::String(s) => Value::Str(s.clone()),
        Literal::Boolean(b) => Value::Bool(*b),
        Literal::Nil => Value::Nil,
        Literal::Number(n) => match n.value {
            NumberValue::Integer(i) => {
                // An integer literal in a float position is a float.
                if matches!(
                    ty,
                    ResolvedType::Primitive(
                        formalang::ast::PrimitiveType::F32 | formalang::ast::PrimitiveType::F64
                    )
                ) {
                    Value::Float(i as f64)
                } else {
                    Value::Int(i)
                }
            }
            NumberValue::Float(f) => Value::Float(f),
            _ => Value::Nil,
        },
        _ => Value::Nil,
    }
}

fn field_of(fields: &[(String, Value)], name: &str) -> Result<Value, Fault> {
    fields
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.clone())
        .map_or_else(|| Err(Fault::Unresolved(format!("field `{name}`"))), Ok)
}

/// Index a container. Out of range is `nil`, which is what the
/// language says: indexing returns `T?`.
fn index_into(container: &Value, key: &Value) -> Value {
    match container {
        Value::Array(items) | Value::Seq(items) => {
            let Value::Int(i) = key else {
                return Value::Nil;
            };
            usize::try_from(*i)
                .ok()
                .and_then(|i| items.get(i))
                .cloned()
                .unwrap_or(Value::Nil)
        }
        Value::Dict(entries) => entries
            .iter()
            .find(|(k, _)| k.equals(key))
            .map_or(Value::Nil, |(_, v)| v.clone()),
        Value::Str(s) => {
            let Value::Int(i) = key else {
                return Value::Nil;
            };
            usize::try_from(*i)
                .ok()
                .and_then(|i| s.chars().nth(i))
                .map_or(Value::Nil, |c| Value::Str(c.to_string()))
        }
        _ => Value::Nil,
    }
}

fn numeric(v: &Value) -> Result<f64, Fault> {
    match v {
        Value::Int(i) => Ok(*i as f64),
        Value::Float(f) => Ok(*f),
        other => Err(Fault::Type(format!("{other:?} is not a number"))),
    }
}

fn integer_op(a: i128, op: BinaryOperator, b: i128) -> Result<Value, Fault> {
    let value = match op {
        BinaryOperator::Add => Value::Int(a.wrapping_add(b)),
        BinaryOperator::Sub => Value::Int(a.wrapping_sub(b)),
        BinaryOperator::Mul => Value::Int(a.wrapping_mul(b)),
        BinaryOperator::Div => {
            if b == 0 {
                return Err(Fault::Type("division by zero".to_string()));
            }
            Value::Int(a.wrapping_div(b))
        }
        BinaryOperator::Mod => {
            if b == 0 {
                return Err(Fault::Type("modulo by zero".to_string()));
            }
            Value::Int(a.wrapping_rem(b))
        }
        BinaryOperator::Lt => Value::Bool(a < b),
        BinaryOperator::Gt => Value::Bool(a > b),
        BinaryOperator::Le => Value::Bool(a <= b),
        BinaryOperator::Ge => Value::Bool(a >= b),
        BinaryOperator::Eq => Value::Bool(a == b),
        BinaryOperator::Ne => Value::Bool(a != b),
        BinaryOperator::Range => Value::Range(a, b),
        BinaryOperator::And | BinaryOperator::Or => {
            return Err(Fault::Type("logical operator on integers".to_string()))
        }
        other => return Err(Fault::Unsupported(format!("{other:?} on integers"))),
    };
    Ok(value)
}

fn float_op(a: f64, op: BinaryOperator, b: f64) -> Result<Value, Fault> {
    let value = match op {
        BinaryOperator::Add => Value::Float(a + b),
        BinaryOperator::Sub => Value::Float(a - b),
        BinaryOperator::Mul => Value::Float(a * b),
        BinaryOperator::Div => Value::Float(a / b),
        BinaryOperator::Mod => Value::Float(a.rem_euclid(b).copysign(a)),
        BinaryOperator::Lt => Value::Bool(a < b),
        BinaryOperator::Gt => Value::Bool(a > b),
        BinaryOperator::Le => Value::Bool(a <= b),
        BinaryOperator::Ge => Value::Bool(a >= b),
        BinaryOperator::Eq => Value::Bool(a == b),
        BinaryOperator::Ne => Value::Bool(a != b),
        BinaryOperator::Range | BinaryOperator::And | BinaryOperator::Or => {
            return Err(Fault::Type(format!("{op:?} is not defined on floats")))
        }
        other => return Err(Fault::Unsupported(format!("{other:?} on floats"))),
    };
    Ok(value)
}

/// The enum a type names, if it names one.
///
/// A generic instantiation such as `Optional<I32>` carries the enum on
/// its base, so both shapes are unwrapped to the same id.
const fn enum_id_of(ty: &ResolvedType) -> Option<formalang::ir::EnumId> {
    match ty {
        ResolvedType::Enum(id) => Some(*id),
        ResolvedType::Generic {
            base: GenericBase::Enum(id),
            ..
        } => Some(*id),
        _ => None,
    }
}
