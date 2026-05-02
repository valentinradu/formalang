# Obtaining the IR

Compile a `.fv` source string to a fully type-resolved `IrModule`:

```rust
use formalang::compile_to_ir;

let source = r#"
pub struct User {
    name: String,
    age: I32
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

For multi-file projects, pair `compile_to_ir_with_resolver` with a
`FileSystemResolver` (or a custom `ModuleResolver` impl). See the
[Public API](../architecture/api.md) for the complete entry-point list.
