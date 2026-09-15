//! Function definition + parameter shapes.

use crate::ast::{ExternAbi, FunctionAttribute, ParamConvention, Visibility};
use crate::ir::{BindingId, IrExpr, IrSpan, ResolvedType};

use super::IrGenericParam;

/// A function definition in the IR.
///
/// Functions are methods defined in impl blocks. They operate on `self`
/// and can take additional parameters.
///
/// # Example
///
/// ```formalang
/// impl Vec2 {
///     fn length(self) -> F64 {
///         self.x * self.x + self.y * self.y
///     }
/// }
/// ```
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct IrFunction {
    /// Function name
    pub name: String,

    /// Visibility (public or private).
    ///
    /// A backend keys its export list on this: a `pub fn` becomes a
    /// symbol the host can call by name, a private `fn` stays internal.
    /// Defaults to private, so a hand-built `IrFunction` and older
    /// serialised IR both stay internal rather than leaking.
    #[serde(default)]
    pub visibility: Visibility,

    /// Generic type parameters declared on the function itself
    /// (e.g. `fn identity<T>(value: T) -> T`).
    /// Empty for methods; method-level generics aren't yet supported;
    /// enclosing-type generics live on the containing `IrImpl` / `IrStruct`.
    pub generic_params: Vec<IrGenericParam>,

    /// Parameters (first is typically `self`)
    pub params: Vec<IrFunctionParam>,

    /// Return type (None = unit/void)
    pub return_type: Option<ResolvedType>,

    /// Function body expression (None for extern functions)
    pub body: Option<IrExpr>,

    /// Calling convention when this function is declared `extern` (no
    /// body, defined outside `FormaLang`). `None` for regular
    /// functions. Tier-1 item E: replaces the previous `is_extern: bool`
    /// flag so backends targeting languages with distinguished calling
    /// conventions can emit the correct call sequence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extern_abi: Option<ExternAbi>,

    /// Codegen-hint attributes (`inline`, `no_inline`, `cold`) declared
    /// before the `fn` keyword. Empty when none are present. Round-
    /// trips serialised IR while remaining backwards-compatible with
    /// documents that predate this field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<FunctionAttribute>,

    /// Joined `///` doc comments preceding this function.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,

    /// Source span for DWARF / source-map emission. For DWARF
    /// `DW_TAG_subprogram` this is the function's declaration span.
    #[serde(default, skip_serializing_if = "IrSpan::is_default")]
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
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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
    #[serde(default, skip_serializing_if = "IrSpan::is_default")]
    pub span: IrSpan,
}
