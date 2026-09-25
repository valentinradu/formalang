# Expressions

The full expression tree as parsed. After semantic analysis the same
expressions are lowered to [`IrExpr`](../ir/expressions.md) for code
generation.

## Expr

```rust
pub enum Expr {
    Literal {
        value: Literal,
        span: Span,
    },

    /// Unified invocation: struct instantiation or function call.
    /// Semantic analysis determines which based on the name.
    Invocation {
        path: Vec<Ident>,                 // Name/path being invoked
        type_args: Vec<Type>,             // Generic type arguments
        args: Vec<(Option<Ident>, Expr)>, // Named or positional args
        span: Span,
    },

    /// A call of the value of an expression: `make()(4)`.
    Call {
        callee: Box<Expr>,
        args: Vec<(Option<Ident>, Expr)>,
        span: Span,
    },

    EnumInstantiation {
        enum_name: Ident,        // `Status`, or `shapes::Status` as one name
        type_args: Vec<Type>,    // `Maybe<I32>.none` gives [I32]
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

    FieldAccess {
        object: Box<Expr>,
        field: Ident,
        span: Span,
    },

    ClosureExpr {
        params: Vec<ClosureParam>,
        return_type: Option<Type>,   // The parser always sets None
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
        args: Vec<(Option<Ident>, Expr)>,  // Arguments with optional labels
        span: Span,
    },

    Block {
        statements: Vec<BlockStatement>,
        result: Box<Expr>,
        span: Span,
    },
}
```

`Expr::span()` returns the span of any variant.

### Calls

The parser reads `name(args)` and `a::b::name(args)` as an
`Invocation`. The semantic pass decides if the name is a struct or a
function. The parser reads `(args)` after any other expression as a
`Call`: `make()(4)`, `f(x)(3)` and `(g)(1)` are calls of a value. A
`Call` lowers to `IrExpr::CallClosure`.

### Enum paths

`EnumInstantiation.enum_name` holds the whole path as one name, joined
with `::`: `shapes::Status.active` gives `enum_name` `shapes::Status`.
`type_args` holds the type arguments written on the path, so
`Maybe<I32>.none` gives `[I32]`. It is empty when the path writes
none. Serialisation omits an empty `type_args`, and deserialisation
reads a missing one as empty.

### Field paths

The parser appends a field to a `Reference` path: `user.name` is a
`Reference` with the path `[user, name]`. After any other expression,
`.field` gives a `FieldAccess`, for example `make().name`.

### `if let`

The parser changes `if let x = value { a } else { b }` into a
`MatchExpr` on `value` with two arms: `.some(x): a` and `.none: b`. So
the AST has no `if let` node.

### Closures

A closure is `(params) -> body`. The parentheses are required, also for
one parameter. The syntax has no place for a return type, so the parser
always sets `return_type` to `None`. The semantic pass reads a
`return_type` that a tool sets on a hand-built AST.

### Value paths

The parser reads `Name.variant` and `Name.variant(label: value)` as an
`EnumInstantiation` when `Name` starts with an uppercase letter. The
same text is a field access or a method call when `Name` is a value,
for example `G.slice(start: 1, end: 3)` on `let G: String`. The parser
cannot tell the two apart. After the semantic analyzer builds the
symbol table, it changes each `EnumInstantiation` whose name is not a
type or a trait into a `Reference` path or a `MethodCall`. The code is
in `src/semantic/value_paths.rs`.

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
    Nil,
}
```

## NumberLiteral

Discriminated payload for a numeric literal: preserves the exact
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

Operator precedence (higher binds tighter). All binary operators are
left-associative:

| Precedence | Operators            |
|------------|----------------------|
| 11         | `.method(...)`, call `(...)` (postfix) |
| 10         | `.field`, `[index]` (postfix) |
| 9          | `-`, `!` (prefix)    |
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
