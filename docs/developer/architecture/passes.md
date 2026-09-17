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
carrying `NumberValue::Float(f64)` falls back to `f64` IEEE 754;
mixed-precision results are stored as `Float`, so
`Integer(2^60) + Float(0.0)` round-trips as `Float`, losing exactness
beyond `2^53`. Backends that need exact integer results should ensure
their inputs are integer-only or skip folding for that expression.

## `ResolveReferencesPass`

Rewrites name-keyed references (`IrExpr::Reference.path`, `LetRef.name`,
`IrMatchArm.variant`) into typed IDs (`ReferenceTarget`, `BindingId`,
`VariantIdx`). Opt-in; **not** included in `Pipeline::default()`. Use it
when the backend emits integer-indexed code (wasm, JVM, native).

## `ClosureConversionPass`

Lifts every closure body to a top-level function and collects the
values it captures into a synthetic env struct, so a backend only ever
sees named functions. `IrExpr::Closure` becomes
`IrExpr::ClosureRef { funcref, env_struct }`. Included in
`Pipeline::for_codegen`.

The pass is idempotent and preserves source spans: a captured
variable's synthesised `__env.x` access carries the span of the
reference it replaces, so a debugger stepping over it lands on the
name the user wrote.

## `DefunctionalisePass`

Turns every closure value into an enum tag. Run it after
`ClosureConversionPass`, whose output it consumes, and before
`DeadCodeEliminationPass`.

Closure conversion answers "where does the body live?": it lifts each
body to a top-level function and collects the captures into an env
struct. It leaves open what the closure *value* is. Two answers are
possible:

```text
address form:     data bytes → address → indirect jump
defunctionalised: data bytes → tag → match → direct call
```

This pass takes the second. It builds one enum per distinct closure
type, with one variant per lifted function, each carrying the env
struct closure conversion already made:

```text
enum __Fn0 {
    __closure0(env: __ClosureEnv0),
    __closure1(env: __ClosureEnv1),
}

fn __call_Fn0(f: __Fn0, x: I32) -> I32 {
    match f {
        .__closure0(env): __closure0(__env: env, x: x),
        .__closure1(env): __closure1(__env: env, x: x),
    }
}
```

Every `ClosureRef` becomes an `EnumInst`, every `CallClosure` becomes a
`FunctionCall` to the dispatch function, and every occurrence of the
closure type becomes the enum type. After the pass, **the module
contains no indirect call**.

**Why a tag and not an address.** With the address form a code address
lives inside a data value, so any defect that corrupts those bytes
becomes a jump to anywhere. With a tag, the same corruption at worst
selects the wrong arm: a wrong answer, not a takeover. A backend also
gains, because every call target is known and each arm can be inlined.

**Closed world.** The pass must see every closure of a shape before it
can build that shape's enum. A just-in-time backend compiles the whole
program at once, so this holds there. It would not hold under separate
compilation, which is why the pass is opt-in rather than part of
`Pipeline::for_codegen`.
