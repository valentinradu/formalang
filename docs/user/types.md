# Type System

## Primitive Types

```formalang
pub struct Primitives {
  text: String,           // Text data
  count: I32,             // 32-bit signed integer
  amount: F64,            // 64-bit IEEE 754 float
  active: Boolean,        // true or false
  logo: Path,             // File/resource paths
  pattern: Regex          // Regular expressions
}
```

## Numeric Types

FormaLang has four width-tagged numeric primitives instead of a single
generic `Number` type. Backends emit native instructions directly without
guessing precision.

| Type  | Range / shape                        |
| ----- | ------------------------------------ |
| `I32` | 32-bit signed integer (default for unsuffixed integer literals) |
| `I64` | 64-bit signed integer                |
| `F32` | 32-bit IEEE 754 float                |
| `F64` | 64-bit IEEE 754 float (default for unsuffixed float literals) |

Numeric literals can carry an uppercase suffix to pin the type at the
literal site:

```formalang
let a = 42        // I32 (integer-syntax default)
let b = 42I64     // I64
let c = 3.14      // F64 (float-syntax default)
let d = 3.14F32   // F32

let big: I64 = 9_223_372_036_854_775_807
let tiny: F32 = 0.5F32
```

Suffix range checks happen at compile time; literals that don't fit
their declared / suffixed type are a compile error.

## Never Type

`Never` is an uninhabited type — it has no values and cannot be instantiated.
It is used as a return type for functions that diverge (infinite loops, panics):

```formalang
extern fn abort() -> Never
```

## Array Types

Arrays hold multiple values of the same type:

```formalang
pub struct Collections {
  names: [String],             // Variable-length array of strings
  scores: [I32],               // Variable-length array of integers
  flags: [Boolean],            // Variable-length array of booleans
  matrix: [[I32]],             // Nested arrays
  users: [User],               // Array of custom types
}

// Array literals
pub let tags = ["urgent", "bug", "frontend"]
pub let numbers = [1, 2, 3, 4, 5]
pub let empty = []

// Array destructuring (see Expressions for full rules)
pub let [first, second] = ["a", "b", "c"]
pub let [user, ...] = ["John", "pass", "etc"]
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

## Dictionary Types

Key-value mappings using bracket syntax with colon:

```formalang
pub struct AppConfig {
  settings: [String: I32],         // String keys to I32 values
  scores: [I32: String],           // I32 keys to String values
  cache: [String: User],           // String keys to custom types
  assets: [Path: String]           // Path keys to String values
}

// Dictionary literals (string keys must be quoted)
pub let settings: [String: I32] = ["timeout": 30, "maxRetries": 3]
pub let scores: [I32: String] = [100: "perfect", 95: "excellent"]
pub let assets: [Path: String] = [/logo.svg: "icon", /bg.png: "background"]
pub let empty: [String: Boolean] = [:]
```

**Rules**:

- Keys can be any compiler-supported type (String, I32, Path, enum, etc.)
- String keys must be quoted in literals: `["key": value]`
- Numeric keys are unquoted: `[42: value]`
- Path keys use path literal syntax: `[/path: value]`
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

// Tuple literals
for item in items {
  let person = (name: "John", age: 30)
  let point = (x: 10, y: 20)
  let nested = (user: (first: "John", last: "Doe"), active: true)
}

// Accessing tuple fields
for item in items {
  let person = (name: "John", age: 30)
  let name = person.name      // Access by field name
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
For closure *expressions*, see [Closures](closures.md).

```formalang
pub struct Controls<E> {
  // No parameters - returns E
  onPress: () -> E,

  // Single parameter (default / let convention)
  onChange: String -> E,

  // Multiple parameters (comma-separated, no parens needed)
  onResize: I32, I32 -> E,

  // mut parameter — caller must pass a mutable binding
  onScale: mut I32 -> E,

  // sink parameter — caller's binding is consumed (moved)
  onSubmit: sink String -> E,

  // Optional closure (can be nil)
  onFocus: (String -> E)?,

  // Closure returning optional
  validate: String -> Boolean?
}
```

**Type syntax**:

| Parameters           | Syntax              | Example                        |
| -------------------- | ------------------- | ------------------------------ |
| None                 | `() -> T`           | `() -> Event`                  |
| One (default)        | `T -> U`            | `String -> Event`              |
| One (mut)            | `mut T -> U`        | `mut I32 -> Event`             |
| One (sink)           | `sink T -> U`       | `sink String -> Event`         |
| Multiple             | `T, U -> V`         | `I32, I32 -> Point`            |
| Mixed conventions    | `mut T, sink U -> V`| `mut I32, sink String -> V`    |

**Rules**:

- Arrow `->` separates parameters from return type
- Multiple parameters are comma-separated (no parentheses required)
- Empty parameters require parentheses: `() -> T`
- Convention keywords (`mut`, `sink`) precede the type in the type position
- Parser uses `->` to determine grouping in ambiguous contexts

## Generic Types

Types parameterized with type variables (full details in [Generics](generics.md)):

```formalang
Box<T>                      // Single type parameter
Pair<A, B>                  // Multiple type parameters
Container<T: Layout>        // With trait constraint
Widget<T: Render + Click>   // Multiple trait constraints
Result<String, I32>         // Instantiated generic
```
