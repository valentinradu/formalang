//! Backend pipeline for IR transformation and code generation.
//!
//! FormaLang produces an [`IrModule`] from source code. This module defines
//! the traits needed to transform that IR and emit code from it.
//!
//! # Architecture
//!
//! ```text
//! IrModule → [IrPass, IrPass, ...] → IrModule → Backend → Output
//! ```
//!
//! - [`IrPass`]: transforms IR → IR (optimization, specialization, lowering)
//! - [`Backend`]: consumes IR and emits output (code generation)
//! - [`Pipeline`]: composes passes and drives a backend
//!
//! # Example
//!
//! ```
//! use formalang::{compile_to_ir, Backend, Pipeline};
//! use formalang::ir::IrModule;
//!
//! struct StructLister;
//!
//! impl Backend for StructLister {
//!     type Output = Vec<String>;
//!     type Error = std::convert::Infallible;
//!
//!     fn generate(&self, module: &IrModule) -> Result<Vec<String>, Self::Error> {
//!         Ok(module.structs.iter().map(|s| s.name.clone()).collect())
//!     }
//! }
//!
//! let source = "pub struct User { name: String }";
//! let ir = compile_to_ir(source).unwrap();
//! let names = Pipeline::new().emit(ir, &StructLister).unwrap();
//! assert_eq!(names, vec!["User"]);
//! ```

use crate::error::CompilerError;
use crate::ir::IrModule;

/// An IR transformation pass.
///
/// Passes take ownership of an [`IrModule`], transform it, and return a new one.
/// Fields that are not modified can be moved through at zero cost:
///
/// ```rust,ignore
/// module.structs.retain(|s| keep(s));
/// module.rebuild_indices();
/// Ok(module)
/// ```
///
/// # Important
///
/// If your pass removes or reorders definitions (structs, traits, enums,
/// functions, or lets), call [`IrModule::rebuild_indices`] before returning.
/// This keeps name-based lookups consistent with the new indices.
///
/// Passes that only modify fields within existing definitions (e.g., folding
/// constant expressions) do not need to call `rebuild_indices`.
///
/// # Example
///
/// ```
/// use formalang::{IrPass, compile_to_ir};
/// use formalang::ir::IrModule;
/// use formalang::error::CompilerError;
/// use formalang::ast::Visibility;
///
/// struct KeepPublicStructs;
///
/// impl IrPass for KeepPublicStructs {
///     fn name(&self) -> &str { "keep-public-structs" }
///
///     fn run(&mut self, mut module: IrModule) -> Result<IrModule, Vec<CompilerError>> {
///         module.structs.retain(|s| s.visibility == Visibility::Public);
///         module.rebuild_indices();
///         Ok(module)
///     }
/// }
///
/// let source = "pub struct User { name: String }";
/// let ir = compile_to_ir(source).unwrap();
/// let result = KeepPublicStructs.run(ir).unwrap();
/// assert_eq!(result.structs.len(), 1);
/// ```
pub trait IrPass {
    /// A short name identifying this pass, used in error messages.
    fn name(&self) -> &str;

    /// Transform the module, returning a new module or errors.
    fn run(&mut self, module: IrModule) -> Result<IrModule, Vec<CompilerError>>;
}

/// A code generation backend.
///
/// Backends consume an [`IrModule`] and produce output. The output type is
/// defined by the implementor — a `String`, `Vec<u8>`, a structured AST, or
/// anything else.
///
/// # Example
///
/// ```
/// use formalang::{Backend, compile_to_ir};
/// use formalang::ir::IrModule;
///
/// struct EnumCounter;
///
/// impl Backend for EnumCounter {
///     type Output = usize;
///     type Error = std::convert::Infallible;
///
///     fn generate(&self, module: &IrModule) -> Result<usize, Self::Error> {
///         Ok(module.enums.len())
///     }
/// }
///
/// let source = "pub enum Status { active, inactive }";
/// let ir = compile_to_ir(source).unwrap();
/// let count = EnumCounter.generate(&ir).unwrap();
/// assert_eq!(count, 1);
/// ```
pub trait Backend {
    /// The type produced by this backend.
    type Output;
    /// The error type this backend can produce.
    type Error: std::error::Error;

    /// Generate output from the IR module.
    fn generate(&self, module: &IrModule) -> Result<Self::Output, Self::Error>;
}

/// Error produced by a [`Pipeline`].
#[derive(Debug)]
pub enum PipelineError<E: std::error::Error> {
    /// One or more passes failed with compiler errors.
    PassErrors(Vec<CompilerError>),
    /// The backend failed.
    BackendError(E),
}

impl<E: std::error::Error> std::fmt::Display for PipelineError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineError::PassErrors(errors) => {
                for e in errors {
                    writeln!(f, "{e}")?;
                }
                Ok(())
            }
            PipelineError::BackendError(e) => write!(f, "{e}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for PipelineError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PipelineError::BackendError(e) => Some(e),
            PipelineError::PassErrors(_) => None,
        }
    }
}

/// A composable sequence of IR passes.
///
/// Passes run in order; the output of each feeds the next. After all passes,
/// call [`Pipeline::emit`] to run a [`Backend`] on the final module, or
/// [`Pipeline::run`] to get the transformed module directly.
///
/// # Example
///
/// ```
/// use formalang::{compile_to_ir, Backend, Pipeline};
/// use formalang::ir::{IrModule, DeadCodeEliminationPass, ConstantFoldingPass};
///
/// # struct MyBackend;
/// # impl Backend for MyBackend {
/// #     type Output = usize;
/// #     type Error = std::convert::Infallible;
/// #     fn generate(&self, m: &IrModule) -> Result<usize, Self::Error> { Ok(m.structs.len()) }
/// # }
///
/// let source = "pub struct User { name: String }";
/// let ir = compile_to_ir(source).unwrap();
///
/// let result = Pipeline::new()
///     .pass(DeadCodeEliminationPass::new())
///     .pass(ConstantFoldingPass::new())
///     .emit(ir, &MyBackend)
///     .unwrap();
/// ```
pub struct Pipeline {
    passes: Vec<Box<dyn IrPass>>,
}

impl Pipeline {
    /// Create an empty pipeline with no passes.
    pub fn new() -> Self {
        Self { passes: Vec::new() }
    }

    /// Append a pass and return `self` for chaining.
    pub fn pass(mut self, p: impl IrPass + 'static) -> Self {
        self.passes.push(Box::new(p));
        self
    }

    /// Run all passes in order, returning the transformed module.
    pub fn run(&mut self, module: IrModule) -> Result<IrModule, Vec<CompilerError>> {
        let mut current = module;
        for pass in &mut self.passes {
            current = pass.run(current)?;
        }
        Ok(current)
    }

    /// Run all passes then emit with the given backend.
    pub fn emit<B: Backend>(
        &mut self,
        module: IrModule,
        backend: &B,
    ) -> Result<B::Output, PipelineError<B::Error>> {
        let module = self.run(module).map_err(PipelineError::PassErrors)?;
        backend
            .generate(&module)
            .map_err(PipelineError::BackendError)
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}
