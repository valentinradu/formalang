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

// Path literals
let logo = /assets/logo.svg

// Regex literals
let pattern = r/[a-z]+/i
```

**Escape sequences** (strings): `\"`, `\\`, `\n`, `\t`, `\r`, `\uXXXX`

**Regex flags**: `g`, `i`, `m`, `s`, `u`, `v`, `y`

## Field Access

```formalang
user.name                   // Access field
point.x                     // Access coordinate
config.timeout              // Access config field
user.profile.avatar         // Nested access
theme.colors.primary        // Multiple levels
```

## Destructuring

Extract values from arrays, structs, and enums:

```formalang
// Array destructuring (positional)
pub let items = ["first", "second", "third", "fourth"]
pub let [a, b] = items              // a="first", b="second"
pub let [x, ...rest] = items        // x="first", rest=["second", "third", "fourth"]
pub let [_, second, ...] = items    // Skip first, get second, ignore rest

// Struct destructuring (by field name)
pub struct User { name: String, age: I32 }
pub let user = User(name: "Alice", age: 30)
pub let {name, age} = user          // name="Alice", age=30
pub let {name as username} = user   // Rename: username="Alice"

// Enum destructuring (extract associated data)
pub enum AccountType {
  admin
  user(permissions: [String], articles: [String])
}

pub let account: AccountType = .user(
  permissions: ["read", "write"],
  articles: ["article1", "article2"]
)

// Destructure enum to extract associated data
pub let (permissions, articles) = account
```

**Rules**:

- Array destructuring is positional (order matters)
- Struct destructuring is by field name
- Enum destructuring extracts associated data in parameter order
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

## Operator Precedence

From highest to lowest:

1. **Parentheses**: `( )`
2. **Field access**: `.`
3. **Multiplicative**: `*`, `/`, `%`
4. **Additive**: `+`, `-`
5. **Comparison**: `<`, `>`, `<=`, `>=`
6. **Equality**: `==`, `!=`
7. **Logical AND**: `&&`
8. **Logical OR**: `||`
9. **Range**: `..`

Examples:

```formalang
10 + 20 * 3              // 70 (multiplication first)
(10 + 20) * 3            // 90 (parentheses override)
x > 5 && y < 10          // Comparison before AND
true || false && false   // true (AND before OR)
user.age > 18 && user.verified  // Field access → comparison → AND
```

## Range Operator

The `..` operator produces a range from a start value (inclusive) to an end
value (exclusive). It is the lowest-precedence binary operator, so its
operands are evaluated before the range itself.

```formalang
// A simple range
let digits = 0..10

// Iterating over a range in a for-expression
for i in 0..n {
    process(i)
}

// Range with arithmetic on the bounds
let window = start..(start + length)
```
