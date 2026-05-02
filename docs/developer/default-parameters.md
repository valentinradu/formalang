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

## Open questions for Direction A

1. **Default expressions referencing other parameters.** Is
   `fn f(x: I32, y: I32 = x + 1)` allowed? If so, the lowering
   needs to capture the call-site values of preceding params and
   feed them into the default.
2. **Default expressions referencing module-level state.** A
   default like `y: I32 = current_count()` could read a global
   that changes between calls. Inline copies would re-evaluate
   per call (matches user expectation); wrapper functions
   centralize the call (also matches user expectation, if the
   wrapper is invoked once per call site).
3. **Generic functions.** When the default expression mentions a
   generic-typed value (`x: T = T::default()`), the IR-lowering
   needs to specialize per instantiation. Direction A sub-option
   "inline" works naturally; "wrapper" needs synthesized wrappers
   per specialization.
4. **Effective overload resolution.** A function with N defaults
   effectively defines `N+1` callable arities. Today the validator
   resolves overloads by exact arity. Defaults broaden each entry
   to a range; the resolver needs to know which params have
   defaults to disambiguate.
5. **Interaction with closure conversion.** A captured variable
   referenced by a default expression needs to be hoisted into the
   capture environment when the default fires inside a lifted
   closure body. Trivially handled if defaults are inlined before
   closure conversion runs.

## Status in the formawasm backend

Phase 5 #3 was queued for default parameter values but found no
IR shape to consume. The `IrFunctionParam.default` field is
preserved end-to-end; the missing piece is the upstream substitute-
or-reject decision at semantic time. Backend lifting is a single
arm in `lower::call::lower_function_call` once the IR shape lands.
