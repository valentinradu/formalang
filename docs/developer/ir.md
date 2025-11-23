# IR Reference

**Last Updated**: 2025-11-23

This document provides a reference for the FormaLang Intermediate Representation (IR). The IR is the recommended output for building code generators targeting TypeScript, Swift, Kotlin, or other languages.

> **Note**: For syntax analysis or source-level tooling, use the [AST](ast.md) instead. The IR is optimized for code generation, not source fidelity.

## Overview

The IR provides:

- **Resolved types** on every expression
- **Linked references** (IDs pointing to definitions, not string names)
- **Flattened structure** optimized for code generation
- **Visitor pattern** for traversal

## Obtaining the IR

```rust
use formalang::compile_to_ir;

let source = r#"
pub struct User(
    name: String,
    age: Number
)
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

## ID Types

The IR uses typed IDs for referencing definitions:

```rust
pub struct StructId(pub u32);
pub struct TraitId(pub u32);
pub struct EnumId(pub u32);
```

IDs index into the corresponding `Vec` in `IrModule`:

```rust
let struct_def = &module.structs[id.0 as usize];
```

## Module Structure

### IrModule

The root container for all IR definitions:

```rust
pub struct IrModule {
    pub structs: Vec<IrStruct>,
    pub traits: Vec<IrTrait>,
    pub enums: Vec<IrEnum>,
    pub impls: Vec<IrImpl>,
}
```

## Type System

### ResolvedType

Every type in the IR is fully resolved:

```rust
pub enum ResolvedType {
    Primitive(PrimitiveType),    // String, Number, Boolean, Path, Regex
    Struct(StructId),            // Reference to struct definition
    Trait(TraitId),              // Reference to trait definition
    Enum(EnumId),                // Reference to enum definition
    Array(Box<ResolvedType>),    // [T]
    Optional(Box<ResolvedType>), // T?
    Tuple(Vec<(String, ResolvedType)>),  // (name: T, ...)
    Generic {                    // Box<String>, Container<T>
        base: StructId,
        args: Vec<ResolvedType>,
    },
    TypeParam(String),           // Unresolved generic parameter (T)
}
```

## Definition Types

### IrStruct

```rust
pub struct IrStruct {
    pub name: String,
    pub visibility: Visibility,
    pub traits: Vec<TraitId>,           // Implemented traits
    pub fields: Vec<IrField>,
    pub mount_fields: Vec<IrField>,
    pub generic_params: Vec<IrGenericParam>,
}
```

### IrTrait

```rust
pub struct IrTrait {
    pub name: String,
    pub visibility: Visibility,
    pub composed_traits: Vec<TraitId>,  // Trait inheritance
    pub fields: Vec<IrField>,
    pub mount_fields: Vec<IrField>,
    pub generic_params: Vec<IrGenericParam>,
}
```

### IrEnum

```rust
pub struct IrEnum {
    pub name: String,
    pub visibility: Visibility,
    pub variants: Vec<IrEnumVariant>,
    pub generic_params: Vec<IrGenericParam>,
}

pub struct IrEnumVariant {
    pub name: String,
    pub fields: Vec<IrField>,
}
```

### IrImpl

```rust
pub struct IrImpl {
    pub struct_id: StructId,
    pub body: Vec<IrExpr>,
}
```

### IrField

```rust
pub struct IrField {
    pub name: String,
    pub ty: ResolvedType,
    pub mutable: bool,
    pub optional: bool,
    pub default: Option<IrExpr>,
}
```

### IrGenericParam

```rust
pub struct IrGenericParam {
    pub name: String,
    pub constraints: Vec<TraitId>,
}
```

## Expressions

Every expression carries its resolved type in the `ty` field:

```rust
pub enum IrExpr {
    Literal {
        value: Literal,
        ty: ResolvedType,
    },
    StructInst {
        struct_id: StructId,
        type_args: Vec<ResolvedType>,
        fields: Vec<(String, IrExpr)>,
        mounts: Vec<(String, IrExpr)>,
        ty: ResolvedType,
    },
    EnumInst {
        enum_id: EnumId,
        variant: String,
        fields: Vec<(String, IrExpr)>,
        ty: ResolvedType,
    },
    Array {
        elements: Vec<IrExpr>,
        ty: ResolvedType,
    },
    Tuple {
        fields: Vec<(String, IrExpr)>,
        ty: ResolvedType,
    },
    Reference {
        name: String,
        ty: ResolvedType,
    },
    FieldAccess {
        base: Box<IrExpr>,
        field: String,
        ty: ResolvedType,
    },
    BinaryOp {
        left: Box<IrExpr>,
        op: BinaryOperator,
        right: Box<IrExpr>,
        ty: ResolvedType,
    },
    If {
        condition: Box<IrExpr>,
        then_branch: Box<IrExpr>,
        else_branch: Option<Box<IrExpr>>,
        ty: ResolvedType,
    },
    For {
        var: String,
        var_ty: ResolvedType,
        collection: Box<IrExpr>,
        body: Box<IrExpr>,
        ty: ResolvedType,
    },
    Match {
        scrutinee: Box<IrExpr>,
        arms: Vec<IrMatchArm>,
        ty: ResolvedType,
    },
}
```

### IrMatchArm

```rust
pub struct IrMatchArm {
    pub variant: String,
    pub bindings: Vec<(String, ResolvedType)>,
    pub body: IrExpr,
}
```

## Visitor Pattern

The IR provides a visitor trait for traversal:

```rust
pub trait IrVisitor {
    fn visit_module(&mut self, module: &IrModule) { /* walks children */ }
    fn visit_struct(&mut self, id: StructId, s: &IrStruct) {}
    fn visit_trait(&mut self, id: TraitId, t: &IrTrait) {}
    fn visit_enum(&mut self, id: EnumId, e: &IrEnum) {}
    fn visit_impl(&mut self, i: &IrImpl) {}
    fn visit_expr(&mut self, e: &IrExpr) {}
    fn visit_field(&mut self, f: &IrField) {}
}
```

### Example: TypeScript Generator

```rust
use formalang::ir::{IrVisitor, IrModule, IrStruct, StructId};

struct TypeScriptGenerator {
    output: String,
}

impl IrVisitor for TypeScriptGenerator {
    fn visit_struct(&mut self, _id: StructId, s: &IrStruct) {
        self.output.push_str(&format!("interface {} {{\n", s.name));
        for field in &s.fields {
            let ts_type = self.resolve_type(&field.ty);
            self.output.push_str(&format!("  {}: {};\n", field.name, ts_type));
        }
        self.output.push_str("}\n\n");
    }
}

fn generate_typescript(module: &IrModule) -> String {
    let mut gen = TypeScriptGenerator { output: String::new() };
    formalang::ir::walk_module(&mut gen, module);
    gen.output
}
```

## Type Resolution Helpers

```rust
impl IrModule {
    /// Look up a struct by ID
    pub fn get_struct(&self, id: StructId) -> &IrStruct;

    /// Look up a trait by ID
    pub fn get_trait(&self, id: TraitId) -> &IrTrait;

    /// Look up an enum by ID
    pub fn get_enum(&self, id: EnumId) -> &IrEnum;
}

impl ResolvedType {
    /// Get the type name for display/codegen
    pub fn display_name(&self, module: &IrModule) -> String;
}
```

## Design Rationale

The IR design follows patterns from the Rust compiler:

- **Separate from AST**: Clean separation allows AST to preserve source fidelity while IR is optimized for codegen
- **ID-based references**: Copy-able, no lifetime complexity, O(1) lookup
- **Typed expressions**: Every expression knows its type, no re-inference needed
- **Visitor pattern**: Consumers implement only what they need

See [plans/ir-design.md](../../plans/ir-design.md) for the full design document.
