"""Tests for JSX-specific LSP features."""

from .conftest import URI, hover_text, def_locations, symbol_names, open_doc
from . import fixtures as F


class TestJsx:
    def test_parses_without_syntax_errors(self, lsp):
        result = open_doc(lsp, URI, F.JSX_COMPONENT, timeout=4.0)
        parse_errs = [
            e for e in result.all
            if "cannot find module" not in e.get("message", "").lower()
            and (
                "parse" in e.get("message", "").lower()
                or (
                    "expected" in e.get("message", "").lower()
                    and "found" in e.get("message", "").lower()
                )
            )
        ]
        assert parse_errs == [], f"Parse errors: {[e.get('message','') for e in parse_errs]}"

    def test_hover_component_fn(self, lsp):
        open_doc(lsp, URI, F.JSX_COMPONENT, timeout=4.0)
        h = hover_text(lsp.hover(URI, 2, 14, timeout=10))
        assert h is not None and "Counter" in h, f"Got: {h}"

    def test_hover_destructured_count(self, lsp):
        open_doc(lsp, URI, F.JSX_COMPONENT, timeout=4.0)
        h = hover_text(lsp.hover(URI, 3, 11))
        assert h is not None, f"Got: {h}"

    def test_hover_destructured_setCount(self, lsp):
        open_doc(lsp, URI, F.JSX_COMPONENT, timeout=4.0)
        h = hover_text(lsp.hover(URI, 3, 18))
        assert h is not None, f"Got: {h}"

    def test_hover_inner_fn(self, lsp):
        open_doc(lsp, URI, F.JSX_COMPONENT, timeout=4.0)
        h = hover_text(lsp.hover(URI, 5, 8))
        assert h is not None and "handleClick" in h, f"Got: {h}"

    def test_goto_def_from_attribute_value(self, lsp):
        open_doc(lsp, URI, F.JSX_COMPONENT, timeout=4.0)
        locs = def_locations(lsp.goto_definition(URI, 11, 30))
        assert len(locs) > 0

    def test_symbols_include_component(self, lsp):
        open_doc(lsp, URI, F.JSX_COMPONENT, timeout=4.0)
        names = symbol_names(lsp.document_symbols(URI))
        assert "Counter" in names, f"Names: {names}"

    def test_symbols_include_inner_fn(self, lsp):
        open_doc(lsp, URI, F.JSX_COMPONENT, timeout=4.0)
        names = symbol_names(lsp.document_symbols(URI))
        assert "handleClick" in names, f"Names: {names}"
