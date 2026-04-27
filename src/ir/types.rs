//! IR definition types (structs, traits, enums, fields).

use crate::ast::Visibility;

use super::{EnumId, IrExpr, ResolvedType, StructId, TraitId};

/// A module-level let binding in the IR.
///
/// Represents a named constant or computed value defined at the module level.
/// These are used for theming, configuration values, and shared expressions.
///
/// # Example
///
/// ```formalang
/// let primaryColor: Color = .hex(value: "#2563EB")
/// let headingFont: Font = Font(family: "Inter", size: 24)
/// ```
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct IrLet {
    /// The binding name
    pub name: String,

    /// Visibility (public or private)
    pub visibility: Visibility,

    /// Whether this binding is mutable
    pub mutable: bool,

    /// The resolved type of the binding
    pub ty: ResolvedType,

    /// The bound expression
    pub value: IrExpr,

    /// Joined `///` doc comments preceding this binding. Audit #51.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// A struct definition in the IR.
///
/// Structs are the primary data type in `FormaLang`, representing both
/// data models and UI components.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct IrStruct {
    /// The struct name
    pub name: String,

    /// Visibility (public or private)
    pub visibility: Visibility,

    /// Traits implemented by this struct, with optional generic-trait
    /// args (`<T>`). Empty args means a non-generic trait. Generic-
    /// traits PR: changed from `Vec<TraitId>` so generic-trait
    /// instantiations (`impl Eq<I32> for Foo`) can be tracked
    /// distinctly per arg-tuple.
    pub traits: Vec<IrTraitRef>,

    /// Regular fields
    pub fields: Vec<IrField>,

    /// Generic type parameters
    pub generic_params: Vec<IrGenericParam>,

    /// Joined `///` doc comments preceding this struct. Audit #51.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// A trait definition in the IR.
///
/// Traits define interfaces that structs can implement.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct IrTrait {
    /// The trait name
    pub name: String,

    /// Visibility (public or private)
    pub visibility: Visibility,

    /// Traits composed into this trait (trait inheritance)
    pub composed_traits: Vec<TraitId>,

    /// Required fields
    pub fields: Vec<IrField>,

    /// Required method signatures
    pub methods: Vec<IrFunctionSig>,

    /// Generic type parameters
    pub generic_params: Vec<IrGenericParam>,

    /// Joined `///` doc comments preceding this trait. Audit #51.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// A function signature in the IR (without a body).
///
/// Used for trait method declarations that define the interface
/// without providing an implementation.
///
/// # Example
///
/// ```formalang
/// trait Drawable {
///     fn draw(self) -> String
/// }
/// ```
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct IrFunctionSig {
    /// Function name
    pub name: String,

    /// Parameters (first is typically `self`)
    pub params: Vec<IrFunctionParam>,

    /// Return type (None = unit/void)
    pub return_type: Option<ResolvedType>,

    /// Codegen-hint attributes (`inline`, `no_inline`, `cold`)
    /// declared on the trait method signature. Empty when none are
    /// present. Round-trips serialised IR while remaining backwards-
    /// compatible with documents that predate this field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<crate::ast::FunctionAttribute>,
}

/// An enum definition in the IR.
///
/// Enums are sum types with named variants, optionally carrying data.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct IrEnum {
    /// The enum name
    pub name: String,

    /// Visibility (public or private)
    pub visibility: Visibility,

    /// Enum variants
    pub variants: Vec<IrEnumVariant>,

    /// Generic type parameters
    pub generic_params: Vec<IrGenericParam>,

    /// Joined `///` doc comments preceding this enum. Audit #51.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// An enum variant.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct IrEnumVariant {
    /// The variant name
    pub name: String,

    /// Associated data fields (empty for unit variants)
    pub fields: Vec<IrField>,
}

/// Target of an impl block - either a struct or enum.
#[expect(
    clippy::exhaustive_enums,
    reason = "IR types are matched exhaustively by code generators"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ImplTarget {
    /// Impl for a struct
    Struct(StructId),
    /// Impl for an enum
    Enum(EnumId),
}

/// An impl block in the IR.
///
/// Impl blocks provide methods for a struct or enum. Backends that need to
/// emit trait-conformance declarations (e.g. `TypeScript` / Kotlin
/// `implements`) can read `trait_id` to learn which trait the block
/// implements — it is `None` for inherent impls. `is_extern` mirrors the
/// `extern impl` syntax and indicates that the impl's methods have no
/// `FormaLang` body. `generic_params` captures `impl<T>` constraints.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct IrImpl {
    /// The struct or enum this impl is for
    pub target: ImplTarget,

    /// `Some(IrTraitRef { trait_id, args })` for `impl Trait for Type`
    /// or `impl Trait<X> for Type`; `None` for inherent impls. The
    /// args slot is empty for non-generic traits — Phase C of the
    /// generic-traits work added it so monomorphisation can
    /// specialise generic-trait impls.
    pub trait_ref: Option<IrTraitRef>,

    /// Whether this is an `extern impl` block (all methods `is_extern = true`).
    pub is_extern: bool,

    /// Generic parameters declared on the impl block itself
    /// (`impl<T: Bound> Box<T>`).
    pub generic_params: Vec<IrGenericParam>,

    /// Methods defined in this impl block
    pub functions: Vec<IrFunction>,
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
            ImplTarget::Enum(_) => None,
        }
    }

    /// Get the enum ID if this impl is for an enum.
    #[must_use]
    pub const fn enum_id(&self) -> Option<EnumId> {
        match self.target {
            ImplTarget::Struct(_) => None,
            ImplTarget::Enum(id) => Some(id),
        }
    }
}

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

    /// Generic type parameters declared on the function itself
    /// (e.g. `fn identity<T>(value: T) -> T`).
    /// Empty for methods — method-level generics aren't yet supported;
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
    pub extern_abi: Option<crate::ast::ExternAbi>,

    /// Codegen-hint attributes (`inline`, `no_inline`, `cold`) declared
    /// before the `fn` keyword. Empty when none are present. Round-
    /// trips serialised IR while remaining backwards-compatible with
    /// documents that predate this field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<crate::ast::FunctionAttribute>,

    /// Joined `///` doc comments preceding this function. Audit #51.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
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
    /// Parameter name
    pub name: String,

    /// External call-site label, for parameters declared as
    /// `fn foo(label name: T)`. `None` when the parameter has no
    /// distinct external label. Preserved so label-based calling
    /// conventions (Swift, Kotlin) can emit the call-site name
    /// distinct from the body-side name. Audit finding #39.
    pub external_label: Option<String>,

    /// Parameter type (None for `self` parameter - type is inferred from impl block)
    pub ty: Option<ResolvedType>,

    /// Default value expression (if provided)
    pub default: Option<IrExpr>,

    /// Parameter passing convention
    pub convention: crate::ast::ParamConvention,
}

/// A field definition.
///
/// Used in structs, traits, and enum variants.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct IrField {
    /// Field name
    pub name: String,

    /// Resolved type
    pub ty: ResolvedType,

    /// Whether this field is mutable
    pub mutable: bool,

    /// Whether this field is optional (T?)
    pub optional: bool,

    /// Default value expression, if any
    pub default: Option<IrExpr>,

    /// Joined `///` doc comments preceding this field. Audit2 B2.
    pub doc: Option<String>,

    /// Capture / passing convention for this field.
    ///
    /// Always [`ParamConvention::Let`] for fields written in source
    /// (struct, trait, enum-variant fields). Set to a non-default
    /// value by [`ClosureConversionPass`](crate::ir::ClosureConversionPass)
    /// on synthesized env-struct fields so backends targeting linear-
    /// memory representations can choose between copy / move /
    /// reference semantics per capture without re-walking the
    /// original closure expression.
    ///
    /// `#[serde(default)]` keeps round-tripped IR documents
    /// produced before this field landed deserialisable as
    /// [`ParamConvention::Let`] (the existing implicit behaviour).
    #[serde(default)]
    pub convention: crate::ast::ParamConvention,
}

/// A generic type parameter.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct IrGenericParam {
    /// Parameter name (e.g., "T")
    pub name: String,

    /// Trait constraints (e.g., `T: Container` or `T: Container<I32>`).
    /// Each entry carries the constrained trait id plus zero or more
    /// concrete arg types — empty when the trait isn't generic.
    pub constraints: Vec<IrTraitRef>,
}

/// A reference to a trait, optionally with concrete type arguments.
///
/// Used in two places after Phase C: as the constraint shape on
/// [`IrGenericParam`] and as the trait-impl shape on [`IrImpl`]. An
/// empty `args` slot means the trait isn't generic (`T: Container`,
/// `impl Container for X`); a non-empty slot carries the
/// instantiation (`T: Container<I32>`, `impl Container<I32> for X`)
/// so monomorphisation can specialise generic traits.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct IrTraitRef {
    pub trait_id: TraitId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<ResolvedType>,
}

impl IrTraitRef {
    /// Construct a non-generic trait reference (no args).
    #[must_use]
    pub const fn simple(trait_id: TraitId) -> Self {
        Self {
            trait_id,
            args: Vec::new(),
        }
    }
}
