# Built-in Passes

Exported from `formalang::ir`. Compose them through a [`Pipeline`](plugins.md).

`Pipeline::new()` and `Pipeline::default()` hold no pass.
`Pipeline::for_codegen()` holds four passes, in this order:

1. `MonomorphisePass`
2. `ResolveReferencesPass`
3. `ClosureConversionPass`
4. `DeadCodeEliminationPass`

`ConstantFoldingPass` and `DefunctionalisePass` are in no preset. Add
them with `.pass(...)`.

`compile_to_ir_with_resolver` and `compile_to_ir_with_path_and_resolver`
run `MonomorphisePass` on their result. The other `compile_*` entry
points run no pass.

## `MonomorphisePass`

Replaces each generic definition with one concrete copy for each set
of type arguments that the program uses. The pass:

- copies each generic struct, enum and trait once for each
  `(base, args)` pair that a type in the module names, and rewrites
  each `Generic { base, args }` to the copy. A copy has a name that
  holds the types, for example `Box__I32`;
- copies each impl block of a generic type once for each copy of the
  type, and points each static dispatch at the right copy;
- copies each generic function once for each tuple of type arguments
  that a call gives it, and points the call at the copy;
- copies each method that declares its own type parameters and has a
  body, in the same way. The copy goes at the end of its impl block
  under a name that holds the types (`pair__I32`), and each call gets
  that name and index. An extern method has no body to copy, so it
  keeps its type parameters; each call carries the concrete types in
  its arguments and its `ty`;
- turns each `Virtual` dispatch on a concrete receiver into a
  `Static` dispatch. The frontend has no dynamic dispatch, so no
  `Virtual` dispatch remains. A dispatch through a generic trait
  carries its `trait_args`, so the pass picks the impl of the right
  instance when one type implements `Container<I32>` and
  `Container<String>`;
- removes the generic originals, renumbers the ids that are left, and
  makes the `function_id` of each call and the `struct_id` or
  `enum_id` of each instance agree with its path or type.

A call gives no `<...>` for a method, and often none for a function.
The pass finds the type arguments by unification of each declared
parameter type with the type of its argument. An argument can already
have a specialised type, such as `Box__I32`. The pass keeps the
instantiation behind each copy, so a parameter of type `Box<T>` still
binds `T` to `I32`. When the arguments bind one type parameter to two
different types, the pass reports an `InternalError`: the semantic
pass refuses such a call, so the conflict is a compiler defect.

The five prelude carriers stay generic: `Array`, `Seq`, `Dictionary`,
`Range` and `Optional`. After the pass, `Generic { base, args }` over
one of them is the final shape of `[T]`, a sequence, `[K: V]`, `a..b`
and `T?`. No other generic definition, `TypeParam` or `Generic`
remains. The pass reports a leftover as an `InternalError`.

**Depth limit.** A generic that calls itself with a larger type, such
as `grow(x: Box(value: x))` inside `grow<T>`, needs a new copy at each
level. The pass stops at a nesting depth of 32 type arguments and
reports `InstantiationDepthExceeded` (E148), one time for each generic.
The field `written` is true when the deep type is in the program text,
such as `Box<Box<...>>`, and false when a generic makes it by a call of
itself.

The result does not depend on the order of `HashMap` walks: two runs
on one source give the same module.

`MonomorphisePass::with_imports` takes the IR of each imported module,
keyed by module path. No entry point uses it: the entry points link
each imported module during the lowering (see
[Obtaining the IR](../ir/obtaining.md#programs-over-several-files)),
so their modules hold no `ResolvedType::External`. It is for a module
that holds `External` types, such as the result of
`formalang::ir::lower_to_ir` for a file with a `use`. The pass then
copies each imported definition into the module under a qualified name
and rewrites each `External` to the copy.

## `ResolveReferencesPass`

Rewrites name-keyed references into typed ids:

- each parameter, local `let`, loop variable and match binding gets a
  `BindingId`, and each `LetRef` and `Reference` to it gets the same id;
- each `Reference` gets a `ReferenceTarget`: a function, struct, enum,
  trait, module-level `let`, local binding or parameter;
- each field read and each field of a struct or variant literal gets
  its `FieldIdx`, each match arm its `VariantIdx`, and each method
  call its `MethodIdx`.

A generic struct or enum has its fields and variants on its base
definition, and the pass looks there. So a match arm on `I32?` or on
`Maybe<I32>` gets the index of its variant, and a field read on
`Box<I32>` gets the index of its field. This holds before
`MonomorphisePass` too.

The pass walks each function body, each impl method body, each
module-level `let`, and the default of each field of each struct, enum
variant and trait. After the walk, it adds the missing trailing default
arguments to each call whose callee was not known at lowering time.

The pass is idempotent. It is in `Pipeline::for_codegen`. Use it when
the backend emits integer-indexed code (wasm, JVM, native).

## `ClosureConversionPass`

Lifts every closure body to a top-level function and collects the
values it captures into a synthetic env struct, so a backend only ever
sees named functions. `IrExpr::Closure` becomes
`IrExpr::ClosureRef { funcref, env_struct }`. The lifted functions are
named `__closure<N>` and the env structs `__ClosureEnv<N>`. Included in
`Pipeline::for_codegen`.

A call of a closure value stays an `IrExpr::CallClosure`. After the
pass its `closure` gives a `ClosureRef`, and a backend reads the lifted
function and the env out of that value.

The pass is idempotent and preserves source spans: a captured
variable's synthesised `__env.x` access carries the span of the
reference it replaces, so a debugger stepping over it lands on the
name the user wrote.

## `DefunctionalisePass`

Turns every closure value into an enum tag. Run it after
`ClosureConversionPass`, whose output it consumes, and before
`DeadCodeEliminationPass`.

Closure conversion answers "where does the body live?": it lifts each
body to a top-level function and collects the captures into an env
struct. It leaves open what the closure *value* is. Two answers are
possible:

```text
address form:     data bytes → address → indirect jump
defunctionalised: data bytes → tag → match → direct call
```

This pass takes the second. It builds one enum per distinct closure
type, with one variant per lifted function, each carrying the env
struct closure conversion already made:

```text
enum __Fn0 {
    __closure0(env: __ClosureEnv0),
    __closure1(env: __ClosureEnv1),
}

fn __call_Fn0(f: __Fn0, x: I32) -> I32 {
    match f {
        .__closure0(env): __closure0(__env: env, x: x),
        .__closure1(env): __closure1(__env: env, x: x),
    }
}
```

Every `ClosureRef` becomes an `EnumInst`, every `CallClosure` becomes a
`FunctionCall` to the dispatch function, and every occurrence of the
closure type becomes the enum type. After the pass, **the module
contains no indirect call**. The pass checks this, and reports a
leftover `ClosureRef` or `CallClosure` as an `InternalError`.

The pass collects the closure types from the closure values and from
the call sites. A closure type that only a parameter declares, with no
closure value in the module, gets an enum with no variants.

**Names.** The number `K` in `__Fn<K>` and `__call_Fn<K>` counts from
0. The pass skips each `K` for which the module already has an enum
`__Fn<K>` or a function `__call_Fn<K>`. A user function called
`__call_Fn0` thus keeps its name, and its calls do not reach a
dispatcher.

**Why a tag and not an address.** With the address form a code address
lives inside a data value, so any defect that corrupts those bytes
becomes a jump to anywhere. With a tag, the same corruption at worst
selects the wrong arm: a wrong answer, not a takeover. A backend also
gains, because every call target is known and each arm can be inlined.

**Closed world.** The pass must see every closure of a shape before it
can build that shape's enum. A just-in-time backend compiles the whole
program at once, so this holds there. It would not hold under separate
compilation, which is why the pass is opt-in rather than part of
`Pipeline::for_codegen`.

## `DeadCodeEliminationPass`

Removes code that nothing uses:

- the branch of an `if` whose condition is a literal `true` or `false`;
- each local `let` binding that nothing reads, when its value can have
  no effect and no fault. A value that holds a call, a division or an
  index stays: a call can reach an extern function, and a division or
  an index can fault;
- each struct, trait and enum that nothing references, and each impl
  block whose target goes. A public definition stays: it is the
  contract of the module with its users. The pass repeats this until
  nothing more goes, because the removal of one definition can make
  another one unused;
- each private module-level `let` that nothing reads, with the same
  rule for its value. A public `let` stays.

The pass removes no function. After a removal, it rewrites each id in
the module and rebuilds the name indexes.

`DeadCodeEliminationPass::new()` sets `remove_unused_structs` to
`true`. With `false`, the pass only removes branches and local
bindings. Included in `Pipeline::for_codegen`.

## `ConstantFoldingPass`

Evaluates constant expressions at compile time. It folds an operation
when each operand is a literal. It does not propagate the value of a
binding: `let x = 1` followed by `x + 1` stays a `BinaryOp`.

A numeric operation computes in the width of its operand type:

| Operand type | Arithmetic | Stays unfolded when |
| --- | --- | --- |
| `I32` | checked `i32` | the result overflows `i32`, or the divisor is 0 |
| `I64` | checked `i64` | the result overflows `i64`, or the divisor is 0 |
| `F32` | IEEE 754 binary32: each operand rounds to `f32` first | the result is not finite, or the divisor is 0 |
| `F64` | IEEE 754 binary64 | the result is not finite, or the divisor is 0 |

So `2147483647 + 1 > 2147483647` does not fold for `I32`: the addition
overflows, and the backend decides what it gives. A result that is not
finite stays unfolded because the IR JSON cannot hold it. The negation
of the lowest value of an integer type overflows too, so it stays
unfolded.

The pass also folds `&&`, `||`, `==` and `!=` on two boolean literals,
`!` on a boolean literal, and `+` on two string literals. It walks
function bodies, impl method bodies, module-level `let` values and
struct field defaults.
