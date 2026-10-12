"""Tokenizer shim.

The real implementation lives in the Rust crate `texplitter_rs` and is
exposed via the `texplitter._native` extension module. This file wraps
the native `Tokenizer` class with a zero-argument constructor that loads
the packaged rule file, so callers don't have to build the pattern list
themselves.
"""

from texplitter import _native
from texplitter._native import UnknownTokenError
from texplitter.parserrules import load_regex_patterns


class Tokenizer:
    """Wrapper around `texplitter._native.Tokenizer` that auto-loads a
    packaged ruleset on construction.

    Parameters
    ----------
    ruleset : str, optional
        Name of a packaged ruleset to load (default ``"default"``).
        Available rulesets can be listed with :func:`texplitter.list_rulesets`.
    move_punctuation_after_quotes : bool, optional
        Move a period or comma written inside closing quotes after them, as
        American English writes it and Spanish (RAE) and most other languages
        do not (default ``True``). Markup is never touched. Turn it off for a
        source already in that order; the rebuild then gives the source back.
    """

    def __init__(self, ruleset: str = "default", move_punctuation_after_quotes: bool = True) -> None:
        self._inner = _native.Tokenizer(load_regex_patterns(ruleset))
        self._inner.set_move_punctuation_after_quotes(move_punctuation_after_quotes)
        # Per-instance list, never shared across tokenizers. The Rust
        # tokenizer already stores attributes on its own state, so this
        # attribute is just the user-visible view.
        self.global_attribute_list: list = []

    def tokenize(self, intext: str):
        return self._inner.tokenize(intext)

    def get_unmatched_tags(self):
        return self._inner.get_unmatched_tags()

    def get_nesting_errors(self):
        return self._inner.get_nesting_errors()


__all__ = ["Tokenizer", "UnknownTokenError"]
