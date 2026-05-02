# Serde Stability

The `File` AST type carries a `format_version` field. Serialized ASTs produced
by this version of the compiler will always have `format_version == 1`. Tools
that consume serialized ASTs should check this field to detect incompatible
wire-format changes.

```formalang
// All parsed files automatically have format_version: 1 set
```

All public AST types implement `Serialize` / `Deserialize` and are marked
`#[non_exhaustive]` so that adding new variants or fields in future releases
does not break existing consumers at the API boundary.
