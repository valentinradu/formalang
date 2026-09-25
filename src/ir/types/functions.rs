//! Function definition + parameter shapes.

use crate::ast::{ExternAbi, FunctionAttribute, ParamConvention, Visibility};
use crate::ir::{BindingId, IrExpr, IrSpan, ResolvedType};

use super::IrGenericParam;
#[cfg(feature = "serde")]
use crate::ir::span::no_span;

/// A function definition in the IR.
///
/// A standalone function in [`crate::ir::IrModule::functions`], or a
/// method in an [`crate::ir::IrImpl`]. A method that takes `self` has
/// `self` as its first parameter.
///
/// # Example
///
/// ```formalang
/// struct Vec2 { x: F64, y: F64 }
///
/// impl Vec2 {
///     fn length_squared(self) -> F64 {
///         self.x * self.x + self.y * self.y
///     }
/// }
/// ```
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IrFunction {
    /// Function name
    pub name: String,

    /// Visibility (public or private).
    ///
    /// A backend keys its export list on this: a `pub fn` becomes a
    /// symbol the host can call by name, a private `fn` stays internal.
    /// Defaults to private, so a hand-built `IrFunction` and older
    /// serialised IR both stay internal rather than leaking.
    #[cfg_attr(feature = "serde", serde(default))]
    pub visibility: Visibility,

    /// Generic type parameters declared on the function or method
    /// itself (e.g. `fn identity<T>(value: T) -> T`,
    /// `fn map<U>(self, f: (T) -> U) -> Box<U>`). Enclosing-type
    /// generics live on the containing `IrImpl` / `IrStruct`.
    ///
    /// After `MonomorphisePass`, only an extern method keeps its own
    /// type parameters: it has no body to specialise, and each call
    /// carries the concrete types. The prelude's
    /// `Seq.collect<K, V>(key:value:)` is one.
    pub generic_params: Vec<IrGenericParam>,

    /// Parameters (first is typically `self`)
    pub params: Vec<IrFunctionParam>,

    /// Return type (None = unit/void)
    pub return_type: Option<ResolvedType>,

    /// Function body expression (None for extern functions)
    pub body: Option<IrExpr>,

    /// Calling convention when this function is declared `extern` (no
    /// body, defined outside `FormaLang`). `None` for regular
    /// functions. A backend for a target with more than one calling
    /// convention reads it to emit the correct call sequence.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub extern_abi: Option<ExternAbi>,

    /// Codegen-hint attributes (`inline`, `no_inline`, `cold`) declared
    /// before the `fn` keyword. Empty when none are present. Round-
    /// trips serialised IR while remaining backwards-compatible with
    /// documents that predate this field.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub attributes: Vec<FunctionAttribute>,

    /// Joined `///` doc comments preceding this function.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub doc: Option<String>,

    /// Source span for DWARF / source-map emission. For DWARF
    /// `DW_TAG_subprogram` this is the function's declaration span.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "no_span"))]
    pub span: IrSpan,
}

impl IrFunction {
    /// Whether this function is declared `extern`. Convenience wrapper
    /// over [`Self::extern_abi`] for the common boolean check.
    #[must_use]
    pub const fn is_extern(&self) -> bool {
        self.extern_abi.is_some()
    }
}

/// A function parameter in the IR.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IrFunctionParam {
    /// Per-function-unique binding identifier; paired with the
    /// `BindingId` carried on uses of this parameter
    /// ([`crate::ir::IrExpr::Reference`] with
    /// [`crate::ir::ReferenceTarget::Param`] or
    /// [`crate::ir::IrExpr::LetRef`]). Lowering emits `BindingId(0)` and
    /// `ResolveReferencesPass` overwrites it.
    pub binding_id: BindingId,

    /// Parameter name
    pub name: String,

    /// External call-site label, for parameters declared as
    /// `fn foo(label name: T)`. `None` when the parameter has no
    /// distinct external label. Preserved so label-based calling
    /// conventions (Swift, Kotlin) can emit the call-site name
    /// distinct from the body-side name.
    pub external_label: Option<String>,

    /// Parameter type (None for `self` parameter; type is inferred from impl block)
    pub ty: Option<ResolvedType>,

    /// Default value expression (if provided)
    pub default: Option<IrExpr>,

    /// Parameter passing convention
    pub convention: ParamConvention,

    /// Source span for DWARF / source-map emission.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "no_span"))]
    pub span: IrSpan,
}
