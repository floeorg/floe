"""Tests for textDocument/codeAction."""

import pytest

from .conftest import URI, diag_all, open_and_diagnose
from . import fixtures as F


class TestCodeActions:
    def test_missing_return_type_has_actions(self, lsp):
        """E010: exported fn without return type should offer a fix."""
        result = open_and_diagnose(lsp, URI, F.CODE_ACTION)
        resp = lsp.code_action(URI, 0, diagnostics=result.all)
        actions = (resp.get("result") or []) if resp else []
        assert len(actions) > 0, f"Expected code actions for E010"

    def test_add_return_type_fix(self, lsp):
        result = open_and_diagnose(lsp, URI, F.CODE_ACTION)
        resp = lsp.code_action(URI, 0, diagnostics=result.all)
        actions = (resp.get("result") or []) if resp else []
        titles = [a.get("title", "") for a in actions]
        assert any("return type" in t.lower() or "-> " in t for t in titles), f"Titles: {titles}"

    def test_valid_code_no_actions(self, lsp):
        open_and_diagnose(lsp, URI, F.SIMPLE)
        resp = lsp.code_action(URI, 0)
        actions = (resp.get("result") or []) if resp else []
        assert len(actions) == 0, f"Got {len(actions)} unexpected actions"
