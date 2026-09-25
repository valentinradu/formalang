# Public API

The crate root (`src/lib.rs`) has these entry points. Each one reads the
source as a `&str`. The library reads no file, except through a
`ModuleResolver` when the source imports a module.

| Function | Returns | Use case |
| --- | --- | --- |
| `compile_to_ir(src)` | `Result<IrModule, Vec<CompilerError>>` | Code generation (canonical) |
| `compile_to_ir_with_path(src, path)` | `Result<IrModule, Vec<CompilerError>>` | Code generation with real source paths in the spans |
| `compile_to_ir_with_resolver(src, resolver)` | `Result<IrModule, Vec<CompilerError>>` | A program of several modules, then `MonomorphisePass` |
| `compile_to_ir_with_path_and_resolver(src, path, resolver)` | `Result<IrModule, Vec<CompilerError>>` | A program of several modules, with the source path, then `MonomorphisePass` |
| `compile_with_analyzer(src)` | `Result<(File, SemanticAnalyzer<FileSystemResolver>), Vec<CompilerError>>` | LSP, AST-level tooling |
| `compile_with_analyzer_and_resolver(src, resolver)` | `Result<(File, SemanticAnalyzer<R>), Vec<CompilerError>>` | The same, with a custom resolver |
| `parse_only(src)` | `Result<File, Vec<CompilerError>>` | Parse with no semantic analysis |
| `compile_and_report(src, filename)` | `Result<IrModule, String>` | CLI: `compile_to_ir` with the errors as a report |

`compile_to_ir`, `compile_to_ir_with_path` and `compile_with_analyzer`
resolve an import through a `FileSystemResolver` that starts at the
current directory. Give a resolver of your own for any other root:
a `FileSystemResolver` with another root, or a custom `ModuleResolver`
for modules in memory.

Each entry point that returns an `IrModule` gives one self-contained
module. The IR of each imported module links into it. See
[Obtaining the IR](../ir/obtaining.md).

`compile_to_ir_with_resolver` and `compile_to_ir_with_path_and_resolver`
run `MonomorphisePass` on the result. The other entry points do not:
their result can hold generic definitions. Add the pass through a
[`Pipeline`](plugins.md) when you need it.

## Cargo features

The crate has one feature, `serde`. It is off by default. It derives
`serde::Serialize` and `serde::Deserialize` on `IrModule` and on every
type in it. It also derives them on the AST types that the IR holds,
for example `Literal`, `PrimitiveType` and `Span`.

```toml
[dependencies]
formalang = { version = "0.0.9-beta", features = ["serde"] }
```

The JSON form of the IR is not a stable format. It can change in each
release. After you read a module, call `IrModule::rebuild_indices`
before a lookup by name.

The AST has no serialized form. `File` has no `format_version`, and
there is no `File::to_json` or `File::from_json`.

## The symbol table

`SemanticAnalyzer::symbols()` returns the `SymbolTable`. IR lowering and
LSP tools read it. Two parts of its public surface changed:

- `SymbolTable::define_trait_impl(trait_name, struct_name, generics,
  trait_args, span)` takes `trait_args: Vec<Type>`. These are the type
  arguments of a generic trait: `[I32]` for
  `impl Container<I32> for Box`. Give an empty vector for a trait with
  no type parameters.
- `TraitImplInfo` has the field `trait_args: Vec<Type>` with the same
  meaning.

A type has one impl of each trait instance. A second impl of the same
trait with the same `trait_args` for the same type is a duplicate. Two
impls with other `trait_args`, such as `Container<I32>` and
`Container<String>`, are two instances, and both are allowed.
