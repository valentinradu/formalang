# Functions

Top-level functions, parameter conventions, codegen attributes, and
overloading. Closure expressions live on a separate page —
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

## Codegen Attributes

Three optional keyword prefixes hint to backends about call-site
behavior. They are pure metadata — the frontend passes them through
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
| `mut`      | `mut x: T`        | Exclusive mutable. Callee may mutate `x`.        |
| `sink`     | `sink x: T`       | Ownership transfer. Caller gives up the value.   |

```formalang
// Default — immutable parameter
fn read(x: I32) -> I32 {
  x
}

// mut — callee may mutate; argument must be let mut at call site
fn bump(mut score: I32) -> I32 {
  score
}

// sink — callee owns the value; caller cannot use it after
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

Call sites are transparent — no extra syntax required:

```formalang
let mut n: I32 = 0
let result = bump(n)   // n is let mut, so it satisfies mut convention
```

Closure parameters carry the same conventions; the convention constrains
the **caller of the closure** — see [Closures](closures.md) for details.

## Function Overloading

Multiple functions with the same name are allowed when their signatures differ.
The compiler selects the right overload at each call site.

**Mode A — named-argument label set match** (exact label set determines the overload):

```formalang
fn format(value: I32) -> String { "number" }
fn format(value: String) -> String { "string" }
fn format(value: I32, precision: I32) -> String { "precise" }
```

**Mode B — first-positional-arg type match** (when call has no labels):

```formalang
fn process(I32) -> String { "number" }
fn process(String) -> String { "string" }
```

**Rules**:

- Overloads are distinguished by their named-argument label sets
- Calling with an ambiguous or unknown label set is a compile error
- An unresolvable call site produces `AmbiguousCall` or `NoMatchingOverload`
