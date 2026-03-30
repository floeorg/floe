"""Tests for edge cases, error recovery, and rapid document updates."""

from .conftest import URI, completion_labels, symbol_names, open_doc
from . import fixtures as F


class TestEdgeCases:
    def test_empty_file_no_crash(self, lsp):
        result = open_doc(lsp, URI, F.EMPTY_FILE)
        assert result.errors == []

    def test_comment_only_no_errors(self, lsp):
        result = open_doc(lsp, URI, F.SINGLE_COMMENT)
        assert result.errors == []

    def test_hover_on_empty_file(self, lsp):
        open_doc(lsp, URI, F.EMPTY_FILE)
        resp = lsp.hover(URI, 0, 0)
        assert resp is not None, "Server should respond to hover on empty file"

    def test_completion_on_comment_only(self, lsp):
        open_doc(lsp, URI, F.SINGLE_COMMENT)
        labels = completion_labels(lsp.completion(URI, 0, 0))
        assert len(labels) > 0

    def test_symbols_on_empty_file(self, lsp):
        open_doc(lsp, URI, F.EMPTY_FILE)
        names = symbol_names(lsp.document_symbols(URI))
        assert names == []


class TestRapidUpdates:
    """Simulate typing by rapidly opening documents with partial content."""

    def test_survives_rapid_edits(self, lsp):
        base = "const x = "
        for i, char in enumerate("42"):
            lsp.open_doc(URI, base + "42"[: i + 1])
        lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
        # No crash = pass

    def test_partial_fn_typing(self, lsp):
        stages = [
            "fn ",
            "fn test",
            "fn test(",
            "fn test() ",
            "fn test() {",
            "fn test() { 42 }",
        ]
        for stage in stages:
            lsp.open_doc(URI, stage)
        lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
        # No crash = pass

    def test_hover_right_after_open(self, lsp):
        lsp.open_doc(URI, F.SIMPLE)
        resp = lsp.hover(URI, 0, 6)
        assert resp is not None
