"""Tests for textDocument/documentSymbol."""

from .conftest import URI, symbol_names, open_doc
from . import fixtures as F


class TestSymbolsBasic:
    def test_lists_functions(self, lsp):
        open_doc(lsp, URI,F.SIMPLE)
        names = symbol_names(lsp.document_symbols(URI))
        assert "add" in names, f"Names: {names}"

    def test_lists_exported_functions(self, lsp):
        open_doc(lsp, URI,F.SIMPLE)
        names = symbol_names(lsp.document_symbols(URI))
        assert "greet" in names, f"Names: {names}"

    def test_lists_consts(self, lsp):
        open_doc(lsp, URI,F.SIMPLE)
        names = symbol_names(lsp.document_symbols(URI))
        assert "x" in names, f"Names: {names}"

    def test_lists_types(self, lsp):
        open_doc(lsp, URI,F.TYPES)
        names = symbol_names(lsp.document_symbols(URI))
        assert "Color" in names, f"Names: {names}"

    def test_lists_union_variants(self, lsp):
        open_doc(lsp, URI,F.TYPES)
        names = symbol_names(lsp.document_symbols(URI))
        assert "Red" in names and "Green" in names, f"Names: {names}"

    def test_lists_record_types(self, lsp):
        open_doc(lsp, URI,F.TYPES)
        names = symbol_names(lsp.document_symbols(URI))
        assert "User" in names, f"Names: {names}"

    def test_forblock_functions(self, lsp):
        open_doc(lsp, URI,F.FORBLOCK)
        names = symbol_names(lsp.document_symbols(URI))
        assert "remaining" in names, f"Names: {names}"


class TestSymbolsAdvanced:
    def test_all_fns_listed(self, lsp):
        open_doc(lsp, URI,F.MULTIPLE_FNS)
        names = symbol_names(lsp.document_symbols(URI))
        assert all(n in names for n in ["first", "second", "third"]), f"Names: {names}"

    def test_all_consts_listed(self, lsp):
        open_doc(lsp, URI,F.MULTIPLE_FNS)
        names = symbol_names(lsp.document_symbols(URI))
        assert all(n in names for n in ["a", "b", "c", "d"]), f"Names: {names}"

    def test_closure_consts(self, lsp):
        open_doc(lsp, URI,F.CLOSURE_ASSIGN)
        names = symbol_names(lsp.document_symbols(URI))
        assert "add" in names and "double" in names, f"Names: {names}"

    def test_trait_definition(self, lsp):
        open_doc(lsp, URI,F.TRAIT_FILE)
        names = symbol_names(lsp.document_symbols(URI))
        assert "Printable" in names, f"Names: {names}"

    def test_trait_type_and_impl(self, lsp):
        open_doc(lsp, URI,F.TRAIT_FILE)
        names = symbol_names(lsp.document_symbols(URI))
        assert "Dog" in names and "print" in names, f"Names: {names}"

    def test_large_union_variants(self, lsp):
        open_doc(lsp, URI,F.LARGE_UNION)
        names = symbol_names(lsp.document_symbols(URI))
        assert all(v in names for v in ["Plus", "Minus", "Star", "Eof"]), f"Names: {names}"

    def test_nested_union_types(self, lsp):
        open_doc(lsp, URI,F.NESTED_MATCH)
        names = symbol_names(lsp.document_symbols(URI))
        assert "Outer" in names and "Inner" in names, f"Names: {names}"

    def test_nested_union_variants(self, lsp):
        open_doc(lsp, URI,F.NESTED_MATCH)
        names = symbol_names(lsp.document_symbols(URI))
        assert "A" in names and "X" in names, f"Names: {names}"

    def test_empty_file(self, lsp):
        open_doc(lsp, URI,F.EMPTY_FILE)
        names = symbol_names(lsp.document_symbols(URI))
        assert len(names) == 0, f"Names: {names}"


class TestSymbolsQualifiedVariant:
    def test_types_in_symbols(self, lsp):
        open_doc(lsp, URI,F.QUALIFIED_VARIANT)
        names = symbol_names(lsp.document_symbols(URI))
        assert "Color" in names and "Filter" in names, f"Names: {names}"

    def test_variants_in_symbols(self, lsp):
        open_doc(lsp, URI,F.QUALIFIED_VARIANT)
        names = symbol_names(lsp.document_symbols(URI))
        assert "Red" in names and "All" in names, f"Names: {names}"


class TestSymbolsGenericFn:
    def test_identity_in_symbols(self, lsp):
        open_doc(lsp, URI,F.GENERIC_FN)
        names = symbol_names(lsp.document_symbols(URI))
        assert "identity" in names, f"Names: {names}"

    def test_pair_in_symbols(self, lsp):
        open_doc(lsp, URI,F.GENERIC_FN)
        names = symbol_names(lsp.document_symbols(URI))
        assert "pair" in names, f"Names: {names}"


class TestSymbolsTour:
    def test_closure_file(self, lsp):
        open_doc(lsp, URI,F.CLOSURE_FILE)
        names = symbol_names(lsp.document_symbols(URI))
        assert "add" in names and "double" in names, f"Names: {names}"

    def test_newtype_wrapper(self, lsp):
        open_doc(lsp, URI,F.NEWTYPE_WRAPPER)
        names = symbol_names(lsp.document_symbols(URI))
        assert "UserId" in names, f"Names: {names}"

    def test_opaque_type(self, lsp):
        open_doc(lsp, URI,F.OPAQUE_TYPE)
        names = symbol_names(lsp.document_symbols(URI))
        assert "HashedPassword" in names, f"Names: {names}"

    def test_test_block_fn(self, lsp):
        open_doc(lsp, URI,F.TEST_BLOCK)
        names = symbol_names(lsp.document_symbols(URI))
        assert "add" in names, f"Names: {names}"

    def test_inline_for_fn(self, lsp):
        open_doc(lsp, URI,F.INLINE_FOR)
        names = symbol_names(lsp.document_symbols(URI))
        assert "shout" in names, f"Names: {names}"

    def test_multi_depth_variants(self, lsp):
        open_doc(lsp, URI,F.MULTI_DEPTH_MATCH)
        names = symbol_names(lsp.document_symbols(URI))
        assert "Timeout" in names and "DnsFailure" in names, f"Names: {names}"

    def test_deriving_type(self, lsp):
        open_doc(lsp, URI,F.DERIVING)
        names = symbol_names(lsp.document_symbols(URI))
        assert "Point" in names, f"Names: {names}"
