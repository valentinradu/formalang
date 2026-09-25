//! Impl-block shape + the discriminator that says what it implements.

use crate::ast::PrimitiveType;
use crate::ir::{EnumId, IrSpan, StructId, TraitId};

use super::{IrFunction, IrGenericParam, IrTraitRef};

/// Target of an impl block.
///
/// `Primitive(PrimitiveType)` is reserved for `extern impl <Primitive> { ... }`
/// blocks (e.g., the compiler-shipped prelude's `extern impl String`),
/// where the language injects host-provided behaviour onto a primitive
/// receiver type. The semantic pass rejects a non-extern impl on a
/// primitive with [`crate::CompilerError::ImplOnPrimitive`].
#[expect(
    clippy::exhaustive_enums,
    reason = "IR types are matched exhaustively by code generators"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ImplTarget {
    /// Impl for a struct
    Struct(StructId),
    /// Impl for an enum
    Enum(EnumId),
    /// Impl for a primitive receiver (e.g. `extern impl String`,
    /// `extern impl I32`). Only valid in `extern impl` blocks; non-
    /// extern impls on primitives are rejected at semantic time.
    Primitive(PrimitiveType),
}
#[cfg(feature = "serde")]
use crate::ir::span::no_span;

/// An impl block in the IR.
///
/// Impl blocks provide methods for a struct, an enum or a primitive. Backends that need to
/// emit trait-conformance declarations (e.g. `TypeScript` / Kotlin
/// `implements`) can read `trait_id` to learn which trait the block
/// implements; it is `None` for inherent impls. `is_extern` mirrors the
/// `extern impl` syntax and indicates that the impl's methods have no
/// `FormaLang` body. `generic_params` captures `impl<T>` constraints.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IrImpl {
    /// The struct or enum this impl is for
    pub target: ImplTarget,

    /// `Some(IrTraitRef { trait_id, args })` for `impl Trait for Type`
    /// or `impl Trait<X> for Type`; `None` for inherent impls. The
    /// args slot is empty for non-generic traits. It lets
    /// monomorphisation specialise generic-trait impls.
    pub trait_ref: Option<IrTraitRef>,

    /// Whether this is an `extern impl` block (all methods `is_extern = true`).
    pub is_extern: bool,

    /// Generic parameters declared on the impl block itself
    /// (`impl<T: Bound> Box<T>`).
    pub generic_params: Vec<IrGenericParam>,

    /// Methods defined in this impl block
    pub functions: Vec<IrFunction>,

    /// Source span for DWARF / source-map emission.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "no_span"))]
    pub span: IrSpan,
}

impl IrImpl {
    /// Convenience: trait id of the impl, ignoring args. Equivalent
    /// to `self.trait_ref.as_ref().map(|t| t.trait_id)`.
    #[must_use]
    pub fn trait_id(&self) -> Option<TraitId> {
        self.trait_ref.as_ref().map(|t| t.trait_id)
    }

    /// Get the struct ID if this impl is for a struct.
    #[must_use]
    pub const fn struct_id(&self) -> Option<StructId> {
        match self.target {
            ImplTarget::Struct(id) => Some(id),
            ImplTarget::Enum(_) | ImplTarget::Primitive(_) => None,
        }
    }

    /// Get the enum ID if this impl is for an enum.
    #[must_use]
    pub const fn enum_id(&self) -> Option<EnumId> {
        match self.target {
            ImplTarget::Struct(_) | ImplTarget::Primitive(_) => None,
            ImplTarget::Enum(id) => Some(id),
        }
    }

    /// Get the primitive receiver if this impl is on a primitive.
    #[must_use]
    pub const fn primitive(&self) -> Option<PrimitiveType> {
        match self.target {
            ImplTarget::Primitive(p) => Some(p),
            ImplTarget::Struct(_) | ImplTarget::Enum(_) => None,
        }
    }
}
