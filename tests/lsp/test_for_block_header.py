"""Tests for LSP support on `for Type: Trait { ... }` headers.

Covers three gaps fixed in #1271:
- Hover on the type name in a for-block header
- Goto-definition on the trait name in a for-block header
- Re-checking dependent files when a trait file changes
"""

import time

from .conftest import (
    at,
    def_locations,
    diag_codes,
    hover_text,
    open_doc,
    open_and_diagnose,
)


SINGLE_FILE = """\
type Container = { label: string }

trait Greeter {
    let hello(self) -> string
}

for Container: Greeter {
    export let hello(self) -> string = {
        self.label
    }
}
"""


class TestForBlockHeaderHover:
    def test_hover_on_type_name(self, lsp):
        uri = "file:///tmp/for_hover_type.fl"
        open_doc(lsp, uri, SINGLE_FILE)
        line, col = at(SINGLE_FILE, "for Container", offset=4)
        h = hover_text(lsp.hover(uri, line, col))
        assert h is not None, "expected hover on type name in for-header"
        assert "Container" in h, f"expected Container in hover, got: {h}"

    def test_hover_on_trait_name(self, lsp):
        uri = "file:///tmp/for_hover_trait.fl"
        open_doc(lsp, uri, SINGLE_FILE)
        line, col = at(SINGLE_FILE, ": Greeter", offset=2)
        h = hover_text(lsp.hover(uri, line, col))
        assert h is not None, "expected hover on trait name in for-header"
        assert "Greeter" in h, f"expected Greeter in hover, got: {h}"


class TestForBlockHeaderGotoDef:
    def test_goto_type_in_header(self, lsp):
        uri = "file:///tmp/for_goto_type.fl"
        open_doc(lsp, uri, SINGLE_FILE)
        line, col = at(SINGLE_FILE, "for Container", offset=4)
        locs = def_locations(lsp.goto_definition(uri, line, col))
        assert locs, "goto-def on type name in for-header returned nothing"

    def test_goto_trait_in_header_cross_file(self, lsp, tmp_path):
        trait_src = (
            "export trait Greeter {\n"
            "    let hello(self) -> string\n"
            "}\n"
        )
        impl_src = (
            'import { for Greeter } from "./greeter"\n'
            "export type MyGreeter = { prefix: string }\n"
            "for MyGreeter: Greeter {\n"
            "    export let hello(self) -> string = { self.prefix }\n"
            "}\n"
        )
        trait_path = tmp_path / "greeter.fl"
        impl_path = tmp_path / "impl.fl"
        trait_path.write_text(trait_src)
        impl_path.write_text(impl_src)
        trait_uri = f"file://{trait_path}"
        impl_uri = f"file://{impl_path}"
        open_doc(lsp, trait_uri, trait_src)
        open_doc(lsp, impl_uri, impl_src)

        line, col = at(impl_src, ": Greeter", offset=2)
        locs = def_locations(lsp.goto_definition(impl_uri, line, col))
        assert locs, "goto-def on imported trait name returned nothing"
        target = locs[0].get("uri", "")
        assert "greeter" in target, f"goto-def should jump to trait file, got: {target}"


class TestDependentRecheck:
    def test_trait_change_refreshes_impl_diagnostics(self, lsp, tmp_path):
        """Editing a trait file must refresh diagnostics on open files
        that implement it — the symptom reported in the original bug
        was that a new required method did not appear as missing until
        the language server was restarted."""
        trait_src_v1 = (
            "export trait Greeter {\n"
            "    let hello(self) -> string\n"
            "}\n"
        )
        trait_src_v2 = (
            "export trait Greeter {\n"
            "    let hello(self) -> string\n"
            "    let bye(self) -> string\n"
            "}\n"
        )
        impl_src = (
            'import { for Greeter } from "./greeter"\n'
            "export type MyGreeter = { prefix: string }\n"
            "for MyGreeter: Greeter {\n"
            "    export let hello(self) -> string = { self.prefix }\n"
            "}\n"
        )
        trait_path = tmp_path / "greeter.fl"
        impl_path = tmp_path / "impl.fl"
        trait_path.write_text(trait_src_v1)
        impl_path.write_text(impl_src)
        trait_uri = f"file://{trait_path}"
        impl_uri = f"file://{impl_path}"

        open_doc(lsp, trait_uri, trait_src_v1)
        impl_diag = open_and_diagnose(lsp, impl_uri, impl_src)
        assert "E023" not in impl_diag.codes, (
            f"impl should have no missing-method error initially, got: {impl_diag.codes}"
        )

        # Write new trait content to disk and notify via didChange so the
        # LSP cascades a recheck onto the impl file.
        trait_path.write_text(trait_src_v2)
        lsp.send(
            "textDocument/didChange",
            {
                "textDocument": {"uri": trait_uri, "version": 2},
                "contentChanges": [{"text": trait_src_v2}],
            },
            notification=True,
        )

        # Collect diagnostics across both files; the cascade republishes
        # diagnostics on the impl file.
        deadline = time.time() + 5.0
        impl_codes: list[str] = []
        while time.time() < deadline:
            notifs = lsp.collect_notifications(
                "textDocument/publishDiagnostics", timeout=1.0
            )
            impl_notifs = [
                n for n in notifs if n.get("params", {}).get("uri") == impl_uri
            ]
            if impl_notifs:
                impl_codes = diag_codes(impl_notifs)
                break

        assert "E023" in impl_codes, (
            f"impl should report missing-method error after trait changed, "
            f"got codes: {impl_codes}"
        )
