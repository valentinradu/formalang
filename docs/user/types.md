# Type System

## Primitive Types

```formalang
pub struct Primitives {
  text: String,           // Text data
  count: I32,             // 32-bit signed integer
  amount: F64,            // 64-bit IEEE 754 float
  active: Boolean         // true or false
}
```

## Numeric Types

FormaLang has four width-tagged numeric primitives instead of a single
generic `Number` type. Backends emit native instructions directly without
guessing precision.

| Type  | Range / shape                        |
| ----- | ------------------------------------ |
| `I32` | 32-bit signed integer                |
| `I64` | 64-bit signed integer                |
| `F32` | 32-bit IEEE 754 float                |
| `F64` | 64-bit IEEE 754 float                |

### The type of a literal

A literal with no suffix takes its type from the position where it
stands. An annotation, a parameter, a field, a variant field, a return
type and a container element each give a type. Where no type is given,
an integer literal is an `I32` and a literal with a fraction or an
exponent is an `F64`. A suffix in uppercase gives the type at the
literal itself:

```formalang
pub let a = 42                          // I32: no type is given
pub let b = 42I64                       // I64: the suffix
pub let c = 3.14                        // F64: no type is given
pub let d = 3.14F32                     // F32: the suffix

pub let big: I64 = 9_223_372_036_854_775_807   // the annotation gives I64
pub let tiny: F32 = 0.5                 // the annotation gives F32
pub let ids: [I64] = [1, 2, 3]          // each element is an I64

pub fn twice(n: I64) -> I64 { n * 2I64 }
pub let four: I64 = twice(n: 2)         // the parameter gives I64
```

An operand of a binary operator takes its type from the other operand.
When both operands are literals, an arithmetic operator (`+`, `-`,
`*`, `/`, `%`) gives them the type of its own position:

```formalang
pub fn next(n: I64) -> I64 {
  n + 1            // 1 is an I64: the other operand gives the type
}

pub let product: I64 = 2 * 3          // 2 and 3 are I64: the annotation
pub let wide: Boolean = 5 < 3_000_000_000I64   // 5 is an I64

pub fn run_checks() {
  assert(condition: next(n: 1) == 2)
  assert(condition: product == 6)
}
```

Two operands that each have a type must have the same type. The
operator converts neither of them:

```formalang,reject=InvalidBinaryOp
pub fn mixed(a: I32, b: I64) -> I64 {
  a + b            // error: I32 and I64 do not mix
}
```

An integer literal never becomes a float: `let f: F64 = 1` is a type
mismatch. Write `1.0`.

### The range of a literal

The compiler checks each literal against its type:

- An integer literal that does not fit is the error `NumericOverflow`.
  The lowest value of a type fits: `-2147483648` is an `I32`, and
  `-9223372036854775808` is an `I64` where the position gives `I64`.
- A float literal that does not fit is the error `InvalidNumber`. An
  `F32` literal must fit the `F32` range, so `3.5e38` is an error where
  the type is `F32`.
- A literal with a fraction cannot take an integer suffix: `3.5I32` is
  the error `InvalidNumber`.

```formalang,reject=NumericOverflow
pub let lowest: I32 = -2147483648      // ok
pub let too_big: I32 = 2147483648      // error: does not fit in I32
```

### Overflow at run time

An integer operation whose result does not fit its type overflows.
The language does not say what the result is: the backend decides.
Integer division and remainder truncate toward zero. The
`ConstantFoldingPass` computes each operation in the declared type. It
leaves an operation that overflows, or that divides by zero, as it is,
so the backend makes the same decision for it.

## Never Type

`Never` is an uninhabited type: it has no values and cannot be instantiated.
It is used as a return type for functions that diverge (infinite loops, panics):

```formalang
extern fn abort() -> Never
```

## Array Types

Arrays hold multiple values of the same type:

```formalang
pub struct User { name: String }

pub struct Collections {
  names: [String],             // Variable-length array of strings
  scores: [I32],               // Variable-length array of integers
  flags: [Boolean],            // Variable-length array of booleans
  matrix: [[I32]],             // Nested arrays
  users: [User]                // Array of custom types
}

// Array literals
pub let tags = ["urgent", "bug", "frontend"]
pub let numbers = [1, 2, 3, 4, 5]
pub let empty: [String] = []

// Array destructuring (see Expressions for full rules)
pub let [first, second] = ["a", "b", "c"]
pub let [user, ...] = ["John", "pass", "etc"]

pub fn run_checks() {
  assert(condition: numbers.len() == 5)
  assert(condition: second == "b")
  assert(condition: user == "John")
}
```

## Optional Types

Optional types can be a value or `nil`:

```formalang
pub struct User {
  name: String,
  email: String,
  nickname: String?,            // Optional field
  avatar: String?               // Optional field
}

pub let user1 = User(
  name: "Alice",
  email: "alice@example.com",
  nickname: "Ally",             // Provide a value
  avatar: nil                   // Explicitly nil
)
```

`Optional<T>` behaves like a built-in two-variant enum with `.some(T)`
and `.none`. To consume the inner value, use the Rust-style `if let`
form documented in [Control Flow](control-flow.md), or `match` against
`.some(x)` / `.none` arms directly. Both branches are required when
the result is used as a value, so the type-checker can unify them.

## Dictionary Types

Key-value mappings using bracket syntax with colon:

```formalang
pub struct User { id: I32, name: String }

pub struct AppConfig {
  settings: [String: I32],         // String keys to I32 values
  scores: [I32: String],           // I32 keys to String values
  cache: [String: User]            // String keys to custom types
}

// Dictionary literals (string keys must be quoted)
pub let settings: [String: I32] = ["timeout": 30, "maxRetries": 3]
pub let scores: [I32: String] = [100: "perfect", 95: "excellent"]
pub let empty: [String: Boolean] = [:]

// A dictionary made from data: see `collect` in Control Flow
pub fn by_id(users: [User]) -> [I32: User] {
  for u in users { u }.collect(key: (u) -> u.id, value: (u) -> u)
}

pub fn run_checks() {
  let found = by_id(users: [User(id: 7, name: "Ada")])
  assert(condition: found.len() == 1)
  assert(condition: settings["timeout"] != nil)
}
```

**Rules**:

- Keys can be `String`, `I32`, `I64`, `Boolean`, a struct, or an enum.
  A struct or an enum is a key only when each of its fields is a key
  type too
- Any other key type is the error E134 (`InvalidDictionaryKey`): a
  float, an optional, an array, a tuple, a dictionary, a closure, or a
  struct or enum that holds one of them
- `F32` and `F64` cannot be keys. A float has no usable
  equality: `NaN` is not equal to itself, so a key can never be found
  again, and `0.0` equals `-0.0`, so two distinct-looking keys collide.
  The rule also applies to a key type that the compiler infers: the
  keys of a literal, the `key` closure of `collect`, and a generic
  call
- A lookup `d[k]` needs a key of the key type. A key of another type,
  an optional key included, is a `TypeMismatch`
- A repeated key makes one entry. The later value replaces the earlier
  one: `["a": 1, "a": 2]` holds one entry, `"a": 2`
- String keys must be quoted in literals: `["key": value]`
- Numeric keys are unquoted: `[42: value]`
- Empty dict: `[:]`
- No destructuring support for dictionaries

## Tuples

Named tuples group related values with field names:

```formalang
pub struct Config {
  person: (name: String, age: I32),
  point: (x: I32, y: I32),
  nested: (user: (first: String, last: String), active: Boolean)
}

pub fn run_checks() {
  // Tuple literals
  let person = (name: "John", age: 30)
  let point = (x: 10, y: 20)
  let nested = (user: (first: "John", last: "Doe"), active: true)

  // Accessing tuple fields by name
  assert(condition: person.name == "John")
  assert(condition: point.x + point.y == 30)
  assert(condition: nested.user.last == "Doe")
}
```

**Rules**:

- Tuples use parentheses: `(name: value, ...)`
- All fields must be named (no positional tuples)
- Access fields with dot notation: `tuple.fieldName`
- Trailing comma allowed: `(x: 1, y: 2,)`
- Tuples can be nested

## Closure Types

Closure types define function signatures for callbacks and transformations.
The parameter list is **always parenthesised**, even for a single parameter,
so every `->` in the language is preceded by `)`. For closure
*expressions*, see [Closures](closures.md).

```formalang
struct Controls<E> {
  // No parameters - returns E
  onPress: () -> E,

  // Single parameter (default / let convention)
  onChange: (String) -> E,

  // Multiple parameters
  onResize: (I32, I32) -> E,

  // mut parameter: caller must pass a mutable binding
  onScale: (mut I32) -> E,

  // sink parameter: caller's binding is consumed (moved)
  onSubmit: (sink String) -> E,

  // Optional closure (can be nil)
  onFocus: ((String) -> E)?,

  // Closure returning optional
  validate: (String) -> Boolean?
}
```

**Type syntax**:

| Parameters        | Syntax                       | Example                          |
| ----------------- | ---------------------------- | -------------------------------- |
| None              | `() -> T`                    | `() -> Event`                    |
| One (default)     | `(T) -> U`                   | `(String) -> Event`              |
| One (mut)         | `(mut T) -> U`               | `(mut I32) -> Event`             |
| One (sink)        | `(sink T) -> U`              | `(sink String) -> Event`         |
| Multiple          | `(T, U) -> V`                | `(I32, I32) -> Point`            |
| Mixed conventions | `(mut T, sink U) -> V`       | `(mut I32, sink String) -> V`    |

**Rules**:

- The parameter list is always parenthesised — even with one parameter
  — and every `->` in the language is preceded by `)`.
- Multiple parameters are comma-separated inside the parens.
- Convention keywords (`mut`, `sink`) precede the type in the type
  position.

## Generic Types

Types parameterized with type variables (full details in [Generics](generics.md)):

```formalang
pub trait Layout { width: I32 }
pub trait Render { fn render(self) -> Boolean }
pub trait Click { fn click(self) -> Boolean }

pub struct Box<T> { value: T }                        // Single type parameter
pub struct Pair<A, B> { first: A, second: B }         // Multiple type parameters
pub struct Container<T: Layout> { items: [T] }        // With trait constraint
pub struct Widget<T: Render + Click> { component: T } // Multiple trait constraints

pub enum Result<T, E> { ok(value: T), error(err: E) }
pub let answer: Result<String, I32> = .ok(value: "yes") // Instantiated generic
```

## Indexing

Three types take an index: an array by position, a dictionary by key,
and a string by byte offset. Nothing else does.

```formalang
pub fn run_checks() {
  let xs = [10, 20]
  let a: I32? = xs[1]      // the position may be out of range
  let d = ["k": 5]
  let b: I32? = d["k"]     // the key may be absent
  let c: I32 = "abc"[0]    // the byte at that offset

  assert(condition: if let v = a { v == 20 } else { false })
  assert(condition: xs[2] == nil)
  assert(condition: if let v = b { v == 5 } else { false })
  assert(condition: c == 97)
}
```

Any other type is an error:

```formalang,reject=NotIndexable
pub struct Point { x: I32 }

pub fn f() -> I32 {
  let p = Point(x: 1)
  p[0]                     // error E139: a struct has no index operation
}
```

The type of the index is checked too. An array and a string take an
`I32` position, and a dictionary takes a key of its key type. Any other
index type is a `TypeMismatch`.

An array index and a dictionary lookup both produce an optional,
because the position may be out of range and the key may be absent.
See [Expressions / Indexing](expressions.md#indexing).

## Immutable Elements

The three types that take an index never change after you make them.
The language has no write through an index. One rule covers all three:

```formalang,reject=AssignmentToElement
pub fn f() {
  let mut xs: [I32] = [1, 2, 3]
  xs[0] = 9                  // error E143: cannot assign to an element

  let mut d: [String: I32] = ["a": 1]
  d["a"] = 9                 // error E143

  let mut s: String = "ab"
  s[0] = 65                  // error E143
}
```

`let mut` does not change this. It lets you assign the binding, and it
says nothing about the contents. A field above the index makes no
difference either: `h.xs[0] = 9` reports the same error, because the
write still passes through the index.

Three kinds of write are legal. You assign a whole binding, you assign
a struct field, and you pass a `mut` parameter:

```formalang
pub struct Point { x: I32 }

pub fn run_checks() {
  let mut xs: [I32] = [1, 2]
  xs = [3, 4, 5]             // ok: the binding takes a new array

  let mut p: Point = Point(x: 1)
  p.x = 9                    // ok: a field, through a mut binding

  assert(condition: xs.len() == 3)
  assert(condition: p.x == 9)
}
```

To change the elements, make a new value. A `for` pipeline builds one
in a single pass:

```formalang
pub fn run_checks() {
  let mut xs: [I32] = [1, 2, 3]
  xs = for x in xs { x * 2 }.collect()
  assert(condition: for x in xs { x }.fold(initial: 0, f: (a, b) -> a + b) == 12)
}
```

A host can still offer mutation. The signature shows it, and the
runtime does the work:

```formalang
extern fn push(mut xs: [I32], value: I32)
```

### Why the elements are immutable

Three properties depend on the rule.

- **A string slice is zero-copy.** `slice` shares the memory of the
  source. A write to a string would therefore reach every slice that
  the source made.
- **The language uses Mutable Value Semantics.** An element write makes
  `let ys = xs` need a copy. The copy would happen where nobody asked
  for it. See [Functions](functions.md).
- **A `for` yields a lazy sequence.** Nothing runs until a terminal
  combinator consumes it. If the source could change in between, the
  result of a pipeline would depend on evaluation order. See
  [Large Data](large-data.md).

## Optional Elements

A value wraps into an optional wherever one is declared, and that
reaches inside a container: `[T]` fits `[T?]` the same way `T` fits
`T?`.

Whether the optional is worth declaring is a separate question. A
plain `let` has no later, so if every element of the literal is
present, the optional says more than the value means:

```formalang,reject=PointlessOptionalElement
let a: [I32?] = [nil, 3]      // ok: one element really is absent
let mut b: [I32?] = [3]       // ok: a nil may be put there later
let c: [I32?] = [3]           // error E142: declare it [I32]
let d: [I32?] = []            // ok: an empty literal claims nothing
```

The same holds for a dictionary's values.
