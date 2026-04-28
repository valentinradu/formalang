//! Intermediate Representation (IR) for `FormaLang`
//!
//! The IR is a type-resolved representation of `FormaLang` programs optimized for
//! code generation. Unlike the AST which preserves source syntax, the IR provides:
//!
//! - Resolved types on every expression
//! - ID-based references to definitions (no string lookups)
//! - Flattened structure for easy traversal
//!
//! # Example
//!
//! ```
//! use formalang::compile_to_ir;
//!
//! let source = r#"
//! pub struct User {
//!     name: String,
//!     age: I32
//! }
//! "#;
//!
//! let module = compile_to_ir(source).unwrap();
//! assert_eq!(module.structs.len(), 1);
//! assert_eq!(module.structs[0].name, "User");
//! ```

mod block;
mod closure_conv;
mod dce;
mod expr;
mod fold;
mod ids;
mod imports;
mod lower;
mod module;
mod monomorphise;
mod resolved_type;
mod types;
mod visitor;

pub use block::{IrBlockStatement, IrMatchArm};
pub use closure_conv::ClosureConversionPass;
pub use dce::{
    eliminate_dead_code, eliminate_dead_code_expr, DeadCodeEliminationPass, DeadCodeEliminator,
};
pub use expr::{DispatchKind, IrExpr};
pub use fold::{fold_constants, ConstantFolder, ConstantFoldingPass};
pub use ids::{EnumId, FunctionId, ImplId, StructId, TraitId};
pub use imports::{ImportedKind, IrImport, IrImportItem};
pub use lower::lower_to_ir;
pub use module::{IrModule, IrModuleNode};
pub use monomorphise::MonomorphisePass;
pub use resolved_type::{GenericBase, ResolvedType};
pub use types::{
    ImplTarget, IrEnum, IrEnumVariant, IrField, IrFunction, IrFunctionParam, IrFunctionSig,
    IrGenericParam, IrImpl, IrLet, IrStruct, IrTrait, IrTraitRef,
};
pub use visitor::{
    walk_block_statement, walk_expr, walk_expr_children, walk_module, walk_module_children,
    IrVisitor,
};

use crate::ast::Visibility;

impl Visibility {
    /// Check if this visibility is public.
    #[must_use]
    pub const fn is_public(&self) -> bool {
        matches!(self, Self::Public)
    }
}

/// Extract the simple type name from a potentially module-qualified path.
///
/// Given a path like `alignment::Horizontal`, returns `Horizontal`.
/// For simple names like `Button`, returns the name unchanged.
///
/// # Example
///
/// ```
/// use formalang::ir::simple_type_name;
///
/// assert_eq!(simple_type_name("alignment::Horizontal"), "Horizontal");
/// assert_eq!(simple_type_name("Button"), "Button");
/// ```
#[must_use]
pub fn simple_type_name(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name)
}
