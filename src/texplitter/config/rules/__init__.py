"""Packaged rule files for texplitter.

Each ``.json`` file in this package is a standalone ruleset — a full,
independent list of regex-pattern dicts in first-match-wins order. There is
no composition, concatenation, or inheritance between rulesets; a caller
picks exactly one via :func:`texplitter.parserrules.load_regex_patterns`,
and that one file is the entire grammar the tokenizer sees.

``default.json`` is the baseline ruleset shipped with the library and is what
:class:`texplitter.Tokenizer` loads when called with no arguments.
"""
