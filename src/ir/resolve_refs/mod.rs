//! `ResolveReferencesPass` — replace string-keyed lookups with typed IDs.
//!
//! Walks every function body and:
//!
//! - assigns a fresh per-function [`BindingId`] to every
//!   [`crate::ir::IrFunctionParam`] and every [`crate::ir::IrBlockStatement::Let`];
//! - rewrites each [`IrExpr::LetRef`] to carry the introducing
//!   binding's [`BindingId`];
//! - rewrites each [`IrExpr::Reference`] to carry a resolved
//!   [`crate::ir::ReferenceTarget`] (function / struct / enum / trait /
//!   module-let / function-local binding / parameter, or a
//!   `path`-keyed `External` placeholder for a reference into a module
//!   that is not linked);
//! - rewrites each [`crate::ir::IrMatchArm`] to carry the matched variant's
//!   [`crate::ir::VariantIdx`] within its scrutinee enum;
//! - fills the `FieldIdx` of each field read and each field of a
//!   struct or variant literal, and the `MethodIdx` of each method
//!   call.
//!
//! A generic struct or enum, such as `Box<I32>`, `I32?` or
//! `Maybe<I32>`, has its fields and variants on its base definition.
//! The pass looks there, so an arm on an optional gets the index of
//! `some` or `none`, not index 0.
//!
//! The pass also walks the default of each field of each struct, enum
//! variant and trait: a default can build a struct, call a function or
//! read a module `let`, so it needs the same ids.
//!
//! The pass is **idempotent** — running it twice produces the same
//! output as running it once. Backends that emit integer-indexed code
//! consume its output directly without re-resolving names.
//!
//! See `docs/developer/architecture/passes.md` for where it sits in a
//! pipeline.

use std::collections::HashMap;

use crate::error::CompilerError;
use crate::ir::{BindingId, IrExpr, IrFunction, IrLet, IrModule};
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
        resolve_field_defaults(&mut module, &symbols, &mut errors);

        // DP-8: post-resolution default substitution.
        // After the resolve walks above bind every previously-None
        // FunctionCall.function_id, walk every function body, every
        // impl method body, and every let initialiser one more time.
        // For each FunctionCall whose args list is shorter than the
        // resolved callee's non-self param count, append the missing
        // trailing defaults. Forward refs (callee defined later in
        // the same module) and cross-module calls land here — at
        // lowering time the function_id was None, so the lowerer's
        // DP-2 substitution skipped.
        substitute_forward_ref_defaults(&mut module);

        if errors.is_empty() {
            Ok(module)
        } else {
            Err(errors)
        }
    }
}

/// Walk every function body / impl method / module-let value in
/// `module` and fill any trailing missing default arguments on
/// `IrExpr::FunctionCall` sites whose `function_id` is now bound.
/// See DP-8 for the rationale.
fn substitute_forward_ref_defaults(module: &mut IrModule) {
    // Snapshot per-function default arrays so the borrow checker
    // doesn't fight us when we mutate one function's body while
    // reading another's params.
    let snapshot: Vec<Vec<Option<IrExpr>>> = module
        .functions
        .iter()
        .map(|f| {
            f.params
                .iter()
                .filter(|p| p.name != "self")
                .map(|p| p.default.clone())
                .collect()
        })
        .collect();

    let mut functions = std::mem::take(&mut module.functions);
    for func in &mut functions {
        if let Some(body) = &mut func.body {
            substitute_in_expr(body, &snapshot);
        }
    }
    module.functions = functions;

    let mut lets = std::mem::take(&mut module.lets);
    for l in &mut lets {
        substitute_in_expr(&mut l.value, &snapshot);
    }
    module.lets = lets;

    for impl_block in &mut module.impls {
        for func in &mut impl_block.functions {
            if let Some(body) = &mut func.body {
                substitute_in_expr(body, &snapshot);
            }
        }
    }
}

/// Recursive walk of an `IrExpr` that fills missing trailing default
/// arguments on every `FunctionCall` whose `function_id` is now Some.
/// Reads each callee's defaults from `default_snapshot`, indexed by
/// the callee's `FunctionId.0`.
fn substitute_in_expr(expr: &mut IrExpr, default_snapshot: &[Vec<Option<IrExpr>>]) {
    if let IrExpr::FunctionCall {
        function_id: Some(id),
        args,
        ..
    } = expr
    {
        if let Some(defaults) = default_snapshot.get(id.0 as usize) {
            let any_labeled = args.iter().any(|(l, _)| l.is_some());
            if !any_labeled && args.len() < defaults.len() {
                for default in defaults.iter().skip(args.len()) {
                    let Some(d) = default else { break };
                    args.push((None, d.clone()));
                }
            }
        }
    }
    crate::ir::walk_expr_children_mut(expr, &mut |child| {
        substitute_in_expr(child, default_snapshot);
    });
}

mod expr;
mod lookups;
mod symbols;
mod walkers;

use expr::resolve_expr;
use symbols::ModuleSymbols;
use walkers::module_prefix_of;

/// Whether a binding was introduced as a function parameter or as a
/// function-local `let` (or for-loop / match-arm / closure parameter,
/// all of which the `Local` arm covers — only top-level
/// [`crate::ir::IrFunctionParam`] entries are `Param`).
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

fn placeholder_function() -> IrFunction {
    IrFunction {
        visibility: crate::ast::Visibility::Private,
        name: String::new(),
        generic_params: Vec::new(),
        params: Vec::new(),
        return_type: None,
        body: None,
        extern_abi: None,
        attributes: Vec::new(),
        doc: None,
        span: crate::ir::IrSpan::default(),
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

/// Resolve the default of each field of each struct, enum variant and
/// trait. A default is an expression like any other: it can build a
/// struct, call a function or read a module `let`, so it needs the same
/// indexes and targets. Each default is taken out of the module for its
/// walk, so the rest of the module stays readable.
#[expect(
    clippy::indexing_slicing,
    reason = "indices come from the bounds of the just-read .len() calls"
)]
fn resolve_field_defaults(
    module: &mut IrModule,
    symbols: &ModuleSymbols,
    errors: &mut Vec<CompilerError>,
) {
    for s in 0..module.structs.len() {
        for f in 0..module.structs[s].fields.len() {
            let prefix = module_prefix_of(&module.structs[s].name);
            let taken = module.structs[s].fields[f].default.take();
            module.structs[s].fields[f].default =
                resolve_default(taken, prefix, symbols, module, errors);
        }
    }
    for e in 0..module.enums.len() {
        for v in 0..module.enums[e].variants.len() {
            for f in 0..module.enums[e].variants[v].fields.len() {
                let prefix = module_prefix_of(&module.enums[e].name);
                let taken = module.enums[e].variants[v].fields[f].default.take();
                module.enums[e].variants[v].fields[f].default =
                    resolve_default(taken, prefix, symbols, module, errors);
            }
        }
    }
    for t in 0..module.traits.len() {
        for f in 0..module.traits[t].fields.len() {
            let prefix = module_prefix_of(&module.traits[t].name);
            let taken = module.traits[t].fields[f].default.take();
            module.traits[t].fields[f].default =
                resolve_default(taken, prefix, symbols, module, errors);
        }
    }
}

fn resolve_default(
    default: Option<IrExpr>,
    prefix: String,
    symbols: &ModuleSymbols,
    module: &IrModule,
    errors: &mut Vec<CompilerError>,
) -> Option<IrExpr> {
    let mut expr = default?;
    let mut r = FnResolver::new(symbols, module, errors, prefix);
    resolve_expr(&mut expr, &mut r);
    Some(expr)
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
