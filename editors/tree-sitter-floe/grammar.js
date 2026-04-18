/// <reference types="tree-sitter-cli/dsl" />

module.exports = grammar({
  name: "floe",

  extras: ($) => [/\s/, $.comment],

  word: ($) => $.identifier,

  conflicts: ($) => [
    [$.expression_statement, $.binary_expression],
    [$.expression_statement, $.call_expression],
    [$.expression_statement, $.tagged_template_expression],
    [$.expression_statement, $.member_expression],
    [$.expression_statement, $.index_expression],
    [$._type_expression, $.generic_type],
    [$._type_expression, $.qualified_type],
    [$.const_declaration, $.binary_expression],
    [$.const_declaration, $.call_expression],
    [$.const_declaration, $.tagged_template_expression],
    [$.const_declaration, $.member_expression],
    [$.const_declaration, $.index_expression],
    [$.use_declaration, $.binary_expression],
    [$.use_declaration, $.call_expression],
    [$.use_declaration, $.tagged_template_expression],
    [$.use_declaration, $.member_expression],
    [$.use_declaration, $.index_expression],
    [$.primary_expression, $.construct_expression],
    [$.primary_expression, $.construct_expression, $.variant_expression],
    [$.for_block],
    [$.assert_statement, $.call_expression],
    [$.assert_statement, $.tagged_template_expression],
    [$.assert_statement, $.member_expression],
    [$.assert_statement, $.binary_expression],
    [$.assert_statement, $.index_expression],
    [$.primary_expression, $.lambda_parameter],
  ],

  precedences: ($) => [
    [
      "member",
      "call",
      "construct",
      "unary",
      "multiply",
      "add",
      "compare",
      "equality",
      "and",
      "or",
      "pipe",
      "unwrap",
      "assign",
    ],
  ],

  rules: {
    source_file: ($) => repeat($._item),

    _item: ($) =>
      choice(
        $.import_declaration,
        $.export_declaration,
        $.function_declaration,
        $.type_declaration,
        $.const_declaration,
        $.for_block,
        $.trait_declaration,
        $.test_block,
        $.use_declaration,
        $.expression_statement,
      ),

    // ── Imports ──────────────────────────────────────────────

    import_declaration: ($) =>
      seq(
        "import",
        optional("trusted"),
        "{",
        commaSep1(choice($.import_specifier, $.import_for_specifier)),
        "}",
        "from",
        $.string,
      ),

    import_specifier: ($) =>
      seq(optional("trusted"), $.identifier, optional(seq("as", $.identifier))),

    import_for_specifier: ($) =>
      seq("for", field("type", choice($.type_identifier, $.identifier))),

    // ── For Blocks ─────────────────────────────────────────

    for_block: ($) =>
      seq(
        optional("export"),
        "for",
        field("type", $._type_expression),
        optional(seq(":", field("trait", $.type_identifier))),
        "{",
        repeat(seq(optional("export"), $.function_declaration)),
        "}",
      ),

    // ── Traits ──────────────────────────────────────────────

    trait_declaration: ($) =>
      seq(
        optional("export"),
        "trait",
        field("name", $.type_identifier),
        "{",
        repeat($.trait_method),
        "}",
      ),

    trait_method: ($) =>
      seq(
        "fn",
        field("name", $.identifier),
        optional(field("type_parameters", $.type_parameters)),
        field("parameters", $.parameter_list),
        optional(seq("->", field("return_type", $._type_expression))),
        optional(field("body", $.block)),
      ),

    self: (_$) => "self",

    // ── Test Blocks ─────────────────────────────────────────

    test_block: ($) =>
      seq(
        "test",
        field("name", $.string),
        "{",
        repeat(choice($.assert_statement, $.expression_statement)),
        "}",
      ),

    assert_statement: ($) =>
      seq("assert", field("expression", $._expression)),

    // ── Exports ─────────────────────────────────────────────

    export_declaration: ($) =>
      seq("export", choice($.function_declaration, $.const_declaration, $.for_block)),

    // ── Functions ───────────────────────────────────────────

    function_declaration: ($) =>
      seq(
        optional("async"),
        "fn",
        field("name", choice($.identifier, $.type_identifier)),
        optional(field("type_parameters", $.type_parameters)),
        field("parameters", $.parameter_list),
        optional(seq("->", field("return_type", $._type_expression))),
        field("body", $.block),
      ),

    parameter_list: ($) => seq("(", commaSep($.parameter), ")"),

    parameter: ($) =>
      seq(
        field("name", $.identifier),
        optional(seq(":", field("type", $._type_expression))),
        optional(seq("=", field("default", $._expression))),
      ),

    // ── Types ───────────────────────────────────────────────

    type_declaration: ($) =>
      seq(
        optional("opaque"),
        "type",
        field("name", $.type_identifier),
        optional($.type_parameters),
        optional("="),
        field("definition", $._type_definition),
        optional($.deriving_clause),
      ),

    deriving_clause: ($) =>
      seq("deriving", "(", commaSep1($.type_identifier), ")"),

    _type_definition: ($) =>
      choice($.union_type_definition, $.newtype_definition, $.record_type, $._type_expression),

    // Newtype with paren syntax: type UserId(string)
    newtype_definition: ($) =>
      seq("(", commaSep1($._type_expression), ")"),

    union_type_definition: ($) =>
      prec.right(seq("|", $.variant, repeat(seq("|", $.variant)))),

    variant: ($) =>
      prec.right(1, seq(
        field("name", $.type_identifier),
        optional(choice(
          // Named-field variants: `{ field: Type, ... }`
          seq("{", commaSep1($.variant_field), "}"),
          // Positional variants: `(Type, ...)` — no field names
          seq("(", commaSep1($._type_expression), ")"),
        )),
      )),

    // Named field inside a brace-form variant. The name is required — the
    // Floe parser rejects type-only fields inside `{ ... }` variants.
    variant_field: ($) =>
      seq(
        field("name", $.identifier),
        ":",
        field("type", $._type_expression),
      ),

    record_type: ($) => seq("{", commaSep($.record_field), optional(","), "}"),

    record_field: ($) =>
      seq(
        field("name", $.identifier),
        ":",
        field("type", $._type_expression),
        optional(seq("=", field("default", $._expression))),
      ),

    _type_expression: ($) =>
      choice(
        $.primitive_type,
        $.type_identifier,
        $.generic_type,
        $.qualified_type,
        $.function_type,
        $.unit_type,
        $.array_type,
        $.tuple_type,
      ),

    qualified_type: ($) =>
      seq($.type_identifier, ".", $.type_identifier),

    primitive_type: ($) =>
      choice("number", "string", "boolean", "unknown"),

    type_identifier: ($) => /[A-Z][a-zA-Z0-9]*/,

    generic_type: ($) =>
      seq($.type_identifier, $.type_arguments),

    type_arguments: ($) =>
      seq("<", commaSep1($._type_expression), ">"),

    type_parameters: ($) =>
      seq("<", commaSep1($.type_identifier), ">"),

    function_type: ($) =>
      seq(
        "fn",
        "(",
        commaSep($._type_expression),
        ")",
        "->",
        $._type_expression,
      ),

    unit_type: ($) => seq("(", ")"),

    array_type: ($) => seq("Array", "<", $._type_expression, ">"),

    tuple_type: ($) => seq("[", commaSep1($._type_expression), "]"),

    // ── Const ───────────────────────────────────────────────

    const_declaration: ($) =>
      seq(
        "const",
        choice($.identifier, $.array_pattern, $.object_pattern),
        optional(seq(":", $._type_expression)),
        "=",
        $._expression,
      ),

    array_pattern: ($) =>
      seq("[", commaSep1(choice($.identifier, "_")), "]"),

    object_pattern: ($) =>
      seq("{", commaSep1($.identifier), "}"),

    // ── Use (callback flattening) ───────────────────────────

    use_declaration: ($) =>
      seq(
        "use",
        optional(
          choice(
            field("binding", $.identifier),
            seq(
              "(",
              commaSep1(field("binding", $.identifier)),
              ")",
            ),
            field("binding", $.object_pattern),
          ),
        ),
        "<-",
        field("call", $._expression),
      ),

    // ── Expressions ─────────────────────────────────────────

    expression_statement: ($) => $._expression,

    _expression: ($) =>
      choice(
        $.primary_expression,
        $.binary_expression,
        $.unary_expression,
        $.pipe_expression,
        $.unwrap_expression,
        $.match_expression,
        $.call_expression,
        $.tagged_template_expression,
        $.member_expression,
        $.index_expression,
        $.construct_expression,
        $.variant_expression,
        $.arrow_closure,
        $.dot_shorthand,
        $.jsx_element,
        $.jsx_self_closing,
        $.jsx_fragment,
        $.block,
        $.assignment_expression,
        $.collect_expression,
        $.return_statement,
      ),

    primary_expression: ($) =>
      choice(
        $.identifier,
        $.type_identifier,
        $.number,
        $.string,
        $.template_literal,
        $.boolean,
        $.array_literal,
        $.parenthesized_expression,
        $.unit_value,
        $.clear,
        $.unchanged,
        $.todo,
        $.unreachable,
        $.mock_expression,
        $.self,
        $.underscore,
      ),

    identifier: ($) => /[a-z_$][a-zA-Z0-9_$]*/,

    number: ($) =>
      choice(
        /\d[\d_]*(\.\d[\d_]*)?/,
        /0[xX][\da-fA-F_]+/,
        /0[bB][01_]+/,
        /0[oO][0-7_]+/,
      ),

    string: ($) =>
      seq('"', optional($.string_content), '"'),

    string_content: ($) => /[^"]*/,

    template_literal: ($) =>
      seq(
        "`",
        repeat(
          choice($.template_string_content, $.template_interpolation),
        ),
        "`",
      ),

    template_string_content: ($) => /[^`$\\]+|\\./,

    template_interpolation: ($) =>
      seq("${", $._expression, "}"),

    boolean: ($) => choice("true", "false"),

    array_literal: ($) => seq("[", commaSep($._expression), optional(","), "]"),

    parenthesized_expression: ($) => seq("(", $._expression, ")"),

    unit_value: ($) => prec(2, seq("(", ")")),

    // None is now a regular identifier, not a keyword

    clear: ($) => "Clear",

    unchanged: ($) => "Unchanged",

    todo: ($) => "todo",

    unreachable: ($) => "unreachable",

    mock_expression: ($) =>
      prec.right(seq(
        "mock",
        "<",
        $._type_expression,
        ">",
        optional(seq("(", commaSep($.argument), ")")),
      )),

    underscore: ($) => "_",

    // ── Binary expressions ──────────────────────────────────

    binary_expression: ($) =>
      choice(
        ...[
          ["+", "add"],
          ["-", "add"],
          ["*", "multiply"],
          ["/", "multiply"],
          ["%", "multiply"],
          ["<", "compare"],
          [">", "compare"],
          ["<=", "compare"],
          [">=", "compare"],
          ["==", "equality"],
          ["!=", "equality"],
          ["&&", "and"],
          ["||", "or"],
        ].map(([op, prec_name]) =>
          prec.left(
            prec_name,
            seq(
              field("left", $._expression),
              field("operator", alias(op, $.operator)),
              field("right", $._expression),
            ),
          ),
        ),
      ),

    unary_expression: ($) =>
      prec.left(
        "unary",
        seq(
          field("operator", $.unary_operator),
          field("operand", $._expression),
        ),
      ),

    unary_operator: ($) => choice("!", "-"),

    // ── Pipe ────────────────────────────────────────────────

    pipe_expression: ($) =>
      prec.left(
        "pipe",
        seq(
          field("left", $._expression),
          choice("|>", "|>?"),
          field("right", $._expression),
        ),
      ),

    unwrap_expression: ($) =>
      prec.left("unwrap", seq($._expression, "?")),

    // ── Match ───────────────────────────────────────────────

    match_expression: ($) =>
      seq("match", field("subject", $._expression), "{", repeat($.match_arm), "}"),

    match_arm: ($) =>
      seq(
        field("pattern", $._pattern),
        optional($.match_guard),
        "->",
        field("body", $._expression),
        optional(","),
      ),

    match_guard: ($) =>
      seq("when", field("condition", $._expression)),

    _pattern: ($) =>
      choice(
        $.variant_pattern,
        $.literal_pattern,
        $.binding_pattern,
        $.wildcard_pattern,
        $.record_pattern,
        $.range_pattern,
      ),

    variant_pattern: ($) =>
      seq(
        field("name", $.type_identifier),
        optional(choice(
          // Positional: `Variant(pat, ...)`
          seq("(", commaSep1($._pattern), ")"),
          // Named: `Variant { field, field: pat, ... }`
          seq("{", commaSep1($.variant_field_pattern), "}"),
        )),
      ),

    variant_field_pattern: ($) =>
      seq(
        field("name", $.identifier),
        optional(seq(":", field("pattern", $._pattern))),
      ),

    literal_pattern: ($) =>
      choice($.number, $.string, $.boolean),

    binding_pattern: ($) => $.identifier,

    wildcard_pattern: ($) => "_",

    record_pattern: ($) =>
      seq("{", commaSep1($.record_pattern_field), "}"),

    record_pattern_field: ($) =>
      seq(
        field("name", $.identifier),
        optional(seq(":", field("pattern", $._pattern))),
      ),

    range_pattern: ($) =>
      seq(field("start", $.number), "..", field("end", $.number)),

    // ── Calls ───────────────────────────────────────────────

    call_expression: ($) =>
      prec.left(
        "call",
        seq(field("function", $._expression), $.argument_list),
      ),

    tagged_template_expression: ($) =>
      prec.left(
        "call",
        seq(field("tag", $._expression), field("template", $.template_literal)),
      ),

    argument_list: ($) =>
      seq("(", commaSep($.argument), optional(","), ")"),

    argument: ($) =>
      choice(
        seq(field("label", $.identifier), ":", field("value", $._expression)),
        $._expression,
      ),

    // ── Members ─────────────────────────────────────────────

    member_expression: ($) =>
      prec.left(
        "member",
        seq(field("object", $._expression), ".", field("property", $.identifier)),
      ),

    index_expression: ($) =>
      prec.left(
        "member",
        seq(field("object", $._expression), "[", field("index", $._expression), "]"),
      ),

    // ── Constructors & Variants ─────────────────────────────

    construct_expression: ($) =>
      prec("construct", seq(
        field("type", $.type_identifier),
        optional(seq(".", field("variant_name", $.type_identifier))),
        "(",
        optional(seq("..", field("spread", $._expression))),
        optional(","),
        commaSep($.construct_field),
        optional(","),
        ")",
      )),

    construct_field: ($) =>
      seq(field("name", $.identifier), ":", field("value", $._expression)),

    variant_expression: ($) =>
      choice(
        // Qualified: Filter.All (Type.Variant - both uppercase)
        prec("member", seq(
          field("type", $.type_identifier),
          ".",
          field("variant", $.type_identifier),
        )),
        // Built-in constructor (Settable)
        seq("Value", "(", field("value", $._expression), ")"),
      ),

    // ── Lambdas ─────────────────────────────────────────────

    arrow_closure: ($) =>
      prec.right(
        "pipe",
        seq(
          "(",
          commaSep($.lambda_parameter),
          ")",
          "=>",
          field("body", $._expression),
        ),
      ),

    lambda_parameter: ($) =>
      seq(
        field("name", $.identifier),
        optional(seq(":", field("type", $._type_expression))),
      ),

    // ── Dot shorthand ───────────────────────────────────────

    dot_shorthand: ($) =>
      prec.left(
        "compare",
        seq(
          ".",
          field("field", $.identifier),
          optional(
            seq(
              field("operator", choice("==", "!=", "<", ">", "<=", ">=")),
              field("value", $._expression),
            ),
          ),
        ),
      ),

    // ── JSX ─────────────────────────────────────────────────

    jsx_element: ($) =>
      seq($.jsx_opening_element, repeat($._jsx_child), $.jsx_closing_element),

    jsx_self_closing: ($) =>
      seq("<", field("name", $._jsx_name), repeat($.jsx_attribute), "/>"),

    jsx_fragment: ($) =>
      seq("<>", repeat($._jsx_child), "</>"),

    jsx_opening_element: ($) =>
      seq("<", field("name", $._jsx_name), repeat($.jsx_attribute), ">"),

    jsx_closing_element: ($) =>
      seq("</", field("name", $._jsx_name), ">"),

    _jsx_name: ($) =>
      choice($.jsx_member_expression, $.identifier, $.type_identifier),

    jsx_member_expression: ($) =>
      seq(
        field("object", choice($.identifier, $.type_identifier)),
        ".",
        field("property", choice($.identifier, $.type_identifier)),
      ),

    jsx_attribute: ($) =>
      choice(
        seq(
          field("name", $.identifier),
          "=",
          field("value", choice($.string, $.jsx_expression)),
        ),
        field("name", $.identifier),
      ),

    jsx_expression: ($) =>
      seq("{", $._expression, "}"),

    _jsx_child: ($) =>
      choice($.jsx_text, $.jsx_expression, $.jsx_element, $.jsx_self_closing),

    jsx_text: ($) => /[^<>{]+/,

    // ── Blocks ──────────────────────────────────────────────

    block: ($) => seq("{", repeat($._item), "}"),

    // ── Statements ──────────────────────────────────────────

    return_statement: ($) =>
      prec.right(seq("return", optional($._expression))),

    assignment_expression: ($) =>
      prec.right("assign", seq(field("left", $._expression), "=", field("right", $._expression))),

    collect_expression: ($) =>
      seq("collect", $.block),

    // ── Comments ────────────────────────────────────────────

    comment: ($) =>
      choice(
        seq("//", /[^\n]*/),
        seq("/*", /[^*]*\*+([^/*][^*]*\*+)*/, "/"),
      ),

    // ── Spread operator ─────────────────────────────────────

    spread: ($) => seq("..", $._expression),
  },
});

/**
 * Comma-separated list (zero or more)
 */
function commaSep(rule) {
  return optional(commaSep1(rule));
}

/**
 * Comma-separated list (one or more)
 */
function commaSep1(rule) {
  return seq(rule, repeat(seq(",", rule)));
}
