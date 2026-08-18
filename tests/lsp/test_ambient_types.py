"""Ambient TypeScript types survive a parse error (#1431).

Ambient declarations come from tsconfig, never from the AST, so a partial
parse must not drop them. Before the fix, one stray semicolon made every
TypeScript lib type report E002 until the file parsed again.
"""

import shutil
import time

import pytest

from .conftest import diag_all, open_doc

# A stub TypeScript lib. `AmbientProbe` exists nowhere else, so a resolved
# reference to it proves the ambient tables reached the checker.
LIB_STUB = """interface AmbientProbe {
    label: string;
}
"""

TSCONFIG = '{"compilerOptions": {"lib": ["probe"]}}'

CLEAN_SOURCE = """type Holder = {
    probe: AmbientProbe,
}
"""

# The second type uses a semicolon instead of a comma, which fails the parse.
BROKEN_SOURCE = """type Holder = {
    probe: AmbientProbe,
}

type Other = {
    label: string;
}
"""


@pytest.fixture
def ambient_uri(tmp_path):
    """URI of a source file in a project whose only ambient type is the stub."""
    lib_dir = tmp_path / "node_modules" / "typescript" / "lib"
    lib_dir.mkdir(parents=True)
    (lib_dir / "lib.probe.d.ts").write_text(LIB_STUB)
    (tmp_path / "tsconfig.json").write_text(TSCONFIG)
    src_dir = tmp_path / "src"
    src_dir.mkdir()

    return f"file://{src_dir / 'ambient.fl'}"


def test_ambient_type_resolves_on_clean_parse(lsp, ambient_uri):
    result = open_doc(lsp, ambient_uri, CLEAN_SOURCE, timeout=5.0)
    assert not any("AmbientProbe" in d["message"] for d in result.errors), (
        f"ambient type must resolve on a clean parse, got: {result.errors}"
    )


def test_ambient_type_survives_parse_error(lsp, ambient_uri):
    result = open_doc(lsp, ambient_uri, BROKEN_SOURCE, timeout=5.0)
    assert not any("AmbientProbe" in d["message"] for d in result.errors), (
        f"a parse error must not drop the ambient types, got: {result.errors}"
    )
    assert result.errors, "the broken source must still report its parse error"


# ── Cache invalidation (#1431) ──────────────────────────────────
#
# The ambient tables are cached per project, so the cache must notice
# when the tsconfig behind them changes, and must not remember a project
# that had no TypeScript installed yet.

ES5_STUB = """interface Es5Probe {
    label: string;
}
"""

TSCONFIG_WITHOUT_PROBE = '{"compilerOptions": {"lib": ["es5"]}}'
TSCONFIG_WITH_PROBE = '{"compilerOptions": {"lib": ["es5", "probe"]}}'

# The edit that follows the tsconfig change. It adds a type and leaves the
# reference to `AmbientProbe` in place.
EDITED_SOURCE = (
    CLEAN_SOURCE
    + """
type Marker = {
    id: string,
}
"""
)


def install_ts_lib(project_dir, *, with_es5: bool):
    """Write the stub TypeScript lib into a project directory."""
    lib_dir = project_dir / "node_modules" / "typescript" / "lib"
    lib_dir.mkdir(parents=True, exist_ok=True)
    (lib_dir / "lib.probe.d.ts").write_text(LIB_STUB)
    if with_es5:
        (lib_dir / "lib.es5.d.ts").write_text(ES5_STUB)


def change_doc(lsp, uri, text, timeout=5.0):
    """Send a didChange and return the notifications republished for `uri`.

    An empty diagnostics list is the passing case here, so the caller needs
    the notifications themselves to tell "no errors" from "no answer".
    """
    lsp.send(
        "textDocument/didChange",
        {
            "textDocument": {"uri": uri, "version": 2},
            "contentChanges": [{"text": text}],
        },
        notification=True,
    )
    deadline = time.time() + timeout
    while time.time() < deadline:
        notifs = lsp.collect_notifications(
            "textDocument/publishDiagnostics", timeout=1.0
        )
        mine = [n for n in notifs if n.get("params", {}).get("uri") == uri]
        if mine:
            return mine

    return []


def test_ambient_types_reload_after_tsconfig_change(lsp, tmp_path):
    """A tsconfig edit takes effect on the next edit, without a restart."""
    (tmp_path / "package.json").write_text('{"name": "stale-probe"}')
    install_ts_lib(tmp_path, with_es5=True)
    (tmp_path / "tsconfig.json").write_text(TSCONFIG_WITHOUT_PROBE)
    src_dir = tmp_path / "src"
    src_dir.mkdir()
    uri = f"file://{src_dir / 'stale.fl'}"

    opened = open_doc(lsp, uri, CLEAN_SOURCE, timeout=5.0)
    assert any("AmbientProbe" in d["message"] for d in opened.errors), (
        f"the probe lib is not in tsconfig yet, so the type must be unknown, "
        f"got: {opened.errors}"
    )

    (tmp_path / "tsconfig.json").write_text(TSCONFIG_WITH_PROBE)
    notifs = change_doc(lsp, uri, EDITED_SOURCE)
    assert notifs, "the edit must republish diagnostics for the document"
    diags = diag_all(notifs)
    assert not any("AmbientProbe" in d["message"] for d in diags), (
        f"the new tsconfig names the probe lib, so the type must resolve, got: {diags}"
    )


def test_ambient_types_load_after_typescript_is_installed(lsp, tmp_path):
    """A project opened before `npm install` picks the types up afterwards."""
    (tmp_path / "package.json").write_text('{"name": "late-install"}')
    (tmp_path / "tsconfig.json").write_text(TSCONFIG)
    src_dir = tmp_path / "src"
    src_dir.mkdir()
    uri = f"file://{src_dir / 'late.fl'}"

    opened = open_doc(lsp, uri, CLEAN_SOURCE, timeout=5.0)
    assert any("AmbientProbe" in d["message"] for d in opened.errors), (
        f"no TypeScript is installed yet, so the type must be unknown, "
        f"got: {opened.errors}"
    )

    # Simulate `npm install typescript`. The project dir does not move,
    # because package.json already pinned it.
    install_ts_lib(tmp_path, with_es5=False)
    notifs = change_doc(lsp, uri, EDITED_SOURCE)
    assert notifs, "the edit must republish diagnostics for the document"
    diags = diag_all(notifs)
    assert not any("AmbientProbe" in d["message"] for d in diags), (
        f"the install must end the miss, got: {diags}"
    )


# ── node_modules invalidation ───────────────────────────────────
#
# The fingerprint also stamps the installed packages, because
# `npm install @types/...` is the action a person takes exactly when the
# editor calls a global unknown.

# A stub `@types` package. `LateProbe` exists nowhere else.
TYPES_STUB = """interface LateProbe {
    id: string;
}
"""

# The lockfile npm rewrites on every install, add and remove.
NPM_MARKER_BEFORE = '{"name": "types-install", "packages": {}}'
NPM_MARKER_AFTER = '{"name": "types-install", "packages": {"node_modules/@types/late": {}}}'

BOTH_PROBES_SOURCE = """type Holder = {
    probe: AmbientProbe,
    late: LateProbe,
}
"""


def install_types_package(project_dir):
    """Write a stub `@types/late` package into a project directory."""
    types_dir = project_dir / "node_modules" / "@types" / "late"
    types_dir.mkdir(parents=True, exist_ok=True)
    (types_dir / "index.d.ts").write_text(TYPES_STUB)


def test_ambient_types_reload_after_types_package_install(lsp, tmp_path):
    """`npm install @types/...` takes effect on the next edit."""
    (tmp_path / "package.json").write_text('{"name": "types-install"}')
    install_ts_lib(tmp_path, with_es5=False)
    (tmp_path / "node_modules" / ".package-lock.json").write_text(NPM_MARKER_BEFORE)
    (tmp_path / "tsconfig.json").write_text(TSCONFIG)
    src_dir = tmp_path / "src"
    src_dir.mkdir()
    uri = f"file://{src_dir / 'types.fl'}"

    # The first open caches the ambient tables for this project.
    opened = open_doc(lsp, uri, CLEAN_SOURCE, timeout=5.0)
    assert not any("AmbientProbe" in d["message"] for d in opened.errors), (
        f"the lib stub must resolve on the first open, got: {opened.errors}"
    )

    # Simulate `npm install @types/late`, which rewrites the lockfile.
    install_types_package(tmp_path)
    (tmp_path / "node_modules" / ".package-lock.json").write_text(NPM_MARKER_AFTER)
    notifs = change_doc(lsp, uri, BOTH_PROBES_SOURCE)
    assert notifs, "the edit must republish diagnostics for the document"
    diags = diag_all(notifs)
    assert not any("LateProbe" in d["message"] for d in diags), (
        f"the installed types package must resolve, got: {diags}"
    )


def test_ambient_types_drop_after_node_modules_removed(lsp, tmp_path):
    """Deleting `node_modules` retires the tables it produced."""
    (tmp_path / "package.json").write_text('{"name": "uninstall"}')
    install_ts_lib(tmp_path, with_es5=False)
    (tmp_path / "tsconfig.json").write_text(TSCONFIG)
    src_dir = tmp_path / "src"
    src_dir.mkdir()
    uri = f"file://{src_dir / 'gone.fl'}"

    opened = open_doc(lsp, uri, CLEAN_SOURCE, timeout=5.0)
    assert not any("AmbientProbe" in d["message"] for d in opened.errors), (
        f"the lib stub must resolve on the first open, got: {opened.errors}"
    )

    shutil.rmtree(tmp_path / "node_modules")
    notifs = change_doc(lsp, uri, EDITED_SOURCE)
    assert notifs, "the edit must republish diagnostics for the document"
    diags = diag_all(notifs)
    assert any("AmbientProbe" in d["message"] for d in diags), (
        f"the lib files are gone, so the type must be unknown again, got: {diags}"
    )
