# IrModule Structure

The root container for all IR definitions, plus the per-source-module
index that mirrors `mod foo { ... }` hierarchy.

## Architecture Overview

```text
IrModule (root)
|
+-- structs: Vec<IrStruct>
|   |
|   +-- name: String
|   +-- visibility: Visibility
|   +-- traits: Vec<IrTraitRef> ----> trait_id + optional generic-trait args
|   +-- fields: Vec<IrField>
|   |   |
|   |   +-- name: String
|   |   +-- ty: ResolvedType (may contain StructId/TraitId/EnumId refs)
|   |   +-- mutable: bool
|   |   +-- optional: bool
|   |   +-- default: Option<IrExpr>
|   |
|   +-- generic_params: Vec<IrGenericParam>
|       |
|       +-- name: String
|       +-- constraints: Vec<IrTraitRef>  (trait_id + Vec<ResolvedType> args)
|
+-- traits: Vec<IrTrait>
|   |
|   +-- name: String
|   +-- visibility: Visibility
|   +-- composed_traits: Vec<TraitId> -----> trait inheritance
|   +-- fields: Vec<IrField>
|   +-- methods: Vec<IrFunctionSig>   -----> required method signatures
|   +-- generic_params: Vec<IrGenericParam>
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
|   |
|   +-- target: ImplTarget ----------> Struct(StructId), Enum(EnumId) or Primitive(PrimitiveType)
|   +-- trait_ref: Option<IrTraitRef>
|   +-- functions: Vec<IrFunction>
|
+-- lets: Vec<IrLet>                // Module-level let bindings
|
+-- functions: Vec<IrFunction>      // Standalone function definitions
|
+-- imports: Vec<IrImport>          // What the `use` statements import
|
+-- modules: Vec<IrModuleNode>      // Source `mod` tree and imported modules
|
+-- file_table: Vec<PathBuf>        // Source files, indexed by FileId
```

## IrModule

```rust
pub struct IrModule {
    pub structs: Vec<IrStruct>,
    pub traits: Vec<IrTrait>,
    pub enums: Vec<IrEnum>,
    pub impls: Vec<IrImpl>,
    pub lets: Vec<IrLet>,               // Module-level let bindings
    pub functions: Vec<IrFunction>,     // Standalone function definitions
    pub imports: Vec<IrImport>,         // What the `use` statements import
    pub modules: Vec<IrModuleNode>,     // Source `mod foo { ... }` hierarchy
    pub file_table: Vec<PathBuf>,       // Source files, indexed by FileId
    // private name-to-id indexes, skipped by serde
}
```

The flat per-type vectors remain authoritative: every definition
lives in the appropriate slot regardless of source nesting. The
`modules` tree is an *index* on top of those flat vectors, opt-in
for backends that need to emit code into namespaces.

The vectors start with the prelude. `Array`, `Seq`, `Dictionary` and
`Range` are the first four structs, `Optional` is the first enum, the
prelude's extern impl blocks are the first impls, and `assert` is the
first function. See [Resolved Types](types.md#the-built-in-compound-types)
for the accessors that find them.

### Lookup Methods

```rust
impl IrModule {
    /// Look up a definition by ID. Returns None if out of bounds.
    pub fn get_struct(&self, id: StructId) -> Option<&IrStruct>;
    pub fn get_trait(&self, id: TraitId) -> Option<&IrTrait>;
    pub fn get_enum(&self, id: EnumId) -> Option<&IrEnum>;
    pub fn get_function(&self, id: FunctionId) -> Option<&IrFunction>;

    /// Look up an ID by name.
    pub fn struct_id(&self, name: &str) -> Option<StructId>;
    pub fn trait_id(&self, name: &str) -> Option<TraitId>;
    pub fn enum_id(&self, name: &str) -> Option<EnumId>;
    /// For an overloaded name, the id of the first overload.
    pub fn function_id(&self, name: &str) -> Option<FunctionId>;

    /// Look up a module-level let binding by name.
    pub fn get_let(&self, name: &str) -> Option<&IrLet>;
    pub fn has_let(&self, name: &str) -> bool;

    /// The source path of a FileId. None for FileId(0).
    pub fn file_path(&self, file: FileId) -> Option<&PathBuf>;
    /// Add a path to `file_table`, or find it there, and return its id.
    pub fn register_file(&mut self, path: PathBuf) -> FileId;

    /// Rebuild the internal name-to-ID indices after mutating the module.
    ///
    /// Call this after adding, removing or reordering definitions in
    /// `structs`, `traits`, `enums`, `functions` or `lets` so that the
    /// `*_id()` lookup methods stay consistent. Serde skips the
    /// indices: call it after you deserialise a module, too.
    pub fn rebuild_indices(&mut self);
}
```

## Imports

`imports` records what the `use` statements of the entry module
import. A backend can use it to emit import statements.

The public entry points link each imported module into the result (see
[Programs over several files](obtaining.md#programs-over-several-files)).
The imported definitions are then local definitions under a qualified
name, such as `geom::Point`, and each reference to them uses a local
id. So `imports` only records names. A backend does not need the IR of
another module.

### IrImport

```rust
pub struct IrImport {
    /// Logical module path (e.g., ["utils", "helpers"])
    pub module_path: Vec<String>,
    /// Items imported from this module
    pub items: Vec<IrImportItem>,
    /// Filesystem path to the source module file
    pub source_file: PathBuf,
}
```

### IrImportItem

```rust
pub struct IrImportItem {
    /// Name of the imported item, as the `use` wrote it
    pub name: String,
    /// Kind of the item
    pub kind: ImportedKind,
}
```

### ImportedKind

```rust
pub enum ImportedKind {
    Struct,
    Trait,
    Enum,
    Function,   // use other::compute
    ModuleLet,  // use other::CONST
}
```

### Using Imports in Code Generators

A generator that emits one output file for each source module can
read `imports` for the import statements, and the
[module tree](#irmodulenode-source-mod-hierarchy) for the items of
each imported module:

```rust
fn generate_typescript_imports(module: &IrModule) -> String {
    let mut output = String::new();
    for import in &module.imports {
        let path = import.module_path.join("/");
        let items: Vec<&str> = import.items.iter().map(|i| i.name.as_str()).collect();
        output.push_str(&format!(
            "import {{ {} }} from './{}';\n",
            items.join(", "),
            path
        ));
    }
    output
}
```

## IrModuleNode: source `mod` hierarchy

`IrModule.modules` mirrors the source `mod foo { ... }` tree. Each
node lists the IDs of struct/trait/enum/function definitions
declared *directly* in that module plus nested sub-modules. The
flat per-type vectors on `IrModule` remain authoritative: this
tree is an *index* on top of them for backends that need to
preserve source structure in their output (JS `export * from`,
Swift nested types, Kotlin packages).

```rust
pub struct IrModuleNode {
    /// Module name as written in source (the unqualified segment,
    /// e.g. `"shapes"` for `mod shapes { ... }`).
    pub name: String,

    /// IDs of structs declared directly in this module.
    pub structs: Vec<StructId>,

    /// IDs of traits declared directly in this module.
    pub traits: Vec<TraitId>,

    /// IDs of enums declared directly in this module.
    pub enums: Vec<EnumId>,

    /// IDs of functions declared directly in this module.
    pub functions: Vec<FunctionId>,

    /// Nested sub-modules.
    pub modules: Vec<IrModuleNode>,
}
```

Top-level (non-`mod`) definitions of the entry module are not
mirrored in the tree; backends iterate the flat vectors for those.

The tree also holds one node for each imported module, at the path
that the `use` wrote. For `use lib::util::one`, the node `lib` holds
the node `util`, and that node lists the id of `lib::util::one`.
