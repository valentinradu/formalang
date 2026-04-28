use crate::ast::{FnSig, GenericParam, Type, Visibility};
use crate::location::Span;
use std::collections::HashMap;

/// Information about a trait with field requirements
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TraitInfo {
    pub visibility: Visibility,
    pub span: Span,
    /// Generic parameters
    pub generics: Vec<GenericParam>,
    /// Required fields, in source order. `Vec` (not map) so order and
    /// doc-comments survive to the IR.
    pub fields: Vec<FieldInfo>,
    /// Trait composition list (trait names this trait extends)
    pub composed_traits: Vec<String>,
    /// Required method signatures declared in the trait body
    pub methods: Vec<FnSig>,
    /// Joined `///` doc comments preceding this trait.
    pub doc: Option<String>,
}

/// Information about a let binding with type
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct LetInfo {
    pub visibility: Visibility,
    pub span: Span,
    /// Inferred type of the binding (optional, computed during semantic analysis)
    pub inferred_type: Option<String>,
    /// Joined `///` doc comments preceding this binding.
    pub doc: Option<String>,
}

/// Information about a struct
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct StructInfo {
    pub visibility: Visibility,
    pub span: Span,
    /// Generic parameters
    pub generics: Vec<GenericParam>,
    /// Regular fields
    pub fields: Vec<FieldInfo>,
    /// Track if impl block exists
    pub has_impl: bool,
    /// Joined `///` doc comments preceding this struct.
    pub doc: Option<String>,
}

/// Information about a field
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FieldInfo {
    pub name: String,
    pub ty: Type,
    /// Joined `///` doc comments preceding this field.
    pub doc: Option<String>,
}

/// Information about an inherent impl block (impl Struct)
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ImplInfo {
    pub struct_name: String,
    pub generics: Vec<GenericParam>,
    pub span: Span,
}

/// Information about a trait implementation (impl Trait for Struct)
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TraitImplInfo {
    /// The trait being implemented
    pub trait_name: String,
    /// The struct implementing the trait
    pub struct_name: String,
    /// Generic parameters
    pub generics: Vec<GenericParam>,
    /// Span for error reporting
    pub span: Span,
}

/// Information about an enum with its variants
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct EnumInfo {
    pub visibility: Visibility,
    pub span: Span,
    /// Generic parameters
    pub generics: Vec<GenericParam>,
    /// Variant name -> (arity, span)
    pub variants: HashMap<String, (usize, Span)>,
    /// Variant name -> ordered field definitions.
    ///
    /// Populated alongside `variants` so IR lowering of imported module
    /// enums can emit the full variant shape instead of empty placeholders.
    pub variant_fields: HashMap<String, Vec<FieldInfo>>,
    /// Traits this enum implements (from : Trait syntax)
    pub traits: Vec<String>,
    /// Joined `///` doc comments preceding this enum.
    pub doc: Option<String>,
}

/// Information about a module with its nested symbol table
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ModuleInfo {
    pub visibility: Visibility,
    pub span: Span,
    /// Nested symbol table containing the module's definitions
    pub symbols: super::SymbolTable,
}

/// Information about a single parameter in a function (stored in symbol table)
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ParamInfo {
    /// Parameter passing convention
    pub convention: crate::ast::ParamConvention,
    /// External call-site label (if specified separately from the internal name)
    pub external_label: Option<crate::ast::Ident>,
    pub name: crate::ast::Ident,
    pub ty: Option<Type>,
}

/// Information about a standalone function
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FunctionInfo {
    pub visibility: Visibility,
    pub span: Span,
    /// Parameter information including external labels
    pub params: Vec<ParamInfo>,
    /// Return type (None for unit/void)
    pub return_type: Option<Type>,
    /// Generic parameters declared on this function
    pub generics: Vec<GenericParam>,
    /// Joined `///` doc comments preceding this function.
    pub doc: Option<String>,
}

/// Kind of symbol (for error reporting)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SymbolKind {
    Trait,
    Struct,
    Impl,
    Enum,
    Let,
    Module,
    Function,
}

impl SymbolKind {
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Trait => "trait",
            Self::Struct => "struct",
            Self::Impl => "impl",
            Self::Enum => "enum",
            Self::Let => "let binding",
            Self::Function => "fn",
            Self::Module => "mod",
        }
    }
}

/// Errors that can occur during symbol import
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImportError {
    /// Imported item is not public
    PrivateItem { name: String, kind: SymbolKind },
    /// Imported item not found in module
    ItemNotFound {
        name: String,
        available: Vec<String>,
    },
}
