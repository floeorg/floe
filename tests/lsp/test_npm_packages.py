"""The editor and `floe check` answer an npm import the same way (#1465).

E013 used to live in the language server alone. `floe check` warned W004
on the same file and exited 0, so continuous integration went green on a
build the editor drew a red line under. The compiler owns the decision
now, and these tests pin the editor half of it.

Three states, three answers:

- the package is not installed     -> E013, an error
- installed, no declarations       -> no error (W004 warning, exit 0)
- installed, with declarations     -> no error at all
"""

import pytest

from .conftest import open_doc

SOURCE = """import trusted {{ shout }} from "{package}"

export let main() -> string = {{
    shout("hello")
}}
"""

DECLARATION = "export declare function shout(message: string): string;\n"


def project(tmp_path, packages):
    """Create a project directory holding `packages`.

    Each entry maps a package name to its `index.d.ts` text, or to None
    for a package that ships no declarations at all.
    """
    modules = tmp_path / "node_modules"
    modules.mkdir()
    (tmp_path / "package.json").write_text('{"name": "npm-package-probe"}')
    for name, declaration in packages.items():
        package_dir = modules / name
        package_dir.mkdir(parents=True)
        if declaration is None:
            package_dir.joinpath("package.json").write_text(f'{{"name": "{name}"}}')
            continue
        package_dir.joinpath("package.json").write_text(
            f'{{"name": "{name}", "types": "index.d.ts"}}'
        )
        package_dir.joinpath("index.d.ts").write_text(declaration)
    src = tmp_path / "src"
    src.mkdir()

    return f"file://{src / 'main.fl'}"


def test_absent_package_reports_e013(lsp, tmp_path):
    uri = project(tmp_path, {})
    result = open_doc(lsp, uri, SOURCE.format(package="absent-package"))
    assert "E013" in result.codes, (
        f"the editor must report E013 for a package that is not installed, "
        f"got: {result.codes}"
    )


def test_absent_package_help_names_the_install(lsp, tmp_path):
    uri = project(tmp_path, {})
    result = open_doc(lsp, uri, SOURCE.format(package="absent-package"))
    messages = " ".join(d.get("message", "") for d in result.all)
    assert "absent-package" in messages, (
        f"the diagnostic must name the package, got: {messages}"
    )


def test_installed_package_without_declarations_is_not_an_error(lsp, tmp_path):
    uri = project(tmp_path, {"no-types": None})
    result = open_doc(lsp, uri, SOURCE.format(package="no-types"))
    assert "E013" not in result.codes, (
        f"a package that resolves must not report E013, got: {result.codes}"
    )
    assert result.errors == [], (
        f"a package without declarations must stay a warning, got: "
        f"{[d.get('message', '') for d in result.errors]}"
    )


def test_installed_package_with_declarations_reports_no_error(lsp, tmp_path):
    uri = project(tmp_path, {"has-types": DECLARATION})
    result = open_doc(lsp, uri, SOURCE.format(package="has-types"))
    assert "E013" not in result.codes, (
        f"a resolvable package must not report E013, got: {result.codes}"
    )
    assert result.errors == [], (
        f"a resolvable package must report no error, got: "
        f"{[d.get('message', '') for d in result.errors]}"
    )


@pytest.mark.parametrize(
    "specifier,installed",
    [
        ("has-types/sub/path", "has-types"),
        ("@scope/pkg", "@scope/pkg"),
    ],
)
def test_installed_package_variants_report_no_e013(lsp, tmp_path, specifier, installed):
    uri = project(tmp_path, {installed: DECLARATION})
    result = open_doc(lsp, uri, SOURCE.format(package=specifier))
    assert "E013" not in result.codes, (
        f"`{specifier}` resolves to an installed package, got: {result.codes}"
    )


NODE_SOURCE = """import trusted {{ randomUUID }} from "{specifier}"

export let main() -> string = {{
    randomUUID()
}}
"""


@pytest.mark.parametrize("specifier", ["node:crypto", "crypto"])
def test_a_node_builtin_without_types_node_is_not_an_error(lsp, tmp_path, specifier):
    """Node supplies the module, so `cannot find module` is false about it.

    A Bun or Deno project, or any Node project that never adds
    `@types/node`, must still build. Only the declarations are missing,
    which is W004 (#1465).
    """
    uri = project(tmp_path, {})
    result = open_doc(lsp, uri, NODE_SOURCE.format(specifier=specifier))
    assert "E013" not in result.codes, (
        f"`{specifier}` is a Node builtin and must not report E013, got: {result.codes}"
    )
    assert result.errors == [], (
        f"`{specifier}` must not fail the build, got: "
        f"{[d.get('message', '') for d in result.errors]}"
    )


def test_a_node_builtin_warns_and_names_the_types_package(lsp, tmp_path):
    uri = project(tmp_path, {})
    result = open_doc(lsp, uri, NODE_SOURCE.format(specifier="node:crypto"))
    helps = " ".join(str(d.get("message", "")) for d in result.all)
    assert "W004" in result.codes, (
        f"a builtin with no declarations must warn W004, got: {result.codes}"
    )
    assert "node:crypto" in helps, (
        f"the warning must name the module, got: {helps}"
    )
