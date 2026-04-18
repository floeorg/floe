"""Tests for textDocument/references."""

from .conftest import URI, result_list, open_doc
from . import fixtures as F


class TestReferences:
    def test_fn_def_and_usage(self, lsp):
        open_doc(lsp, URI, F.GOTO_DEF)
        refs = result_list(lsp.references(URI, 0, 3))
        assert len(refs) >= 2, f"Expected def + usage, got {len(refs)} refs"

    def test_type_references(self, lsp):
        open_doc(lsp, URI, F.TYPES + "\nfn pick(c: Color) => string { \"ok\" }\n")
        refs = result_list(lsp.references(URI, 0, 5))
        assert len(refs) >= 2, f"Got {len(refs)} refs"

    def test_fn_first_three_uses(self, lsp):
        open_doc(lsp, URI, F.MULTIPLE_FNS)
        refs = result_list(lsp.references(URI, 0, 3))
        assert len(refs) >= 3, f"Got {len(refs)} refs"

    def test_const_def_and_usage(self, lsp):
        open_doc(lsp, URI, F.MULTIPLE_FNS)
        refs = result_list(lsp.references(URI, 4, 6))
        assert len(refs) >= 2, f"Got {len(refs)} refs"

    def test_large_union_variant(self, lsp):
        open_doc(lsp, URI, F.LARGE_UNION)
        refs = result_list(lsp.references(URI, 1, 6))
        assert len(refs) >= 2, f"Got {len(refs)} refs"
