# Control Flow & Pattern Matching

All control flow is **compile-time validated**. Each form is an
expression: it evaluates to a value.

## For Expressions

Iterate over arrays:

```formalang
// Basic for loop
for item in items {
  process(item: item)
}

// With field access
for email in user.emails {
  validate(address: email)
}

// With literal array
for n in [1, 2, 3, 4, 5] {
  record(value: n)
}

// Nested loops
for row in matrix {
  for cell in row {
    process(value: cell)
  }
}
```

**Rules**:

- Expression must be an array type
- Returns array of body results
- Loop variable scoped to body

## If Expressions

Conditional expressions:

```formalang
// Boolean condition
if count > 0 {
  showItems()
} else {
  showEmpty()
}

// Without else (returns nil if false)
if isAdmin {
  showAdminPanel()
}

// Chained conditions
if x > 100 {
  showLarge()
} else if x > 50 {
  showMedium()
} else {
  showSmall()
}
```

**Optionals in conditionals (`if let`)**:

To consume an optional value, use `if let` to bind the inner value
inside the truthy branch. The form is Rust-style: pattern, equals,
optional expression, then both branches.

```formalang
if let nickname = user.nickname {
  // nickname is bound to the unwrapped String here
  greet(name: nickname)
} else {
  // taken when user.nickname is nil
  greet(name: user.name)
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

match status {
  .pending: waitFor(),
  .active: runNow(),
  .completed: finalize()
}

// With data binding (named parameters)
pub enum Message {
  text(content: String)
  image(url: String, size: I32)
}

match message {
  .text(content): displayText(value: content),
  .image(url, size): displayImage(src: url, bytes: size)
}
```

**Rules**:

- Must be exhaustive (cover all variants)
- Pattern uses `.variant` syntax (short form)
- Associated data bound to identifiers using parameter names
