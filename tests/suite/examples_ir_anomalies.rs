//! Integration check: compile every `examples/*.fv` end to end through
//! the canonical pipeline and walk the resulting `IrModule` looking
//! for shapes that should never survive a clean compile.
//!
//! Anomalies we flag, grouped by class:
//!
//! Type-level
//! - `ResolvedType::Error` anywhere — upstream `CompilerError` should
//!   have aborted the compile.
//! - `ResolvedType::TypeParam(_)` outside the prelude built-in
//!   carriers — generic bodies must have been specialised away.
//! - `ResolvedType::Generic { … }` whose base is a non-prelude
//!   struct/enum — monomorphise should have rewritten it to its
//!   concrete clone.
//!
//! ID-level
//! - `Struct/Enum/Trait/Impl` ids equal to `u32::MAX` or out of bounds
//!   relative to the module's vectors. Sentinel values indicate the
//!   `TooManyDefinitions` abort path was bypassed.
//! - `IrExpr::LetRef.binding_id` whose `BindingId` doesn't match any
//!   binding defined in the enclosing function — `ResolveReferencesPass`
//!   left a slot at its placeholder value.
//!
//! Reference / dispatch
//! - `ReferenceTarget::Unresolved` on an `IrExpr::Reference`.
//! - `IrExpr::Reference.path` segments literally equal to "Unknown".
//!
//! Name-level
//! - Any struct/enum/trait/function/field/let whose name is "Unknown".
//!
//! Function-shape
//! - Non-extern function with `body: None`.
//!
//! The prelude-shipped built-ins (`Optional`, `Array`, `Seq`,
//! `Dictionary`, `Range`) are intentionally generic templates; their bodies are
//! skipped when scanning for `TypeParam` survivors and for in-bounds
//! checks on their own ids.

#![expect(
    clippy::expect_used,
    reason = "test asserts pipeline success; expect() is the desired panic-on-failure shape"
)]

use formalang::ir::{
    walk_block_statement, walk_expr_children, BindingId, DispatchKind, EnumId, GenericBase, ImplId,
    ImplTarget, IrBlockStatement, IrExpr, IrField, IrFunction, IrModule, IrVisitor,
    ReferenceTarget, ResolvedType, StructId, TraitId,
};
use formalang::{compile_to_ir, Pipeline};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::common::Checked;

/// The prelude's generic carriers. Their declarations are templates
/// that monomorphisation deliberately retains, so a `TypeParam` in one
/// of their signatures is correct rather than a leftover.
const fn is_prelude_builtin_name(name: &str) -> bool {
    matches!(
        name.as_bytes(),
        b"Array" | b"Seq" | b"Dictionary" | b"Range" | b"Optional"
    )
}

struct Anomalies<'m> {
    module: &'m IrModule,
    findings: Vec<String>,
    /// True while the visitor is inside a prelude-built-in struct or
    /// enum body. Generic carriers legitimately mention `TypeParam(T)`
    /// in their fields and method signatures; we mute those slots.
    in_prelude_builtin: bool,
    /// Binding-ids defined in the function currently being walked.
    /// Cleared on entry, populated from the function's params / let
    /// statements / match-arm bindings, then consulted on every
    /// `IrExpr::LetRef` and `ReferenceTarget::{Local, Param}`.
    defined_bindings: HashSet<u32>,
    current_fn: Option<String>,
}

impl<'m> Anomalies<'m> {
    fn new(module: &'m IrModule) -> Self {
        Self {
            module,
            findings: Vec::new(),
            in_prelude_builtin: false,
            defined_bindings: HashSet::new(),
            current_fn: None,
        }
    }

    fn note(&mut self, where_: &str, detail: impl AsRef<str>) {
        self.findings.push(format!("{where_}: {}", detail.as_ref()));
    }

    fn check_struct_id(&mut self, where_: &str, id: StructId) {
        if id.0 == u32::MAX {
            self.note(where_, "StructId(u32::MAX) sentinel");
            return;
        }
        if self.module.get_struct(id).is_none() {
            self.note(where_, format!("StructId({}) out of bounds", id.0));
        }
    }

    fn check_enum_id(&mut self, where_: &str, id: EnumId) {
        if id.0 == u32::MAX {
            self.note(where_, "EnumId(u32::MAX) sentinel");
            return;
        }
        if self.module.get_enum(id).is_none() {
            self.note(where_, format!("EnumId({}) out of bounds", id.0));
        }
    }

    fn check_trait_id(&mut self, where_: &str, id: TraitId) {
        if id.0 == u32::MAX {
            self.note(where_, "TraitId(u32::MAX) sentinel");
            return;
        }
        if self.module.get_trait(id).is_none() {
            self.note(where_, format!("TraitId({}) out of bounds", id.0));
        }
    }

    fn check_impl_id(&mut self, where_: &str, id: ImplId) {
        if id.0 == u32::MAX {
            self.note(where_, "ImplId(u32::MAX) sentinel");
            return;
        }
        if self.module.impls.get(id.0 as usize).is_none() {
            self.note(where_, format!("ImplId({}) out of bounds", id.0));
        }
    }

    fn check_binding_id(&mut self, where_: &str, id: BindingId) {
        if !self.defined_bindings.contains(&id.0) {
            let fn_label = self
                .current_fn
                .as_deref()
                .unwrap_or("<no enclosing function>");
            self.note(
                where_,
                format!("BindingId({}) not defined in fn `{fn_label}`", id.0),
            );
        }
    }

    fn check_type(&mut self, where_: &str, ty: &ResolvedType) {
        if self.in_prelude_builtin {
            return;
        }
        match ty {
            ResolvedType::Error => self.note(where_, "ResolvedType::Error"),
            ResolvedType::TypeParam(name) => {
                self.note(where_, format!("unresolved TypeParam(`{name}`)"));
            }
            ResolvedType::Generic { base, args } => {
                match base {
                    GenericBase::Struct(id) => self.check_struct_id(where_, *id),
                    GenericBase::Enum(id) => self.check_enum_id(where_, *id),
                    GenericBase::Trait(id) => {
                        self.note(where_, format!("Generic with Trait base id {}", id.0));
                    }
                }
                for a in args {
                    self.check_type(where_, a);
                }
            }
            ResolvedType::Tuple(fields) => {
                for (_, t) in fields {
                    self.check_type(where_, t);
                }
            }
            ResolvedType::Closure {
                param_tys,
                return_ty,
            } => {
                for (_, t) in param_tys {
                    self.check_type(where_, t);
                }
                self.check_type(where_, return_ty);
            }
            ResolvedType::External { type_args, .. } => {
                for a in type_args {
                    self.check_type(where_, a);
                }
            }
            ResolvedType::Struct(id) => self.check_struct_id(where_, *id),
            ResolvedType::Enum(id) => self.check_enum_id(where_, *id),
            ResolvedType::Trait(id) => self.check_trait_id(where_, *id),
            ResolvedType::Primitive(_) => {}
        }
    }

    fn check_dispatch(&mut self, where_: &str, d: &DispatchKind) {
        match d {
            DispatchKind::Static { impl_id } => self.check_impl_id(where_, *impl_id),
            DispatchKind::Virtual { trait_id, .. } => self.check_trait_id(where_, *trait_id),
        }
    }

    fn check_reference_target(&mut self, where_: &str, target: &ReferenceTarget) {
        match target {
            ReferenceTarget::Unresolved => {
                self.note(where_, "ReferenceTarget::Unresolved");
            }
            ReferenceTarget::Local(id) | ReferenceTarget::Param(id) => {
                self.check_binding_id(where_, *id);
            }
            ReferenceTarget::Struct(id) => self.check_struct_id(where_, *id),
            ReferenceTarget::Enum(id) => self.check_enum_id(where_, *id),
            ReferenceTarget::Trait(id) => self.check_trait_id(where_, *id),
            ReferenceTarget::Function(_) | ReferenceTarget::ModuleLet(_) => {}
            ReferenceTarget::External { name, .. } => {
                if name == "Unknown" {
                    self.note(where_, "External target name is \"Unknown\"");
                }
            }
        }
    }

    fn check_name(&mut self, where_: &str, name: &str) {
        if name == "Unknown" {
            self.note(where_, "name is \"Unknown\"");
        }
    }

    /// Pre-walk a function body to collect every binding-id defined by
    /// it. After `ResolveReferencesPass`, each `IrBlockStatement::Let`,
    /// `IrMatchArm` binding, and function parameter has a fresh
    /// per-function id. The visitor then uses this set to check that
    /// every `LetRef` / `Reference::Local` / `Reference::Param` points
    /// at one of those.
    fn collect_defined_bindings(f: &IrFunction, out: &mut HashSet<u32>) {
        for p in &f.params {
            out.insert(p.binding_id.0);
        }
        if let Some(body) = &f.body {
            collect_bindings_in_expr(body, out);
        }
    }
}

fn collect_bindings_in_expr(expr: &IrExpr, out: &mut HashSet<u32>) {
    match expr {
        IrExpr::Block {
            statements, result, ..
        } => {
            for stmt in statements {
                if let IrBlockStatement::Let { binding_id, .. } = stmt {
                    out.insert(binding_id.0);
                }
                walk_block_statement_for_bindings(stmt, out);
            }
            collect_bindings_in_expr(result, out);
        }
        IrExpr::Match {
            arms, scrutinee, ..
        } => {
            collect_bindings_in_expr(scrutinee, out);
            for arm in arms {
                for (_, bid, _) in &arm.bindings {
                    out.insert(bid.0);
                }
                collect_bindings_in_expr(&arm.body, out);
            }
        }
        IrExpr::For {
            var_binding_id,
            collection,
            body,
            ..
        } => {
            out.insert(var_binding_id.0);
            collect_bindings_in_expr(collection, out);
            collect_bindings_in_expr(body, out);
        }
        IrExpr::Closure {
            params,
            captures,
            body,
            ..
        } => {
            for (_, bid, _, _) in params {
                out.insert(bid.0);
            }
            for (bid, _, _, _) in captures {
                out.insert(bid.0);
            }
            collect_bindings_in_expr(body, out);
        }
        IrExpr::Literal { .. }
        | IrExpr::StructInst { .. }
        | IrExpr::EnumInst { .. }
        | IrExpr::Array { .. }
        | IrExpr::Tuple { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::FieldAccess { .. }
        | IrExpr::LetRef { .. }
        | IrExpr::BinaryOp { .. }
        | IrExpr::UnaryOp { .. }
        | IrExpr::If { .. }
        | IrExpr::FunctionCall { .. }
        | IrExpr::CallClosure { .. }
        | IrExpr::MethodCall { .. }
        | IrExpr::ClosureRef { .. }
        | IrExpr::DictLiteral { .. }
        | IrExpr::DictAccess { .. } => recurse_children(expr, out),
    }
}

fn walk_block_statement_for_bindings(stmt: &IrBlockStatement, out: &mut HashSet<u32>) {
    match stmt {
        IrBlockStatement::Let { value, .. } => collect_bindings_in_expr(value, out),
        IrBlockStatement::Assign { target, value, .. } => {
            collect_bindings_in_expr(target, out);
            collect_bindings_in_expr(value, out);
        }
        IrBlockStatement::Expr(e) => collect_bindings_in_expr(e, out),
    }
}

fn recurse_children(expr: &IrExpr, out: &mut HashSet<u32>) {
    match expr {
        IrExpr::Tuple { fields, .. } => {
            for (_, e) in fields {
                collect_bindings_in_expr(e, out);
            }
        }
        IrExpr::StructInst { fields, .. } | IrExpr::EnumInst { fields, .. } => {
            for (_, _, e) in fields {
                collect_bindings_in_expr(e, out);
            }
        }
        IrExpr::Array { elements, .. } => {
            for e in elements {
                collect_bindings_in_expr(e, out);
            }
        }
        IrExpr::FieldAccess { object, .. } => collect_bindings_in_expr(object, out),
        IrExpr::BinaryOp { left, right, .. } => {
            collect_bindings_in_expr(left, out);
            collect_bindings_in_expr(right, out);
        }
        IrExpr::UnaryOp { operand, .. } => collect_bindings_in_expr(operand, out),
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_bindings_in_expr(condition, out);
            collect_bindings_in_expr(then_branch, out);
            if let Some(eb) = else_branch {
                collect_bindings_in_expr(eb, out);
            }
        }
        IrExpr::FunctionCall { args, .. } => {
            for (_, e) in args {
                collect_bindings_in_expr(e, out);
            }
        }
        IrExpr::CallClosure { closure, args, .. } => {
            collect_bindings_in_expr(closure, out);
            for (_, e) in args {
                collect_bindings_in_expr(e, out);
            }
        }
        IrExpr::MethodCall { receiver, args, .. } => {
            collect_bindings_in_expr(receiver, out);
            for (_, e) in args {
                collect_bindings_in_expr(e, out);
            }
        }
        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                collect_bindings_in_expr(k, out);
                collect_bindings_in_expr(v, out);
            }
        }
        IrExpr::DictAccess { dict, key, .. } => {
            collect_bindings_in_expr(dict, out);
            collect_bindings_in_expr(key, out);
        }
        // The `Block / Match / For / Closure` cases are unreachable here:
        // they're handled in the top-level `collect_bindings_in_expr` match.
        // The remaining variants are leaves with no bindings to collect.
        IrExpr::ClosureRef { .. }
        | IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::LetRef { .. }
        | IrExpr::Block { .. }
        | IrExpr::Match { .. }
        | IrExpr::For { .. }
        | IrExpr::Closure { .. } => {}
    }
}

impl IrVisitor for Anomalies<'_> {
    fn visit_module(&mut self, module: &IrModule) {
        for s in &module.structs {
            self.check_name(&format!("struct `{}`", s.name), &s.name);
            let was = self.in_prelude_builtin;
            if is_prelude_builtin_name(&s.name) {
                self.in_prelude_builtin = true;
            }
            for field in &s.fields {
                self.visit_field(field);
            }
            self.in_prelude_builtin = was;
        }
        for t in &module.traits {
            self.check_name(&format!("trait `{}`", t.name), &t.name);
            for field in &t.fields {
                self.visit_field(field);
            }
            for sig in &t.methods {
                self.check_name(&format!("trait `{}` method", t.name), &sig.name);
            }
        }
        for e in &module.enums {
            self.check_name(&format!("enum `{}`", e.name), &e.name);
            let was = self.in_prelude_builtin;
            if is_prelude_builtin_name(&e.name) {
                self.in_prelude_builtin = true;
            }
            for v in &e.variants {
                self.check_name(&format!("variant in `{}`", e.name), &v.name);
                for field in &v.fields {
                    self.visit_field(field);
                }
            }
            self.in_prelude_builtin = was;
        }
        for imp in &module.impls {
            let on_builtin = match imp.target {
                ImplTarget::Struct(id) => {
                    self.check_struct_id("impl target", id);
                    module
                        .get_struct(id)
                        .is_some_and(|s| is_prelude_builtin_name(&s.name))
                }
                ImplTarget::Enum(id) => {
                    self.check_enum_id("impl target", id);
                    module
                        .get_enum(id)
                        .is_some_and(|e| is_prelude_builtin_name(&e.name))
                }
                ImplTarget::Primitive(_) => false,
            };
            let was = self.in_prelude_builtin;
            if on_builtin {
                self.in_prelude_builtin = true;
            }
            for f in &imp.functions {
                self.visit_function(f);
            }
            self.in_prelude_builtin = was;
        }
        for f in &module.functions {
            self.visit_function(f);
        }
        for l in &module.lets {
            self.visit_let(l);
        }
    }

    fn visit_field(&mut self, f: &IrField) {
        self.check_name(&format!("field `{}`", f.name), &f.name);
        self.check_type(&format!("field `{}`", f.name), &f.ty);
        if let Some(default) = &f.default {
            self.visit_expr(default);
        }
    }

    fn visit_function(&mut self, f: &IrFunction) {
        self.check_name(&format!("fn `{}`", f.name), &f.name);
        let saved_fn = self.current_fn.replace(f.name.clone());
        let saved_bindings = std::mem::take(&mut self.defined_bindings);
        Anomalies::collect_defined_bindings(f, &mut self.defined_bindings);

        for p in &f.params {
            self.check_name(&format!("fn `{}` param `{}`", f.name, p.name), &p.name);
            if let Some(ty) = &p.ty {
                self.check_type(&format!("fn `{}` param `{}`", f.name, p.name), ty);
            }
            if let Some(d) = &p.default {
                self.visit_expr(d);
            }
        }
        if let Some(ret) = &f.return_type {
            self.check_type(&format!("fn `{}` return", f.name), ret);
        }
        if !self.in_prelude_builtin && f.body.is_none() && f.extern_abi.is_none() {
            self.note(
                &format!("fn `{}`", f.name),
                "non-extern function with no body",
            );
        }
        if let Some(body) = &f.body {
            self.visit_expr(body);
        }
        self.defined_bindings = saved_bindings;
        self.current_fn = saved_fn;
    }

    fn visit_let(&mut self, l: &formalang::ir::IrLet) {
        self.check_name(&format!("let `{}`", l.name), &l.name);
        self.check_type(&format!("let `{}`", l.name), &l.ty);
        let saved_fn = self.current_fn.replace(format!("module-let `{}`", l.name));
        let saved = std::mem::take(&mut self.defined_bindings);
        collect_bindings_in_expr(&l.value, &mut self.defined_bindings);
        self.visit_expr(&l.value);
        self.defined_bindings = saved;
        self.current_fn = saved_fn;
    }

    fn visit_expr(&mut self, e: &IrExpr) {
        self.check_type("expr", e.ty());
        match e {
            IrExpr::Reference { target, path, .. } => {
                self.check_reference_target("reference", target);
                for seg in path {
                    if seg == "Unknown" {
                        self.note("reference path", "segment is \"Unknown\"");
                    }
                }
            }
            IrExpr::LetRef {
                binding_id, name, ..
            } => {
                self.check_name(&format!("LetRef `{name}`"), name);
                self.check_binding_id(&format!("LetRef `{name}`"), *binding_id);
            }
            IrExpr::MethodCall { dispatch, .. } => {
                self.check_dispatch("MethodCall", dispatch);
            }
            IrExpr::Block { statements, .. } => {
                for stmt in statements {
                    if let IrBlockStatement::Let { ty: Some(ty), .. } = stmt {
                        self.check_type("let stmt ty", ty);
                    }
                    walk_block_statement(self, stmt);
                }
            }
            IrExpr::Literal { .. }
            | IrExpr::StructInst { .. }
            | IrExpr::EnumInst { .. }
            | IrExpr::Array { .. }
            | IrExpr::Tuple { .. }
            | IrExpr::SelfFieldRef { .. }
            | IrExpr::FieldAccess { .. }
            | IrExpr::BinaryOp { .. }
            | IrExpr::UnaryOp { .. }
            | IrExpr::If { .. }
            | IrExpr::For { .. }
            | IrExpr::Match { .. }
            | IrExpr::FunctionCall { .. }
            | IrExpr::CallClosure { .. }
            | IrExpr::Closure { .. }
            | IrExpr::ClosureRef { .. }
            | IrExpr::DictLiteral { .. }
            | IrExpr::DictAccess { .. } => {}
        }
        walk_expr_children(self, e);
    }
}

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples")
}

fn discover_examples() -> Vec<PathBuf> {
    let dir = examples_dir();
    let mut entries: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("examples dir readable")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "fv"))
        .collect();
    entries.sort();
    assert!(
        !entries.is_empty(),
        "no .fv files found in {}",
        dir.display()
    );
    entries
}

#[test]
fn every_example_compiles_with_no_ir_anomalies() {
    let mut total_failures: Vec<String> = Vec::new();
    let mut checked = Checked::new("examples scanned for IR anomalies", 20);
    for path in discover_examples() {
        let source = fs::read_to_string(&path).expect("example file readable");
        let module = match compile_to_ir(&source) {
            Ok(m) => m,
            Err(errors) => {
                total_failures.push(format!(
                    "{} did not compile: {} error(s) ({:?})",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    errors.len(),
                    errors,
                ));
                continue;
            }
        };
        let module = match Pipeline::for_codegen().run(module) {
            Ok(m) => m,
            Err(errors) => {
                total_failures.push(format!(
                    "{} pipeline failed: {} error(s) ({:?})",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    errors.len(),
                    errors,
                ));
                continue;
            }
        };
        let mut anomalies = Anomalies::new(&module);
        anomalies.visit_module(&module);
        checked.hit();
        if !anomalies.findings.is_empty() {
            total_failures.push(format!(
                "{}: {} anomalies\n  - {}",
                path.file_name().unwrap_or_default().to_string_lossy(),
                anomalies.findings.len(),
                anomalies.findings.join("\n  - "),
            ));
        }
    }
    assert!(
        total_failures.is_empty(),
        "examples produced IR anomalies:\n\n{}",
        total_failures.join("\n\n"),
    );
}
