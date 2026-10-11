"""Tests for the high-level texplitter.segment() API."""

import texplitter
from texplitter import segment, SegmentResult, Segment, ValidationError


class TestSegmentFunction:
    """Tests for texplitter.segment()."""

    def test_basic_html(self):
        result = segment("<p>Hello world.</p>")
        assert isinstance(result, SegmentResult)
        assert result.template
        assert len(result.segments) >= 1

    def test_returns_template_with_placeholders(self):
        result = segment("<p>Hello.</p>")
        assert "%%" in result.template

    def test_segments_are_full_sentences(self):
        """Segments from generate_placeholders are full translation units,
        not leaf-level decomposed fragments."""
        result = segment('<p>Click <a href="url">here</a> to continue.</p>')
        assert len(result.segments) == 1
        seg = result.segments[0]
        # The segment is a full sentence with numbered placeholders
        assert "Click" in seg.text
        assert "here" in seg.text
        assert "to continue" in seg.text
        assert "{#" in seg.text

    def test_attributes_extracted(self):
        result = segment('<p><a href="url">link</a></p>')
        assert any("href" in k for k, v in result.attributes)

    def test_multi_sentence_produces_multiple_segments(self):
        result = segment("<p>Hello world. Goodbye world.</p>")
        assert len(result.segments) >= 2

    def test_custom_ruleset(self):
        result = segment("== Title\n\nSome text.", ruleset="asciidoc")
        assert isinstance(result, SegmentResult)
        assert len(result.segments) >= 1


class TestSegmentRebuild:
    """Tests for SegmentResult.rebuild()."""

    def test_identity_rebuild(self):
        """Rebuilding without changes restores the original."""
        html = "<p>Hello world.</p>"
        result = segment(html)
        rebuilt = result.rebuild()
        # Structure and attributes restored
        assert "<p>" in rebuilt
        assert "</p>" in rebuilt

    def test_translation_roundtrip(self):
        result = segment('<p>Click <a href="url">here</a> to continue.</p>')
        result.segments[0].text = "Para continuar pulse {#1}aquí{/#1}."
        rebuilt = result.rebuild()
        assert "Para continuar" in rebuilt
        assert 'href="url"' in rebuilt
        assert "{#1}" not in rebuilt

    def test_attribute_placeholders_restored(self):
        result = segment('<p><a href="https://example.com">link</a></p>')
        rebuilt = result.rebuild()
        assert "https://example.com" in rebuilt
        assert "@a/href" not in rebuilt

    def test_multi_sentence_rebuild(self):
        result = segment("<p>Hello <b>world</b>. Goodbye <i>friend</i>.</p>")
        # Translate both segments (preserving numbered placeholders)
        for seg in result.segments:
            if "Hello" in seg.text:
                seg.text = seg.text.replace("Hello", "Hola").replace("world", "mundo")
            elif "Goodbye" in seg.text:
                seg.text = seg.text.replace("Goodbye", "Adiós").replace("friend", "amigo")
        rebuilt = result.rebuild()
        assert "Hola" in rebuilt
        assert "Adiós" in rebuilt


class TestTranslatableAttributes:
    """Tests for the translatable_attrs parameter."""

    def test_default_no_translatable_attrs(self):
        result = segment(
            '<p><img src="ok.png" alt="checkmark"/></p>'
        )
        # All attributes in .attributes, none as segments
        assert all(seg.kind == "text" for seg in result.segments)
        attr_keys = [k for k, v in result.attributes]
        assert any("alt" in k for k in attr_keys)

    def test_translatable_attrs_become_segments(self):
        result = segment(
            '<p><img src="ok.png" alt="green checkmark"/></p>',
            translatable_attrs=["alt"],
        )
        attr_segments = [s for s in result.segments if s.kind == "attribute"]
        assert len(attr_segments) == 1
        assert attr_segments[0].text == "green checkmark"
        assert "alt" in attr_segments[0].key
        # src should NOT be in attribute segments
        assert all("src" not in s.key for s in attr_segments)
        # src should be in non-translatable attributes
        assert any("src" in k for k, v in result.attributes)

    def test_translatable_attr_rebuild(self):
        result = segment(
            '<p><img src="ok.png" alt="green checkmark"/></p>',
            translatable_attrs=["alt"],
        )
        for seg in result.segments:
            if seg.kind == "attribute":
                seg.text = "marca de verificación verde"
        rebuilt = result.rebuild()
        assert "marca de verificación verde" in rebuilt
        assert 'src="ok.png"' in rebuilt

    def test_empty_translatable_attrs(self):
        """Passing empty list means no attributes are translatable."""
        result = segment(
            '<p><img src="ok.png" alt="checkmark"/></p>',
            translatable_attrs=[],
        )
        assert all(seg.kind == "text" for seg in result.segments)

    def test_multiple_translatable_attrs(self):
        result = segment(
            '<p><a href="url" title="tooltip">link</a></p>',
            translatable_attrs=["title"],
        )
        attr_segments = [s for s in result.segments if s.kind == "attribute"]
        assert len(attr_segments) == 1
        assert attr_segments[0].text == "tooltip"
        # href is non-translatable
        assert any("href" in k for k, v in result.attributes)


class TestNumberedInlineFormat:
    """Tests for inline_format='numbered'."""

    def test_paired_tag_becomes_numbered(self):
        result = segment(
            '<p>Click <a href="url">here</a> to continue.</p>',
            inline_format="numbered",
        )
        seg = result.segments[0].text
        assert "{#1}here{/#1}" in seg
        assert "<a" not in seg
        assert "Click" in seg
        assert "to continue" in seg

    def test_multiple_paired_tags(self):
        result = segment(
            '<p>Click <a href="url">here</a> to <b>continue</b>.</p>',
            inline_format="numbered",
        )
        seg = result.segments[0].text
        assert "{#1}here{/#1}" in seg
        assert "{#2}continue{/#2}" in seg

    def test_self_closing_tag(self):
        result = segment(
            '<p>See <img src="x.png"/> here.</p>',
            inline_format="numbered",
        )
        seg = result.segments[0].text
        assert "{#1}" in seg
        assert "<img" not in seg

    def test_nested_tags(self):
        result = segment(
            '<p>Click <a href="u"><b>here</b></a> now.</p>',
            inline_format="numbered",
        )
        seg = result.segments[0].text
        # Inner <b> becomes {#1}, outer <a> becomes {#2}
        assert "here" in seg
        assert "<a" not in seg
        assert "<b" not in seg

    def test_original_data_populated(self):
        result = segment(
            '<p>Click <a href="url">here</a>.</p>',
            inline_format="numbered",
        )
        assert len(result.original_data) > 0
        # Original data should contain the actual tag with restored attrs
        values = " ".join(result.original_data.values())
        assert "url" in values

    def test_rebuild_roundtrip(self):
        result = segment(
            '<p>Click <a href="url">here</a> to continue.</p>',
            inline_format="numbered",
        )
        result.segments[0].text = "Para continuar pulse {#1}aqui{/#1}."
        rebuilt = result.rebuild()
        assert "Para continuar" in rebuilt
        assert 'href="url"' in rebuilt
        assert "{#1}" not in rebuilt

    def test_rebuild_with_reordering(self):
        result = segment(
            '<p>Click <a href="url">here</a> to <b>continue</b>.</p>',
            inline_format="numbered",
        )
        # Swap the order of tags in translation
        result.segments[0].text = "Para {#2}continuar{/#2} pulse {#1}aqui{/#1}."
        rebuilt = result.rebuild()
        assert "<b>continuar</b>" in rebuilt
        assert '<a href="url">aqui</a>' in rebuilt

    def test_default_format_is_numbered(self):
        """Default inline_format is 'numbered'."""
        result = segment('<p>Click <a href="url">here</a>.</p>')
        assert "{#" in result.segments[0].text
        assert "<a" not in result.segments[0].text
        assert len(result.original_data) > 0

    def test_invalid_format_raises(self):
        import pytest
        with pytest.raises(ValueError, match="Unsupported"):
            segment("<p>Hello.</p>", inline_format="invalid")


class TestXliffInlineFormat:
    """Tests for inline_format='xliff'."""

    def test_paired_tag_becomes_pc(self):
        result = segment(
            '<p>Click <a href="url">here</a> to continue.</p>',
            inline_format="xliff",
        )
        seg = result.segments[0].text
        assert "<pc " in seg
        assert "</pc>" in seg
        assert "dataRefStart" in seg
        assert "dataRefEnd" in seg
        assert "<a" not in seg

    def test_self_closing_becomes_ph(self):
        result = segment(
            '<p>See <img src="x.png"/> here.</p>',
            inline_format="xliff",
        )
        seg = result.segments[0].text
        assert "<ph " in seg
        assert 'dataRef="' in seg
        assert "<img" not in seg

    def test_nested_tags(self):
        result = segment(
            '<p>Click <a href="u"><b>here</b></a> now.</p>',
            inline_format="xliff",
        )
        seg = result.segments[0].text
        # Both tags should be <pc> elements, nested
        assert seg.count("<pc ") == 2
        assert seg.count("</pc>") == 2

    def test_original_data_has_tags(self):
        result = segment(
            '<p>Click <a href="url">here</a>.</p>',
            inline_format="xliff",
        )
        assert len(result.original_data) >= 2  # open + close
        values = list(result.original_data.values())
        assert any('<a href="url">' in v for v in values)
        assert any("</a>" in v for v in values)

    def test_rebuild_roundtrip(self):
        result = segment(
            '<p>Click <a href="url">here</a> to continue.</p>',
            inline_format="xliff",
        )
        # Translate keeping pc elements
        seg = result.segments[0].text
        result.segments[0].text = seg.replace(
            "Click", "Pulse"
        ).replace("to continue", "para continuar")
        rebuilt = result.rebuild()
        assert "Pulse" in rebuilt
        assert 'href="url"' in rebuilt
        assert "<pc" not in rebuilt

    def test_attributes_restored_in_original_data(self):
        """Attribute placeholders should be resolved in original_data."""
        result = segment(
            '<p>Click <a href="https://example.com">here</a> now.</p>',
            inline_format="xliff",
        )
        values = " ".join(result.original_data.values())
        assert "https://example.com" in values
        assert "@a/href" not in values


class TestToXliff:
    """Tests for SegmentResult.to_xliff()."""

    def test_basic_xliff_output(self):
        result = segment(
            '<p>Click <a href="url">here</a>.</p>',
            inline_format="numbered",
        )
        xliff = result.to_xliff(source_lang="en", target_lang="es")
        assert 'version="2.0"' in xliff
        assert 'srcLang="en"' in xliff
        assert 'trgLang="es"' in xliff
        assert "<source>" in xliff
        assert "<target>" in xliff
        assert "<originalData>" in xliff

    def test_source_preserved_after_translation(self):
        result = segment(
            '<p>Click <a href="url">here</a>.</p>',
            inline_format="numbered",
        )
        result.segments[0].text = "Pulse {#1}aqui{/#1}."
        xliff = result.to_xliff()
        assert "<source>" in xliff
        assert "here" in xliff  # source preserved
        assert "aqui" in xliff  # target has translation

    def test_pc_elements_in_xliff(self):
        result = segment(
            '<p>Click <b>here</b>.</p>',
            inline_format="numbered",
        )
        xliff = result.to_xliff()
        assert "<pc " in xliff
        assert "</pc>" in xliff
        assert "dataRefStart" in xliff
        assert "dataRefEnd" in xliff

    def test_ph_elements_for_self_closing(self):
        result = segment(
            '<p>See <img src="x.png"/> here.</p>',
            inline_format="numbered",
        )
        xliff = result.to_xliff()
        assert "<ph " in xliff
        assert "dataRef=" in xliff

    def test_data_elements_have_original_tags(self):
        result = segment(
            '<p>Click <a href="url">here</a>.</p>',
            inline_format="numbered",
        )
        xliff = result.to_xliff()
        assert "<data " in xliff
        # Original tag should be XML-escaped in <data>
        assert "&lt;a href=" in xliff

    def test_multi_segment_unique_ids(self):
        """IDs are globally unique across segments."""
        result = segment(
            '<p>Hello <b>world</b>. See <i>this</i>.</p>',
            inline_format="numbered",
        )
        # Segment 1 has {#1}, segment 2 has {#2}
        assert "{#1}" in result.segments[0].text
        assert "{#2}" in result.segments[1].text
        # No collision in original_data
        assert "1" in result.original_data
        assert "2" in result.original_data

    def test_reordered_placeholders_share_data_refs(self):
        result = segment(
            '<p>Click <a href="url">here</a> to <b>continue</b>.</p>',
            inline_format="numbered",
        )
        result.segments[0].text = "Para {#2}continuar{/#2} pulse {#1}aqui{/#1}."
        xliff = result.to_xliff()
        # Both source and target should use same data refs
        assert xliff.count("d1.s") >= 2  # in source + target
        assert xliff.count("d2.s") >= 2

    def test_works_with_default_numbered_format(self):
        """to_xliff works with default (numbered) format."""
        result = segment('<p>Click <b>here</b>.</p>')
        xliff = result.to_xliff()
        assert "<xliff" in xliff
        assert "<pc " in xliff  # numbered placeholders converted to XLIFF


class TestSegmentRepr:
    """Test string representations."""

    def test_segment_repr(self):
        seg = Segment(index=1, text="hello")
        assert "hello" in repr(seg)
        assert "1" in repr(seg)

    def test_segment_result_repr(self):
        result = segment("<p>Hello.</p>")
        r = repr(result)
        assert "SegmentResult" in r
        assert "template" in r


class TestModuleLevelAccess:
    """Test that the API is accessible from the top-level module."""

    def test_segment_function(self):
        assert hasattr(texplitter, "segment")
        assert callable(texplitter.segment)

    def test_segment_result_class(self):
        assert hasattr(texplitter, "SegmentResult")

    def test_segment_class(self):
        assert hasattr(texplitter, "Segment")

    def test_validation_error_class(self):
        assert hasattr(texplitter, "ValidationError")


class TestValidateNumbered:
    """Tests for SegmentResult.validate() with numbered format."""

    def test_valid_translation_passes(self):
        result = segment(
            '<p>Click <a href="url">here</a> to continue.</p>',
            inline_format="numbered",
        )
        result.segments[0].text = "Para continuar pulse {#1}aquí{/#1}."
        assert result.validate() == []

    def test_missing_placeholder(self):
        result = segment(
            '<p>Click <a href="url">here</a> to continue.</p>',
            inline_format="numbered",
        )
        # MT dropped the placeholder entirely
        result.segments[0].text = "Para continuar pulse aquí."
        errors = result.validate()
        assert len(errors) >= 1
        assert any(e.code == "missing_placeholder" for e in errors)
        assert any("1" in e.message for e in errors)

    def test_unknown_placeholder(self):
        result = segment(
            '<p>Click <a href="url">here</a> to continue.</p>',
            inline_format="numbered",
        )
        # MT hallucinated a placeholder
        result.segments[0].text = "Para {#1}aquí{/#1} {#99}fantasma{/#99}."
        errors = result.validate()
        assert any(e.code == "unknown_placeholder" for e in errors)
        assert any("99" in e.message for e in errors)

    def test_orphan_open(self):
        result = segment(
            '<p>Click <a href="url">here</a> to continue.</p>',
            inline_format="numbered",
        )
        # MT kept open but dropped close
        result.segments[0].text = "Para {#1}aquí."
        errors = result.validate()
        assert any(e.code == "orphan_open" for e in errors)

    def test_orphan_close(self):
        result = segment(
            '<p>Click <a href="url">here</a> to continue.</p>',
            inline_format="numbered",
        )
        # MT kept close but dropped open
        result.segments[0].text = "Para aquí{/#1}."
        errors = result.validate()
        assert any(e.code == "orphan_close" for e in errors)

    def test_nesting_violation(self):
        result = segment(
            '<p>Click <a href="url"><b>here</b></a> now.</p>',
            inline_format="numbered",
        )
        # Crossed nesting: {#2} opens, {#1} closes before {#2}
        source = result.segments[0].text
        # Source has properly nested tags; swap the closing order
        result.segments[0].text = "{#2}{#1}aquí{/#2}{/#1}."
        errors = result.validate()
        assert any(e.code == "nesting_violation" for e in errors)

    def test_self_closing_missing(self):
        result = segment(
            '<p>See <img src="x.png"/> here.</p>',
            inline_format="numbered",
        )
        # MT dropped the self-closing placeholder
        result.segments[0].text = "Vea aquí."
        errors = result.validate()
        assert any(e.code == "missing_placeholder" for e in errors)

    def test_multiple_segments_independent(self):
        """Validation checks each segment independently."""
        result = segment(
            '<p>Hello <b>world</b>. See <i>this</i>.</p>',
            inline_format="numbered",
        )
        # First segment is fine
        result.segments[0].text = "Hola {#1}mundo{/#1}."
        # Second segment drops placeholder
        result.segments[1].text = "Vea esto."
        errors = result.validate()
        # Only errors from segment 2
        assert all(e.segment_index == result.segments[1].index for e in errors)

    def test_corrupted_default_detected(self):
        """Default (numbered) format detects corrupted placeholders."""
        result = segment('<p>Click <a href="url">here</a>.</p>')
        result.segments[0].text = "something completely different"
        errors = result.validate()
        assert any(e.code == "missing_placeholder" for e in errors)

    def test_unmodified_passes(self):
        """Unmodified segments always pass."""
        result = segment(
            '<p>Click <a href="url">here</a> to <b>continue</b>.</p>',
            inline_format="numbered",
        )
        assert result.validate() == []


class TestValidateXliff:
    """Tests for SegmentResult.validate() with XLIFF format."""

    def test_valid_xliff_passes(self):
        result = segment(
            '<p>Click <a href="url">here</a> to continue.</p>',
            inline_format="xliff",
        )
        # Keep the same XLIFF structure, just translate text
        seg = result.segments[0].text
        result.segments[0].text = seg.replace(
            "Click", "Pulse"
        ).replace("to continue", "para continuar")
        assert result.validate() == []

    def test_missing_xliff_element(self):
        result = segment(
            '<p>Click <a href="url">here</a> to continue.</p>',
            inline_format="xliff",
        )
        # Strip all XLIFF elements
        result.segments[0].text = "Pulse aquí para continuar."
        errors = result.validate()
        assert any(e.code == "missing_placeholder" for e in errors)

    def test_mismatched_pc_tags(self):
        result = segment(
            '<p>Click <a href="url">here</a> to continue.</p>',
            inline_format="xliff",
        )
        # Remove the closing </pc> but keep the opening
        result.segments[0].text = result.segments[0].text.replace(
            "</pc>", ""
        )
        errors = result.validate()
        assert any(e.code == "orphan_open" for e in errors)


def test_courtesy_titles_do_not_end_a_sentence():
    """English and Spanish titles before a name keep the sentence whole."""
    english = segment("<p>He was far gone. Dr. Livesey came late. Mr. Smith left.</p>")
    assert [s.text for s in english.segments] == ["He was far gone.", "Dr. Livesey came late.", "Mr. Smith left."]
    spanish = segment("<p>El Sr. Pérez llegó. La Sra. Gómez y la Dra. Ruiz salieron. ¿Quieren Vds. saberlo?</p>")
    assert [s.text for s in spanish.segments] == [
        "El Sr. Pérez llegó.", "La Sra. Gómez y la Dra. Ruiz salieron.", "¿Quieren Vds. saberlo?",
    ]
