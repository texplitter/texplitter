"""High-level segmentation API.

This module provides the primary public interface to texplitter:
a single ``segment()`` function that takes markup text and returns
classified segments ready for translation or other AI pipeline tasks.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple

from .tokenizer import Tokenizer
from .splitter import SegmentTree


# Regex patterns for inline tag replacement
_SELF_CLOSING_TAG_RE = re.compile(
    r'<([a-zA-Z][a-zA-Z0-9]*)\b([^>]*)/>'
)
_OPEN_TAG_RE = re.compile(
    r'<([a-zA-Z][a-zA-Z0-9]*)\b([^>]*)>'
)
_CLOSE_TAG_RE = re.compile(
    r'</([a-zA-Z][a-zA-Z0-9]*)>'
)


def _numbered_inline(
    segment_text: str,
    attributes: List[Tuple[str, str]],
    start_id: int = 0,
) -> Tuple[str, Dict[str, str], int]:
    """Replace inline HTML tags with simple numbered placeholders.

    Paired tags (``<b>text</b>``) become ``{#N}text{/#N}``.
    Self-closing tags (``<img .../>``) become ``{#N}``.
    Original tag text is stored in ``original_data`` keyed by
    placeholder IDs (``#1``, ``#2``, ...).

    Attribute placeholders (``@tag/attr~N@``) in the original tags
    are restored to their actual values before storing in
    ``original_data``, so the round-trip is lossless.

    Args:
        start_id: Starting code ID (for globally unique IDs across
            multiple segments).

    Returns:
        Tuple of (transformed segment text, original_data dict,
        next available code_id).
    """
    attr_map = {k: v for k, v in attributes}

    original_data: Dict[str, str] = {}
    code_id = start_id

    def _restore_attrs(tag_text: str) -> str:
        def _replace_attr(m: re.Match) -> str:
            key = m.group(1)
            return attr_map.get(key, m.group(0))
        return re.sub(r'@([^@]+)@', _replace_attr, tag_text)

    result = segment_text

    # Pass 1: Replace self-closing tags with {N}
    self_closing_matches = list(_SELF_CLOSING_TAG_RE.finditer(result))
    for match in reversed(self_closing_matches):
        code_id += 1
        original_tag = _restore_attrs(match.group(0))
        original_data[str(code_id)] = original_tag
        replacement = f'{{#{code_id}}}'
        result = result[:match.start()] + replacement + result[match.end():]

    # Pass 2: Replace paired tags (innermost first)
    while True:
        m = re.search(
            r'<([a-zA-Z][a-zA-Z0-9]*)\b([^>]*)>'
            r'((?:(?!<[a-zA-Z/]).)*?)'
            r'</\1>',
            result,
        )
        if not m:
            break

        code_id += 1
        tag_name = m.group(1)
        tag_attrs = m.group(2)
        content = m.group(3)

        original_open = _restore_attrs(f"<{tag_name}{tag_attrs}>")
        original_close = f"</{tag_name}>"
        original_data[str(code_id)] = original_open + "..." + original_close

        replacement = f'{{#{code_id}}}{content}{{/#{code_id}}}'
        result = result[:m.start()] + replacement + result[m.end():]

    return result, original_data, code_id


def _reverse_numbered_inline(text: str, original_data: Dict[str, str]) -> str:
    """Reverse numbered placeholders back to original HTML tags.

    Replaces ``{#N}content{/#N}`` with the original open/close tags
    and ``{#N}`` (standalone) with the original self-closing tag.
    """
    result = text

    # Replace paired {#N}content{/#N} — innermost first
    while True:
        m = re.search(
            r'\{#(\d+)\}((?:(?!\{#\d+\}).)*?)\{/#\1\}',
            result,
        )
        if not m:
            break

        pid = m.group(1)
        content = m.group(2)
        stored = original_data.get(pid, "")

        if "..." in stored:
            open_tag, close_tag = stored.split("...", 1)
        else:
            open_tag, close_tag = stored, ""

        result = (
            result[:m.start()]
            + open_tag + content + close_tag
            + result[m.end():]
        )

    # Replace standalone {#N} (self-closing tags)
    def _replace_standalone(m: re.Match) -> str:
        pid = m.group(1)
        return original_data.get(pid, m.group(0))

    result = re.sub(r'\{#(\d+)\}', _replace_standalone, result)

    return result


def _xliff_inline(
    segment_text: str,
    attributes: List[Tuple[str, str]],
    start_id: int = 0,
) -> Tuple[str, Dict[str, str], int]:
    """Replace inline HTML tags with XLIFF 2.1 inline elements.

    Paired tags (``<b>text</b>``) become ``<pc>`` elements.
    Self-closing tags (``<img .../>``) become ``<ph>`` elements.
    Original tag text is stored in ``original_data`` keyed by
    data IDs (``d1``, ``d2``, ...).

    Attribute placeholders (``@tag/attr~N@``) in the original tags
    are restored to their actual values before storing in
    ``original_data``, so the round-trip is lossless.

    Args:
        start_id: Starting code ID (for globally unique IDs across
            multiple segments).

    Returns:
        Tuple of (transformed segment text, original_data dict,
        next available code_id).
    """
    # Build attribute lookup for restoring placeholders in original data
    attr_map = {k: v for k, v in attributes}

    original_data: Dict[str, str] = {}
    code_id = start_id
    data_id = start_id * 2  # ensure data IDs don't collide

    def _restore_attrs(tag_text: str) -> str:
        """Replace @tag/attr~N@ placeholders with actual values."""
        def _replace_attr(m: re.Match) -> str:
            key = m.group(1)
            return attr_map.get(key, m.group(0))
        return re.sub(r'@([^@]+)@', _replace_attr, tag_text)

    result = segment_text

    # Pass 1: Replace self-closing tags with <ph/> elements
    # Must do this before paired tags to avoid matching self-closing as open
    self_closing_matches = list(_SELF_CLOSING_TAG_RE.finditer(result))
    for match in reversed(self_closing_matches):
        code_id += 1
        data_id += 1
        original_tag = _restore_attrs(match.group(0))
        d_key = f"d{data_id}"
        original_data[d_key] = original_tag
        replacement = f'<ph id="{code_id}" dataRef="{d_key}"/>'
        result = result[:match.start()] + replacement + result[match.end():]

    # Pass 2: Replace paired tags (innermost first via iterative approach)
    # Keep replacing until no more paired tags remain.
    # The regex must NOT match <pc> or <ph> elements we just inserted.
    while True:
        m = re.search(
            r'<(?!pc\b|ph\b|/pc\b)([a-zA-Z][a-zA-Z0-9]*)\b([^>]*)>'
            r'((?:(?!<(?!pc\b|ph\b|/pc\b)[a-zA-Z/]).)*?)'
            r'</\1>',
            result,
        )
        if not m:
            break

        code_id += 1
        tag_name = m.group(1)
        tag_attrs = m.group(2)
        content = m.group(3)

        # Store original open and close tags
        data_id += 1
        d_start = f"d{data_id}"
        original_open = _restore_attrs(f"<{tag_name}{tag_attrs}>")
        original_data[d_start] = original_open

        data_id += 1
        d_end = f"d{data_id}"
        original_data[d_end] = f"</{tag_name}>"

        replacement = (
            f'<pc id="{code_id}" '
            f'dataRefStart="{d_start}" '
            f'dataRefEnd="{d_end}">'
            f'{content}</pc>'
        )
        result = result[:m.start()] + replacement + result[m.end():]

    return result, original_data, code_id


def _numbered_to_xliff(
    text: str,
    original_data: Dict[str, str],
) -> Tuple[str, Dict[str, str]]:
    """Convert numbered placeholders to XLIFF 2.1 inline elements.

    Transforms ``{#N}text{/#N}`` → ``<pc id="N" ...>text</pc>`` and
    ``{#N}`` → ``<ph id="N" .../>``, building the ``originalData``
    mapping from the stored original tag text.

    Data IDs are deterministic based on placeholder number:
    standalone ``{#N}`` → ``dataRef="dN"``, paired ``{#N}...{/#N}`` →
    ``dataRefStart="dN.s"`` / ``dataRefEnd="dN.e"``. This ensures
    source and target share the same data references even when
    placeholders are reordered.

    Returns:
        Tuple of (XLIFF inline text, xliff_original_data dict).
    """
    xliff_data: Dict[str, str] = {}
    result = text

    # Identify which IDs are paired vs standalone
    paired_ids = set()
    for m in re.finditer(r'\{#(\d+)\}.*?\{/#\1\}', result, re.DOTALL):
        paired_ids.add(m.group(1))

    # Pass 1: Replace standalone {#N} (not paired) → <ph/>
    def _replace_standalone_only(m: re.Match) -> str:
        pid = m.group(1)
        if pid in paired_ids:
            return m.group(0)
        d_key = f"d{pid}"
        xliff_data[d_key] = original_data.get(pid, "")
        return f'<ph id="{pid}" dataRef="{d_key}"/>'

    result = re.sub(r'\{#(\d+)\}', _replace_standalone_only, result)

    # Pass 2: paired {#N}...{/#N} → <pc> (innermost first)
    while True:
        m = re.search(
            r'\{#(\d+)\}((?:(?!\{#\d+\}).)*?)\{/#\1\}',
            result,
        )
        if not m:
            break

        pid = m.group(1)
        content = m.group(2)
        stored = original_data.get(pid, "")

        if "..." in stored:
            open_tag, close_tag = stored.split("...", 1)
        else:
            open_tag, close_tag = stored, ""

        d_start = f"d{pid}.s"
        d_end = f"d{pid}.e"
        xliff_data[d_start] = open_tag
        xliff_data[d_end] = close_tag

        replacement = (
            f'<pc id="{pid}" '
            f'dataRefStart="{d_start}" '
            f'dataRefEnd="{d_end}">'
            f'{content}</pc>'
        )
        result = result[:m.start()] + replacement + result[m.end():]

    return result, xliff_data


def _escape_xml(text: str) -> str:
    """Escape text for inclusion in XML content/attributes."""
    return (
        text.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )


@dataclass
class ValidationError:
    """A single validation issue found in a segment.

    Attributes:
        segment_index: The ``%%N%%`` index of the affected segment.
        code: Machine-readable error code.
        message: Human-readable description.
    """
    segment_index: int
    code: str
    message: str


# Regex patterns for extracting numbered placeholders from text
_NUMBERED_OPEN_RE = re.compile(r'\{#(\d+)\}')
_NUMBERED_CLOSE_RE = re.compile(r'\{/#(\d+)\}')
# XLIFF inline patterns
_XLIFF_PC_OPEN_RE = re.compile(r'<pc\s+id="(\d+)"')
_XLIFF_PC_CLOSE_RE = re.compile(r'</pc>')
_XLIFF_PH_RE = re.compile(r'<ph\s+id="(\d+)"')


@dataclass
class Segment:
    """A single translation unit extracted from the document.

    Attributes:
        index: Placeholder index (matches ``%%N%%`` in the template).
        text: The segment text containing inline placeholders
            (``{#N}`` for numbered format or XLIFF elements).
        kind: Either ``"text"`` for content segments or ``"attribute"``
            for translatable attribute values (e.g. ``img alt``).
        key: For attribute segments, the attribute key (e.g. ``img/alt~0``).
            Empty string for text segments.
        source_text: The original source text before any modifications.
            Set automatically by ``segment()``.
    """
    index: int
    text: str
    kind: str = "text"
    key: str = ""
    source_text: str = ""


class SegmentResult:
    """Result of segmenting a markup document.

    Contains the structural template, classified segments, and
    attributes. Segments can be modified (e.g. translated) and the
    document rebuilt with ``rebuild()``.
    """

    def __init__(
        self,
        template: str,
        segments: List[Segment],
        attributes: List[Tuple[str, str]],
        original_data: Optional[Dict[str, str]] = None,
        inline_format: str = "numbered",
    ):
        self.template = template
        self.segments = segments
        self.attributes = attributes
        self.original_data = original_data or {}
        self._inline_format = inline_format

    def rebuild(self) -> str:
        """Rebuild the document from (possibly modified) segments.

        Substitutes ``%%N%%`` placeholders in the template with
        segment text, then restores attribute placeholders
        (``@tag/attr~N@``) to their original values.

        When ``inline_format="xliff"`` was used, the rebuild reverses
        the XLIFF inline elements back to original HTML tags using
        the ``original_data`` mapping.

        Returns:
            The reconstructed document string.
        """
        result = self.template

        # Substitute %%N%% placeholders with segment text
        for seg in self.segments:
            if seg.kind == "text":
                result = result.replace(f"%%{seg.index}%%", seg.text)

        # Substitute non-translatable attribute placeholders
        for attr_key, attr_value in self.attributes:
            result = result.replace(f"@{attr_key}@", attr_value)

        # Substitute translatable attribute segments
        for seg in self.segments:
            if seg.kind == "attribute" and seg.key:
                result = result.replace(f"@{seg.key}@", seg.text)

        # Reverse inline placeholders back to original HTML tags
        if self._inline_format == "xliff":
            result = _reverse_xliff_inline(result, self.original_data)
        else:  # numbered
            result = _reverse_numbered_inline(result, self.original_data)

        return result

    def validate(self) -> List[ValidationError]:
        """Check segments for placeholder integrity after modification.

        Compares the current ``.text`` of each segment against the
        original ``source_text`` to detect placeholder corruption
        introduced by MT engines, LLMs, or manual editing.

        Checks performed (per segment):

        - **missing_placeholder**: a placeholder ID present in the
          source is absent from the translated text.
        - **unknown_placeholder**: a placeholder ID in the translated
          text was not present in the source.
        - **orphan_open**: an opening placeholder ``{#N}`` appears
          without a matching close ``{/#N}`` (numbered format).
        - **orphan_close**: a closing placeholder ``{/#N}`` appears
          without a matching open ``{#N}`` (numbered format).
        - **nesting_violation**: paired placeholders overlap
          incorrectly, e.g. ``{#1}{#2}...{/#1}{/#2}`` (numbered
          format).

        Returns:
            A list of ``ValidationError`` objects. An empty list
            means all segments are valid.
        """
        errors: List[ValidationError] = []

        for seg in self.segments:
            if seg.kind != "text":
                continue

            if self._inline_format == "numbered":
                errors.extend(
                    self._validate_numbered(seg)
                )
            elif self._inline_format == "xliff":
                errors.extend(
                    self._validate_xliff(seg)
                )

        return errors

    def _validate_numbered(self, seg: "Segment") -> List[ValidationError]:
        """Validate placeholder integrity for numbered format."""
        errors: List[ValidationError] = []
        source = seg.source_text or ""
        target = seg.text

        # Extract IDs from source and target
        src_open = set(_NUMBERED_OPEN_RE.findall(source))
        src_close = set(_NUMBERED_CLOSE_RE.findall(source))
        src_ids = src_open | src_close

        tgt_open = set(_NUMBERED_OPEN_RE.findall(target))
        tgt_close = set(_NUMBERED_CLOSE_RE.findall(target))
        tgt_ids = tgt_open | tgt_close

        # Missing: in source but not in target
        for pid in sorted(src_ids - tgt_ids):
            errors.append(ValidationError(
                segment_index=seg.index,
                code="missing_placeholder",
                message=f"Placeholder {{#{pid}}} present in source "
                        f"but missing from translated text.",
            ))

        # Unknown: in target but not in source
        for pid in sorted(tgt_ids - src_ids):
            errors.append(ValidationError(
                segment_index=seg.index,
                code="unknown_placeholder",
                message=f"Placeholder {{#{pid}}} in translated text "
                        f"was not present in source.",
            ))

        # Orphan open/close: opening without closing or vice versa
        # (only check IDs that are paired in the source)
        src_paired = src_open & src_close
        for pid in sorted(tgt_open - tgt_close):
            if pid in src_paired:
                errors.append(ValidationError(
                    segment_index=seg.index,
                    code="orphan_open",
                    message=f"Opening {{#{pid}}} without matching "
                            f"close {{/#{pid}}}.",
                ))
        for pid in sorted(tgt_close - tgt_open):
            if pid in src_paired:
                errors.append(ValidationError(
                    segment_index=seg.index,
                    code="orphan_close",
                    message=f"Closing {{/#{pid}}} without matching "
                            f"open {{#{pid}}}.",
                ))

        # Nesting violations: check that paired placeholders don't cross
        # Scan left to right, track open/close positions
        stack: List[str] = []
        for m in re.finditer(r'\{#(\d+)\}|\{/#(\d+)\}', target):
            open_id = m.group(1)
            close_id = m.group(2)
            if open_id:
                stack.append(open_id)
            elif close_id:
                if stack and stack[-1] == close_id:
                    stack.pop()
                elif close_id in stack:
                    errors.append(ValidationError(
                        segment_index=seg.index,
                        code="nesting_violation",
                        message=f"Placeholder {{#{close_id}}} closed "
                                f"before {{#{stack[-1]}}} — crossed "
                                f"nesting.",
                    ))
                    # Remove from stack to avoid cascading errors
                    stack.remove(close_id)

        return errors

    def _validate_xliff(self, seg: "Segment") -> List[ValidationError]:
        """Validate placeholder integrity for XLIFF format."""
        errors: List[ValidationError] = []
        source = seg.source_text or ""
        target = seg.text

        # Extract IDs from <pc id="N"> and <ph id="N">
        src_pc = set(_XLIFF_PC_OPEN_RE.findall(source))
        src_ph = set(_XLIFF_PH_RE.findall(source))
        src_ids = src_pc | src_ph

        tgt_pc = set(_XLIFF_PC_OPEN_RE.findall(target))
        tgt_ph = set(_XLIFF_PH_RE.findall(target))
        tgt_ids = tgt_pc | tgt_ph

        for pid in sorted(src_ids - tgt_ids):
            errors.append(ValidationError(
                segment_index=seg.index,
                code="missing_placeholder",
                message=f"Inline element id=\"{pid}\" present in "
                        f"source but missing from translated text.",
            ))

        for pid in sorted(tgt_ids - src_ids):
            errors.append(ValidationError(
                segment_index=seg.index,
                code="unknown_placeholder",
                message=f"Inline element id=\"{pid}\" in translated "
                        f"text was not present in source.",
            ))

        # Check pc open/close pairing
        tgt_pc_close_count = len(_XLIFF_PC_CLOSE_RE.findall(target))
        tgt_pc_open_count = len(tgt_pc)
        if tgt_pc_open_count != tgt_pc_close_count:
            errors.append(ValidationError(
                segment_index=seg.index,
                code="orphan_open" if tgt_pc_open_count > tgt_pc_close_count
                    else "orphan_close",
                message=f"Mismatched <pc>/</pc> count: "
                        f"{tgt_pc_open_count} opens vs "
                        f"{tgt_pc_close_count} closes.",
            ))

        return errors

    def to_xliff(
        self,
        source_lang: str = "en",
        target_lang: str = "es",
    ) -> str:
        """Export segments as an XLIFF 2.1 document.

        Each text segment becomes a ``<unit>`` with ``<source>`` and
        ``<target>`` elements. Inline tags are represented as XLIFF 2.1
        ``<pc>`` (paired) and ``<ph>`` (standalone) elements, with
        original tag data in ``<originalData>/<data>`` elements.

        Works with any ``inline_format`` — if the result was segmented
        with ``"numbered"`` placeholders, they are converted to XLIFF
        inline elements automatically.

        Args:
            source_lang: BCP 47 source language code.
            target_lang: BCP 47 target language code.

        Returns:
            A complete XLIFF 2.1 XML document as a string.
        """
        units = []
        for seg in self.segments:
            if seg.kind != "text":
                continue

            source_text = seg.source_text or seg.text
            target_text = seg.text

            if self._inline_format == "numbered":
                src_xliff, src_data = _numbered_to_xliff(
                    source_text, self.original_data,
                )
                tgt_xliff, tgt_data = _numbered_to_xliff(
                    target_text, self.original_data,
                )
                # Merge data from both source and target
                unit_data = {}
                unit_data.update(src_data)
                unit_data.update(tgt_data)
            else:  # xliff
                src_xliff = source_text
                tgt_xliff = target_text
                unit_data = self.original_data

            # Build <originalData> section
            data_elements = ""
            if unit_data:
                data_entries = "\n".join(
                    f'        <data id="{did}">'
                    f'{_escape_xml(dval)}</data>'
                    for did, dval in unit_data.items()
                )
                data_elements = (
                    f"\n      <originalData>\n"
                    f"{data_entries}\n"
                    f"      </originalData>"
                )

            unit = (
                f'    <unit id="{seg.index}">'
                f'{data_elements}\n'
                f'      <segment>\n'
                f'        <source>{src_xliff}</source>\n'
                f'        <target>{tgt_xliff}</target>\n'
                f'      </segment>\n'
                f'    </unit>'
            )
            units.append(unit)

        units_xml = "\n".join(units)
        return (
            f'<?xml version="1.0" encoding="UTF-8"?>\n'
            f'<xliff xmlns="urn:oasis:names:tc:xliff:document:2.0"\n'
            f'       version="2.0" srcLang="{source_lang}"'
            f' trgLang="{target_lang}">\n'
            f'  <file id="f1">\n'
            f'{units_xml}\n'
            f'  </file>\n'
            f'</xliff>\n'
        )

    def __repr__(self) -> str:
        return (
            f"SegmentResult(template={self.template!r}, "
            f"segments={self.segments!r}, "
            f"attributes={self.attributes!r})"
        )


def _reverse_xliff_inline(text: str, original_data: Dict[str, str]) -> str:
    """Reverse XLIFF 2.1 inline elements back to original HTML tags.

    Replaces ``<pc id="N" dataRefStart="dX" dataRefEnd="dY">content</pc>``
    with the original open/close tags from ``original_data``, and
    ``<ph id="N" dataRef="dX"/>`` with the original self-closing tag.
    """
    result = text

    # Replace <ph id="N" dataRef="dX"/> with original tag
    def _replace_ph(m: re.Match) -> str:
        data_ref = m.group(1)
        return original_data.get(data_ref, m.group(0))

    result = re.sub(
        r'<ph\s+id="[^"]*"\s+dataRef="([^"]*)"\s*/>',
        _replace_ph,
        result,
    )

    # Replace <pc id="N" dataRefStart="dX" dataRefEnd="dY">content</pc>
    # Process innermost first
    while True:
        m = re.search(
            r'<pc\s+id="[^"]*"\s+'
            r'dataRefStart="([^"]*)"\s+'
            r'dataRefEnd="([^"]*)"\s*>'
            r'((?:(?!<pc\b).)*?)'  # content with no nested <pc>
            r'</pc>',
            result,
        )
        if not m:
            break

        d_start = m.group(1)
        d_end = m.group(2)
        content = m.group(3)

        open_tag = original_data.get(d_start, "")
        close_tag = original_data.get(d_end, "")
        result = (
            result[:m.start()]
            + open_tag + content + close_tag
            + result[m.end():]
        )

    return result


def segment(
    text: str,
    *,
    ruleset: str = "default",
    translatable_attrs: Optional[List[str]] = None,
    inline_format: str = "numbered",
) -> SegmentResult:
    """Segment a markup document into classified translation units.

    This is the primary entry point to texplitter. It tokenizes the
    input, builds a segment tree, and extracts segments ready for
    translation or other AI pipeline processing.

    Args:
        text: The markup text to segment (HTML, XML, AsciiDoc, etc.).
        ruleset: Name of the tokenization ruleset to use.
            Defaults to ``"default"`` (HTML/XML).
        translatable_attrs: List of attribute names that should be
            extracted as translatable segments (e.g. ``["alt", "title"]``).
            Defaults to ``None`` (all attributes are non-translatable
            and restored silently during rebuild).
        inline_format: Format for inline tag representation in segments.
            ``"numbered"`` (default) — inline tags are replaced with
            simple numbered placeholders (``{#1}text{/#1}`` for paired
            tags, ``{#1}`` for standalone tags) suitable for LLM/MT
            input. The ``{#N}`` syntax avoids collision with format
            strings, template variables, and curly-brace patterns
            common in IT content.
            ``"xliff"`` — inline tags are replaced with XLIFF 2.1
            elements (``<pc>`` for paired tags, ``<ph>`` for standalone
            tags) with original tag data in ``result.original_data``.
            Designed for interchange with CAT tools.

    Returns:
        A ``SegmentResult`` with template, segments, attributes, and
        original_data.

    Examples::

        import texplitter

        # Default: numbered placeholders — clean, MT/LLM-friendly
        result = texplitter.segment(
            '<p>Click <a href="url">here</a> to continue.</p>',
        )
        print(result.segments[0].text)
        # 'Click {#1}here{/#1} to continue.'

        # XLIFF 2.1 format — for CAT tools and XLIFF interchange
        result = texplitter.segment(
            '<p>Click <a href="url">here</a> to continue.</p>',
            inline_format="xliff",
        )
        print(result.segments[0].text)
        # 'Click <pc id="1" dataRefStart="d1" dataRefEnd="d2">here</pc> to continue.'
    """
    _valid_formats = {"xliff", "numbered"}
    if inline_format not in _valid_formats:
        raise ValueError(
            f"Unsupported inline_format={inline_format!r}. "
            f"Use 'numbered' (default) or 'xliff'."
        )

    tok = Tokenizer(ruleset)
    tokens = tok.tokenize(text)
    tree = SegmentTree(tokens).tree

    # Generate the template and post-editing segments
    _, template, raw_segments = tree.generate_placeholders_for_segments("")

    # Get all attributes from the tree
    all_attributes = tree.get_attributes()

    # Split attributes into translatable and non-translatable
    translatable_attr_set = set(translatable_attrs) if translatable_attrs else set()
    non_translatable_attrs = []
    translatable_attr_segments = []

    for attr_key, attr_value in all_attributes:
        # Extract the attribute name from the key (e.g. "alt" from "img/alt~0")
        parts = attr_key.split("/")
        if len(parts) == 2:
            attr_name = parts[1].split("~")[0]
        else:
            attr_name = attr_key.split("~")[0]

        if attr_name in translatable_attr_set:
            translatable_attr_segments.append((attr_key, attr_value))
        else:
            non_translatable_attrs.append((attr_key, attr_value))

    # Build Segment objects for text segments
    segments = []
    merged_original_data: Dict[str, str] = {}
    next_code_id = 0

    for idx, seg_text in raw_segments:
        if inline_format == "xliff":
            fmt_text, seg_original_data, next_code_id = _xliff_inline(
                seg_text, all_attributes, start_id=next_code_id,
            )
        else:  # numbered
            fmt_text, seg_original_data, next_code_id = _numbered_inline(
                seg_text, all_attributes, start_id=next_code_id,
            )
        merged_original_data.update(seg_original_data)
        segments.append(Segment(
            index=idx, text=fmt_text, kind="text",
            source_text=fmt_text,
        ))

    # Build Segment objects for translatable attributes
    next_index = max((s.index for s in segments), default=0) + 1
    for attr_key, attr_value in translatable_attr_segments:
        segments.append(Segment(
            index=next_index,
            text=attr_value,
            kind="attribute",
            key=attr_key,
            source_text=attr_value,
        ))
        next_index += 1

    return SegmentResult(
        template=template,
        segments=segments,
        attributes=non_translatable_attrs,
        original_data=merged_original_data,
        inline_format=inline_format,
    )
