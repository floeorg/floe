"""Tests for textDocument/codeAction."""

from .conftest import URI, result_list, open_doc
from . import fixtures as F


class TestCodeActions:
    def test_missing_return_type_has_actions(self, lsp):
        """E010: exported fn without return type should offer a fix."""
        result = open_doc(lsp, URI, F.CODE_ACTION)
        actions = result_list(lsp.code_action(URI, 0, diagnostics=result.all))
        assert len(actions) > 0, "Expected code actions for E010"

    def test_add_return_type_fix(self, lsp):
        result = open_doc(lsp, URI, F.CODE_ACTION)
        actions = result_list(lsp.code_action(URI, 0, diagnostics=result.all))
        titles = [a.get("title", "") for a in actions]
        assert any("return type" in t.lower() or "-> " in t for t in titles), f"Titles: {titles}"

    def test_valid_code_no_actions(self, lsp):
        open_doc(lsp, URI, F.SIMPLE)
        actions = result_list(lsp.code_action(URI, 0))
        assert len(actions) == 0, f"Got {len(actions)} unexpected actions"
