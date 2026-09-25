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
pub struct User { emails: [String] }

extern fn validate(address: String) -> Boolean

pub fn check(user: User, matrix: [[I32]]) -> I32 {
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

  total + doubled.len() + count + sum
}
```

### Combinators

`for` is in the language; the combinators are in the standard library.
A pipeline normally uses both.

| Kind | Members |
| --- | --- |
| Adapter, `Seq` to `Seq` | `map`, `filter`, `take`, `skip` |
| Terminal, ends the pipeline | `collect`, `count`, `fold`, `first`, `any`, `all`, `run` |

```formalang
pub fn run_checks() {
  let xs = [3, 6, 9]
  let big: I32 = for x in xs { x * 2 }
    .filter(f: (v) -> v > 10)
    .count()
  assert(condition: big == 2)
}
```

`map` can change the element type: its closure gives the new one.

```formalang
pub fn run_checks() {
  let xs = [5, 15]
  let flags: [Boolean] = for x in xs { x }.map(f: (v) -> v > 10).collect()
  assert(condition: flags.len() == 2)
}
```

`collect` has two forms. With no arguments, it makes an array. With
`key` and `value`, it makes a dictionary: the two closures give the
entry of each element.

```formalang
pub struct User { id: I32, name: String }

pub fn run_checks() {
  let users = [User(id: 1, name: "Ada"), User(id: 2, name: "Bo")]
  let ids: [I32] = for u in users { u.id }.collect()
  let names: [I32: String] = for u in users { u }
    .collect(key: (u) -> u.id, value: (u) -> u.name)
  assert(condition: ids.len() == 2)
  assert(condition: names[2] != nil)
}
```

When two elements give the same key, the value of the later element
replaces the earlier one. The entry keeps its place, and the
dictionary holds one entry for the key.

The accumulator of `fold` takes the type of `initial`. This type can
be different from the element type:

```formalang
pub fn run_checks() {
  let xs = [4, 12]
  let any_big: Boolean = for x in xs { x }
    .fold(initial: false, f: (seen, x) -> seen || x > 10)
  assert(condition: any_big)
}
```

### A sequence is consumed exactly once

**At most once.** Reading a sequence twice is an error (E136). A
sequence runs once and keeps nothing, so a second read cannot mean
what you want. To read the same values twice, `collect()` into an
array first and iterate that.

**At least once.** Dropping a sequence is an error (E135). A dropped
sequence is not like a dropped number: drop an `I32` and a value was
computed and ignored, but drop a `Seq` and nothing ran at all.

```formalang,reject=SeqNotConsumed
extern fn log(message: String)

pub fn f(xs: [I32]) {
  for x in xs { log(message: "x") }        // E135: the logs never run
}
```

```formalang,reject=SeqUsedTwice
pub fn f(xs: [I32]) -> I32 {
  let s = for x in xs { x }
  let a: I32 = s.count()
  let b: I32 = s.count()                   // E136: s holds nothing now
  a + b
}
```

A sequence is read once also where the program branches: a read in
each branch of an `if`, or in each arm of a `match`, is one read. A
read in only one branch leaves the sequence unconsumed on the other
path, which is the error E135.

**Rules**:

- The source is an array, a range, or another sequence. Any other
  source, such as a dictionary or an optional array, is the error
  `ForLoopNotArray`
- The result is `Seq<body_type>`, consumed exactly once
- The loop variable is scoped to the body and typed as the element
- A sequence lives only where it is made and consumed. It holds no
  elements and runs once, so there is nothing to store or hand back.
  These positions are the error `SeqInvalidPosition` (E137): a struct
  field, an enum variant field, a module-level `let`, a function return
  type, a closure return type, and a value inside another type, such
  as `[Seq<I32>]`, `Seq<I32>?` or a tuple field
- A parameter can take a sequence only with the `sink` convention,
  because the function must consume it: `fn total(sink s: Seq<I32>)`
- An `extern fn` may return a sequence: the host produces the
  elements. See [Large Data](large-data.md)

## If Expressions

Conditional expressions:

```formalang
pub fn size_label(count: I32, x: I32) -> String {
  // Boolean condition
  let items = if count > 0 {
    "some"
  } else {
    "none"
  }

  // Chained conditions
  if x > 100 {
    "large"
  } else if x > 50 {
    "medium"
  } else {
    items
  }
}

pub fn run_checks() {
  assert(condition: size_label(count: 1, x: 200) == "large")
  assert(condition: size_label(count: 0, x: 1) == "none")
}
```

The condition must be a `Boolean`. Any other type is the error
`InvalidIfCondition`. The two branches must have one type.

Without `else`, an `if` is `nil` when the condition is false. So the
type of an `if` with no `else` is optional. It fits an optional binding
and not a plain one:

```formalang,reject=OptionalUsedAsNonOptional
pub fn f(is_admin: Boolean) -> I32 {
  let a: I32? = if is_admin { 1 }    // ok
  let b: I32 = if is_admin { 1 }     // error: an I32? does not fit I32
  b
}
```

**Optionals in conditionals (`if let`)**:

To consume an optional value, use `if let` to bind the inner value
inside the truthy branch. The form is Rust-style: pattern, equals,
optional expression, then both branches.

```formalang
pub struct User { name: String, nickname: String? }

fn greet(name: String) -> String { "Hi, " + name }

pub fn welcome(user: User) -> String {
  if let nickname = user.nickname {
    // nickname is bound to the unwrapped String here
    greet(name: nickname)
  } else {
    // taken when user.nickname is nil
    greet(name: user.name)
  }
}

pub fn run_checks() {
  assert(condition: welcome(user: User(name: "Ada", nickname: "A")) == "Hi, A")
  assert(condition: welcome(user: User(name: "Ada", nickname: nil)) == "Hi, Ada")
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

pub fn step(status: Status) -> I32 {
  match status {
    .pending: 0,
    .active: 1,
    .completed: 2
  }
}

// With data binding
pub enum Message {
  text(content: String),
  image(url: String, size: I32)
}

pub fn weight(message: Message) -> I32 {
  match message {
    .text(content): content.len(),
    .image(url, size): url.len() + size
  }
}

// `_` ignores a value
pub fn size(message: Message) -> I32 {
  match message {
    .text(_): 0,
    .image(_, s): s
  }
}

pub fn run_checks() {
  assert(condition: step(status: .active) == 1)
  assert(condition: weight(message: .text(content: "hey")) == 3)
  assert(condition: weight(message: .image(url: "/a", size: 10)) == 12)
  assert(condition: size(message: .image(url: "/a", size: 10)) == 10)
}
```

**Rules**:

- The value must be an enum or an optional. Any other type is the
  error `MatchNotEnum`. An optional has the arms `.some(x)` and `.none`
- Must be exhaustive (cover all variants): a missing variant is the
  error `NonExhaustiveMatch`. A `_` arm covers each variant that no arm
  above it names
- Pattern uses `.variant` syntax (short form)
- Each name in the pattern takes the associated value at the same
  position in the variant. The names are free: `.image(u, s)` binds
  `u` to `url` and `s` to `size`. A name does not select a field by its
  name
- The pattern gives one name for each associated value. Another count
  is the error `VariantArityMismatch`. A name that the arm does not
  read is legal
- Write `_` in place of a name to ignore that value. `.image(_, s)`
  binds only `s`. A `_` counts as one value, and it can occur more
  than once, as in `.pair(_, _)`
- A name bound in an arm is visible only in that arm
- Two arms for one variant are the error `DuplicateMatchArm`
- All arms must have one type

## An Arm Below `_` Never Runs

A `_` arm takes every value the arms above it did not, so nothing
below it can run:

```formalang,reject=UnreachableMatchArm
pub enum E { a, b }

pub fn f(e: E) -> I32 {
  match e {
    _: 0,
    .a: 1           // error E140: this arm can never run
  }
}
```

Write the named arms first and `_` last.
