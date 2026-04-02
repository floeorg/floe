"""Tests for textDocument/completion."""

from .conftest import URI, completion_labels, open_doc
from . import fixtures as F

KEYWORDS = ["fn", "const", "type", "match", "import", "export"]

DOT_ACCESS_SOURCE = 'type User { name: string, age: number }\nconst u = User(name: "a", age: 1)\nconst n = u.\n'


class TestCompletionBasic:
    def test_has_items(self, lsp):
        open_doc(lsp, URI, F.SIMPLE + "\n")
        labels = completion_labels(lsp.completion(URI, 11, 0))
        assert len(labels) > 0

    def test_includes_keywords(self, lsp):
        open_doc(lsp, URI, F.SIMPLE + "\n")
        labels = completion_labels(lsp.completion(URI, 11, 0))
        assert any(k in labels for k in ["fn", "const", "type", "match", "import"]), f"Labels: {labels[:10]}"

    def test_includes_document_symbols(self, lsp):
        open_doc(lsp, URI, F.SIMPLE + "\n")
        labels = completion_labels(lsp.completion(URI, 11, 0))
        assert any(s in labels for s in ["add", "greet", "x", "msg"]), f"Labels: {labels[:10]}"


class TestCompletionPipe:
    def test_after_pipe_has_items(self, lsp):
        open_doc(lsp, URI, F.COMPLETION_PIPE)
        labels = completion_labels(lsp.completion(URI, 1, len("const result = nums |> ")))
        assert len(labels) > 0

    def test_array_module_methods(self, lsp):
        open_doc(lsp, URI, "const nums = [1, 2, 3]\nconst r = nums |> Array.\n")
        labels = completion_labels(lsp.completion(URI, 1, len("const r = nums |> Array.")))
        assert any(m in labels for m in ["map", "filter", "reduce", "sort", "length"]), f"Labels: {labels[:15]}"

    def test_string_module_methods(self, lsp):
        open_doc(lsp, URI, 'const s = "hello"\nconst r = s |> String.\n')
        labels = completion_labels(lsp.completion(URI, 1, len("const r = s |> String.")))
        assert any(m in labels for m in ["trim", "toUpperCase", "toLowerCase", "length", "split"]), f"Labels: {labels[:15]}"


class TestCompletionMatch:
    def test_match_arms_show_variants(self, lsp):
        source = "type Color { | Red | Green | Blue }\nconst c: Color = Red\nconst r = match c {\n    \n}"
        open_doc(lsp, URI, source)
        labels = completion_labels(lsp.completion(URI, 3, 4))
        assert any(v in labels for v in ["Red", "Green", "Blue"]), f"Labels: {labels[:10]}"


class TestCompletionJsx:
    def test_jsx_attributes(self, lsp):
        source = 'import { useState } from "react"\nexport fn App() -> JSX.Element {\n    <button on\n}'
        open_doc(lsp, URI, source)
        labels = completion_labels(lsp.completion(URI, 2, 15))
        assert any("on" in l.lower() for l in labels), f"Labels: {labels[:10]}"


class TestCompletionAdvanced:
    def test_prefix_filtering(self, lsp):
        open_doc(lsp, URI, "fn apple() -> number { 1 }\nfn apricot() -> number { 2 }\nconst r = ap\n")
        labels = completion_labels(lsp.completion(URI, 2, 12))
        assert "apple" in labels and "apricot" in labels, f"Labels: {labels[:10]}"

    def test_imported_symbols(self, lsp):
        lsp.open_doc("file:///tmp/helpers.fl", "export fn helperFn() -> number { 42 }\n")
        lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
        open_doc(lsp, URI, 'import { helperFn } from "./helpers"\n\n')
        labels = completion_labels(lsp.completion(URI, 1, 0))
        assert "helperFn" in labels, f"Labels: {labels[:15]}"

    def test_local_vars_in_fn_body(self, lsp):
        open_doc(lsp, URI, "fn outer() -> number {\n    const local = 42\n    \n}")
        labels = completion_labels(lsp.completion(URI, 2, 4))
        assert "local" in labels, f"Labels: {labels[:15]}"

    def test_union_constructors(self, lsp):
        open_doc(lsp, URI, "type Color { | Red | Green | Blue }\nconst c = \n", timeout=2.0)
        labels = completion_labels(lsp.completion(URI, 1, 10))
        assert any(v in labels for v in ["Red", "Green", "Blue"]), f"Labels: {labels[:15]}"

    def test_ok_err_builtins(self, lsp):
        open_doc(lsp, URI, "type Color { | Red | Green | Blue }\nconst c = \n", timeout=2.0)
        labels = completion_labels(lsp.completion(URI, 1, 10))
        assert "Ok" in labels and "Err" in labels, f"Labels: {labels[:15]}"


# ── Dot-access completions ──────────────────────────────────


class TestCompletionDotAccess:
    """Dot-access should return only fields of the accessed type, never global symbols."""

    def test_record_fields(self, lsp):
        open_doc(lsp, URI, DOT_ACCESS_SOURCE)
        labels = completion_labels(lsp.completion(URI, 2, 13))
        assert "name" in labels, f"Labels: {labels[:15]}"
        assert "age" in labels, f"Labels: {labels[:15]}"

    def test_no_unrelated_fields(self, lsp):
        source = 'type User { name: string, age: number }\ntype Item { title: string }\nconst u = User(name: "a", age: 1)\nconst n = u.\n'
        open_doc(lsp, URI, source)
        labels = completion_labels(lsp.completion(URI, 3, 13))
        assert "title" not in labels, f"Item field 'title' leaked into User dot-access: {labels[:15]}"

    def test_no_keywords_in_dot_access(self, lsp):
        open_doc(lsp, URI, DOT_ACCESS_SOURCE)
        labels = completion_labels(lsp.completion(URI, 2, 13))
        for kw in KEYWORDS:
            assert kw not in labels, f"Keyword '{kw}' should not appear in dot-access: {labels[:15]}"

    def test_no_global_vars_in_dot_access(self, lsp):
        """Regression test for #701."""
        source = 'const foo = 42\nconst setFoo = 99\ntype User { name: string }\nconst u = User(name: "a")\nconst n = u.\n'
        open_doc(lsp, URI, source)
        labels = completion_labels(lsp.completion(URI, 4, 13))
        assert "foo" not in labels, f"Global var 'foo' leaked into dot-access: {labels[:15]}"
        assert "setFoo" not in labels, f"Global var 'setFoo' leaked into dot-access: {labels[:15]}"

    def test_unresolved_type_returns_empty_not_globals(self, lsp):
        source = "const foo = 42\nconst x = unknown.\n"
        open_doc(lsp, URI, source)
        labels = completion_labels(lsp.completion(URI, 1, 19))
        assert "foo" not in labels, f"Global var leaked into unresolved dot-access: {labels[:15]}"
        for kw in KEYWORDS:
            assert kw not in labels, f"Keyword '{kw}' leaked into unresolved dot-access: {labels[:15]}"

    def test_spread_record_fields(self, lsp):
        source = 'type Base { id: string }\ntype Extended { ...Base, extra: number }\nconst e = Extended(id: "1", extra: 42)\nconst n = e.\n'
        open_doc(lsp, URI, source)
        labels = completion_labels(lsp.completion(URI, 3, 13))
        assert "id" in labels, f"Spread field 'id' missing: {labels[:15]}"
        assert "extra" in labels, f"Field 'extra' missing: {labels[:15]}"


# ── Suppression tests ───────────────────────────────────────


class TestCompletionSuppression:
    """Completions should be suppressed in comments and string literals."""

    def test_no_completions_in_line_comment(self, lsp):
        source = "const x = 42\n// x\n"
        open_doc(lsp, URI, source)
        labels = completion_labels(lsp.completion(URI, 1, 4))
        assert "x" not in labels, f"Completions should be suppressed in comments: {labels[:10]}"

    def test_no_completions_in_block_comment(self, lsp):
        source = "const x = 42\n/* x */\n"
        open_doc(lsp, URI, source)
        labels = completion_labels(lsp.completion(URI, 1, 3))
        assert "x" not in labels, f"Completions should be suppressed in block comments: {labels[:10]}"

    def test_no_completions_in_string_literal(self, lsp):
        source = 'const x = 42\nconst s = "x"\n'
        open_doc(lsp, URI, source)
        labels = completion_labels(lsp.completion(URI, 1, 12))
        assert "x" not in labels, f"Completions should be suppressed in strings: {labels[:10]}"

    def test_completions_work_after_comment(self, lsp):
        """Completions should still work on the line after a comment."""
        source = "const x = 42\n// comment\n\n"
        open_doc(lsp, URI, source)
        labels = completion_labels(lsp.completion(URI, 2, 0))
        assert len(labels) > 0, "Should have completions after comment line"


# ── Negative tests for pipe context ─────────────────────────


class TestCompletionPipeFiltering:
    """Pipe completions should not include irrelevant items."""

    def test_pipe_no_keywords(self, lsp):
        open_doc(lsp, URI, F.COMPLETION_PIPE)
        labels = completion_labels(lsp.completion(URI, 1, len("const result = nums |> ")))
        for kw in KEYWORDS:
            assert kw not in labels, f"Keyword '{kw}' should not appear in pipe completions: {labels[:15]}"

    def test_pipe_uses_bare_names(self, lsp):
        """Pipe completions should use bare names like 'map', not 'Array.map'."""
        open_doc(lsp, URI, F.COMPLETION_PIPE)
        labels = completion_labels(lsp.completion(URI, 1, len("const result = nums |> ")))
        assert "map" in labels, f"Bare 'map' should appear in pipe completions: {labels[:15]}"
        assert "filter" in labels, f"Bare 'filter' should appear: {labels[:15]}"
        assert "Array.map" not in labels, f"Qualified 'Array.map' should not appear in pipe completions: {labels[:15]}"

    def test_pipe_for_block_functions(self, lsp):
        """User-defined for-block functions should appear in pipe completions."""
        source = 'type Todo { text: string, done: boolean }\n\nfor Array<Todo> {\n    export fn remaining(self) -> number {\n        self |> filter(.done == false) |> length\n    }\n}\n\nconst todos: Array<Todo> = []\nconst r = todos |> \n'
        open_doc(lsp, URI, source)
        labels = completion_labels(lsp.completion(URI, 9, len("const r = todos |> ")))
        assert "remaining" in labels, f"For-block function 'remaining' should appear: {labels[:15]}"

    def test_pipe_string_bare_names(self, lsp):
        """String pipe completions should use bare names."""
        open_doc(lsp, URI, 'const s = "hello"\nconst r = s |> \n')
        labels = completion_labels(lsp.completion(URI, 1, len("const r = s |> ")))
        assert "trim" in labels, f"Bare 'trim' should appear: {labels[:15]}"
        assert "String.trim" not in labels, f"Qualified 'String.trim' should not appear: {labels[:15]}"
