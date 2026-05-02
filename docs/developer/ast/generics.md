# Generics

Type parameters and their constraints. Used wherever a definition
introduces generics — `TraitDef.generics`, `StructDef.generics`,
`EnumDef.generics`, `ImplDef.generics`, `FunctionDef.generics`.

## GenericParam

```rust
pub struct GenericParam {
    pub name: Ident,
    pub constraints: Vec<GenericConstraint>,
    pub span: Span,
}
```

## GenericConstraint

```rust
pub enum GenericConstraint {
    Trait { name: Ident, args: Vec<Type> },  // T: TraitName  or  T: TraitName<X, Y>
}
```

The `args` slot carries concrete type arguments when the constraint
references a generic trait — `<T: Container<I32>>` parses with
`args = [I32]`. Empty `args` means a non-generic trait bound.
