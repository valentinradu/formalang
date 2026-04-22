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
#[derive(Clone, Debug)]
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
}

/// A struct definition in the IR.
///
/// Structs are the primary data type in `FormaLang`, representing both
/// data models and UI components.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug)]
pub struct IrStruct {
    /// The struct name
    pub name: String,

    /// Visibility (public or private)
    pub visibility: Visibility,

    /// Traits implemented by this struct
    pub traits: Vec<TraitId>,

    /// Regular fields
    pub fields: Vec<IrField>,

    /// Generic type parameters
    pub generic_params: Vec<IrGenericParam>,
}

/// A trait definition in the IR.
///
/// Traits define interfaces that structs can implement.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
pub struct IrFunctionSig {
    /// Function name
    pub name: String,

    /// Parameters (first is typically `self`)
    pub params: Vec<IrFunctionParam>,

    /// Return type (None = unit/void)
    pub return_type: Option<ResolvedType>,
}

/// An enum definition in the IR.
///
/// Enums are sum types with named variants, optionally carrying data.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug)]
pub struct IrEnum {
    /// The enum name
    pub name: String,

    /// Visibility (public or private)
    pub visibility: Visibility,

    /// Enum variants
    pub variants: Vec<IrEnumVariant>,

    /// Generic type parameters
    pub generic_params: Vec<IrGenericParam>,
}

/// An enum variant.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImplTarget {
    /// Impl for a struct
    Struct(StructId),
    /// Impl for an enum
    Enum(EnumId),
}

/// An impl block in the IR.
///
/// Impl blocks provide methods for a struct or enum.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug)]
pub struct IrImpl {
    /// The struct or enum this impl is for
    pub target: ImplTarget,

    /// Methods defined in this impl block
    pub functions: Vec<IrFunction>,
}

impl IrImpl {
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
///     fn length(self) -> Number {
///         self.x * self.x + self.y * self.y
///     }
/// }
/// ```
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug)]
pub struct IrFunction {
    /// Function name
    pub name: String,

    /// Parameters (first is typically `self`)
    pub params: Vec<IrFunctionParam>,

    /// Return type (None = unit/void)
    pub return_type: Option<ResolvedType>,

    /// Function body expression (None for extern functions)
    pub body: Option<IrExpr>,

    /// Whether this function is extern (no body, defined outside `FormaLang`)
    pub is_extern: bool,
}

/// A function parameter in the IR.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug)]
pub struct IrFunctionParam {
    /// Parameter name
    pub name: String,

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
#[derive(Clone, Debug)]
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
}

/// A generic type parameter.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug)]
pub struct IrGenericParam {
    /// Parameter name (e.g., "T")
    pub name: String,

    /// Trait constraints (e.g., T: Container)
    pub constraints: Vec<TraitId>,
}
