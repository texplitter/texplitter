"""Real documents and the cases cut from them that found defects.

A test marked xfail records a defect that is not fixed, with the input that
shows it; `strict=True` turns it red once it passes, so the marker is
removed with the fix. Every expectation was checked against the deployed
texplitter.
"""

from pathlib import Path

import pytest

from texplitter import segment

FIXTURES = Path(__file__).parent / "fixtures"


def texts(doc):
    return [s.text for s in segment(doc).segments]


# --- documents ----------------------------------------------------------------


@pytest.mark.parametrize("name,sentence", [
    ("p17-chap-12-en.html", "The 2025 rate for business use of a vehicle is 70 cents a mile."),
    ("p17-chap-12-es.html", "La tarifa para uso comercial de un vehículo es 70 centavos por milla para 2025."),
])
def test_an_epub_chapter_is_split_into_sentences(name, sentence):
    """The IRS chapter came back as one 42,000-character segment before 90df64e and a790a77."""
    result = texts((FIXTURES / name).read_text(encoding="utf-8"))
    assert len(result) > 300
    assert max(len(t) for t in result) < 1000
    assert any(sentence in t for t in result)


@pytest.mark.xfail(strict=True, reason="text rules run inside attribute values: a ')' and '.' in a title attribute leave a stray </div>")
@pytest.mark.parametrize("name", ["p17-chap-12-en.html", "p17-chap-12-es.html"])
def test_an_epub_chapter_leaves_no_markup_in_its_segments(name):
    result = texts((FIXTURES / name).read_text(encoding="utf-8"))
    assert not [t for t in result if "</div>" in t]


@pytest.mark.xfail(strict=True, reason="<meta charset=...> without a slash is counted as an open element and the last paragraph is dropped")
def test_the_last_paragraph_survives_a_head_with_a_void_meta():
    result = texts((FIXTURES / "treasure-island-en.html").read_text(encoding="utf-8"))
    assert result[-1] == "It was the second death I had known, and the sorrow of the first was still fresh in my heart."


# --- markup that works and must keep working ---------------------------------


def test_html_may_leave_out_html_head_and_body():
    assert texts("<!DOCTYPE html><title>T</title><p>A one.</p><p>B two. C three.</p>") == ["T", "A one.", "B two.", "C three."]


def test_an_image_alt_text_is_kept_as_a_translatable_attribute():
    """alt is read through the attribute path, not as segment text; a fix of the inline cases must keep this."""
    result = segment('<html><body>\n<p>A one. <img src="x.png" alt="A map"/> More.</p>\n<p>B two.</p>\n</body></html>')
    assert ("img/alt~0", "A map") in [tuple(a) for a in result.attributes]
    assert ("img/src~0", "x.png") in [tuple(a) for a in result.attributes]


# --- defects in inline self-closing elements ---------------------------------


def test_text_after_an_xliff_inline_code_is_kept():
    """An XLIFF 2.1 segment as the Format Service hands it over (04 §2.1): <ph/> is an inline code.
    The text after it was labelled markup and dropped ("Click {#1} .")."""
    doc = 'Click <ph id="1" dataRef="d1"/> to continue. Then stop.'
    result = segment(doc)
    assert [s.text for s in result.segments] == ["Click {#1} to continue.", "Then stop."]
    assert result.rebuild() == doc


def test_text_after_an_inline_image_is_kept():
    doc = '<p>Press <b>Start</b> and <img src="i.png" alt="the icon"/> wait. Then go.</p>'
    result = segment(doc)
    assert [s.text for s in result.segments][0].endswith("wait.")
    assert result.rebuild() == doc


def test_a_line_break_inside_a_sentence_stays_an_inline_placeholder():
    """90df64e took <br/> out of the text run here ("Pressand wait."); the original library kept it inline."""
    assert texts("<p>Press <br/> and wait. Then go.</p>") == ["Press {#1} and wait.", "Then go."]


@pytest.mark.xfail(strict=True, reason="<img> without a slash is counted as an open element; the paragraph after it is lost")
def test_the_paragraph_after_a_void_image_survives():
    assert texts('<html><body>\n<p>A one. <img src="x.png" alt="A map"> More.</p>\n<p>B two.</p>\n</body></html>')[-1] == "B two."


@pytest.mark.xfail(strict=True, raises=BaseException,
                   reason="parentheses and a period in an attribute value panic the splitter (splitter.rs: slice index starts at 39 but ends at 38)")
def test_parentheses_and_a_period_in_an_attribute_value():
    assert texts('<div><section title="Title (with parens)."><h2>Head</h2><div>One. Two.</div></section></div>') == ["Head", "One.", "Two."]


# --- sentence breaks on rebuild -----------------------------------------------


@pytest.mark.parametrize("doc", [
    "<p>One. Two.</p>",
    "<p>A one. B two. C three.</p>",
    "<p>Is it? Yes. Go!</p>",
    "<p>Hello <b>world</b>. Goodbye <i>friend</i>.</p>",
    "<p>Leading <b>x</b> end.</p>\n<p> spaced </p>",
])
def test_a_document_rebuilds_exactly_from_its_own_segments(doc):
    """The break between sentences ('. ', '?', '!') was written twice: once in the
    sentence before it, once in the template ('One.{sp}. Two.', 'Go!!')."""
    assert segment(doc).rebuild() == doc


def test_translated_sentences_rebuild_with_one_space_between_them():
    result = segment("<p>Hello <b>world</b>. Goodbye <i>friend</i>.</p>")
    result.segments[0].text = "Hola {#1}mundo{/#1}."
    result.segments[1].text = "Adiós {#2}amigo{/#2}."
    assert result.rebuild() == "<p>Hola <b>mundo</b>. Adiós <i>amigo</i>.</p>"
