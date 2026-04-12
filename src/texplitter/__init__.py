"""texplitter — Rust-backed text segmentation tool.

The core splitter, tokenizer, and tree utilities are implemented in Rust
and exposed to Python via the `texplitter._native` extension module (built
from the `texplitter_rs` crate by maturin). The Python modules under this
package are thin shims that re-export names from the native module.
"""

from importlib.metadata import PackageNotFoundError, version as _pkg_version

try:
    __version__ = _pkg_version("texplitter")
except PackageNotFoundError:
    __version__ = "0.0.0+unknown"

from .tokenizer import Tokenizer, UnknownTokenError
from .splitter import (
    TreeNode,
    SegmentTree,
    convert_tokens_to_string,
    replace_substring_from_end,
)
from .parserrules import regex_patterns, load_regex_patterns, list_rulesets
from .segment import segment, SegmentResult, Segment, ValidationError

__all__ = [
    # High-level API
    "segment",
    "SegmentResult",
    "Segment",
    "ValidationError",
    # Low-level API
    "Tokenizer",
    "UnknownTokenError",
    "TreeNode",
    "SegmentTree",
    "convert_tokens_to_string",
    "replace_substring_from_end",
    "regex_patterns",
    "load_regex_patterns",
    "list_rulesets",
    "__version__",
]
