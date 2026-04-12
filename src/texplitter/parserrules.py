"""Parser rules shim.

Loads a packaged rule file from :mod:`texplitter.config.rules` via
:mod:`importlib.resources` (so it keeps working inside wheels and zipapps)
and exposes the :data:`regex_patterns` module-level constant so the
zero-argument ``Tokenizer()`` constructor has a ready-to-use pattern list.

Each ``.json`` file in ``texplitter/config/rules/`` is a standalone ruleset —
a full, order-sensitive list of regex-pattern dicts. There is no composition
between rulesets; callers load exactly one file by name.
"""

import json
import re
from importlib.resources import files
from typing import Any, Dict, List

# Ruleset names are file stems under `texplitter.config.rules`. We restrict
# the allowed character set so user-supplied names can never escape the
# packaged rules directory (no slashes, no `..`, no leading dots). The
# editor API reuses this same regex to validate incoming URL path segments.
_RULESET_NAME_RE = re.compile(r"^[a-zA-Z0-9_-]+$")

DEFAULT_RULESET = "default"


def _validate_ruleset_name(name: str) -> None:
    if not _RULESET_NAME_RE.match(name):
        raise ValueError(
            f"invalid ruleset name {name!r}: must match {_RULESET_NAME_RE.pattern}"
        )


def load_regex_patterns(name: str = DEFAULT_RULESET) -> List[Dict[str, Any]]:
    """Load a packaged ruleset by name.

    Returns a list of dicts, one per rule, in the order they appear in the
    JSON file. Each dict has ``class``, ``type``, ``regex``, ``callback``,
    and optionally ``description`` / ``sample`` keys.

    The default name is :data:`DEFAULT_RULESET` (``"default"``), which maps
    to ``texplitter/config/rules/default.json`` — the baseline grammar
    shipped with the library.
    """
    _validate_ruleset_name(name)
    resource = files("texplitter.config.rules").joinpath(f"{name}.json")
    if not resource.is_file():
        raise FileNotFoundError(f"ruleset {name!r} not found")
    with resource.open("r", encoding="utf-8") as file:
        return json.load(file)


def list_rulesets() -> List[str]:
    """Return the sorted list of packaged ruleset names (file stems).

    Enumerates every ``.json`` file under :mod:`texplitter.config.rules` and
    returns just the stem (no extension, no directory). Useful for building
    a file picker UI or for sanity-checking that a named ruleset exists
    before calling :func:`load_regex_patterns`.
    """
    out: List[str] = []
    for entry in files("texplitter.config.rules").iterdir():
        name = entry.name
        if name.endswith(".json") and entry.is_file():
            out.append(name[: -len(".json")])
    out.sort()
    return out


# Eagerly load the default ruleset on import so
# `from texplitter.parserrules import regex_patterns` returns a ready-to-use
# list without the caller having to invoke `load_regex_patterns()` themselves.
regex_patterns: List[Dict[str, Any]] = load_regex_patterns()


__all__ = [
    "regex_patterns",
    "load_regex_patterns",
    "list_rulesets",
    "DEFAULT_RULESET",
]
