# Type Definitions

The shape of trait, struct, impl, enum, and module declarations. Method
bodies inside impls are described separately on
[Functions & Parameters](functions.md).

## TraitDef

```rust
pub struct TraitDef {
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub traits: Vec<Ident>,    // Trait composition (A + B + C)
    pub fields: Vec<FieldDef>, // Required fields
    pub methods: Vec<FnSig>,   // Required method signatures
    pub span: Span,
}
```

## StructDef

```rust
pub struct StructDef {
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub fields: Vec<StructField>, // Regular fields
    pub span: Span,
}
```

Trait conformance is declared separately via `impl Trait for Type` blocks —
not inline on the struct definition.

### StructField

```rust
pub struct StructField {
    pub mutable: bool,
    pub name: Ident,
    pub ty: Type,
    pub optional: bool,         // true if Type?
    pub default: Option<Expr>,  // Default value
    pub span: Span,
}
```

### FieldDef

Used in traits and enum variants.

```rust
pub struct FieldDef {
    pub mutable: bool,
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}
```

## ImplDef

Implementation body for structs. Supports inherent implementations, trait
implementations, and extern impl blocks.

```rust
pub struct ImplDef {
    pub trait_name: Option<Ident>,    // None for inherent impl, Some for trait impl
    pub trait_args: Vec<Type>,        // generic-trait args: `impl Foo<X> for Y` → [X]
    pub name: Ident,                  // Struct/enum being implemented
    pub generics: Vec<GenericParam>,
    pub functions: Vec<FnDef>,        // Method definitions
    pub is_extern: bool,              // true for `extern impl` blocks
    pub span: Span,
}
```

`trait_args` carries the concrete type arguments when the impl
instantiates a generic trait (`impl Container<I32> for Box`).
Empty for non-generic traits and inherent impls.

The `functions` vector contains [`FnDef`](functions.md#fndef) values —
their parameter shape and conventions are documented on the
[Functions & Parameters](functions.md) page.

## EnumDef

Sum types.

```rust
pub struct EnumDef {
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}
```

### EnumVariant

```rust
pub struct EnumVariant {
    pub name: Ident,
    pub fields: Vec<FieldDef>,  // Named fields (empty for simple variants)
    pub span: Span,
}
```

## ModuleDef

Namespace for grouping types.

```rust
pub struct ModuleDef {
    pub visibility: Visibility,
    pub name: Ident,
    pub definitions: Vec<Definition>,
    pub span: Span,
}
```
