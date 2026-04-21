"""Tests for textDocument/definition."""

import pytest

from .conftest import URI, at, def_locations, open_doc
from . import fixtures as F


class TestGotoDefBasic:
    def test_fn_usage_to_definition(self, lsp):
        open_doc(lsp, URI,F.GOTO_DEF)
        locs = def_locations(lsp.goto_definition(URI, 4, 15))
        assert len(locs) > 0

    def test_fn_jumps_to_correct_line(self, lsp):
        open_doc(lsp, URI,F.GOTO_DEF)
        locs = def_locations(lsp.goto_definition(URI, 4, 15))
        assert locs
        target_line = locs[0].get("range", {}).get("start", {}).get("line", -1)
        assert target_line == 0, f"Expected line 0, got {target_line}"

    def test_type_usage_to_definition(self, lsp):
        open_doc(lsp, URI,F.TYPES + "\nfn pick(c: Color) -> string { \"ok\" }\n")
        locs = def_locations(lsp.goto_definition(URI, 12, 11))
        assert len(locs) > 0

    def test_keyword_returns_empty(self, lsp):
        open_doc(lsp, URI,F.SIMPLE)
        locs = def_locations(lsp.goto_definition(URI, 0, 1))  # "const"
        assert len(locs) == 0


class TestGotoDefTaggedTemplate:
    def test_tag_jumps_to_fn(self, lsp):
        open_doc(lsp, URI, F.TAGGED_TEMPLATE)
        # `sql` in `sql`select ...`` is at line 5, column 10
        locs = def_locations(lsp.goto_definition(URI, 5, 10))
        assert len(locs) > 0, "expected tag identifier to resolve to its definition"


class TestGotoDefAdvanced:
    def test_union_variant_in_match(self, lsp):
        open_doc(lsp, URI,F.TYPES)
        locs = def_locations(lsp.goto_definition(URI, 6, 8))  # "Red" in match
        assert len(locs) > 0

    def test_const_variable_usage(self, lsp):
        open_doc(lsp, URI,F.MULTIPLE_FNS)
        locs = def_locations(lsp.goto_definition(URI, 5, 15))  # "a" in second(a)
        assert len(locs) > 0

    @pytest.mark.parametrize(
        "char,name",
        [(10, "first"), (16, "second"), (23, "third")],
    )
    def test_fn_in_nested_call(self, lsp, char, name):
        open_doc(lsp, URI,F.MULTIPLE_FNS)
        # line 7: d = first(second(third(0)))
        locs = def_locations(lsp.goto_definition(URI, 7, char))
        assert len(locs) > 0, f"Expected goto def for {name}"

    def test_type_in_parameter(self, lsp):
        open_doc(lsp, URI,F.RECORD_SPREAD)
        locs = def_locations(lsp.goto_definition(URI, 6, 21))  # User in parameter
        assert len(locs) > 0

    def test_type_in_return_annotation(self, lsp):
        open_doc(lsp, URI,F.RECORD_SPREAD)
        locs = def_locations(lsp.goto_definition(URI, 6, 47))  # -> User
        assert len(locs) > 0


class TestGotoDefQualifiedVariant:
    def test_type_name(self, lsp):
        open_doc(lsp, URI,F.QUALIFIED_VARIANT)
        locs = def_locations(lsp.goto_definition(URI, 3, 11))
        assert len(locs) > 0


class TestGotoDefDefaultExport:
    """#1297: clicking the identifier in `export default foo` should jump to
    the `let foo = ...` declaration."""

    SRC = (
        'let app = 42\n'
        'export default app\n'
    )

    def test_default_export_jumps_to_binding(self, lsp):
        open_doc(lsp, URI, self.SRC)
        line, col = at(self.SRC, "app", nth=1)
        locs = def_locations(lsp.goto_definition(URI, line, col + 1))
        assert len(locs) > 0, "Expected goto-def from default export to resolve"
        target_line = locs[0].get("range", {}).get("start", {}).get("line", -1)
        assert target_line == 0, f"Expected jump to line 0 (the `let app` binding), got {target_line}"
