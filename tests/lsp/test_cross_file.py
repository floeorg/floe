"""Tests for cross-file LSP features (multiple documents open)."""

import pytest

from .conftest import def_locations, completion_labels, open_and_diagnose


URI_A = "file:///tmp/types.fl"
URI_B = "file:///tmp/main.fl"

TYPES_SRC = 'export type Color { | Red | Green | Blue }\nexport fn makeRed() -> Color { Red }\n'
MAIN_SRC = 'import { Color, makeRed } from "./types"\nconst c = makeRed()\n'


class TestCrossFile:
    def test_references_across_files(self, lsp):
        open_and_diagnose(lsp, URI_A, TYPES_SRC)
        open_and_diagnose(lsp, URI_B, MAIN_SRC)

        resp = lsp.references(URI_A, 0, 14)  # "Color" in types.fl
        refs = (resp.get("result") or []) if resp else []
        assert len(refs) >= 2, f"Got {len(refs)} refs"

    def test_references_include_other_file(self, lsp):
        open_and_diagnose(lsp, URI_A, TYPES_SRC)
        open_and_diagnose(lsp, URI_B, MAIN_SRC)

        resp = lsp.references(URI_A, 0, 14)
        refs = (resp.get("result") or []) if resp else []
        cross_file = [r for r in refs if r.get("uri") != URI_A]
        assert len(cross_file) > 0, f"No cross-file refs found"

    def test_goto_def_across_files(self, lsp):
        open_and_diagnose(lsp, URI_A, TYPES_SRC)
        open_and_diagnose(lsp, URI_B, MAIN_SRC)

        locs = def_locations(lsp.goto_definition(URI_B, 1, 10))  # "makeRed" in main.fl
        assert len(locs) > 0

    def test_goto_def_points_to_types_file(self, lsp):
        open_and_diagnose(lsp, URI_A, TYPES_SRC)
        open_and_diagnose(lsp, URI_B, MAIN_SRC)

        locs = def_locations(lsp.goto_definition(URI_B, 1, 10))
        assert locs
        target_uri = locs[0].get("uri", "")
        assert "types" in target_uri, f"Target: {target_uri}"

    def test_completion_shows_cross_file_symbols(self, lsp):
        open_and_diagnose(lsp, URI_A, TYPES_SRC)
        new_main = 'import { Color, makeRed } from "./types"\nconst c = makeRed()\nmake\n'
        open_and_diagnose(lsp, URI_B, new_main)

        labels = completion_labels(lsp.completion(URI_B, 2, 4))
        assert "makeRed" in labels, f"Labels: {labels[:10]}"
