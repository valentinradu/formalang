# Imports & Let Bindings

The two top-level statement shapes that aren't type definitions:
`use` imports and module-level `let` bindings.

## UseStmt

```rust
pub struct UseStmt {
    pub visibility: Visibility, // pub use for re-exports
    pub path: Vec<Ident>,       // Module path segments
    pub items: UseItems,        // What to import
    pub span: Span,
}
```

## UseItems

```rust
pub enum UseItems {
    Single(Ident),          // use module::Item
    Multiple(Vec<Ident>),   // use module::{A, B, C}
    Glob,                   // use module::* (imports all public symbols)
}
```

## LetBinding

File-level constants.

```rust
pub struct LetBinding {
    pub visibility: Visibility,
    pub mutable: bool,
    pub pattern: BindingPattern,
    pub type_annotation: Option<Type>,  // Optional: let x: String = "hello"
    pub value: Expr,
    pub span: Span,
}
```

The `pattern` field uses [`BindingPattern`](patterns.md#bindingpattern),
allowing array / struct / tuple destructuring in module-level lets.
