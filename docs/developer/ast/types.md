# Type Expressions

The shape of every type written in source: used in field annotations,
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
}
```

The parser gives `Type::Ident` for every name that is not a primitive:
a struct, an enum, a trait, and a type parameter such as `T`. A path
such as `shapes::Point` is one `Ident` with the segments joined by
`::`. The
semantic pass decides what the name means. The type `Never` is
`Type::Primitive(PrimitiveType::Never)`.

## PrimitiveType

```rust
pub enum PrimitiveType {
    String,
    I32,
    I64,
    F32,
    F64,
    Boolean,
    Never,  // Uninhabited type: has no values
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
