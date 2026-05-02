# AST Overview

The FormaLang compiler produces a validated AST as a Rust data structure.
The AST represents the complete structure of a `.fv` source file after
parsing and semantic validation.

> **Note**: For code generation, use the
> [IR (Intermediate Representation)](../ir/overview.md) instead. The IR
> provides resolved types, linked references, and is optimized for
> backend code generation.

## Obtaining the AST

Use `compile_with_analyzer` for a fully validated AST plus the semantic
analyzer (useful for LSP tooling). For pure syntax inspection without
semantic checks, use `parse_only`.

```rust
use formalang::compile_with_analyzer;

let source = r#"
pub struct User {
    name: String,
    age: I32
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
