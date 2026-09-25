# Functions

Top-level functions, parameter conventions, codegen attributes, and
overloading. Closure expressions live on a separate page: see
[Closures](closures.md).

## Definitions

```formalang
fn add(a: I32, b: I32) -> I32 {
  a + b
}

pub fn greet(name: String) -> String {
  "Hello, " + name
}

// No return type (returns unit)
fn log_value(value: I32) {
  value
}

// Generic function
pub fn identity<T>(value: T) -> T {
  value
}
```

## Calls

A call gives each argument with the label of its parameter, or by
position:

```formalang
fn area(width: I32, height: I32) -> I32 {
  width * height
}

pub fn run_checks() {
  assert(condition: area(width: 2, height: 3) == 6)   // labels
  assert(condition: area(height: 3, width: 2) == 6)   // labels in any order
  assert(condition: area(2, 3) == 6)                  // positions
  assert(condition: area(2, height: 3) == 6)          // both
}
```

A label binds the argument to the parameter of that name, in any
order. An argument with no label takes the parameter at its position.

A parameter can have an external label in front of its name. The call
writes the label, and the body reads the name:

```formalang
fn shift(by amount: I32, from start: I32) -> I32 {
  start + amount
}

pub fn run_checks() {
  assert(condition: shift(by: 2, from: 10) == 12)
}
```

The compiler checks each call:

- Too many or too few arguments is the error `ArgumentCountMismatch`
  (E138).
- A label that names no parameter is the error `NoMatchingOverload`.
- Two arguments with one label are the error `DuplicateDefinition`.
- Each argument must have the type of its parameter. An unsuffixed
  literal takes the parameter type (see
  [Type System / The type of a literal](types.md#the-type-of-a-literal)).

A call can call the closure that another call returns: `adder()(3)`
calls `adder`, then calls its result with `3`.

```formalang
fn adder() -> (I32) -> I32 {
  (y) -> y + 10
}

pub fn run_checks() {
  assert(condition: adder()(3) == 13)
}
```

## Default Parameter Values

Parameters may declare a default value with `= expr`:

```formalang
fn greet(name: String, greeting: String = "Hello") -> String {
  greeting + ", " + name
}

pub fn run_checks() {
  assert(condition: greet("world") == "Hello, world")        // the default
  assert(condition: greet("world", "Hi") == "Hi, world")     // an argument
  assert(condition: greet(name: "you", greeting: "Hey") == "Hey, you")
}
```

Rules:

- **Defaults must be positional from the right.** `fn f(x: I32 = 0, y: I32)`
  is rejected at definition time (`RequiredParamAfterDefault`): every
  parameter after a defaulted one must also have a default (`self` is
  not counted).
- **Defaults may reference earlier parameters.** `fn f(x: I32, y:
  I32 = x + 1)` is valid; calls like `f(5)` lower to a Let-wrapped
  Block that binds `x` to the call-site value, so the default sees
  the actual passed value. A default cannot read a later parameter.
- **Defaults are re-evaluated on every call.** `fn f(x: I32 = current_count())`
  runs `current_count()` once per call: Python's mutable-default
  footgun is avoided.
- **Overload resolution prefers the no-default match.** With both
  `fn f(x: I32)` and `fn f(x: I32, y: I32 = 1)` defined, `f(5)`
  resolves to the no-default overload.

## Codegen Attributes

Three optional keyword prefixes hint to backends about call-site
behavior. They are pure metadata: the frontend passes them through
unchanged. Multiple prefixes can stack and combine freely with
`pub` and `extern`.

```formalang
inline fn fast_add(a: I32, b: I32) -> I32 { a + b }
no_inline fn dont_inline_me() -> I32 { 42 }
cold fn rare_error_path() { 0 }

pub cold extern fn abort() -> Never
```

| Prefix      | Meaning                                                |
| ----------- | ------------------------------------------------------ |
| `inline`    | Hint: inline this function at every call site if possible |
| `no_inline` | Hint: do not inline                                    |
| `cold`      | Hint: this function is rarely called (error / branch)  |

## Parameter Conventions

FormaLang uses Mutable Value Semantics. Every parameter has a convention that
controls how the callee may use the value:

| Convention | Syntax            | Meaning                                          |
| ---------- | ----------------- | ------------------------------------------------ |
| (default)  | `x: T`            | Immutable. Callee reads only.                    |
| `mut`      | `mut x: T`        | Exclusive mutable. **The caller sees the change.** |
| `sink`     | `sink x: T`       | Ownership transfer. Caller gives up the value.   |

`mut` is not a private copy. The callee gets exclusive access to the
caller's value, and every change it makes is visible to the caller
when the call returns — which is why the argument must be a `let mut`
binding. Exclusivity is guaranteed by the compiler, so no two live
references to the same value ever exist.

```formalang
// Default: immutable parameter
fn read(x: I32) -> I32 {
  x
}

// mut: callee may mutate; argument must be let mut at call site
fn bump(mut score: I32) -> I32 {
  score
}

// sink: callee owns the value; caller cannot use it after
fn consume(sink label: String) -> String {
  label
}
```

The same conventions apply to `self` in impl methods:

```formalang
pub struct Counter { count: I32 }

impl Counter {
  fn value(self) -> I32 { self.count }             // immutable self
  fn increment(mut self) { self.count = self.count + 1 } // mutable self
  fn destroy(sink self) -> I32 { self.count }      // consuming self
}

pub fn run_checks() {
  let mut c = Counter(count: 1)
  c.increment()                  // c is let mut, so it satisfies mut self
  assert(condition: c.value() == 2)
  assert(condition: c.destroy() == 2)   // c is consumed here
}
```

Call sites are transparent: no extra syntax required. The caller sees
the change that a `mut` parameter makes:

```formalang
fn bump(mut score: I32) {
  score = score + 1
}

pub fn run_checks() {
  let mut n: I32 = 0
  bump(score: n)         // n is let mut, so it satisfies mut convention
  assert(condition: n == 1)
}
```

A `mut` argument that is not a `let mut` binding is the error
`MutabilityMismatch`. A literal is not a binding, so it cannot be a
`mut` argument either.

### Sink

A `sink` argument gives the value away. The rules follow from that:

- A binding that a `sink` argument consumed cannot be read again: the
  error is `UseAfterSink`. This holds on every path: a sink in one
  branch of an `if` consumes the binding after the `if`.
- A `let mut` binding that was consumed can take a new value. After
  the assignment, you can read it again.
- A sink in a loop body or in a closure body can run more than once, so
  it cannot consume a binding from outside that body (`UseAfterSink`).
- A module `let` belongs to every function, so no call can consume it
  (`UseAfterSink`).
- A plain or `mut` parameter belongs to the caller. A function cannot
  give it away with `sink` (`MutabilityMismatch`).
- A sink of a field consumes a part of the struct. After it, the
  struct is not whole, and a read of the whole struct is a
  `UseAfterSink`.

```formalang,reject=UseAfterSink
fn take(sink s: String) -> I32 { s.len() }

pub fn f(flag: Boolean) -> I32 {
  let label: String = "hello"
  let n = if flag { take(s: label) } else { 0 }
  n + label.len()          // error: label may be consumed
}
```

```formalang
fn take(sink s: String) -> I32 { s.len() }

pub fn run_checks() {
  let mut label: String = "hello"
  let a = take(s: label)
  label = "hi"             // the binding takes a new value
  assert(condition: a + label.len() == 7)
}
```

Closure parameters carry the same conventions; the convention constrains
the **caller of the closure**: see [Closures](closures.md) for details.

### Exclusive Access

A `mut` argument and a `sink` argument each need sole access to the
value for the whole call. So two arguments of one call must not reach
the same value when one of them is `mut` or `sink`:

```formalang,reject=OverlappingArguments
fn swap(mut a: I32, mut b: I32) {
  let t = a
  a = b
  b = t
}

fn put(mut a: I32, b: I32) {
  a = b
}

pub fn f() {
  let mut x: I32 = 1
  swap(a: x, b: x)         // error E144: two arguments reach 'x'
  put(a: x, b: x)          // error E144: 'b' reads what 'a' changes
}
```

A default argument goes by pointer too, so it would see the change that
the callee makes. That is why a read and a write may not share a value.
Two reads may.

The check follows fields. Two different fields of one binding are two
places, and a field is part of its struct:

```formalang,reject=OverlappingArguments
pub struct Point { x: I32, y: I32 }

fn swap(mut a: I32, mut b: I32) {
  let t = a
  a = b
  b = t
}

fn reset(mut p: Point, x: I32) {
  p.x = x
}

pub fn f() {
  let mut p: Point = Point(x: 1, y: 2)
  swap(a: p.x, b: p.y)     // ok: two places
  reset(p: p, x: p.x)      // error E144: 'p' holds 'p.x'
}
```

The receiver of a method counts as an argument, with the convention of
its `self` parameter. So `c.set(to: c.n)` is an error when `set` takes
`mut self`. A call to a closure binding follows the same rule.

To pass the same value twice, copy it into its own binding first. Under
value semantics the copy is a separate place:

```formalang
fn put(mut a: I32, b: I32) {
  a = b
}

pub fn run_checks() {
  let mut x: I32 = 1
  let y: I32 = x           // a copy: a separate place
  x = 7
  put(a: x, b: y)          // ok
  assert(condition: x == 1)
}
```

## Function Overloading

Multiple functions with the same name are allowed when their signatures differ.
The compiler selects the right overload at each call site.

**By labels and count**: the labels of the call and the number of
arguments pick the overload:

```formalang
fn format(value: I32) -> String { "number" }
fn format(value: I32, precision: I32) -> String { "precise" }
fn format(text: String) -> String { "text" }

pub fn run_checks() {
  assert(condition: format(value: 1) == "number")
  assert(condition: format(value: 1, precision: 2) == "precise")
  assert(condition: format(text: "a") == "text")
}
```

**By argument types**: when the labels and the count fit more than one
overload, the types of the arguments pick one. This works with labels
and with positions:

```formalang
fn describe(value: I32) -> String { "I32" }
fn describe(value: I64) -> String { "I64" }
fn describe(value: String) -> String { "String" }

fn process(I32) -> String { "number" }
fn process(String) -> String { "string" }

pub fn run_checks() {
  assert(condition: describe(value: 1) == "I32")
  assert(condition: describe(value: 1I64) == "I64")
  assert(condition: describe(value: "a") == "String")
  assert(condition: process(1) == "number")
  assert(condition: process("a") == "string")
}
```

**Rules**:

- An overload fits a call when each label of the call names one of its
  parameters, the argument count is between its required and its
  declared parameters, and each argument has the type of its parameter
- The types are compared exactly first. An unsuffixed literal fits its
  default type first: `describe(value: 1)` means the `I32` overload. If
  no overload fits exactly, the literal fits any numeric type of its
  kind
- Of the overloads that fit, the one that leaves the fewest parameters
  to their defaults wins
- Two definitions with the same labels and parameter types are a
  `DuplicateDefinition`, also when their return types differ
- A parameter with no name, as in `fn process(I32)`, takes only an
  argument by position
- A call that no overload fits is the error `NoMatchingOverload`. A
  call that two overloads fit equally well is the error `AmbiguousCall`

### Methods

A method overloads the same way a free function does, and the same
rules pick the one a call means:

```formalang
pub struct Formatter {
  tag: I32
}

impl Formatter {
  fn format(self, text: String) -> String { text }
  fn format(self, value: I32, prefix: String) -> String { prefix }
}

let a: String = Formatter(tag: 1).format(text: "x")
let b: String = Formatter(tag: 1).format(value: 2, prefix: "p")
```

The receiver is not counted when the labels are matched: a call
supplies it separately.
