# Extern Declarations

Extern declarations describe functions and method stubs defined outside FormaLang
(in the host runtime or a linked library). They have no FormaLang body.

Types are always declared as normal structs. Use `extern impl` to attach host-provided
methods to a struct, and `extern fn` for standalone host-provided functions.

## Extern Functions

A bodyless function provided by the host:

```formalang
pub struct Canvas { width: I32, height: I32 }
pub struct Connection { id: I32 }

extern fn create_canvas(width: I32, height: I32) -> Canvas
extern fn connect(url: String) -> Connection
extern fn log(message: String)

extern "C" fn read(fd: I32) -> I32
extern "system" fn GetTickCount() -> I32
```

A bare `extern fn` defaults to the C calling convention. Specify
`"C"` or `"system"` explicitly when the calling convention matters
(e.g. Win32 stdcall on x86). Unknown ABI strings are rejected at
parse time.

An extern function has no body: a body after an `extern fn` is a
`ParseError`. A regular function needs a body: a function without one is the error
`RegularFnWithoutBody`. The compiler checks a call to an extern
function as it checks each call: the argument count, the labels and
the types.

## Extern Impl

Host-provided methods on a struct:

```formalang
struct Canvas { width: I32, height: I32 }

extern impl Canvas {
  fn get_width(self) -> I32
  fn get_height(self) -> I32
  fn clear(self)
}
```

**Rules**:

- Types are always normal structs: there is no `extern type`
- Extern functions and extern impl methods have no body. A method with
  a body in an `extern impl` is the error `ExternImplWithBody`
- A struct can have both a regular `impl` block and an `extern impl` block

## Extern Impl on Primitive Types

Host-provided methods on built-in types like `String`, `I32`, `F64`.
A primitive type takes only an `extern impl`. A regular `impl` or a
trait impl on a primitive is the error `ImplOnPrimitive`:

```formalang
extern impl String {
  fn len(self) -> I32
  fn slice(self, start: I32, end: I32) -> String
}

extern impl I32 {
  fn abs(self) -> I32
}
```

The compiler ships a prelude (`src/prelude.fv`). Each program sees it
without a `use`. It declares:

| Type | Methods |
| --- | --- |
| `String` | `len`, `is_empty`, `slice`, `starts_with`, `contains`, `byte_at` |
| `[T]` (`Array<T>`) | `len`, `is_empty` |
| `[K: V]` (`Dictionary<K, V>`) | `len`, `is_empty` |
| a range (`Range<T>`) | `len`, `is_empty` |
| `T?` (`Optional<T>`) | `is_some`, `is_none` |
| `Seq<T>` | the combinators: see [Control Flow](control-flow.md#combinators) |

It also declares `extern fn assert(condition: Boolean)`, which the
examples in this guide use in `run_checks()`.

Backends bind these as host-provided extern functions through their
existing extern-binding paths (wasm component imports, JS runtime
bindings, etc.). The `Seq` combinators are the exception: each one takes a
closure, which a host cannot call, so a backend lowers them itself as
loop structure.

```formalang
pub fn run_checks() {
  let s = "hello"
  assert(condition: s.len() == 5)
  assert(condition: s.slice(start: 1, end: 3) == "el")
  assert(condition: s.starts_with(prefix: "he"))
  assert(condition: s.contains(needle: "ll"))
  assert(condition: s.byte_at(i: 1) == 101)
  assert(condition: [1, 2].len() == 2)
  assert(condition: ["a": 1].is_empty() == false)
  assert(condition: (0..4).len() == 4)
}
```

`s[i]` for a `String` receiver desugars to `s.byte_at(i)` at IR
lowering, so backends only see standard `MethodCall` shapes.
