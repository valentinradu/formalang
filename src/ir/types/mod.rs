//! IR definition types (structs, traits, enums, fields, functions).
//!
//! Each definition kind lives in its own submodule so callers can grep
//! one source-of-truth file for a given shape; `ir/mod.rs` re-exports
//! every public name through this `mod.rs` so consumers of
//! `formalang::ir` see a flat surface.

mod enums;
mod functions;
mod impls;
mod lets;
mod structs;
mod traits;

pub use enums::{IrEnum, IrEnumVariant};
pub use functions::{IrFunction, IrFunctionParam};
pub use impls::{ImplTarget, IrImpl};
pub use lets::IrLet;
pub use structs::{IrField, IrGenericParam, IrStruct, IrTraitRef};
pub use traits::{IrFunctionSig, IrTrait};
