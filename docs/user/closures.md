# Closures

Closure *types* (function-shaped types in fields, params, returns) live
in [Type System / Closure Types](types.md#closure-types). This page
covers the **expression form**: the values you assign to those types.

Closures are single-expression functions. Every closure form
wraps its parameter list in parentheses, mirroring `fn` signatures, so
every `->` in the language is preceded by `)`:

```formalang
pub enum Event {
  textChanged(value: String),
  resized(width: I32, height: I32),
  submit
}

struct Form<E> {
  onChange:  (String) -> E,
  onResize:  (I32, I32) -> E,
  onSubmit:  () -> E,
  onScale:   (mut I32) -> E,
  onConsume: (sink String) -> E
}

fn make_form() -> Form<Event> {
  Form<Event>(
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
  )
}
```

Each field type gives the closure its parameter types and its return
type, so the closures need no annotations.

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

- The body is one expression. A block `{ ... }` is one expression, so
  a body can hold `let` statements inside braces.
- The parameter list is *always* parenthesised — even for a single
  parameter — so `->` is unambiguous.
- Empty parameters: `() -> expr`.
- Convention keywords (`mut`, `sink`) precede the parameter name
  inside the parens.
- A parameter type is optional only where a declared type gives it.
  See [Parameter Types](#parameter-types).
- Convention on a closure param means the **caller of the closure**
  must satisfy it.
- Closures are **internal-only**: a `pub struct` field, or a `pub enum`
  variant field, cannot have a closure type. Drop the `pub` (so the type
  stays inside its module), or replace the field with a non-closure type.

## Parameter Types

A closure parameter with no type takes its type from the position of
the closure. A position gives a type only when a declared type gives
it:

- the annotation of a `let`: `let f: (I32) -> I32 = (x) -> x + 1`;
- the declared type of a parameter, for an argument:
  `apply(f: (x) -> x * 2)`;
- the declared type of a field, and of a parameter or field default;
- the declared return type, for the result of a function, or of a
  closure that declares its return type.

The type goes through `( )`, the branches of `if` and `match`, the
result of a block, and the elements of an array, a tuple and a
dictionary. An optional closure slot, such as `(() -> E)?`, gives the
type of its closure.

In each other position, a parameter with no type is an error:

```formalang,reject=ClosureParameterNeedsType
let f = (x) -> x + 1            // error E145: 'x' needs a type
let g = (x: I32) -> x + 1       // ok
let h: (I32) -> I32 = (x) -> x  // ok
```

The compiler does not infer a parameter type from the body, or from a
later call. A closure with the wrong number of parameters for its
position is a type mismatch.

## The Call Shape

A call to a closure gives one argument for each parameter the closure
type declares, and each argument must have the parameter's type:

```formalang,reject=ArgumentCountMismatch
fn apply(f: (I32, I32) -> I32, a: I32, b: I32) -> I32 {
  f(a, b)                 // ok
}

fn wrong(f: (I32) -> I32) -> I32 {
  f(1, 2)                 // error E138: this closure takes 1 argument
}

fn also_wrong(f: (I32) -> I32) -> I32 {
  f("text")               // error: TypeMismatch: expected I32
}
```

A closure type has no parameter names, so a call to a closure takes
its arguments by position. A label is the error E150
(`LabelledClosureArgument`):

```formalang,reject=LabelledClosureArgument
fn apply(f: (I32) -> I32) -> I32 {
  f(x: 1)                 // error E150: a closure call takes no labels
}
```

## Returning a Closure

A function can return a closure. A closure captures by value: when the
closure is made, it keeps a copy of each binding that its body names.
So a returned closure can capture a module `let`, a plain parameter, a
`mut` parameter or a local `let`. A later change to the binding does
not change the copy. A `sink` parameter moves into the closure, and the
function cannot use it after that:

```formalang
pub let base: I32 = 100

fn from_base() -> (I32) -> I32 {
  (y) -> y + base                  // ok: a module let
}

fn from_owned(sink step: I32) -> (I32) -> I32 {
  (y) -> y + step                  // ok: step moves into the closure
}

fn adder(x: I32) -> (I32) -> I32 {
  (y) -> x + y                     // ok: a copy of the parameter
}

fn from_local() -> (I32) -> I32 {
  let k = 3
  (y) -> y + k                     // ok: a copy of the local let
}

pub fn run_checks() {
  assert(condition: from_base()(1) == 101)
  assert(condition: from_owned(step: 2)(1) == 3)
  assert(condition: adder(x: 2)(3) == 5)
  assert(condition: from_local()(1) == 4)
}
```

## Caller Constraints

When a closure type carries `mut` or `sink`, every caller is checked
against that requirement at compile time:

```formalang,reject=MutabilityMismatch
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
