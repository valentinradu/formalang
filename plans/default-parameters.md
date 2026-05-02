# Default Parameter Values

**Status**: open / upstream-blocked
**Last updated**: 2026-05-02

This document is for backend authors. It describes why
`IrFunctionParam.default: Option<IrExpr>` is dead-code in the IR
today and what would need to change upstream for backends to
consume it.

The motivating consumer is the WebAssembly backend at
`~/projects/formawasm` Phase 5 #3, which planned to lower default
parameter values and found there's no IR shape for a call site
that omits a parameter.

---

## What exists today

The IR field is in place:

- [`IrFunctionParam.default: Option<IrExpr>`](../../src/ir/types.rs)
  — populated by IR lowering from the AST, threaded through
  monomorphisation / closure conversion / DCE / fold passes
  unchanged. Every consumer that walks parameter slots already
  preserves the default.

What's missing is **call-site default substitution**. Today the
semantic validator at
[`src/semantic/validation/invocation.rs`](../../src/semantic/validation/invocation.rs:426)
does an exact-arity check:

```rust
let non_self_count = params.iter().filter(|p| p.name.name != "self").count();
if args.len() != non_self_count {
    return false;
}
```

Calls that omit a parameter — even one with a declared default —
are rejected as no-overload-matches. So the resulting IR's
`FunctionCall.args` always contains exactly `non_self_count`
entries, and `param.default` is never consulted at lowering or
codegen time.

## What backends want

When a call site omits a parameter that has a default, the IR
should already contain the default expression in the args list.
Concretely, given:

```formalang
fn greet(name: String, greeting: String = "hello") -> String { ... }

greet("world")
```

The backend wants `IrExpr::FunctionCall { args: [(None, "world"),
(None, /* default expr */)], .. }` — same shape as
`greet("world", "hello")`. Backends emit one code path; the IR
hides the source-level distinction.

## Two design directions

### Direction A — fill defaults at IR-lowering time

When `IrLowerer` lowers a `FunctionCall` AST node, walk the
callee's `params` and, for each parameter not present in the call's
arg list, substitute the parameter's `default` expression. After
this pass, every `FunctionCall.args` is exactly the callee's
arity; every backend sees the same shape.

Two sub-choices:

- **Inline the default expression at every call site.** Each call
  carries its own copy of the default IR. Simple, debuggable, but
  bloats the module if the default expression is large. Cache
  invalidation is trivial (every call has its own copy).
- **Synthesize a wrapper function per parameter-default permutation.**
  `greet(name)` lowers to a call to a synthesized
  `__greet_with_default_greeting(name)` whose body forwards to
  `greet(name, default_greeting)`. The default expression lives in
  one place; call sites stay tight. Adds N synthesized functions
  per defaults-bearing function, which is fine until a function
  has many defaults (combinatorial blowup).

Pro of inline: simpler. Pro of wrapper: smaller modules. The
language semantic level should pick one.

### Direction B — flow `Option<IrExpr>` through to the backend

Keep the call-site arg list as written; let backends fill defaults
themselves. Each backend looks up the callee's `params`, walks the
call's args, and for each missing param emits the default
expression's IR.

Con: every backend has to reimplement the substitution, including
re-resolving any references inside the default expression to the
caller's binding scope.
Con: type-checking the default at the call site needs the call
site's generic-arg substitution; if the default contains `T`-typed
expressions, the IR-lowering has already specialized once,
and re-doing it per-backend is painful.

This direction is mostly worse than Direction A. Listed only for
completeness.

---

## Plan

**Chosen direction:** A, **inline at every call site**. Direction B
rejected (the design note already calls it "mostly worse"). The
wrapper-function sub-option of A rejected: defaults are usually
small, the per-call IR copy is cheap, and inline keeps the
debugger trace pointing at the actual default expression rather
than a synthesized wrapper.

### Current state of the relevant code

- `IrFunctionParam.default: Option<IrExpr>` is populated by
  IR lowering and threaded through every pass unchanged. Field is
  ready; nothing reads it post-lowering today.
- Semantic validator at
  [`src/semantic/validation/invocation.rs:426`](../../src/semantic/validation/invocation.rs)
  rejects calls with omitted defaults via exact-arity check.
- `IrExpr::FunctionCall` is constructed at
  [`src/ir/lower/expr/literals_and_containers.rs:182`](../../src/ir/lower/expr/literals_and_containers.rs).
- Function overloading exists; resolver lives in the same
  validation file (`get_function_overloads` →
  most-specific-by-type matching).
- Pipeline: lowering → monomorphise → closure conv → DCE. Default
  substitution must happen during initial lowering so subsequent
  passes see the substituted args.

### Steps

1. **Broaden the validator's arity check.**
   At
   [`src/semantic/validation/invocation.rs:426`](../../src/semantic/validation/invocation.rs),
   replace the exact-arity rejection. Compute
   `required = params.iter().filter(|p| p.name.name != "self" && p.default.is_none()).count()`
   and accept iff `args.len() ∈ [required, non_self_count]`. Defaults
   must be positional from the right (i.e. no parameter with a
   default may precede a parameter without one, except `self`) —
   enforce in pass1 so the range check is meaningful.

2. **Type-check defaulted positions.**
   For each missing arg position, look up the param's `default`
   expression and verify the type matches the (post-generic-substitution)
   expected type. The expression has already been type-checked at
   definition time; the per-call check just propagates the call's
   generic substitution.

3. **Most-specific overload resolution under defaults.**
   When multiple overloads pass the broadened arity range, prefer
   the one with the smallest `non_self_count - args.len()` gap (the
   overload requiring the fewest defaults to fire). Tie at zero gap
   uses the existing type-based resolution path. Tie at the same
   non-zero gap → existing "ambiguous overload" error.

4. **Inline default substitution at IR-lowering.**
   In the `FunctionCall` branch at
   [`literals_and_containers.rs:182`](../../src/ir/lower/expr/literals_and_containers.rs),
   before constructing `IrExpr::FunctionCall`:
   - For each missing arg position `i ∈ [args.len(), non_self_count)`,
     clone the resolved callee's `params[i].default` IR.
   - **If any cloned default references an earlier param** (an
     `IrExpr::Ref` to one of params 0..i), wrap the entire
     `IrExpr::FunctionCall` in `IrExpr::Let` bindings — one per
     preceding non-defaulted arg — that bind those param names to
     fresh temporaries holding the call-site arg values. The
     defaults' `IrExpr::Ref(x)` then resolve to the temporary.
     Equivalent to source-level desugaring:
     `f(arg_x)` with `f(x, y = x + 1)` becomes
     `{ let __t_x = arg_x; f(__t_x, __t_x + 1) }`.
     Avoids duplicating side-effects when `arg_x` is non-trivial.
   - **If no default references an earlier param,** skip the let
     wrapper — direct substitution is sound.

5. **Pipeline order audit.**
   Confirm `MonomorphisePass` and `ClosureConversionPass` run after
   lowering. Default substitution at step 4 happens during initial
   lowering, before any pass — closures and generics see the
   substituted args naturally, satisfying open question 3 (generic
   defaults monomorphise per specialization for free) and open
   question 5 (captured vars in defaults are visible to closure
   conversion).

6. **Tests.**
   - Validator: `f(1)` accepted for `f(x, y=2)`; `f()` rejected.
   - Pass1: parser/semantic rejects `f(x = 1, y)` (default before
     non-default).
   - Lowering: `f(5)` for `f(x, y=2)` produces
     `FunctionCall { args: [5, 2] }`.
   - Earlier-param ref: `f(5)` for `f(x, y = x + 1)` produces a
     `Let { __t_x: 5; f(__t_x, __t_x + 1) }` shape.
   - Generic defaults: `f::<I32>()` for `f<T>(x: T = T::default())`
     monomorphises with the I32-specialized default expression.
   - Overload resolution: `f(x)` and `f(x, y=1)` both defined; `f(1)`
     resolves to the no-default overload.
   - Module-level state: `f(x = current_count())` re-evaluates per
     call (verifiable via side-effect counter).
   - Closure conversion: a captured variable referenced by a default
     expression survives the closure-conversion sweep.

7. **Documentation.**
   `docs/user/formalang.md` Function Definitions section: document
   default-parameter syntax, positional-from-the-right rule,
   earlier-param references, re-evaluation semantics, overload-
   resolution rule.
   `docs/developer/ir.md`: note that `IrFunctionParam.default` is a
   transient AST/lowering artifact — the `IrExpr` is inlined at
   every call site, never threaded through codegen.

### Resolved open questions

1. **Default expressions referencing other parameters.** Allowed.
   The lowering wraps the call in `IrExpr::Let` bindings for each
   preceding non-defaulted arg when any default references an
   earlier param. The default's `IrExpr::Ref(x)` resolves to the
   bound temporary, avoiding side-effect duplication.
2. **Default expressions referencing module-level state.**
   Re-evaluated per call. Each call site has its own copy of the
   default IR; `current_count()` runs once per call. Sidesteps
   Python's mutable-default footgun.
3. **Generic functions.** Works naturally under inline + monomorphise.
   Each specialization gets its own copy of the default expression
   with `T` substituted by the existing monomorphise machinery.
4. **Effective overload resolution.** Most-specific wins. The
   overload requiring the fewest defaults to fire is preferred.
   Genuine ambiguity (two equally-specific matches at the same gap)
   errors at semantic time.
5. **Interaction with closure conversion.** Trivially handled.
   Defaults are inlined during initial IR lowering, before
   `ClosureConversionPass` runs. Captured variables referenced by
   defaults are visible to the closure-conversion sweep like any
   other expression.

### Exit criteria

- `f(1)` compiles where `f(x: I32, y: I32 = 2)` is defined; the IR's
  `FunctionCall.args` has two entries (the explicit `1` and the
  inlined `2`).
- `f(5)` compiles where `f(x: I32, y: I32 = x + 1)` is defined; the
  call site is wrapped in a `Let` binding `x` to `5`, with `y`
  resolving the `x` reference to that binding.
- formawasm Phase 5 #3 lowers programs using default parameters
  end-to-end with no backend-side substitution work.
- This plan file deleted by the implementing PR.

## Status in the formawasm backend

Phase 5 #3 was queued for default parameter values but found no
IR shape to consume. The `IrFunctionParam.default` field is
preserved end-to-end; the missing piece is the upstream substitute-
or-reject decision at semantic time. Backend lifting is a single
arm in `lower::call::lower_function_call` once the IR shape lands.
