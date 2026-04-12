//! Top-level error type for texplitter_rs.
//!
//! `TexplitterError` is the Rust-side error boundary for entry points
//! that accept user-supplied input — `SegmentTree::new`,
//! `TreeNode::from_dict`, `convert_tokens_to_string`, and the
//! `Tokenizer` constructor. Malformed input is reported as a typed
//! error so the Python binding can surface a catchable exception
//! instead of aborting the interpreter.
//!
//! Internal code paths that operate on already-validated data keep
//! `unwrap()`/`expect()` as structural-invariant assertions, each with
//! a comment explaining the invariant.
//!
//! Wrap with `?` from any function that returns
//! `Result<_, TexplitterError>`; `fancy_regex::Error` flows through
//! automatically via `#[from]`.

use thiserror::Error;

/// Error returned from user-facing entry points in texplitter_rs.
#[derive(Error, Debug)]
pub enum TexplitterError {
    /// A token map was missing a field the splitter needs, or the field
    /// was the wrong shape (e.g., a number where a string was expected).
    /// Carries the field name and a short hint for diagnosis.
    #[error("invalid token at index {index}: {reason}")]
    InvalidToken {
        index: usize,
        reason: String,
    },

    /// A node dict passed to `TreeNode::from_dict` was missing a field
    /// or had a field with the wrong shape.
    #[error("invalid node dict: {0}")]
    InvalidNodeDict(String),

    /// A regex pattern failed to compile. Wraps the underlying
    /// fancy-regex compile error.
    #[error("invalid regex pattern: {0}")]
    InvalidPattern(#[from] fancy_regex::Error),

    /// A rule file couldn't be read or deserialized.
    #[error("rule file error: {0}")]
    RuleFile(String),
}
