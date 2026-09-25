//! A structural check of an `IrModule`.
//!
//! The compiler gives a backend an `IrModule` and a promise: each
//! expression carries its resolved type, and each id names something
//! that exists. `docs/developer/ir/expressions.md` writes the promise
//! down as the type contract. This module checks the promise, the way
//! the LLVM and Cranelift verifiers check theirs.
//!
//! The check is conservative. It reports two types as different only
//! when both are concrete and nominal: a primitive, a plain struct, or
//! a plain enum. A generic, an optional, a closure, a tuple and a type
//! parameter never conflict with anything here. So a report is a real
//! contradiction inside the module, not a gap in this checker.
//!
//! Two stages exist. Lowering writes placeholder `0` indices, and
//! `ResolveReferencesPass` writes the real ones. [`verify`] checks the
//! names and the types, which hold at every stage. [`verify_resolved`]
//! also checks the indices, which hold only after that pass.

#![allow(
    clippy::wildcard_enum_match_arm,
    clippy::too_many_lines,
    clippy::match_same_arms,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::missing_const_for_fn,
    clippy::needless_pass_by_value,
    clippy::option_if_let_else,
    clippy::collapsible_match,
    // A checker walks every variant; the catch-all arms name the
    // shapes it does not constrain.
)]

use std::collections::HashMap;

use formalang::ast::{BinaryOperator, Literal, NumberValue, PrimitiveType, UnaryOperator};
use formalang::ir::{
    DispatchKind, GenericBase, IrBlockStatement, IrEnumVariant, IrExpr, IrField, IrFunction,
    IrModule, ReferenceTarget, ResolvedType,
};

/// Which invariants apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Straight from lowering, or after a pass that keeps names.
    Lowered,
    /// After `ResolveReferencesPass`: every index is real.
    Resolved,
}

/// Every contradiction in `module`, checked by name and by type.
pub fn verify(module: &IrModule) -> Vec<String> {
    Verifier::new(module, Stage::Lowered).run()
}

/// Every contradiction in `module`, and also every wrong index.
pub fn verify_resolved(module: &IrModule) -> Vec<String> {
    Verifier::new(module, Stage::Resolved).run()
}

/// Panic with every contradiction in `module`. `what` names the
/// module in the report.
pub fn assert_well_formed(module: &IrModule, what: &str) {
    let problems = verify(module);
    assert!(
        problems.is_empty(),
        "{what}: the IR breaks its own contract ({} problem(s)):\n  {}",
        problems.len(),
        problems.join("\n  ")
    );
}

struct Verifier<'m> {
    module: &'m IrModule,
    stage: Stage,
    problems: Vec<String>,
    /// Where the walk is, for the report.
    place: String,
    scopes: Vec<HashMap<String, ResolvedType>>,
}

impl<'m> Verifier<'m> {
    fn new(module: &'m IrModule, stage: Stage) -> Self {
        Self {
            module,
            stage,
            problems: Vec::new(),
            place: String::new(),
            scopes: Vec::new(),
        }
    }

    fn report(&mut self, message: String) {
        let line = format!("{}: {message}", self.place);
        if !self.problems.contains(&line) {
            self.problems.push(line);
        }
    }

    /// A value wraps into an optional without an explicit node (see
    /// `docs/user/types.md`). After monomorphisation an optional is a
    /// plain enum, so the checker must still know it.
    fn is_optional(&self, ty: &ResolvedType) -> bool {
        let ResolvedType::Enum(id) = ty else {
            return false;
        };
        Some(*id) == self.module.prelude_optional_id()
            || self
                .module
                .get_enum(*id)
                .is_some_and(|e| e.name == "Optional" || e.name.starts_with("Optional__"))
    }

    fn name(&self, ty: &ResolvedType) -> String {
        ty.display_name(self.module)
    }

    fn run(mut self) -> Vec<String> {
        let module = self.module;
        for s in &module.structs {
            self.place = format!("struct {}", s.name);
            for f in &s.fields {
                self.field(f);
            }
        }
        for e in &module.enums {
            self.place = format!("enum {}", e.name);
            for v in &e.variants {
                for f in &v.fields {
                    self.field(f);
                }
            }
        }
        for l in &module.lets {
            self.place = format!("let {}", l.name);
            self.ty(&l.ty);
            self.scopes.push(HashMap::new());
            self.expr(&l.value);
            self.scopes.pop();
            self.agree(l.value.ty(), &l.ty, "the value of the let");
        }
        for f in &module.functions {
            self.place = format!("fn {}", f.name);
            self.function(f);
        }
        for (i, imp) in module.impls.iter().enumerate() {
            for f in &imp.functions {
                self.place = format!("impl #{i} fn {}", f.name);
                self.function(f);
            }
        }
        self.problems
    }

    fn field(&mut self, f: &IrField) {
        self.ty(&f.ty);
        if let Some(d) = &f.default {
            self.scopes.push(HashMap::new());
            self.expr(d);
            self.scopes.pop();
            self.agree(d.ty(), &f.ty, &format!("the default of field `{}`", f.name));
        }
    }

    fn function(&mut self, f: &IrFunction) {
        let mut scope = HashMap::new();
        for p in &f.params {
            // A `self` parameter carries no type. A type parameter never
            // conflicts, so it stands in for the receiver type.
            let t =
                p.ty.clone()
                    .unwrap_or_else(|| ResolvedType::TypeParam("Self".to_string()));
            self.ty(&t);
            scope.insert(p.name.clone(), t);
        }
        self.scopes.push(scope);
        for p in &f.params {
            if let (Some(d), Some(t)) = (&p.default, &p.ty) {
                self.expr(d);
                self.agree(d.ty(), t, &format!("the default of parameter `{}`", p.name));
            }
        }
        if let Some(r) = &f.return_type {
            self.ty(r);
        }
        if let Some(body) = &f.body {
            self.expr(body);
            if let Some(r) = &f.return_type {
                self.agree(body.ty(), r, "the body against the return type");
            }
        }
        self.scopes.pop();
    }

    // -----------------------------------------------------------------
    // Types
    // -----------------------------------------------------------------

    /// Every id in `ty` names a definition, and `ty` holds no `Error`.
    fn ty(&mut self, ty: &ResolvedType) {
        let m = self.module;
        match ty {
            ResolvedType::Struct(id) if m.get_struct(*id).is_none() => {
                self.report(format!("struct id {} is out of range", id.0));
            }
            ResolvedType::Enum(id) if m.get_enum(*id).is_none() => {
                self.report(format!("enum id {} is out of range", id.0));
            }
            ResolvedType::Trait(id) if m.get_trait(*id).is_none() => {
                self.report(format!("trait id {} is out of range", id.0));
            }
            ResolvedType::Generic { base, args } => {
                let ok = match base {
                    GenericBase::Struct(id) => m.get_struct(*id).is_some(),
                    GenericBase::Enum(id) => m.get_enum(*id).is_some(),
                    GenericBase::Trait(id) => m.get_trait(*id).is_some(),
                };
                if !ok {
                    self.report(format!("generic base {base:?} is out of range"));
                }
                for a in args {
                    self.ty(a);
                }
            }
            ResolvedType::Tuple(fields) => {
                for (_, t) in fields {
                    self.ty(t);
                }
            }
            ResolvedType::Closure {
                param_tys,
                return_ty,
            } => {
                for (_, t) in param_tys {
                    self.ty(t);
                }
                self.ty(return_ty);
            }
            ResolvedType::External { type_args, .. } => {
                for a in type_args {
                    self.ty(a);
                }
            }
            ResolvedType::Error => {
                self.report("an accepted module holds `ResolvedType::Error`".to_string());
            }
            _ => {}
        }
    }

    /// Report when `found` and `expected` are both concrete and differ.
    fn agree(&mut self, found: &ResolvedType, expected: &ResolvedType, what: &str) {
        if conflict(found, expected) && !self.is_optional(found) && !self.is_optional(expected) {
            let (f, e) = (self.name(found), self.name(expected));
            self.report(format!("{what} has type {f}, but {e} is expected"));
        }
    }

    // -----------------------------------------------------------------
    // Expressions
    // -----------------------------------------------------------------

    fn expr(&mut self, e: &IrExpr) {
        self.ty(e.ty());
        match e {
            IrExpr::Literal { value, ty, .. } => self.literal(value, ty),

            IrExpr::StructInst {
                struct_id,
                type_args,
                fields,
                ty,
                ..
            } => {
                for (_, _, v) in fields {
                    self.expr(v);
                }
                let Some(id) = struct_id else { return };
                let Some(s) = self.module.get_struct(*id) else {
                    self.report(format!("struct instance of out-of-range id {}", id.0));
                    return;
                };
                let base_matches = match ty {
                    ResolvedType::Struct(t) => t == id,
                    ResolvedType::Generic {
                        base: GenericBase::Struct(t),
                        ..
                    } => t == id,
                    _ => true,
                };
                if !base_matches {
                    let n = self.name(ty);
                    self.report(format!("an instance of `{}` has type {n}", s.name));
                }
                let args = generic_args(ty).unwrap_or(type_args);
                let params: Vec<String> = s.generic_params.iter().map(|p| p.name.clone()).collect();
                let what = format!("struct `{}`", s.name);
                let declared = s.fields.clone();
                self.fields(&what, &declared, fields, &params, args);
            }

            IrExpr::EnumInst {
                enum_id,
                variant,
                variant_idx,
                fields,
                ty,
                ..
            } => {
                for (_, _, v) in fields {
                    self.expr(v);
                }
                let Some(id) = enum_id.or_else(|| enum_of(ty)) else {
                    return;
                };
                let Some(en) = self.module.get_enum(id) else {
                    self.report(format!("enum instance of out-of-range id {}", id.0));
                    return;
                };
                if let Some(t) = enum_of(ty) {
                    if t != id {
                        let n = self.name(ty);
                        self.report(format!("an instance of `{}` has type {n}", en.name));
                    }
                }
                let Some(pos) = en.variants.iter().position(|v| v.name == *variant) else {
                    self.report(format!(
                        "enum `{}` has no variant `{variant}`, but an instance names it",
                        en.name
                    ));
                    return;
                };
                if self.stage == Stage::Resolved && variant_idx.0 as usize != pos {
                    self.report(format!(
                        "variant `{variant}` of `{}` is at index {pos}, but the instance says {}",
                        en.name, variant_idx.0
                    ));
                }
                let params: Vec<String> =
                    en.generic_params.iter().map(|p| p.name.clone()).collect();
                let args = generic_args(ty).map(<[_]>::to_vec).unwrap_or_default();
                let v: &IrEnumVariant = &en.variants[pos];
                let what = format!("variant `{}.{}`", en.name, v.name);
                let declared = v.fields.clone();
                self.fields(&what, &declared, fields, &params, &args);
            }

            IrExpr::Array { elements, ty, .. } => {
                let elem = self.module.array_element_ty(ty).cloned();
                for el in elements {
                    self.expr(el);
                    if let Some(t) = &elem {
                        self.agree(el.ty(), t, "an array element");
                    }
                }
            }

            IrExpr::Tuple { fields, ty, .. } => {
                for (_, v) in fields {
                    self.expr(v);
                }
                if let ResolvedType::Tuple(declared) = ty {
                    if declared.len() == fields.len() {
                        for ((n, v), (dn, dt)) in fields.iter().zip(declared) {
                            if n != dn {
                                self.report(format!(
                                    "tuple field `{n}` sits where its type says `{dn}`"
                                ));
                            }
                            self.agree(v.ty(), dt, &format!("tuple field `{n}`"));
                        }
                    } else {
                        self.report(format!(
                            "a tuple of {} field(s) has a type of {} field(s)",
                            fields.len(),
                            declared.len()
                        ));
                    }
                }
            }

            IrExpr::Reference { target, path, .. } => {
                self.reference(target, path);
                self.bound(target, path);
            }

            IrExpr::SelfFieldRef { .. } => {}

            IrExpr::FieldAccess {
                object,
                field,
                field_idx,
                ty,
                ..
            } => {
                self.expr(object);
                if let Some((declared, pos)) = self.field_of(object.ty(), field) {
                    self.agree(ty, &declared, &format!("a read of field `{field}`"));
                    if self.stage == Stage::Resolved && field_idx.0 as usize != pos {
                        self.report(format!(
                            "field `{field}` is at index {pos}, but the read says {}",
                            field_idx.0
                        ));
                    }
                }
            }

            IrExpr::LetRef { name, ty, .. } => {
                if let Some(bound) = self.lookup(name) {
                    self.agree(ty, &bound, &format!("a read of `{name}`"));
                }
            }

            IrExpr::BinaryOp {
                left,
                op,
                right,
                ty,
                ..
            } => {
                self.expr(left);
                self.expr(right);
                self.binary(left.ty(), *op, right.ty(), ty);
            }

            IrExpr::UnaryOp {
                op, operand, ty, ..
            } => {
                self.expr(operand);
                match op {
                    UnaryOperator::Neg => self.agree(operand.ty(), ty, "the operand of `-`"),
                    UnaryOperator::Not => {
                        self.agree(operand.ty(), &boolean(), "the operand of `!`");
                        self.agree(ty, &boolean(), "the result of `!`");
                    }
                    _ => {}
                }
            }

            IrExpr::If {
                condition,
                then_branch,
                else_branch,
                ty,
                ..
            } => {
                self.expr(condition);
                self.agree(condition.ty(), &boolean(), "the condition of an `if`");
                self.nested(then_branch);
                if let Some(e) = else_branch {
                    self.nested(e);
                    self.agree(then_branch.ty(), ty, "the `then` branch");
                    self.agree(e.ty(), ty, "the `else` branch");
                }
            }

            IrExpr::For {
                var,
                var_ty,
                collection,
                body,
                ..
            } => {
                self.expr(collection);
                let m = self.module;
                let c = collection.ty();
                let elem = m
                    .array_element_ty(c)
                    .or_else(|| m.seq_element_ty(c))
                    .or_else(|| m.range_element_ty(c))
                    .cloned();
                if let Some(t) = elem {
                    self.agree(var_ty, &t, &format!("loop variable `{var}`"));
                }
                self.scopes
                    .push(HashMap::from([(var.clone(), var_ty.clone())]));
                self.expr(body);
                self.scopes.pop();
            }

            IrExpr::Match {
                scrutinee,
                arms,
                ty,
                ..
            } => {
                self.expr(scrutinee);
                let sty = scrutinee.ty().clone();
                for arm in arms {
                    let mut scope = HashMap::new();
                    for (n, _, t) in &arm.bindings {
                        self.ty(t);
                        scope.insert(n.clone(), t.clone());
                    }
                    if !arm.is_wildcard {
                        self.arm(&sty, arm);
                    }
                    self.scopes.push(scope);
                    self.expr(&arm.body);
                    self.scopes.pop();
                    self.agree(arm.body.ty(), ty, &format!("the arm `{}`", arm.variant));
                }
            }

            IrExpr::FunctionCall {
                path,
                function_id,
                args,
                ty,
                ..
            } => {
                for (_, a) in args {
                    self.expr(a);
                }
                let Some(id) = function_id else { return };
                let Some(f) = self.module.get_function(*id) else {
                    self.report(format!(
                        "a call to `{}` names function id {}",
                        path.join("::"),
                        id.0
                    ));
                    return;
                };
                let last = path.last().map_or("", String::as_str);
                let qualified = f.name.rsplit("::").next() == Some(last);
                if f.name != last && f.name != path.join("::") && !qualified {
                    self.report(format!(
                        "a call to `{}` carries the id of `{}`",
                        path.join("::"),
                        f.name
                    ));
                    return;
                }
                let f = f.clone();
                self.call_args(&f, args);
                if f.generic_params.is_empty() {
                    if let Some(r) = &f.return_type {
                        self.agree(ty, r, &format!("the call to `{}`", f.name));
                    }
                }
            }

            IrExpr::CallClosure {
                closure, args, ty, ..
            } => {
                self.expr(closure);
                for (_, a) in args {
                    self.expr(a);
                }
                if let ResolvedType::Closure {
                    param_tys,
                    return_ty,
                } = closure.ty()
                {
                    if param_tys.len() != args.len() {
                        self.report(format!(
                            "a closure of {} parameter(s) gets {} argument(s)",
                            param_tys.len(),
                            args.len()
                        ));
                    }
                    for ((_, pt), (_, a)) in param_tys.iter().zip(args) {
                        self.agree(a.ty(), pt, "a closure argument");
                    }
                    self.agree(ty, return_ty, "the closure call");
                }
            }

            IrExpr::MethodCall {
                receiver,
                method,
                method_idx,
                args,
                dispatch,
                ..
            } => {
                self.expr(receiver);
                for (_, a) in args {
                    self.expr(a);
                }
                if let DispatchKind::Static { impl_id } = dispatch {
                    let Some(imp) = self.module.impls.get(impl_id.0 as usize) else {
                        self.report(format!("method `{method}` names impl id {}", impl_id.0));
                        return;
                    };
                    // Overloads share a name, so any function of that name
                    // at the index is a match.
                    let at_idx = imp
                        .functions
                        .get(method_idx.0 as usize)
                        .is_some_and(|f| f.name == *method);
                    let pos = if at_idx {
                        Some(method_idx.0 as usize)
                    } else {
                        imp.functions.iter().position(|f| f.name == *method)
                    };
                    match pos {
                        None => self.report(format!(
                            "method `{method}` dispatches to an impl that has no such method"
                        )),
                        Some(p) if self.stage == Stage::Resolved && p != method_idx.0 as usize => {
                            self.report(format!(
                                "method `{method}` is at index {p}, but the call says {}",
                                method_idx.0
                            ));
                        }
                        Some(_) => {}
                    }
                }
            }

            IrExpr::Closure {
                params,
                captures,
                body,
                ty,
                ..
            } => {
                let mut scope = HashMap::new();
                for (_, n, _, t) in captures {
                    scope.insert(n.clone(), t.clone());
                }
                for (_, _, n, t) in params {
                    self.ty(t);
                    scope.insert(n.clone(), t.clone());
                }
                self.scopes.push(scope);
                self.expr(body);
                self.scopes.pop();
                if let ResolvedType::Closure {
                    param_tys,
                    return_ty,
                } = ty
                {
                    if param_tys.len() != params.len() {
                        self.report(format!(
                            "a closure of {} parameter(s) has a type of {}",
                            params.len(),
                            param_tys.len()
                        ));
                    }
                    for ((_, _, n, t), (_, pt)) in params.iter().zip(param_tys) {
                        self.agree(t, pt, &format!("closure parameter `{n}`"));
                    }
                    self.agree(body.ty(), return_ty, "the closure body");
                }
            }

            IrExpr::ClosureRef {
                funcref,
                env_struct,
                ..
            } => {
                self.expr(env_struct);
                let name = funcref.join("::");
                let last = funcref.last().cloned().unwrap_or_default();
                let found = self
                    .module
                    .functions
                    .iter()
                    .any(|f| f.name == name || f.name == last);
                if !found {
                    self.report(format!("a closure reference names no function `{name}`"));
                }
            }

            IrExpr::DictLiteral { entries, ty, .. } => {
                let kv = self
                    .module
                    .dictionary_kv_ty(ty)
                    .map(|(k, v)| (k.clone(), v.clone()));
                for (k, v) in entries {
                    self.expr(k);
                    self.expr(v);
                    if let Some((kt, vt)) = &kv {
                        self.agree(k.ty(), kt, "a dictionary key");
                        self.agree(v.ty(), vt, "a dictionary value");
                    }
                }
            }

            IrExpr::DictAccess { dict, key, .. } => {
                self.expr(dict);
                self.expr(key);
            }

            IrExpr::Block {
                statements,
                result,
                ty,
                ..
            } => {
                self.scopes.push(HashMap::new());
                for s in statements {
                    self.statement(s);
                }
                self.expr(result);
                self.scopes.pop();
                self.agree(result.ty(), ty, "the result of a block");
            }
        }
    }

    fn nested(&mut self, e: &IrExpr) {
        self.scopes.push(HashMap::new());
        self.expr(e);
        self.scopes.pop();
    }

    fn statement(&mut self, s: &IrBlockStatement) {
        match s {
            IrBlockStatement::Let {
                name, ty, value, ..
            } => {
                self.expr(value);
                let bound = if let Some(t) = ty {
                    self.ty(t);
                    self.agree(value.ty(), t, &format!("the value of `let {name}`"));
                    t.clone()
                } else {
                    value.ty().clone()
                };
                if let Some(scope) = self.scopes.last_mut() {
                    scope.insert(name.clone(), bound);
                }
            }
            IrBlockStatement::Assign { target, value, .. } => {
                self.expr(target);
                self.expr(value);
                self.agree(value.ty(), target.ty(), "the value of an assignment");
            }
            IrBlockStatement::Expr(e) => self.expr(e),
        }
    }

    /// A one-name reference to a local or a parameter must name a
    /// binding in scope, or a module-level item.
    fn bound(&mut self, target: &ReferenceTarget, path: &[String]) {
        let [name] = path else { return };
        if !matches!(
            target,
            ReferenceTarget::Unresolved | ReferenceTarget::Local(_) | ReferenceTarget::Param(_)
        ) {
            return;
        }
        if self.lookup(name).is_some() {
            return;
        }
        let m = self.module;
        let global = m.lets.iter().any(|l| l.name == *name)
            || m.functions.iter().any(|f| f.name == *name)
            || m.structs.iter().any(|s| s.name == *name)
            || m.enums.iter().any(|e| e.name == *name)
            || m.traits.iter().any(|t| t.name == *name)
            || m.imports
                .iter()
                .any(|i| i.items.iter().any(|it| it.name == *name));
        if !global {
            self.report(format!(
                "`{name}` is read where no binding of that name is in scope"
            ));
        }
    }

    fn lookup(&self, name: &str) -> Option<ResolvedType> {
        self.scopes.iter().rev().find_map(|s| s.get(name).cloned())
    }

    fn literal(&mut self, value: &Literal, ty: &ResolvedType) {
        let prim = match ty {
            ResolvedType::Primitive(p) => Some(*p),
            _ => None,
        };
        match value {
            Literal::Number(n) => match (n.value, prim) {
                (NumberValue::Integer(v), Some(PrimitiveType::I32)) => {
                    if i32::try_from(v).is_err() {
                        self.report(format!("the I32 literal {v} does not fit in 32 bits"));
                    }
                }
                (NumberValue::Integer(v), Some(PrimitiveType::I64)) => {
                    if i64::try_from(v).is_err() {
                        self.report(format!("the I64 literal {v} does not fit in 64 bits"));
                    }
                }
                (NumberValue::Integer(_), Some(PrimitiveType::F32 | PrimitiveType::F64)) => {}
                (NumberValue::Float(f), Some(PrimitiveType::F32)) => {
                    if f.is_finite() && f.abs() > f64::from(f32::MAX) {
                        self.report(format!("the F32 literal {f} does not fit in 32 bits"));
                    }
                }
                (NumberValue::Float(_), Some(PrimitiveType::F64)) => {}
                (_, Some(p)) => {
                    self.report(format!("the number literal {:?} has type {p:?}", n.value));
                }
                (_, None) => {}
            },
            Literal::Boolean(_) => {
                if concrete(ty) && *ty != boolean() {
                    let n = self.name(ty);
                    self.report(format!("a boolean literal has type {n}"));
                }
            }
            Literal::String(_) => {
                if concrete(ty) && *ty != ResolvedType::Primitive(PrimitiveType::String) {
                    let n = self.name(ty);
                    self.report(format!("a string literal has type {n}"));
                }
            }
            Literal::Nil => {
                if concrete(ty) {
                    let n = self.name(ty);
                    self.report(format!("`nil` has the non-optional type {n}"));
                }
            }
            _ => {}
        }
    }

    fn binary(
        &mut self,
        l: &ResolvedType,
        op: BinaryOperator,
        r: &ResolvedType,
        ty: &ResolvedType,
    ) {
        let sym = format!("{op:?}");
        match op {
            BinaryOperator::Add
            | BinaryOperator::Sub
            | BinaryOperator::Mul
            | BinaryOperator::Div
            | BinaryOperator::Mod => {
                self.agree(r, l, &format!("the right operand of {sym}"));
                self.agree(ty, l, &format!("the result of {sym}"));
            }
            BinaryOperator::Lt
            | BinaryOperator::Gt
            | BinaryOperator::Le
            | BinaryOperator::Ge
            | BinaryOperator::Eq
            | BinaryOperator::Ne => {
                self.agree(r, l, &format!("the right operand of {sym}"));
                self.agree(ty, &boolean(), &format!("the result of {sym}"));
            }
            BinaryOperator::And | BinaryOperator::Or => {
                self.agree(l, &boolean(), &format!("the left operand of {sym}"));
                self.agree(r, &boolean(), &format!("the right operand of {sym}"));
                self.agree(ty, &boolean(), &format!("the result of {sym}"));
            }
            BinaryOperator::Range => self.agree(r, l, "the end of a range"),
            _ => {}
        }
    }

    fn reference(&mut self, target: &ReferenceTarget, path: &[String]) {
        let m = self.module;
        let p = path.join("::");
        let ok = match target {
            ReferenceTarget::Function(id) => m.get_function(*id).is_some(),
            ReferenceTarget::Struct(id) => m.get_struct(*id).is_some(),
            ReferenceTarget::Enum(id) => m.get_enum(*id).is_some(),
            ReferenceTarget::Trait(id) => m.get_trait(*id).is_some(),
            ReferenceTarget::ModuleLet(id) => (id.0 as usize) < m.lets.len(),
            ReferenceTarget::Unresolved => {
                if self.stage == Stage::Resolved {
                    self.report(format!("the reference `{p}` is still unresolved"));
                }
                true
            }
            _ => true,
        };
        if !ok {
            self.report(format!(
                "the reference `{p}` targets an out-of-range {target:?}"
            ));
        }
    }

    /// The declared type and the index of `field` on `ty`.
    fn field_of(&self, ty: &ResolvedType, field: &str) -> Option<(ResolvedType, usize)> {
        let m = self.module;
        let (fields, params, args): (&[IrField], Vec<String>, Vec<ResolvedType>) = match ty {
            ResolvedType::Struct(id) => {
                let s = m.get_struct(*id)?;
                (&s.fields, Vec::new(), Vec::new())
            }
            ResolvedType::Generic {
                base: GenericBase::Struct(id),
                args,
            } => {
                if m.is_prelude_struct(*id) {
                    return None;
                }
                let s = m.get_struct(*id)?;
                let params = s.generic_params.iter().map(|p| p.name.clone()).collect();
                (&s.fields, params, args.clone())
            }
            _ => return None,
        };
        let pos = fields.iter().position(|f| f.name == field)?;
        Some((subst(&fields[pos].ty, &params, &args), pos))
    }

    /// Check the written fields of an instance against the declaration.
    fn fields(
        &mut self,
        what: &str,
        declared: &[IrField],
        written: &[(String, formalang::ir::FieldIdx, IrExpr)],
        params: &[String],
        args: &[ResolvedType],
    ) {
        let mut seen: Vec<&str> = Vec::new();
        for (name, idx, value) in written {
            if seen.contains(&name.as_str()) {
                self.report(format!("{what} gets field `{name}` twice"));
            }
            seen.push(name);
            let Some(pos) = declared.iter().position(|f| f.name == *name) else {
                self.report(format!(
                    "{what} has no field `{name}`, but an instance sets it"
                ));
                continue;
            };
            if self.stage == Stage::Resolved && idx.0 as usize != pos {
                self.report(format!(
                    "field `{name}` of {what} is at index {pos}, but the instance says {}",
                    idx.0
                ));
            }
            let t = subst(&declared[pos].ty, params, args);
            self.agree(value.ty(), &t, &format!("field `{name}` of {what}"));
        }
        for f in declared {
            let required = f.default.is_none()
                && !f.optional
                && self.module.optional_inner_ty(&f.ty).is_none();
            if required && !seen.contains(&f.name.as_str()) {
                self.report(format!("{what} is built without its field `{}`", f.name));
            }
        }
    }

    fn call_args(&mut self, f: &IrFunction, args: &[(Option<String>, IrExpr)]) {
        if args.len() > f.params.len() {
            self.report(format!(
                "`{}` takes {} parameter(s), but a call gives {} argument(s)",
                f.name,
                f.params.len(),
                args.len()
            ));
            return;
        }
        let mut filled = vec![false; f.params.len()];
        for (i, (label, value)) in args.iter().enumerate() {
            let pos = label
                .as_ref()
                .and_then(|l| {
                    f.params
                        .iter()
                        .position(|p| p.external_label.as_ref() == Some(l) || p.name == *l)
                })
                .unwrap_or(i);
            if let Some(slot) = filled.get_mut(pos) {
                *slot = true;
            }
            if let Some(Some(t)) = f.params.get(pos).map(|p| p.ty.clone()) {
                self.agree(value.ty(), &t, &format!("an argument to `{}`", f.name));
            }
        }
        for (p, done) in f.params.iter().zip(filled) {
            if !done && p.default.is_none() && p.name != "self" {
                self.report(format!("a call to `{}` leaves out `{}`", f.name, p.name));
            }
        }
    }

    fn arm(&mut self, scrutinee: &ResolvedType, arm: &formalang::ir::IrMatchArm) {
        let Some(id) = enum_of(scrutinee) else { return };
        let Some(en) = self.module.get_enum(id) else {
            return;
        };
        let Some(pos) = en.variants.iter().position(|v| v.name == arm.variant) else {
            self.report(format!(
                "a match arm names `{}`, which enum `{}` does not have",
                arm.variant, en.name
            ));
            return;
        };
        if self.stage == Stage::Resolved && arm.variant_idx.0 as usize != pos {
            self.report(format!(
                "arm `{}` is at index {pos}, but the arm says {}",
                arm.variant, arm.variant_idx.0
            ));
        }
        let v = &en.variants[pos];
        if arm.bindings.len() > v.fields.len() {
            self.report(format!(
                "arm `{}` binds {} name(s), but the variant has {} field(s)",
                arm.variant,
                arm.bindings.len(),
                v.fields.len()
            ));
        }
        let params: Vec<String> = en.generic_params.iter().map(|p| p.name.clone()).collect();
        let args = generic_args(scrutinee)
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        let declared: Vec<ResolvedType> = v
            .fields
            .iter()
            .map(|f| subst(&f.ty, &params, &args))
            .collect();
        for ((n, _, t), d) in arm.bindings.iter().zip(declared) {
            self.agree(t, &d, &format!("binding `{n}` of arm `{}`", arm.variant));
        }
    }
}

fn boolean() -> ResolvedType {
    ResolvedType::Primitive(PrimitiveType::Boolean)
}

/// A type the checker may compare: a primitive other than `Never`, a
/// plain struct, or a plain enum.
fn concrete(ty: &ResolvedType) -> bool {
    match ty {
        ResolvedType::Primitive(p) => *p != PrimitiveType::Never,
        ResolvedType::Struct(_) | ResolvedType::Enum(_) => true,
        _ => false,
    }
}

fn conflict(a: &ResolvedType, b: &ResolvedType) -> bool {
    concrete(a) && concrete(b) && a != b
}

const fn enum_of(ty: &ResolvedType) -> Option<formalang::ir::EnumId> {
    match ty {
        ResolvedType::Enum(id) => Some(*id),
        ResolvedType::Generic {
            base: GenericBase::Enum(id),
            ..
        } => Some(*id),
        _ => None,
    }
}

fn generic_args(ty: &ResolvedType) -> Option<&[ResolvedType]> {
    match ty {
        ResolvedType::Generic { args, .. } => Some(args),
        _ => None,
    }
}

/// Put `args` in place of the type parameters `params` in `ty`. A
/// parameter with no argument stays a parameter, which never conflicts.
fn subst(ty: &ResolvedType, params: &[String], args: &[ResolvedType]) -> ResolvedType {
    match ty {
        ResolvedType::TypeParam(n) => params
            .iter()
            .position(|p| p == n)
            .and_then(|i| args.get(i))
            .cloned()
            .unwrap_or_else(|| ty.clone()),
        ResolvedType::Generic { base, args: inner } => ResolvedType::Generic {
            base: *base,
            args: inner.iter().map(|a| subst(a, params, args)).collect(),
        },
        ResolvedType::Tuple(f) => ResolvedType::Tuple(
            f.iter()
                .map(|(n, t)| (n.clone(), subst(t, params, args)))
                .collect(),
        ),
        other => other.clone(),
    }
}
