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
|   +-- target: ImplTarget ----------> ImplTarget::Struct(StructId) or ImplTarget::Enum(EnumId)
|   +-- functions: Vec<IrFunction>
|
+-- lets: Vec<IrLet>        // Module-level let bindings
|
+-- functions: Vec<IrFunction>  // Standalone function definitions
```

## IrModule

```rust
pub struct IrModule {
    pub structs: Vec<IrStruct>,
    pub traits: Vec<IrTrait>,
    pub enums: Vec<IrEnum>,
    pub impls: Vec<IrImpl>,
    pub lets: Vec<IrLet>,            // Module-level let bindings
    pub functions: Vec<IrFunction>,  // Standalone function definitions
    pub imports: Vec<IrImport>,      // External module imports
    pub modules: Vec<IrModuleNode>,  // Source `mod foo { ... }` hierarchy
}
```

The flat per-type vectors remain authoritative — every definition
lives in the appropriate slot regardless of source nesting. The
`modules` tree is an *index* on top of those flat vectors, opt-in
for backends that need to emit code into namespaces.

### Lookup Methods

```rust
impl IrModule {
    /// Look up a struct by ID. Returns None if out of bounds.
    pub fn get_struct(&self, id: StructId) -> Option<&IrStruct>;

    /// Look up a trait by ID. Returns None if out of bounds.
    pub fn get_trait(&self, id: TraitId) -> Option<&IrTrait>;

    /// Look up an enum by ID. Returns None if out of bounds.
    pub fn get_enum(&self, id: EnumId) -> Option<&IrEnum>;

    /// Look up a function by ID. Returns None if out of bounds.
    pub fn get_function(&self, id: FunctionId) -> Option<&IrFunction>;

    /// Look up a struct ID by name.
    pub fn struct_id(&self, name: &str) -> Option<StructId>;

    /// Look up a trait ID by name.
    pub fn trait_id(&self, name: &str) -> Option<TraitId>;

    /// Look up an enum ID by name.
    pub fn enum_id(&self, name: &str) -> Option<EnumId>;

    /// Look up a function ID by name.
    pub fn function_id(&self, name: &str) -> Option<FunctionId>;

    /// Rebuild the internal name-to-ID indices after mutating the module.
    ///
    /// Call this after adding or removing definitions from `structs`, `traits`,
    /// `enums`, or `functions` so that the `*_id()` lookup methods stay
    /// consistent.
    pub fn rebuild_indices(&mut self);
}
```

## External Imports

When a module uses types from other modules via `use` statements, those
types are represented as `External` variants in `ResolvedType`. The
`imports` field tracks which external types are used.

### IrImport

```rust
pub struct IrImport {
    /// Logical module path (e.g., ["utils", "helpers"])
    pub module_path: Vec<String>,
    /// Items imported from this module
    pub items: Vec<IrImportItem>,
}
```

### IrImportItem

```rust
pub struct IrImportItem {
    /// Name of the imported type
    pub name: String,
    /// Kind of type (struct, trait, or enum)
    pub kind: ImportedKind,
}
```

### ImportedKind

```rust
pub enum ImportedKind {
    Struct,
    Trait,
    Enum,
}
```

### Using Imports in Code Generators

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

## IrModuleNode — source `mod` hierarchy

`IrModule.modules` mirrors the source `mod foo { ... }` tree. Each
node lists the IDs of struct/trait/enum/function definitions
declared *directly* in that module plus nested sub-modules. The
flat per-type vectors on `IrModule` remain authoritative — this
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

Top-level (non-`mod`) definitions are not mirrored in the tree —
backends iterate the flat vectors for those.
