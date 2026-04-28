//! Newtype IDs for IR-level definitions. Each ID is a `u32` index into
//! the matching `Vec` on [`super::IrModule`].

/// ID for referencing struct definitions.
///
/// Use this to look up structs in [`crate::ir::IrModule::structs`]:
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
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct StructId(pub u32);

/// ID for referencing trait definitions.
///
/// Use this to look up traits in [`crate::ir::IrModule::traits`]:
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
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct TraitId(pub u32);

/// ID for referencing enum definitions.
///
/// Use this to look up enums in [`crate::ir::IrModule::enums`]:
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
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct EnumId(pub u32);

/// ID for referencing standalone function definitions.
///
/// Use this to look up functions in [`crate::ir::IrModule::functions`]:
/// ```
/// use formalang::compile_to_ir;
///
/// let source = "pub fn add(a: I32, b: I32) -> I32 { a + b }";
/// let module = compile_to_ir(source).unwrap();
/// let id = formalang::FunctionId(0);
/// let func_def = &module.functions[id.0 as usize];
/// assert_eq!(func_def.name, "add");
/// ```
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct FunctionId(pub u32);

/// ID for referencing impl blocks.
///
/// Use this to look up impl blocks in [`crate::ir::IrModule::impls`]. Impl IDs are
/// stable for the lifetime of an `IrModule` as long as the `impls` vector
/// is not reordered.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct ImplId(pub u32);

/// ID for a binding inside a function body.
///
/// Either a `Let` introduced by an
/// [`IrBlockStatement::Let`](super::block::IrBlockStatement) or a parameter
/// from [`IrFunctionParam`](super::IrFunctionParam). Unique within the
/// containing function only; not stable across functions. Assigned by
/// `ResolveReferencesPass`, with a fresh counter per function starting at 0.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct BindingId(pub u32);

/// Position of a field within an [`IrStruct`](super::IrStruct)'s `fields`.
///
/// For an [`IrEnum`](super::IrEnum) variant, indexes into that variant's
/// `fields`. Backends use this to compute layout offsets without re-doing
/// field-name lookups.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct FieldIdx(pub u32);

/// Position of a variant within an [`IrEnum`](super::IrEnum)'s `variants`.
///
/// Used by backends to drive `br_table` / `switch` emission against the
/// runtime tag.
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct VariantIdx(pub u32);

/// Position of a method within an [`IrImpl`](super::IrImpl) or [`IrTrait`].
///
/// For `DispatchKind::Static`, indexes into [`IrImpl::functions`]; for
/// `DispatchKind::Virtual`, into [`IrTrait::methods`].
///
/// [`IrTrait`]: super::IrTrait
/// [`IrTrait::methods`]: super::IrTrait
/// [`IrImpl::functions`]: super::IrImpl
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct MethodIdx(pub u32);

/// ID for a module-scope `let` binding in [`IrModule::lets`].
///
/// Distinct from [`BindingId`], which indexes function-local bindings.
///
/// [`IrModule::lets`]: super::IrModule
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct LetId(pub u32);
