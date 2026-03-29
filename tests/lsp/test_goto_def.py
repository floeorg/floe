"""Tests for textDocument/definition."""

import pytest

from .conftest import URI, def_locations, open_and_diagnose
from . import fixtures as F


def _open(lsp, source, timeout=1.0):
    open_and_diagnose(lsp, URI, source, timeout=timeout)


class TestGotoDefBasic:
    def test_fn_usage_to_definition(self, lsp):
        _open(lsp, F.GOTO_DEF)
        locs = def_locations(lsp.goto_definition(URI, 4, 15))
        assert len(locs) > 0

    def test_fn_jumps_to_correct_line(self, lsp):
        _open(lsp, F.GOTO_DEF)
        locs = def_locations(lsp.goto_definition(URI, 4, 15))
        assert locs
        target_line = locs[0].get("range", {}).get("start", {}).get("line", -1)
        assert target_line == 0, f"Expected line 0, got {target_line}"

    def test_type_usage_to_definition(self, lsp):
        _open(lsp, F.TYPES + "\nfn pick(c: Color) -> string { \"ok\" }\n")
        locs = def_locations(lsp.goto_definition(URI, 20, 11))
        assert len(locs) > 0

    def test_keyword_returns_empty(self, lsp):
        _open(lsp, F.SIMPLE)
        locs = def_locations(lsp.goto_definition(URI, 0, 1))  # "const"
        assert len(locs) == 0


class TestGotoDefAdvanced:
    def test_union_variant_in_match(self, lsp):
        _open(lsp, F.TYPES)
        locs = def_locations(lsp.goto_definition(URI, 14, 8))  # "Red" in match
        assert len(locs) > 0

    def test_const_variable_usage(self, lsp):
        _open(lsp, F.MULTIPLE_FNS)
        locs = def_locations(lsp.goto_definition(URI, 5, 17))  # "a" in second(a)
        assert len(locs) > 0

    @pytest.mark.parametrize(
        "char,name",
        [(10, "first"), (16, "second"), (23, "third")],
    )
    def test_fn_in_nested_call(self, lsp, char, name):
        _open(lsp, F.MULTIPLE_FNS)
        # line 7: const d = first(second(third(0)))
        locs = def_locations(lsp.goto_definition(URI, 7, char))
        assert len(locs) > 0, f"Expected goto def for {name}"

    def test_type_in_parameter(self, lsp):
        _open(lsp, F.RECORD_SPREAD)
        locs = def_locations(lsp.goto_definition(URI, 6, 20))  # User in parameter
        assert len(locs) > 0

    def test_type_in_return_annotation(self, lsp):
        _open(lsp, F.RECORD_SPREAD)
        locs = def_locations(lsp.goto_definition(URI, 6, 47))  # -> User
        assert len(locs) > 0


class TestGotoDefQualifiedVariant:
    def test_type_name(self, lsp):
        _open(lsp, F.QUALIFIED_VARIANT)
        locs = def_locations(lsp.goto_definition(URI, 3, 11))
        assert len(locs) > 0
