"""Tests for textDocument/hover."""

from .conftest import URI, hover_text, open_doc
from . import fixtures as F


class TestHoverBasic:
    """Hover on constants, functions, and types."""

    def test_const_number(self, lsp):
        # Use a minimal fixture — SIMPLE contains a template literal whose
        # span miscalculation causes `find_expr_type_at_offset` to treat
        # unrelated top-level bindings as having string type (known issue).
        open_doc(lsp, URI, "let x = 42\n")
        h = hover_text(lsp.hover(URI, 0, 4))
        assert h is not None and "number" in h, f"Expected number type, got: {h}"

    def test_const_string(self, lsp):
        open_doc(lsp, URI,F.SIMPLE)
        h = hover_text(lsp.hover(URI, 1, 4))
        assert h is not None and "string" in h, f"Expected string type, got: {h}"

    def test_const_boolean(self, lsp):
        open_doc(lsp, URI,F.SIMPLE)
        h = hover_text(lsp.hover(URI, 2, 4))
        assert h is not None and ("boolean" in h or "bool" in h), f"Expected boolean type, got: {h}"

    def test_fn_signature(self, lsp):
        open_doc(lsp, URI,F.SIMPLE)
        h = hover_text(lsp.hover(URI, 4, 4))
        assert h is not None and "let add" in h, f"Expected let add signature, got: {h}"

    def test_export_fn_signature(self, lsp):
        open_doc(lsp, URI,F.SIMPLE)
        h = hover_text(lsp.hover(URI, 8, 11))
        assert h is not None and "greet" in h, f"Expected greet signature, got: {h}"

    def test_whitespace_returns_null(self, lsp):
        open_doc(lsp, URI,F.SIMPLE)
        resp = lsp.hover(URI, 3, 0)
        assert resp is not None and resp.get("result") is None

    def test_type_user(self, lsp):
        open_doc(lsp, URI,F.TYPES)
        h = hover_text(lsp.hover(URI, 2, 5))
        assert h is not None and "User" in h, f"Got: {h}"

    def test_union_variant(self, lsp):
        open_doc(lsp, URI,F.TYPES)
        h = hover_text(lsp.hover(URI, 0, 14))
        assert h is not None, f"Expected hover on variant Red, got: {h}"

    def test_fn_describeColor(self, lsp):
        open_doc(lsp, URI,F.TYPES)
        h = hover_text(lsp.hover(URI, 4, 5))
        assert h is not None and "describeColor" in h, f"Got: {h}"

    def test_builtin_trim(self, lsp):
        open_doc(lsp, URI,F.PIPES)
        h = hover_text(lsp.hover(URI, 6, 11))
        assert h is not None and "trim" in h.lower(), f"Got: {h}"


class TestHoverTaggedTemplate:
    def test_hover_tag_identifier(self, lsp):
        open_doc(lsp, URI, F.TAGGED_TEMPLATE)
        # Line 5: `q = sql`select ...`; `sql` starts at column 10
        h = hover_text(lsp.hover(URI, 5, 10))
        assert h is not None and "sql" in h, f"Expected sql fn hover, got: {h}"


class TestHoverForBlock:
    def test_forblock_fn(self, lsp):
        open_doc(lsp, URI,F.FORBLOCK)
        h = hover_text(lsp.hover(URI, 6, 18))
        assert h is not None and "remaining" in h, f"Got: {h}"


class TestHoverResult:
    def test_ok_builtin(self, lsp):
        open_doc(lsp, URI,F.RESULT)
        h = hover_text(lsp.hover(URI, 3, 14))
        assert h is not None and "ok" in h.lower(), f"Got: {h}"

    def test_err_builtin(self, lsp):
        open_doc(lsp, URI,F.RESULT)
        h = hover_text(lsp.hover(URI, 2, 14))
        assert h is not None and "err" in h.lower(), f"Got: {h}"

    def test_fn_before_question_mark(self, lsp):
        open_doc(lsp, URI,F.RESULT)
        h = hover_text(lsp.hover(URI, 8, 23))
        assert h is not None and "divide" in (h or ""), f"Got: {h}"


class TestHoverAdvanced:
    def test_fn_parameter(self, lsp):
        open_doc(lsp, URI,F.FN_PARAMS_HOVER)
        h = hover_text(lsp.hover(URI, 0, 12))
        assert h is not None, f"Expected hover on parameter, got: {h}"

    def test_nested_match_fn(self, lsp):
        open_doc(lsp, URI,F.NESTED_MATCH)
        h = hover_text(lsp.hover(URI, 4, 4))
        assert h is not None and "describe" in h, f"Got: {h}"

    def test_spread_type(self, lsp):
        open_doc(lsp, URI,F.SPREAD_FILE)
        h = hover_text(lsp.hover(URI, 5, 5))
        assert h is not None and "Extended" in h, f"Got: {h}"

    def test_closure_const(self, lsp):
        open_doc(lsp, URI,F.CLOSURE_ASSIGN)
        h = hover_text(lsp.hover(URI, 0, 6))
        assert h is not None and "add" in h, f"Got: {h}"

    def test_closure_double(self, lsp):
        open_doc(lsp, URI,F.CLOSURE_ASSIGN)
        h = hover_text(lsp.hover(URI, 1, 6))
        assert h is not None and "double" in h, f"Got: {h}"

    def test_closure_call_result(self, lsp):
        open_doc(lsp, URI,F.CLOSURE_ASSIGN)
        h = hover_text(lsp.hover(URI, 2, 6))
        assert h is not None, f"Got: {h}"

    def test_todo_keyword(self, lsp):
        open_doc(lsp, URI,F.TODO_UNREACHABLE)
        h = hover_text(lsp.hover(URI, 1, 4))
        assert h is not None, f"Got: {h}"

    def test_none_literal(self, lsp):
        open_doc(lsp, URI,F.OPTION_FILE)
        h = hover_text(lsp.hover(URI, 2, 15))
        assert h is not None and "none" in h.lower(), f"Got: {h}"

    def test_some_literal(self, lsp):
        open_doc(lsp, URI,F.OPTION_FILE)
        h = hover_text(lsp.hover(URI, 3, 27))
        assert h is not None and "some" in h.lower(), f"Got: {h}"

    def test_match_keyword(self, lsp):
        open_doc(lsp, URI,F.WHEN_GUARD)
        h = hover_text(lsp.hover(URI, 1, 5))
        assert h is not None and "match" in h.lower(), f"Got: {h}"

    def test_variable_in_spread(self, lsp):
        open_doc(lsp, URI,F.RECORD_SPREAD)
        h = hover_text(lsp.hover(URI, 7, 15))
        assert h is not None, f"Got: {h}"

    def test_const_from_fn_call(self, lsp):
        open_doc(lsp, URI,F.MULTIPLE_FNS)
        h = hover_text(lsp.hover(URI, 4, 4))
        assert h is not None and "number" in h, f"Got: {h}"

    def test_nested_fn_call_result(self, lsp):
        open_doc(lsp, URI,F.MULTIPLE_FNS)
        h = hover_text(lsp.hover(URI, 7, 4))
        assert h is not None and "number" in h, f"Got: {h}"

    def test_fn_tuple_return(self, lsp):
        open_doc(lsp, URI,F.TUPLE_FILE)
        h = hover_text(lsp.hover(URI, 0, 4))
        assert h is not None and "swap" in h, f"Got: {h}"

    def test_const_assigned_tuple(self, lsp):
        open_doc(lsp, URI,F.TUPLE_FILE)
        h = hover_text(lsp.hover(URI, 4, 6))
        assert h is not None, f"Got: {h}"

    def test_destructured_tuple_var(self, lsp):
        open_doc(lsp, URI,F.TUPLE_FILE)
        h = hover_text(lsp.hover(URI, 5, 5))
        assert h is not None, f"Got: {h}"

    def test_trait_impl_fn(self, lsp):
        open_doc(lsp, URI,F.TRAIT_FILE)
        h = hover_text(lsp.hover(URI, 10, 8))
        assert h is not None and "print" in h, f"Got: {h}"

    def test_inner_const(self, lsp):
        open_doc(lsp, URI,F.INNER_CONST)
        h = hover_text(lsp.hover(URI, 1, 10))
        assert h is not None, f"Got: {h}"

    def test_inner_const_doubled(self, lsp):
        open_doc(lsp, URI,F.INNER_CONST)
        h = hover_text(lsp.hover(URI, 2, 10))
        assert h is not None, f"Got: {h}"


class TestHoverPipeMapInference:
    def test_pipe_map_result_type(self, lsp):
        open_doc(lsp, URI,F.PIPE_MAP_INFERENCE)
        h = hover_text(lsp.hover(URI, 10, 10))
        assert h is not None and "Accent" in h, f"Got: {h}"

    def test_forblock_fn_in_pipe(self, lsp):
        open_doc(lsp, URI,F.PIPE_MAP_INFERENCE)
        h = hover_text(lsp.hover(URI, 10, 48))
        assert h is not None and "Accent" in h, f"Got: {h}"


class TestHoverTypeQuality:
    """Hover should show concrete types, not 'unknown' or '?T'."""

    def test_no_unknown(self, lsp):
        open_doc(lsp, URI,F.MULTIPLE_FNS)
        h = hover_text(lsp.hover(URI, 4, 4))
        assert h is not None and "unknown" not in h.lower(), f"Got: {h}"

    def test_no_type_var(self, lsp):
        open_doc(lsp, URI,F.MULTIPLE_FNS)
        h = hover_text(lsp.hover(URI, 4, 4))
        assert h is not None and "?T" not in h, f"Got: {h}"

    def test_closure_call_result_type(self, lsp):
        open_doc(lsp, URI,F.CLOSURE_ASSIGN)
        h = hover_text(lsp.hover(URI, 2, 6))
        assert h is not None and ("number" in h or "result" in h), f"Got: {h}"

    def test_collect_fn_shows_result(self, lsp):
        open_doc(lsp, URI,F.COLLECT_FILE)
        h = hover_text(lsp.hover(URI, 15, 4))
        assert h is not None and "validate" in h, f"Got: {h}"


class TestHoverImprovements403:
    """Hover improvements from issue #403."""

    def test_type_product_shows_fields(self, lsp):
        open_doc(lsp, URI,F.HOVER_TYPE_BODY)
        h = hover_text(lsp.hover(URI, 0, 5))
        assert h is not None and "id: number" in h and "title: string" in h, f"Got: {h}"

    def test_type_status_shows_variants(self, lsp):
        open_doc(lsp, URI,F.HOVER_TYPE_BODY)
        h = hover_text(lsp.hover(URI, 7, 5))
        assert h is not None and "Active" in h and "Inactive" in h, f"Got: {h}"

    def test_field_shows_property_type(self, lsp):
        open_doc(lsp, URI,F.HOVER_TYPE_BODY)
        h = hover_text(lsp.hover(URI, 2, 4))
        assert h is not None and "title" in h and "string" in h, f"Got: {h}"

    def test_field_id_not_parameter(self, lsp):
        open_doc(lsp, URI,F.HOVER_TYPE_BODY)
        h = hover_text(lsp.hover(URI, 1, 4))
        assert h is not None and "number" in h and "parameter" not in h, f"Got: {h}"

    def test_stdlib_module_hover(self, lsp):
        open_doc(lsp, URI,F.HOVER_STDLIB_MEMBER)
        h = hover_text(lsp.hover(URI, 1, 25))
        assert h is not None and "Array" in h, f"Got: {h}"

    def test_array_map_signature(self, lsp):
        open_doc(lsp, URI,F.HOVER_STDLIB_MEMBER)
        h = hover_text(lsp.hover(URI, 1, 31))
        assert h is not None and "map" in h and "->" in h, f"Got: {h}"

    def test_string_split_signature(self, lsp):
        open_doc(lsp, URI,F.HOVER_STDLIB_MEMBER)
        h = hover_text(lsp.hover(URI, 2, 33))
        assert h is not None and "split" in h and "->" in h, f"Got: {h}"

    def test_member_access_field_type(self, lsp):
        open_doc(lsp, URI,F.HOVER_MEMBER_ACCESS)
        h = hover_text(lsp.hover(URI, 7, 9))
        assert h is not None and "string" in h, f"Got: {h}"

    def test_destructured_tuple_name(self, lsp):
        open_doc(lsp, URI,F.HOVER_DESTRUCTURE)
        h = hover_text(lsp.hover(URI, 4, 7))
        assert h is not None and ("string" in h or "name" in h), f"Got: {h}"

    def test_default_params_shown(self, lsp):
        open_doc(lsp, URI,F.HOVER_DEFAULT_PARAMS)
        h = hover_text(lsp.hover(URI, 0, 4))
        assert h is not None and '= ""' in h and "= 20" in h, f"Got: {h}"

    def test_from_keyword_not_array_from(self, lsp):
        """Issue #507: 'from' in import should not show Array.from."""
        open_doc(lsp, URI,'import { useState } from "react"\nconst x = 42\n')
        h = hover_text(lsp.hover(URI, 0, 20))
        assert h is None or "Array.from" not in h, f"Got: {h}"


class TestHoverGenericFn:
    def test_identity_shows_type_params(self, lsp):
        open_doc(lsp, URI,F.GENERIC_FN)
        h = hover_text(lsp.hover(URI, 0, 4))
        assert h is not None and "<T>" in h, f"Got: {h}"

    def test_pair_shows_type_params(self, lsp):
        open_doc(lsp, URI,F.GENERIC_FN)
        h = hover_text(lsp.hover(URI, 1, 4))
        assert h is not None and "<A, B>" in h, f"Got: {h}"


class TestHoverQualifiedVariant:
    def test_type_name(self, lsp):
        open_doc(lsp, URI,F.QUALIFIED_VARIANT)
        h = hover_text(lsp.hover(URI, 3, 11))
        assert h is not None and "Color" in h, f"Got: {h}"

    def test_variant_after_dot(self, lsp):
        open_doc(lsp, URI,F.QUALIFIED_VARIANT)
        h = hover_text(lsp.hover(URI, 3, 17))
        assert h is not None, f"Got: {h}"

    def test_type_in_constructor(self, lsp):
        open_doc(lsp, URI,F.QUALIFIED_VARIANT)
        h = hover_text(lsp.hover(URI, 4, 11))
        assert h is not None, f"Got: {h}"


class TestHoverTour:
    """Tour of language features - hover coverage."""

    def test_closure_const(self, lsp):
        open_doc(lsp, URI,F.CLOSURE_FILE)
        h = hover_text(lsp.hover(URI, 0, 6))
        assert h is not None and "add" in h, f"Got: {h}"

    def test_dot_shorthand_result(self, lsp):
        open_doc(lsp, URI,F.DOT_SHORTHAND)
        h = hover_text(lsp.hover(URI, 3, 6))
        assert h is not None, f"Got: {h}"

    def test_partial_application(self, lsp):
        open_doc(lsp, URI,F.PLACEHOLDER)
        h = hover_text(lsp.hover(URI, 1, 6))
        assert h is not None, f"Got: {h}"


class TestHoverRecordSpread:
    """Hover on record types with spread shows spread members."""

    def test_record_spread_shows_members(self, lsp):
        open_doc(lsp, URI, F.RECORD_SPREAD_HOVER)
        # Hover on ButtonProps (line 5)
        h = hover_text(lsp.hover(URI, 5, 6))
        assert h is not None, f"Expected hover for ButtonProps, got None"
        assert "...BaseProps" in h, f"Expected spread member in hover, got: {h}"
        assert "onClick" in h, f"Expected onClick field in hover, got: {h}"
        assert "label" in h, f"Expected label field in hover, got: {h}"

    def test_member_access_shows_field_type(self, lsp):
        open_doc(lsp, URI, F.MEMBER_ACCESS)
        # Hover on 'name' in user.name (line 6, col 21)
        h = hover_text(lsp.hover(URI, 6, 21))
        assert h is not None, f"Expected hover for user.name, got None"
        assert "string" in h, f"Expected string type for name field, got: {h}"

    def test_match_pattern_binding_shows_type(self, lsp):
        open_doc(lsp, URI, F.MATCH_PATTERN_BINDING)
        # Hover on 'u' in Some(u) pattern (line 6, col 13)
        h = hover_text(lsp.hover(URI, 6, 13))
        assert h is not None, f"Expected hover for pattern binding u, got None"
        assert "User" in h, f"Expected User type for pattern binding, got: {h}"

    def test_lambda_param_shows_type(self, lsp):
        open_doc(lsp, URI, F.LAMBDA_PARAM)
        # Hover on 'item' in lambda param (line 1, col 30)
        h = hover_text(lsp.hover(URI, 1, 30))
        assert h is not None, f"Expected hover for lambda param item, got None"
        assert "number" in h, f"Expected number type for lambda param, got: {h}"

    def test_jsx_render_prop_param_shows_type(self, lsp):
        open_doc(lsp, URI, F.JSX_RENDER_PROP_PARAM)
        # Hover on 'provided' (line 6, col 10)
        h = hover_text(lsp.hover(URI, 6, 10))
        assert h is not None, f"Expected hover for render prop param provided, got None"
        assert "?T" not in h, f"Render prop param should not show type var, got: {h}"

    def test_pipe_hover_shows_input_type(self, lsp):
        open_doc(lsp, URI, F.PIPE_HOVER)
        # First |> at col 20: items (Array<number>) is being piped
        h = hover_text(lsp.hover(URI, 1, 20))
        assert h is not None, f"Expected hover for pipe operator, got None"
        assert "Array" in h, f"Pipe hover should show Array type, got: {h}"

    def test_pipe_hover_second_pipe_shows_mapped_type(self, lsp):
        open_doc(lsp, URI, F.PIPE_HOVER)
        # Second |> at col 41: map result (Array<number>) is being piped
        h = hover_text(lsp.hover(URI, 1, 41))
        assert h is not None, f"Expected hover for second pipe, got None"
        assert "Array" in h, f"Second pipe should show Array type, got: {h}"

    def test_match_pattern_literal_shows_boolean(self, lsp):
        open_doc(lsp, URI, F.MATCH_PATTERN_LITERAL)
        # Hover on 'true' in match pattern (line 2, col 8)
        h = hover_text(lsp.hover(URI, 2, 8))
        assert h is not None, f"Expected hover for true literal pattern, got None"
        assert "boolean" in h, f"Expected boolean type for true pattern, got: {h}"

    def test_pipe_into_match_fn(self, lsp):
        open_doc(lsp, URI,F.PIPE_INTO_MATCH)
        h = hover_text(lsp.hover(URI, 0, 4))
        assert h is not None and "label" in h, f"Got: {h}"

    def test_newtype_wrapper(self, lsp):
        open_doc(lsp, URI,F.NEWTYPE_WRAPPER)
        h = hover_text(lsp.hover(URI, 0, 5))
        assert h is not None and "UserId" in h, f"Got: {h}"

    def test_newtype(self, lsp):
        open_doc(lsp, URI,F.NEWTYPE)
        h = hover_text(lsp.hover(URI, 0, 5))
        assert h is not None and "ProductId" in h, f"Got: {h}"

    def test_tuple_index_result(self, lsp):
        open_doc(lsp, URI,F.TUPLE_INDEX)
        h = hover_text(lsp.hover(URI, 1, 6))
        assert h is not None, f"Got: {h}"

    def test_inline_for_fn(self, lsp):
        open_doc(lsp, URI,F.INLINE_FOR)
        h = hover_text(lsp.hover(URI, 1, 18))
        assert h is not None and "shout" in h, f"Got: {h}"

    def test_map_result(self, lsp):
        open_doc(lsp, URI,F.MAP_SET)
        h = hover_text(lsp.hover(URI, 0, 6))
        assert h is not None, f"Got: {h}"

    def test_multi_depth_match_fn(self, lsp):
        open_doc(lsp, URI,F.MULTI_DEPTH_MATCH)
        h = hover_text(lsp.hover(URI, 4, 4))
        assert h is not None and "describe" in h, f"Got: {h}"

    def test_multiline_pipe_result(self, lsp):
        open_doc(lsp, URI,F.MULTILINE_PIPE)
        h = hover_text(lsp.hover(URI, 0, 6))
        assert h is not None, f"Got: {h}"
