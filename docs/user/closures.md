# Closures

Closure *types* (function-shaped types in fields, params, returns) live
in [Type System / Closure Types](types.md#closure-types). This page
covers the **expression form**: the values you assign to those types.

Closures are pure, single-expression functions. Every closure form
wraps its parameter list in parentheses, mirroring `fn` signatures, so
every `->` in the language is preceded by `)`:

```formalang
pub enum Event {
  textChanged(value: String),
  resized(width: I32, height: I32),
  submit
}

pub struct Form<E> {
  onChange:  (String) -> E,
  onResize:  (I32, I32) -> E,
  onSubmit:  () -> E,
  onScale:   (mut I32) -> E,
  onConsume: (sink String) -> E
}

impl Form {
  // Single parameter — parens required
  onChange: (x) -> .textChanged(value: x),

  // Multiple parameters — comma separated, inside the parens
  onResize: (w, h) -> .resized(width: w, height: h),

  // No parameters — empty parens
  onSubmit: () -> .submit,

  // mut convention: caller must pass a mutable binding
  onScale: (mut n) -> .resized(width: n, height: n),

  // sink convention: caller's binding is consumed
  onConsume: (sink s) -> .textChanged(value: s)
}
```

**Expression syntax**:

| Parameters    | Syntax            | Example                                |
| ------------- | ----------------- | -------------------------------------- |
| None          | `() -> expr`      | `() -> .submit`                        |
| One           | `(x) -> expr`     | `(x) -> .changed(value: x)`            |
| One (mut)     | `(mut x) -> expr` | `(mut n) -> .resized(width: n, height: n)` |
| One (sink)    | `(sink x) -> expr`| `(sink s) -> .text(value: s)`          |
| Multiple      | `(x, y) -> expr`  | `(x, y) -> .point(x: x, y: y)`         |
| With types    | `(x: T) -> expr`  | `(x: String) -> .text(x: x)`           |

**Rules**:

- Closures are **pure**: no side effects, single expression body.
- The parameter list is *always* parenthesised — even for a single
  parameter — so `->` is unambiguous.
- Empty parameters: `() -> expr`.
- Convention keywords (`mut`, `sink`) precede the parameter name
  inside the parens.
- Type annotations are optional when the closure type can be inferred
  from the binding annotation or call context.
- Convention on a closure param means the **caller of the closure**
  must satisfy it.

## Caller Constraints

When a closure type carries `mut` or `sink`, every caller is checked
against that requirement at compile time:

```formalang
let scale: (mut I32) -> I32 = (mut n) -> n

let mut x: I32 = 10
let _r: I32 = scale(x)   // ok: x is mutable

let y: I32 = 5
let _s: I32 = scale(y)   // error: MutabilityMismatch: y is immutable

let consume: (sink String) -> String = (sink s) -> s

let label: String = "hello"
let _a: String = consume(label)  // ok: label is moved
let _b: String = label           // error: UseAfterSink: label was consumed
```
