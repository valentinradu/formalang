//! Monomorphisation pass stub.
//!
//! `FormaLang`'s IR preserves generics after lowering: `ResolvedType::Generic`
//! and `ResolvedType::TypeParam` appear wherever the source had type
//! parameters. Most statically-typed code-generation targets (C, WGSL,
//! `TypeScript` with typed emission, Swift, Kotlin) cannot emit parametric
//! types directly — they need one concrete specialisation per instantiation.
//!
//! A full monomorphisation pass would:
//!
//! 1. Walk the IR collecting every concrete instantiation of each generic
//!    definition (e.g. `Box<User>`, `Box<Post>`).
//! 2. Clone each generic definition once per unique type-argument tuple,
//!    substituting type parameters for their concrete arguments.
//! 3. Rewrite references in the IR to point at the specialised copies.
//! 4. Remove the original generic definitions.
//!
//! The real pass is not implemented yet. This module provides a stub that
//! rejects modules containing any remaining generic types so backends fail
//! loudly (with a specific error) instead of silently emitting wrong code.
//!
//! # Usage
//!
//! ```no_run
//! use formalang::{compile_to_ir, Pipeline};
//! use formalang::ir::MonomorphisePass;
//!
//! let source = "pub struct Box<T> { value: T }";
//! let module = compile_to_ir(source).unwrap();
//!
//! // Running the stub on a generic module yields an InternalError.
//! let mut pipeline = Pipeline::new().pass(MonomorphisePass);
//! let result = pipeline.run(module);
//! assert!(result.is_err());
//! ```

use crate::error::CompilerError;
use crate::ir::{IrExpr, IrFunction, IrModule, IrVisitor, ResolvedType};
use crate::location::Span;
use crate::pipeline::IrPass;

/// Monomorphisation pass stub. See module docs.
///
/// The pass currently rejects any IR module that still contains generics
/// after lowering; it never rewrites the IR. Once a real implementation
/// lands, existing callers will automatically benefit without needing to
/// change the pipeline wiring.
#[expect(
    clippy::exhaustive_structs,
    reason = "zero-sized marker type; no fields to add"
)]
#[derive(Debug, Clone, Copy, Default)]
pub struct MonomorphisePass;

impl IrPass for MonomorphisePass {
    fn name(&self) -> &'static str {
        "monomorphise (stub)"
    }

    fn run(&mut self, module: IrModule) -> Result<IrModule, Vec<CompilerError>> {
        let mut detector = GenericDetector::new();
        detector.visit_module(&module);
        // Also scan function / struct / trait / enum signatures. The visitor
        // only walks expressions; generic types can show up in field/param
        // annotations as well, so walk the definitions directly.
        detector.scan_definitions(&module);

        if let Some(sample) = detector.first_sample() {
            let detail = format!(
                "monomorphisation is not yet implemented — generic or type-parameter reference remains after lowering (first sample: {sample}). Emit concrete types in source until this pass is available."
            );
            return Err(vec![CompilerError::InternalError {
                detail,
                span: Span::default(),
            }]);
        }

        Ok(module)
    }
}

#[derive(Debug, Default)]
struct GenericDetector {
    first_sample: Option<String>,
}

impl GenericDetector {
    fn new() -> Self {
        Self::default()
    }

    fn first_sample(&self) -> Option<&str> {
        self.first_sample.as_deref()
    }

    fn note(&mut self, sample: String) {
        if self.first_sample.is_none() {
            self.first_sample = Some(sample);
        }
    }

    fn check_type(&mut self, ty: &ResolvedType) {
        match ty {
            ResolvedType::Generic { base, args } => {
                self.note(format!("Generic(base={:?}, {} args)", base, args.len()));
                for a in args {
                    self.check_type(a);
                }
            }
            ResolvedType::Array(inner) | ResolvedType::Optional(inner) => self.check_type(inner),
            ResolvedType::Tuple(fields) => {
                for (_, t) in fields {
                    self.check_type(t);
                }
            }
            ResolvedType::Dictionary { key_ty, value_ty } => {
                self.check_type(key_ty);
                self.check_type(value_ty);
            }
            ResolvedType::Closure {
                param_tys,
                return_ty,
            } => {
                for (_, t) in param_tys {
                    self.check_type(t);
                }
                self.check_type(return_ty);
            }
            ResolvedType::External { type_args, .. } => {
                for a in type_args {
                    self.check_type(a);
                }
            }
            // Concrete types and the overloaded TypeParam placeholder
            // (which the current IR uses for unresolved/placeholder types
            // like "Unknown" and field-access paths, not just true generic
            // parameters) are treated as fully resolved by this stub.
            ResolvedType::Primitive(_)
            | ResolvedType::Struct(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Enum(_)
            | ResolvedType::TypeParam(_) => {}
        }
    }

    fn scan_definitions(&mut self, module: &IrModule) {
        for s in &module.structs {
            if !s.generic_params.is_empty() {
                self.note(format!(
                    "generic struct `{}` with {} type parameters",
                    s.name,
                    s.generic_params.len()
                ));
            }
            for f in &s.fields {
                self.check_type(&f.ty);
            }
        }
        for t in &module.traits {
            if !t.generic_params.is_empty() {
                self.note(format!(
                    "generic trait `{}` with {} type parameters",
                    t.name,
                    t.generic_params.len()
                ));
            }
            for f in &t.fields {
                self.check_type(&f.ty);
            }
        }
        for e in &module.enums {
            if !e.generic_params.is_empty() {
                self.note(format!(
                    "generic enum `{}` with {} type parameters",
                    e.name,
                    e.generic_params.len()
                ));
            }
            for v in &e.variants {
                for f in &v.fields {
                    self.check_type(&f.ty);
                }
            }
        }
        for f in &module.functions {
            self.check_function(f);
        }
        for i in &module.impls {
            for f in &i.functions {
                self.check_function(f);
            }
        }
        for l in &module.lets {
            self.check_type(&l.ty);
        }
    }

    fn check_function(&mut self, f: &IrFunction) {
        for p in &f.params {
            if let Some(ty) = &p.ty {
                self.check_type(ty);
            }
        }
        if let Some(ty) = &f.return_type {
            self.check_type(ty);
        }
    }
}

impl IrVisitor for GenericDetector {
    fn visit_expr(&mut self, expr: &IrExpr) {
        self.check_type(expr.ty());
    }
}
