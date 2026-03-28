; ── Keywords ──────────────────────────────────────────────
"fn" @keyword
"const" @keyword
"type" @keyword
"match" @keyword
"return" @keyword
"import" @keyword
"from" @keyword
"export" @keyword
"async" @keyword
"await" @keyword
"try" @keyword
"trusted" @keyword
"for" @keyword
"trait" @keyword
"opaque" @keyword
"test" @keyword
"assert" @keyword
"when" @keyword
"collect" @keyword
"deriving" @keyword

; ── Self ────────────────────────────────────────────────
(self) @variable.builtin

; ── Built-in constructors ────────────────────────────────
"Ok" @constructor
"Err" @constructor
"Some" @constructor
(none) @constant.builtin
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
(none) @constant.builtin
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

; ── Parameters ───────────────────────────────────────────
(parameter
  name: (identifier) @variable.parameter)

(lambda_parameter
  name: (identifier) @variable.parameter)

; ── Arrow closure ────────────────────────────────────────
(arrow_closure "=>" @operator)
(arrow_closure "async" @keyword)

; ── Dot shorthand ────────────────────────────────────────
(dot_shorthand
  "." @punctuation.delimiter
  field: (identifier) @property)

; ── Operators ────────────────────────────────────────────
"|>" @operator
"->" @operator
"?" @operator
".." @operator
(operator) @operator
(unary_operator) @operator

; ── Variants ─────────────────────────────────────────────
(variant
  name: (type_identifier) @constructor)

(variant_pattern
  name: (type_identifier) @constructor)

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
