# texplitter

> **Alpha release (0.1.0a0)** — this project is under active development.
> The API may change in future releases without notice.

**Rust-backed text segmentation for AI pipelines.**

texplitter classifies every piece of a markup document — translatable
prose, structural tags, protected attributes, sentence boundaries —
and extracts clean segments your model can work with.

## Key features

- **Deterministic**: rule-based classification, same input produces
  same output every time.
- **Fast**: Rust core, Python API via pyo3. ~25 ms on 1 MB HTML.
- **Customizable**: tokenization rules are plain JSON files.
  Ship the built-in HTML/XML or AsciiDoc rulesets, or create your own.

## Get started

```bash
pip install texplitter
```

```python
import texplitter

result = texplitter.segment('<p>Click <a href="url">here</a> to continue.</p>')
print(result.segments[0].text)
# 'Click {#1}here{/#1} to continue.'
```

See the [Getting Started](getting-started.md) guide for a full walkthrough.

## Links

- [GitHub Repository](https://github.com/texplitter/texplitter)
- [PyPI Package](https://pypi.org/project/texplitter/)
- [Changelog](changelog.md)
