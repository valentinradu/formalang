# Files & Statements

Spans, locations, identifiers, and the root nodes of every parsed `.fv` file.

## Locations

### Span

Every AST node includes a `Span` that tracks its source location for
error reporting.

```rust
pub struct Span {
    pub start: Location,
    pub end: Location,
}
```

### Location

```rust
pub struct Location {
    pub offset: usize,  // Byte offset from start of file
    pub line: usize,    // Line number (1-indexed)
    pub column: usize,  // Column number (1-indexed, byte-based)
}
```

### Ident

Identifiers carry both their name and source location.

```rust
pub struct Ident {
    pub name: String,
    pub span: Span,
}
```

## Root Nodes

### File

The root node representing a complete `.fv` source file.

```rust
pub struct File {
    pub format_version: u32,        // Always FORMAT_VERSION (currently 1)
    pub statements: Vec<Statement>,
    pub span: Span,
}
```

`format_version` is set automatically by the parser. Tools that
deserialize serialized ASTs should check this field to detect
wire-format incompatibilities.

### Statement

Top-level statements in a file.

```rust
pub enum Statement {
    Use(UseStmt),
    Let(Box<LetBinding>),
    Definition(Box<Definition>),
}
```

### Definition

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

## Visibility

```rust
pub enum Visibility {
    Public,   // pub keyword
    Private,  // default (no modifier)
}
```
