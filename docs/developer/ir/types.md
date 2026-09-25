# Resolved Types

Every type in the IR is fully resolved. Unlike AST types which use
string names, resolved types use IDs that directly reference definitions.

## ResolvedType

```rust
pub enum ResolvedType {
    /// Primitive type (String, I32, I64, F32, F64, Boolean, Never)
    Primitive(PrimitiveType),

    /// Reference to a struct definition
    Struct(StructId),

    /// Reference to a trait definition
    Trait(TraitId),

    /// Reference to an enum definition
    Enum(EnumId),

    /// Named tuple type: (name1: T1, name2: T2)
    Tuple(Vec<(String, ResolvedType)>),

    /// Generic type instantiation: Box<String>, Optional<I32>, [I32]
    Generic {
        /// The generic struct, enum or trait.
        base: GenericBase,
        args: Vec<ResolvedType>,
    },

    /// Type parameter (T) inside a generic definition
    TypeParam(String),

    /// Reference to a type in another module
    External {
        module_path: Vec<String>,      // e.g., ["utils", "helpers"]
        name: String,                  // Type name
        kind: ImportedKind,            // Struct, Trait, Enum, Function or ModuleLet
        type_args: Vec<ResolvedType>,  // For generics
    },

    /// Closure / function type: (T1, T2) -> R
    ///
    /// Each element is `(convention, type)`. The convention constrains
    /// the caller of the closure.
    Closure {
        param_tys: Vec<(ParamConvention, ResolvedType)>,
        return_ty: Box<ResolvedType>,
    },

    /// Error placeholder. See below.
    Error,
}
```

## The built-in compound types

The IR has no special variant for an array, an optional, a dictionary,
a range or a sequence. The prelude declares each one as an ordinary
generic definition, and a type of that kind is a `Generic` over it:

| Source type | Prelude definition | `ResolvedType` |
| ----------- | ------------------ | -------------- |
| `[T]` | `struct Array<T>` | `Generic { base: Struct(array_id), args: [T] }` |
| a `for` result | `struct Seq<T>` | `Generic { base: Struct(seq_id), args: [T] }` |
| `[K: V]` | `struct Dictionary<K, V>` | `Generic { base: Struct(dictionary_id), args: [K, V] }` |
| `a..b` | `struct Range<T>` | `Generic { base: Struct(range_id), args: [T] }` |
| `T?` | `enum Optional<T>` | `Generic { base: Enum(optional_id), args: [T] }` |

The prelude comes first in each module, so these definitions have the
lowest ids: `Array`, `Seq`, `Dictionary` and `Range` are `StructId(0)`
to `StructId(3)`, and `Optional` is `EnumId(0)`. Do not write these
numbers in a backend. Use the accessors on `IrModule`:

```rust
impl IrModule {
    pub fn prelude_array_id(&self) -> Option<StructId>;
    pub fn prelude_seq_id(&self) -> Option<StructId>;
    pub fn prelude_dictionary_id(&self) -> Option<StructId>;
    pub fn prelude_range_id(&self) -> Option<StructId>;
    pub fn prelude_optional_id(&self) -> Option<EnumId>;

    /// `T` when `ty` is `[T]`, and so on for each carrier.
    pub fn array_element_ty<'a>(&self, ty: &'a ResolvedType) -> Option<&'a ResolvedType>;
    pub fn seq_element_ty<'a>(&self, ty: &'a ResolvedType) -> Option<&'a ResolvedType>;
    pub fn dictionary_kv_ty<'a>(&self, ty: &'a ResolvedType)
        -> Option<(&'a ResolvedType, &'a ResolvedType)>;
    pub fn range_element_ty<'a>(&self, ty: &'a ResolvedType) -> Option<&'a ResolvedType>;
    pub fn optional_inner_ty<'a>(&self, ty: &'a ResolvedType) -> Option<&'a ResolvedType>;

    /// True for `Array`, `Seq`, `Dictionary` and `Range`.
    pub fn is_prelude_struct(&self, id: StructId) -> bool;
    /// True for `Optional`.
    pub fn is_prelude_enum(&self, id: EnumId) -> bool;
    /// The structs and enums without the prelude definitions.
    pub fn user_structs(&self) -> impl Iterator<Item = &IrStruct>;
    pub fn user_enums(&self) -> impl Iterator<Item = &IrEnum>;
}
```

`MonomorphisePass` does not specialise these five definitions. After
the pass, a `Generic` over one of them is the final shape, and a
backend reads the element type from `args`.

## GenericBase

Target of a `Generic` instantiation: a generic struct, enum, or
trait. Traits appear here only inside generic constraints
(`<T: Foo<X>>`) and impl headers (`impl Foo<X> for Y`). FormaLang
has no dynamic dispatch, so a trait base never sits in a value
type position. Match exhaustively when you extract the ID.

```rust
pub enum GenericBase {
    Struct(StructId),
    Enum(EnumId),
    Trait(TraitId),
}
```

## External

The lowering of each module links in the modules that it imports (see
[Obtaining the IR](obtaining.md#programs-over-several-files)). An
imported struct, enum or trait is then a local definition with a local
id, so a type that names it is `Struct`, `Enum` or `Trait`. The public
entry points do not give `External` for a type that a `use` imports.

`External` stays in the IR for backends that build IR by hand or read
it from another tool. [`MonomorphisePass::with_imports`](../architecture/passes.md#monomorphisepass)
can replace each `External` with a local copy of the imported
definition.

## Error

The lowering gives `Error` when it has already recorded a
`CompilerError` for the node, and it must still give the node a type.
The compile then returns that error, so a module that reaches a backend
holds no `Error`. A backend can treat `Error` as unreachable.

## Type Resolution Examples

The ids below are the ids in a module with no other definitions:
the prelude takes the first ids.

| FormaLang Type | ResolvedType |
| -------------- | ------------ |
| `String` | `Primitive(PrimitiveType::String)` |
| `I32` / `I64` | `Primitive(PrimitiveType::I32)` / `Primitive(PrimitiveType::I64)` |
| `F32` / `F64` | `Primitive(PrimitiveType::F32)` / `Primitive(PrimitiveType::F64)` |
| `Boolean` | `Primitive(PrimitiveType::Boolean)` |
| `Never` | `Primitive(PrimitiveType::Never)` |
| `User` (local struct) | `Struct(StructId(4))` |
| `Named` (local trait) | `Trait(TraitId(0))` |
| `Status` (local enum) | `Enum(EnumId(1))` |
| `[String]` | `Generic { base: Struct(StructId(0)), args: [Primitive(String)] }` |
| `0..10` | `Generic { base: Struct(StructId(3)), args: [Primitive(I32)] }` |
| `String?` | `Generic { base: Enum(EnumId(0)), args: [Primitive(String)] }` |
| `[[I32]]` | `Generic { base: Struct(StructId(0)), args: [Generic { base: Struct(StructId(0)), args: [Primitive(I32)] }] }` |
| `[String: I32]` | `Generic { base: Struct(StructId(2)), args: [Primitive(String), Primitive(I32)] }` |
| `Box<String>` (local generic struct) | `Generic { base: Struct(StructId(4)), args: [Primitive(String)] }` |
| `Maybe<I32>` (local generic enum) | `Generic { base: Enum(EnumId(1)), args: [Primitive(I32)] }` |
| `(x: I32, y: I32)` | `Tuple([("x", Primitive(I32)), ("y", Primitive(I32))])` |
| `T` (in generic) | `TypeParam("T")` |
| `(String, I32) -> Boolean` | `Closure { param_tys: [(Let, Primitive(String)), (Let, Primitive(I32))], return_ty: Primitive(Boolean) }` |
| `(mut I32) -> Boolean` | `Closure { param_tys: [(Mut, Primitive(I32))], return_ty: Primitive(Boolean) }` |
| `(sink String) -> Boolean` | `Closure { param_tys: [(Sink, Primitive(String))], return_ty: Primitive(Boolean) }` |

## Display Names

```rust
impl ResolvedType {
    /// A display name for this type, for debug output and messages.
    pub fn display_name(&self, module: &IrModule) -> String;
}

// Example
let ty = &field.ty;
println!("Field type: {}", ty.display_name(&module));
// Output: "[String]" or "User" or "Box<I32>"
```

`display_name` writes the source form of a built-in carrier: `[T]`,
`T?`, `[K: V]` and `T..T`. A sequence has the form `Seq<T>`.
