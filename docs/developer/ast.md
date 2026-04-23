# AST Reference

**Last Updated**: 2026-04-22

This document provides a complete reference for the FormaLang Abstract Syntax Tree (AST). The AST represents the syntactic structure of FormaLang source files and is useful for tooling, syntax analysis, and source-level transforms.

> **Note**: For code generation, use the [IR (Intermediate Representation)](ir.md) instead. The IR provides resolved types, linked references, and is optimized for backend code generation.

## Overview

The FormaLang compiler produces a validated AST as a Rust data structure. The AST represents the complete structure of a FormaLang source file after parsing and semantic validation.

### Obtaining the AST

Use `compile_with_analyzer` for a fully validated AST plus the semantic
analyzer (useful for LSP tooling). For pure syntax inspection without
semantic checks, use `parse_only`.

```rust
use formalang::compile_with_analyzer;

let source = r#"
pub struct User {
    name: String,
    age: Number
}
"#;

match compile_with_analyzer(source) {
    Ok((file, _analyzer)) => {
        // file is the root AST node
        for statement in &file.statements {
            // process statements
        }
    }
    Err(errors) => {
        for error in errors {
            eprintln!("Error: {}", error);
        }
    }
}
```

Use `parse_only` for syntax-only parsing without semantic validation:

```rust
use formalang::parse_only;

let file = parse_only(source)?;
```

## Node Reference

### Location Types

#### Span

Every AST node includes a `Span` that tracks its source location for error reporting.

```rust
pub struct Span {
    pub start: Location,
    pub end: Location,
}
```

#### Location

```rust
pub struct Location {
    pub offset: usize,  // Byte offset from start of file
    pub line: usize,    // Line number (1-indexed)
    pub column: usize,  // Column number (1-indexed, byte-based)
}
```

#### Ident

Identifiers carry both their name and source location.

```rust
pub struct Ident {
    pub name: String,
    pub span: Span,
}
```

### Root Nodes

#### File

The root node representing a complete `.fv` source file.

```rust
pub struct File {
    pub format_version: u32,        // Always FORMAT_VERSION (currently 1)
    pub statements: Vec<Statement>,
    pub span: Span,
}
```

`format_version` is set automatically by the parser. Tools that deserialize
serialized ASTs should check this field to detect wire-format incompatibilities.

#### Statement

Top-level statements in a file.

```rust
pub enum Statement {
    Use(UseStmt),
    Let(Box<LetBinding>),
    Definition(Box<Definition>),
}
```

#### Definition

Type definitions.

```rust
pub enum Definition {
    Trait(TraitDef),
    Struct(StructDef),
    Impl(ImplDef),
    Enum(EnumDef),
    Module(ModuleDef),
    Function(Box<FunctionDef>),
}
```

### Visibility

```rust
pub enum Visibility {
    Public,   // pub keyword
    Private,  // default (no modifier)
}
```

### Import Statements

#### UseStmt

```rust
pub struct UseStmt {
    pub visibility: Visibility, // pub use for re-exports
    pub path: Vec<Ident>,       // Module path segments
    pub items: UseItems,        // What to import
    pub span: Span,
}
```

#### UseItems

```rust
pub enum UseItems {
    Single(Ident),          // use module::Item
    Multiple(Vec<Ident>),   // use module::{A, B, C}
    Glob,                   // use module::* (imports all public symbols)
}
```

### Let Bindings

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

### Trait Definitions

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

### Struct Definitions

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

#### StructField

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

#### FieldDef

Used in traits and enum variants.

```rust
pub struct FieldDef {
    pub mutable: bool,
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}
```

### Impl Blocks

Implementation body for structs. Supports inherent implementations, trait
implementations, and extern impl blocks.

```rust
pub struct ImplDef {
    pub trait_name: Option<Ident>,    // None for inherent impl, Some for trait impl
    pub name: Ident,                  // Struct/enum being implemented
    pub generics: Vec<GenericParam>,
    pub functions: Vec<FnDef>,        // Method definitions
    pub is_extern: bool,              // true for `extern impl` blocks
    pub span: Span,
}
```

#### FnDef

Function definition inside an impl block.

```rust
pub struct FnDef {
    pub name: Ident,
    pub params: Vec<FnParam>,
    pub return_type: Option<Type>,
    pub body: Option<Expr>,    // None for extern fn / extern impl methods
    pub span: Span,
}
```

#### FnSig

A signature-only function declaration (no body). Used in trait method declarations.

```rust
pub struct FnSig {
    pub name: Ident,
    pub params: Vec<FnParam>,
    pub return_type: Option<Type>,
    pub span: Span,
}
```

#### ParamConvention

Controls how a parameter receives its argument (Mutable Value Semantics).

```rust
#[non_exhaustive]
#[derive(Default)]
pub enum ParamConvention {
    #[default]
    Let,   // Immutable reference — the callee cannot mutate the value
    Mut,   // Exclusive mutable access — callee may mutate the value
    Sink,  // Ownership transfer — the binding is consumed at the call site
}
```

Syntax summary (`Let` is the Rust enum variant name — there is no `let` keyword in FormaLang parameter position):

| Variant | FormaLang parameter syntax | Meaning                              |
|---------|----------------------------|--------------------------------------|
| `Let`   | `fn f(x: T)` (no keyword)  | Default; callee reads the value      |
| `Mut`   | `fn f(mut x: T)`           | Callee may mutate; arg must be `mut` |
| `Sink`  | `fn f(sink x: T)`          | Callee owns the value; arg is moved  |

All three use the same call syntax: `f(x)`. There is no annotation at the call site.

Semantic rules enforced during validation:

- A `Mut` parameter requires that the argument binding is declared `let mut` (or is another `mut`/`sink` parameter). Passing an immutable binding produces `MutabilityMismatch`.
- A `Sink` parameter consumes the argument binding. Any subsequent use of that binding produces `UseAfterSink`.

`self` parameters follow the same conventions: `fn f(self)`, `fn f(mut self)`, `fn f(sink self)`.

#### FnParam

```rust
pub struct FnParam {
    pub convention: ParamConvention, // Let (default), Mut, or Sink
    pub external_label: Option<Ident>, // External call-site label (e.g., `to` in `fn send(to name: String)`)
    pub name: Ident,
    pub ty: Option<Type>,            // None for bare `self` parameter
    pub default: Option<Expr>,       // Default value expression
    pub span: Span,
}
```

`external_label` mirrors Swift's argument-label convention: `fn send(to recipient: String)` creates a parameter whose external label is `to` and whose internal name is `recipient`. Callers write `send(to: "Alice")`. When `external_label` is `None`, the internal `name` is used at the call site.

### Standalone Functions

```rust
pub struct FunctionDef {
    pub visibility: Visibility,
    pub name: Ident,
    pub params: Vec<FnParam>,
    pub return_type: Option<Type>,
    pub body: Option<Expr>,    // None for `extern fn` declarations
    pub span: Span,
}
```

### Enum Definitions

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

#### EnumVariant

```rust
pub struct EnumVariant {
    pub name: Ident,
    pub fields: Vec<FieldDef>,  // Named fields (empty for simple variants)
    pub span: Span,
}
```

### Module Definitions

Namespace for grouping types.

```rust
pub struct ModuleDef {
    pub visibility: Visibility,
    pub name: Ident,
    pub definitions: Vec<Definition>,
    pub span: Span,
}
```

### Generics

#### GenericParam

```rust
pub struct GenericParam {
    pub name: Ident,
    pub constraints: Vec<GenericConstraint>,
    pub span: Span,
}
```

#### GenericConstraint

```rust
pub enum GenericConstraint {
    Trait(Ident),  // Trait bound: T: TraitName
}
```

### Type System

#### Type

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

#### PrimitiveType

```rust
pub enum PrimitiveType {
    String,
    Number,
    Boolean,
    Path,
    Regex,
    Never,
}
```

#### TupleField

```rust
pub struct TupleField {
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}
```

### Expressions

#### Expr

```rust
pub enum Expr {
    Literal(Literal),

    /// Unified invocation: struct instantiation or function call
    /// Semantic analysis determines which based on the name
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

#### BlockStatement

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

#### ClosureParam

```rust
pub struct ClosureParam {
    pub convention: ParamConvention,  // Let (default), Mut, or Sink
    pub name: Ident,
    pub ty: Option<Type>,
    pub span: Span,
}
```

`convention` on a `ClosureParam` constrains the **caller of the closure**, not the closure itself. `Sink` means the caller gives up the argument on each invocation; `Mut` means the caller must pass a mutable binding.

#### Literal

```rust
pub enum Literal {
    String(String),
    Number(f64),
    Boolean(bool),
    Regex { pattern: String, flags: String },
    Path(String),
    Nil,
    // Note: integer types (i32, u32) and GPU numeric types are not primitive
    // literals in FormaLang — use Number for all numeric values
}
```

#### BinaryOperator

```rust
pub enum BinaryOperator {
    // Arithmetic
    Add,    // +
    Sub,    // -
    Mul,    // *
    Div,    // /
    Mod,    // %

    // Comparison
    Lt,     // <
    Gt,     // >
    Le,     // <=
    Ge,     // >=
    Eq,     // ==
    Ne,     // !=

    // Logical
    And,    // &&
    Or,     // ||

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

#### UnaryOperator

```rust
pub enum UnaryOperator {
    Neg,  // -x
    Not,  // !x
}
```

### Pattern Matching

#### MatchArm

```rust
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
    pub span: Span,
}
```

#### Pattern

```rust
pub enum Pattern {
    Variant {
        name: Ident,
        bindings: Vec<Ident>,
    },
    Wildcard,  // _
}
```

### Binding Patterns

For destructuring in let bindings.

#### BindingPattern

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

#### ArrayPatternElement

```rust
pub enum ArrayPatternElement {
    Binding(BindingPattern),
    Rest(Option<Ident>),  // ...rest or just ...
    Wildcard,             // _
}
```

#### StructPatternField

```rust
pub struct StructPatternField {
    pub name: Ident,
    pub alias: Option<Ident>,  // field: alias
    pub span: Span,
}
```

## Examples

### Simple Struct

**FormaLang source:**

```formalang
pub struct User {
    name: String,
    age: Number
}
```

**AST structure:**

```text
File
└── statements[0]: Statement::Definition
    └── Definition::Struct
        ├── visibility: Public
        ├── name: "User"
        ├── generics: []
        └── fields:
            ├── [0] StructField
            │   ├── mutable: false
            │   ├── name: "name"
            │   ├── ty: Type::Primitive(String)
            │   ├── optional: false
            │   └── default: None
            └── [1] StructField
                ├── mutable: false
                ├── name: "age"
                ├── ty: Type::Primitive(Number)
                ├── optional: false
                └── default: None
```

### Enum with Variants

**FormaLang source:**

```formalang
pub enum Status {
    Active,
    Inactive,
    Pending(reason: String)
}
```

**AST structure:**

```text
File
└── statements[0]: Statement::Definition
    └── Definition::Enum
        ├── visibility: Public
        ├── name: "Status"
        ├── generics: []
        └── variants:
            ├── [0] EnumVariant
            │   ├── name: "Active"
            │   └── fields: []
            ├── [1] EnumVariant
            │   ├── name: "Inactive"
            │   └── fields: []
            └── [2] EnumVariant
                ├── name: "Pending"
                └── fields:
                    └── [0] FieldDef
                        ├── name: "reason"
                        └── ty: Type::Primitive(String)
```

### Generic Struct with Trait

**FormaLang source:**

```formalang
pub trait Container {
    items: [String]
}

pub struct Box<T: Container> {
    content: T,
    label: String?
}
```

**AST structure:**

```text
File
├── statements[0]: Statement::Definition
│   └── Definition::Trait
│       ├── visibility: Public
│       ├── name: "Container"
│       ├── generics: []
│       ├── traits: []
│       ├── fields:
│       │   └── [0] FieldDef
│       │       ├── name: "items"
│       │       └── ty: Type::Array(Type::Primitive(String))
│       └── methods: []
│
└── statements[1]: Statement::Definition
    └── Definition::Struct
        ├── visibility: Public
        ├── name: "Box"
        ├── generics:
        │   └── [0] GenericParam
        │       ├── name: "T"
        │       └── constraints:
        │           └── [0] GenericConstraint::Trait("Container")
        └── fields:
            ├── [0] StructField
            │   ├── name: "content"
            │   ├── ty: Type::TypeParameter("T")
            │   └── optional: false
            └── [1] StructField
                ├── name: "label"
                ├── ty: Type::Optional(Type::Primitive(String))
                └── optional: true
```

### Impl Block with Functions

**FormaLang source:**

```formalang
pub struct Counter {
    count: Number
}

impl Counter {
    fn increment(self) -> Number {
        self.count + 1
    }

    fn display(self) -> String {
        if self.count > 10 {
            "High"
        } else {
            "Low"
        }
    }
}
```

**AST structure:**

```text
File
├── statements[0]: Statement::Definition
│   └── Definition::Struct (Counter)
│
└── statements[1]: Statement::Definition
    └── Definition::Impl
        ├── trait_name: None
        ├── name: "Counter"
        ├── generics: []
        └── functions:
            ├── [0] FnDef
            │   ├── name: "increment"
            │   ├── params: [FnParam { convention: Let, external_label: None, name: "self", ty: None }]
            │   ├── return_type: Some(Type::Primitive(Number))
            │   └── body: Expr::BinaryOp { ... }
            └── [1] FnDef
                ├── name: "display"
                ├── params: [FnParam { convention: Let, external_label: None, name: "self", ty: None }]
                ├── return_type: Some(Type::Primitive(String))
                └── body: Expr::IfExpr { ... }
```

### Trait Implementation

**FormaLang source:**

```formalang
pub trait Drawable {
    fn draw(self) -> String
}

impl Drawable for Counter {
    fn draw(self) -> String {
        "Counter: " + self.count
    }
}
```

**AST structure:**

```text
File
└── statements[1]: Statement::Definition
    └── Definition::Impl
        ├── trait_name: Some("Drawable")
        ├── name: "Counter"
        ├── generics: []
        └── functions:
            └── [0] FnDef
                ├── name: "draw"
                ├── params: [FnParam { name: "self", ty: None }]
                ├── return_type: Some(Type::Primitive(String))
                └── body: Expr::BinaryOp { ... }
```

### Match Expression with Wildcard

**FormaLang source:**

```formalang
match status {
    Active => Label(text: "Online"),
    Inactive => Label(text: "Offline"),
    _ => Label(text: "Unknown")
}
```

**AST structure:**

```text
Expr::MatchExpr
├── scrutinee: Expr::Reference { path: ["status"] }
└── arms:
    ├── [0] MatchArm
    │   ├── pattern: Pattern::Variant { name: "Active", bindings: [] }
    │   └── body: Expr::Invocation { path: ["Label"], ... }
    ├── [1] MatchArm
    │   ├── pattern: Pattern::Variant { name: "Inactive", bindings: [] }
    │   └── body: Expr::Invocation { path: ["Label"], ... }
    └── [2] MatchArm
        ├── pattern: Pattern::Wildcard
        └── body: Expr::Invocation { path: ["Label"], ... }
```

### Block Expression

**FormaLang source:**

```formalang
{
    let x = compute_value()
    let y = x * 2
    Result(value: y)
}
```

**AST structure:**

```text
Expr::Block
├── statements:
│   ├── [0] BlockStatement::Let
│   │   ├── mutable: false
│   │   ├── pattern: BindingPattern::Simple("x")
│   │   └── value: Expr::Invocation { path: ["compute_value"], ... }
│   └── [1] BlockStatement::Let
│       ├── mutable: false
│       ├── pattern: BindingPattern::Simple("y")
│       └── value: Expr::BinaryOp { left: "x", op: Mul, right: 2 }
└── result: Expr::Invocation { path: ["Result"], args: [("value", "y")] }
```

### For Expression

**FormaLang source:**

```formalang
for item in items {
    ListItem(text: item)
}
```

**AST structure:**

```text
Expr::ForExpr
├── var: "item"
├── collection: Expr::Reference { path: ["items"] }
└── body: Expr::Invocation
    ├── path: ["ListItem"]
    └── args: [(Some("text"), Expr::Reference { path: ["item"] })]
```

### Closure Expression

**FormaLang source:**

```formalang
let add = |x: Number, y: Number| x + y
let scale: mut Number -> Number = mut n -> n
```

**AST structure:**

```text
Statement::Let                          // let add = ...
├── pattern: BindingPattern::Simple("add")
└── value: Expr::ClosureExpr
    ├── params:
    │   ├── [0] ClosureParam { convention: Let, name: "x", ty: Some(Number) }
    │   └── [1] ClosureParam { convention: Let, name: "y", ty: Some(Number) }
    └── body: Expr::BinaryOp { op: Add, ... }

Statement::Let                          // let scale: mut Number -> Number = ...
├── pattern: BindingPattern::Simple("scale")
├── type_annotation: Some(Type::Closure {
│       params: [(Mut, Number)],
│       ret: Number
│   })
└── value: Expr::ClosureExpr
    ├── params:
    │   └── [0] ClosureParam { convention: Mut, name: "n", ty: None }
    └── body: Expr::Reference { path: ["n"] }
```
