# Type Expressions

The shape of every type written in source — used in field annotations,
function signatures, generic arguments, and let-binding annotations.

## Type

```rust
pub enum Type {
    Primitive(PrimitiveType),
    Ident(Ident),                    // Type reference
    Generic {
        name: Ident,
        args: Vec<Type>,
        span: Span,
    },
    Array(Box<Type>),                // [T]
    Optional(Box<Type>),             // T?
    Tuple(Vec<TupleField>),          // (name1: T1, name2: T2)
    Dictionary {                     // [K: V]
        key: Box<Type>,
        value: Box<Type>,
    },
    Closure {                        // (T1, T2) -> R, with optional mut/sink per param
        params: Vec<(ParamConvention, Type)>,
        ret: Box<Type>,
    },
    Never,                           // Never type (!)
    TypeParameter(Ident),            // Reference to type parameter
}
```

## PrimitiveType

```rust
pub enum PrimitiveType {
    String,
    I32,
    I64,
    F32,
    F64,
    Boolean,
    Path,
    Regex,
    /// Uninhabited type — has no values.
    Never,
}
```

## TupleField

```rust
pub struct TupleField {
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}
```
