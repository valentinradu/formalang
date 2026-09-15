//! Companion enums for [`super::IrExpr`]: how a `Reference` was
//! resolved (or that it hasn't been yet) and how a `MethodCall`
//! should be dispatched.

use crate::ir::{BindingId, EnumId, FunctionId, ImplId, ImportedKind, LetId, StructId, TraitId};

/// Target of an [`super::IrExpr::Reference`] after
/// `ResolveReferencesPass` runs.
///
/// Pre-resolve, every reference carries [`Self::Unresolved`]. The pass
/// rewrites each one to the matching variant. Backends can dispatch on
/// the variant directly without re-walking module symbol tables. The
/// original `path` on `IrExpr::Reference` is preserved alongside for
/// diagnostics.
#[expect(
    clippy::exhaustive_enums,
    reason = "IR types are matched exhaustively by code generators"
)]
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReferenceTarget {
    /// A standalone function (resolved against `IrModule::functions`).
    Function(FunctionId),
    /// A struct definition used as a value or type.
    Struct(StructId),
    /// An enum definition.
    Enum(EnumId),
    /// A trait definition.
    Trait(TraitId),
    /// A module-scope `let` binding.
    ModuleLet(LetId),
    /// A function-local `let` binding (introduced by
    /// [`crate::ir::IrBlockStatement::Let`]).
    Local(BindingId),
    /// A function parameter (introduced by
    /// [`crate::ir::IrFunctionParam`]).
    Param(BindingId),
    /// A reference into another module that has not yet been linked;
    /// cross-module linking lands later (formawasm Phase 4).
    External {
        module_path: Vec<String>,
        name: String,
        kind: ImportedKind,
    },
    /// Pre-`ResolveReferencesPass` placeholder. Lowering emits this
    /// and the pass overwrites it; backends should never see
    /// `Unresolved`.
    Unresolved,
}

/// How a method call should be dispatched.
///
/// Backends must pick the correct emission strategy depending on whether
/// the receiver's concrete type is known at compile time. Static dispatch
/// resolves to a specific `impl` block; virtual dispatch must go through a
/// vtable keyed by the trait and method name.
#[expect(
    clippy::exhaustive_enums,
    reason = "IR types are matched exhaustively by code generators"
)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum DispatchKind {
    /// Direct call on a known concrete type; no runtime lookup needed.
    Static {
        /// The impl block that provides the method body.
        impl_id: ImplId,
    },
    /// Trait method call through a generic type parameter (`T: Trait`).
    ///
    /// There is no trait-object form: a trait cannot be the type of a
    /// value, so semantic analysis rejects one before lowering.
    /// `MonomorphisePass` rewrites every `Virtual` site to `Static`
    /// once the receiver type is concrete, and reports any that
    /// survive. A backend therefore never needs a vtable.
    Virtual {
        /// The trait declaring the method.
        trait_id: TraitId,
        /// The method name on the trait.
        method_name: String,
    },
}
