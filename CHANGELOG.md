# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

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
