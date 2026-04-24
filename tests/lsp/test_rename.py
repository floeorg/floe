"""Tests for textDocument/rename and textDocument/prepareRename."""

from __future__ import annotations

from .conftest import URI, at, open_doc
from . import fixtures as F


def edits_for(workspace_edit: dict | None, uri: str) -> list[dict]:
    if workspace_edit is None:
        return []
    result = workspace_edit.get("result")
    if not result:
        return []
    return result.get("changes", {}).get(uri, [])


def apply_edits(source: str, edits: list[dict]) -> str:
    """Apply LSP TextEdits to a source string. Edits are applied right-to-left
    so earlier offsets stay valid."""
    lines = source.split("\n")

    def offset(pos: dict) -> int:
        return sum(len(l) + 1 for l in lines[: pos["line"]]) + pos["character"]

    spans = sorted(
        ((offset(e["range"]["start"]), offset(e["range"]["end"]), e["newText"]) for e in edits),
        reverse=True,
    )
    result = source
    for start, end, new_text in spans:
        result = result[:start] + new_text + result[end:]
    return result


class TestPrepareRename:
    def test_returns_range_on_fn_definition(self, lsp):
        open_doc(lsp, URI, F.GOTO_DEF)
        line, col = at(F.GOTO_DEF, "add")
        resp = lsp.prepare_rename(URI, line, col)
        assert resp is not None and resp.get("result") is not None
        rng = resp["result"]
        assert rng["start"] == {"line": line, "character": col}
        assert rng["end"] == {"line": line, "character": col + len("add")}

    def test_returns_range_on_fn_usage(self, lsp):
        open_doc(lsp, URI, F.GOTO_DEF)
        # Second occurrence of "add" — the call site.
        line, col = at(F.GOTO_DEF, "add", nth=1)
        resp = lsp.prepare_rename(URI, line, col)
        assert resp is not None and resp.get("result") is not None
        rng = resp["result"]
        assert rng["start"] == {"line": line, "character": col}

    def test_returns_null_off_identifier(self, lsp):
        open_doc(lsp, URI, F.GOTO_DEF)
        # Position 0,0 lands on `let`, not on a tracked symbol.
        resp = lsp.prepare_rename(URI, 0, 0)
        assert resp is not None and resp.get("result") is None


class TestRename:
    def test_renames_fn_def_and_usage(self, lsp):
        open_doc(lsp, URI, F.GOTO_DEF)
        line, col = at(F.GOTO_DEF, "add")
        resp = lsp.rename(URI, line, col, "sum")
        edits = edits_for(resp, URI)
        assert len(edits) == 2, f"Expected def + 1 usage, got {len(edits)} edits"
        new_source = apply_edits(F.GOTO_DEF, edits)
        assert "let sum(a: number, b: number)" in new_source
        assert "sum(1, 2)" in new_source
        assert "add" not in new_source

    def test_rename_from_usage_site(self, lsp):
        open_doc(lsp, URI, F.GOTO_DEF)
        line, col = at(F.GOTO_DEF, "add", nth=1)
        resp = lsp.rename(URI, line, col, "sum")
        new_source = apply_edits(F.GOTO_DEF, edits_for(resp, URI))
        assert "add" not in new_source
        assert "let sum(" in new_source and "sum(1, 2)" in new_source

    def test_renames_const(self, lsp):
        open_doc(lsp, URI, F.MULTIPLE_FNS)
        line, col = at(F.MULTIPLE_FNS, "let a = ")
        # Cursor on the `a` of `let a = first(1)`.
        resp = lsp.rename(URI, line, col + 4, "alpha")
        new_source = apply_edits(F.MULTIPLE_FNS, edits_for(resp, URI))
        assert "let alpha = first(1)" in new_source
        assert "let b = second(alpha)" in new_source

    def test_renames_all_three_call_sites(self, lsp):
        open_doc(lsp, URI, F.MULTIPLE_FNS)
        line, col = at(F.MULTIPLE_FNS, "first")
        resp = lsp.rename(URI, line, col, "primero")
        edits = edits_for(resp, URI)
        # Definition + two call sites (let a = first(1), let d = first(...)).
        assert len(edits) == 3
        new_source = apply_edits(F.MULTIPLE_FNS, edits)
        assert "first" not in new_source
        assert new_source.count("primero") == 3

    def test_returns_null_off_identifier(self, lsp):
        open_doc(lsp, URI, F.GOTO_DEF)
        resp = lsp.rename(URI, 0, 0, "foo")
        assert resp is not None and resp.get("result") is None

    def test_invalid_new_name_returns_error(self, lsp):
        open_doc(lsp, URI, F.GOTO_DEF)
        line, col = at(F.GOTO_DEF, "add")
        resp = lsp.rename(URI, line, col, "2bad")
        assert resp is not None
        assert resp.get("error") is not None
