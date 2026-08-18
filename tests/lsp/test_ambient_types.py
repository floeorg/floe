"""Ambient TypeScript types survive a parse error (#1431).

Ambient declarations come from tsconfig, never from the AST, so a partial
parse must not drop them. Before the fix, one stray semicolon made every
TypeScript lib type report E002 until the file parsed again.
"""

import pytest

from .conftest import open_doc

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
