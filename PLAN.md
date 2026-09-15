# Plan — Language Changes for a Native Backend

**Status**: Agreed and fully specified. No open questions. Not started.
**Date**: 2026-09-15

This is **phase 1** of a two-phase plan. Phase 2 builds the backend
itself and lives in `formajit/PLAN.md`, in its own repository. Read that
one for the Cranelift lowering, the memory model and the host ABI.

Phase 1 changes the language. Nothing here depends on the backend
existing, and every change stands on its own.

## Ground rules

These override every other consideration in this document.

- **Ignore `formawasm`.** Do not check it, do not keep it compiling, do
  not shape a decision around it. If a change breaks it, that is
  acceptable and it is not this plan's problem.
- **Ignore backwards compatibility.** Serialised `IrModule` JSON, the
  public API surface, and the `File` `format_version` may all break.
  This extends the pre-1.0 rule in `AGENTS.md` to cover the two
  exceptions that rule still listed.
- **Ignore version churn.** Take the newest stable release of every
  dependency. Raise the minimum Rust version when a dependency demands
  it. Do not pin an old release to keep a number stable.

## Goal

Make the language fit a machine-code backend, and make it fit **large
data**. A loop over one million elements is the normal case, not the
exception.

Two things follow, and they drive everything below:

1. **Cut what a machine-code target cannot carry cheaply.** Trait values
   need vtables. `Regex` needs an engine. Float dictionary keys are a
   defect in any language.
2. **Stop every loop from allocating.** `for` becomes a lazy, linear
   `Seq<T>`, so a pipeline that ends in a reduction allocates nothing
   and holds memory that does not grow with the input.

The backend consumes the result. See `formajit/PLAN.md`.

## Decisions

These are settled. The reasoning is in the sections that follow.

| Topic | Decision |
| --- | --- |
| Trait as a value type | Cut. Users write an enum and a `match`. |
| `Regex` and `Path` primitives | Cut, with their literal syntax. |
| Dictionary keys | Cut `F32` and `F64`. Keep struct and enum keys. |
| Closures in `pub` signatures | Keep. The JIT compiles the whole program. |
| Recursion | Keep. The backend bounds the depth with a counter. |
| Closure representation | Defunctionalise. No indirect calls in the output. |
| Loops | `for` produces a lazy `Seq<T>`, consumed exactly once. See Sequences. |
| A dropped loop | Always an error, `E134`. Dropping a `Seq` means nothing ran. |
| `mut` | Mutable Value Semantics. By pointer; the caller sees the change. See R2. |
| `sink` | No effect on code generation under an arena. See R3. |
| Repository split | `formalang` keeps the frontend. `formajit` is a **new repository**, like `formawasm`. |

## Why not start over

The frontend is 40,774 lines of source and 44,072 lines of tests. The
cuts above delete about 900 lines, which is 2% of the source. The
lexer, the parser, the semantic analyzer, inference, monomorphisation,
closure conversion and the diagnostics do not change when the backend
changes.

The IR shape does need a second, lower form for a machine-code target,
but that belongs in the new repository. See the MIR section in
`formajit/PLAN.md`.

## Sequences

This is the part that decides whether one million elements is
comfortable or merely survivable.

### The problem

`IrExpr::For` carries `ty: Array(body_type)`. A loop is an expression
that returns its collected results, so **every loop allocates**. I
confirmed that `for i in 0..n { i }` compiles and has type `[I32]`.

```formalang
let a: [I32] = for x in xs { x * 2 }   // allocates
let b: [I32] = for y in a  { y + 1 }   // allocates again
let n: I32   = b.len()                 // reads one number
```

Over one million `I32` values that is 8 MB allocated and three passes
over memory, to produce one integer. At ten million it is 80 MB. The
arena cap (R9, in `formajit/PLAN.md`) turns that from a slowdown into a
hard failure.

### The answer: a lazy, linear `Seq<T>`

`for` produces a **lazy sequence**, and that sequence is **linear**: it
must be consumed **exactly once**.

```formalang
// ends in a reduction: zero allocation, one pass
let total: I32 = for x in xs { x * 2 }.fold(0, (a, b) -> a + b)

// ends in an array: one allocation, and the reader can see it
let doubled: [I32] = for x in xs { x * 2 }.collect()
```

Linearity is two separate rules that happen to travel together. They are
worth keeping apart, because they earn their place for different
reasons.

**At most once — this is what buys fusion.** A `Seq` has at most one
reader, so there is never a second consumer to materialise for. The
compiler never decides whether fusion is safe, so it can never decide
wrong. There is no performance cliff to fall off. Reuse is a compile
error, not a silent second run of the pipeline.

**At least once — this is what stops silent dead code.** See below.

The frontend already does the first half for `sink`, through
`UseAfterSink`. Extend it rather than build it again.

### Dropping a sequence is an error

A dropped sequence is not like a dropped number. Drop an `I32` and you
computed a value and ignored it: wasteful, harmless. Drop a `Seq` and
**nothing ran at all**. The loop in the source never executed.

That is true whether the body is pure or not, and it is a mistake in
both cases.

```formalang
for x in xs { x * 2 }               // computes nothing; the author meant to use it
for x in xs { log(message: "x") }   // the logs never appear
```

The first is dead code that looks like work. The second is a silent bug.
Neither is something anybody writes on purpose, so reject both with one
rule:

> A sequence must be consumed. Dropping one is an error.

The author picks the fix, and the fix states the intent: `.collect()`
for an array, `.fold(...)` or `.count()` for a reduction, `.run()` to
execute for effects alone, or delete the line.

This also means **no effect analysis is needed for this rule**. One
unconditional check is simpler than a call-graph walk, and it catches
the pure case that an effect-based rule would wave through.

**Why not run it automatically.** The compiler could notice a dropped
sequence with an effectful body and emit the loop eagerly, leaving the
source untouched. Reject that. It makes "does this loop run?" depend on
the body's **transitive** call graph, which the reader cannot see:

```formalang
for x in xs { helper(value: x) }
```

Nobody can tell from that line. Now let somebody add a `log` inside
`helper`, in another file, next week:

| | What happens to that loop |
| --- | --- |
| refuse to compile | it was already an error; the author already wrote `.run()` |
| run automatically | it silently starts running, with no diagnostic anywhere |

Removing the last `log` from `helper` is worse: under an automatic run,
a loop that was running quietly stops, with nothing changed at the call
site. Refusing to compile makes the question moot, because the author
said what they meant the first time.

There is no third option: `src/reporting/` builds only
`ReportKind::Error`, with no warning infrastructure to lean on.

### The diagnostic

Next free code is `E134`; the project is at `E133`. Put the renderer
next to `public_closure_field` in
`src/reporting/errors_advanced/misc.rs`:

```text
[E134] Error: This sequence is never consumed
    ╭─[app.fv:7:3]
    │
  7 │   for x in xs { log(message: "x") }
    │   ────────────────┬───────────────
    │                   ╰─── this loop never runs
    │
    │ Help: a sequence does nothing until something consumes it.
    │       Add '.collect()' for an array, '.fold(...)' or '.count()'
    │       for a single value, or '.run()' to execute it for effects.
────╯
```

```rust
pub(in crate::reporting) fn seq_not_consumed(
    filename: &str,
    span: Span,
) -> ReportBuilder<'_> {
    report(filename, span, "E134")
        .with_message("This sequence is never consumed")
        .with_label(label(filename, span).with_message("this loop never runs"))
        .with_help(
            "a sequence does nothing until something consumes it; add \
             '.collect()' for an array, '.fold(...)' or '.count()' for a \
             single value, or '.run()' to execute it for effects",
        )
}
```

**A later refinement, not on the critical path.** An effect analysis
would let the help line name what the author loses:

```text
Help: the body reaches 'helper' → 'format_row' → 'log', which is an
      extern function, so those calls never happen. Add '.run()'.
```

That needs a call-graph walk returning `Option<Vec<FunctionId>>` — the
shortest path to an `extern fn` — rather than a boolean. It is worth
doing eventually, because constant folding and dead-code elimination
both want the same answer, and purity is a headline claim of the
language. It is not needed to ship the rule.

### Two levels, on purpose

`for` is **in the language**. The combinators are **in the library**.
They are not competitors; a pipeline normally uses both.

```formalang
let total: I32 =
  for row in rows() { row.score }
    .filter((s) -> s > 0)
    .fold(0, (a, b) -> a + b)
```

`for` starts a pipeline from an array, a range, a dictionary or another
sequence. The combinators continue and end it.

| Kind | Members |
| --- | --- |
| Adapter, `Seq` to `Seq` | `map`, `filter`, `take`, `skip`, `zip` |
| Terminal, ends the pipeline | `collect`, `count`, `fold`, `first`, `any`, `all`, `run` |

`map` overlaps with `for` on purpose. `for` reads better as a pipeline
source; `map` reads better in the middle of a chain.

### The combinators are intrinsics, not runtime functions

**Write this down before anyone adds them to `src/prelude.fv` as
`extern impl`.**

Every combinator takes a closure. After defunctionalisation a closure is
an enum tag that only the compiler understands, so **the host cannot
call one**. `filter` therefore cannot work the way `String::len` works.

Declare the combinators in the prelude, so users see one standard
library. Lower them in the backend as compiler-known intrinsics, so the
closure never crosses a boundary. Library to the user, intrinsic to the
compiler.

### Where a `Seq` may appear

| Position | Allowed |
| --- | --- |
| a local `let` | yes |
| a function parameter | yes, `sink` only |
| a `for` source | yes |
| an `extern fn` return type | yes — this is the host cursor |
| a struct field or enum payload | **no** |
| a `pub fn` return type | **no** |
| a `fn` return type | **no in v1**; see below |

The struct and `pub fn` bans are the same rule as the existing one on
closure-typed fields, for the same reason: a lazy pipeline has no
representation that survives a boundary.

**The `fn` return ban is a v1 restriction, not a design limit.** A
function that returns a `Seq` is a pipeline fragment, and it composes
correctly if the backend always inlines it at the call site. Inlining a
a linear pipeline is mechanical, and the whole program is available. Lift
the restriction when mandatory inlining exists, and require such a
function to be non-recursive.

### What this gives you

| | Before | After |
| --- | --- | --- |
| 3-stage pipeline over 1M `I32`, ending in a sum | 8 MB, 3 passes | 0 bytes, 1 pass |
| memory against input length | O(n) | **O(1)** |
| loop run for its calls alone | allocates | compile error until `.run()` |
| loop whose result is dropped | allocates silently | compile error |
| a chain that is not fusible | silently allocates | cannot be written |

## Phase 1 — `formalang`

Eight changes. Each one is its own pull request. Steps 1.1 to 1.5 are
small; 1.6 is the largest piece of work in the phase.

### 1.1 Fix E934 first

Inferred enum construction fails in array-element position and in
argument position:

```formalang
pub let shapes: [Shape] = [.square(s: Square(side: 1))]
// [E934] Internal compiler error: inferred-enum `.square`
//        has no resolvable return-type enum `Array`
```

There is no qualified `Shape::square(s: ...)` form either, so the only
way through today is a separate annotated `let` for each value. Enums
become the only replacement for trait values, so this must land first.

### 1.2 Cut trait values

A trait may no longer stand in as a value type. These three forms stop
compiling:

```formalang
let s: Shape = if flag { Square(...) } else { Rect(...) }   // unified branch
pub let shapes: [Shape] = [Square(...), Rect(...)]          // heterogeneous array
pub struct Scene { root: Shape, tag: String }               // trait-typed field
```

Users write an enum instead:

```formalang
pub enum Shape { square(s: Square), rect(r: Rect) }

fn area_of(shape: Shape) -> I32 {
  match shape { .square(s): s.area(), .rect(r): r.area() }
}
```

Traits stay as constraints, so `src/semantic/trait_check/` stays.
`DispatchKind::Virtual` leaves the IR. It appears in 11 files, mostly as
one match arm each.

A trait-typed return already fails today with `[E110]`, so that part is
a repair, not a removal.

### 1.3 Cut `Regex` and `Path`

Remove both `PrimitiveType` variants and their literal syntax. The
prelude declares no methods on either type, so nothing can act on them
today. `r.len()` fails with `Undefined reference`. Both types are inert
literals.

### 1.4 Cut float dictionary keys

Reject `F32` and `F64` in a dictionary key position. Keep `String`,
`I32`, `I64`, `Boolean`, struct and enum keys. This adds a validation
rule of about 40 lines.

A float key is a defect in any language, because `NaN != NaN` and
`0.0 == -0.0`. Rust refuses it too, since `f64` is not `Eq`.

### 1.5 Add `visibility` to `IrFunction`

`IrStruct`, `IrEnum`, `IrTrait` and `IrLet` all carry a `visibility`
field. `IrFunction` does not, even though the AST's `FunctionDef` does.
Lowering drops it.

So `pub fn` parses and then vanishes. Nothing downstream can tell a
public function from a private one. The entry-point design in
`formajit/PLAN.md` § 2.9 needs this, so add the field and carry it
through lowering.

### 1.6 Add `Seq<T>`

The design is in [Sequences](#sequences). The work in the frontend:

**The type.** Declare `Seq<T>` in `src/prelude.fv` as an opaque generic
struct, next to `Array<T>`. Add `ResolvedType` support so
monomorphisation specialises it like any other generic.

**The `for` type.** Change `IrExpr::For` so `ty` is `Seq(body_type)`
instead of `Array(body_type)`. This is the change that ripples: every
test and example that assigns a `for` to `[T]` grows a `.collect()`.

**Linearity.** A `Seq`-typed value is implicitly `sink` at every use.
Two new errors:

| Error | Raised when | Code |
| --- | --- | --- |
| `SeqUsedTwice` | a `Seq` binding is read after it was consumed | `E135` |
| `SeqNotConsumed` | a `Seq` value reaches the end of its scope unread | `E134` |

Extend the existing `UseAfterSink` analysis for both. Neither needs an
effect analysis; see [Dropping a sequence is an
error](#dropping-a-sequence-is-an-error). The full diagnostic for
`E134` is in [The diagnostic](#the-diagnostic).

**Placement rules.** Reject a `Seq` in a struct field, an enum payload,
a `pub fn` return type, and a `fn` return type. Accept it in a local
`let`, in a `sink` parameter, as a `for` source, and as an `extern fn`
return type. The last one is the host cursor.

**The combinators.** Declare `map`, `filter`, `take`, `skip`, `zip`,
`collect`, `count`, `fold`, `first`, `any`, `all` and `run` on `Seq<T>`
in the prelude. Mark them as intrinsics. They are **not** `extern impl`
and they never reach a host symbol; see the warning in
[Sequences](#sequences).

**Fix nested `for` inference.** `for x in xs { for y in xs { x + y } }`
fails today with `[E110]`. Nested pipelines are the normal case for this
work, so repair it here.

### 1.7 Add `DefunctionalisePass`

Runs after `ClosureConversionPass`. Replaces each closure value with an
enum tag plus the captured fields.

Closure conversion already lifts each body to a top-level function and
collects the captures into a struct. This pass answers the remaining
question: what does a closure **value** look like?

```formalang
fn make_adder(sink n: I32) -> (I32) -> I32  { (x: I32) -> x + n }
fn make_scaler(sink k: I32) -> (I32) -> I32 { (x: I32) -> x * k }
```

One enum covers every closure of one shape:

```rust
enum Fn_I32_I32 { C0 { n: i32 }, C1 { k: i32 } }

fn call_Fn_I32_I32(f: &Fn_I32_I32, x: i32) -> i32 {
    match f {
        Fn_I32_I32::C0 { n } => x + n,
        Fn_I32_I32::C1 { k } => x * k,
    }
}
```

Each `IrExpr::ClosureRef` becomes an `IrExpr::EnumInst`. Each
`IrExpr::CallClosure` becomes an `IrExpr::FunctionCall` to the generated
top-level `call` function. After the pass, the module contains no
closure type and no indirect call.

The pass needs the whole program, because it must see every closure of a
shape before it can build the enum. That holds for a JIT, which sees
the whole program at once. Make the pass opt-in anyway, so a future
backend with native function references can skip it.

**This is the security-relevant choice.** With a function pointer, a
code address sits inside a data value in the arena, and corrupt bytes
become a jump to anywhere. With an enum tag, corrupt bytes at worst pick
the wrong `match` arm, which is a wrong answer, not a takeover. The
generated code contains zero indirect calls.

### 1.8 Documentation

`docs/user/control-flow.md` and `README.md` both teach `for` as an
expression that yields an array. Rewrite both against
[Sequences](#sequences), and add a page on large data that shows the
host cursor and the emit hook end to end.

Also fix the `mut` gap named in R2: state that the caller sees the
change.

## Resolved design

These were open in an earlier draft and are now settled, most of them by
something the code already states. Nothing here is left to the
implementer's judgement.

The numbering is shared with `formajit/PLAN.md`, so a number means the
same thing in both documents. R1, R5, R7, R8 and R9 are backend
decisions and live in `formajit/PLAN.md`.

### R2 `mut` and assignment

The language uses **Mutable Value Semantics**, which
`docs/user/functions.md` states directly. That fixes the answer:

| Convention | Meaning | How it is passed |
| --- | --- | --- |
| default | the callee reads only | by pointer, or in a register when it fits |
| `mut` | exclusive mutable; **the caller sees the change** | by pointer, always |
| `sink` | ownership transfer | by pointer, or in a register when it fits |

The frontend guarantees exclusivity, so no two live pointers ever alias.
The backend therefore passes a raw pointer and adds no checks. This is
the main reason Mutable Value Semantics is easy to compile.

**Where a local lives.** Run an address-taken analysis over each
function:

- A scalar that is never passed as a `mut` argument lives in a Cranelift
  variable, even when it is assigned. `def_var` handles reassignment.
  This is the common case and it costs nothing.
- A scalar that **is** passed as a `mut` argument needs an address, so
  the caller puts it in a Cranelift stack slot and passes the pointer.
- An aggregate always has an address. It lives in a stack slot when it
  does not escape the function, and in the arena when it does.

**Assignment.** `IrBlockStatement::Assign` has two shapes. `n = expr`
on a scalar in a variable becomes `def_var`. `p.x = expr` becomes a
`store` at the field offset from the layout table.

**A documentation gap to fix.** `docs/user/functions.md` says "Callee
may mutate `x`" without saying the caller sees the change. Under
Mutable Value Semantics it does. State that explicitly when 1.5 lands.

### R3 `sink`

`sink` has **no effect on code generation**. Under a bump arena nothing
is ever freed, so a transfer of ownership costs no instruction. It stays
a frontend rule that stops the caller reading a moved binding.

Do not model it in the MIR. Do not revisit this unless the arena is
replaced.

### R4 `Optional<T>`

Not a special case. `src/prelude.fv` declares it as an ordinary generic
enum:

```formalang
pub enum Optional<T> { some(value: T), none }
```

So it takes the standard enum layout from the table: a 4-byte tag, then
the payload. No niche packing. `nil` is `Optional::none`. Monomorphi-
sation specialises it like any other generic.

### R6 Loops and `Seq<T>`

Settled in [Sequences](#sequences). In short:

- `for` produces `Seq<T>`, not `[T]`. `IrExpr::For` changes its `ty`
  from `Array(body_type)` to `Seq(body_type)`.
- A `Seq` is linear. It must be consumed exactly once. "At most once"
  is what makes fusion the semantics rather than an optimisation; "at
  least once" is what stops a loop that never runs from looking like
  work.
- `collect()` is the only step in a pipeline that allocates for the
  data, and the reader can see it on the line.

The "elide the result when nobody reads it" rule from the earlier draft
is gone. It was a patch for a problem the type now prevents.

## Order of work

```text
1.1   fix E934                              ← must land first
1.2   cut trait values
1.3   cut Regex and Path
1.4   cut float dictionary keys
1.5   visibility on IrFunction
1.6   Seq<T>: type, linearity, combinators  ← R6, the big one
1.7   DefunctionalisePass
1.8   documentation                         ← R2 gap, Sequences
```

Each step is its own pull request. Steps 1.1 to 1.5 are small; 1.6 is
the largest piece of work in the phase.

**Why `layout` is not here.** Computing sizes and offsets needs to know
the target, because a 32-bit pointer and a 64-bit pointer put the same
field at different offsets. Anything that knows the target belongs with
the target, so `layout` lives in `formajit`. `DefunctionalisePass` stays
here: it is a pure IR-to-IR transform that needs no target, and it
belongs next to its sibling `ClosureConversionPass` in
`src/ir/closure_conv/`.

## What phase 2 needs from this

`formajit` cannot start its own milestone until these exist:

| From | What the backend consumes |
| --- | --- |
| 1.5 | `visibility` on `IrFunction`, so `pub fn` becomes an export |
| 1.6 | `Seq<T>` in the IR, so a pipeline can lower to one loop |
| 1.7 | an `IrModule` with no closure type and no indirect call |

Publish a new minor version of `formalang` once phase 1 completes. The
current `0.0.5-beta` line describes a language that will no longer
exist.
