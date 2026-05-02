# Enums

Enums define sum types (tagged unions) — a value is exactly one of the
declared variants.

## Definitions

```formalang
// Simple enum
pub enum Status {
  pending
  active
  completed
}

// With associated data (named parameters)
pub enum Message {
  text(content: String)
  image(url: String, size: I32)
  video(url: String, duration: I32)
}

// Generic enum
pub enum Result<T, E> {
  ok(value: T)
  error(err: E)
}

pub enum Option<T> {
  some(value: T)
  none
}
```

## Instantiation

Enum values use the leading-dot shorthand `.variant`:

```formalang
// Simple variant
let status1: Status = .pending
let status2: Status = .active

// With named parameters
let msg1: Message = .text(content: "Hello")
let msg2: Message = .image(url: /pic.jpg, size: 1024)

// Generic enum
let result1: Result<String, I32> = .ok(value: "success")
let result2: Result<String, I32> = .error(err: 404)
```

## Pattern Matching

To consume enum values, use a `match` expression — see
[Control Flow & Pattern Matching](control-flow.md#match-expressions). To
extract associated data without matching, use enum destructuring — see
[Expressions / Destructuring](expressions.md#destructuring).
