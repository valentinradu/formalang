# IR Design Plan

**Status**: Approved
**Date**: 2025-11-23

## Overview

Introduce an Intermediate Representation (IR) layer between the validated AST and code generators. The IR provides resolved type information and linked references, making it easy for consumers to generate interfaces for TypeScript, Swift, and Kotlin.

## Motivation

The raw AST is insufficient for code generation because:

- Type references are unresolved strings
- Generic instantiation details not captured
- Trait implementation relationships not linked
- Expression types not annotated
- Consumers must correlate AST with symbol table manually

## Design Decisions

### 1. Separate IR (not Typed AST)

Create new data structures in `src/ir/` rather than annotating existing AST nodes.

**Rationale**: Clean separation, optimized for codegen, omits irrelevant syntax details (spans, use statements, etc.). Follows rustc's approach with HIR/THIR/MIR layering.

### 2. ID-Based References

Use newtype IDs for type-safe references:

```rust
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct StructId(pub u32);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct TraitId(pub u32);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct EnumId(pub u32);
```

**Rationale**: Copy, cheap to pass, O(1) Vec lookup, no lifetime complexity. Used by rust-analyzer and salsa.

### 3. Every Expression Carries Resolved Type

All `IrExpr` variants include their resolved type:

```rust
pub enum IrExpr {
    Literal { value: Literal, ty: ResolvedType },
    StructInst { struct_id: StructId, fields: Vec<(String, IrExpr)>, ty: ResolvedType },
    // ...
}
```

**Rationale**: Consumers don't need to re-infer types. Single source of truth for expression types.

### 4. Visitor Pattern for Traversal

Provide `IrVisitor` trait with default empty methods:

```rust
pub trait IrVisitor {
    fn visit_module(&mut self, module: &IrModule) { /* default walks children */ }
    fn visit_struct(&mut self, id: StructId, s: &IrStruct) {}
    fn visit_trait(&mut self, id: TraitId, t: &IrTrait) {}
    fn visit_enum(&mut self, id: EnumId, e: &IrEnum) {}
    fn visit_impl(&mut self, i: &IrImpl) {}
    fn visit_expr(&mut self, e: &IrExpr) {}
    fn visit_field(&mut self, f: &IrField) {}
}

pub fn walk_module<V: IrVisitor>(visitor: &mut V, module: &IrModule);
pub fn walk_expr<V: IrVisitor>(visitor: &mut V, expr: &IrExpr);
```

**Rationale**: Consumers implement only what they need. Producer controls traversal. Easy to extend without breaking consumers.

## IR Structure

### Module (Root)

```rust
pub struct IrModule {
    pub structs: Vec<IrStruct>,
    pub traits: Vec<IrTrait>,
    pub enums: Vec<IrEnum>,
    pub impls: Vec<IrImpl>,
}
```

### Resolved Type

```rust
pub enum ResolvedType {
    Primitive(PrimitiveType),
    Struct(StructId),
    Trait(TraitId),
    Enum(EnumId),
    Array(Box<ResolvedType>),
    Optional(Box<ResolvedType>),
    Tuple(Vec<(String, ResolvedType)>),
    Generic { base: StructId, args: Vec<ResolvedType> },
    TypeParam(String),  // Unresolved generic parameter
}
```

### Definitions

```rust
pub struct IrStruct {
    pub name: String,
    pub visibility: Visibility,
    pub traits: Vec<TraitId>,
    pub fields: Vec<IrField>,
    pub mount_fields: Vec<IrField>,
    pub generic_params: Vec<IrGenericParam>,
}

pub struct IrTrait {
    pub name: String,
    pub visibility: Visibility,
    pub composed_traits: Vec<TraitId>,
    pub fields: Vec<IrField>,
    pub mount_fields: Vec<IrField>,
    pub generic_params: Vec<IrGenericParam>,
}

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

pub struct IrImpl {
    pub struct_id: StructId,
    pub body: Vec<IrExpr>,
}

pub struct IrField {
    pub name: String,
    pub ty: ResolvedType,
    pub mutable: bool,
    pub optional: bool,
    pub default: Option<IrExpr>,
}

pub struct IrGenericParam {
    pub name: String,
    pub constraints: Vec<TraitId>,
}
```

### Expressions

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

pub struct IrMatchArm {
    pub variant: String,
    pub bindings: Vec<(String, ResolvedType)>,
    pub body: IrExpr,
}
```

## Compiler Pipeline

```text
Source
  │
  ▼
Lexer → Tokens
  │
  ▼
Parser → AST (File)
  │
  ▼
Semantic Analyzer → Validated AST + SymbolTable
  │
  ▼
IR Lowering (new) → IrModule
  │
  ▼
Code Generators → TypeScript / Swift / Kotlin
```

## Module Structure

```text
src/
├── ir/
│   ├── mod.rs          // IrModule, ResolvedType, ID types
│   ├── types.rs        // IrStruct, IrTrait, IrEnum, IrField
│   ├── expr.rs         // IrExpr, IrMatchArm
│   ├── visitor.rs      // IrVisitor trait, walk functions
│   └── lower.rs        // AST + SymbolTable → IrModule
```

## Public API

```rust
// Main entry point
pub fn compile_to_ir(source: &str) -> Result<IrModule, Vec<CompilerError>>;

// Or from existing compilation
pub fn lower_to_ir(ast: &File, symbols: &SymbolTable) -> Result<IrModule, Vec<CompilerError>>;
```

## Documentation

All IR types will be documented via rustdocs in code. This serves as the primary documentation for code generator consumers.

Key documentation requirements:

- Every public type has doc comment with example
- `IrModule` explains how to look up definitions by ID
- `IrVisitor` explains how to implement custom visitors
- `ResolvedType` explains all variants and when each is used
- `IrExpr` documents the `ty` field contract

## Implementation Order

1. Define ID types and `ResolvedType`
2. Define `IrStruct`, `IrTrait`, `IrEnum`, `IrField`
3. Define `IrExpr` variants
4. Define `IrModule`
5. Implement `IrVisitor` trait and walk functions
6. Implement `lower_to_ir` (AST + SymbolTable → IrModule)
7. Add `compile_to_ir` convenience function
8. Write tests

## References

- [HIR - Rust Compiler Dev Guide](https://rustc-dev-guide.rust-lang.org/hir.html)
- [THIR - Rust Compiler Dev Guide](https://rustc-dev-guide.rust-lang.org/thir.html)
- [Memory Management in rustc](https://rustc-dev-guide.rust-lang.org/memory.html)
