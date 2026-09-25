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

The loop above also prints the prelude structs (`Array`, `Seq`,
`Dictionary` and `Range`). The prelude comes first in each module. Use
`module.user_structs()` and `module.user_enums()` to skip it.

For multi-file projects, pair `compile_to_ir_with_resolver` with a
`FileSystemResolver` (or a custom `ModuleResolver` impl). See the
[Public API](../architecture/api.md) for the complete entry-point list.

## Programs over several files

Each entry point that takes a resolver gives one self-contained
`IrModule`. A backend does not need the IR of the imported modules.
The linker (`src/ir/link/` and `src/ir/lower/linked.rs`) builds this
module. It works in these steps:

1. The prelude is analysed and lowered one time for each process
   (`prelude_ir` in `src/lib.rs`).
2. Each imported module lowers before the modules that import it. Its
   lowering starts from a copy of the lowered prelude.
3. The linker copies each item of each imported module into the new
   module: the structs, enums, traits, impls, functions and `let`
   bindings. The copy gets new ids in the new module. An item that two
   imports share arrives one time. The linker finds a shared item by
   its origin: the module that defines it and its place in that module.
4. The module's own statements lower on top of these items. The
   lowering thus finds each imported item as a local item with a real
   id, and gives a `Struct`, `Enum`, `Trait` or `FunctionCall` with
   that id. It gives no `ResolvedType::External`.
5. The entry module lowers last, in the same way.

### Hidden names

Two modules can each have a private item of one name, but a name of a
module-level item must be unique in one `IrModule`. So the own items
of an imported module get a hidden name: `@`, the id of the module,
`::`, then the name. An example is `@2::helper`. A program cannot
write `@`, so a hidden name never collides with a name in the source.

While a module lowers, each item that it imports has its short name
(`use lib::open` gives `@3::open` the name `open`), so that the
lowering finds it. After the lowering, the item gets its hidden name
back. A call in the code of a module names the item of that module. A
private helper of an imported module is thus the function that the
module calls, even when the importer defines a function with the same
name.

### Final names

When the entry module is complete, `finish_entry` replaces each hidden
prefix with the module path that the source wrote in its `use`. For
example, `@2::double_x` becomes `geom::double_x`, and a module in
`lib/util.fv` gives `lib::util::one`. Sometimes this path is not
free: two modules would get one path, or the first segment of the path
is also the name of an inline `mod` of the entry module. Then the
module gets its id as a suffix, for example `geom#2`.

`finish_entry` also puts each imported struct, enum, trait and
function in the [module tree](module.md#irmodulenode-source-mod-hierarchy)
under the path of its module.

### What the result holds

For this program in `main.fv`:

```text
// geom.fv
pub struct Point { x: I32, y: I32 }
fn helper(n: I32) -> I32 { n * 2 }
pub fn double_x(p: Point) -> I32 { helper(n: p.x) }
let secret: I32 = 7

// main.fv
use geom::{Point, double_x}
fn helper(n: I32) -> I32 { n + 100 }
pub fn run() -> I32 { double_x(p: Point(x: 1, y: 2)) + helper(n: 1) }
```

`compile_to_ir_with_path_and_resolver` gives:

```text
structs:   Array, Seq, Dictionary, Range, geom::Point
functions: assert, geom::helper, geom::double_x, helper, run
lets:      geom::secret
modules:   [IrModuleNode { name: "geom", structs: [StructId(4)],
                           functions: [FunctionId(1), FunctionId(2)] }]
imports:   [IrImport { module_path: ["geom"],
                       items: [Point (Struct), double_x (Function)] }]
file_table: [main.fv, geom.fv]
```

- Each item of an imported module is in the result one time, under
  the path of its module. This includes the private items
  (`geom::helper`, `geom::secret`) and the items that the entry module
  does not name.
- An imported struct keeps its impl blocks, and an imported trait
  keeps the impls that satisfy it. So an imported trait satisfies a
  local generic bound.
- The call in `geom::double_x` names `geom::helper`, not the `helper`
  of `main.fv`.
- `imports` lists what the entry module wrote in its `use` statements.
  A backend can use it to emit import statements.
- `file_table` holds each source file. The spans of each copied item
  name the file of its module.

`compile_to_ir_with_resolver` and `compile_to_ir_with_path_and_resolver`
then run `MonomorphisePass`.

`lower_to_ir` and `lower_to_ir_with_path` in `formalang::ir` lower one
AST alone. They do not link the imported modules, so a type that a
`use` imports is a `ResolvedType::External` in their result.
