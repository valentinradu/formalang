; FormaLang syntax highlighting queries

; Comments
(line_comment) @comment
(block_comment) @comment
(doc_comment) @comment.documentation

; Keywords
[
  "trait"
  "struct"
  "enum"
  "mod"
  "use"
  "let"
  "default"
  "for"
  "in"
  "if"
  "else"
  "match"
  "provides"
  "consumes"
  "as"
] @keyword

; Modifiers
[
  "pub"
  "mut"
  "mount"
] @keyword.modifier

; Primitive types
(primitive_type) @type.builtin

; Type identifiers
(type_identifier) @type

; Type references in various contexts
(type_reference (type_identifier) @type)
(struct_traits (type_reference (type_identifier) @type))
(trait_bounds (type_reference (type_identifier) @type))
(generic_parameter (type_identifier) @type.parameter)

; Struct/trait/enum names in definitions
(struct_definition name: (type_identifier) @type.definition)
(trait_definition name: (type_identifier) @type.definition)
(enum_definition name: (type_identifier) @type.definition)
(module_definition name: (identifier) @module)

; Field names
(struct_field name: (identifier) @property)
(trait_field name: (identifier) @property)
(default_field name: (identifier) @property)
(mount_field name: (identifier) @property)
(call_argument name: (identifier) @property)

; Enum variants
(enum_variant_definition name: (identifier) @constant)
(enum_variant (identifier) @constant)
(match_pattern (identifier) @constant)

; Variables
(identifier) @variable
(let_expression (identifier) @variable)
(for_expression variable: (identifier) @variable)
(provides_binding (identifier) @variable)

; Tuple fields
(tuple_field name: (identifier) @property)
(tuple_type_field name: (identifier) @property)

; Literals
(string) @string
(multiline_string) @string
(string_content) @string
(multiline_string_content) @string
(escape_sequence) @string.escape

(number) @number
(boolean) @boolean
(nil) @constant.builtin

(path) @string.special.path
(regex) @string.regex
(regex_content) @string.regex
(regex_flags) @string.regex

; Operators
[
  "+"
  "-"
  "*"
  "/"
  "%"
  "=="
  "!="
  "<"
  ">"
  "<="
  ">="
  "&&"
  "||"
  "="
  ":"
  "::"
  "."
  ","
  "?"
] @operator

; Punctuation
[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

; Special
"..." @punctuation.special
