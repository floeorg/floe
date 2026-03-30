"""Tests for textDocument/completion."""

from .conftest import URI, completion_labels, open_doc
from . import fixtures as F


class TestCompletionBasic:
    def test_has_items(self, lsp):
        open_doc(lsp, URI,F.SIMPLE + "\n")
        labels = completion_labels(lsp.completion(URI, 11, 0))
        assert len(labels) > 0

    def test_includes_keywords(self, lsp):
        open_doc(lsp, URI,F.SIMPLE + "\n")
        labels = completion_labels(lsp.completion(URI, 11, 0))
        assert any(k in labels for k in ["fn", "const", "type", "match", "import"]), f"Labels: {labels[:10]}"

    def test_includes_document_symbols(self, lsp):
        open_doc(lsp, URI,F.SIMPLE + "\n")
        labels = completion_labels(lsp.completion(URI, 11, 0))
        assert any(s in labels for s in ["add", "greet", "x", "msg"]), f"Labels: {labels[:10]}"


class TestCompletionPipe:
    def test_after_pipe_has_items(self, lsp):
        open_doc(lsp, URI,F.COMPLETION_PIPE)
        labels = completion_labels(lsp.completion(URI, 1, len("const result = nums |> ")))
        assert len(labels) > 0

    def test_array_module_methods(self, lsp):
        open_doc(lsp, URI,"const nums = [1, 2, 3]\nconst r = nums |> Array.\n")
        labels = completion_labels(lsp.completion(URI, 1, len("const r = nums |> Array.")))
        assert any(m in labels for m in ["map", "filter", "reduce", "sort", "length"]), f"Labels: {labels[:15]}"

    def test_string_module_methods(self, lsp):
        open_doc(lsp, URI,'const s = "hello"\nconst r = s |> String.\n')
        labels = completion_labels(lsp.completion(URI, 1, len("const r = s |> String.")))
        assert any(m in labels for m in ["trim", "toUpperCase", "toLowerCase", "length", "split"]), f"Labels: {labels[:15]}"


class TestCompletionMatch:
    def test_match_arms_show_variants(self, lsp):
        source = "type Color { | Red | Green | Blue }\nconst c: Color = Red\nconst r = match c {\n    \n}"
        open_doc(lsp, URI,source)
        labels = completion_labels(lsp.completion(URI, 3, 4))
        assert any(v in labels for v in ["Red", "Green", "Blue"]), f"Labels: {labels[:10]}"


class TestCompletionJsx:
    def test_jsx_attributes(self, lsp):
        source = 'import trusted { useState } from "react"\nexport fn App() -> JSX.Element {\n    <button on\n}'
        open_doc(lsp, URI,source)
        labels = completion_labels(lsp.completion(URI, 2, 15))
        assert any("on" in l.lower() for l in labels), f"Labels: {labels[:10]}"


class TestCompletionAdvanced:
    def test_prefix_filtering(self, lsp):
        open_doc(lsp, URI,"fn apple() -> number { 1 }\nfn apricot() -> number { 2 }\nconst r = ap\n")
        labels = completion_labels(lsp.completion(URI, 2, 12))
        assert "apple" in labels and "apricot" in labels, f"Labels: {labels[:10]}"

    def test_imported_symbols(self, lsp):
        # Use a local import (cross-file) so this doesn't depend on npm packages
        lsp.open_doc("file:///tmp/helpers.fl", "export fn helperFn() -> number { 42 }\n")
        lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
        open_doc(lsp, URI,'import { helperFn } from "./helpers"\n\n')
        labels = completion_labels(lsp.completion(URI, 1, 0))
        assert "helperFn" in labels, f"Labels: {labels[:15]}"

    def test_local_vars_in_fn_body(self, lsp):
        open_doc(lsp, URI,"fn outer() -> number {\n    const local = 42\n    \n}")
        labels = completion_labels(lsp.completion(URI, 2, 4))
        assert "local" in labels, f"Labels: {labels[:15]}"

    def test_union_constructors(self, lsp):
        open_doc(lsp, URI,"type Color { | Red | Green | Blue }\nconst c = \n", timeout=2.0)
        labels = completion_labels(lsp.completion(URI, 1, 10))
        assert any(v in labels for v in ["Red", "Green", "Blue"]), f"Labels: {labels[:15]}"

    def test_ok_err_builtins(self, lsp):
        open_doc(lsp, URI,"type Color { | Red | Green | Blue }\nconst c = \n", timeout=2.0)
        labels = completion_labels(lsp.completion(URI, 1, 10))
        assert "Ok" in labels and "Err" in labels, f"Labels: {labels[:15]}"
