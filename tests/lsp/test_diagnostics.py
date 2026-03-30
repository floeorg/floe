"""Tests for LSP diagnostics: valid files produce no errors, invalid files produce correct error codes."""

import pytest

from .conftest import URI, open_doc
from . import fixtures as F


# ── Valid files: no errors expected ──────────────────────────────


VALID_SOURCES = [
    ("simple", F.SIMPLE),
    ("types + unions", F.TYPES),
    ("pipes", F.PIPES),
    ("Result/?", F.RESULT),
    ("for-block", F.FORBLOCK),
    ("nested match", F.NESTED_MATCH),
    ("multiple fns", F.MULTIPLE_FNS),
    ("tuple", F.TUPLE_FILE),
    ("option", F.OPTION_FILE),
    ("trait impl", F.TRAIT_FILE),
    ("spread type", F.SPREAD_FILE),
    ("record spread", F.RECORD_SPREAD),
    ("closure assign", F.CLOSURE_ASSIGN),
    ("string literal union", F.STRING_LITERAL_UNION),
    ("collect/error accumulation", F.COLLECT_FILE),
    ("default params", F.DEFAULT_PARAMS),
    ("when guards", F.WHEN_GUARD),
    ("large union (14 variants)", F.LARGE_UNION),
    ("inner const", F.INNER_CONST),
    ("todo/unreachable", F.TODO_UNREACHABLE),
    ("import for-block", F.IMPORT_FOR),
    ("closures", F.CLOSURE_FILE),
    ("dot shorthand", F.DOT_SHORTHAND),
    ("placeholder/partial app", F.PLACEHOLDER),
    ("range match", F.RANGE_MATCH),
    ("array patterns", F.ARRAY_PATTERN),
    ("string patterns", F.STRING_PATTERN),
    ("pipe into match", F.PIPE_INTO_MATCH),
    ("newtype wrappers", F.NEWTYPE_WRAPPER),
    ("newtypes", F.NEWTYPE),
    ("opaque types", F.OPAQUE_TYPE),
    ("tuple index access", F.TUPLE_INDEX),
    ("deriving", F.DERIVING),
    ("test blocks", F.TEST_BLOCK),
    ("unreachable", F.UNREACHABLE),
    ("map/set stdlib", F.MAP_SET),
    ("structural equality", F.STRUCTURAL_EQ),
    ("inline for", F.INLINE_FOR),
    ("number separators", F.NUMBER_SEPARATOR),
    ("multi-depth match", F.MULTI_DEPTH_MATCH),
    ("qualified variants", F.QUALIFIED_VARIANT),
    ("generic functions", F.GENERIC_FN),
    ("multiline pipe", F.MULTILINE_PIPE),
]


@pytest.mark.parametrize("name,source", VALID_SOURCES, ids=[s[0] for s in VALID_SOURCES])
def test_valid_file_no_errors(lsp, name, source):
    result = open_doc(lsp, URI, source)
    assert result.errors == [], f"Expected no errors for '{name}', got: {[e.get('message','') for e in result.errors]}"


# ── Error detection ──────────────────────────────────────────────


def test_banned_keywords_produce_errors(lsp):
    """let/var/class/enum should produce parse errors."""
    result = open_doc(lsp, URI, F.ERRORS_BANNED_KEYWORDS)
    assert len(result.errors) > 0, "Expected errors for banned keywords"


def test_shadowing_detected(lsp):
    """Redeclaring a variable in the same scope should error."""
    result = open_doc(lsp, URI, F.SHADOWING)
    assert len(result.all) > 0, "Expected shadowing diagnostic"
    messages = " ".join(d.get("message", "").lower() for d in result.all)
    assert any(kw in messages for kw in ["already defined", "shadow", "redecl"]), (
        f"Expected shadowing message, got: {messages}"
    )


def test_undefined_variable(lsp):
    """Using an undefined variable should produce E002."""
    result = open_doc(lsp, URI, F.UNDEFINED_VAR)
    assert len(result.errors) > 0, "Expected undefined variable error"
    messages = " ".join(d.get("message", "").lower() for d in result.errors)
    assert any(kw in messages for kw in ["undefined", "not defined", "undeclared"]), (
        f"Expected undefined variable message, got: {messages}"
    )


def test_type_mismatch(lsp):
    """Assigning number to string should produce E001."""
    result = open_doc(lsp, URI, F.TYPE_MISMATCH)
    assert "E001" in result.codes, f"Expected E001, got codes: {result.codes}"


# ── Exhaustiveness checking (E004) ──────────────────────────────


@pytest.mark.parametrize(
    "name,source",
    [
        ("union missing variants", F.MATCH_EXHAUSTIVE),
        ("partial match Color", F.PARTIAL_MATCH),
        ("number without wildcard", F.MATCH_NUMBER_NO_WILDCARD),
        ("string without wildcard", F.MATCH_STRING_NO_WILDCARD),
        ("number guards without wildcard", F.MATCH_NUMBER_GUARDS_NO_WILDCARD),
        ("ranges without wildcard", F.MATCH_RANGES_NO_WILDCARD),
        ("tuple missing cases", F.MATCH_TUPLE_MISSING),
    ],
)
def test_non_exhaustive_match(lsp, name, source):
    """Non-exhaustive matches should produce E004."""
    result = open_doc(lsp, URI, source)
    assert "E004" in result.codes, (
        f"Expected E004 for '{name}', got codes: {result.codes}, "
        f"messages: {[d.get('message','') for d in result.all]}"
    )


# ── Qualified/ambiguous variants ─────────────────────────────────


def test_ambiguous_variant_with_qualification_ok(lsp):
    """Qualified + unambiguous bare variants should have no errors."""
    result = open_doc(lsp, URI, F.AMBIGUOUS_VARIANT)
    assert result.errors == [], f"Expected no errors, got: {[e.get('message','') for e in result.errors]}"


def test_bare_ambiguous_variant_errors(lsp):
    """Using a bare ambiguous variant should error."""
    source = 'type Color { | Red | Green | Blue }\ntype Light { | Red | Yellow | Green }\nconst _x = Red\n'
    result = open_doc(lsp, URI, source)
    messages = " ".join(e.get("message", "").lower() for e in result.errors)
    assert "ambiguous" in messages, f"Expected ambiguous error, got: {[e.get('message','') for e in result.errors]}"


# ── Generic functions ────────────────────────────────────────────


def test_generic_fn_no_type_mismatch(lsp):
    """Generic functions should not produce E001 type mismatch errors."""
    result = open_doc(lsp, URI, F.GENERIC_FN)
    type_errs = [c for c in result.codes if c == "E001"]
    assert type_errs == [], f"Expected no E001 errors for generics, got: {[e.get('message','') for e in result.errors]}"
