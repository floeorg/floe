"""Tests for textDocument/references."""

import pytest

from .conftest import URI, open_and_diagnose
from . import fixtures as F


def _open(lsp, source, timeout=1.0):
    open_and_diagnose(lsp, URI, source, timeout=timeout)


class TestReferences:
    def test_fn_def_and_usage(self, lsp):
        _open(lsp, F.GOTO_DEF)
        resp = lsp.references(URI, 0, 3)  # "add" definition
        refs = (resp.get("result") or []) if resp else []
        assert len(refs) >= 2, f"Expected def + usage, got {len(refs)} refs"

    def test_type_references(self, lsp):
        _open(lsp, F.TYPES + "\nfn pick(c: Color) -> string { \"ok\" }\n")
        resp = lsp.references(URI, 0, 5)  # "Color"
        refs = (resp.get("result") or []) if resp else []
        assert len(refs) >= 2, f"Got {len(refs)} refs"

    def test_fn_first_three_uses(self, lsp):
        _open(lsp, F.MULTIPLE_FNS)
        resp = lsp.references(URI, 0, 3)  # "first"
        refs = (resp.get("result") or []) if resp else []
        assert len(refs) >= 3, f"Got {len(refs)} refs"

    def test_const_def_and_usage(self, lsp):
        _open(lsp, F.MULTIPLE_FNS)
        resp = lsp.references(URI, 4, 6)  # "a"
        refs = (resp.get("result") or []) if resp else []
        assert len(refs) >= 2, f"Got {len(refs)} refs"

    def test_large_union_variant(self, lsp):
        _open(lsp, F.LARGE_UNION)
        resp = lsp.references(URI, 1, 6)  # "Plus"
        refs = (resp.get("result") or []) if resp else []
        assert len(refs) >= 2, f"Got {len(refs)} refs"
