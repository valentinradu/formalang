//! IR definition types (structs, traits, enums, fields).

use crate::ast::Visibility;

use super::{IrExpr, ResolvedType, StructId, TraitId};

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
/// Structs are the primary data type in FormaLang, representing both
/// data models and UI components.
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

    /// Mount fields (UI container slots)
    pub mount_fields: Vec<IrField>,

    /// Generic type parameters
    pub generic_params: Vec<IrGenericParam>,
}

/// A trait definition in the IR.
///
/// Traits define interfaces that structs can implement.
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

    /// Required mount fields
    pub mount_fields: Vec<IrField>,

    /// Generic type parameters
    pub generic_params: Vec<IrGenericParam>,
}

/// An enum definition in the IR.
///
/// Enums are sum types with named variants, optionally carrying data.
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
#[derive(Clone, Debug)]
pub struct IrEnumVariant {
    /// The variant name
    pub name: String,

    /// Associated data fields (empty for unit variants)
    pub fields: Vec<IrField>,
}

/// An impl block in the IR.
///
/// Impl blocks provide methods for a struct.
#[derive(Clone, Debug)]
pub struct IrImpl {
    /// The struct this impl is for
    pub struct_id: StructId,

    /// Methods defined in this impl block
    pub functions: Vec<IrFunction>,
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
///     fn length(self) -> f32 {
///         sqrt(self.x * self.x + self.y * self.y)
///     }
/// }
/// ```
#[derive(Clone, Debug)]
pub struct IrFunction {
    /// Function name
    pub name: String,

    /// Parameters (first is typically `self`)
    pub params: Vec<IrFunctionParam>,

    /// Return type (None = unit/void)
    pub return_type: Option<ResolvedType>,

    /// Function body expression
    pub body: IrExpr,
}

/// A function parameter in the IR.
#[derive(Clone, Debug)]
pub struct IrFunctionParam {
    /// Parameter name
    pub name: String,

    /// Parameter type (None for `self` parameter - type is inferred from impl block)
    pub ty: Option<ResolvedType>,

    /// Default value expression (if provided)
    pub default: Option<IrExpr>,
}

/// A field definition.
///
/// Used in structs, traits, and enum variants.
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
#[derive(Clone, Debug)]
pub struct IrGenericParam {
    /// Parameter name (e.g., "T")
    pub name: String,

    /// Trait constraints (e.g., T: Container)
    pub constraints: Vec<TraitId>,
}
