"""Tests for textDocument/hover."""

import pytest

from .conftest import URI, hover_text, open_and_diagnose
from . import fixtures as F


def _open(lsp, source, timeout=1.0):
    """Open a document and drain diagnostics."""
    open_and_diagnose(lsp, URI, source, timeout=timeout)


class TestHoverBasic:
    """Hover on constants, functions, and types."""

    def test_const_number(self, lsp):
        _open(lsp, F.SIMPLE)
        h = hover_text(lsp.hover(URI, 0, 6))
        assert h is not None and "number" in h, f"Expected number type, got: {h}"

    def test_const_string(self, lsp):
        _open(lsp, F.SIMPLE)
        h = hover_text(lsp.hover(URI, 1, 6))
        assert h is not None and "string" in h, f"Expected string type, got: {h}"

    def test_const_boolean(self, lsp):
        _open(lsp, F.SIMPLE)
        h = hover_text(lsp.hover(URI, 2, 6))
        assert h is not None and ("boolean" in h or "bool" in h), f"Expected boolean type, got: {h}"

    def test_fn_signature(self, lsp):
        _open(lsp, F.SIMPLE)
        h = hover_text(lsp.hover(URI, 4, 3))
        assert h is not None and "fn add" in h, f"Expected fn add signature, got: {h}"

    def test_export_fn_signature(self, lsp):
        _open(lsp, F.SIMPLE)
        h = hover_text(lsp.hover(URI, 8, 11))
        assert h is not None and "greet" in h, f"Expected greet signature, got: {h}"

    def test_whitespace_returns_null(self, lsp):
        _open(lsp, F.SIMPLE)
        resp = lsp.hover(URI, 3, 0)
        assert resp is not None and resp.get("result") is None

    def test_type_user(self, lsp):
        _open(lsp, F.TYPES)
        h = hover_text(lsp.hover(URI, 6, 5))
        assert h is not None and "User" in h, f"Got: {h}"

    def test_union_variant(self, lsp):
        _open(lsp, F.TYPES)
        h = hover_text(lsp.hover(URI, 1, 6))
        assert h is not None, f"Expected hover on variant Red, got: {h}"

    def test_fn_describeColor(self, lsp):
        _open(lsp, F.TYPES)
        h = hover_text(lsp.hover(URI, 12, 5))
        assert h is not None and "describeColor" in h, f"Got: {h}"

    def test_builtin_trim(self, lsp):
        _open(lsp, F.PIPES)
        h = hover_text(lsp.hover(URI, 6, 11))
        assert h is not None and "trim" in h.lower(), f"Got: {h}"


class TestHoverForBlock:
    def test_forblock_fn(self, lsp):
        _open(lsp, F.FORBLOCK)
        h = hover_text(lsp.hover(URI, 6, 18))
        assert h is not None and "remaining" in h, f"Got: {h}"


class TestHoverResult:
    def test_ok_builtin(self, lsp):
        _open(lsp, F.RESULT)
        h = hover_text(lsp.hover(URI, 3, 14))
        assert h is not None and "ok" in h.lower(), f"Got: {h}"

    def test_err_builtin(self, lsp):
        _open(lsp, F.RESULT)
        h = hover_text(lsp.hover(URI, 2, 14))
        assert h is not None and "err" in h.lower(), f"Got: {h}"

    def test_fn_before_question_mark(self, lsp):
        _open(lsp, F.RESULT)
        h = hover_text(lsp.hover(URI, 8, 23))
        assert h is not None and "divide" in (h or ""), f"Got: {h}"


class TestHoverAdvanced:
    def test_fn_parameter(self, lsp):
        _open(lsp, F.FN_PARAMS_HOVER)
        h = hover_text(lsp.hover(URI, 0, 12))
        assert h is not None, f"Expected hover on parameter, got: {h}"

    def test_nested_match_fn(self, lsp):
        _open(lsp, F.NESTED_MATCH)
        h = hover_text(lsp.hover(URI, 10, 3))
        assert h is not None and "describe" in h, f"Got: {h}"

    def test_spread_type(self, lsp):
        _open(lsp, F.SPREAD_FILE)
        h = hover_text(lsp.hover(URI, 5, 5))
        assert h is not None and "Extended" in h, f"Got: {h}"

    def test_closure_const(self, lsp):
        _open(lsp, F.CLOSURE_ASSIGN)
        h = hover_text(lsp.hover(URI, 0, 6))
        assert h is not None and "add" in h, f"Got: {h}"

    def test_closure_double(self, lsp):
        _open(lsp, F.CLOSURE_ASSIGN)
        h = hover_text(lsp.hover(URI, 1, 6))
        assert h is not None and "double" in h, f"Got: {h}"

    def test_closure_call_result(self, lsp):
        _open(lsp, F.CLOSURE_ASSIGN)
        h = hover_text(lsp.hover(URI, 2, 6))
        assert h is not None, f"Got: {h}"

    def test_todo_keyword(self, lsp):
        _open(lsp, F.TODO_UNREACHABLE)
        h = hover_text(lsp.hover(URI, 1, 4))
        assert h is not None, f"Got: {h}"

    def test_none_literal(self, lsp):
        _open(lsp, F.OPTION_FILE)
        h = hover_text(lsp.hover(URI, 2, 15))
        assert h is not None and "none" in h.lower(), f"Got: {h}"

    def test_some_literal(self, lsp):
        _open(lsp, F.OPTION_FILE)
        h = hover_text(lsp.hover(URI, 3, 27))
        assert h is not None and "some" in h.lower(), f"Got: {h}"

    def test_match_keyword(self, lsp):
        _open(lsp, F.WHEN_GUARD)
        h = hover_text(lsp.hover(URI, 1, 5))
        assert h is not None and "match" in h.lower(), f"Got: {h}"

    def test_variable_in_spread(self, lsp):
        _open(lsp, F.RECORD_SPREAD)
        h = hover_text(lsp.hover(URI, 7, 15))
        assert h is not None, f"Got: {h}"

    def test_const_from_fn_call(self, lsp):
        _open(lsp, F.MULTIPLE_FNS)
        h = hover_text(lsp.hover(URI, 4, 6))
        assert h is not None and "number" in h, f"Got: {h}"

    def test_nested_fn_call_result(self, lsp):
        _open(lsp, F.MULTIPLE_FNS)
        h = hover_text(lsp.hover(URI, 7, 6))
        assert h is not None and "number" in h, f"Got: {h}"

    def test_fn_tuple_return(self, lsp):
        _open(lsp, F.TUPLE_FILE)
        h = hover_text(lsp.hover(URI, 0, 3))
        assert h is not None and "swap" in h, f"Got: {h}"

    def test_const_assigned_tuple(self, lsp):
        _open(lsp, F.TUPLE_FILE)
        h = hover_text(lsp.hover(URI, 4, 6))
        assert h is not None, f"Got: {h}"

    def test_destructured_tuple_var(self, lsp):
        _open(lsp, F.TUPLE_FILE)
        h = hover_text(lsp.hover(URI, 5, 7))
        assert h is not None, f"Got: {h}"

    def test_trait_impl_fn(self, lsp):
        _open(lsp, F.TRAIT_FILE)
        h = hover_text(lsp.hover(URI, 10, 7))
        assert h is not None and "print" in h, f"Got: {h}"

    def test_inner_const(self, lsp):
        _open(lsp, F.INNER_CONST)
        h = hover_text(lsp.hover(URI, 1, 10))
        assert h is not None, f"Got: {h}"

    def test_inner_const_doubled(self, lsp):
        _open(lsp, F.INNER_CONST)
        h = hover_text(lsp.hover(URI, 2, 10))
        assert h is not None, f"Got: {h}"


class TestHoverPipeMapInference:
    def test_pipe_map_result_type(self, lsp):
        _open(lsp, F.PIPE_MAP_INFERENCE)
        h = hover_text(lsp.hover(URI, 10, 10))
        assert h is not None and "Accent" in h, f"Got: {h}"

    def test_forblock_fn_in_pipe(self, lsp):
        _open(lsp, F.PIPE_MAP_INFERENCE)
        h = hover_text(lsp.hover(URI, 10, 48))
        assert h is not None and "Accent" in h, f"Got: {h}"


class TestHoverTypeQuality:
    """Hover should show concrete types, not 'unknown' or '?T'."""

    def test_no_unknown(self, lsp):
        _open(lsp, F.MULTIPLE_FNS)
        h = hover_text(lsp.hover(URI, 4, 6))
        assert h is not None and "unknown" not in h.lower(), f"Got: {h}"

    def test_no_type_var(self, lsp):
        _open(lsp, F.MULTIPLE_FNS)
        h = hover_text(lsp.hover(URI, 4, 6))
        assert h is not None and "?T" not in h, f"Got: {h}"

    def test_closure_call_result_type(self, lsp):
        _open(lsp, F.CLOSURE_ASSIGN)
        h = hover_text(lsp.hover(URI, 2, 6))
        assert h is not None and ("number" in h or "result" in h), f"Got: {h}"

    def test_collect_fn_shows_result(self, lsp):
        _open(lsp, F.COLLECT_FILE)
        h = hover_text(lsp.hover(URI, 15, 3))
        assert h is not None and "validate" in h, f"Got: {h}"


class TestHoverImprovements403:
    """Hover improvements from issue #403."""

    def test_type_product_shows_fields(self, lsp):
        _open(lsp, F.HOVER_TYPE_BODY)
        h = hover_text(lsp.hover(URI, 0, 5))
        assert h is not None and "id: number" in h and "title: string" in h, f"Got: {h}"

    def test_type_status_shows_variants(self, lsp):
        _open(lsp, F.HOVER_TYPE_BODY)
        h = hover_text(lsp.hover(URI, 7, 5))
        assert h is not None and "Active" in h and "Inactive" in h, f"Got: {h}"

    def test_field_shows_property_type(self, lsp):
        _open(lsp, F.HOVER_TYPE_BODY)
        h = hover_text(lsp.hover(URI, 2, 4))
        assert h is not None and "title" in h and "string" in h, f"Got: {h}"

    def test_field_id_not_parameter(self, lsp):
        _open(lsp, F.HOVER_TYPE_BODY)
        h = hover_text(lsp.hover(URI, 1, 4))
        assert h is not None and "number" in h and "parameter" not in h, f"Got: {h}"

    def test_stdlib_module_hover(self, lsp):
        _open(lsp, F.HOVER_STDLIB_MEMBER)
        h = hover_text(lsp.hover(URI, 1, 25))
        assert h is not None and "Array" in h, f"Got: {h}"

    def test_array_map_signature(self, lsp):
        _open(lsp, F.HOVER_STDLIB_MEMBER)
        h = hover_text(lsp.hover(URI, 1, 31))
        assert h is not None and "map" in h and "->" in h, f"Got: {h}"

    def test_string_split_signature(self, lsp):
        _open(lsp, F.HOVER_STDLIB_MEMBER)
        h = hover_text(lsp.hover(URI, 2, 38))
        assert h is not None and "split" in h and "->" in h, f"Got: {h}"

    def test_member_access_field_type(self, lsp):
        _open(lsp, F.HOVER_MEMBER_ACCESS)
        h = hover_text(lsp.hover(URI, 7, 9))
        assert h is not None and "string" in h, f"Got: {h}"

    def test_destructured_tuple_name(self, lsp):
        _open(lsp, F.HOVER_DESTRUCTURE)
        h = hover_text(lsp.hover(URI, 4, 7))
        assert h is not None and ("string" in h or "name" in h), f"Got: {h}"

    def test_default_params_shown(self, lsp):
        _open(lsp, F.HOVER_DEFAULT_PARAMS)
        h = hover_text(lsp.hover(URI, 0, 3))
        assert h is not None and '= ""' in h and "= 20" in h, f"Got: {h}"

    def test_from_keyword_not_array_from(self, lsp):
        """Issue #507: 'from' in import should not show Array.from."""
        _open(lsp, 'import { useState } from "react"\nconst x = 42\n')
        h = hover_text(lsp.hover(URI, 0, 20))
        assert h is None or "Array.from" not in h, f"Got: {h}"


class TestHoverGenericFn:
    def test_identity_shows_type_params(self, lsp):
        _open(lsp, F.GENERIC_FN)
        h = hover_text(lsp.hover(URI, 0, 3))
        assert h is not None and "<T>" in h, f"Got: {h}"

    def test_pair_shows_type_params(self, lsp):
        _open(lsp, F.GENERIC_FN)
        h = hover_text(lsp.hover(URI, 1, 3))
        assert h is not None and "<A, B>" in h, f"Got: {h}"


class TestHoverQualifiedVariant:
    def test_type_name(self, lsp):
        _open(lsp, F.QUALIFIED_VARIANT)
        h = hover_text(lsp.hover(URI, 3, 11))
        assert h is not None and "Color" in h, f"Got: {h}"

    def test_variant_after_dot(self, lsp):
        _open(lsp, F.QUALIFIED_VARIANT)
        h = hover_text(lsp.hover(URI, 3, 17))
        assert h is not None, f"Got: {h}"

    def test_type_in_constructor(self, lsp):
        _open(lsp, F.QUALIFIED_VARIANT)
        h = hover_text(lsp.hover(URI, 4, 11))
        assert h is not None, f"Got: {h}"


class TestHoverTour:
    """Tour of language features - hover coverage."""

    def test_closure_const(self, lsp):
        _open(lsp, F.CLOSURE_FILE)
        h = hover_text(lsp.hover(URI, 0, 6))
        assert h is not None and "add" in h, f"Got: {h}"

    def test_dot_shorthand_result(self, lsp):
        _open(lsp, F.DOT_SHORTHAND)
        h = hover_text(lsp.hover(URI, 3, 6))
        assert h is not None, f"Got: {h}"

    def test_partial_application(self, lsp):
        _open(lsp, F.PLACEHOLDER)
        h = hover_text(lsp.hover(URI, 1, 6))
        assert h is not None, f"Got: {h}"

    def test_pipe_into_match_fn(self, lsp):
        _open(lsp, F.PIPE_INTO_MATCH)
        h = hover_text(lsp.hover(URI, 0, 3))
        assert h is not None and "label" in h, f"Got: {h}"

    def test_newtype_wrapper(self, lsp):
        _open(lsp, F.NEWTYPE_WRAPPER)
        h = hover_text(lsp.hover(URI, 0, 5))
        assert h is not None and "UserId" in h, f"Got: {h}"

    def test_newtype(self, lsp):
        _open(lsp, F.NEWTYPE)
        h = hover_text(lsp.hover(URI, 0, 5))
        assert h is not None and "ProductId" in h, f"Got: {h}"

    def test_tuple_index_result(self, lsp):
        _open(lsp, F.TUPLE_INDEX)
        h = hover_text(lsp.hover(URI, 1, 6))
        assert h is not None, f"Got: {h}"

    def test_inline_for_fn(self, lsp):
        _open(lsp, F.INLINE_FOR)
        h = hover_text(lsp.hover(URI, 1, 18))
        assert h is not None and "shout" in h, f"Got: {h}"

    def test_map_result(self, lsp):
        _open(lsp, F.MAP_SET)
        h = hover_text(lsp.hover(URI, 0, 6))
        assert h is not None, f"Got: {h}"

    def test_multi_depth_match_fn(self, lsp):
        _open(lsp, F.MULTI_DEPTH_MATCH)
        h = hover_text(lsp.hover(URI, 10, 3))
        assert h is not None and "describe" in h, f"Got: {h}"

    def test_multiline_pipe_result(self, lsp):
        _open(lsp, F.MULTILINE_PIPE)
        h = hover_text(lsp.hover(URI, 0, 6))
        assert h is not None, f"Got: {h}"
