"""Tests for the multi-ruleset loader API in `texplitter.parserrules`.

Every packaged ruleset is a standalone file under
`texplitter/config/rules/`, loadable by name. The zero-arg `Tokenizer()`
constructor loads the `default` ruleset.
"""

import pytest

from texplitter.parserrules import (
    DEFAULT_RULESET,
    list_rulesets,
    load_regex_patterns,
    regex_patterns,
)
from texplitter.tokenizer import Tokenizer


def test_default_constant_is_default():
    assert DEFAULT_RULESET == "default"


def test_list_rulesets_includes_default():
    names = list_rulesets()
    # The default ruleset must always be present — it is what zero-arg
    # Tokenizer() loads.
    assert "default" in names
    # Sorted, deduped, stems only (no `.json` suffix, no directory).
    assert names == sorted(set(names))
    for n in names:
        assert not n.endswith(".json")
        assert "/" not in n
        assert "\\" not in n


def test_load_default_ruleset_matches_module_level_constant():
    # The eagerly loaded `regex_patterns` module-level constant must be
    # byte-identical to what `load_regex_patterns("default")` returns —
    # they are two call sites that must stay in sync.
    loaded = load_regex_patterns("default")
    assert loaded == regex_patterns
    assert len(loaded) > 0
    # Every rule must carry the four required fields. The loader returns
    # plain dicts, not Pattern objects, so we check membership directly.
    for rule in loaded:
        assert "class" in rule
        assert "type" in rule
        assert "regex" in rule
        assert "callback" in rule


def test_load_regex_patterns_default_argument():
    # Called with no arguments, `load_regex_patterns()` must load the
    # default ruleset (the name constant is the default parameter value).
    assert load_regex_patterns() == load_regex_patterns("default")


def test_load_unknown_ruleset_raises_file_not_found():
    with pytest.raises(FileNotFoundError):
        load_regex_patterns("this-ruleset-does-not-exist")


@pytest.mark.parametrize(
    "bad_name",
    [
        "",               # empty
        "../etc/passwd",  # traversal attempt
        "a/b",            # path separator
        "a\\b",           # windows path separator
        ".hidden",        # leading dot
        "name with spaces",
        "name;rm -rf /",  # shell metacharacter
    ],
)
def test_load_regex_patterns_rejects_invalid_names(bad_name):
    # The `_RULESET_NAME_RE` guard must reject every shape of path-escape
    # attempt with a ValueError, never a FileNotFoundError (which would
    # imply the loader actually looked at the filesystem).
    with pytest.raises(ValueError):
        load_regex_patterns(bad_name)


def test_tokenizer_default_construction_still_works():
    # Regression guard for the rename: zero-arg `Tokenizer()` must keep
    # loading the default ruleset without any ceremony from the caller.
    t = Tokenizer()
    tokens = t.tokenize("Hello world.")
    assert len(tokens) > 0
