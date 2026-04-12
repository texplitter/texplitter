<p align="center">
   <img src="assets/texplitter_b.svg" alt="texplitter Logo" width="50%">
</p>

# texplitter

> **Alpha release (0.1.0a0)** — this project is under active development.
> The API may change in future releases without notice.

Rust-backed text segmentation for AI pipelines. texplitter classifies
every piece of a markup document — translatable prose, structural tags,
protected attributes, sentence boundaries — and extracts clean segments
your model can work with. The structure stays untouched; the model only
sees language.

## Why texplitter?

- **Deterministic**: rule-based classification, same input produces
  same output every time. No model variability, no silent tag corruption.
- **Fast**: Rust core, Python API via pyo3. ~25 ms on a 1 MB HTML
  document where a pure Python regex approach takes over 8 minutes.
- **Customizable**: tokenization rules are plain JSON files. Ship the
  built-in HTML/XML or AsciiDoc rulesets, or create your own.

## Installation

```bash
pip install texplitter
```

> **Note**: texplitter ships as a source distribution that requires a Rust
> toolchain to compile. Install [Rust](https://rustup.rs/) before running
> `pip install`.

## Quick Start

```python
import texplitter

result = texplitter.segment('<p>Click <a href="url">here</a> to continue.</p>')

print(result.template)
# '<p>%%1%%</p>'

print(result.segments[0].text)
# 'Click {#1}here{/#1} to continue.'

print(result.attributes)
# [('a/href~0', 'url')]
```

Each segment is a full translation unit — a complete sentence where
inline HTML tags have been replaced with clean `{#N}` placeholders.
The model sees the whole sentence and can reorder words and
placeholders together, without risk of corrupting markup. Attribute
values (URLs, class names) are extracted and restored automatically
during rebuild.

### Translate and rebuild

```python
result.segments[0].text = 'Para continuar pulse {#1}aquí{/#1}.'

print(result.rebuild())
# '<p>Para continuar pulse <a href="url">aquí</a>.</p>'
```

Tag reordering works naturally — placeholders follow the words:

```python
result = texplitter.segment(
    '<p>Click <a href="url">here</a> to <b>continue</b>.</p>',
)
# 'Click {#1}here{/#1} to {#2}continue{/#2}.'

result.segments[0].text = 'Para {#2}continuar{/#2} pulse {#1}aquí{/#1}.'
print(result.rebuild())
# '<p>Para <b>continuar</b> pulse <a href="url">aquí</a>.</p>'
```

The `{#N}` syntax avoids collision with format strings (`{name}`),
template variables (`${var}`), and other curly-brace patterns in
IT content.

### XLIFF 2.1 export

For production pipelines: segment, run MT, then export XLIFF 2.1
for human post-editing in CAT tools:

```python
result = texplitter.segment(html)
result.segments[0].text = translated_text  # from MT
xliff = result.to_xliff(source_lang="en", target_lang="es")
```

Use `inline_format="xliff"` if you need XLIFF 2.1 inline elements
(`<pc>`, `<ph>`) directly in segment text for CAT tool interchange.

### Validate MT output

```python
errors = result.validate()
for e in errors:
    print(f"[{e.code}] {e.message}")
# Catches: missing/hallucinated placeholders, broken nesting
```

See the [documentation](https://texplitter.github.io/texplitter/)
for the full guide.

### Translatable attributes

Some attributes contain text that should be translated (e.g. `alt`,
`title`). Opt in with `translatable_attrs`:

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

By default `translatable_attrs=None` — all attributes are
non-translatable and restored silently during rebuild. This keeps the
API format-agnostic; only opt in when working with formats that have
translatable attributes (HTML/XML).

## Custom Rulesets

```python
import texplitter

# See available rulesets
print(texplitter.list_rulesets())  # ['asciidoc', 'default']

# Segment with a different ruleset
result = texplitter.segment("See <<chapter-1>> for details.", ruleset="asciidoc")
```

The AsciiDoc ruleset is provided as a sample to illustrate custom
ruleset creation; it has not been fully tested in production scenarios.

To create your own ruleset, place a JSON file in
`texplitter/config/rules/` or use the config editor:

```bash
pip install texplitter[config-editor]
texplitter config
```

## Building from Source

```bash
git clone https://github.com/texplitter/texplitter.git
cd texplitter
pip install maturin
maturin develop --release
pip install -e ".[dev]"
pytest tests/
```

## License

MIT License. Copyright (c) 2024 Alex Martinez Corria.
See [LICENSE](LICENSE) for details.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes and add tests
4. Run `pytest tests/` and `cd texplitter_rs && cargo test --no-default-features`
5. Submit a pull request

[GitHub Issues](https://github.com/texplitter/texplitter/issues) |
[Documentation](https://texplitter.github.io/texplitter/) |
[PyPI](https://pypi.org/project/texplitter/)
