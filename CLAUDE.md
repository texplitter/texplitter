# texplitter

Rule-driven, markup-aware segmentation for AI pipelines: a Rust core
(`texplitter_rs/`) with Python bindings; the Python modules in
`src/texplitter/` re-export it.

## Tests are the reference

The Rust core was ported from a Python version, and the expected outputs in
`tests/` were made by that Python version. The suite proves that the Rust
version still does what the Python version did.

- Any change, to a rule file in `src/texplitter/config/rules/` or to the
  Rust core, runs the whole suite. A change that alters an expected output
  is a change of behaviour and is decided as one, not patched into the test.
- Before fixing a defect, add the input that shows it to the suite, as a
  plain test or, while it is not fixed, as `xfail(strict=True)` naming the
  defect. `tests/test_regressions.py` holds the real documents and the cases
  cut from them; `tests/fixtures/` the documents.
- The `tkt-texplitter` service builds this library at a named commit and
  runs the suite on the wheel it built; a failing test fails that build.
