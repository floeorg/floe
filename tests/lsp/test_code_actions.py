"""Tests for textDocument/codeAction."""

from .conftest import URI, result_list, open_doc
from . import fixtures as F


class TestCodeActions:
    def test_no_actions_when_return_type_missing(self, lsp):
        """Return-type annotations are optional in Floe — the old
        "add inferred return type" quickfix is no longer offered."""
        result = open_doc(lsp, URI, F.CODE_ACTION)
        actions = result_list(lsp.code_action(URI, 0, diagnostics=result.all))
        assert actions == [], f"Expected no code actions, got {actions}"

    def test_valid_code_no_actions(self, lsp):
        open_doc(lsp, URI, F.SIMPLE)
        actions = result_list(lsp.code_action(URI, 0))
        assert len(actions) == 0, f"Got {len(actions)} unexpected actions"
