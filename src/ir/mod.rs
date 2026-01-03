//! Intermediate Representation (IR) for FormaLang
//!
//! The IR is a type-resolved representation of FormaLang programs optimized for
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
//!     age: Number
//! }
//! "#;
//!
//! let module = compile_to_ir(source).unwrap();
//! assert_eq!(module.structs.len(), 1);
//! assert_eq!(module.structs[0].name, "User");
//! ```

mod dce;
mod expr;
mod fold;
mod lower;
mod types;
mod visitor;

pub use dce::{eliminate_dead_code, DeadCodeEliminator};
pub use expr::{EventBindingSource, EventFieldBinding, IrExpr, IrMatchArm};
pub use fold::{fold_constants, ConstantFolder};
pub use lower::lower_to_ir;
pub use types::{
    IrEnum, IrEnumVariant, IrField, IrFunction, IrFunctionParam, IrGenericParam, IrImpl, IrLet,
    IrStruct, IrTrait,
};
pub use visitor::{walk_expr, walk_expr_children, walk_module, IrVisitor};

use std::collections::HashMap;

use crate::ast::{PrimitiveType, Visibility};

/// ID for referencing struct definitions.
///
/// Use this to look up structs in [`IrModule::structs`]:
/// ```
/// use formalang::compile_to_ir;
///
/// let source = "pub struct User { name: String }";
/// let module = compile_to_ir(source).unwrap();
/// let id = formalang::StructId(0);
/// let struct_def = &module.structs[id.0 as usize];
/// assert_eq!(struct_def.name, "User");
/// ```
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct StructId(pub u32);

/// ID for referencing trait definitions.
///
/// Use this to look up traits in [`IrModule::traits`]:
/// ```
/// use formalang::compile_to_ir;
///
/// let source = "pub trait Named { name: String }";
/// let module = compile_to_ir(source).unwrap();
/// let id = formalang::TraitId(0);
/// let trait_def = &module.traits[id.0 as usize];
/// assert_eq!(trait_def.name, "Named");
/// ```
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct TraitId(pub u32);

/// ID for referencing enum definitions.
///
/// Use this to look up enums in [`IrModule::enums`]:
/// ```
/// use formalang::compile_to_ir;
///
/// let source = "pub enum Status { active, inactive }";
/// let module = compile_to_ir(source).unwrap();
/// let id = formalang::EnumId(0);
/// let enum_def = &module.enums[id.0 as usize];
/// assert_eq!(enum_def.name, "Status");
/// ```
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct EnumId(pub u32);

/// Kind of external type reference.
///
/// Used to distinguish between different definition types when referencing
/// types from other modules.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ExternalKind {
    /// External struct type
    Struct,
    /// External trait type
    Trait,
    /// External enum type
    Enum,
}

/// An import from another module.
///
/// Tracks which types were imported from external modules, enabling code
/// generators to emit proper import statements in target languages.
#[derive(Clone, Debug, PartialEq)]
pub struct IrImport {
    /// Logical module path (e.g., `["utils", "helpers"]`)
    pub module_path: Vec<String>,
    /// Items imported from this module
    pub items: Vec<IrImportItem>,
}

/// A single imported item from a module.
#[derive(Clone, Debug, PartialEq)]
pub struct IrImportItem {
    /// Name of the imported type
    pub name: String,
    /// Kind of type (struct, trait, or enum)
    pub kind: ExternalKind,
}

/// A fully resolved type.
///
/// Unlike AST types which use string names, resolved types use IDs that
/// directly reference definitions. This eliminates the need for symbol
/// table lookups during code generation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ResolvedType {
    /// Primitive type (String, Number, Boolean, Path, Regex)
    Primitive(PrimitiveType),

    /// Reference to a struct definition
    Struct(StructId),

    /// Reference to a trait definition
    Trait(TraitId),

    /// Reference to an enum definition
    Enum(EnumId),

    /// Array type: `[T]`
    Array(Box<ResolvedType>),

    /// Optional type: `T?`
    Optional(Box<ResolvedType>),

    /// Named tuple type: `(name1: T1, name2: T2)`
    Tuple(Vec<(String, ResolvedType)>),

    /// Generic type instantiation: `Box<String>`
    Generic {
        /// The generic struct being instantiated
        base: StructId,
        /// Type arguments
        args: Vec<ResolvedType>,
    },

    /// Unresolved type parameter (e.g., `T` in a generic definition)
    ///
    /// This variant is used within generic definitions where the actual
    /// type is not yet known. Code generators should handle this by
    /// emitting the type parameter name.
    TypeParam(String),

    /// Reference to a type defined in another module.
    ///
    /// This variant is used for types imported via `use` statements.
    /// Code generators should use this information to emit proper import
    /// statements in target languages.
    ///
    /// # Example
    ///
    /// For `use utils::Helper`, a field of type `Helper` becomes:
    /// ```text
    /// External {
    ///     module_path: ["utils"],
    ///     name: "Helper",
    ///     kind: ExternalKind::Struct,
    ///     type_args: [],
    /// }
    /// ```
    External {
        /// Logical module path (e.g., `["utils", "helpers"]`)
        module_path: Vec<String>,
        /// Type name in that module
        name: String,
        /// Kind of type (struct, trait, or enum)
        kind: ExternalKind,
        /// Type arguments for generic types (empty for non-generic)
        type_args: Vec<ResolvedType>,
    },

    /// Event mapping type: `() -> E` or `T -> E`
    ///
    /// Represents a restricted closure that maps input to an enum variant.
    /// Used for event handlers like `onChange: x -> .valueChanged(value: x)`.
    EventMapping {
        /// Parameter type (None for `() -> E`)
        param_ty: Option<Box<ResolvedType>>,
        /// Return type (the event enum type)
        return_ty: Box<ResolvedType>,
    },

    /// Dictionary type: `[K: V]`
    ///
    /// Maps keys of type K to values of type V.
    Dictionary {
        /// Key type
        key_ty: Box<ResolvedType>,
        /// Value type
        value_ty: Box<ResolvedType>,
    },
}

/// The root IR node containing all definitions.
///
/// Definitions are stored in vectors, indexed by their respective ID types.
/// For example, `StructId(0)` refers to `structs[0]`.
///
/// # Example
///
/// ```
/// use formalang::{compile_to_ir, StructId};
///
/// let source = "pub struct User { name: String }";
/// let module = compile_to_ir(source).unwrap();
/// let struct_id = StructId(0);
///
/// // Look up a struct by ID (direct indexing)
/// let struct_def = &module.structs[struct_id.0 as usize];
/// assert_eq!(struct_def.name, "User");
///
/// // Or use the helper method
/// let struct_def = module.get_struct(struct_id);
/// assert_eq!(struct_def.name, "User");
/// ```
#[derive(Clone, Debug, Default)]
pub struct IrModule {
    /// All struct definitions, indexed by StructId
    pub structs: Vec<IrStruct>,

    /// All trait definitions, indexed by TraitId
    pub traits: Vec<IrTrait>,

    /// All enum definitions, indexed by EnumId
    pub enums: Vec<IrEnum>,

    /// All impl blocks
    pub impls: Vec<IrImpl>,

    /// Module-level let bindings
    ///
    /// Contains all `let` declarations at the module level, such as
    /// theme colors, fonts, and shared configuration values.
    pub lets: Vec<IrLet>,

    /// Imports from other modules
    ///
    /// Contains information about all types imported from external modules,
    /// enabling code generators to emit proper import statements.
    pub imports: Vec<IrImport>,

    /// Mapping from struct names to IDs for lookup during lowering
    struct_names: HashMap<String, StructId>,

    /// Mapping from trait names to IDs for lookup during lowering
    trait_names: std::collections::HashMap<String, TraitId>,

    /// Mapping from enum names to IDs for lookup during lowering
    enum_names: std::collections::HashMap<String, EnumId>,

    /// Mapping from let binding names to their index in the lets vector
    let_names: HashMap<String, usize>,
}

impl IrModule {
    /// Create a new empty IR module.
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a struct by ID.
    ///
    /// # Panics
    ///
    /// Panics if the ID is out of bounds.
    pub fn get_struct(&self, id: StructId) -> &IrStruct {
        &self.structs[id.0 as usize]
    }

    /// Look up a trait by ID.
    ///
    /// # Panics
    ///
    /// Panics if the ID is out of bounds.
    pub fn get_trait(&self, id: TraitId) -> &IrTrait {
        &self.traits[id.0 as usize]
    }

    /// Look up an enum by ID.
    ///
    /// # Panics
    ///
    /// Panics if the ID is out of bounds.
    pub fn get_enum(&self, id: EnumId) -> &IrEnum {
        &self.enums[id.0 as usize]
    }

    /// Look up a struct ID by name.
    pub fn struct_id(&self, name: &str) -> Option<StructId> {
        self.struct_names.get(name).copied()
    }

    /// Look up a trait ID by name.
    pub fn trait_id(&self, name: &str) -> Option<TraitId> {
        self.trait_names.get(name).copied()
    }

    /// Look up an enum ID by name.
    pub fn enum_id(&self, name: &str) -> Option<EnumId> {
        self.enum_names.get(name).copied()
    }

    /// Add a struct and return its ID.
    pub(crate) fn add_struct(&mut self, name: String, s: IrStruct) -> StructId {
        let id = StructId(self.structs.len() as u32);
        self.struct_names.insert(name, id);
        self.structs.push(s);
        id
    }

    /// Add a trait and return its ID.
    pub(crate) fn add_trait(&mut self, name: String, t: IrTrait) -> TraitId {
        let id = TraitId(self.traits.len() as u32);
        self.trait_names.insert(name, id);
        self.traits.push(t);
        id
    }

    /// Add an enum and return its ID.
    pub(crate) fn add_enum(&mut self, name: String, e: IrEnum) -> EnumId {
        let id = EnumId(self.enums.len() as u32);
        self.enum_names.insert(name, id);
        self.enums.push(e);
        id
    }

    /// Add an impl block.
    pub(crate) fn add_impl(&mut self, i: IrImpl) {
        self.impls.push(i);
    }

    /// Look up a let binding by name.
    pub fn get_let(&self, name: &str) -> Option<&IrLet> {
        self.let_names.get(name).map(|&idx| &self.lets[idx])
    }

    /// Check if a let binding exists.
    pub fn has_let(&self, name: &str) -> bool {
        self.let_names.contains_key(name)
    }

    /// Add a let binding.
    pub(crate) fn add_let(&mut self, l: IrLet) {
        let idx = self.lets.len();
        self.let_names.insert(l.name.clone(), idx);
        self.lets.push(l);
    }
}

impl ResolvedType {
    /// Get a display name for this type.
    ///
    /// Useful for error messages and debugging. For code generation,
    /// prefer pattern matching on the variants directly.
    pub fn display_name(&self, module: &IrModule) -> String {
        match self {
            ResolvedType::Primitive(p) => match p {
                PrimitiveType::String => "String".to_string(),
                PrimitiveType::Number => "Number".to_string(),
                PrimitiveType::Boolean => "Boolean".to_string(),
                PrimitiveType::Path => "Path".to_string(),
                PrimitiveType::Regex => "Regex".to_string(),
                PrimitiveType::Never => "Never".to_string(),
                // GPU scalar types
                PrimitiveType::F32 => "f32".to_string(),
                PrimitiveType::I32 => "i32".to_string(),
                PrimitiveType::U32 => "u32".to_string(),
                PrimitiveType::Bool => "bool".to_string(),
                // GPU vector types (float)
                PrimitiveType::Vec2 => "vec2".to_string(),
                PrimitiveType::Vec3 => "vec3".to_string(),
                PrimitiveType::Vec4 => "vec4".to_string(),
                // GPU vector types (signed int)
                PrimitiveType::IVec2 => "ivec2".to_string(),
                PrimitiveType::IVec3 => "ivec3".to_string(),
                PrimitiveType::IVec4 => "ivec4".to_string(),
                // GPU vector types (unsigned int)
                PrimitiveType::UVec2 => "uvec2".to_string(),
                PrimitiveType::UVec3 => "uvec3".to_string(),
                PrimitiveType::UVec4 => "uvec4".to_string(),
                // GPU matrix types
                PrimitiveType::Mat2 => "mat2".to_string(),
                PrimitiveType::Mat3 => "mat3".to_string(),
                PrimitiveType::Mat4 => "mat4".to_string(),
            },
            ResolvedType::Struct(id) => module.get_struct(*id).name.clone(),
            ResolvedType::Trait(id) => module.get_trait(*id).name.clone(),
            ResolvedType::Enum(id) => module.get_enum(*id).name.clone(),
            ResolvedType::Array(inner) => format!("[{}]", inner.display_name(module)),
            ResolvedType::Optional(inner) => format!("{}?", inner.display_name(module)),
            ResolvedType::Tuple(fields) => {
                let fields_str: Vec<_> = fields
                    .iter()
                    .map(|(name, ty)| format!("{}: {}", name, ty.display_name(module)))
                    .collect();
                format!("({})", fields_str.join(", "))
            }
            ResolvedType::Generic { base, args } => {
                let base_name = module.get_struct(*base).name.clone();
                let args_str: Vec<_> = args.iter().map(|a| a.display_name(module)).collect();
                format!("{}<{}>", base_name, args_str.join(", "))
            }
            ResolvedType::TypeParam(name) => name.clone(),
            ResolvedType::External {
                name, type_args, ..
            } => {
                if type_args.is_empty() {
                    name.clone()
                } else {
                    let args_str: Vec<_> =
                        type_args.iter().map(|a| a.display_name(module)).collect();
                    format!("{}<{}>", name, args_str.join(", "))
                }
            }
            ResolvedType::EventMapping {
                param_ty,
                return_ty,
            } => {
                let param_str = match param_ty {
                    Some(ty) => ty.display_name(module),
                    None => "()".to_string(),
                };
                format!("{} -> {}", param_str, return_ty.display_name(module))
            }
            ResolvedType::Dictionary { key_ty, value_ty } => {
                format!(
                    "[{}: {}]",
                    key_ty.display_name(module),
                    value_ty.display_name(module)
                )
            }
        }
    }
}

impl Visibility {
    /// Check if this visibility is public.
    pub fn is_public(&self) -> bool {
        matches!(self, Visibility::Public)
    }
}
