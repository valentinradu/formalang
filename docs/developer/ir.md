# IR Reference

**Last Updated**: 2025-11-27

This document provides a complete reference for the FormaLang Intermediate
Representation (IR). The IR is the recommended output for building code
generators targeting TypeScript, Swift, Kotlin, or other languages.

> **Note**: For syntax analysis or source-level tooling, use the
> [AST](ast.md) instead. The IR is optimized for code generation, not source
> fidelity.

## Overview

The IR is a type-resolved representation of FormaLang programs, produced after
semantic analysis. Unlike the AST which preserves source syntax, the IR provides:

- **Resolved types** on every expression
- **Linked references** (IDs pointing to definitions, not string names)
- **Flattened structure** optimized for code generation
- **Visitor pattern** for traversal

### Compiler Pipeline

```text
Source
  |
  v
Lexer -> Tokens
  |
  v
Parser -> AST (File)
  |
  v
Semantic Analyzer -> Validated AST + SymbolTable
  |
  v
IR Lowering -> IrModule  <-- This document
  |
  v
Code Generators -> TypeScript / Swift / Kotlin
```

### What the IR Provides

| Feature | AST | IR |
| ------- | --- | --- |
| Source locations (spans) | Yes | No |
| Type resolution | No | Yes |
| ID-based references | No | Yes |
| String type names | Yes | No |
| Use statements | Yes | No |
| Comments | Yes | No |
| Parentheses/grouping | Yes | No |

### What the IR Does NOT Include

The IR intentionally omits:

- **Source positions (Spans)**: Use the AST for error reporting
- **Use statements**: Already resolved during lowering
- **Comments**: Purely syntactic, not needed for codegen
- **Parentheses/grouping**: Expression structure is normalized
- **String type references**: All resolved to typed IDs
- **Module nesting**: Currently flattened (nested modules TODO)

## Obtaining the IR

### Main Entry Point

```rust
use formalang::compile_to_ir;

let source = r#"
pub struct User {
    name: String,
    age: Number
}
"#;

match compile_to_ir(source) {
    Ok(module) => {
        // module is the root IR node
        for (id, struct_def) in module.structs.iter().enumerate() {
            println!("Struct {}: {}", id, struct_def.name);
        }
    }
    Err(errors) => {
        for error in errors {
            eprintln!("Error: {}", error);
        }
    }
}
```

## Module Structure

### Architecture Overview

```text
IrModule (root)
|
+-- structs: Vec<IrStruct>
|   |
|   +-- name: String
|   +-- visibility: Visibility
|   +-- traits: Vec<TraitId> -----> points to IrTrait entries
|   +-- fields: Vec<IrField>
|   |   |
|   |   +-- name: String
|   |   +-- ty: ResolvedType (may contain StructId/TraitId/EnumId refs)
|   |   +-- mutable: bool
|   |   +-- optional: bool
|   |   +-- default: Option<IrExpr>
|   |
|   +-- mount_fields: Vec<IrField>
|   +-- generic_params: Vec<IrGenericParam>
|       |
|       +-- name: String
|       +-- constraints: Vec<TraitId>
|
+-- traits: Vec<IrTrait>
|   |
|   +-- (similar structure to IrStruct)
|   +-- composed_traits: Vec<TraitId> -----> trait inheritance
|
+-- enums: Vec<IrEnum>
|   |
|   +-- name: String
|   +-- visibility: Visibility
|   +-- variants: Vec<IrEnumVariant>
|   |   |
|   |   +-- name: String
|   |   +-- fields: Vec<IrField>
|   |
|   +-- generic_params: Vec<IrGenericParam>
|
+-- impls: Vec<IrImpl>
    |
    +-- struct_id: StructId -----> points to IrStruct
    +-- defaults: Vec<(String, IrExpr)>
```

### IrModule

The root container for all IR definitions:

```rust
pub struct IrModule {
    pub structs: Vec<IrStruct>,
    pub traits: Vec<IrTrait>,
    pub enums: Vec<IrEnum>,
    pub impls: Vec<IrImpl>,
    pub imports: Vec<IrImport>,  // External module imports
}
```

#### Lookup Methods

```rust
impl IrModule {
    /// Look up a struct by ID (panics if out of bounds)
    pub fn get_struct(&self, id: StructId) -> &IrStruct;

    /// Look up a trait by ID (panics if out of bounds)
    pub fn get_trait(&self, id: TraitId) -> &IrTrait;

    /// Look up an enum by ID (panics if out of bounds)
    pub fn get_enum(&self, id: EnumId) -> &IrEnum;

    /// Look up a struct ID by name
    pub fn struct_id(&self, name: &str) -> Option<StructId>;

    /// Look up a trait ID by name
    pub fn trait_id(&self, name: &str) -> Option<TraitId>;

    /// Look up an enum ID by name
    pub fn enum_id(&self, name: &str) -> Option<EnumId>;
}
```

### External Imports

When a module uses types from other modules via `use` statements, those types
are represented as `External` variants in `ResolvedType`. The `imports` field
tracks which external types are used.

#### IrImport

```rust
pub struct IrImport {
    /// Logical module path (e.g., ["utils", "helpers"])
    pub module_path: Vec<String>,
    /// Items imported from this module
    pub items: Vec<IrImportItem>,
}
```

#### IrImportItem

```rust
pub struct IrImportItem {
    /// Name of the imported type
    pub name: String,
    /// Kind of type (struct, trait, or enum)
    pub kind: ExternalKind,
}
```

#### ExternalKind

```rust
pub enum ExternalKind {
    Struct,
    Trait,
    Enum,
}
```

#### Using Imports in Code Generators

Code generators can use the imports to emit proper import statements:

```rust
fn generate_typescript(module: &IrModule) -> String {
    let mut output = String::new();

    // Generate import statements from the imports list
    for import in &module.imports {
        let path = import.module_path.join("/");
        let items: Vec<_> = import.items.iter().map(|i| &i.name).collect();
        output.push_str(&format!(
            "import {{ {} }} from '{}';\n",
            items.join(", "),
            path
        ));
    }

    // Generate local definitions
    for struct_def in &module.structs {
        // ... generate struct
    }

    output
}
```

When generating type references, handle `External` separately:

```rust
fn type_to_typescript(ty: &ResolvedType, module: &IrModule) -> String {
    match ty {
        ResolvedType::Struct(id) => module.get_struct(*id).name.clone(),
        ResolvedType::External { name, type_args, .. } => {
            if type_args.is_empty() {
                name.clone()
            } else {
                let args: Vec<_> = type_args
                    .iter()
                    .map(|t| type_to_typescript(t, module))
                    .collect();
                format!("{}<{}>", name, args.join(", "))
            }
        }
        // ... other cases
    }
}
```

## ID Types

The IR uses typed IDs for referencing definitions. IDs are simple newtypes
wrapping `u32`, making them copyable and cheap to pass around.

```rust
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct StructId(pub u32);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct TraitId(pub u32);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct EnumId(pub u32);
```

IDs index into the corresponding `Vec` in `IrModule`:

```rust
// Direct indexing
let struct_def = &module.structs[id.0 as usize];

// Or use helper method
let struct_def = module.get_struct(id);

// Lookup by name
if let Some(id) = module.struct_id("User") {
    let struct_def = module.get_struct(id);
}
```

### ID Type Safety

IDs are type-safe: you cannot accidentally use a `StructId` where a `TraitId`
is expected. This prevents a common class of bugs:

```rust
let struct_id = StructId(0);
let trait_id = TraitId(0);

// Compile error: types don't match
// module.get_struct(trait_id);
```

## Type System

### ResolvedType

Every type in the IR is fully resolved. Unlike AST types which use string
names, resolved types use IDs that directly reference definitions.

```rust
pub enum ResolvedType {
    /// Primitive type (String, Number, Boolean, Path, Regex, Never)
    Primitive(PrimitiveType),

    /// Reference to a struct definition
    Struct(StructId),

    /// Reference to a trait definition
    Trait(TraitId),

    /// Reference to an enum definition
    Enum(EnumId),

    /// Array type: [T]
    Array(Box<ResolvedType>),

    /// Optional type: T?
    Optional(Box<ResolvedType>),

    /// Named tuple type: (name1: T1, name2: T2)
    Tuple(Vec<(String, ResolvedType)>),

    /// Generic type instantiation: Box<String>
    Generic {
        base: StructId,
        args: Vec<ResolvedType>,
    },

    /// Unresolved type parameter (T) in generic definitions
    TypeParam(String),

    /// Reference to a type in another module (imported via `use`)
    External {
        module_path: Vec<String>,  // e.g., ["utils", "helpers"]
        name: String,              // Type name
        kind: ExternalKind,        // Struct, Trait, or Enum
        type_args: Vec<ResolvedType>,  // For generics
    },
}
```

### Type Resolution Examples

| FormaLang Type | ResolvedType |
| -------------- | ------------ |
| `String` | `Primitive(PrimitiveType::String)` |
| `Number` | `Primitive(PrimitiveType::Number)` |
| `Boolean` | `Primitive(PrimitiveType::Boolean)` |
| `User` (local struct) | `Struct(StructId(n))` |
| `Named` (local trait) | `Trait(TraitId(n))` |
| `Status` (local enum) | `Enum(EnumId(n))` |
| `[String]` | `Array(Box::new(Primitive(String)))` |
| `String?` | `Optional(Box::new(Primitive(String)))` |
| `[[Number]]` | `Array(Box::new(Array(Box::new(Primitive(Number)))))` |
| `Box<String>` | `Generic { base: StructId(n), args: [Primitive(String)] }` |
| `(x: Number, y: Number)` | `Tuple(vec![("x", Primitive(Number)), ("y", Primitive(Number))])` |
| `T` (in generic) | `TypeParam("T")` |
| `Helper` (from `use utils::Helper`) | `External { module_path: ["utils"], name: "Helper", ... }` |
| `Box<String>` (from `use containers::Box`) | `External { module_path: ["containers"], name: "Box", type_args: [...] }` |

### Display Names

```rust
impl ResolvedType {
    /// Get a display name for this type (useful for debugging/error messages)
    pub fn display_name(&self, module: &IrModule) -> String;
}

// Example usage
let ty = &field.ty;
println!("Field type: {}", ty.display_name(&module));
// Output: "[String]" or "User" or "Box<Number>"
```

## Definition Types

### IrStruct

```rust
pub struct IrStruct {
    /// The struct name
    pub name: String,

    /// Visibility (public or private)
    pub visibility: Visibility,

    /// Traits implemented by this struct
    pub traits: Vec<TraitId>,

    /// Regular fields
    pub fields: Vec<IrField>,

    /// Mount fields (UI container slots)
    pub mount_fields: Vec<IrField>,

    /// Generic type parameters
    pub generic_params: Vec<IrGenericParam>,
}
```

### IrTrait

```rust
pub struct IrTrait {
    /// The trait name
    pub name: String,

    /// Visibility (public or private)
    pub visibility: Visibility,

    /// Traits composed into this trait (trait inheritance)
    pub composed_traits: Vec<TraitId>,

    /// Required fields
    pub fields: Vec<IrField>,

    /// Required mount fields
    pub mount_fields: Vec<IrField>,

    /// Generic type parameters
    pub generic_params: Vec<IrGenericParam>,
}
```

### IrEnum

```rust
pub struct IrEnum {
    /// The enum name
    pub name: String,

    /// Visibility (public or private)
    pub visibility: Visibility,

    /// Enum variants
    pub variants: Vec<IrEnumVariant>,

    /// Generic type parameters
    pub generic_params: Vec<IrGenericParam>,
}

pub struct IrEnumVariant {
    /// The variant name
    pub name: String,

    /// Associated data fields (empty for unit variants)
    pub fields: Vec<IrField>,
}
```

### IrImpl

Impl blocks provide field defaults for a struct:

```rust
pub struct IrImpl {
    /// The struct this impl is for
    pub struct_id: StructId,

    /// Field defaults: (field_name, default_value)
    pub defaults: Vec<(String, IrExpr)>,
}
```

### IrField

Used in structs, traits, and enum variants:

```rust
pub struct IrField {
    /// Field name
    pub name: String,

    /// Resolved type
    pub ty: ResolvedType,

    /// Whether this field is mutable (mut keyword)
    pub mutable: bool,

    /// Whether this field is optional (T?)
    pub optional: bool,

    /// Default value expression, if any
    pub default: Option<IrExpr>,
}
```

### IrGenericParam

```rust
pub struct IrGenericParam {
    /// Parameter name (e.g., "T")
    pub name: String,

    /// Trait constraints (e.g., T: Container)
    pub constraints: Vec<TraitId>,
}
```

## Expressions

Every expression carries its resolved type in the `ty` field. This eliminates
the need for code generators to re-infer types.

### IrExpr

```rust
pub enum IrExpr {
    /// Literal value (string, number, boolean, etc.)
    Literal {
        value: Literal,
        ty: ResolvedType,
    },

    /// Struct instantiation: User(name: "Alice", age: 30)
    StructInst {
        struct_id: StructId,
        type_args: Vec<ResolvedType>,
        fields: Vec<(String, IrExpr)>,
        mounts: Vec<(String, IrExpr)>,
        ty: ResolvedType,
    },

    /// Enum variant instantiation: Status.active or .active
    EnumInst {
        enum_id: EnumId,
        variant: String,
        fields: Vec<(String, IrExpr)>,
        ty: ResolvedType,
    },

    /// Array literal: [1, 2, 3]
    Array {
        elements: Vec<IrExpr>,
        ty: ResolvedType,
    },

    /// Tuple literal: (x: 1, y: 2)
    Tuple {
        fields: Vec<(String, IrExpr)>,
        ty: ResolvedType,
    },

    /// Variable or field reference
    Reference {
        path: Vec<String>,
        ty: ResolvedType,
    },

    /// Binary operation: a + b, x == y, p && q
    BinaryOp {
        left: Box<IrExpr>,
        op: BinaryOperator,
        right: Box<IrExpr>,
        ty: ResolvedType,
    },

    /// Conditional expression: if cond { a } else { b }
    If {
        condition: Box<IrExpr>,
        then_branch: Box<IrExpr>,
        else_branch: Option<Box<IrExpr>>,
        ty: ResolvedType,
    },

    /// For loop: for item in items { body }
    For {
        var: String,
        var_ty: ResolvedType,
        collection: Box<IrExpr>,
        body: Box<IrExpr>,
        ty: ResolvedType,
    },

    /// Match expression: match x { A => ..., B => ... }
    Match {
        scrutinee: Box<IrExpr>,
        arms: Vec<IrMatchArm>,
        ty: ResolvedType,
    },
}
```

### Type Contract

The `ty` field is guaranteed correct after lowering:

| Expression | Type |
| ---------- | ---- |
| `Literal { value: Number(_), .. }` | `Primitive(Number)` |
| `Literal { value: String(_), .. }` | `Primitive(String)` |
| `Literal { value: Boolean(_), .. }` | `Primitive(Boolean)` |
| `BinaryOp { op: Add/Sub/Mul/Div/Mod, .. }` | Same as operands |
| `BinaryOp { op: Eq/Ne/Lt/Gt/Le/Ge, .. }` | `Primitive(Boolean)` |
| `BinaryOp { op: And/Or, .. }` | `Primitive(Boolean)` |
| `For { body, .. }` | `Array(body.ty())` |
| `If { then_branch, .. }` | Same as branches |
| `Match { arms, .. }` | Same as arm bodies |

### Getting Expression Type

```rust
impl IrExpr {
    /// Get the resolved type of this expression
    pub fn ty(&self) -> &ResolvedType;
}

// Example
let expr: &IrExpr = /* ... */;
let ty = expr.ty();
match ty {
    ResolvedType::Primitive(PrimitiveType::String) => {
        // Generate string handling code
    }
    ResolvedType::Array(inner) => {
        // Generate array handling code
    }
    // ...
}
```

### IrMatchArm

```rust
pub struct IrMatchArm {
    /// Variant name being matched
    pub variant: String,

    /// Bindings for associated data: (name, type)
    pub bindings: Vec<(String, ResolvedType)>,

    /// Body expression
    pub body: IrExpr,
}
```

## Visitor Pattern

The IR provides a visitor trait for traversal, allowing code generators to
process nodes without implementing manual traversal logic.

### IrVisitor Trait

```rust
pub trait IrVisitor {
    /// Visit entire module (default walks all children)
    fn visit_module(&mut self, module: &IrModule) {
        walk_module_children(self, module);
    }

    /// Visit a struct definition
    fn visit_struct(&mut self, _id: StructId, _s: &IrStruct) {}

    /// Visit a trait definition
    fn visit_trait(&mut self, _id: TraitId, _t: &IrTrait) {}

    /// Visit an enum definition
    fn visit_enum(&mut self, _id: EnumId, _e: &IrEnum) {}

    /// Visit an enum variant
    fn visit_enum_variant(&mut self, _v: &IrEnumVariant) {}

    /// Visit an impl block
    fn visit_impl(&mut self, _i: &IrImpl) {}

    /// Visit a field definition
    fn visit_field(&mut self, _f: &IrField) {}

    /// Visit an expression (default walks children)
    fn visit_expr(&mut self, e: &IrExpr) {
        walk_expr_children(self, e);
    }
}
```

### Walking Functions

```rust
/// Walk an entire IR module
pub fn walk_module<V: IrVisitor>(visitor: &mut V, module: &IrModule);

/// Walk children of a module (called by default visit_module)
pub fn walk_module_children<V: IrVisitor>(visitor: &mut V, module: &IrModule);

/// Walk an expression tree
pub fn walk_expr<V: IrVisitor>(visitor: &mut V, expr: &IrExpr);

/// Walk children of an expression (called by default visit_expr)
pub fn walk_expr_children<V: IrVisitor>(visitor: &mut V, expr: &IrExpr);
```

### Visitor Example: Type Counter

```rust
use formalang::compile_to_ir;
use formalang::ir::{
    IrVisitor, IrStruct, IrEnum, StructId, EnumId, walk_module
};

struct TypeCounter {
    struct_count: usize,
    enum_count: usize,
}

impl IrVisitor for TypeCounter {
    fn visit_struct(&mut self, _id: StructId, _s: &IrStruct) {
        self.struct_count += 1;
    }

    fn visit_enum(&mut self, _id: EnumId, _e: &IrEnum) {
        self.enum_count += 1;
    }
}

let source = r#"
pub struct User { name: String }
pub enum Status { active, inactive }
"#;
let module = compile_to_ir(source).unwrap();
let mut counter = TypeCounter { struct_count: 0, enum_count: 0 };
walk_module(&mut counter, &module);

assert_eq!(counter.struct_count, 1);
assert_eq!(counter.enum_count, 1);
```

## Complete Examples

### Simple Struct

**FormaLang source:**

```formalang
pub struct User {
    name: String,
    age: Number
}
```

**IR structure:**

```text
IrModule
+-- structs[0]: IrStruct
    +-- name: "User"
    +-- visibility: Public
    +-- traits: []
    +-- fields:
    |   +-- [0] IrField
    |   |   +-- name: "name"
    |   |   +-- ty: Primitive(String)
    |   |   +-- mutable: false
    |   |   +-- optional: false
    |   |   +-- default: None
    |   +-- [1] IrField
    |       +-- name: "age"
    |       +-- ty: Primitive(Number)
    |       +-- mutable: false
    |       +-- optional: false
    |       +-- default: None
    +-- mount_fields: []
    +-- generic_params: []
```

### Enum with Variants

**FormaLang source:**

```formalang
pub enum Status {
    active,
    inactive,
    pending(reason: String)
}
```

**IR structure:**

```text
IrModule
+-- enums[0]: IrEnum
    +-- name: "Status"
    +-- visibility: Public
    +-- variants:
    |   +-- [0] IrEnumVariant
    |   |   +-- name: "active"
    |   |   +-- fields: []
    |   +-- [1] IrEnumVariant
    |   |   +-- name: "inactive"
    |   |   +-- fields: []
    |   +-- [2] IrEnumVariant
    |       +-- name: "pending"
    |       +-- fields:
    |           +-- [0] IrField
    |               +-- name: "reason"
    |               +-- ty: Primitive(String)
    +-- generic_params: []
```

### Struct Implementing Trait

**FormaLang source:**

```formalang
pub trait Named {
    name: String
}

pub struct User: Named {
    name: String,
    age: Number
}
```

**IR structure:**

```text
IrModule
+-- traits[0]: IrTrait              // TraitId(0)
|   +-- name: "Named"
|   +-- visibility: Public
|   +-- composed_traits: []
|   +-- fields:
|   |   +-- [0] IrField
|   |       +-- name: "name"
|   |       +-- ty: Primitive(String)
|   +-- mount_fields: []
|   +-- generic_params: []
|
+-- structs[0]: IrStruct            // StructId(0)
    +-- name: "User"
    +-- visibility: Public
    +-- traits: [TraitId(0)]        // <-- linked to Named trait
    +-- fields:
    |   +-- [0] IrField { name: "name", ty: Primitive(String), ... }
    |   +-- [1] IrField { name: "age", ty: Primitive(Number), ... }
    +-- mount_fields: []
    +-- generic_params: []
```

### Generic Struct with Constraint

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

**IR structure:**

```text
IrModule
+-- traits[0]: IrTrait              // TraitId(0)
|   +-- name: "Container"
|   +-- fields:
|       +-- [0] IrField
|           +-- name: "items"
|           +-- ty: Array(Box::new(Primitive(String)))
|
+-- structs[0]: IrStruct            // StructId(0)
    +-- name: "Box"
    +-- visibility: Public
    +-- traits: []
    +-- fields:
    |   +-- [0] IrField
    |   |   +-- name: "content"
    |   |   +-- ty: TypeParam("T")  // Unresolved in definition
    |   |   +-- optional: false
    |   +-- [1] IrField
    |       +-- name: "label"
    |       +-- ty: Optional(Box::new(Primitive(String)))
    |       +-- optional: true
    +-- mount_fields: []
    +-- generic_params:
        +-- [0] IrGenericParam
            +-- name: "T"
            +-- constraints: [TraitId(0)]  // <-- must implement Container
```

### Struct with Cross-References

**FormaLang source:**

```formalang
enum Status { active, inactive }

struct Author {
    name: String
}

struct Book {
    title: String,
    author: Author,
    status: Status
}
```

**IR structure:**

```text
IrModule
+-- enums[0]: IrEnum                // EnumId(0)
|   +-- name: "Status"
|   +-- variants: [active, inactive]
|
+-- structs[0]: IrStruct            // StructId(0)
|   +-- name: "Author"
|   +-- fields:
|       +-- [0] IrField { name: "name", ty: Primitive(String) }
|
+-- structs[1]: IrStruct            // StructId(1)
    +-- name: "Book"
    +-- fields:
        +-- [0] IrField { name: "title", ty: Primitive(String) }
        +-- [1] IrField { name: "author", ty: Struct(StructId(0)) }  // linked!
        +-- [2] IrField { name: "status", ty: Enum(EnumId(0)) }      // linked!
```

### Impl Block with Expressions

**FormaLang source:**

```formalang
pub struct Counter {
    count: Number,
    display: String
}

impl Counter {
    count: 0,
    display: if count > 10 { "High" } else { "Low" }
}
```

**IR structure:**

```text
IrModule
+-- structs[0]: IrStruct            // StructId(0)
|   +-- name: "Counter"
|   +-- fields:
|       +-- [0] IrField { name: "count", ty: Primitive(Number) }
|       +-- [1] IrField { name: "display", ty: Primitive(String) }
|
+-- impls[0]: IrImpl
    +-- struct_id: StructId(0)      // Counter
    +-- defaults:
        +-- ("count", IrExpr::Literal { value: Number(0.0), ty: Primitive(Number) })
        +-- ("display", IrExpr::If {
                condition: IrExpr::BinaryOp {
                    left: IrExpr::Reference { path: ["count"], ty: Primitive(Number) },
                    op: Gt,
                    right: IrExpr::Literal { value: Number(10.0), ty: Primitive(Number) },
                    ty: Primitive(Boolean)
                },
                then_branch: IrExpr::Literal { value: String("High"), ty: Primitive(String) },
                else_branch: Some(IrExpr::Literal { value: String("Low"), ty: Primitive(String) }),
                ty: Primitive(String)
            })
```

### Match Expression

**FormaLang source:**

```formalang
enum Option {
    none,
    some(value: Number)
}

struct Display {
    opt: Option,
    text: String
}

impl Display {
    text: match opt {
        none => "Nothing",
        some(value) => "Got value"
    }
}
```

**IR structure:**

```text
IrModule
+-- enums[0]: IrEnum                // EnumId(0)
|   +-- name: "Option"
|   +-- variants:
|       +-- [0] IrEnumVariant { name: "none", fields: [] }
|       +-- [1] IrEnumVariant { name: "some", fields: [IrField { name: "value", ... }] }
|
+-- structs[0]: IrStruct            // StructId(0)
|   +-- name: "Display"
|   +-- fields:
|       +-- [0] IrField { name: "opt", ty: Enum(EnumId(0)) }
|       +-- [1] IrField { name: "text", ty: Primitive(String) }
|
+-- impls[0]: IrImpl
    +-- struct_id: StructId(0)
    +-- defaults:
        +-- ("text", IrExpr::Match {
                scrutinee: IrExpr::Reference { path: ["opt"], ty: Enum(EnumId(0)) },
                arms: [
                    IrMatchArm {
                        variant: "none",
                        bindings: [],
                        body: IrExpr::Literal { value: String("Nothing"), ty: Primitive(String) }
                    },
                    IrMatchArm {
                        variant: "some",
                        bindings: [("value", Primitive(Number))],
                        body: IrExpr::Literal { value: String("Got value"), ty: Primitive(String) }
                    }
                ],
                ty: Primitive(String)
            })
```

### For Expression

**FormaLang source:**

```formalang
struct List {
    items: [String],
    labels: [String]
}

impl List {
    labels: for item in items { item }
}
```

**IR structure:**

```text
IrModule
+-- structs[0]: IrStruct
|   +-- name: "List"
|   +-- fields:
|       +-- [0] IrField { name: "items", ty: Array(Primitive(String)) }
|       +-- [1] IrField { name: "labels", ty: Array(Primitive(String)) }
|
+-- impls[0]: IrImpl
    +-- struct_id: StructId(0)
    +-- defaults:
        +-- ("labels", IrExpr::For {
                var: "item",
                var_ty: Primitive(String),
                collection: IrExpr::Reference { path: ["items"], ty: Array(Primitive(String)) },
                body: IrExpr::Reference { path: ["item"], ty: Primitive(String) },
                ty: Array(Primitive(String))  // For produces array of body type
            })
```

## Building a Code Generator

### Complete TypeScript Generator Example

This example demonstrates a full TypeScript interface generator:

```rust
use formalang::compile_to_ir;
use formalang::ir::{
    IrModule, IrStruct, IrEnum, IrEnumVariant, IrField, IrVisitor,
    StructId, EnumId, ResolvedType, walk_module
};
use formalang::ast::PrimitiveType;

struct TypeScriptGenerator<'a> {
    module: &'a IrModule,
    output: String,
}

impl<'a> TypeScriptGenerator<'a> {
    fn new(module: &'a IrModule) -> Self {
        Self {
            module,
            output: String::new(),
        }
    }

    fn resolve_type(&self, ty: &ResolvedType) -> String {
        match ty {
            ResolvedType::Primitive(p) => match p {
                PrimitiveType::String => "string".to_string(),
                PrimitiveType::Number => "number".to_string(),
                PrimitiveType::Boolean => "boolean".to_string(),
                PrimitiveType::Path => "string".to_string(),
                PrimitiveType::Regex => "RegExp".to_string(),
                PrimitiveType::Never => "never".to_string(),
            },
            ResolvedType::Struct(id) => {
                self.module.get_struct(*id).name.clone()
            }
            ResolvedType::Trait(id) => {
                self.module.get_trait(*id).name.clone()
            }
            ResolvedType::Enum(id) => {
                self.module.get_enum(*id).name.clone()
            }
            ResolvedType::Array(inner) => {
                format!("{}[]", self.resolve_type(inner))
            }
            ResolvedType::Optional(inner) => {
                format!("{} | null", self.resolve_type(inner))
            }
            ResolvedType::Tuple(fields) => {
                let fields_str: Vec<_> = fields
                    .iter()
                    .map(|(name, ty)| format!("{}: {}", name, self.resolve_type(ty)))
                    .collect();
                format!("{{ {} }}", fields_str.join("; "))
            }
            ResolvedType::Generic { base, args } => {
                let base_name = &self.module.get_struct(*base).name;
                let args_str: Vec<_> = args
                    .iter()
                    .map(|a| self.resolve_type(a))
                    .collect();
                format!("{}<{}>", base_name, args_str.join(", "))
            }
            ResolvedType::TypeParam(name) => name.clone(),
        }
    }

    fn emit_field(&mut self, field: &IrField) {
        let ts_type = self.resolve_type(&field.ty);
        let optional = if field.optional { "?" } else { "" };
        self.output.push_str(&format!(
            "  {}{}: {};\n",
            field.name, optional, ts_type
        ));
    }
}

impl<'a> IrVisitor for TypeScriptGenerator<'a> {
    fn visit_struct(&mut self, _id: StructId, s: &IrStruct) {
        // Skip private structs
        if !s.visibility.is_public() {
            return;
        }

        // Generic parameters
        let generics = if s.generic_params.is_empty() {
            String::new()
        } else {
            let params: Vec<_> = s.generic_params
                .iter()
                .map(|p| p.name.clone())
                .collect();
            format!("<{}>", params.join(", "))
        };

        // Extends clause for traits
        let extends = if s.traits.is_empty() {
            String::new()
        } else {
            let traits: Vec<_> = s.traits
                .iter()
                .map(|id| self.module.get_trait(*id).name.clone())
                .collect();
            format!(" extends {}", traits.join(", "))
        };

        self.output.push_str(&format!(
            "export interface {}{}{} {{\n",
            s.name, generics, extends
        ));

        for field in &s.fields {
            self.emit_field(field);
        }

        self.output.push_str("}\n\n");
    }

    fn visit_enum(&mut self, _id: EnumId, e: &IrEnum) {
        if !e.visibility.is_public() {
            return;
        }

        // Generate discriminated union
        self.output.push_str(&format!(
            "export type {} =\n",
            e.name
        ));

        for (i, variant) in e.variants.iter().enumerate() {
            let sep = if i == e.variants.len() - 1 { ";" } else { " |" };

            if variant.fields.is_empty() {
                self.output.push_str(&format!(
                    "  | {{ type: \"{}\" }}{}\n",
                    variant.name, sep
                ));
            } else {
                let fields: Vec<_> = variant.fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name, self.resolve_type(&f.ty)))
                    .collect();
                self.output.push_str(&format!(
                    "  | {{ type: \"{}\"; {} }}{}\n",
                    variant.name, fields.join("; "), sep
                ));
            }
        }

        self.output.push('\n');
    }
}

fn generate_typescript(source: &str) -> Result<String, Vec<formalang::CompilerError>> {
    let module = compile_to_ir(source)?;
    let mut gen = TypeScriptGenerator::new(&module);
    walk_module(&mut gen, &module);
    Ok(gen.output)
}

// Usage
let source = r#"
pub trait Named {
    name: String
}

pub struct User: Named {
    name: String,
    age: Number,
    email: String?
}

pub enum Status {
    active,
    pending(reason: String),
    inactive
}
"#;

let typescript = generate_typescript(source).unwrap();
println!("{}", typescript);

// Output:
// export interface Named {
//   name: string;
// }
//
// export interface User extends Named {
//   name: string;
//   age: number;
//   email?: string | null;
// }
//
// export type Status =
//   | { type: "active" } |
//   | { type: "pending"; reason: string } |
//   | { type: "inactive" };
```

## Design Rationale

The IR design follows patterns from the Rust compiler (HIR/THIR/MIR):

### Why Separate from AST?

- **Clean separation**: AST preserves source fidelity, IR optimizes for codegen
- **No syntax noise**: IR omits spans, comments, use statements
- **Different consumers**: Linters use AST, code generators use IR

### Why ID-Based References?

- **Copyable**: IDs are `Copy`, no lifetime complexity
- **Cheap**: O(1) Vec lookup by index
- **Type-safe**: `StructId` cannot be used where `TraitId` expected
- **Stable**: IDs don't change when other definitions are added

### Why Type on Every Expression?

- **No re-inference**: Code generators don't need to re-derive types
- **Single source of truth**: Type is computed once during lowering
- **Simpler codegen**: Just read `expr.ty()` and emit appropriate code

### Why Visitor Pattern?

- **Selective processing**: Implement only methods you need
- **Controlled traversal**: Producer decides traversal order
- **Extensible**: New node types don't break existing visitors

## See Also

- [AST Reference](ast.md): For syntax analysis and source-level tooling
- [Architecture](architecture.md): Overall compiler architecture
- [IR Design Plan](../../plans/ir-design.md): Original design document
