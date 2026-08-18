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
    """Simulate typing by rapidly opening documents with partial content.

    Each test here opens a document several times and asks only that the
    server survives it. That leaves one publish per open, and every test
    in the session shares one notification buffer, so each test drains
    to quiet before it ends. A publish left behind is read by the next
    test as if it described that test's own document.

    The URI is this class's own for the same reason. `conftest.URI` is
    the document a dozen other test files open, so a publish that
    escapes this class under that name is indistinguishable from theirs.
    """

    URI = "file:///tmp/rapid_updates.fl"

    def test_survives_rapid_edits(self, lsp):
        base = "x = "
        for i, char in enumerate("42"):
            lsp.open_doc(self.URI, base + "42"[: i + 1])
        lsp.drain_notifications("textDocument/publishDiagnostics")
        # No crash = pass

    def test_partial_fn_typing(self, lsp):
        stages = [
            "let ",
            "let test",
            "let test(",
            "let test() ",
            "let test() {",
            "let test() { 42 }",
        ]
        for stage in stages:
            lsp.open_doc(self.URI, stage)
        lsp.drain_notifications("textDocument/publishDiagnostics")
        # No crash = pass

    def test_hover_right_after_open(self, lsp):
        lsp.open_doc(self.URI, F.SIMPLE)
        resp = lsp.hover(self.URI, 0, 6)
        lsp.drain_notifications("textDocument/publishDiagnostics")
        assert resp is not None
