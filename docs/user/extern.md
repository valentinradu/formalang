# Extern Declarations

Extern declarations describe functions and method stubs defined outside FormaLang
(in the host runtime or a linked library). They have no FormaLang body.

Types are always declared as normal structs. Use `extern impl` to attach host-provided
methods to a struct, and `extern fn` for standalone host-provided functions.

## Extern Functions

A bodyless function provided by the host:

```formalang
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

- Types are always normal structs — there is no `extern type`
- Extern functions and extern impl methods have no body
- A struct can have both a regular `impl` block and an `extern impl` block
