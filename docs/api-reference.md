# API Reference

## High-level API

The high-level API is the recommended way to use texplitter. It
handles tokenization, tree building, and segment extraction in a
single call.

### `texplitter.segment(text, *, ruleset="default", translatable_attrs=None, inline_format="numbered")`

Segment a markup document into classified translation units.

**Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `text` | `str` | (required) | The markup text to segment |
| `ruleset` | `str` | `"default"` | Name of the tokenization ruleset |
| `translatable_attrs` | `list[str]` or `None` | `None` | Attribute names to extract as translatable segments |
| `inline_format` | `str` | `"numbered"` | Inline tag representation: `"numbered"` (compact `{#N}` placeholders) or `"xliff"` (XLIFF 2.1 `<pc>`/`<ph>` elements) |

**Returns:** `SegmentResult`

**Raises:** `ValueError` if `inline_format` is not `"numbered"` or `"xliff"`.

```python
import texplitter

# Default: numbered placeholders — clean, MT/LLM-friendly
result = texplitter.segment('<p>Click <a href="url">here</a> to continue.</p>')
# segments[0].text: 'Click {#1}here{/#1} to continue.'

# XLIFF 2.1 inline elements for CAT tools
result = texplitter.segment(
    '<p>Click <a href="url">here</a> to continue.</p>',
    inline_format="xliff",
)
# segments[0].text: 'Click <pc id="1" ...>here</pc> to continue.'
```

---

### `SegmentResult`

Result of segmenting a markup document. Contains the structural
template, classified segments, and attributes.

**Attributes:**

| Attribute | Type | Description |
|-----------|------|-------------|
| `template` | `str` | Structural skeleton with `%%N%%` placeholders |
| `segments` | `list[Segment]` | Translation units (mutable) |
| `attributes` | `list[tuple[str, str]]` | Non-translatable attribute `(key, value)` pairs |
| `original_data` | `dict[str, str]` | Mapping from placeholder/data IDs to original HTML tags. Populated when `inline_format` is set; empty otherwise. |

**Methods:**

#### `rebuild() -> str`

Rebuild the document from (possibly modified) segments. Substitutes
`%%N%%` placeholders with segment text, then restores `@tag/attr~N@`
placeholders to their original values. When `inline_format` was used,
reverses the inline placeholders back to their original HTML tags
using `original_data`.

```python
result.segments[0].text = 'Para continuar pulse {#1}aquí{/#1}.'
rebuilt = result.rebuild()
# '<p>Para continuar pulse <a href="url">aquí</a>.</p>'
```

#### `to_xliff(source_lang="en", target_lang="es") -> str`

Export segments as an XLIFF 2.1 document. Each text segment becomes
a `<unit>` with `<source>` and `<target>` elements. Inline tags are
represented as XLIFF 2.1 `<pc>` (paired) and `<ph>` (standalone)
elements, with original tag data in `<originalData>/<data>` sections.

Works with any `inline_format` — numbered placeholders and raw HTML
are converted to XLIFF inline elements automatically.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `source_lang` | `str` | `"en"` | BCP 47 source language code |
| `target_lang` | `str` | `"es"` | BCP 47 target language code |

```python
result = texplitter.segment(html, inline_format="numbered")
result.segments[0].text = 'Para continuar pulse {#1}aquí{/#1}.'
xliff = result.to_xliff(source_lang="en", target_lang="es")
```

The XLIFF output contains the original source text in `<source>` and
the (possibly translated) text in `<target>`, both with XLIFF inline
annotations. Data IDs are deterministic (`d{N}.s`/`d{N}.e` for
paired tags, `d{N}` for standalone), so source and target reference
the same `<data>` entries even when placeholders are reordered.

#### `validate() -> list[ValidationError]`

Check segments for placeholder integrity after modification. Compares
each segment's current `.text` against its `.source_text` to detect
corruption introduced by MT engines, LLMs, or manual editing.

**Checks performed:**

| Error code | Meaning |
|-----------|---------|
| `missing_placeholder` | A placeholder ID in the source is absent from the translated text |
| `unknown_placeholder` | A placeholder ID in the translated text was not in the source |
| `orphan_open` | Opening placeholder without matching close |
| `orphan_close` | Closing placeholder without matching open |
| `nesting_violation` | Paired placeholders overlap incorrectly (crossed nesting) |

**Returns:** `list[ValidationError]` — empty if all segments are valid.

Works with both `"numbered"` and `"xliff"` inline formats.

```python
result = texplitter.segment(html, inline_format="numbered")
result.segments[0].text = mt_output

errors = result.validate()
for e in errors:
    print(f"Segment {e.segment_index}: [{e.code}] {e.message}")
```

---

### `ValidationError`

A single validation issue found in a segment.

**Attributes:**

| Attribute | Type | Description |
|-----------|------|-------------|
| `segment_index` | `int` | The `%%N%%` index of the affected segment |
| `code` | `str` | Machine-readable error code (see table above) |
| `message` | `str` | Human-readable description |

---

### `Segment`

A single translation unit extracted from the document.

**Attributes:**

| Attribute | Type | Default | Description |
|-----------|------|---------|-------------|
| `index` | `int` | (required) | Placeholder index (matches `%%N%%` in template) |
| `text` | `str` | (required) | Segment text with inline placeholders (`{#N}` or XLIFF elements) |
| `kind` | `str` | `"text"` | `"text"` for content segments, `"attribute"` for translatable attributes |
| `key` | `str` | `""` | For attribute segments, the attribute key (e.g., `img/alt~0`) |
| `source_text` | `str` | `""` | Original source text before modifications. Set automatically by `segment()`. Used by `to_xliff()` for the `<source>` element. |

---

### `list_rulesets() -> list[str]`

Return a sorted list of available ruleset names.

```python
print(texplitter.list_rulesets())  # ['asciidoc', 'default']
```

---

## Advanced API

The classes below give fine-grained control over the tokenization
and tree-building process. Most users won't need them — use
`texplitter.segment()` instead.

### `Tokenizer(ruleset="default")`

Tokenize text using a named ruleset.

**Parameters:**

- `ruleset` (str, optional): Name of a packaged ruleset. Default: `"default"`.
  Use `list_rulesets()` to see available names.

**Methods:**

#### `tokenize(text: str) -> list[dict]`

Tokenize the input text. Returns a list of token dictionaries, each with:

| Key | Type | Description |
|-----|------|-------------|
| `token_class` | str | One of the 15 token classes (see [Concepts](concepts.md)) |
| `token_type` | str | The rule `type` that matched |
| `text` | str | The matched text |
| `label` | str | Display label |
| `start` | int | Start position in input |
| `end` | int | End position in input |

#### `get_unmatched_tags() -> list`

Return tags from the last `tokenize()` call that have no matching
open/close counterpart.

#### `get_nesting_errors() -> list`

Return tags from the last `tokenize()` call that were closed in the
wrong order.

---

### `SegmentTree(tokens)`

Build a segment tree from a list of tokens.

**Parameters:**

- `tokens` (list[dict]): Token list from `Tokenizer.tokenize()`.
  Each token must have `token_class`, `text`, and `label` as strings.
  Raises `TexplitterError.InvalidToken` if validation fails.

**Attributes:**

- `tree` (TreeNode): The root node of the segment tree.

---

### `TreeNode`

A node in the segment tree. Created by `SegmentTree`, not directly
instantiated.

**Attributes:**

- `node_type`: The node type (Multiple, Single, Binary, BinaryTemplate,
  Translatable, or Dummy).
- `value` (str): The node's text value.
- `children` (list[TreeNode]): Child nodes.
- `attributes` (list): Tag attributes.

**Methods:**

#### `build_full_value() -> str`

Reconstruct the full tagged string from the tree, with attribute
values replaced by placeholders.

#### `extract_segments() -> list[dict[str, str]]`

Extract a flat list of leaf-level segments. Each dictionary maps
tree-path keys to translatable text values.

> **Note:** These are leaf-level decomposed fragments, not full
> translation units. For translation, use `texplitter.segment()`
> which returns complete sentences with inline tags preserved.

#### `extract_extended_segments() -> dict`

Extract a nested dictionary preserving the full tree structure,
including attributes, operators, left/right children, and metadata.

#### `generate_placeholders_for_segments(prefix) -> tuple`

Replace each segment with a numbered placeholder (`%%1%%`, `%%2%%`, etc.).
Returns a tuple of `(segment_index_map, template_string, segments_list)`.

#### `rebuild_from_segments(index, node_type, tag_list, extended_segments) -> str`

Rebuild the node's value from edited extended segments.

#### `build_pattern() -> str`

Build a pattern string showing the tree structure with node types.

#### `get_attributes() -> list`

Collect all attributes from this node and its descendants.

#### `generate_svg() -> str`

Generate an SVG visualization of the tree.

#### `generate_dot_representation() -> str`

Generate a Graphviz DOT representation of the tree.

---

### Utility functions

#### `convert_tokens_to_string(tokens) -> str`

Concatenate the `text` field of each token into a single string.

#### `replace_substring_from_end(text, old, new, count) -> str`

Replace `count` occurrences of `old` with `new`, searching from the
end of the string.

#### `load_regex_patterns(name="default") -> list[dict]`

Load a packaged ruleset by name. Returns the list of rule dictionaries
from the JSON file.

---

### Exceptions

#### `UnknownTokenError`

Raised when the tokenizer encounters text that doesn't match any
rule in the ruleset.

#### `TexplitterError`

Base error type for the Rust core. Subtypes:

- `TexplitterError.InvalidToken { index, reason }`: a token at the
  given index failed validation.
- `TexplitterError.InvalidNodeDict { reason }`: a node dictionary
  passed to `TreeNode.from_dict()` has an invalid structure.
