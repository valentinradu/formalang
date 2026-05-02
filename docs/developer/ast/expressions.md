# Expressions

The full expression tree as parsed. After semantic analysis the same
expressions are lowered to [`IrExpr`](../ir/expressions.md) for code
generation.

## Expr

```rust
pub enum Expr {
    Literal(Literal),

    /// Unified invocation: struct instantiation or function call.
    /// Semantic analysis determines which based on the name.
    Invocation {
        path: Vec<Ident>,                 // Name/path being invoked
        type_args: Vec<Type>,             // Generic type arguments
        args: Vec<(Option<Ident>, Expr)>, // Named or positional args
        span: Span,
    },

    EnumInstantiation {
        enum_name: Ident,
        variant: Ident,
        data: Vec<(Ident, Expr)>,
        span: Span,
    },

    InferredEnumInstantiation {
        variant: Ident,
        data: Vec<(Ident, Expr)>,
        span: Span,
    },

    Array {
        elements: Vec<Expr>,
        span: Span,
    },

    Tuple {
        fields: Vec<(Ident, Expr)>,
        span: Span,
    },

    Reference {
        path: Vec<Ident>,
        span: Span,
    },

    BinaryOp {
        left: Box<Expr>,
        op: BinaryOperator,
        right: Box<Expr>,
        span: Span,
    },

    UnaryOp {
        op: UnaryOperator,
        operand: Box<Expr>,
        span: Span,
    },

    ForExpr {
        var: Ident,
        collection: Box<Expr>,
        body: Box<Expr>,
        span: Span,
    },

    IfExpr {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Option<Box<Expr>>,
        span: Span,
    },

    MatchExpr {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
        span: Span,
    },

    Group {
        expr: Box<Expr>,
        span: Span,
    },

    DictLiteral {
        entries: Vec<(Expr, Expr)>,  // Key-value pairs
        span: Span,
    },

    DictAccess {
        dict: Box<Expr>,
        key: Box<Expr>,
        span: Span,
    },

    ClosureExpr {
        params: Vec<ClosureParam>,
        body: Box<Expr>,
        span: Span,
    },

    LetExpr {
        mutable: bool,
        pattern: BindingPattern,
        ty: Option<Type>,
        value: Box<Expr>,
        body: Box<Expr>,
        span: Span,
    },

    MethodCall {
        receiver: Box<Expr>,
        method: Ident,
        args: Vec<Expr>,
        span: Span,
    },

    Block {
        statements: Vec<BlockStatement>,
        result: Box<Expr>,
        span: Span,
    },
}
```

## BlockStatement

```rust
pub enum BlockStatement {
    Let {
        mutable: bool,
        pattern: BindingPattern,
        ty: Option<Type>,
        value: Expr,
        span: Span,
    },
    Assign {
        target: Expr,
        value: Expr,
        span: Span,
    },
    Expr(Expr),
}
```

## ClosureParam

```rust
pub struct ClosureParam {
    pub convention: ParamConvention,  // Let (default), Mut, or Sink
    pub name: Ident,
    pub ty: Option<Type>,
    pub span: Span,
}
```

`convention` on a `ClosureParam` constrains the **caller of the
closure**, not the closure itself. `Sink` means the caller gives up the
argument on each invocation; `Mut` means the caller must pass a mutable
binding.

## Literal

```rust
pub enum Literal {
    String(String),
    /// Numeric literal: see `NumberLiteral` for the carried payload.
    Number(NumberLiteral),
    Boolean(bool),
    Regex { pattern: String, flags: String },
    Path(String),
    Nil,
}
```

## NumberLiteral

Discriminated payload for a numeric literal — preserves the exact
integer digits as `i128` (so `i64`-and-narrower targets round-trip
without precision loss) or the float bits as `f64`. Carries the
optional source-level type suffix and the integer-vs-float source-
syntax kind so later passes can pick the resolved primitive without
re-running inference.

```rust
pub struct NumberLiteral {
    pub value: NumberValue,
    pub suffix: Option<NumericSuffix>,
    pub kind: NumberSourceKind,
}

pub enum NumberValue {
    Integer(i128),  // integer-syntax literals: 42, 1_000, 0xFF
    Float(f64),     // float-syntax literals: 3.14, 1e5
}

pub enum NumericSuffix {
    I32, I64, F32, F64,  // uppercase suffix: 42I64, 3.14F32
}

pub enum NumberSourceKind {
    Integer,  // unsuffixed default → I32
    Float,    // unsuffixed default → F64
}
```

## BinaryOperator

```rust
pub enum BinaryOperator {
    // Arithmetic
    Add, Sub, Mul, Div, Mod,
    // Comparison
    Lt, Gt, Le, Ge, Eq, Ne,
    // Logical
    And, Or,
    // Range
    Range,  // ..
}
```

Operator precedence (higher binds tighter):

| Precedence | Operators            |
|------------|----------------------|
| 6          | `*`, `/`, `%`        |
| 5          | `+`, `-`             |
| 4          | `<`, `>`, `<=`, `>=` |
| 3          | `==`, `!=`           |
| 2          | `&&`                 |
| 1          | `\|\|`               |
| 0          | `..`                 |

## UnaryOperator

```rust
pub enum UnaryOperator {
    Neg,  // -x
    Not,  // !x
}
```
