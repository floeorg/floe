"""Shared fixtures for Floe LSP integration tests."""

from __future__ import annotations

import json
import subprocess
import sys
import threading
import time
from dataclasses import dataclass, field

import pytest


def pytest_addoption(parser):
    parser.addoption(
        "--floe-bin",
        default="./target/debug/floe",
        help="Path to the floe binary",
    )


# ── LSP Client ────────────────────────────────────────────


class LspClient:
    """JSON-RPC client that communicates with an LSP server over stdin/stdout."""

    def __init__(self, binary: str):
        self.proc = subprocess.Popen(
            [binary, "lsp"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self.next_id = 1
        self.responses: dict[int, dict] = {}
        self.notifications: list[dict] = []
        self._lock = threading.Lock()
        self._reader = threading.Thread(target=self._read_loop, daemon=True)
        self._reader.start()

    def _read_loop(self):
        buf = b""
        while True:
            try:
                chunk = self.proc.stdout.read(1)
                if not chunk:
                    break
                buf += chunk
                while b"\r\n\r\n" in buf:
                    header_end = buf.index(b"\r\n\r\n")
                    header = buf[:header_end].decode("utf-8")
                    content_length = None
                    for line in header.split("\r\n"):
                        if line.lower().startswith("content-length:"):
                            content_length = int(line.split(":")[1].strip())
                    if content_length is None:
                        buf = buf[header_end + 4 :]
                        continue
                    body_start = header_end + 4
                    body_end = body_start + content_length
                    if len(buf) < body_end:
                        break
                    body = buf[body_start:body_end].decode("utf-8")
                    buf = buf[body_end:]
                    try:
                        msg = json.loads(body)
                    except json.JSONDecodeError:
                        continue
                    with self._lock:
                        if "id" in msg and "method" not in msg:
                            self.responses[msg["id"]] = msg
                        else:
                            self.notifications.append(msg)
            except Exception:
                break

    def send(self, method: str, params: dict, *, notification: bool = False) -> int | None:
        msg: dict = {"jsonrpc": "2.0", "method": method, "params": params}
        msg_id = None
        if not notification:
            msg_id = self.next_id
            msg["id"] = msg_id
            self.next_id += 1
        body = json.dumps(msg)
        header = f"Content-Length: {len(body)}\r\n\r\n"
        self.proc.stdin.write(header.encode() + body.encode())
        self.proc.stdin.flush()
        return msg_id

    def wait_response(self, msg_id: int, timeout: float = 5.0) -> dict | None:
        deadline = time.time() + timeout
        while time.time() < deadline:
            with self._lock:
                if msg_id in self.responses:
                    return self.responses.pop(msg_id)
            time.sleep(0.05)
        return None

    def collect_notifications(
        self, method: str, timeout: float = 2.0, settle: float = 0.1
    ) -> list[dict]:
        """Collect notifications, returning once settled or timed out."""
        deadline = time.time() + timeout
        found_any = False
        settle_deadline = None
        while True:
            now = time.time()
            with self._lock:
                matches = [n for n in self.notifications if n.get("method") == method]
            if matches and not found_any:
                found_any = True
                settle_deadline = now + settle
            if found_any and now >= settle_deadline:
                break
            if now >= deadline:
                break
            time.sleep(0.05)
        with self._lock:
            result = [n for n in self.notifications if n.get("method") == method]
            self.notifications = [n for n in self.notifications if n.get("method") != method]
        return result

    # ── High-level helpers ────────────────────────────────

    def initialize(self):
        msg_id = self.send("initialize", {"capabilities": {}, "rootUri": "file:///tmp"})
        resp = self.wait_response(msg_id)
        self.send("initialized", {}, notification=True)
        return resp

    def open_doc(self, uri: str, text: str):
        self.send(
            "textDocument/didOpen",
            {"textDocument": {"uri": uri, "languageId": "floe", "version": 1, "text": text}},
            notification=True,
        )

    def hover(self, uri: str, line: int, char: int, timeout: float = 5.0) -> dict | None:
        msg_id = self.send(
            "textDocument/hover",
            {"textDocument": {"uri": uri}, "position": {"line": line, "character": char}},
        )
        return self.wait_response(msg_id, timeout=timeout)

    def completion(self, uri: str, line: int, char: int) -> dict | None:
        msg_id = self.send(
            "textDocument/completion",
            {"textDocument": {"uri": uri}, "position": {"line": line, "character": char}},
        )
        return self.wait_response(msg_id)

    def goto_definition(self, uri: str, line: int, char: int) -> dict | None:
        msg_id = self.send(
            "textDocument/definition",
            {"textDocument": {"uri": uri}, "position": {"line": line, "character": char}},
        )
        return self.wait_response(msg_id)

    def references(self, uri: str, line: int, char: int) -> dict | None:
        msg_id = self.send(
            "textDocument/references",
            {
                "textDocument": {"uri": uri},
                "position": {"line": line, "character": char},
                "context": {"includeDeclaration": True},
            },
        )
        return self.wait_response(msg_id)

    def document_symbols(self, uri: str) -> dict | None:
        msg_id = self.send("textDocument/documentSymbol", {"textDocument": {"uri": uri}})
        return self.wait_response(msg_id)

    def code_action(self, uri: str, line: int, diagnostics: list[dict] | None = None) -> dict | None:
        msg_id = self.send(
            "textDocument/codeAction",
            {
                "textDocument": {"uri": uri},
                "range": {
                    "start": {"line": line, "character": 0},
                    "end": {"line": line, "character": 0},
                },
                "context": {"diagnostics": diagnostics or []},
            },
        )
        return self.wait_response(msg_id)

    def shutdown(self):
        msg_id = self.send("shutdown", {})
        self.wait_response(msg_id, timeout=2)
        self.send("exit", {}, notification=True)
        try:
            self.proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            self.proc.kill()


# ── Response extractors ───────────────────────────────────


def hover_text(resp: dict | None) -> str | None:
    if resp is None:
        return None
    result = resp.get("result")
    if result is None:
        return None
    contents = result.get("contents", {})
    if isinstance(contents, dict):
        return contents.get("value", "")
    if isinstance(contents, str):
        return contents
    return str(contents)


def completion_labels(resp: dict | None) -> list[str]:
    if resp is None:
        return []
    result = resp.get("result")
    if result is None:
        return []
    if isinstance(result, list):
        items = result
    elif isinstance(result, dict):
        items = result.get("items", [])
    else:
        return []
    return [i.get("label", "") for i in items]


def def_locations(resp: dict | None) -> list:
    if resp is None:
        return []
    result = resp.get("result")
    if result is None:
        return []
    if isinstance(result, list):
        return result
    if isinstance(result, dict):
        return [result]
    return []


def symbol_names(resp: dict | None) -> list[str]:
    if resp is None:
        return []
    result = resp.get("result")
    if result is None:
        return []
    if isinstance(result, list):
        return [s.get("name", "") for s in result]
    return []


def diag_errors(notifs: list[dict]) -> list[dict]:
    """Extract error-severity diagnostics (severity=1)."""
    all_diags = []
    for n in notifs:
        for d in n.get("params", {}).get("diagnostics", []):
            if d.get("severity", 1) == 1:
                all_diags.append(d)
    return all_diags


def diag_all(notifs: list[dict]) -> list[dict]:
    all_diags = []
    for n in notifs:
        all_diags.extend(n.get("params", {}).get("diagnostics", []))
    return all_diags


def diag_codes(notifs: list[dict]) -> list[str]:
    """Extract error codes from diagnostics (e.g. ['E001', 'E004'])."""
    codes = []
    for n in notifs:
        for d in n.get("params", {}).get("diagnostics", []):
            code = d.get("code")
            if isinstance(code, dict):
                code = code.get("value")
            if code:
                codes.append(str(code))
    return codes


# ── Diagnostic helper for opening a doc and getting results ──


@dataclass
class DocDiagnostics:
    """Result of opening a document: all notifications, errors, and codes."""

    notifs: list[dict]
    errors: list[dict] = field(default_factory=list)
    all: list[dict] = field(default_factory=list)
    codes: list[str] = field(default_factory=list)


def open_and_diagnose(
    lsp: LspClient, uri: str, text: str, timeout: float = 2.0
) -> DocDiagnostics:
    """Open a document and collect its diagnostics."""
    lsp.open_doc(uri, text)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=timeout)
    return DocDiagnostics(
        notifs=notifs,
        errors=diag_errors(notifs),
        all=diag_all(notifs),
        codes=diag_codes(notifs),
    )


# ── Fixtures ──────────────────────────────────────────────

URI = "file:///tmp/test.fl"


@pytest.fixture(scope="session")
def lsp(request):
    """Single LSP session shared across all tests."""
    binary = request.config.getoption("--floe-bin")
    client = LspClient(binary)
    resp = client.initialize()
    client.init_response = resp
    yield client
    client.shutdown()
