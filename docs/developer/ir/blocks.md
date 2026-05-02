# Match Arms & Block Statements

The two non-expression building blocks referenced from
[`IrExpr::Match`](expressions.md#irexpr) and
[`IrExpr::Block`](expressions.md#irexpr).

## IrMatchArm

```rust
pub struct IrMatchArm {
    /// Variant name being matched (empty for wildcard); preserved
    /// alongside `variant_idx` for diagnostics.
    pub variant: String,

    /// Position of the matched variant in the scrutinee enum's `variants`
    /// vector. Lowering emits `VariantIdx(0)` and `ResolveReferencesPass`
    /// overwrites it.
    pub variant_idx: VariantIdx,

    /// Whether this is a wildcard (`_`).
    pub is_wildcard: bool,

    /// Bindings for associated data: `(name, binding_id, type)`. Each
    /// `binding_id` is a fresh per-function id introduced by the arm —
    /// backends key on it to reach the slot the arm writes the payload
    /// into. Lowering emits `BindingId(0)` and `ResolveReferencesPass`
    /// overwrites it.
    pub bindings: Vec<(String, BindingId, ResolvedType)>,

    /// Body expression
    pub body: IrExpr,
}
```

## IrBlockStatement

Statements inside an `IrExpr::Block`.

```rust
pub enum IrBlockStatement {
    /// Let binding: `let x = expr` or `let mut x = expr`.
    Let {
        /// Per-function-unique id paired with `LetRef::binding_id` on
        /// references inside the block. Lowering emits `BindingId(0)`
        /// and `ResolveReferencesPass` overwrites it.
        binding_id: BindingId,
        name: String,
        mutable: bool,
        ty: Option<ResolvedType>,
        value: IrExpr,
    },
    /// Assignment: `x = expr`.
    Assign {
        /// Variable or field path being written.
        target: IrExpr,
        value: IrExpr,
    },
    /// Expression evaluated for its side effects.
    Expr(IrExpr),
}
```
