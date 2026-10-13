"""Tests for the AsciiDoc ruleset.

Verifies that representative AsciiDoc constructs are tokenized with the
correct token classes when using the ``asciidoc`` ruleset.
"""

import pytest

from texplitter import Tokenizer, SegmentTree, load_regex_patterns, list_rulesets, segment


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


def test_a_link_keeps_its_target_and_translates_its_label():
    """link:url[label] and xref:page[label]: the target is the opening mark, the label is prose."""
    tok = Tokenizer("asciidoc")
    classes = _token_classes(tok.tokenize("Visit link:https://example.com[our site] today"))
    assert ("OPEN_BRACKET", "link:https://example.com[") in classes
    assert ("CLOSE_BRACKET", "]") in classes
    result = segment("See xref:guide.adoc[the guide] now.\n", ruleset="asciidoc")
    assert [s.text for s in result.segments] == ["See {#1}the guide{/#1} now."]
    assert result.original_data == {"1": "xref:guide.adoc[...]"}


def test_a_macro_whose_content_is_not_prose_is_one_protected_token():
    tok = Tokenizer("asciidoc")
    classes = _token_classes(tok.tokenize("Press kbd:[Ctrl+C] now"))
    assert ("SINGLE_TAG", "kbd:[Ctrl+C]") in classes


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


def test_monospace_is_one_protected_token():
    """`code` is a command or a name: it reaches the model as a placeholder, never as words."""
    tok = Tokenizer("asciidoc")
    assert ("SINGLE_TAG", "`kubectl get pods`") in _token_classes(tok.tokenize("Run `kubectl get pods` here"))
    result = segment("Run `kubectl get pods` here.\n", ruleset="asciidoc")
    assert [s.text for s in result.segments] == ["Run {#1} here."]


# ---- the thinkube.org documentation's constructs (kubexlat experiments/asciidoc-roundtrip) ----

DOC = (
    "= Page title\n"
    ":page-role: tiles\n"
    "\n"
    "Intro line, wrapped\n"
    "over two lines.\n"
    "\n"
    "== Heading\n"
    "\n"
    "See xref:a.adoc[the guide] and run `kubectl get pods` now.\n"
    "\n"
    "[source,yaml]\n"
    "----\n"
    "key: value. Not a sentence\n"
    "----\n"
    "\n"
    "* first item\n"
    "** nested *bold* item\n"
    ". numbered item\n"
    "\n"
    ".Table title\n"
    "[cols=\"1,2\"]\n"
    "|===\n"
    "| Name | Does\n"
    "| a | b\n"
    "|===\n"
    "\n"
    "Last words.\n"
)


def test_a_document_comes_back_byte_for_byte():
    result = segment(DOC, ruleset="asciidoc", move_punctuation_after_quotes=False)
    assert result.rebuild() == DOC


def test_structure_after_the_first_line_is_recognised():
    """Every rule anchored with ^ or $ needs multi-line mode: the tokenizer compiles with (?sx) only, so
    the AsciiDoc rules carry (?m:...) themselves. A heading on line 7 is a break; the listing block,
    its attribute line and the table delimiters are templates."""
    tok = Tokenizer("asciidoc")
    classes = _token_classes(tok.tokenize(DOC))
    assert ("BREAK_TEMPLATE", "== ") in classes
    assert ("BREAK_TEMPLATE", "[source,yaml]") in classes
    assert ("BREAK_TEMPLATE", "----\nkey: value. Not a sentence\n----") in classes
    assert ("BREAK_TEMPLATE", "|===") in classes
    assert ("BREAK_TEMPLATE", ":page-role: tiles") in classes


def test_the_segments_are_the_prose_only():
    result = segment(DOC, ruleset="asciidoc", move_punctuation_after_quotes=False)
    assert [s.text for s in result.segments] == [
        "Page title", "Intro line, wrapped\nover two lines.", "Heading",
        "See {#1}the guide{/#1} and run {#2} now.",
        "first item", "nested {#3}bold{/#3} item", "numbered item",
        "Table title", "Name", "Does", "a", "b", "Last words."]


def test_a_list_marker_is_not_a_bold_mark():
    """'** item' starts with two asterisks; the list marker rule comes before bold."""
    result = segment("* one\n** two\n", ruleset="asciidoc")
    assert [s.text for s in result.segments] == ["one", "two"]
    assert result.rebuild() == "* one\n** two\n"


def test_a_closing_mark_before_punctuation_closes():
    """'*Playbooks*,' closed with a comma: the closing rule comes before the opening one."""
    result = segment("Choose *Playbooks*, by area.\n", ruleset="asciidoc")
    assert [s.text for s in result.segments] == ["Choose {#1}Playbooks{/#1}, by area."]
    result = segment('Say _"make an app"_, then more.\n', ruleset="asciidoc", move_punctuation_after_quotes=False)
    assert [s.text for s in result.segments] == ['Say {#1}"make an app"{/#1}, then more.']


def test_bold_around_code_rebuilds():
    doc = "* *`thinkube.yaml`* describes an app.\n"
    result = segment(doc, ruleset="asciidoc")
    assert [s.text for s in result.segments] == ["{#1}{#2}{/#1} describes an app."]
    assert result.rebuild() == doc


def test_a_sentence_end_before_a_structural_line_keeps_the_newline():
    """'now.' then '[source,yaml]': the generic symbol rule took '.\n', and the attribute line became prose."""
    doc = "Do it now.\n[source,yaml]\n----\nx\n----\n"
    result = segment(doc, ruleset="asciidoc")
    assert [s.text for s in result.segments] == ["Do it now."]
    assert result.rebuild() == doc


def test_a_definition_term_with_a_link():
    doc = "xref:a.adoc[Page]:: what it adds.\n"
    result = segment(doc, ruleset="asciidoc")
    assert [s.text for s in result.segments] == ["{#1}Page{/#1}::", "what it adds."]
    assert result.rebuild() == doc


def test_the_newline_after_the_last_block_is_kept():
    doc = "|===\n| a\n|===\n"
    assert segment(doc, ruleset="asciidoc").rebuild() == doc


def test_an_unmatched_mark_is_text():
    """A lone * or _ is ordinary text ("5 * 3", "a_b"); an unclosed mark dropped the rest of the document."""
    doc = "Choose *Playbooks, by area. More text.\n"
    result = segment(doc, ruleset="asciidoc")
    assert [s.text for s in result.segments] == ["Choose *Playbooks, by area.", "More text."]
    assert result.rebuild() == doc
    doc = "Say 5 * 3 and a_b here.\n"
    assert [s.text for s in segment(doc, ruleset="asciidoc").segments] == ["Say 5 * 3 and a_b here."]


def test_the_punctuation_move_leaves_protected_tokens_alone():
    """The move of a period out of closing quotes is for prose: a listing block and a code span keep their bytes."""
    doc = 'Text "here." and `"x."` more.\n\n----\n{"a": "b."}\n----\n'
    result = segment(doc, ruleset="asciidoc")
    assert [s.text for s in result.segments] == ['Text "here". and {#1} more.']
    assert result.rebuild() == 'Text "here". and `"x."` more.\n\n----\n{"a": "b."}\n----\n'

