# Built-in Passes

Exported from `formalang::ir`. Compose them through a [`Pipeline`](plugins.md);
none are wired in by default unless noted.

## `MonomorphisePass`

Specialises every `Generic { base, args }` instantiation (struct, enum,
trait), specialises generic functions per call-site arg-tuple, and
devirtualises `Virtual` dispatch on concrete receivers. The frontend has
no dynamic dispatch, so this pass is the bridge from generic source to
fully-resolved IR.

## `DeadCodeEliminationPass`

Removes unreachable definitions.

## `ConstantFoldingPass`

Evaluates constant expressions at compile time. Numeric folding takes
the high-precision path when both operands are
`NumberValue::Integer(i128)` (checked `i128` arithmetic; overflow leaves
the `BinaryOp` un-folded so codegen decides the emit). Any operand
carrying `NumberValue::Float(f64)` falls back to `f64` IEEE 754 —
mixed-precision results are stored as `Float`, so
`Integer(2^60) + Float(0.0)` round-trips as `Float`, losing exactness
beyond `2^53`. Backends that need exact integer results should ensure
their inputs are integer-only or skip folding for that expression.

## `ResolveReferencesPass`

Rewrites name-keyed references (`IrExpr::Reference.path`, `LetRef.name`,
`IrMatchArm.variant`) into typed IDs (`ReferenceTarget`, `BindingId`,
`VariantIdx`). Opt-in; **not** included in `Pipeline::default()`. Use it
when the backend emits integer-indexed code (wasm, JVM, native).
