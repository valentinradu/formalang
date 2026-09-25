# Enums

Enums define sum types (tagged unions): a value is exactly one of the
declared variants.

## Definitions

A comma separates two variants. A line break after the comma is
optional. A line break alone does not separate two variants: an enum
with one variant on each line needs a comma at the end of each line
but the last.

```formalang
// Simple enum
pub enum Status {
  pending,
  active,
  completed
}

// With associated data (named parameters)
pub enum Message {
  text(content: String),
  image(url: String, size: I32),
  video(url: String, duration: I32)
}

// Generic enum
pub enum Result<T, E> {
  ok(value: T),
  error(err: E)
}

pub enum Option<T> {
  some(value: T),
  none
}
```

## Instantiation

Enum values use the leading-dot shorthand `.variant`. The type that
the context gives, here the type of the `let`, tells which enum the
variant belongs to:

```formalang
pub enum Status {
  pending,
  active
}

pub enum Message {
  text(content: String),
  image(url: String, size: I32)
}

pub enum Result<T, E> {
  ok(value: T),
  error(err: E)
}

// Simple variant
let status1: Status = .pending
let status2: Status = .active

// With named parameters
let msg1: Message = .text(content: "Hello")
let msg2: Message = .image(url: "/pic.jpg", size: 1024)

// Generic enum
let result1: Result<String, I32> = .ok(value: "success")
let result2: Result<String, I32> = .error(err: 404)
```

The full form names the enum: `Status.pending`. A generic enum takes
its type arguments on the path, the same way a struct literal does:
`Result<String, I32>.ok(value: "done")`. The full form needs no
context:

```formalang
pub enum Status {
  pending,
  active
}

pub enum Maybe<T> {
  some(value: T),
  none
}

pub let first = Status.pending
pub let empty = Maybe<I32>.none
pub let held = Maybe<I32>.some(value: 3)

pub fn run_checks() {
  assert(condition: first == Status.pending)
  assert(condition: empty != held)
}
```

The compiler checks the dot form against the enum that the context
expects:

- A dot form with no expected enum, such as `let v = .pending`, is the
  error `CannotInferEnumType`. Write the annotation or the full form.
- A variant that the expected enum does not have is the error
  `UnknownEnumVariant`, also when another enum has the variant.
- Each payload field is checked against its declared type. A missing
  payload field is the error `MissingField`, an unknown one is the
  error `UnknownField`, and a value of the wrong type is a
  `TypeMismatch`.
- A variant with data needs its data: `.image` alone is the error
  `EnumVariantRequiresData`. A variant without data takes none:
  `.pending(x: 1)` is the error `EnumVariantWithoutData`.

```formalang,reject=UnknownEnumVariant
pub enum Status { pending, active }
pub enum Level { low, high }

pub let s: Status = .low       // error: 'low' is a variant of Level
```

## Pattern Matching

To consume enum values, use a `match` expression: see
[Control Flow & Pattern Matching](control-flow.md#match-expressions). A
destructuring pattern cannot take an enum value: see
[Expressions / Destructuring](expressions.md#destructuring).
