"""Tests for the AsciiDoc ruleset.

Verifies that representative AsciiDoc constructs are tokenized with the
correct token classes when using the ``asciidoc`` ruleset.
"""

from texplitter import Tokenizer, SegmentTree, load_regex_patterns, list_rulesets


def _token_classes(tokens):
    """Return the list of (token_class, text) pairs for debugging."""
    return [(t["token_class"], t["text"]) for t in tokens]


# ---- ruleset discovery ----

def test_asciidoc_ruleset_is_listed():
    rulesets = list_rulesets()
    assert "asciidoc" in rulesets
    assert "default" in rulesets


def test_asciidoc_ruleset_loads():
    patterns = load_regex_patterns("asciidoc")
    assert len(patterns) > 0
    # Should have both AsciiDoc-specific and inherited rules
    types = {p["type"] for p in patterns}
    assert "asciidoc_section_title" in types
    assert "lowercase_word" in types


# ---- Tokenizer constructor with ruleset name ----

def test_tokenizer_accepts_ruleset_name():
    tok = Tokenizer("asciidoc")
    tokens = tok.tokenize("Hello world")
    texts = [t["text"] for t in tokens]
    assert "Hello" in texts
    assert "world" in texts


# ---- AsciiDoc-specific token classes ----

def test_attribute_reference():
    tok = Tokenizer("asciidoc")
    tokens = tok.tokenize("The {product-name} is ready")
    attr_tokens = [t for t in tokens if "product-name" in t["text"]]
    assert len(attr_tokens) == 1
    assert attr_tokens[0]["token_class"] == "SINGLE_TAG"


def test_cross_reference():
    tok = Tokenizer("asciidoc")
    tokens = tok.tokenize("See <<chapter-1>> for details")
    classes = {t["text"]: t["token_class"] for t in tokens}
    assert classes["<<chapter-1>>"] == "SINGLE_TAG"


def test_cross_reference_with_label():
    tok = Tokenizer("asciidoc")
    tokens = tok.tokenize("See <<chapter-1,Chapter One>> for details")
    xref_tokens = [t for t in tokens if "<<" in t["text"]]
    assert len(xref_tokens) == 1
    assert xref_tokens[0]["token_class"] == "SINGLE_TAG"


def test_inline_macro():
    tok = Tokenizer("asciidoc")
    tokens = tok.tokenize("Visit link:https://example.com[our site] today")
    macro_tokens = [t for t in tokens if "link:" in t["text"]]
    assert len(macro_tokens) == 1
    assert macro_tokens[0]["token_class"] == "SINGLE_TAG"


def test_block_anchor():
    tok = Tokenizer("asciidoc")
    tokens = tok.tokenize("[[my-section]]")
    anchor_tokens = [t for t in tokens if "[[" in t["text"]]
    assert len(anchor_tokens) == 1
    assert anchor_tokens[0]["token_class"] == "SINGLE_TAG"


def test_callout_marker():
    tok = Tokenizer("asciidoc")
    tokens = tok.tokenize("some code <1>")
    callout_tokens = [t for t in tokens if t["text"].strip() == "<1>"]
    assert len(callout_tokens) == 1
    assert callout_tokens[0]["token_class"] == "SINGLE_TAG"


def test_superscript_and_subscript():
    tok = Tokenizer("asciidoc")

    # Superscript ^text^ is a single protected token (SINGLE_TAG).
    # Content like ^2^ or ^[1]^ is notation/annotation, not translatable prose.
    tokens = tok.tokenize("E is mc ^2^ plus")
    super_tokens = [t for t in tokens if "^" in t["text"]]
    assert any(t["token_class"] == "SINGLE_TAG" for t in super_tokens)

    tokens = tok.tokenize("H ~2~ O")
    sub_tokens = [t for t in tokens if "~" in t["text"]]
    assert any(t["token_class"] == "SINGLE_TAG" for t in sub_tokens)


def test_superscript_footnote_ref():
    """Superscript wrapping a footnote reference: ^[1]^ is one protected token."""
    tok = Tokenizer("asciidoc")
    tokens = tok.tokenize("There is a footer note ^[1]^")
    super_tokens = [t for t in tokens if t["text"] == "^[1]^"]
    assert len(super_tokens) == 1
    assert super_tokens[0]["token_class"] == "SINGLE_TAG"


# ---- segment tree round-trip ----

def test_segment_tree_with_asciidoc():
    """Build a segment tree from AsciiDoc tokens and verify round-trip."""
    tok = Tokenizer("asciidoc")
    text = "The {product-name} is ready for release"
    tokens = tok.tokenize(text)
    tree = SegmentTree(tokens)
    segments = tree.tree.extract_segments()
    assert len(segments) > 0


def test_bold_inline_markup():
    tok = Tokenizer("asciidoc")
    tokens = tok.tokenize("This is *bold* text")
    classes = _token_classes(tokens)
    # The bold markers should be OPEN_BRACKET / CLOSE_BRACKET with
    # translatable content between them.
    open_idx = next(i for i, (cls, txt) in enumerate(classes) if cls == "OPEN_BRACKET" and "*" in txt)
    close_idx = next(i for i, (cls, txt) in enumerate(classes) if cls == "CLOSE_BRACKET" and "*" in txt)
    assert open_idx < close_idx
    # Content between the markers should be translatable
    between = classes[open_idx + 1 : close_idx]
    assert any(cls == "TRANSLATABLE" for cls, _ in between)


def test_italic_inline_markup():
    tok = Tokenizer("asciidoc")
    tokens = tok.tokenize("This is _italic_ text")
    classes = _token_classes(tokens)
    open_idx = next(i for i, (cls, txt) in enumerate(classes) if cls == "OPEN_BRACKET" and "_" in txt)
    close_idx = next(i for i, (cls, txt) in enumerate(classes) if cls == "CLOSE_BRACKET" and "_" in txt)
    assert open_idx < close_idx
    between = classes[open_idx + 1 : close_idx]
    assert any(cls == "TRANSLATABLE" for cls, _ in between)


def test_monospace_inline_markup():
    tok = Tokenizer("asciidoc")
    tokens = tok.tokenize("Use `command` here")
    classes = _token_classes(tokens)
    open_idx = next(i for i, (cls, txt) in enumerate(classes) if cls == "OPEN_BRACKET" and "`" in txt)
    close_idx = next(i for i, (cls, txt) in enumerate(classes) if cls == "CLOSE_BRACKET" and "`" in txt)
    assert open_idx < close_idx
    between = classes[open_idx + 1 : close_idx]
    assert any(cls == "TRANSLATABLE" for cls, _ in between)
