# Control Flow & Pattern Matching

All control flow is **compile-time validated**. Each form is an
expression: it evaluates to a value.

## For Expressions

A `for` produces a **lazy sequence**, `Seq<T>`. It is not a
collection: it holds no elements and **nothing runs** until a terminal
combinator consumes it. So a chain of stages collapses into a single
pass with no intermediate array, and the one step that allocates is
the one you can see.

```formalang
// Ends in a single value: no allocation, one pass.
let total: I32 = for n in [1, 2, 3, 4, 5] { n }.fold(initial: 0, f: (a, b) -> a + b)

// Ends in an array: one allocation, and `collect` says so.
let doubled: [I32] = for n in [1, 2, 3] { n * 2 }.collect()

// Runs for its calls alone.
for email in user.emails { validate(address: email) }.run()

// Iterates a range.
let count: I32 = for i in 0..10 { i }.count()

// Nested: the inner sequence is made and consumed within one step of
// the outer one, so neither builds an array.
let sum: I32 = for row in matrix {
  for cell in row { cell }.fold(initial: 0, f: (a, b) -> a + b)
}.fold(initial: 0, f: (a, b) -> a + b)
```

### Combinators

`for` is in the language; the combinators are in the standard library.
A pipeline normally uses both.

| Kind | Members |
| --- | --- |
| Adapter, `Seq` to `Seq` | `map`, `filter`, `take`, `skip` |
| Terminal, ends the pipeline | `collect`, `count`, `fold`, `first`, `any`, `all`, `run` |

```formalang
let big: I32 = for x in xs { x * 2 }
  .filter(f: (v) -> v > 10)
  .count()
```

### A sequence is consumed exactly once

**At most once.** Reading a sequence twice is an error (E136). A
sequence runs once and keeps nothing, so a second read cannot mean
what you want. To read the same values twice, `collect()` into an
array first and iterate that.

**At least once.** Dropping a sequence is an error (E135). A dropped
sequence is not like a dropped number: drop an `I32` and a value was
computed and ignored, but drop a `Seq` and nothing ran at all.

```formalang
for x in xs { log(message: "x") }        // E135: the logs never run
for x in xs { x * 2 }                    // E135: computes nothing

let s = for x in xs { x }
let a: I32 = s.count()
let b: I32 = s.count()                   // E136: s holds nothing now
```

**Rules**:

- The source is an array, a range, or another sequence
- The result is `Seq<body_type>`, consumed exactly once
- The loop variable is scoped to the body and typed as the element
- A sequence cannot be a struct field, an enum payload, a module-level
  `let`, or a function return type (E137). It holds no elements and
  runs once, so there is nothing to store or hand back

## If Expressions

Conditional expressions:

```formalang
// Boolean condition
if count > 0 {
  showItems()
} else {
  showEmpty()
}

// Without else (returns nil if false)
if isAdmin {
  showAdminPanel()
}

// So the type of an `if` with no `else` is optional. It fits an
// optional binding and not a plain one:
let a: I32? = if isAdmin { 1 }    // ok
let b: I32  = if isAdmin { 1 }    // error: I32? does not fit I32

// Chained conditions
if x > 100 {
  showLarge()
} else if x > 50 {
  showMedium()
} else {
  showSmall()
}
```

**Optionals in conditionals (`if let`)**:

To consume an optional value, use `if let` to bind the inner value
inside the truthy branch. The form is Rust-style: pattern, equals,
optional expression, then both branches.

```formalang
if let nickname = user.nickname {
  // nickname is bound to the unwrapped String here
  greet(name: nickname)
} else {
  // taken when user.nickname is nil
  greet(name: user.name)
}
```

`if let` requires both a `then` and an `else` branch, since the result
type unifies the two. The else branch is taken when the optional is
nil. Internally `if let pat = optional { … } else { … }` desugars to a
match on `.some(pat)` and `.none`, so its semantics match a two-arm
match expression.

## Match Expressions

Pattern matching on enums (exhaustive):

```formalang
pub enum Status { pending, active, completed }

match status {
  .pending: waitFor(),
  .active: runNow(),
  .completed: finalize()
}

// With data binding (named parameters)
pub enum Message {
  text(content: String)
  image(url: String, size: I32)
}

match message {
  .text(content): displayText(value: content),
  .image(url, size): displayImage(src: url, bytes: size)
}
```

**Rules**:

- Must be exhaustive (cover all variants)
- Pattern uses `.variant` syntax (short form)
- Associated data bound to identifiers using parameter names

## An Arm Below `_` Never Runs

A `_` arm takes every value the arms above it did not, so nothing
below it can run:

```formalang
match e {
  _: 0,
  .a: 1           // error E140: this arm can never run
}
```

Write the named arms first and `_` last.
