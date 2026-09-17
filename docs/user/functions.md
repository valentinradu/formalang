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

## Default Parameter Values

Parameters may declare a default value with `= expr`:

```formalang
fn greet(name: String, greeting: String = "Hello") -> String {
  greeting + ", " + name
}

greet("world")              // greeting = "Hello"
greet("world", "Hi there")  // greeting = "Hi there"
```

Rules:

- **Defaults must be positional from the right.** `fn f(x = 0, y)`
  is rejected at definition time: every parameter after a defaulted
  one must also have a default (`self` is not counted).
- **Defaults may reference earlier parameters.** `fn f(x: I32, y:
  I32 = x + 1)` is valid; calls like `f(5)` lower to a Let-wrapped
  Block that binds `x` to the call-site value, so the default sees
  the actual passed value.
- **Defaults are re-evaluated on every call.** `fn f(x: I32 = current_count())`
  runs `current_count()` once per call site: Python's mutable-
  default footgun is avoided.
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
impl Counter {
  fn value(self) -> I32 { self.count }       // immutable self
  fn increment(mut self) -> I32 { self.count } // mutable self
  fn destroy(sink self) -> I32 { self.count }  // consuming self
}
```

Call sites are transparent: no extra syntax required:

```formalang
let mut n: I32 = 0
let result = bump(n)   // n is let mut, so it satisfies mut convention
```

Closure parameters carry the same conventions; the convention constrains
the **caller of the closure**: see [Closures](closures.md) for details.

## Function Overloading

Multiple functions with the same name are allowed when their signatures differ.
The compiler selects the right overload at each call site.

**Mode A: named-argument label set match** (exact label set determines the overload):

```formalang
fn format(value: I32) -> String { "number" }
fn format(value: String) -> String { "string" }
fn format(value: I32, precision: I32) -> String { "precise" }
```

**Mode B: first-positional-arg type match** (when call has no labels):

```formalang
fn process(I32) -> String { "number" }
fn process(String) -> String { "string" }
```

**Rules**:

- Overloads are distinguished by their named-argument label sets
- Calling with an ambiguous or unknown label set is a compile error
- An unresolvable call site produces `AmbiguousCall` or `NoMatchingOverload`

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
