# Pattern Matching

Two pattern shapes coexist in the AST:
[`Pattern`](#pattern) for `match` arms (enum-variant matching) and
[`BindingPattern`](#bindingpattern) for destructuring `let` bindings.

## MatchArm

```rust
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
    pub span: Span,
}
```

## Pattern

```rust
pub enum Pattern {
    Variant {
        name: Ident,
        bindings: Vec<Ident>,
    },
    Wildcard,  // _
}
```

`bindings` are positional. The first name binds the first field of the
variant, the second name binds the second field, and so on. The names
do not have to match the field names. The parser also makes a
`Pattern::Variant` for each arm of an `if let`; see
[Expressions](expressions.md#if-let).

## BindingPattern

For destructuring in `let` bindings (file-level and inside blocks).

```rust
pub enum BindingPattern {
    Simple(Ident),
    Array {
        elements: Vec<ArrayPatternElement>,
        span: Span,
    },
    Struct {
        fields: Vec<StructPatternField>,
        span: Span,
    },
    Tuple {
        elements: Vec<BindingPattern>,
        span: Span,
    },
}
```

### ArrayPatternElement

```rust
pub enum ArrayPatternElement {
    Binding(BindingPattern),
    Rest(Option<Ident>),  // ...rest or just ...
    Wildcard,             // _
}
```

### StructPatternField

```rust
pub struct StructPatternField {
    pub name: Ident,
    pub alias: Option<Ident>,  // field as alias
}
```
