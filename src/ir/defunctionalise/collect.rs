//! Gather every closure value in the module, grouped by closure type.

use crate::ir::{walk_expr_children, walk_module, IrExpr, IrModule, IrVisitor, ResolvedType};

/// One lifted function that a closure value can name, with the env
/// struct type that carries its captures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Target {
    /// Name of the lifted top-level function.
    pub(super) funcref: String,
    /// Type of the env struct the lifted function takes first. `None`
    /// when the closure captured nothing and the env is absent.
    pub(super) env_ty: Option<ResolvedType>,
}

/// Every closure value of one closure type.
#[derive(Debug, Clone)]
pub(super) struct Shape {
    /// The closure type these values have.
    pub(super) closure_ty: ResolvedType,
    /// Lifted functions of this type, in first-seen order.
    pub(super) targets: Vec<Target>,
}

/// Collect the closure shapes in the module.
///
/// Order is the walk order, which `walk_module` fixes, so the
/// synthesised names are deterministic across runs.
pub(super) fn closure_shapes(module: &IrModule) -> Vec<Shape> {
    let mut collector = Collector { shapes: Vec::new() };
    walk_module(&mut collector, module);
    collector.shapes
}

struct Collector {
    shapes: Vec<Shape>,
}

impl Collector {
    fn record(&mut self, closure_ty: &ResolvedType, target: Target) {
        let shape =
            if let Some(existing) = self.shapes.iter_mut().find(|s| s.closure_ty == *closure_ty) {
                existing
            } else {
                self.shapes.push(Shape {
                    closure_ty: closure_ty.clone(),
                    targets: Vec::new(),
                });
                // The push above guarantees a last element.
                let Some(last) = self.shapes.last_mut() else {
                    return;
                };
                last
            };
        // One lifted function can appear at several sites — a closure
        // stored twice, say — and it needs exactly one variant.
        if !shape.targets.contains(&target) {
            shape.targets.push(target);
        }
    }
}

impl Collector {
    /// Record a shape that has a call site but may have no values.
    fn record_shape(&mut self, closure_ty: &ResolvedType) {
        if !self.shapes.iter().any(|s| s.closure_ty == *closure_ty) {
            self.shapes.push(Shape {
                closure_ty: closure_ty.clone(),
                targets: Vec::new(),
            });
        }
    }
}

impl IrVisitor for Collector {
    fn visit_expr(&mut self, expr: &IrExpr) {
        // A call site names a shape too, and the shape needs its enum
        // and its dispatcher whether or not this module holds a value
        // of it. A function declared `fn apply(f: (I32) -> I32)` that
        // nothing calls yet has a `CallClosure` in its body and no
        // `ClosureRef` anywhere, so collecting only from values left
        // that call with nothing to dispatch through — and the pass
        // then failed its own post-condition with "indirect call
        // remains".
        //
        // The enum such a shape gets has no variants. Nothing can
        // construct one, so the call is unreachable, which is the
        // right answer for a callback nobody supplies.
        if let IrExpr::CallClosure { closure, .. } = expr {
            if let ResolvedType::Closure { .. } = closure.ty() {
                self.record_shape(closure.ty());
            }
        }
        if let IrExpr::ClosureRef {
            funcref,
            env_struct,
            ty,
            ..
        } = expr
        {
            if let Some(name) = funcref.last() {
                // A closure that captured nothing still gets an env
                // struct from closure conversion, but it is empty.
                // Keep the type either way: the variant holds whatever
                // the lifted function's first parameter expects. Only
                // an `Error` placeholder means there is nothing to
                // carry.
                let env_ty = match env_struct.ty() {
                    ResolvedType::Error => None,
                    ty @ (ResolvedType::Primitive(_)
                    | ResolvedType::Struct(_)
                    | ResolvedType::Trait(_)
                    | ResolvedType::Enum(_)
                    | ResolvedType::Tuple(_)
                    | ResolvedType::Generic { .. }
                    | ResolvedType::TypeParam(_)
                    | ResolvedType::External { .. }
                    | ResolvedType::Closure { .. }) => Some(ty.clone()),
                };
                self.record(
                    ty,
                    Target {
                        funcref: name.clone(),
                        env_ty,
                    },
                );
            }
        }
        walk_expr_children(self, expr);
    }
}
