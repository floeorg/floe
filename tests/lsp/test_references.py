"""Tests for textDocument/references."""

from .conftest import URI, at, result_list, open_doc
from . import fixtures as F


class TestReferences:
    def test_fn_def_and_usage(self, lsp):
        open_doc(lsp, URI, F.GOTO_DEF)
        line, col = at(F.GOTO_DEF, "add")
        refs = result_list(lsp.references(URI, line, col))
        assert len(refs) >= 2, f"Expected def + usage, got {len(refs)} refs"

    def test_fn_first_three_uses(self, lsp):
        open_doc(lsp, URI, F.MULTIPLE_FNS)
        line, col = at(F.MULTIPLE_FNS, "first")
        refs = result_list(lsp.references(URI, line, col))
        assert len(refs) >= 3, f"Got {len(refs)} refs"

    def test_const_def_and_usage(self, lsp):
        open_doc(lsp, URI, F.MULTIPLE_FNS)
        line, col = at(F.MULTIPLE_FNS, "let a")
        # Cursor on the `a` of `let a = first(1)` — usage is on next line.
        refs = result_list(lsp.references(URI, line, col + 4))
        assert len(refs) >= 2, f"Got {len(refs)} refs"

    def test_no_decl_when_include_declaration_false(self, lsp):
        open_doc(lsp, URI, F.MULTIPLE_FNS)
        line, col = at(F.MULTIPLE_FNS, "first")
        msg_id = lsp.send(
            "textDocument/references",
            {
                "textDocument": {"uri": URI},
                "position": {"line": line, "character": col},
                "context": {"includeDeclaration": False},
            },
        )
        with_decl_refs = result_list(lsp.references(URI, line, col))
        without_decl_refs = result_list(lsp.wait_response(msg_id))
        assert len(without_decl_refs) == len(with_decl_refs) - 1

    def test_cursor_off_identifier_returns_no_refs(self, lsp):
        open_doc(lsp, URI, F.GOTO_DEF)
        # Cursor inside the type annotation `number` (not a tracked symbol).
        # The tracker has no entry, so we return no references.
        refs = result_list(lsp.references(URI, 0, 0))
        assert refs == []
