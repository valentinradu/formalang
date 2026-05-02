# ID Types

The IR uses typed IDs for referencing definitions. IDs are simple
newtypes wrapping `u32`, making them copyable and cheap to pass around.

```rust
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct StructId(pub u32);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct TraitId(pub u32);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct EnumId(pub u32);
```

IDs index into the corresponding `Vec` in `IrModule`:

```rust
// Use helper method (returns Option)
if let Some(struct_def) = module.get_struct(id) {
    // use struct_def
}

// Lookup by name
if let Some(id) = module.struct_id("User") {
    if let Some(struct_def) = module.get_struct(id) {
        // use struct_def
    }
}

// Direct indexing (when ID is known valid)
let struct_def = &module.structs[id.0 as usize];
```

## ID Type Safety

IDs are type-safe: you cannot accidentally use a `StructId` where a
`TraitId` is expected. This prevents a common class of bugs:

```rust
let struct_id = StructId(0);
let trait_id = TraitId(0);

// Compile error: types don't match
// module.get_struct(trait_id);
```

## Other typed IDs

Beyond the four definition-level IDs above, several expression-level
typed IDs flow through the IR after `ResolveReferencesPass` rewrites
name-keyed references:

- `BindingId` — function-local `let` bindings, parameters, loop variables
- `FieldIdx` — index into the matching struct/enum variant's `fields`
- `VariantIdx` — index into the matching enum's `variants`
- `MethodIdx` — index into the matching impl's or trait's `methods`
- `LetId` — module-level `let` bindings
- `ImplId` — impl blocks
- `FunctionId` — standalone or impl-method functions

These appear on [`IrExpr`](expressions.md), [`IrMatchArm`](blocks.md),
and [`IrBlockStatement`](blocks.md).
