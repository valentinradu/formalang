//! Closure-conversion pass.
//!
//! Lifts each [`IrExpr::Closure`] in the module to a top-level
//! [`IrFunction`] paired with a synthesized capture-environment
//! [`IrStruct`], replacing the original expression with an
//! [`IrExpr::ClosureRef`] that names the lifted function and constructs
//! the env value at runtime. The body of every lifted function is
//! rewritten so that captured-name reads (`Reference` / `LetRef`) load
//! from the env struct's fields instead.
//!
//! After the pass runs, no [`IrExpr::Closure`] node remains in the
//! module. Backends targeting languages without first-class closures
//! (notably the WASM component-model backend) can then lower the
//! resulting `funcref` + env pair as a function pointer plus a heap-
//! allocated record.
//!
//! # Pipeline placement
//!
//! Run **after** [`MonomorphisePass`](crate::ir::MonomorphisePass) so
//! that capture types and closure parameter types are concrete (no
//! leftover [`ResolvedType::TypeParam`](crate::ir::ResolvedType)).
//! Run **before**
//! [`DeadCodeEliminationPass`](crate::ir::DeadCodeEliminationPass) so
//! that the env structs and lifted functions go through the same
//! reachability sweep as hand-written definitions.
//!
//! # Status
//!
//! In progress. The pass currently:
//! - synthesizes one capture-environment [`IrStruct`] per
//!   [`IrExpr::Closure`] (PR 2 mc3).
//!
//! Still TODO across later microcommits: lifted-function synthesis
//! (mc4), body capture rewrite (mc5), site rewrite to
//! [`IrExpr::ClosureRef`] (mc6), callsite rewrite (mc7), and the
//! post-pass invariant assertion (mc8).
//!
//! [`IrExpr::Closure`]: crate::ir::IrExpr::Closure
//! [`IrExpr::ClosureRef`]: crate::ir::IrExpr::ClosureRef
//! [`IrFunction`]: crate::ir::IrFunction
//! [`IrStruct`]: crate::ir::IrStruct

use crate::ast::{ParamConvention, Visibility};
use crate::error::CompilerError;
use crate::ir::{walk_module, IrExpr, IrField, IrModule, IrStruct, IrVisitor, ResolvedType};
use crate::pipeline::IrPass;

/// Closure-conversion pass.
///
/// See the module-level documentation in `src/ir/closure_conv.rs` for
/// the full algorithm and pipeline placement.
#[expect(
    clippy::exhaustive_structs,
    reason = "no fields planned; flag-typed knobs would be added explicitly later"
)]
#[derive(Debug, Clone, Default)]
pub struct ClosureConversionPass;

impl ClosureConversionPass {
    /// Create a new closure-conversion pass.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl IrPass for ClosureConversionPass {
    fn name(&self) -> &'static str {
        "closure-conversion"
    }

    fn run(&mut self, mut module: IrModule) -> Result<IrModule, Vec<CompilerError>> {
        // Phase 1 (mc3): collect every closure's capture list in a
        // deterministic walk order, synthesize one env struct per
        // closure, append to `module.structs`. Subsequent microcommits
        // will reuse this same walk order to correlate each closure
        // with its env struct when lifting bodies and rewriting sites.
        let mut collector = ClosureCollector::default();
        walk_module(&mut collector, &module);

        let mut next_idx: usize = 0;
        for captures in collector.captures {
            let name = allocate_env_struct_name(&module, &mut next_idx);
            let env_struct = synthesize_env_struct(name, &captures);
            module.structs.push(env_struct);
        }

        module.rebuild_indices();
        Ok(module)
    }
}

/// Visitor that records every closure's captures in module-walk order.
#[derive(Default)]
struct ClosureCollector {
    captures: Vec<Vec<(String, ParamConvention, ResolvedType)>>,
}

impl IrVisitor for ClosureCollector {
    fn visit_expr(&mut self, e: &IrExpr) {
        if let IrExpr::Closure { captures, .. } = e {
            self.captures.push(captures.clone());
        }
        crate::ir::walk_expr_children(self, e);
    }
}

/// Allocate a fresh `__ClosureEnv<N>` name not already used by an
/// existing struct in the module. The counter starts at `*next_idx`
/// and is advanced past the chosen index so successive calls keep
/// allocating uniquely.
fn allocate_env_struct_name(module: &IrModule, next_idx: &mut usize) -> String {
    loop {
        let candidate = format!("__ClosureEnv{n}", n = *next_idx);
        *next_idx = next_idx.saturating_add(1);
        if module.struct_id(&candidate).is_none() {
            return candidate;
        }
    }
}

/// Build the capture-environment struct for a closure with the given
/// captures. Each capture becomes a private field carrying the
/// captured value's type. The capture's [`ParamConvention`] is not
/// materialised on the field today; later microcommits (mc9) will
/// revisit how `Mut` captures preserve borrow semantics through the
/// env.
fn synthesize_env_struct(
    name: String,
    captures: &[(String, ParamConvention, ResolvedType)],
) -> IrStruct {
    let fields = captures
        .iter()
        .map(|(field_name, _convention, ty)| IrField {
            name: field_name.clone(),
            ty: ty.clone(),
            mutable: false,
            optional: false,
            default: None,
            doc: None,
        })
        .collect();

    IrStruct {
        name,
        visibility: Visibility::Private,
        traits: Vec::new(),
        fields,
        generic_params: Vec::new(),
        doc: Some(
            "Auto-generated capture environment for a lifted closure. Produced by `ClosureConversionPass`."
                .to_string(),
        ),
    }
}
