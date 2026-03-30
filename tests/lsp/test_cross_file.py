"""Tests for cross-file LSP features (multiple documents open)."""

from .conftest import def_locations, completion_labels, result_list, open_doc


URI_A = "file:///tmp/types.fl"
URI_B = "file:///tmp/main.fl"

TYPES_SRC = 'export type Color { | Red | Green | Blue }\nexport fn makeRed() -> Color { Red }\n'
MAIN_SRC = 'import { Color, makeRed } from "./types"\nconst c = makeRed()\n'


def _open_both(lsp):
    open_doc(lsp, URI_A, TYPES_SRC)
    open_doc(lsp, URI_B, MAIN_SRC)


class TestCrossFile:
    def test_references_across_files(self, lsp):
        _open_both(lsp)
        refs = result_list(lsp.references(URI_A, 0, 14))
        assert len(refs) >= 2, f"Got {len(refs)} refs"

    def test_references_include_other_file(self, lsp):
        _open_both(lsp)
        refs = result_list(lsp.references(URI_A, 0, 14))
        cross_file = [r for r in refs if r.get("uri") != URI_A]
        assert len(cross_file) > 0, "No cross-file refs found"

    def test_goto_def_across_files(self, lsp):
        _open_both(lsp)
        locs = def_locations(lsp.goto_definition(URI_B, 1, 10))
        assert len(locs) > 0

    def test_goto_def_points_to_types_file(self, lsp):
        _open_both(lsp)
        locs = def_locations(lsp.goto_definition(URI_B, 1, 10))
        assert locs
        target_uri = locs[0].get("uri", "")
        assert "types" in target_uri, f"Target: {target_uri}"

    def test_completion_shows_cross_file_symbols(self, lsp):
        open_doc(lsp, URI_A, TYPES_SRC)
        new_main = 'import { Color, makeRed } from "./types"\nconst c = makeRed()\nmake\n'
        open_doc(lsp, URI_B, new_main)
        labels = completion_labels(lsp.completion(URI_B, 2, 4))
        assert "makeRed" in labels, f"Labels: {labels[:10]}"
