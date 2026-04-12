# Getting Started

## Installation

texplitter ships as a source distribution with a Rust extension.
You need a [Rust toolchain](https://rustup.rs/) installed before
running pip:

```bash
pip install texplitter
```

To also install the GUI ruleset editor:

```bash
pip install texplitter[config-editor]
```

## Segment a document

The `segment()` function is the primary entry point. Pass it any
markup text and get back a structured result:

```python
import texplitter

result = texplitter.segment('<p>Click <a href="url">here</a> to continue.</p>')
```

The result contains three pieces:

```python
print(result.template)
# '<p>%%1%%</p>'

print(result.segments[0].text)
# 'Click {#1}here{/#1} to continue.'

print(result.attributes)
# [('a/href~0', 'url')]
```

- **template** — the structural skeleton with `%%N%%` placeholders
  where translatable content was removed.
- **segments** — full translation units. Each segment is a complete
  sentence where inline HTML tags have been replaced with clean
  `{#N}` placeholders. The model sees language, not markup.
- **attributes** — extracted attribute values (URLs, class names)
  stored separately and restored automatically during rebuild.

## Translate and rebuild

Modify the segment text (e.g., after machine translation), then
call `rebuild()` to reassemble the document:

```python
result.segments[0].text = 'Para continuar pulse {#1}aquí{/#1}.'

print(result.rebuild())
# '<p>Para continuar pulse <a href="url">aquí</a>.</p>'
```

The model sees the full sentence with compact placeholders and can
freely reorder words and tags. During rebuild, `{#N}` placeholders
are restored to their original HTML tags, and attributes (URLs,
class names) are put back automatically.

## Multi-sentence documents

When a document contains multiple sentences, each becomes its own
segment with globally unique placeholder IDs:

```python
result = texplitter.segment('<p>Hello <b>world</b>. Goodbye <i>friend</i>.</p>')

for seg in result.segments:
    print(f"  %%{seg.index}%%: {seg.text}")
# %%1%%: Hello {#1}world{/#1}.
# %%2%%: Goodbye {#2}friend{/#2}.
```

Translate each segment independently, then rebuild:

```python
for seg in result.segments:
    if "Hello" in seg.text:
        seg.text = "Hola {#1}mundo{/#1}."
    elif "Goodbye" in seg.text:
        seg.text = "Adiós {#2}amigo{/#2}."

print(result.rebuild())
# '<p>Hola <b>mundo</b>. Adiós <i>amigo</i>.</p>'
```

## Translatable attributes

Some attributes contain text that should be translated (e.g., `alt`,
`title`). Opt in with the `translatable_attrs` parameter:

```python
result = texplitter.segment(
    '<p><img src="ok.png" alt="green checkmark"/> Continue.</p>',
    translatable_attrs=["alt"],
)

for seg in result.segments:
    print(f"{seg.kind}: {seg.text}")
# text: {#1} Continue.
# attribute: green checkmark
```

Attribute segments have `kind="attribute"` and a `key` that
identifies which placeholder they correspond to. Translate them
like any other segment:

```python
for seg in result.segments:
    if seg.kind == "attribute":
        seg.text = "marca de verificación verde"

rebuilt = result.rebuild()
# alt="marca de verificación verde", src="ok.png" restored silently
```

By default `translatable_attrs=None` — all attributes are
non-translatable and restored silently during rebuild. This keeps
the API format-agnostic; only opt in when working with formats
that have translatable attributes (HTML/XML).

## Inline format modes

texplitter replaces inline HTML tags with clean placeholders so that
MT engines and LLMs never see raw markup. The default `"numbered"`
format uses compact `{#N}` tokens; alternatively, `"xliff"` produces
XLIFF 2.1 inline elements for CAT tool interchange.

### Numbered placeholders (default)

Paired tags become `{#N}...{/#N}`, self-closing tags become `{#N}`.
The `{#...}` syntax was chosen specifically to avoid collision with
format strings, template variables, and curly-brace patterns common
in IT content (e.g., `{name}`, `${var}`, `{{placeholder}}`).

Tag reordering works naturally — placeholders follow the words:

```python
result = texplitter.segment(
    '<p>Click <a href="url">here</a> to <b>continue</b>.</p>',
)
# Source: 'Click {#1}here{/#1} to {#2}continue{/#2}.'

# Spanish word order is different — tags follow the words
result.segments[0].text = 'Para {#2}continuar{/#2} pulse {#1}aquí{/#1}.'

rebuilt = result.rebuild()
# '<p>Para <b>continuar</b> pulse <a href="url">aquí</a>.</p>'
```

### XLIFF 2.1 inline elements — for CAT tools

The `"xliff"` format replaces tags with XLIFF 2.1 inline elements
(`<pc>` for paired tags, `<ph>` for standalone):

```python
result = texplitter.segment(
    '<p>Click <a href="url">here</a> to continue.</p>',
    inline_format="xliff",
)

print(result.segments[0].text)
# 'Click <pc id="1" dataRefStart="d2" dataRefEnd="d3">here</pc> to continue.'
```

This format is designed for interchange with professional translation
tools (CAT tools like memoQ, Trados, Memsource) that natively
understand XLIFF 2.1 inline annotations.

### Exporting XLIFF 2.1 documents

The recommended workflow for production translation pipelines is:

1. **Segment with numbered placeholders** — simpler for MT engines
2. **Run MT** — the engine preserves `{#N}` tokens
3. **Export to XLIFF 2.1** — for human post-editing in CAT tools

```python
# Step 1: Segment
result = texplitter.segment(
    '<p>Click <a href="url">here</a> to continue.</p>',
    inline_format="numbered",
)

# Step 2: Machine translate (preserving placeholders)
result.segments[0].text = 'Para continuar pulse {#1}aquí{/#1}.'

# Step 3: Export XLIFF 2.1 with source + target
xliff = result.to_xliff(source_lang="en", target_lang="es")
```

The XLIFF output includes both the original source and the MT output
as `<source>` and `<target>` elements, with inline tags represented
as proper XLIFF 2.1 `<pc>` and `<ph>` elements and original tag data
in `<originalData>/<data>` sections. Human translators can then
post-edit the MT output in their preferred CAT tool using the source
as reference.

`to_xliff()` works with any inline format — it converts numbered
placeholders and raw HTML tags to XLIFF inline elements automatically.

## Validating MT output

MT engines and LLMs can corrupt inline placeholders — dropping them,
inventing new ones, or breaking nesting. Call `validate()` after
translation to catch these problems before they reach `rebuild()`:

```python
result = texplitter.segment(html, inline_format="numbered")
result.segments[0].text = mt_engine.translate(result.segments[0].text)

errors = result.validate()
if errors:
    for e in errors:
        print(f"Segment {e.segment_index}: [{e.code}] {e.message}")
    # Handle: retry MT, fall back to source, flag for review, etc.
else:
    rebuilt = result.rebuild()  # safe to rebuild
```

The `validate()` method checks each segment for:

- **missing_placeholder** — a placeholder in the source was dropped
- **unknown_placeholder** — a placeholder was hallucinated
- **orphan_open / orphan_close** — mismatched open/close pairs
- **nesting_violation** — crossed placeholder nesting
  (e.g., `{#1}{#2}...{/#1}{/#2}`)

Validation returns a list of `ValidationError` objects (empty = all
valid). It works with both `"numbered"` and `"xliff"` inline formats.

## Using a custom ruleset

texplitter ships with rulesets for HTML/XML (default) and AsciiDoc:

```python
print(texplitter.list_rulesets())  # ['asciidoc', 'default']

result = texplitter.segment("See <<chapter-1>> for details.", ruleset="asciidoc")
```

See [Custom Rulesets](custom-rulesets.md) for how to create your own.

## Next steps

- [Concepts](concepts.md): understand how tokenization and the
  segment tree work under the hood.
- [Custom Rulesets](custom-rulesets.md): create rulesets for your format.
- [Config Editor](config-editor.md): use the GUI to build rulesets.
- [API Reference](api-reference.md): full API documentation.
