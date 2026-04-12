"""Packaged rule data for texplitter.

This subpackage exists purely so ``importlib.resources.files("texplitter.config")``
can locate the rule files regardless of whether the library is installed from a
wheel, run from an editable checkout, or bundled inside a zipapp. No Python code
lives here.

Rule files live one level deeper under :mod:`texplitter.config.rules` so that
multiple named rulesets can coexist side by side — ``default.json`` is the
baseline grammar, and users can drop additional files next to it and load them
by name via :func:`texplitter.parserrules.load_regex_patterns`.
"""
