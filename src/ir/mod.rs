//! Intermediate Representation (IR) for `FormaLang`
//!
//! The IR is a type-resolved representation of `FormaLang` programs optimized for
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

pub use dce::{
    eliminate_dead_code, eliminate_dead_code_expr, DeadCodeEliminationPass, DeadCodeEliminator,
};
pub use expr::{EventBindingSource, EventFieldBinding, IrBlockStatement, IrExpr, IrMatchArm};
pub use fold::{fold_constants, ConstantFolder, ConstantFoldingPass};
pub use lower::lower_to_ir;
pub use types::{
    ImplTarget, IrEnum, IrEnumVariant, IrField, IrFunction, IrFunctionParam, IrFunctionSig,
    IrGenericParam, IrImpl, IrLet, IrStruct, IrTrait,
};
pub use visitor::{
    walk_block_statement, walk_expr, walk_expr_children, walk_module, walk_module_children,
    IrVisitor,
};

use std::collections::HashMap;

use crate::ast::{PrimitiveType, Visibility};
use crate::error::CompilerError;
use crate::location::Span;

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
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
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
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
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
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct EnumId(pub u32);

/// ID for referencing standalone function definitions.
///
/// Use this to look up functions in [`IrModule::functions`]:
/// ```
/// use formalang::compile_to_ir;
///
/// let source = "pub fn add(a: Number, b: Number) -> Number { a + b }";
/// let module = compile_to_ir(source).unwrap();
/// let id = formalang::FunctionId(0);
/// let func_def = &module.functions[id.0 as usize];
/// assert_eq!(func_def.name, "add");
/// ```
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct FunctionId(pub u32);

/// Kind of external type reference.
///
/// Used to distinguish between different definition types when referencing
/// types from other modules.
#[expect(
    clippy::exhaustive_enums,
    reason = "IR types are matched exhaustively by code generators"
)]
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
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrImport {
    /// Logical module path (e.g., `["utils", "helpers"]`)
    pub module_path: Vec<String>,
    /// Items imported from this module
    pub items: Vec<IrImportItem>,
    /// Filesystem path to the source module file.
    ///
    /// Used by codegen backends to look up the cached `IrModule` for generating
    /// impl blocks from imported types. Populated from symbol table's
    /// `module_origins` during IR lowering.
    pub source_file: std::path::PathBuf,
}

/// A single imported item from a module.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Clone, Debug, PartialEq, Eq)]
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
#[expect(
    clippy::exhaustive_enums,
    reason = "IR types are matched exhaustively by code generators"
)]
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
    Array(Box<Self>),

    /// Optional type: `T?`
    Optional(Box<Self>),

    /// Named tuple type: `(name1: T1, name2: T2)`
    Tuple(Vec<(String, Self)>),

    /// Generic type instantiation: `Box<String>`
    Generic {
        /// The generic struct being instantiated
        base: StructId,
        /// Type arguments
        args: Vec<Self>,
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
        type_args: Vec<Self>,
    },

    /// Event mapping type: `() -> E` or `T -> E`
    ///
    /// Represents a restricted closure that maps input to an enum variant.
    /// Used for event handlers like `onChange: x -> .valueChanged(value: x)`.
    EventMapping {
        /// Parameter type (None for `() -> E`)
        param_ty: Option<Box<Self>>,
        /// Return type (the event enum type)
        return_ty: Box<Self>,
    },

    /// Dictionary type: `[K: V]`
    ///
    /// Maps keys of type K to values of type V.
    Dictionary {
        /// Key type
        key_ty: Box<Self>,
        /// Value type
        value_ty: Box<Self>,
    },

    /// Closure/function type: `(T1, T2) -> R`
    ///
    /// Represents a general closure type with multiple parameters.
    /// Unlike `EventMapping` which is restricted to enum variant returns,
    /// this represents arbitrary pure functions.
    Closure {
        /// Parameter types
        param_tys: Vec<Self>,
        /// Return type
        return_ty: Box<Self>,
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
/// let struct_def = module.get_struct(struct_id).expect("struct exists");
/// assert_eq!(struct_def.name, "User");
/// ```
#[derive(Clone, Debug, Default)]
pub struct IrModule {
    /// All struct definitions, indexed by `StructId`
    pub structs: Vec<IrStruct>,

    /// All trait definitions, indexed by `TraitId`
    pub traits: Vec<IrTrait>,

    /// All enum definitions, indexed by `EnumId`
    pub enums: Vec<IrEnum>,

    /// All impl blocks
    pub impls: Vec<IrImpl>,

    /// Module-level let bindings
    ///
    /// Contains all `let` declarations at the module level, such as
    /// theme colors, fonts, and shared configuration values.
    pub lets: Vec<IrLet>,

    /// Standalone function definitions
    ///
    /// Contains all standalone function definitions (outside of impl blocks).
    pub functions: Vec<IrFunction>,

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

    /// Mapping from function names to IDs for lookup during lowering
    function_names: HashMap<String, FunctionId>,

    /// Mapping from let binding names to their index in the lets vector
    let_names: HashMap<String, usize>,
}

impl IrModule {
    /// Create a new empty IR module.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a struct by ID. Returns `None` if the ID is out of bounds.
    #[must_use]
    pub fn get_struct(&self, id: StructId) -> Option<&IrStruct> {
        self.structs.get(id.0 as usize)
    }

    /// Look up a trait by ID. Returns `None` if the ID is out of bounds.
    #[must_use]
    pub fn get_trait(&self, id: TraitId) -> Option<&IrTrait> {
        self.traits.get(id.0 as usize)
    }

    /// Look up an enum by ID. Returns `None` if the ID is out of bounds.
    #[must_use]
    pub fn get_enum(&self, id: EnumId) -> Option<&IrEnum> {
        self.enums.get(id.0 as usize)
    }

    /// Look up a struct ID by name.
    #[must_use]
    pub fn struct_id(&self, name: &str) -> Option<StructId> {
        self.struct_names.get(name).copied()
    }

    /// Look up a trait ID by name.
    #[must_use]
    pub fn trait_id(&self, name: &str) -> Option<TraitId> {
        self.trait_names.get(name).copied()
    }

    /// Look up an enum ID by name.
    #[must_use]
    pub fn enum_id(&self, name: &str) -> Option<EnumId> {
        self.enum_names.get(name).copied()
    }

    /// Add a struct and return its ID.
    #[expect(
        clippy::result_large_err,
        reason = "CompilerError is large by design; callers push errors into a Vec so allocation is bounded"
    )]
    pub(crate) fn add_struct(
        &mut self,
        name: String,
        s: IrStruct,
    ) -> Result<StructId, CompilerError> {
        let id = u32::try_from(self.structs.len())
            .map(StructId)
            .map_err(|_| CompilerError::TooManyDefinitions {
                kind: "struct",
                span: Span::default(),
            })?;
        self.struct_names.insert(name, id);
        self.structs.push(s);
        Ok(id)
    }

    /// Add a trait and return its ID.
    #[expect(
        clippy::result_large_err,
        reason = "CompilerError is large by design; callers push errors into a Vec so allocation is bounded"
    )]
    pub(crate) fn add_trait(&mut self, name: String, t: IrTrait) -> Result<TraitId, CompilerError> {
        let id = u32::try_from(self.traits.len()).map(TraitId).map_err(|_| {
            CompilerError::TooManyDefinitions {
                kind: "trait",
                span: Span::default(),
            }
        })?;
        self.trait_names.insert(name, id);
        self.traits.push(t);
        Ok(id)
    }

    /// Add an enum and return its ID.
    #[expect(
        clippy::result_large_err,
        reason = "CompilerError is large by design; callers push errors into a Vec so allocation is bounded"
    )]
    pub(crate) fn add_enum(&mut self, name: String, e: IrEnum) -> Result<EnumId, CompilerError> {
        let id = u32::try_from(self.enums.len()).map(EnumId).map_err(|_| {
            CompilerError::TooManyDefinitions {
                kind: "enum",
                span: Span::default(),
            }
        })?;
        self.enum_names.insert(name, id);
        self.enums.push(e);
        Ok(id)
    }

    /// Add an impl block.
    pub(crate) fn add_impl(&mut self, i: IrImpl) {
        self.impls.push(i);
    }

    /// Look up a let binding by name.
    #[must_use]
    pub fn get_let(&self, name: &str) -> Option<&IrLet> {
        self.let_names.get(name).and_then(|&idx| self.lets.get(idx))
    }

    /// Check if a let binding exists.
    #[must_use]
    pub fn has_let(&self, name: &str) -> bool {
        self.let_names.contains_key(name)
    }

    /// Add a let binding.
    pub(crate) fn add_let(&mut self, l: IrLet) {
        let idx = self.lets.len();
        self.let_names.insert(l.name.clone(), idx);
        self.lets.push(l);
    }

    /// Look up a function by ID. Returns `None` if the ID is out of bounds.
    #[must_use]
    pub fn get_function(&self, id: FunctionId) -> Option<&IrFunction> {
        self.functions.get(id.0 as usize)
    }

    /// Look up a function ID by name.
    #[must_use]
    pub fn function_id(&self, name: &str) -> Option<FunctionId> {
        self.function_names.get(name).copied()
    }

    /// Add a standalone function and return its ID.
    #[expect(
        clippy::result_large_err,
        reason = "CompilerError is large by design; callers push errors into a Vec so allocation is bounded"
    )]
    pub(crate) fn add_function(
        &mut self,
        name: String,
        f: IrFunction,
    ) -> Result<FunctionId, CompilerError> {
        let id = u32::try_from(self.functions.len())
            .map(FunctionId)
            .map_err(|_| CompilerError::TooManyDefinitions {
                kind: "function",
                span: Span::default(),
            })?;
        self.function_names.insert(name, id);
        self.functions.push(f);
        Ok(id)
    }

    /// Rebuild the name-to-ID index maps from the current definition lists.
    ///
    /// Call this after any [`IrPass`] that adds, removes, or reorders
    /// definitions in `structs`, `traits`, `enums`, `functions`, or `lets`.
    /// Passes that only mutate fields within existing definitions do not need
    /// to call this.
    ///
    /// [`IrPass`]: crate::pipeline::IrPass
    pub fn rebuild_indices(&mut self) {
        self.struct_names.clear();
        for (idx, s) in self.structs.iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "checked by add_struct which errors before len reaches u32::MAX"
            )]
            self.struct_names
                .insert(s.name.clone(), StructId(idx as u32));
        }

        self.trait_names.clear();
        for (idx, t) in self.traits.iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "checked by add_trait which errors before len reaches u32::MAX"
            )]
            self.trait_names.insert(t.name.clone(), TraitId(idx as u32));
        }

        self.enum_names.clear();
        for (idx, e) in self.enums.iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "checked by add_enum which errors before len reaches u32::MAX"
            )]
            self.enum_names.insert(e.name.clone(), EnumId(idx as u32));
        }

        self.function_names.clear();
        for (idx, f) in self.functions.iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "checked by add_function which errors before len reaches u32::MAX"
            )]
            self.function_names
                .insert(f.name.clone(), FunctionId(idx as u32));
        }

        self.let_names.clear();
        for (idx, l) in self.lets.iter().enumerate() {
            self.let_names.insert(l.name.clone(), idx);
        }
    }
}

impl ResolvedType {
    /// Get a display name for this type.
    ///
    /// Useful for error messages and debugging. For code generation,
    /// prefer pattern matching on the variants directly.
    #[must_use]
    pub fn display_name(&self, module: &IrModule) -> String {
        match self {
            Self::Primitive(p) => match p {
                PrimitiveType::String => "String".to_string(),
                PrimitiveType::Number => "Number".to_string(),
                PrimitiveType::Boolean => "Boolean".to_string(),
                PrimitiveType::Path => "Path".to_string(),
                PrimitiveType::Regex => "Regex".to_string(),
                PrimitiveType::Never => "Never".to_string(),
            },
            Self::Struct(id) => module
                .get_struct(*id)
                .map_or_else(|| format!("<invalid-struct-{}>", id.0), |s| s.name.clone()),
            Self::Trait(id) => module
                .get_trait(*id)
                .map_or_else(|| format!("<invalid-trait-{}>", id.0), |t| t.name.clone()),
            Self::Enum(id) => module
                .get_enum(*id)
                .map_or_else(|| format!("<invalid-enum-{}>", id.0), |e| e.name.clone()),
            Self::Array(inner) => format!("[{}]", inner.display_name(module)),
            Self::Optional(inner) => format!("{}?", inner.display_name(module)),
            Self::Tuple(fields) => {
                let fields_str: Vec<_> = fields
                    .iter()
                    .map(|(name, ty)| format!("{}: {}", name, ty.display_name(module)))
                    .collect();
                format!("({})", fields_str.join(", "))
            }
            Self::Generic { base, args } => {
                let base_name = module.get_struct(*base).map_or_else(
                    || format!("<invalid-struct-{}>", base.0),
                    |s| s.name.clone(),
                );
                let args_str: Vec<_> = args.iter().map(|a| a.display_name(module)).collect();
                format!("{}<{}>", base_name, args_str.join(", "))
            }
            Self::TypeParam(name) => name.clone(),
            Self::External {
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
            Self::EventMapping {
                param_ty,
                return_ty,
            } => {
                let param_str = param_ty
                    .as_ref()
                    .map_or_else(|| "()".to_string(), |ty| ty.display_name(module));
                format!("{} -> {}", param_str, return_ty.display_name(module))
            }
            Self::Dictionary { key_ty, value_ty } => {
                format!(
                    "[{}: {}]",
                    key_ty.display_name(module),
                    value_ty.display_name(module)
                )
            }
            Self::Closure {
                param_tys,
                return_ty,
            } => {
                let params_str: Vec<_> = param_tys.iter().map(|t| t.display_name(module)).collect();
                format!(
                    "({}) -> {}",
                    params_str.join(", "),
                    return_ty.display_name(module)
                )
            }
        }
    }
}

impl Visibility {
    /// Check if this visibility is public.
    #[must_use]
    pub const fn is_public(&self) -> bool {
        matches!(self, Self::Public)
    }
}

/// Extract the simple type name from a potentially module-qualified path.
///
/// Given a path like `alignment::Horizontal`, returns `Horizontal`.
/// For simple names like `Button`, returns the name unchanged.
///
/// # Example
///
/// ```
/// use formalang::ir::simple_type_name;
///
/// assert_eq!(simple_type_name("alignment::Horizontal"), "Horizontal");
/// assert_eq!(simple_type_name("Button"), "Button");
/// ```
#[must_use]
pub fn simple_type_name(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name)
}
