# Resolved Types

Every type in the IR is fully resolved. Unlike AST types which use
string names, resolved types use IDs that directly reference definitions.

## ResolvedType

```rust
pub enum ResolvedType {
    /// Primitive type (String, I32, I64, F32, F64, Boolean, Path, Regex, Never)
    Primitive(PrimitiveType),

    /// Reference to a struct definition
    Struct(StructId),

    /// Reference to a trait definition
    Trait(TraitId),

    /// Reference to an enum definition
    Enum(EnumId),

    /// Array type: [T]
    Array(Box<ResolvedType>),

    /// Range type: T..T — produced by `start..end` expressions and consumed
    /// by `for x in start..end { ... }` loops.
    Range(Box<ResolvedType>),

    /// Optional type: T?
    Optional(Box<ResolvedType>),

    /// Named tuple type: (name1: T1, name2: T2)
    Tuple(Vec<(String, ResolvedType)>),

    /// Generic type instantiation: Box<String> or Option<I32>
    Generic {
        /// The generic struct or enum being instantiated.
        base: GenericBase,
        args: Vec<ResolvedType>,
    },

    /// Unresolved type parameter (T) in generic definitions
    TypeParam(String),

    /// Reference to a type in another module (imported via `use`)
    External {
        module_path: Vec<String>,  // e.g., ["utils", "helpers"]
        name: String,              // Type name
        kind: ImportedKind,        // Struct, Trait, or Enum
        type_args: Vec<ResolvedType>,  // For generics
    },

    /// Dictionary type: [K: V]
    Dictionary {
        key_ty: Box<ResolvedType>,
        value_ty: Box<ResolvedType>,
    },

    /// General closure / function type: (T1, T2) -> R
    ///
    /// Each element is `(convention, type)` — convention constrains the
    /// **caller** of the closure. Event-handler shapes like
    /// `String -> Event` use this variant with the enum return type.
    Closure {
        param_tys: Vec<(ParamConvention, ResolvedType)>,
        return_ty: Box<ResolvedType>,
    },

    /// Typed-out-of-band error placeholder. Produced by IR lowering when an
    /// upstream `CompilerError` has already been pushed but the surrounding
    /// code still needs to materialise *some* `ResolvedType` to keep walking
    /// the AST. Backends should treat `Error` as unreachable: if it survives
    /// to code generation, the compile would already have returned the
    /// associated `CompilerError` to the caller. Replaced the previous
    /// stringly-typed `TypeParam("Unknown")` sentinel.
    Error,
}
```

## GenericBase

Target of a `Generic` instantiation — a generic struct, enum, or
trait. Traits appear here only inside generic constraints
(`<T: Foo<X>>`) and impl headers (`impl Foo<X> for Y`); FormaLang
has no dynamic dispatch, so a trait base never sits in a value-
type position. Match exhaustively when extracting the underlying ID.

```rust
pub enum GenericBase {
    Struct(StructId),
    Enum(EnumId),
    Trait(TraitId),
}
```

## Type Resolution Examples

| FormaLang Type | ResolvedType |
| -------------- | ------------ |
| `String` | `Primitive(PrimitiveType::String)` |
| `I32` / `I64` | `Primitive(PrimitiveType::I32)` / `Primitive(PrimitiveType::I64)` |
| `F32` / `F64` | `Primitive(PrimitiveType::F32)` / `Primitive(PrimitiveType::F64)` |
| `Boolean` | `Primitive(PrimitiveType::Boolean)` |
| `Path` | `Primitive(PrimitiveType::Path)` |
| `Regex` | `Primitive(PrimitiveType::Regex)` |
| `Never` | `Primitive(PrimitiveType::Never)` |
| `User` (local struct) | `Struct(StructId(n))` |
| `Named` (local trait) | `Trait(TraitId(n))` |
| `Status` (local enum) | `Enum(EnumId(n))` |
| `[String]` | `Array(Box::new(Primitive(String)))` |
| `0..10` | `Range(Box::new(Primitive(I32)))` |
| `String?` | `Optional(Box::new(Primitive(String)))` |
| `[[I32]]` | `Array(Box::new(Array(Box::new(Primitive(I32)))))` |
| `Box<String>` | `Generic { base: GenericBase::Struct(StructId(n)), args: [Primitive(String)] }` |
| `Option<I32>` | `Generic { base: GenericBase::Enum(EnumId(n)), args: [Primitive(I32)] }` |
| `(x: I32, y: I32)` | `Tuple(vec![("x", Primitive(I32)), ("y", Primitive(I32))])` |
| `T` (in generic) | `TypeParam("T")` |
| `Helper` (from `use utils::Helper`) | `External { module_path: ["utils"], name: "Helper", ... }` |
| `Box<String>` (from `use containers::Box`) | `External { module_path: ["containers"], name: "Box", type_args: [...] }` |
| `[String: I32]` | `Dictionary { key_ty: Primitive(String), value_ty: Primitive(I32) }` |
| `String, I32 -> Boolean` | `Closure { param_tys: [(Let, Primitive(String)), (Let, Primitive(I32))], return_ty: Primitive(Boolean) }` |
| `mut I32 -> Boolean` | `Closure { param_tys: [(Mut, Primitive(I32))], return_ty: Primitive(Boolean) }` |
| `sink String -> Boolean` | `Closure { param_tys: [(Sink, Primitive(String))], return_ty: Primitive(Boolean) }` |

## Display Names

```rust
impl ResolvedType {
    /// Get a display name for this type (useful for debugging/error messages)
    pub fn display_name(&self, module: &IrModule) -> String;
}

// Example usage
let ty = &field.ty;
println!("Field type: {}", ty.display_name(&module));
// Output: "[String]" or "User" or "Box<I32>"
```
