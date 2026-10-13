# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed

- A segment without a letter or a digit (a table cell holding a dash) is no segment: it stays in the template as written.
- The generic symbol rule (both rule sets) could end a token with a newline (`a.b\n`), so a blank line after a word with a dot in it (`=== spec.deployment`) was no paragraph break.

### Added

- The AsciiDoc rule set handles a documentation site (checked byte for byte
  on the 33 pages of thinkube.org, `kubexlat/experiments/asciidoc-roundtrip`):
  listing, literal, passthrough and comment blocks are protected whole;
  `` `code` `` is one protected token; `xref:page[label]` and
  `link:url[label]` keep their target and translate their label; headings,
  list markers, block titles, attribute lines, table delimiters and cells,
  and the blank line between paragraphs are structure; a sentence end right
  before a structural line keeps its newline. Rules anchored with `^` or `$`
  carry `(?m:...)`: the tokenizer compiles with `(?sx)`, so without it they
  matched on the first line only.
- Inline marks of a rule set other than the HTML one become `{#N}`
  placeholders by their token classes (`*bold*` → `{#1}bold{/#1}`,
  `` `code` `` → `{#1}`), so `validate()`, `rebuild()` and `to_xliff()` work
  the same for every rule set.

- `segment(..., move_punctuation_after_quotes=True)` and the same option on
  `Tokenizer`: a period or comma written inside closing quotes, as American
  English does, moves after them, as Spanish and most other languages write
  it. Off for a source already in that order.
- Courtesy titles keep their sentence: `Dr.`, `Jr.`, `Messrs.`, and the
  Spanish `Sr.`, `Sra.`, `Srta.`, `Dra.`, `Dña.`, `Ud.`, `Uds.`, `Vd.`, `Vds.`.
- Regression tests from real documents (`tests/test_regressions.py`,
  `tests/fixtures/`): IRS Publication 17 (2025) chapter 12 in English and
  Spanish, *Treasure Island* chapters I–III.

### Fixed

- A sentence break right after wrapped content (`<p><b>Hello</b>. Next</p>`)
  was written inside the wrapper and again by the break and by every empty
  sibling; it is now written once, after the closing mark.
- A pair holding a self-closing element (`<b><img/></b>`, `*`code`*`) was
  never rebuilt: standalone placeholders are now reversed first.
- Whitespace after the last block (a file's final newline) was dropped.
- `validate()` reported a standalone placeholder inside a pair (`{#9}{#10}{/#9}`,
  bold around a code span) as crossed nesting.
- An opening bracket mark without its close (`*Playbooks,`) dropped the rest
  of the document; an unmatched mark is now text, as a lone `*` or `_` is.
- `move_punctuation_after_quotes` edited the whole document, protected tokens
  included: a period inside a listing block's JSON moved. It now applies to
  the text between protected tokens only.
- `rebuild()` gives back the document: a sentence break was written both in
  the sentence before it and in the template (`One.{sp}. Two.`, `Go!!`), and
  `{sp}` was never turned back into a space.
- Text after a self-closing element with attributes, as XLIFF writes every
  inline code (`<ph id="1"/>`), was dropped: `Click <ph/> to continue.` lost
  "to continue".
- HTML void elements written without a slash (`<meta charset="utf-8">`,
  `<img src="...">`, `<hr>`) close themselves; they dropped the text after
  them. `<link>...</link>` in XML keeps its content.
- A bare self-closing element between blocks (`<p/>`) no longer makes the
  rest of the document one segment, and is kept on rebuild.
- The period-after-quote move no longer runs inside markup: a `title`
  attribute ending in `.` lost the tag's closing `">` and could panic the
  splitter. It also moves the comma and covers typographic quotes.
- No space is added after `¿` and `¡` or before `;` and `=`; the source's
  spacing is kept, attribute values included.
- A sentence may start with `¿`, `¡`, an opening quote or a dialogue dash.
- Tag and attribute names may hold digits, `_`, `:` and `.` (`epub:type`,
  `xml:lang`), so XHTML and EPUB are read as markup.

## [0.1.0a0] - Unreleased

Alpha release. **The API is not yet stable and may change in future
releases without notice.**

### Added

- High-level `texplitter.segment()` API: single-function entry point
  that returns a `SegmentResult` with template, segments, and
  attributes. Includes `rebuild()` for the full translate-and-reassemble
  round-trip.
- Inline format modes for segment text: `"numbered"` (default) replaces
  inline HTML tags with `{#N}` placeholders safe for MT/LLM input;
  `"xliff"` replaces them with XLIFF 2.1 `<pc>`/`<ph>` elements for
  CAT tool interchange. Attribute values are extracted and restored
  automatically — the model never sees raw markup.
- XLIFF 2.1 export via `SegmentResult.to_xliff()`: produces a complete
  XLIFF document with `<source>` and `<target>` elements, XLIFF inline
  annotations, and `<originalData>` sections. Works with both inline
  formats — numbered placeholders are converted automatically.
- Placeholder validation via `SegmentResult.validate()`: detects
  missing, hallucinated, orphaned, and improperly nested placeholders
  after MT/LLM translation. Returns structured `ValidationError`
  objects with machine-readable error codes.
- Translatable attributes support via `translatable_attrs` parameter:
  opt in to extract attribute values (e.g., `alt`, `title`) as
  separate translatable segments.
- Rust-backed tokenizer with first-match-wins regex rules, exposed to
  Python via pyo3 0.28 bindings.
- Segment tree: build from token stream, extract translatable segments,
  generate numbered placeholders, rebuild after translation.
- Default HTML/XML ruleset (55 rules) covering tags, entities,
  abbreviations, CJK characters, URLs, emails, words, and punctuation.
- AsciiDoc ruleset for segmenting AsciiDoc documents (section titles,
  inline formatting, attribute references, cross-references,
  block delimiters). Provided as a sample to illustrate custom ruleset
  creation; not fully tested in production scenarios.
- Multi-ruleset system: load rulesets by name with `Tokenizer("name")`
  and `load_regex_patterns("name")`. Discover available rulesets with
  `list_rulesets()`.
- Config editor: browser-based Vue 3 + FastAPI GUI for creating,
  editing, reordering, and testing regex rules. Includes optional
  AI assistant for LLM-powered regex generation (works with any
  OpenAI-compatible API).
- SVG and DOT tree visualization via `TreeNode.generate_svg()` and
  `TreeNode.generate_dot_representation()`.
- Typed error boundary: `TexplitterError` with `InvalidToken` and
  `InvalidNodeDict` variants, replacing panics on malformed input.
- All tree walks are iterative (explicit work stacks), safe at any
  nesting depth.
- CLI entry point: `texplitter config` launches the config editor.

[0.1.0a0]: https://github.com/texplitter/texplitter/releases/tag/v0.1.0a0
