"""Tests for LSP server initialization and capability advertisement."""

import pytest


class TestInitialization:
    """Server responds to initialize with correct capabilities."""

    def test_server_responds(self, lsp):
        assert lsp.init_response is not None and "result" in lsp.init_response

    @pytest.mark.parametrize(
        "capability",
        [
            "hoverProvider",
            "completionProvider",
            "definitionProvider",
            "referencesProvider",
            "documentSymbolProvider",
            "codeActionProvider",
        ],
    )
    def test_capability_advertised(self, lsp, capability):
        caps = lsp.init_response["result"]["capabilities"]
        assert capability in caps and caps[capability], f"Missing capability: {capability}"

    def test_text_document_sync_full(self, lsp):
        caps = lsp.init_response["result"]["capabilities"]
        sync = caps.get("textDocumentSync")
        assert sync == 1 or (isinstance(sync, dict) and sync.get("change") == 1)

    def test_completion_trigger_characters(self, lsp):
        caps = lsp.init_response["result"]["capabilities"]
        triggers = caps.get("completionProvider", {}).get("triggerCharacters", [])
        assert "." in triggers and "|" in triggers and ">" in triggers
