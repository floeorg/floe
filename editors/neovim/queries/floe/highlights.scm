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
"opaque" @keyword
"when" @keyword

; ── Self ────────────────────────────────────────────────
(self) @variable.builtin

; ── Built-in constructors ────────────────────────────────
"Ok" @constructor
"Err" @constructor
"Some" @constructor
(none) @constant.builtin
(todo) @keyword
(unreachable) @keyword

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

; ── Lambda ───────────────────────────────────────────────
(pipe_lambda "|" @punctuation.delimiter)
(pipe_lambda "||" @punctuation.delimiter)

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

(member_expression
  property: (identifier) @property)
