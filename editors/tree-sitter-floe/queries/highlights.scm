; ── Keywords ──────────────────────────────────────────────
"let" @keyword
"fn" @keyword
"async" @keyword
"type" @keyword
"typealias" @keyword
"match" @keyword
"return" @keyword
"import" @keyword
"from" @keyword
"export" @keyword
"as" @keyword
"default" @keyword
"trusted" @keyword
"for" @keyword
"impl" @keyword
"trait" @keyword
"opaque" @keyword
"test" @keyword
"assert" @keyword
"when" @keyword
"collect" @keyword
"use" @keyword

; ── Self ────────────────────────────────────────────────
(self) @variable.builtin

; ── Built-in constructors ────────────────────────────────
"Value" @constructor
(clear) @constant.builtin
(unchanged) @constant.builtin
(todo) @keyword
(unreachable) @keyword
(mock_expression "mock" @keyword)

; ── Literals ─────────────────────────────────────────────
(number) @number
(string) @string
(template_literal) @string
(template_interpolation
  "${" @punctuation.special
  "}" @punctuation.special)
(boolean) @boolean
(underscore) @variable.builtin
(unit_value) @constant.builtin

; ── Types ────────────────────────────────────────────────
(primitive_type) @type.builtin
(type_identifier) @type
(type_parameters "<" @punctuation.bracket ">" @punctuation.bracket)
(type_arguments "<" @punctuation.bracket ">" @punctuation.bracket)

; ── Functions ────────────────────────────────────────────
(function_declaration
  name: (identifier) @function)

(function_declaration
  name: (type_identifier) @function)

; Generic function type parameters: fn identity<T>(...)
(function_declaration
  type_parameters: (type_parameters
    (type_identifier) @type.parameter))

(call_expression
  function: (primary_expression
    (identifier) @function.call))

(call_expression
  function: (member_expression
    property: (identifier) @function.method))

(tagged_template_expression
  tag: (primary_expression
    (identifier) @function.call))

(tagged_template_expression
  tag: (member_expression
    property: (identifier) @function.method))

; ── Parameters ───────────────────────────────────────────
(parameter
  name: (identifier) @variable.parameter)

(lambda_parameter
  name: (identifier) @variable.parameter)

; ── Arrow closure ────────────────────────────────────────
(arrow_closure "->" @operator)

; ── Function type / return type arrow ─────────────────────
(function_type "->" @operator)
(function_declaration "->" @operator)
(trait_method "->" @operator)

; ── Dot shorthand ────────────────────────────────────────
(dot_shorthand
  "." @punctuation.delimiter
  field: (identifier) @property)

; ── Operators ────────────────────────────────────────────
"|>" @operator
"|>?" @operator
"->" @operator
"?" @operator
".." @operator
(operator) @operator
(unary_operator) @operator

; ── Variants ─────────────────────────────────────────────
(variant
  name: (type_identifier) @constructor)

(variant_field
  name: (identifier) @property)

(variant_pattern
  name: (type_identifier) @constructor)

(variant_field_pattern
  name: (identifier) @property)

(variant_expression
  variant: (type_identifier) @constructor)

(construct_expression
  type: (type_identifier) @constructor)

; ── Traits ──────────────────────────────────────────────
(trait_declaration
  name: (type_identifier) @type.definition)

(trait_method
  name: (identifier) @function)

; ── Record fields ────────────────────────────────────────
(record_field
  name: (identifier) @property)

; ── Match ────────────────────────────────────────────────
(match_arm
  "->" @operator)

; ── JSX ──────────────────────────────────────────────────
(jsx_opening_element
  "<" @tag.delimiter
  name: (identifier) @tag
  ">" @tag.delimiter)

(jsx_opening_element
  name: (type_identifier) @tag)

(jsx_closing_element
  "</" @tag.delimiter
  name: (identifier) @tag
  ">" @tag.delimiter)

(jsx_closing_element
  name: (type_identifier) @tag)

(jsx_self_closing
  "<" @tag.delimiter
  name: (identifier) @tag
  "/>" @tag.delimiter)

(jsx_self_closing
  name: (type_identifier) @tag)

(jsx_member_expression
  object: (identifier) @tag
  "." @tag.delimiter
  property: (identifier) @tag)

(jsx_member_expression
  object: (type_identifier) @tag
  "." @tag.delimiter
  property: (type_identifier) @tag)

(jsx_member_expression
  object: (type_identifier) @tag
  "." @tag.delimiter
  property: (identifier) @tag)

(jsx_member_expression
  object: (identifier) @tag
  "." @tag.delimiter
  property: (type_identifier) @tag)

(jsx_attribute
  name: (identifier) @tag.attribute)

(jsx_expression
  "{" @punctuation.special
  "}" @punctuation.special)

; ── Punctuation ──────────────────────────────────────────
"(" @punctuation.bracket
")" @punctuation.bracket
"{" @punctuation.bracket
"}" @punctuation.bracket
"[" @punctuation.bracket
"]" @punctuation.bracket
"," @punctuation.delimiter
":" @punctuation.delimiter
"." @punctuation.delimiter
"=" @operator

; ── Comments ─────────────────────────────────────────────
(comment) @comment

; ── Identifiers (last) ───────────────────────────────────
(identifier) @variable

; ── Import specifiers ────────────────────────────────────
(import_specifier
  (identifier) @variable)

(import_for_specifier
  type: (type_identifier) @type)

(import_for_specifier
  type: (identifier) @type)

(member_expression
  property: (identifier) @property)
