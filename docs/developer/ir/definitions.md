# Definition Types

Top-level definitions stored in the per-type vectors on
[`IrModule`](module.md). Functions live on a separate page —
see [Functions](functions.md). Module-level let bindings appear
at the bottom of this page as [`IrLet`](#irlet).

## IrStruct

```rust
pub struct IrStruct {
    /// The struct name
    pub name: String,

    /// Visibility (public or private)
    pub visibility: Visibility,

    /// Traits implemented by this struct, with optional generic-trait
    /// args (`impl Container<I32> for Foo` → entry with non-empty
    /// args). Empty args means a non-generic trait.
    pub traits: Vec<IrTraitRef>,

    /// Regular fields
    pub fields: Vec<IrField>,

    /// Generic type parameters
    pub generic_params: Vec<IrGenericParam>,
}
```

## IrTrait

```rust
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
```

## IrEnum

```rust
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

pub struct IrEnumVariant {
    /// The variant name
    pub name: String,

    /// Associated data fields (empty for unit variants)
    pub fields: Vec<IrField>,
}
```

## ImplTarget

Identifies what an impl block implements — a struct or an enum.

```rust
pub enum ImplTarget {
    Struct(StructId),
    Enum(EnumId),
}
```

## IrImpl

Impl blocks provide methods for a struct or enum.

```rust
pub struct IrImpl {
    /// The struct or enum this impl is for
    pub target: ImplTarget,

    /// `Some(IrTraitRef { trait_id, args })` for `impl Trait for Type`
    /// or `impl Trait<X> for Type`; `None` for inherent impls. Args
    /// are empty for non-generic traits and carry the concrete
    /// instantiation for generic-trait impls.
    pub trait_ref: Option<IrTraitRef>,

    /// Whether this is an `extern impl` block (all methods have
    /// `extern_abi = Some(_)` and `body = None`).
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
    pub fn trait_id(&self) -> Option<TraitId>;

    /// Returns the struct ID if `target` is a struct, otherwise `None`.
    pub fn struct_id(&self) -> Option<StructId>;

    /// Returns the enum ID if `target` is an enum, otherwise `None`.
    pub fn enum_id(&self) -> Option<EnumId>;
}
```

## IrField

Used in structs, traits, and enum variants:

```rust
pub struct IrField {
    /// Field name
    pub name: String,

    /// Resolved type
    pub ty: ResolvedType,

    /// Whether this field is mutable (mut keyword)
    pub mutable: bool,

    /// Whether this field is optional (T?)
    pub optional: bool,

    /// Default value expression, if any
    pub default: Option<IrExpr>,

    /// Joined `///` doc comments preceding this field, if any.
    pub doc: Option<String>,
}
```

## IrGenericParam

```rust
pub struct IrGenericParam {
    /// Parameter name (e.g., "T")
    pub name: String,

    /// Trait constraints. Each entry carries the constrained trait
    /// id plus zero or more concrete arg types — empty when the
    /// trait isn't generic (`T: Container`), populated for
    /// generic-trait constraints (`T: Container<I32>`).
    pub constraints: Vec<IrTraitRef>,
}
```

## IrTraitRef

A reference to a trait, optionally with concrete type arguments.
Used in two places: as the constraint shape on
[`IrGenericParam`](#irgenericparam) and as the
implements-relationship shape on [`IrImpl`](#irimpl) /
[`IrStruct.traits`](#irstruct). An empty `args` slot means the
trait isn't generic; a non-empty slot carries the instantiation so
monomorphisation can specialise generic traits.

```rust
pub struct IrTraitRef {
    pub trait_id: TraitId,
    pub args: Vec<ResolvedType>,
}

impl IrTraitRef {
    /// Construct a non-generic trait reference (no args).
    pub const fn simple(trait_id: TraitId) -> Self;
}
```

## IrLet

Module-level let bindings (constants and computed values stored on
[`IrModule.lets`](module.md#irmodule)):

```rust
pub struct IrLet {
    /// Binding name
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
```
