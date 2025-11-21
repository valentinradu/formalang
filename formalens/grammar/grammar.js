/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

/**
 * FormaLang tree-sitter grammar
 *
 * A declarative UI language with traits, structs, enums, and a mount system.
 */

const PREC = {
  COMMENT: 0,
  OR: 1,
  AND: 2,
  EQUALITY: 3,
  COMPARISON: 4,
  ADDITIVE: 5,
  MULTIPLICATIVE: 6,
  UNARY: 7,
  FIELD_ACCESS: 8,
  CALL: 9,
};

module.exports = grammar({
  name: 'formalang',

  extras: $ => [
    /\s/,
    $.line_comment,
    $.block_comment,
  ],

  word: $ => $.identifier,

  conflicts: $ => [
    [$.type_identifier, $.identifier],
    [$._expression, $.enum_variant],
    [$.struct_instantiation],
    [$.enum_variant],
  ],

  rules: {
    source_file: $ => repeat($._item),

    _item: $ => choice(
      $.use_statement,
      $.module_definition,
      $.trait_definition,
      $.struct_definition,
      $.enum_definition,
      $.let_binding,
      $.default_block,
    ),

    // =========================================================================
    // COMMENTS
    // =========================================================================

    line_comment: $ => token(seq('//', /.*/)),

    block_comment: $ => token(seq(
      '/*',
      /[^*]*\*+([^/*][^*]*\*+)*/,
      '/'
    )),

    doc_comment: $ => token(seq('///', /.*/)),

    // =========================================================================
    // USE STATEMENTS
    // =========================================================================

    use_statement: $ => seq(
      'use',
      $.use_path,
    ),

    use_path: $ => seq(
      $.identifier,
      repeat(seq('::', choice(
        $.identifier,
        $.use_group,
      ))),
    ),

    use_group: $ => seq(
      '{',
      sepBy(',', $.identifier),
      optional(','),
      '}',
    ),

    // =========================================================================
    // MODULE DEFINITION
    // =========================================================================

    module_definition: $ => seq(
      optional('pub'),
      'mod',
      field('name', $.identifier),
      '{',
      repeat($._item),
      '}',
    ),

    // =========================================================================
    // TRAIT DEFINITION
    // =========================================================================

    trait_definition: $ => seq(
      optional($.doc_comment),
      optional('pub'),
      'trait',
      field('name', $.type_identifier),
      optional($.generic_parameters),
      optional($.trait_bounds),
      '{',
      repeat($.trait_field),
      '}',
    ),

    trait_bounds: $ => seq(
      ':',
      sepBy1('+', $.type_reference),
    ),

    trait_field: $ => seq(
      optional('mount'),
      field('name', $.identifier),
      ':',
      field('type', $._type),
      optional(','),
    ),

    // =========================================================================
    // STRUCT DEFINITION
    // =========================================================================

    struct_definition: $ => seq(
      optional($.doc_comment),
      optional('pub'),
      'struct',
      field('name', $.type_identifier),
      optional($.generic_parameters),
      optional($.struct_traits),
      '{',
      repeat($.struct_field),
      '}',
    ),

    struct_traits: $ => seq(
      ':',
      sepBy1('+', $.type_reference),
    ),

    struct_field: $ => seq(
      optional('mount'),
      optional('mut'),
      field('name', $.identifier),
      ':',
      field('type', $._type),
      optional(','),
    ),

    // =========================================================================
    // ENUM DEFINITION
    // =========================================================================

    enum_definition: $ => seq(
      optional($.doc_comment),
      optional('pub'),
      'enum',
      field('name', $.type_identifier),
      optional($.generic_parameters),
      '{',
      sepBy(',', $.enum_variant_definition),
      optional(','),
      '}',
    ),

    enum_variant_definition: $ => seq(
      field('name', $.identifier),
      optional($.enum_variant_parameters),
    ),

    enum_variant_parameters: $ => seq(
      '(',
      sepBy(',', $.enum_variant_parameter),
      optional(','),
      ')',
    ),

    enum_variant_parameter: $ => seq(
      field('name', $.identifier),
      ':',
      field('type', $._type),
    ),

    // =========================================================================
    // DEFAULT BLOCK
    // =========================================================================

    default_block: $ => seq(
      'default',
      $.type_reference,
      '{',
      repeat($.default_field),
      '}',
    ),

    default_field: $ => seq(
      field('name', $.identifier),
      ':',
      field('value', $._expression),
      optional(','),
    ),

    // =========================================================================
    // LET BINDING
    // =========================================================================

    let_binding: $ => seq(
      optional('pub'),
      'let',
      optional('mut'),
      choice(
        $.identifier,
        $.array_pattern,
        $.struct_pattern,
      ),
      optional(seq(':', $._type)),
      '=',
      $._expression,
    ),

    // =========================================================================
    // PATTERNS (for destructuring)
    // =========================================================================

    array_pattern: $ => seq(
      '[',
      sepBy(',', choice(
        $.identifier,
        '_',
        $.rest_pattern,
      )),
      optional(','),
      ']',
    ),

    struct_pattern: $ => seq(
      '{',
      sepBy(',', choice(
        $.identifier,
        $.renamed_field,
      )),
      optional(','),
      '}',
    ),

    renamed_field: $ => seq(
      $.identifier,
      'as',
      $.identifier,
    ),

    rest_pattern: $ => seq('...', optional($.identifier)),

    // =========================================================================
    // TYPES
    // =========================================================================

    _type: $ => choice(
      $.primitive_type,
      $.type_reference,
      $.array_type,
      $.dictionary_type,
      $.optional_type,
      $.generic_type,
    ),

    primitive_type: $ => choice(
      'String',
      'Number',
      'Boolean',
      'Path',
      'Regex',
      'Never',
    ),

    type_reference: $ => seq(
      $.type_identifier,
      repeat(seq('::', $.type_identifier)),
    ),

    array_type: $ => seq(
      '[',
      $._type,
      optional(seq(',', $.number)),
      ']',
    ),

    dictionary_type: $ => seq(
      '[',
      field('key', $._type),
      ':',
      field('value', $._type),
      ']',
    ),

    optional_type: $ => seq(
      $._type,
      '?',
    ),

    generic_type: $ => seq(
      $.type_identifier,
      $.generic_arguments,
    ),

    generic_parameters: $ => seq(
      '<',
      sepBy1(',', $.generic_parameter),
      '>',
    ),

    generic_parameter: $ => seq(
      $.type_identifier,
      optional(seq(':', sepBy1('+', $.type_reference))),
    ),

    generic_arguments: $ => seq(
      '<',
      sepBy1(',', $._type),
      '>',
    ),

    // =========================================================================
    // EXPRESSIONS
    // =========================================================================

    _expression: $ => choice(
      $.literal,
      $.identifier,
      $.field_access,
      $.binary_expression,
      $.enum_variant,
      $.struct_instantiation,
      $.array_literal,
      $.dictionary_literal,
      $.for_expression,
      $.if_expression,
      $.match_expression,
      $.provides_expression,
      $.consumes_expression,
      $.parenthesized_expression,
    ),

    parenthesized_expression: $ => seq('(', $._expression, ')'),

    // =========================================================================
    // LITERALS
    // =========================================================================

    literal: $ => choice(
      $.string,
      $.multiline_string,
      $.number,
      $.boolean,
      $.nil,
      $.path,
      $.regex,
    ),

    string: $ => seq(
      '"',
      repeat(choice(
        $.string_content,
        $.escape_sequence,
      )),
      '"',
    ),

    string_content: $ => token.immediate(prec(1, /[^"\\]+/)),

    escape_sequence: $ => token.immediate(seq(
      '\\',
      choice(
        /[\\'"nrt0]/,
        /u\{[0-9a-fA-F]+\}/,
        /u[0-9a-fA-F]{4}/,
      ),
    )),

    multiline_string: $ => seq(
      '"""',
      repeat(choice(
        $.multiline_string_content,
        $.escape_sequence,
      )),
      '"""',
    ),

    multiline_string_content: $ => token.immediate(prec(1, /[^"\\]+|"[^"]|""[^"]/)),

    number: $ => token(choice(
      /\d[\d_]*/,
      /\d[\d_]*\.\d[\d_]*/,
      /-\d[\d_]*/,
      /-\d[\d_]*\.\d[\d_]*/,
    )),

    boolean: $ => choice('true', 'false'),

    nil: $ => 'nil',

    path: $ => token(seq('/', /[a-zA-Z0-9_.\-\/]+/)),

    regex: $ => seq(
      'r/',
      $.regex_content,
      '/',
      optional($.regex_flags),
    ),

    regex_content: $ => token.immediate(/[^\/]*/),

    regex_flags: $ => token.immediate(/[gimsuyv]+/),

    // =========================================================================
    // FIELD ACCESS
    // =========================================================================

    field_access: $ => prec.left(PREC.FIELD_ACCESS, seq(
      $._expression,
      '.',
      $.identifier,
    )),

    // =========================================================================
    // BINARY EXPRESSIONS
    // =========================================================================

    binary_expression: $ => choice(
      prec.left(PREC.OR, seq($._expression, '||', $._expression)),
      prec.left(PREC.AND, seq($._expression, '&&', $._expression)),
      prec.left(PREC.EQUALITY, seq($._expression, choice('==', '!='), $._expression)),
      prec.left(PREC.COMPARISON, seq($._expression, choice('<', '>', '<=', '>='), $._expression)),
      prec.left(PREC.ADDITIVE, seq($._expression, choice('+', '-'), $._expression)),
      prec.left(PREC.MULTIPLICATIVE, seq($._expression, choice('*', '/', '%'), $._expression)),
    ),

    // =========================================================================
    // ENUM VARIANT
    // =========================================================================

    enum_variant: $ => seq(
      '.',
      $.identifier,
      optional($.call_arguments),
    ),

    // =========================================================================
    // STRUCT INSTANTIATION
    // =========================================================================

    struct_instantiation: $ => seq(
      $.type_reference,
      optional($.generic_arguments),
      optional($.call_arguments),
      optional($.mount_block),
    ),

    call_arguments: $ => seq(
      '(',
      sepBy(',', $.call_argument),
      optional(','),
      ')',
    ),

    call_argument: $ => seq(
      field('name', $.identifier),
      ':',
      field('value', $._expression),
    ),

    mount_block: $ => seq(
      '{',
      repeat($.mount_field),
      '}',
    ),

    mount_field: $ => seq(
      field('name', $.identifier),
      ':',
      field('value', choice(
        $._expression,
        $.mount_children,
      )),
    ),

    mount_children: $ => seq(
      '{',
      repeat($._expression),
      '}',
    ),

    // =========================================================================
    // ARRAY AND DICTIONARY LITERALS
    // =========================================================================

    array_literal: $ => seq(
      '[',
      sepBy(',', $._expression),
      optional(','),
      ']',
    ),

    dictionary_literal: $ => seq(
      '[',
      choice(
        seq(sepBy1(',', $.dictionary_entry), optional(',')),
        ':',  // empty dict [:]
      ),
      ']',
    ),

    dictionary_entry: $ => seq(
      field('key', $._expression),
      ':',
      field('value', $._expression),
    ),

    // =========================================================================
    // CONTROL FLOW
    // =========================================================================

    for_expression: $ => seq(
      'for',
      field('variable', $.identifier),
      'in',
      field('iterable', $._expression),
      $.block,
    ),

    if_expression: $ => seq(
      'if',
      field('condition', $._expression),
      $.block,
      optional(seq(
        'else',
        choice($.if_expression, $.block),
      )),
    ),

    match_expression: $ => seq(
      'match',
      field('value', $._expression),
      '{',
      repeat($.match_arm),
      '}',
    ),

    match_arm: $ => seq(
      $.match_pattern,
      ':',
      $._expression,
    ),

    match_pattern: $ => seq(
      '.',
      $.identifier,
      optional(seq('(', sepBy(',', $.identifier), ')')),
    ),

    block: $ => seq(
      '{',
      repeat($._expression),
      '}',
    ),

    // =========================================================================
    // CONTEXT SYSTEM
    // =========================================================================

    provides_expression: $ => seq(
      'provides',
      sepBy1(',', $.provides_binding),
      $.block,
    ),

    provides_binding: $ => seq(
      $._expression,
      'as',
      $.identifier,
    ),

    consumes_expression: $ => seq(
      'consumes',
      sepBy1(',', $.identifier),
      $.block,
    ),

    // =========================================================================
    // IDENTIFIERS
    // =========================================================================

    identifier: $ => /[a-z_][a-zA-Z0-9_]*/,

    type_identifier: $ => /[A-Z][a-zA-Z0-9_]*/,
  },
});

/**
 * Creates a rule that matches one or more occurrences separated by a delimiter.
 */
function sepBy1(delimiter, rule) {
  return seq(rule, repeat(seq(delimiter, rule)));
}

/**
 * Creates a rule that matches zero or more occurrences separated by a delimiter.
 */
function sepBy(delimiter, rule) {
  return optional(sepBy1(delimiter, rule));
}
