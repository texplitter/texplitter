"""Stress and regression tests for the Python shim layer.

Test buckets:

1. Deep nesting: synthesize a 1000-level <div> tree and verify the pipeline doesn't stack overflow.
2. Large file: synthesize a ~1 MB HTML blob and verify end-to-end segmentation completes in a
   reasonable time.
3. Round-trip property: verify tokenize produces tokens whose concatenated text recovers the
   original input (modulo the documented '." <-> ".' normalization in tokenizer.py).
4. Duplicate-type correction fixtures: the three inputs that exercise the three duplicate-`type`
   rule pairs in config/rules/default.json. Both variants must survive as distinct `CLASS__type`
   named capture groups so the combined alternation regex dispatches each to its own callback.
"""

import time
import pytest

from texplitter.tokenizer import Tokenizer
from texplitter.splitter import SegmentTree


# --------------------------------------------------------------------------- #
# 1. Deep-nesting stress test                                                 #
# --------------------------------------------------------------------------- #

def test_deep_nesting_1000_levels():
    """A 1000-level <div> tree must not stack-overflow the tokenizer or SegmentTree build.

    SegmentTree walks are iterative in the Rust core, but this test guards against any future
    regression that reintroduces unbounded recursion. Sphinx-generated docs routinely hit
    20+ levels of nesting; synthetic inputs can be deeper.
    """
    depth = 1000
    opens = "<div>" * depth
    closes = "</div>" * depth
    html = opens + "content" + closes

    tok = Tokenizer()
    tokens = tok.tokenize(html)
    assert len(tokens) == 2 * depth + 1, (
        f"expected {2 * depth + 1} tokens (open+close per level + content), got {len(tokens)}"
    )

    import sys
    original_limit = sys.getrecursionlimit()
    sys.setrecursionlimit(max(10000, original_limit))
    try:
        tree = SegmentTree(tokens)
        assert tree is not None
    finally:
        sys.setrecursionlimit(original_limit)


# --------------------------------------------------------------------------- #
# 2. Large-file stress test                                                   #
# --------------------------------------------------------------------------- #

def _build_1mb_html_blob() -> str:
    """Synthesize a ~1 MB HTML blob with a realistic Sphinx-like structure.

    5000 sections is ~910 KB; enough to exercise the regex engine, tree builder, and placeholder
    generation without being so large that CI runs take forever.
    """
    parts = ["<html><body>"]
    for i in range(5000):
        parts.append(
            f'<div class="section"><h2>Section {i}</h2>'
            f"<p>This is paragraph {i} with <em>emphasis</em> and "
            f'<code>inline code</code> and a <a href="#">link</a>. '
            f"Some more text to pad it out.</p></div>"
        )
    parts.append("</body></html>")
    return "".join(parts)


def test_large_file_tokenize_under_10s():
    """Tokenization of ~1 MB HTML should complete well under 10s on a typical dev machine.

    The 10s ceiling is a catastrophic-regression bound, not a micro-benchmark.
    """
    html = _build_1mb_html_blob()
    assert len(html) >= 900 * 1024, f"test blob smaller than expected: {len(html)} bytes"

    tok = Tokenizer()
    t0 = time.perf_counter()
    tokens = tok.tokenize(html)
    elapsed = time.perf_counter() - t0

    assert len(tokens) > 100_000, f"expected >100k tokens from 1 MB blob, got {len(tokens)}"
    assert elapsed < 10.0, f"tokenize took {elapsed:.2f}s, expected <10s"
    print(f"\n[baseline] tokenize 1 MB: {elapsed:.2f}s ({len(tokens)} tokens)")


def test_large_file_segment_tree_under_30s():
    """SegmentTree build on ~1 MB HTML should complete well under 30s on a typical dev machine.

    The 30s ceiling is a catastrophic-regression bound, not a micro-benchmark.
    """
    html = _build_1mb_html_blob()
    tok = Tokenizer()
    tokens = tok.tokenize(html)

    t0 = time.perf_counter()
    tree = SegmentTree(tokens)
    elapsed = time.perf_counter() - t0

    assert tree is not None
    assert elapsed < 30.0, f"SegmentTree build took {elapsed:.2f}s, expected <30s"
    print(f"\n[baseline] SegmentTree build 1 MB: {elapsed:.2f}s")


# --------------------------------------------------------------------------- #
# 3. Round-trip property test                                                 #
# --------------------------------------------------------------------------- #

@pytest.mark.parametrize("text", [
    "Hello World",
    "<p>Simple paragraph.</p>",
    "<div><p>Nested <em>emphasis</em> inside paragraph.</p></div>",
    "<html><body><h1>Title</h1><p>Body text with <a href='#'>link</a>.</p></body></html>",
    "Line one.\nLine two.\nLine three.",
])
def test_tokenize_roundtrip(text):
    """For any input that doesn't trigger normalization, tokenize should produce tokens whose
    concatenated text recovers the input byte-for-byte.

    The documented normalization in `Tokenizer.tokenize` swaps `."` <-> `".` and `.'` <-> `'.`,
    so inputs containing those sequences would differ. This test parametrizes only inputs that
    don't trigger the normalization.
    """
    tok = Tokenizer()
    tokens = tok.tokenize(text)
    recovered = "".join(t["text"] for t in tokens)
    assert recovered == text, f"round-trip failed:\n  in:  {text!r}\n  out: {recovered!r}"


# --------------------------------------------------------------------------- #
# 4. Duplicate-type correction fixtures                                       #
# --------------------------------------------------------------------------- #
#
# These three inputs exercise the three duplicate-`type` rule pairs in
# config/rules/default.json:
#
#   | type          | variant A            | variant B                 |
#   |---------------|----------------------|---------------------------|
#   | dash          | BREAK ` - `          | PUNCTUATION `-`           |
#   | vertical_bar  | BREAK ` | `          | PUNCTUATION `|`           |
#   | xml_entity    | SYMBOL `{foo-bar}`   | SINGLE_TAG `&abc;`        |
#
# Both variants must survive as distinct `CLASS__type` named capture groups
# in the combined alternation regex so each dispatches to its own callback.

def test_duplicate_type_dash_break():
    """` - ` between words must tokenize as a single BREAK/dash token."""
    tok = Tokenizer()
    tokens = tok.tokenize("sentence - another sentence")
    break_tokens = [t for t in tokens if t["token_class"] == "BREAK" and t["token_type"] == "dash"]
    assert len(break_tokens) == 1, (
        f"expected one BREAK/dash token for ' - ', got {len(break_tokens)}: {tokens}"
    )
    assert break_tokens[0]["text"] == " - "


def test_duplicate_type_vertical_bar_break():
    """` | ` between items must tokenize as a single BREAK/vertical_bar token."""
    tok = Tokenizer()
    tokens = tok.tokenize("item | other item")
    break_tokens = [t for t in tokens if t["token_class"] == "BREAK" and t["token_type"] == "vertical_bar"]
    assert len(break_tokens) == 1, (
        f"expected one BREAK/vertical_bar token for ' | ', got {len(break_tokens)}: {tokens}"
    )
    assert break_tokens[0]["text"] == " | "


def test_duplicate_type_xml_entity_symbol():
    """`{placeholder-name}` must tokenize as a single SYMBOL/xml_entity token.

    Note: the rule is misnamed — it matches curly-brace placeholder variables, not XML entities.
    """
    tok = Tokenizer()
    tokens = tok.tokenize("see {placeholder-name} here")
    symbol_tokens = [t for t in tokens if t["token_class"] == "SYMBOL" and t["token_type"] == "xml_entity"]
    assert len(symbol_tokens) == 1, (
        f"expected one SYMBOL/xml_entity token for '{{placeholder-name}}', got {len(symbol_tokens)}: {tokens}"
    )
    assert "{placeholder-name}" in symbol_tokens[0]["text"]


def test_duplicate_type_xml_entity_single_tag():
    """`&amp;` must tokenize as a single SINGLE_TAG/xml_entity token (HTML entity)."""
    tok = Tokenizer()
    tokens = tok.tokenize("see &amp; here")
    single_tag_tokens = [t for t in tokens if t["token_class"] == "SINGLE_TAG" and t["token_type"] == "xml_entity"]
    assert len(single_tag_tokens) == 1, (
        f"expected one SINGLE_TAG/xml_entity token for '&amp;', got {len(single_tag_tokens)}: {tokens}"
    )
    assert single_tag_tokens[0]["text"] == "&amp;"
