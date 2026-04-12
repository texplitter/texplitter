"""Splitter shim.

Re-exports the Rust-backed `TreeNode`, `SegmentTree`, and helper
functions from `texplitter._native`. See `texplitter_rs/src/splitter.rs`
for the real code.
"""

from texplitter._native import (
    TreeNode,
    SegmentTree,
    convert_tokens_to_string,
    replace_substring_from_end,
)

__all__ = [
    "TreeNode",
    "SegmentTree",
    "convert_tokens_to_string",
    "replace_substring_from_end",
]
