# AST Examples

Worked examples showing how source maps to the AST. For the IR shape of
the same constructs, see [Worked Examples](../ir/examples.md) in the IR
reference.

## Simple Struct

**FormaLang source:**

```formalang
pub struct User {
    name: String,
    age: I32
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
                ├── ty: Type::Primitive(I32)
                ├── optional: false
                └── default: None
```

## Enum with Variants

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

## Generic Struct with Trait

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
        │           └── [0] GenericConstraint::Trait { name: "Container", args: [] }
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

## Impl Block with Functions

**FormaLang source:**

```formalang
pub struct Counter {
    count: I32
}

impl Counter {
    fn increment(self) -> I32 {
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
        ├── trait_args: []
        ├── name: "Counter"
        ├── generics: []
        └── functions:
            ├── [0] FnDef
            │   ├── name: "increment"
            │   ├── params: [FnParam { convention: Let, external_label: None, name: "self", ty: None }]
            │   ├── return_type: Some(Type::Primitive(I32))
            │   └── body: Expr::BinaryOp { ... }
            └── [1] FnDef
                ├── name: "display"
                ├── params: [FnParam { convention: Let, external_label: None, name: "self", ty: None }]
                ├── return_type: Some(Type::Primitive(String))
                └── body: Expr::IfExpr { ... }
```

## Trait Implementation

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
        ├── trait_args: []
        ├── name: "Counter"
        ├── generics: []
        └── functions:
            └── [0] FnDef
                ├── name: "draw"
                ├── params: [FnParam { name: "self", ty: None }]
                ├── return_type: Some(Type::Primitive(String))
                └── body: Expr::BinaryOp { ... }
```

## Match Expression with Wildcard

**FormaLang source:**

```formalang
match status {
    .active: Label(text: "Online"),
    .inactive: Label(text: "Offline"),
    _: Label(text: "Unknown")
}
```

**AST structure:**

```text
Expr::MatchExpr
├── scrutinee: Expr::Reference { path: ["status"] }
└── arms:
    ├── [0] MatchArm
    │   ├── pattern: Pattern::Variant { name: "active", bindings: [] }
    │   └── body: Expr::Invocation { path: ["Label"], ... }
    ├── [1] MatchArm
    │   ├── pattern: Pattern::Variant { name: "inactive", bindings: [] }
    │   └── body: Expr::Invocation { path: ["Label"], ... }
    └── [2] MatchArm
        ├── pattern: Pattern::Wildcard
        └── body: Expr::Invocation { path: ["Label"], ... }
```

## Block Expression

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

## For Expression

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

## Closure Expression

**FormaLang source:**

```formalang
let add = |x: I32, y: I32| x + y
let scale: mut I32 -> I32 = mut n -> n
```

**AST structure:**

```text
Statement::Let                          // let add = ...
├── pattern: BindingPattern::Simple("add")
└── value: Expr::ClosureExpr
    ├── params:
    │   ├── [0] ClosureParam { convention: Let, name: "x", ty: Some(I32) }
    │   └── [1] ClosureParam { convention: Let, name: "y", ty: Some(I32) }
    └── body: Expr::BinaryOp { op: Add, ... }

Statement::Let                          // let scale: mut I32 -> I32 = ...
├── pattern: BindingPattern::Simple("scale")
├── type_annotation: Some(Type::Closure {
│       params: [(Mut, I32)],
│       ret: I32
│   })
└── value: Expr::ClosureExpr
    ├── params:
    │   └── [0] ClosureParam { convention: Mut, name: "n", ty: None }
    └── body: Expr::Reference { path: ["n"] }
```
