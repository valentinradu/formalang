//! `ResolveReferencesPass` — replace string-keyed lookups with typed IDs.
//!
//! Walks every function body and:
//!
//! - assigns a fresh per-function [`BindingId`] to every
//!   [`IrFunctionParam`] and every [`IrBlockStatement::Let`];
//! - rewrites each [`IrExpr::LetRef`] to carry the introducing
//!   binding's [`BindingId`];
//! - rewrites each [`IrExpr::Reference`] to carry a resolved
//!   [`ReferenceTarget`] (function / struct / enum / trait /
//!   module-let / function-local binding / parameter, or a
//!   `path`-keyed `External` placeholder for cross-module references
//!   that haven't been linked yet);
//! - rewrites each [`IrMatchArm`] to carry the matched variant's
//!   [`VariantIdx`] within its scrutinee enum.
//!
//! The pass is **idempotent** — running it twice produces the same
//! output as running it once. Backends that emit integer-indexed code
//! consume its output directly without re-resolving names.
//!
//! See `docs/developer/resolve_references_pass.md` for the design.

use std::collections::HashMap;

use crate::error::CompilerError;
use crate::ir::{
    BindingId, IrBlockStatement, IrExpr, IrFunction, IrLet, IrMatchArm, IrModule, ReferenceTarget,
    ResolvedType, VariantIdx,
};
use crate::pipeline::IrPass;

/// IR pass that resolves every name-keyed reference into a typed ID.
///
/// Insert this pass between `MonomorphisePass` (which may synthesise
/// new functions and rewrite types) and `ClosureConversionPass` (which
/// inspects captures): running here means closure conversion sees fully
/// resolved bindings.
///
/// # Example
///
/// ```
/// use formalang::ir::ResolveReferencesPass;
/// use formalang::{compile_to_ir, IrPass};
///
/// let module = compile_to_ir("pub fn id(x: I32) -> I32 { x }").unwrap();
/// let mut pass = ResolveReferencesPass::new();
/// let resolved = pass.run(module).unwrap();
/// assert!(!resolved.functions.is_empty());
/// ```
#[derive(Debug, Default, Clone, Copy)]
#[expect(
    clippy::exhaustive_structs,
    reason = "stateless pass marker — constructed directly"
)]
pub struct ResolveReferencesPass;

impl ResolveReferencesPass {
    /// Construct a fresh pass instance. Equivalent to
    /// [`Self::default`].
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl IrPass for ResolveReferencesPass {
    fn name(&self) -> &'static str {
        "resolve-references"
    }

    fn run(&mut self, mut module: IrModule) -> Result<IrModule, Vec<CompilerError>> {
        // Snapshot the module-level symbol table before mutating bodies. The
        // pass needs `&module` for variant_idx lookups while it mutates
        // bodies, so we detach the mutable collections, walk them in
        // isolation, then reattach.
        let symbols = ModuleSymbols::build(&module);

        // Detach the bodies that need rewriting. Each detached collection
        // is processed against the still-attached parts of `module` (used
        // for variant_idx lookup against `module.enums`).
        let mut functions = std::mem::take(&mut module.functions);
        let mut impls = std::mem::take(&mut module.impls);
        let mut lets = std::mem::take(&mut module.lets);

        for func in &mut functions {
            resolve_function(func, &symbols, &module);
        }
        for imp in &mut impls {
            for func in &mut imp.functions {
                resolve_function(func, &symbols, &module);
            }
        }
        for l in &mut lets {
            resolve_module_let(l, &symbols, &module);
        }

        module.functions = functions;
        module.impls = impls;
        module.lets = lets;

        Ok(module)
    }
}

/// Module-scope symbol table — name → resolved target. Built once per
/// pass invocation. Currently a flat top-level lookup; nested-module
/// resolution is a follow-up.
struct ModuleSymbols {
    by_name: HashMap<String, ReferenceTarget>,
}

impl ModuleSymbols {
    fn build(module: &IrModule) -> Self {
        let mut by_name = HashMap::new();
        for (i, f) in module.functions.iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "function count is bounded by add_function's u32 ceiling upstream"
            )]
            by_name.insert(
                f.name.clone(),
                ReferenceTarget::Function(crate::ir::FunctionId(i as u32)),
            );
        }
        for (i, s) in module.structs.iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "struct count bounded upstream"
            )]
            by_name.insert(
                s.name.clone(),
                ReferenceTarget::Struct(crate::ir::StructId(i as u32)),
            );
        }
        for (i, e) in module.enums.iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "enum count bounded upstream"
            )]
            by_name.insert(
                e.name.clone(),
                ReferenceTarget::Enum(crate::ir::EnumId(i as u32)),
            );
        }
        for (i, t) in module.traits.iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "trait count bounded upstream"
            )]
            by_name.insert(
                t.name.clone(),
                ReferenceTarget::Trait(crate::ir::TraitId(i as u32)),
            );
        }
        for (i, l) in module.lets.iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "let count bounded upstream"
            )]
            by_name.insert(
                l.name.clone(),
                ReferenceTarget::ModuleLet(crate::ir::LetId(i as u32)),
            );
        }
        Self { by_name }
    }
}

/// Whether a binding was introduced as a function parameter or as a
/// function-local `let` (or for-loop / match-arm / closure parameter,
/// all of which the `Local` arm covers — only top-level
/// [`IrFunctionParam`] entries are `Param`).
#[derive(Copy, Clone)]
enum BindingKind {
    Param,
    Local,
}

/// Per-function rewriter — assigns `BindingId`s and walks the body
/// resolving references.
struct FnResolver<'a> {
    symbols: &'a ModuleSymbols,
    module: &'a IrModule,
    next_id: u32,
    /// Stack of name → (`BindingId`, kind) frames. Each block / for /
    /// match-arm / closure body pushes a frame; lookup walks
    /// innermost-first.
    scopes: Vec<HashMap<String, (BindingId, BindingKind)>>,
}

impl<'a> FnResolver<'a> {
    fn new(symbols: &'a ModuleSymbols, module: &'a IrModule) -> Self {
        Self {
            symbols,
            module,
            next_id: 0,
            scopes: vec![HashMap::new()],
        }
    }

    const fn fresh(&mut self) -> BindingId {
        let id = BindingId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    fn bind(&mut self, name: String, id: BindingId, kind: BindingKind) {
        if let Some(frame) = self.scopes.last_mut() {
            frame.insert(name, (id, kind));
        }
    }

    fn lookup(&self, name: &str) -> Option<(BindingId, BindingKind)> {
        self.scopes
            .iter()
            .rev()
            .find_map(|frame| frame.get(name).copied())
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }
}

fn resolve_function(func: &mut IrFunction, symbols: &ModuleSymbols, module: &IrModule) {
    let mut r = FnResolver::new(symbols, module);
    for param in &mut func.params {
        let id = r.fresh();
        param.binding_id = id;
        r.bind(param.name.clone(), id, BindingKind::Param);
        if let Some(default) = param.default.as_mut() {
            resolve_expr(default, &mut r);
        }
    }
    if let Some(body) = func.body.as_mut() {
        resolve_expr(body, &mut r);
    }
}

fn resolve_module_let(l: &mut IrLet, symbols: &ModuleSymbols, module: &IrModule) {
    let mut r = FnResolver::new(symbols, module);
    resolve_expr(&mut l.value, &mut r);
}

#[expect(
    clippy::too_many_lines,
    reason = "exhaustive walk over IrExpr variants"
)]
fn resolve_expr(expr: &mut IrExpr, r: &mut FnResolver<'_>) {
    match expr {
        IrExpr::Literal { .. } | IrExpr::SelfFieldRef { .. } => {}
        IrExpr::Reference { path, target, .. } => {
            *target = resolve_path(path, r);
        }
        IrExpr::LetRef {
            name, binding_id, ..
        } => {
            if let Some((id, _)) = r.lookup(name) {
                *binding_id = id;
            }
        }
        IrExpr::FunctionCall { args, .. } => {
            for (_, arg) in args {
                resolve_expr(arg, r);
            }
        }
        IrExpr::MethodCall { receiver, args, .. } => {
            resolve_expr(receiver, r);
            for (_, arg) in args {
                resolve_expr(arg, r);
            }
        }
        IrExpr::FieldAccess { object, .. } => {
            resolve_expr(object, r);
        }
        IrExpr::StructInst { fields, .. }
        | IrExpr::EnumInst { fields, .. }
        | IrExpr::Tuple { fields, .. } => {
            for (_, fexpr) in fields {
                resolve_expr(fexpr, r);
            }
        }
        IrExpr::Array { elements, .. } => {
            for e in elements {
                resolve_expr(e, r);
            }
        }
        IrExpr::BinaryOp { left, right, .. } => {
            resolve_expr(left, r);
            resolve_expr(right, r);
        }
        IrExpr::UnaryOp { operand, .. } => {
            resolve_expr(operand, r);
        }
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            resolve_expr(condition, r);
            r.push_scope();
            resolve_expr(then_branch, r);
            r.pop_scope();
            if let Some(eb) = else_branch.as_mut() {
                r.push_scope();
                resolve_expr(eb, r);
                r.pop_scope();
            }
        }
        IrExpr::For {
            var,
            collection,
            body,
            ..
        } => {
            resolve_expr(collection, r);
            r.push_scope();
            let id = r.fresh();
            r.bind(var.clone(), id, BindingKind::Local);
            resolve_expr(body, r);
            r.pop_scope();
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            resolve_expr(scrutinee, r);
            let scrutinee_ty = scrutinee.ty().clone();
            for arm in arms {
                resolve_match_arm(arm, &scrutinee_ty, r);
            }
        }
        IrExpr::Closure {
            params,
            captures: _,
            body,
            ..
        } => {
            r.push_scope();
            for (_, name, _) in params.iter() {
                let id = r.fresh();
                r.bind(name.clone(), id, BindingKind::Local);
            }
            resolve_expr(body, r);
            r.pop_scope();
        }
        IrExpr::ClosureRef { env_struct, .. } => {
            resolve_expr(env_struct, r);
        }
        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                resolve_expr(k, r);
                resolve_expr(v, r);
            }
        }
        IrExpr::DictAccess { dict, key, .. } => {
            resolve_expr(dict, r);
            resolve_expr(key, r);
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            r.push_scope();
            for stmt in statements {
                resolve_block_stmt(stmt, r);
            }
            resolve_expr(result, r);
            r.pop_scope();
        }
    }
}

fn resolve_block_stmt(stmt: &mut IrBlockStatement, r: &mut FnResolver<'_>) {
    match stmt {
        IrBlockStatement::Let {
            binding_id,
            name,
            value,
            ..
        } => {
            resolve_expr(value, r);
            let id = r.fresh();
            *binding_id = id;
            r.bind(name.clone(), id, BindingKind::Local);
        }
        IrBlockStatement::Assign { target, value } => {
            resolve_expr(target, r);
            resolve_expr(value, r);
        }
        IrBlockStatement::Expr(e) => resolve_expr(e, r),
    }
}

fn resolve_match_arm(arm: &mut IrMatchArm, scrutinee_ty: &ResolvedType, r: &mut FnResolver<'_>) {
    if !arm.is_wildcard {
        if let Some(idx) = match_variant_idx(scrutinee_ty, &arm.variant, r.module) {
            arm.variant_idx = VariantIdx(idx);
        }
    }
    r.push_scope();
    for (name, _ty) in &arm.bindings {
        let id = r.fresh();
        r.bind(name.clone(), id, BindingKind::Local);
    }
    resolve_expr(&mut arm.body, r);
    r.pop_scope();
}

fn match_variant_idx(scrutinee_ty: &ResolvedType, variant: &str, module: &IrModule) -> Option<u32> {
    let &ResolvedType::Enum(enum_id) = scrutinee_ty else {
        return None;
    };
    let e = module.get_enum(enum_id)?;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "variant count is bounded upstream"
    )]
    e.variants
        .iter()
        .position(|v| v.name == variant)
        .map(|i| i as u32)
}

fn resolve_path(path: &[String], r: &FnResolver<'_>) -> ReferenceTarget {
    if let [single] = path {
        if let Some((id, kind)) = r.lookup(single) {
            return match kind {
                BindingKind::Param => ReferenceTarget::Param(id),
                BindingKind::Local => ReferenceTarget::Local(id),
            };
        }
        if let Some(target) = r.symbols.by_name.get(single) {
            return target.clone();
        }
    } else if let [first, ..] = path {
        // Multi-segment path: try the first segment as a module-scope
        // symbol; leave nested-module / external resolution as a follow-up.
        if let Some(target) = r.symbols.by_name.get(first) {
            return target.clone();
        }
    }
    ReferenceTarget::Unresolved
}
