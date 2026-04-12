# Concepts

This page explains how texplitter works internally. You don't need
this to use the high-level `segment()` API, but it helps if you're
creating custom rulesets or need fine-grained control over the
segmentation process.

## How segmentation works

texplitter processes a document in three stages:

1. **Tokenize** — a regex-based scanner classifies every character
   span in the input (tags, text, attributes, sentence boundaries).
2. **Build tree** — tokens are assembled into a hierarchical
   segment tree that mirrors the document's nesting structure.
3. **Extract segments** — the tree produces full translation units
   (complete sentences with inline tags) and a structural template.

The `texplitter.segment()` function runs all three stages and returns
the result as a `SegmentResult`.

## Token classes

Every token produced by the tokenizer carries a `token_class` that
determines how the segment tree treats it. There are 15 token classes:

### Content tokens

| Class | Meaning |
|-------|---------|
| `TRANSLATABLE` | Text that should be translated |
| `SYMBOL` | Non-translatable inline element (URL, email, generic placeholder) |
| `SPACE` | Single whitespace character |
| `SEVERAL_SPACES` | Two or more consecutive whitespace characters |
| `QUOTE` | Quotation mark (single, double, or Unicode) |
| `PUNCTUATION` | Comma, semicolon, bracket, etc. |

### Structural tokens

| Class | Meaning |
|-------|---------|
| `BREAK` | Sentence boundary (period + space, question mark, double newline) |
| `BREAK_TEMPLATE` | Non-translatable block (XML declaration, processing instruction) |

### Tag tokens

| Class | Meaning |
|-------|---------|
| `OPEN_TAG` | Opening tag without attributes (`<div>`) |
| `BEGINNING_TAG` | Opening tag with attributes (`<div `) |
| `MIDDLE_TAG` | Attribute assignment inside a tag (`class="`) |
| `END_OPEN_TAG` | Closing quote + angle bracket of a tag (`">`) |
| `END_CLOSE_TAG` | Self-closing end (`"/>`) |
| `CLOSE_TAG` | Closing tag (`</div>`) |
| `SINGLE_TAG` | Self-closing tag, entity, or protected inline construct (`<br/>`, `&amp;`, AsciiDoc `<<xref>>`, `{attr}`) |

### Bracket tokens

| Class | Meaning |
|-------|---------|
| `OPEN_BRACKET` | Opening bracket-like marker |
| `CLOSE_BRACKET` | Closing bracket-like marker |

## First-match-wins tokenization

The tokenizer compiles all rules into a single combined regex using
named capture groups. When scanning input text, `fancy_regex` returns
the first match at the earliest position. If two rules match at the
same position, the one listed first in the JSON file wins.

This means **rule order matters**. More specific rules (like AsciiDoc
section titles) must come before generic rules (like the word catch-all)
in the ruleset file.

### Example

Given these two rules:

1. `asciidoc_attribute_ref` matching `\{[a-zA-Z0-9_-]+\}`
2. `open_curly_bracket` matching `\s*\{`

The input `{name}` would be matched by rule 1 (attribute reference)
because it appears first. If the order were reversed, rule 2 would
match the `{` before rule 1 could match the full `{name}`.

## The segment tree

The `SegmentTree` builds a tree of `TreeNode` objects from a flat
token stream. Each node has a `node_type`:

| Node type | Meaning |
|-----------|---------|
| `Multiple` | Sequence of children (the root is always Multiple) |
| `Single` | Tag pair wrapper — has `open` and `close` strings |
| `Binary` | Split by a BREAK operator (e.g., `. ` separating two sentences) |
| `BinaryTemplate` | Split by a BREAK_TEMPLATE |
| `Translatable` | Leaf node containing translatable text |
| `Dummy` | Empty placeholder |

### Tree example

For `<p>Hello <b>world</b>. Goodbye.</p>`:

```
Multiple
  └─ Single (open="<p>", close="</p>")
       └─ Binary (operator=". ")
            ├─ Translatable: "Hello <b>world</b>"
            └─ Translatable: "Goodbye."
```

The tree is built **iteratively** (not recursively) using explicit
work stacks. This is critical for real-world documents that can be
hundreds or thousands of levels deep — a recursive approach would
overflow the stack.

## Templates and placeholders

When the tree generates translation units, it produces:

- A **template** — the structural skeleton with `%%N%%` where
  translatable content was: `<p>%%1%%</p>`
- **Segments** — each `%%N%%` is a complete sentence where inline
  HTML tags have been replaced with placeholders:
  `Hello {#1}world{/#1}.`
- **Attributes** — tag attributes extracted and stored separately:
  `('a/href~0', 'https://example.com')`

This design means:

- The translator sees **full sentences**, not fragments. Inline tags
  are represented as clean `{#N}` placeholders that the model can
  reorder freely without risk of corrupting markup.
- Attribute values (URLs, class names, IDs) are extracted and
  restored automatically — they never reach the model.
- The structural template preserves all block-level nesting untouched.

## Attributes

Every attribute in the source document (e.g., `href="url"`,
`class="main"`, `alt="description"`) is extracted during
segmentation and replaced with a `@tag/attr~N@` placeholder.

By default, all attributes are **non-translatable** — they are
stored separately and restored silently during rebuild. This is the
right default for most attributes (URLs, CSS classes, IDs).

Some attributes contain text that should be translated (e.g.,
`alt`, `title` in HTML). The `translatable_attrs` parameter on
`segment()` lets you opt in:

- Attributes whose name is in `translatable_attrs` become separate
  `Segment` objects with `kind="attribute"` that you can translate.
- All other attributes go into the `.attributes` list for silent
  restoration.

## Inline format abstraction

When texplitter extracts segments, inline HTML tags like `<b>`,
`<a href="...">`, and `<img/>` remain inside the segment text.
This is fine if the downstream consumer understands HTML, but real
translation workflows face a practical problem: **most MT engines
and LLMs corrupt inline markup.**

Common failure modes:

- The model invents attributes that weren't there
- Nested tags lose their pairing (`<a><b>` closed as `</a></b>`)
- Self-closing tags are silently dropped
- Attribute placeholders like `@a/href~0@` are treated as
  translatable text and modified

The `inline_format` parameter controls how HTML tags are represented
in segment text. Two modes are available:

### Numbered (default)

Tags are replaced with compact `{#N}...{/#N}` tokens:

```
Click {#1}here{/#1} to continue.
```

Why this syntax? Curly braces with a hash prefix (`{#1}`) were
chosen to avoid collision with common patterns in content being
translated:

- Format strings: `{name}`, `{0}`, `{count}`
- Template variables: `${var}`, `{{placeholder}}`
- JSON/code: `{ "key": "value" }`

The `#` prefix makes the pattern distinctive enough that MT engines
can learn to pass it through unchanged, without confusing it with
content that should be translated.

The original HTML is stored separately in `result.original_data`
(keyed by placeholder ID) and restored during `rebuild()`.

### XLIFF 2.1 (`inline_format="xliff"`)

Tags are replaced with XLIFF 2.1 inline elements:

```
Click <pc id="1" dataRefStart="d2" dataRefEnd="d3">here</pc> to continue.
```

Paired tags become `<pc>` (paired code) elements; self-closing tags
become `<ph>` (placeholder) elements. This follows the XLIFF 2.1
specification (OASIS Standard, 2014) which replaced the older
`<bpt>`/`<ept>`/`<x>` elements from XLIFF 1.2.

Each inline element carries `dataRef`/`dataRefStart`/`dataRefEnd`
attributes pointing to entries in `<originalData>/<data>` sections,
where the actual HTML tags are stored. This design means the
original markup can be reconstructed losslessly, even after the
segment text passes through a CAT tool that only understands
XLIFF semantics.

### Multi-segment ID uniqueness

Placeholder IDs are globally unique across all segments in a
document. If segment 1 has `{#1}` (for `<b>`) and segment 2 has
`{#2}` (for `<i>`), the IDs never collide. This is essential for
XLIFF export, where `<originalData>` maps each ID to its actual
tag — overlapping IDs would create ambiguity.

### The numbered-then-XLIFF workflow

The `inline_format` modes are designed to compose. The recommended
production workflow is:

1. `segment(html, inline_format="numbered")` — extract segments
   with compact placeholders
2. Send segments to MT — the engine preserves `{#N}` tokens
3. `result.to_xliff()` — export an XLIFF 2.1 document with both
   source and MT output, for human post-editing in a CAT tool

`to_xliff()` converts numbered placeholders to proper XLIFF 2.1
`<pc>`/`<ph>` elements automatically. Data IDs are deterministic
(`d{N}.s`/`d{N}.e` for paired, `d{N}` for standalone), so source
and target share the same data references even when placeholders
are reordered by the translator.

### Placeholder validation

Between steps 2 and 3, MT output should be validated. Real MT
engines and LLMs frequently corrupt inline placeholders:

- **Dropped placeholders** — the engine omits `{#2}` entirely,
  losing a `<b>` tag from the output.
- **Hallucinated placeholders** — the engine invents `{#99}` that
  has no corresponding entry in `original_data`.
- **Broken pairing** — `{#1}` is present but `{/#1}` is missing,
  so the paired tag can't be reconstructed.
- **Crossed nesting** — `{#1}{#2}...{/#1}{/#2}` produces invalid
  HTML (`<a><b>...</a></b>`).

The `validate()` method catches all of these before `rebuild()` is
called. The caller decides the recovery strategy: retry with a
different MT engine, fall back to the source text, or flag the
segment for human review.

## Visualizing the tree

texplitter can generate tree visualizations in two formats:

- **SVG**: `tree.tree.generate_svg()` returns an SVG string.
- **DOT**: `tree.tree.generate_dot_representation()` returns a
  Graphviz DOT string that you can render with `dot -Tpng`.

Both are useful for debugging tokenization rules and understanding
how the tree interprets your input.
