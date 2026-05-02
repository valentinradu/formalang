# Closures

Closure *types* (function-shaped types in fields, params, returns) live
in [Type System / Closure Types](types.md#closure-types). This page
covers the **expression form** — the values you assign to those types.

Closures are pure, single-expression functions:

```formalang
pub enum Event {
  textChanged(value: String),
  resized(width: I32, height: I32),
  submit
}

pub struct Form<E> {
  onChange: String -> E,
  onResize: I32, I32 -> E,
  onSubmit: () -> E,
  onScale: mut I32 -> E,
  onConsume: sink String -> E
}

impl Form {
  // Single parameter - no parens needed
  onChange: x -> .textChanged(value: x),

  // Multiple parameters - comma separated
  onResize: w, h -> .resized(width: w, height: h),

  // No parameters - empty parens required
  onSubmit: () -> .submit,

  // mut convention — caller must pass a mutable binding
  onScale: mut n -> .resized(width: n, height: n),

  // sink convention — caller's binding is consumed
  onConsume: sink s -> .textChanged(value: s)
}
```

**Expression syntax**:

| Parameters          | Syntax              | Example                          |
| ------------------- | ------------------- | -------------------------------- |
| None                | `() -> expr`        | `() -> .submit`                  |
| One (default)       | `x -> expr`         | `x -> .changed(value: x)`        |
| One (mut)           | `mut x -> expr`     | `mut n -> .resized(width: n, height: n)` |
| One (sink)          | `sink x -> expr`    | `sink s -> .text(value: s)`      |
| Multiple            | `x, y -> expr`      | `x, y -> .point(x: x, y: y)`     |
| With types          | `x: T -> expr`      | `x: String -> .text(x: x)`       |
| Pipe syntax         | `\|x, y\| expr`     | `\|x, y\| x + y`                 |

**Rules**:

- Closures are **pure** — no side effects, single expression body
- Single parameter does not need parentheses
- Multiple parameters are comma-separated
- Empty parameters require parentheses: `() -> expr`
- Convention keywords (`mut`, `sink`) precede the parameter name
- Type annotations optional when inferable
- Convention on a closure param means the **caller of the closure** must satisfy it

## Caller Constraints

When a closure type carries `mut` or `sink`, every caller is checked
against that requirement at compile time:

```formalang
let scale: mut I32 -> I32 = mut n -> n

let mut x: I32 = 10
let _r: I32 = scale(x)   // ok: x is mutable

let y: I32 = 5
let _s: I32 = scale(y)   // error: MutabilityMismatch — y is immutable

let consume: sink String -> String = sink s -> s

let label: String = "hello"
let _a: String = consume(label)  // ok: label is moved
let _b: String = label           // error: UseAfterSink — label was consumed
```
