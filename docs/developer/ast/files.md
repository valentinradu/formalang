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
    pub doc: Option<String>,        // The `//!` lines at the start of the file
    pub statements: Vec<Statement>,
    pub span: Span,
}
```

`doc` holds the `//!` doc comments at the start of the file, joined
with newlines.

The AST has no serialized form. To compare two trees, use
`PartialEq` or `Debug`.

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

Each definition has a `doc: Option<String>` field. It holds the `///`
lines before the definition, joined with `\n`. A `LetBinding`, a
`StructField`, a `FieldDef` and a method `FnDef` have one too. A `use`
statement drops its doc comment.

## Visibility

```rust
pub enum Visibility {
    Public,   // pub keyword
    Private,  // default (no modifier); also the serde default
}
```
