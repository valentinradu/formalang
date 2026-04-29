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
    BindingId, FieldIdx, IrExpr, IrFunction, IrLet, IrModule, MethodIdx, ReferenceTarget,
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
        // Snapshot the module-level symbol table; the resolver also reads
        // structs / enums / traits / impls during expression rewriting to
        // look up field / variant / method indices.
        let symbols = ModuleSymbols::build(&module);
        let mut errors: Vec<CompilerError> = Vec::new();

        // Functions / module-lets can be detached and walked freely against
        // the rest of `module`. For impl methods we need `module.impls`
        // visible during the walk (so `lookup_method_idx` works), so we
        // extract one method body at a time via `std::mem::take` instead
        // of detaching the whole impls vec.
        let mut functions = std::mem::take(&mut module.functions);
        let mut lets = std::mem::take(&mut module.lets);

        for func in &mut functions {
            resolve_function(func, &symbols, &module, &mut errors);
        }
        module.functions = functions;
        // Method bodies are extracted one-at-a-time via mem::replace so
        // `module.impls` stays attached for `lookup_method_idx` reads
        // during the body walk. Indexing is loop-bound by construction;
        // `.get_mut(...).unwrap_or_else(unreachable!())` would be the
        // panic-free spelling but clippy's `unreachable` ban turns that
        // into an `internal_error_type` push for an invariant the
        // surrounding pass has just established. The tighter form is
        // strictly safer.
        #[expect(
            clippy::indexing_slicing,
            reason = "indices come from the bounds of the just-read .len() calls"
        )]
        for impl_idx in 0..module.impls.len() {
            for fn_idx in 0..module.impls[impl_idx].functions.len() {
                let mut taken = std::mem::replace(
                    &mut module.impls[impl_idx].functions[fn_idx],
                    placeholder_function(),
                );
                resolve_function(&mut taken, &symbols, &module, &mut errors);
                module.impls[impl_idx].functions[fn_idx] = taken;
            }
        }
        for l in &mut lets {
            resolve_module_let(l, &symbols, &module, &mut errors);
        }
        module.lets = lets;

        if errors.is_empty() {
            Ok(module)
        } else {
            Err(errors)
        }
    }
}

mod lookups;
mod symbols;
mod walkers;

use lookups::{lookup_method_idx, struct_field_idx};
use symbols::ModuleSymbols;
use walkers::{module_prefix_of, resolve_block_stmt, resolve_match_arm, resolve_path};

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
    /// Errors collected during the walk. The pass surfaces these via
    /// its `Err` return when non-empty so callers see a real
    /// `CompilerError` rather than silent `Unresolved` placeholders.
    errors: &'a mut Vec<CompilerError>,
    /// Module prefix of the function being resolved (extracted from
    /// the qualified `IrFunction.name`, e.g. `"foo"` for
    /// `"foo::caller"`). Used to bias single-segment function-call
    /// resolution to the local module so intra-module calls win over
    /// same-named top-level functions, matching lexical scope.
    /// Empty string for top-level functions.
    module_prefix: String,
    next_id: u32,
    /// Stack of name → (`BindingId`, kind) frames. Each block / for /
    /// match-arm / closure body pushes a frame; lookup walks
    /// innermost-first.
    scopes: Vec<HashMap<String, (BindingId, BindingKind)>>,
}

impl<'a> FnResolver<'a> {
    fn new(
        symbols: &'a ModuleSymbols,
        module: &'a IrModule,
        errors: &'a mut Vec<CompilerError>,
        module_prefix: String,
    ) -> Self {
        Self {
            symbols,
            module,
            errors,
            module_prefix,
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

const fn placeholder_function() -> IrFunction {
    IrFunction {
        name: String::new(),
        generic_params: Vec::new(),
        params: Vec::new(),
        return_type: None,
        body: None,
        extern_abi: None,
        attributes: Vec::new(),
        doc: None,
    }
}

fn resolve_function(
    func: &mut IrFunction,
    symbols: &ModuleSymbols,
    module: &IrModule,
    errors: &mut Vec<CompilerError>,
) {
    let prefix = module_prefix_of(&func.name);
    let mut r = FnResolver::new(symbols, module, errors, prefix);
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

fn resolve_module_let(
    l: &mut IrLet,
    symbols: &ModuleSymbols,
    module: &IrModule,
    errors: &mut Vec<CompilerError>,
) {
    let prefix = module_prefix_of(&l.name);
    let mut r = FnResolver::new(symbols, module, errors, prefix);
    resolve_expr(&mut l.value, &mut r);
}

#[expect(
    clippy::too_many_lines,
    reason = "exhaustive walk over IrExpr variants"
)]
fn resolve_expr(expr: &mut IrExpr, r: &mut FnResolver<'_>) {
    match expr {
        IrExpr::Literal { .. } | IrExpr::SelfFieldRef { .. } => {}
        IrExpr::Reference { path, target, ty } => {
            *target = resolve_path(path, r);
            // Promote a remaining `Unresolved` to a typed
            // `UndefinedReference` error — but only when the upstream
            // didn't already mark the reference's type as `Error`,
            // which is how lowering signals "I already pushed a
            // CompilerError for this site". Without the gate we'd
            // double-count any unbound name.
            if matches!(target, ReferenceTarget::Unresolved) && !matches!(ty, ResolvedType::Error) {
                r.errors.push(CompilerError::UndefinedReference {
                    name: path.join("::"),
                    span: crate::location::Span::default(),
                });
            }
        }
        IrExpr::LetRef {
            name, binding_id, ..
        } => {
            if let Some((id, _)) = r.lookup(name) {
                *binding_id = id;
            }
        }
        IrExpr::FunctionCall {
            path,
            function_id,
            args,
            ..
        } => {
            if function_id.is_none() {
                *function_id = walkers::resolve_function_call_id(path, r);
            }
            for (_, arg) in args {
                resolve_expr(arg, r);
            }
        }
        IrExpr::MethodCall {
            receiver,
            method,
            method_idx,
            dispatch,
            args,
            ..
        } => {
            if let Some(idx) = lookup_method_idx(dispatch, method, r.module) {
                *method_idx = MethodIdx(idx);
            }
            resolve_expr(receiver, r);
            for (_, arg) in args {
                resolve_expr(arg, r);
            }
        }
        IrExpr::FieldAccess {
            object,
            field,
            field_idx,
            ..
        } => {
            resolve_expr(object, r);
            if let Some(idx) = struct_field_idx(object.ty(), field, r.module) {
                *field_idx = FieldIdx(idx);
            }
        }
        IrExpr::Tuple { fields, .. } => {
            for (_, fexpr) in fields {
                resolve_expr(fexpr, r);
            }
        }
        IrExpr::StructInst {
            struct_id, fields, ..
        } => {
            for (name, idx, fexpr) in fields.iter_mut() {
                resolve_expr(fexpr, r);
                if let Some(sid) = struct_id {
                    if let Some(found) = r
                        .module
                        .get_struct(*sid)
                        .and_then(|s| s.fields.iter().position(|f| f.name == *name))
                    {
                        #[expect(
                            clippy::cast_possible_truncation,
                            reason = "field count is bounded upstream"
                        )]
                        let new_idx = FieldIdx(found as u32);
                        *idx = new_idx;
                    }
                }
            }
        }
        IrExpr::EnumInst {
            enum_id,
            variant,
            variant_idx,
            fields,
            ..
        } => {
            if let Some(eid) = enum_id {
                if let Some(found) = r
                    .module
                    .get_enum(*eid)
                    .and_then(|e| e.variants.iter().position(|v| v.name == *variant))
                {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "variant count is bounded upstream"
                    )]
                    let new_idx = VariantIdx(found as u32);
                    *variant_idx = new_idx;
                }
            }
            for (fname, fidx, fexpr) in fields.iter_mut() {
                resolve_expr(fexpr, r);
                if let Some(eid) = enum_id {
                    if let Some(found_field) = r
                        .module
                        .get_enum(*eid)
                        .and_then(|e| {
                            e.variants
                                .iter()
                                .find(|v| v.name == *variant)
                                .map(|v| v.fields.iter().position(|f| f.name == *fname))
                        })
                        .flatten()
                    {
                        #[expect(
                            clippy::cast_possible_truncation,
                            reason = "field count is bounded upstream"
                        )]
                        let new_field_idx = FieldIdx(found_field as u32);
                        *fidx = new_field_idx;
                    }
                }
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
            var_binding_id,
            collection,
            body,
            ..
        } => {
            resolve_expr(collection, r);
            r.push_scope();
            let id = r.fresh();
            *var_binding_id = id;
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
            captures,
            body,
            ..
        } => {
            // Capture binding-id resolution: each capture's
            // `outer_binding_id` must point at the introducing
            // binding *in the enclosing scope*, which we look up
            // BEFORE pushing the closure's own scope frame.
            for (cap_bid, cap_name, _, _) in captures.iter_mut() {
                if let Some((id, _)) = r.lookup(cap_name) {
                    *cap_bid = id;
                }
            }
            r.push_scope();
            for (_, param_bid, name, _) in params.iter_mut() {
                let id = r.fresh();
                *param_bid = id;
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
