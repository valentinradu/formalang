# Large Data

FormaLang is built for programs where a loop over a million elements
is the normal case, not the exception. Two properties make that work,
and both come from the same decision: **a `for` yields a lazy
sequence, not an array**.

- A pipeline holds memory that does **not grow with the input**.
- A pipeline makes **one pass** over the data.

## Why a loop does not allocate

A `for` produces `Seq<T>`. A sequence holds no elements. Nothing runs
until a terminal combinator consumes it, and consuming it drives the
whole chain in a single pass.

```formalang
// One pass, no allocation. The intermediate stages never exist.
let total: I32 = for x in xs { x * 2 }
  .filter(f: (v) -> v > 10)
  .fold(initial: 0, f: (a, b) -> a + b)
```

Had a loop produced an array, the same three stages over one million
`I32` values would allocate three arrays and walk memory three times,
to produce one number.

`collect()` is the one call in a pipeline that allocates for the data,
and it is visible on the line that asks for it:

```formalang
let doubled: [I32] = for x in xs { x * 2 }.collect()
```

## Where the data lives

For real volumes the data belongs to the host, not to the program. It
never has to be copied in.

**Pull on the way in.** An `extern fn` that returns a sequence is a
cursor. The host produces the elements and the generated code pulls
them one at a time, so the host's million rows stay in the host's
memory:

```formalang
pub struct Row { id: I32, score: I32 }

extern fn rows() -> Seq<Row>
```

**Push on the way out.** A program does not hand a sequence back. It
calls a hook once per element:

```formalang
extern fn emit(id: I32)
```

Put together, a program that filters ten million rows holds a working
set of one row:

```formalang
pub fn process() {
  for row in rows() { row }
    .filter(f: (r) -> r.score > 0)
    .run()
}
```

## The rules that keep it honest

A sequence is consumed **exactly once**. Both halves of that rule earn
their place.

**At most once** is what makes the single pass a guarantee rather than
a hope. A sequence has at most one reader, so there is never a second
consumer to build an array for, and the compiler never has to decide
whether joining two stages is safe. There is no case where a pipeline
quietly allocates.

**At least once** is what stops a loop that never runs from looking
like work:

```formalang
for x in xs { emit(id: x) }        // E135: the calls never happen
```

Add `.run()` and the intent is on the page.

## Choosing the terminal

| You want | Use | Allocates |
| --- | --- | --- |
| one value | `fold`, `count`, `first`, `any`, `all` | no |
| the effects only | `run` | no |
| an array | `collect` | yes, once |

Reach for `collect()` when you genuinely need random access or a
second pass over the same values. Everywhere else, a terminal that
reduces keeps the program flat in memory however large the input
grows.
