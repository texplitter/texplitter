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


@pytest.mark.parametrize("name", ["p17-chap-12-en.html", "p17-chap-12-es.html"])
def test_an_epub_chapter_leaves_no_markup_in_its_segments(name):
    """A title attribute ending in '.' lost its closing '">' to the quote swap, leaving
    stray </div> in segments."""
    result = texts((FIXTURES / name).read_text(encoding="utf-8"))
    assert not [t for t in result if "</div>" in t]


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


def test_the_paragraph_after_a_void_image_survives():
    """HTML never closes a void element; <img> without a slash was waited for as an
    open element, and the text after it was lost."""
    doc = '<html><body>\n<p>A one. <img src="x.png" alt="A map"> More.</p>\n<p>B two.</p>\n</body></html>'
    result = segment(doc)
    assert [s.text for s in result.segments] == ["A one.", "{#1} More.", "B two."]
    assert ("img/alt~0", "A map") in [tuple(a) for a in result.attributes]
    assert result.rebuild() == doc


@pytest.mark.parametrize("doc,expected", [
    ('<html><head><meta charset="utf-8"><title>T</title></head><body>\n<p>A one.</p>\n<p>B two.</p>\n</body></html>', ["T", "A one.", "B two."]),
    ("<p>One line<hr>Two line.</p>", ["One line{#1}Two line."]),
    ('<div><input type="text" value="x"><p>After.</p></div>', ["After."]),
])
def test_void_elements_without_a_slash_close_themselves(doc, expected):
    result = segment(doc)
    if expected is not None:
        assert [s.text for s in result.segments] == expected
    assert result.rebuild() == doc


def test_an_xml_link_with_content_is_not_void():
    """In RSS and other XML, <link> and <meta> hold content; only an element with no
    closing tag is taken as void."""
    doc = "<item><title>News today.</title><link>https://example.com/a</link><description>A short item.</description></item>"
    result = segment(doc)
    assert "News today." in [s.text for s in result.segments]
    assert "A short item." in [s.text for s in result.segments]
    assert result.rebuild() == doc


def test_parentheses_and_a_period_in_an_attribute_value():
    """The period-after-quote swap ran inside markup: title="Title (with parens)." lost
    its closing '">', the element never closed and the splitter panicked."""
    doc = '<div><section title="Title (with parens)."><h2>Head</h2><div>One. Two.</div></section></div>'
    result = segment(doc)
    assert [s.text for s in result.segments] == ["Head", "One.", "Two."]
    assert ("section/title~0", "Title (with parens).") in [tuple(a) for a in result.attributes]
    assert result.rebuild() == doc


# --- punctuation and closing quotes (American English to logical placement) ---


@pytest.mark.parametrize("source,expected", [
    ('<p>He called it "the end."</p>', 'He called it "the end".'),
    ("<p>He called it 'the end.'</p>", "He called it 'the end'."),
    ("<p>He called it “the end.”</p>", "He called it “the end”."),
    ("<p>He called it ‘the end.’</p>", "He called it ‘the end’."),
    ('<p>"The end," he said.</p>', '"The end", he said.'),
    ("<p>“The end,” he said.</p>", "“The end”, he said."),
])
def test_a_period_or_comma_inside_closing_quotes_moves_after_them(source, expected):
    """American English writes the period and comma inside the closing quotes; Spanish,
    like most languages, after them (RAE). The swap now covers the comma and
    typographic quotes."""
    assert texts(source) == [expected]


def test_the_quote_swap_never_touches_markup():
    doc = '<p title="He said &quot;go.&quot;" data-x="a." class=\'b.\'>He said "go."</p>'
    result = segment(doc)
    assert [s.text for s in result.segments] == ['He said "go".']
    attrs = dict(tuple(a) for a in result.attributes)
    assert attrs["p/data-x~0"] == "a."
    assert attrs["p/class~0"] == "b."


def test_apostrophes_are_not_quotes_for_the_swap():
    assert texts("<p>It's the dogs' toys.</p>") == ["It's the dogs' toys."]


def test_the_swap_can_be_turned_off_for_a_source_that_is_not_american():
    doc = '<p>He called it "the end."</p>'
    result = segment(doc, move_punctuation_after_quotes=False)
    assert [s.text for s in result.segments] == ['He called it "the end."']
    assert result.rebuild() == doc


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


def test_a_void_element_in_xliff_inline_format_is_a_ph():
    result = segment('<p>See <img src="x.png" alt="map"> here.</p>', inline_format="xliff")
    assert [s.text for s in result.segments] == ['See <ph id="1" dataRef="d1"/> here.']


# --- question and exclamation marks -------------------------------------------


@pytest.mark.parametrize("source,expected", [
    # Spanish (RAE): after ? or !, a lower-case word continues the same sentence.
    ("<p>—¡Mil carronadas! gritó él, ¡una semana!</p>", ["—¡Mil carronadas! gritó él, ¡una semana!"]),
    ("<p>—¿Herido? dijo el Doctor; ¡qué va á estarlo! Ni más ni menos.</p>",
     ["—¿Herido? dijo el Doctor; ¡qué va á estarlo!", "Ni más ni menos."]),
    ("<p>—¡Ya está hecho! dijo entonces el ciego y se fue.</p>", ["—¡Ya está hecho! dijo entonces el ciego y se fue."]),
    # English: a speech tag after the closing quote continues the sentence.
    ("<p>“Were you addressing me, sir?” says the doctor. He waited.</p>",
     ["“Were you addressing me, sir?” says the doctor.", "He waited."]),
    # A new sentence after the mark, with or without a closing quote.
    ('<p>"Go!" He left.</p>', ['"Go!"', "He left."]),
    ("<p>Is it? Yes. Go!</p>", ["Is it?", "Yes.", "Go!"]),
    ("<p>What?! No way.</p>", ["What?!", "No way."]),
    ("<p>¿Vienes? ¿O no?</p>", ["¿Vienes?", "¿O no?"]),
    # Not a sentence end: a question mark in a URL.
    ("<p>See https://example.com/a?b=1 for more.</p>", ["See https://example.com/a?b=1 for more."]),
])
def test_question_and_exclamation_marks_end_a_sentence_only_before_a_new_one(source, expected):
    result = segment(source)
    assert [s.text for s in result.segments] == expected
    assert result.rebuild() == source


def test_an_exclamation_before_japanese_text_still_ends_the_sentence():
    assert len(texts("<p>すごい!こんにちは</p>")) == 2


# --- a break after wrapped content, and a pair holding a standalone element -----


def test_a_break_right_after_a_wrapper_is_written_once_after_it():
    """The break's text was given to the wrapper's content and written again by the break and by
    every empty sibling: '<p>. <b>Hello. </b>. . Next</p>'."""
    doc = "<p><b>Hello</b>. Next</p>"
    result = segment(doc)
    assert result.rebuild() == doc
    assert texts(doc) == ["Hello", "Next"]


def test_a_pair_holding_a_self_closing_element_rebuilds():
    """{#1}{#2}{/#1}: the pair was never matched because its content held a placeholder."""
    doc = '<p>See <b>the <img src="x.png"/> map</b>. Next</p>'
    assert segment(doc).rebuild() == doc


def test_whitespace_after_the_last_block_is_kept():
    doc = "<p>A one.</p>\n\n"
    assert segment(doc).rebuild() == doc


def test_a_standalone_placeholder_inside_a_pair_validates():
    """{#9}{#10}{/#9} (bold around a code span) was reported as crossed nesting: the standalone {#10} was
    taken for an open mark."""
    result = segment("* *`thinkube.yaml`* describes an app.\n", ruleset="asciidoc")
    assert result.validate() == []
    result.segments[0].text = "{#1}{#2}{/#1} describe una aplicación."
    assert result.validate() == []
    result.segments[0].text = "{#1}{/#1}{#2} describe una aplicación."
    assert result.validate() == []  # moved, still whole
    result.segments[0].text = "{#1}{#2} describe una aplicación."
    assert [e.code for e in result.validate()] == ["orphan_open"]
