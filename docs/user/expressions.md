# Expressions

This page covers value-producing expressions: literals, field access,
destructuring, operators, and the range operator. For function-call
shapes see [Functions](functions.md); for closure expressions see
[Closures](closures.md); for control flow (`if` / `for` / `match`) see
[Control Flow & Pattern Matching](control-flow.md).

## Literals

All literal types as expressions:

```formalang
// String literals
let text = "Hello, World"
let multiline = """
  Multi-line
  string literal
"""

// Numeric literals (see Numeric Types for suffixes and defaults)
let integer = 42                     // I32
let negative = -17                   // I32
let float = 3.14                     // F64
let with_underscore = 1_000_000      // I32
let wide: I64 = 9_223_372_036_854_775_807
let tagged = 3.14F32                 // F32 via suffix

// Boolean literals
let yes = true
let no = false

// Nil literal
let nothing: String? = nil

// Array literals
let tags = ["urgent", "bug", "frontend"]
let numbers = [1, 2, 3, 4, 5]
let empty: [String] = []

// Dictionary literals
let settings: [String: I32] = ["timeout": 30, "maxRetries": 3]
let emptyDict: [String: Boolean] = [:]
```

An unsuffixed number takes its type from the position where it stands.
The comments above give the type where no position gives one. See
[Type System / The type of a literal](types.md#the-type-of-a-literal).

A plain string stays on one line: a line break in it is the error
`UnterminatedString`. A `"""` string can hold line breaks.

**Escape sequences** (strings): `\"`, `\\`, `\n`, `\t`, `\r`, `\uXXXX`.
The Unicode escape takes exactly four hex digits, and it must name a
Unicode scalar value. A backslash before any other character is the
error `InvalidEscape`. A wrong `\u` escape is the error
`InvalidUnicodeEscape`: fewer than four digits, a character that is not
a hex digit, a surrogate such as `\uD800`, or the brace form `\u{41}`.

```formalang
pub fn run_checks() {
  assert(condition: "\u0041" == "A")
  assert(condition: "\u00E9".len() == 2)   // two bytes of UTF-8
  assert(condition: "a\tb".len() == 3)     // one byte for each escape
}
```

## Field Access

A `.` reads a field of a struct or a tuple. The reads chain:

```formalang
pub struct Profile { avatar: String }
pub struct User { name: String, profile: Profile }

pub fn run_checks() {
  let user = User(name: "Ada", profile: Profile(avatar: "ada.png"))
  let point = (x: 1, y: 2)

  assert(condition: user.name == "Ada")                 // a field
  assert(condition: point.x == 1)                       // a tuple field
  assert(condition: user.profile.avatar == "ada.png")   // nested access
}
```

A field that the type does not have is the error `UnknownField`. An
enum, a number, an array and a closure have no fields.

## Destructuring

Extract values from arrays, tuples and structs:

```formalang
// Array destructuring (positional)
pub let items = ["first", "second", "third", "fourth"]
pub let [a, b] = items              // a="first", b="second"
pub let [x, ...rest] = items        // x="first", rest=["second", "third", "fourth"]
pub let [_, second, ...] = items    // Skip first, get second, ignore rest

// Tuple destructuring (by position)
pub let pair = (x: 3, y: 4)
pub let (px, py) = pair             // px=3, py=4

// Struct destructuring (by field name)
pub struct User { name: String, age: I32 }
pub let user = User(name: "Alice", age: 30)
pub let {name, age} = user          // name="Alice", age=30
pub let {name as username} = user   // Rename: username="Alice"

// Enum data: use `match`, not a destructuring pattern
pub enum AccountType {
  admin,
  user(permissions: [String], articles: [String])
}

pub fn article_count(account: AccountType) -> I32 {
  match account {
    .user(permissions, articles): articles.len(),
    .admin: 0
  }
}
```

An enum value is one of several variants, and each variant has its own
data, or none. So a destructuring pattern cannot take an enum value. To
read the data of a variant, use `match`, or `if let` for an optional.
A tuple pattern on an enum value is a type mismatch:

```formalang,reject=TypeMismatch
pub enum AccountType {
  admin,
  user(permissions: [String], articles: [String])
}

pub let account: AccountType = .admin
pub let (permissions, articles) = account   // error: not a tuple
```

**Rules**:

- Array destructuring is positional (order matters)
- Tuple destructuring is positional too, and the pattern needs one
  name for each field of the tuple
- Struct destructuring is by field name. A name that is not a field is
  the error `UnknownField`
- An array pattern on another type is the error
  `ArrayDestructuringNotArray`. A struct pattern on another type, a
  tuple included, is the error `StructDestructuringNotStruct`
- An enum value cannot be destructured: use `match`
- Use `as` to rename fields during destructuring
- Use `_` to skip array elements
- Use `...` for rest pattern (can appear anywhere in array destructuring)
- Dictionaries do not support destructuring

## Binary Operators

```formalang
// Arithmetic
let sum = 10 + 20
let difference = 50 - 30
let product = 4 * 5
let quotient = 100 / 4
let remainder = 17 % 5

// Comparison
let greater = 10 > 5
let less = 3 < 7
let greaterEq = 10 >= 10
let lessEq = 5 <= 5

// Equality
let equal = 5 == 5
let notEqual = 5 != 10

// Logical
let andResult = true && false
let orResult = true || false

// String concatenation
let greeting = "Hello, " + "World"

// Complex expressions with precedence
let complex = (10 + 20) * 3
let condition = (5 > 3) && (10 < 20)
```

## Unary Operators

`-` negates a number, and `!` negates a `Boolean`. The operand is
checked: `-"text"` and `-true` are a `TypeMismatch`, and so is `!1`.

```formalang
pub fn run_checks() {
  let n: I32 = 5
  let s = "abc"
  assert(condition: -n == 0 - 5)
  assert(condition: !(n > 10))
  assert(condition: -s.len() == -3)     // `.` binds before `-`
}
```

## Operator Precedence

From highest to lowest:

1. **Parentheses**: `( )`
2. **Field access, call and index**: `.`, `f(...)`, `xs[i]`
3. **Unary**: `-`, `!`
4. **Multiplicative**: `*`, `/`, `%`
5. **Additive**: `+`, `-`
6. **Comparison**: `<`, `>`, `<=`, `>=`
7. **Equality**: `==`, `!=`
8. **Logical AND**: `&&`
9. **Logical OR**: `||`
10. **Range**: `..`

A comparison does not chain: `1 < 2 < 3` compares a `Boolean` with a
number, which is an `InvalidBinaryOp`.

Examples:

```formalang
pub struct User { age: I32, verified: Boolean }

pub fn run_checks() {
  let x = 7
  let y = 3
  let user = User(age: 30, verified: true)

  assert(condition: 10 + 20 * 3 == 70)            // multiplication first
  assert(condition: (10 + 20) * 3 == 90)          // parentheses override
  assert(condition: x > 5 && y < 10)              // comparison before AND
  assert(condition: true || false && false)       // AND before OR
  assert(condition: user.age > 18 && user.verified)  // field access, comparison, AND
  assert(condition: -2 * 3 == -6)                 // unary before multiplication
}
```

### Operand Types

Each operator group accepts a limited set of operands. There is no
implicit conversion between types, so the two operands must have the
same type. An unsuffixed numeric literal takes its type from the other
operand: in `n + 1` with `n: I64`, the `1` is an `I64`. See
[the type of a literal](types.md#the-type-of-a-literal).

| Group | Operands |
| --- | --- |
| `+` | two numbers of one type, or two strings (concatenation) |
| `-`, `*`, `/`, `%` | two numbers of one type |
| `<`, `>`, `<=`, `>=` | two numbers of one type |
| `==`, `!=` | two values of one type, if the type is equatable |
| `&&`, `\|\|` | two booleans |
| `..` | two integers (`I32` or `I64`) |

An optional compares to `nil`, which is the plainest way to ask
whether it holds anything. Either side may be the `nil`:

```formalang
pub fn run_checks() {
  let held: I32? = 5
  let empty: I32? = nil

  assert(condition: !(held == nil))
  assert(condition: empty == nil)
  assert(condition: nil == empty)       // the same question
  assert(condition: held != nil)
}
```

This answers exactly what `.is_some()` and `.is_none()` answer; pick
whichever reads better where it sits.

Two optionals of one type compare too: they are equal when both are
`nil`, or when both hold equal values. An optional and a plain value do
not compare, because they are two types: `held == 5` is an
`InvalidBinaryOp`. Unwrap the optional with `if let` first. The
language has no `??` operator and no `?.` chaining.

Equality is structural: it compares each field of a struct and each
element of a container. A closure has no structure to compare, so a
closure is not equatable. Neither is a type that holds one — an array
of closures, or a struct with a closure field.

## Indexing

Both array indexing (`xs[i]`) and dictionary lookup (`d[k]`) return an
optional value (`T?`). The bound may be missing for an array index and
the key may be absent from a dictionary, so the result is wrapped in an
optional and the caller is responsible for handling the `nil` case.

```formalang
pub fn run_checks() {
  let xs: [I32] = [1, 2, 3]
  let first: I32? = xs[0]      // I32?, not I32

  let cfg: [String: I32] = ["timeout": 30]
  let t: I32? = cfg["timeout"] // same shape

  // Unwrap with `if let`, and give the value for the nil case
  let timeout: I32 = if let v = t { v } else { 60 }
  assert(condition: timeout == 30)
  assert(condition: first != nil)
  assert(condition: xs[3] == nil)
}
```

To get a plain value, unwrap the optional with `if let` and give a
fallback for the `nil` case, or `match` on `.some` and `.none`. See
[Control Flow](control-flow.md#if-expressions).

The type of the index is checked: an array takes an `I32` position,
and a dictionary takes a key of its key type. See
[Type System / Indexing](types.md#indexing).

An index reads; it never writes. `xs[0] = 9` is an error, and `let mut`
does not make it legal. See
[Type System / Immutable Elements](types.md#immutable-elements).

## Range Operator

The `..` operator produces a range from a start value (inclusive) to an end
value (exclusive). It is the lowest-precedence binary operator, so its
operands are evaluated before the range itself.

A range counts in steps of one, so both bounds must be integers
(`I32` or `I64`). To iterate floats, put them in an array.

```formalang
pub fn run_checks() {
  let n = 4
  let start = 2
  let length = 3

  // A simple range
  let digits = 0..10

  // Iterating over a range. A `for` yields a lazy sequence, so a
  // terminal combinator ends the pipeline.
  let count: I32 = for i in 0..n { i }.count()

  // Range with arithmetic on the bounds
  let window = start..(start + length)

  assert(condition: digits.len() == 10)
  assert(condition: count == 4)
  assert(condition: for i in window { i }.fold(initial: 0, f: (a, b) -> a + b) == 9)
}
```
