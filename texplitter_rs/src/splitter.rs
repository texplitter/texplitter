use serde::{Deserialize, Serialize};
use serde_json::Value;
use indexmap::IndexMap;
use std::fmt;
use xmlwriter::XmlWriter;

use crate::error::TexplitterError;

/// Kind of a tree node produced by `SegmentTree::build_node`.
///
/// Data-bearing variants carry the fields that only make sense for that
/// variant: the `open`/`close` tag pair on SINGLE nodes and the `operator`
/// symbol on BINARY/BINARY_TEMPLATE nodes. This means the compiler enforces
/// that e.g. a BINARY node always has an operator, and that matching over
/// node kinds is exhaustive.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind")]
pub enum NodeType {
    Multiple,
    /// A single tag wrapper (e.g., `<p>...</p>`). `open` is the full open tag
    /// text; `close` is `Some(close_tag)` when the tag pair was closed, `None`
    /// for the unclosed edge case.
    Single { open: String, close: Option<String> },
    /// A binary split produced by a BINARY token (e.g., a dot/break operator).
    /// `operator` is the token text that caused the split.
    Binary { operator: String },
    /// Binary split from a BINARY_TEMPLATE token (same shape as Binary, but the
    /// downstream placeholder logic treats the operator differently).
    BinaryTemplate { operator: String },
    Translatable,
    Dummy,
}

impl NodeType {
    /// String tag used for SegmentTree::build_node's pattern-matching on
    /// token classes. This keeps the `token_class` → `NodeType` mapping from
    /// the tokenizer's output explicit at the one ingestion site.
    fn legacy_tag(&self) -> &'static str {
        match self {
            NodeType::Multiple => "MULTIPLE",
            NodeType::Single { .. } => "SINGLE",
            NodeType::Binary { .. } => "BINARY",
            NodeType::BinaryTemplate { .. } => "BINARY_TEMPLATE",
            NodeType::Translatable => "TRANSLATABLE",
            NodeType::Dummy => "DUMMY",
        }
    }
}

/// Strongly-typed view of a token's `token_class` field. Parsed once from the
/// raw `Value::String` when a comparison is needed, so the tokenizer is still
/// free to emit `IndexMap<String, Value>` tokens for the Python side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenClass {
    OpenBracket,
    CloseBracket,
    OpenTag,
    CloseTag,
    BeginningTag,
    MiddleTag,
    EndCloseTag,
    EndOpenTag,
    /// Self-closing structural element (e.g., `<br/>`, `&amp;`,
    /// or AsciiDoc constructs like `<<anchor>>`). Pushes and
    /// immediately pops the tag stack so the element is treated
    /// as a protected inline node, not translatable content.
    SingleTag,
    /// Anything the tokenizer emits that isn't one of the variants above
    /// (e.g., PUNCTUATION, BREAK, SYMBOL, SPACE, TRANSLATABLE, etc.).
    /// The splitter cares about structural classes; lexical classes all
    /// funnel through here, and the `split_token_list` BREAK pass reads
    /// the raw `token_class` string directly when it needs to
    /// distinguish `BREAK` / `BREAK_TEMPLATE` from the rest.
    Other,
}

impl TokenClass {
    /// Parse from the raw string stored in `token["token_class"]`.
    pub fn from_str(s: &str) -> TokenClass {
        match s {
            "OPEN_BRACKET" => TokenClass::OpenBracket,
            "CLOSE_BRACKET" => TokenClass::CloseBracket,
            "OPEN_TAG" => TokenClass::OpenTag,
            "CLOSE_TAG" => TokenClass::CloseTag,
            "BEGINNING_TAG" => TokenClass::BeginningTag,
            "MIDDLE_TAG" => TokenClass::MiddleTag,
            "END_CLOSE_TAG" => TokenClass::EndCloseTag,
            "END_OPEN_TAG" => TokenClass::EndOpenTag,
            "SINGLE_TAG" => TokenClass::SingleTag,
            _ => TokenClass::Other,
        }
    }

    /// Pull the `token_class` string out of a token map and parse it.
    ///
    /// Structural invariant: every token reaching this function has
    /// already been validated by `validate_tokens` at the entry to
    /// `SegmentTree::new`, so `token_class` is guaranteed present and
    /// a `Value::String`. The `unwrap`s below are assertions, not
    /// error paths. If this panics, either (a) a caller bypassed the
    /// `SegmentTree::new` entry point — a bug in that caller — or
    /// (b) the tokenizer emitted a malformed token, which is a bug in
    /// the tokenizer.
    pub fn of(token: &IndexMap<String, Value>) -> TokenClass {
        TokenClass::from_str(token["token_class"].as_str().unwrap())
    }
}


/// Iterative post-order walker used by `build_pattern` and `build_full_value`.
///
/// At each node the caller's closure is run with a slice of the already-built
/// child strings (in source order). The walker uses two stacks internally:
///
///   * `work: Vec<Frame<'a>>`   — pre- and post-order markers. `Visit(node)`
///                                 pushes `Finalize(node, n_children)` then
///                                 pushes every child as another `Visit`
///                                 (rightmost first so leftmost pops first).
///                                 `Finalize` pops `n_children` results, runs
///                                 the closure, and pushes one result.
///   * `results: Vec<String>`   — parallel stack of finished values.
///
/// This is the standard trampoline for folds over a tree. The walker takes
/// `&TreeNode`, so no unsafe, no ownership juggling — lifetimes tie each
/// borrow to the root the caller passed in.
fn post_order_string_fold<'a, F>(root: &'a TreeNode, mut finalize: F) -> String
where
    F: FnMut(&'a TreeNode, &[String]) -> String,
{
    enum Frame<'a> {
        Visit(&'a TreeNode),
        Finalize(&'a TreeNode, usize),
    }

    let mut work: Vec<Frame<'a>> = vec![Frame::Visit(root)];
    let mut results: Vec<String> = Vec::new();

    while let Some(frame) = work.pop() {
        match frame {
            Frame::Visit(node) => {
                let n = node.children.len();
                work.push(Frame::Finalize(node, n));
                // Push children in reverse so the leftmost child resolves
                // first (deposits result_0 at the bottom of this node's
                // slice of `results`).
                for child in node.children.iter().rev() {
                    work.push(Frame::Visit(child.as_ref()));
                }
            }
            Frame::Finalize(node, n) => {
                let start = results.len() - n;
                // Run the fold closure with a borrow of the child results.
                // Collecting into a local Vec so we can drain `results` in
                // place without fighting the borrow checker.
                let out = {
                    let child_results = &results[start..];
                    finalize(node, child_results)
                };
                results.truncate(start);
                results.push(out);
            }
        }
    }

    debug_assert_eq!(results.len(), 1);
    results.pop().expect("post_order_string_fold: empty result")
}

/// Extended-segment record produced by `TreeNode::extract_extended_segments`
/// and consumed by `TreeNode::rebuild_from_segments`.
///
/// This replaces the old `serde_json::Value` return shape, which nested one
/// object per `Single` / `Binary` level and recursed on construction, on
/// conversion, and on drop. `ExtendedSegment` stores children inside a
/// dedicated enum with an iterative `Drop` impl, so a 2000-level tree no
/// longer overflows the stack at any point.
///
/// The wire-level shape is the same as the Python implementation's nested
/// dict: a `Leaf` carries `{segment, attributes}`, a `WithChildren` carries
/// `{segment, attributes, children: [...]}`, and a `Binary` carries
/// `{left, right, operator}`. `rebuild_from_segments` only reads the `segment`
/// field at the top-level tag, so the nested children exist for callers
/// (notably the Python side) that want to walk the structure themselves.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedSegment {
    pub segment: String,
    pub attributes: Vec<(String, String)>,
    pub inner: ExtendedSegmentInner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ExtendedSegmentInner {
    /// `Translatable` / `Dummy` leaf. No children.
    Leaf,
    /// `Single` node: carries the per-child nested extended-segment maps in
    /// source order. Matches the Python shape `children: [ {...}, {...} ]`.
    WithChildren(Vec<IndexMap<String, ExtendedSegment>>),
    /// `Binary` / `BinaryTemplate` node: carries left and right child maps
    /// plus the operator text. Boxed so `ExtendedSegment`'s size stays O(1)
    /// regardless of how deep the nesting goes.
    Binary {
        left: Box<IndexMap<String, ExtendedSegment>>,
        right: Box<IndexMap<String, ExtendedSegment>>,
        operator: String,
    },
}

impl ExtendedSegment {
    fn new_leaf(segment: String, attributes: Vec<(String, String)>) -> Self {
        ExtendedSegment {
            segment,
            attributes,
            inner: ExtendedSegmentInner::Leaf,
        }
    }
}

/// Iterative Drop for `ExtendedSegment`. The derived `Drop` would recurse
/// through the `IndexMap<String, ExtendedSegment>` values one frame per
/// nesting level — same shape of bug as `TreeNode::Drop` on
/// `Vec<Box<TreeNode>>`. The fix is the same: move the nested maps out into
/// an explicit heap work stack and drain it in place.
impl Drop for ExtendedSegment {
    fn drop(&mut self) {
        // Pull this node's inner payload out so the derived Drop that runs
        // when `self` finishes scoping has nothing to recurse through.
        let inner = std::mem::replace(&mut self.inner, ExtendedSegmentInner::Leaf);
        let mut stack: Vec<IndexMap<String, ExtendedSegment>> = Vec::new();
        match inner {
            ExtendedSegmentInner::Leaf => return,
            ExtendedSegmentInner::WithChildren(children) => {
                stack.extend(children);
            }
            ExtendedSegmentInner::Binary { left, right, .. } => {
                stack.push(*left);
                stack.push(*right);
            }
        }
        while let Some(mut map) = stack.pop() {
            // Drain this map's values, taking each `ExtendedSegment`'s inner
            // payload out before the derived drop runs. The drained
            // sub-ExtendedSegments will in turn hit this same Drop impl at
            // leaf depth, where the `Leaf` short-circuit returns immediately,
            // so recursion never stacks up.
            for (_, mut child) in map.drain(..) {
                let child_inner =
                    std::mem::replace(&mut child.inner, ExtendedSegmentInner::Leaf);
                match child_inner {
                    ExtendedSegmentInner::Leaf => {}
                    ExtendedSegmentInner::WithChildren(grandchildren) => {
                        stack.extend(grandchildren);
                    }
                    ExtendedSegmentInner::Binary { left, right, .. } => {
                        stack.push(*left);
                        stack.push(*right);
                    }
                }
                // `child` drops here with `inner = Leaf`, so the Drop impl
                // above takes the early-return branch.
            }
        }
    }
}

/// Concatenate the `text` field of every token in order. Tokens
/// missing the `text` field contribute an empty string rather than
/// panicking — the old `.unwrap()` turned a missing key into a
/// process abort, which is indefensible for a pub helper.
pub fn convert_tokens_to_string(tokens: &[IndexMap<String, String>]) -> String {
    tokens
        .iter()
        .map(|token| token.get("text").map(String::as_str).unwrap_or(""))
        .collect::<Vec<&str>>()
        .join("")
}

/// Validate a token list before handing it to `SegmentTree::build_node`.
///
/// After this pass returns `Ok(())`, every token is guaranteed to have
/// `token_class`, `text`, and `label` as `Value::String`, which lets the
/// internal walks in `build_node`, `split_token_list`,
/// `analyze_token_segments`, and `TokenClass::of` treat
/// `.as_str().unwrap()` as a structural-invariant assertion.
///
/// We only check the three fields the splitter actually reads. Extra
/// fields (`start`, `end`, `token_type`, …) are allowed to be any
/// shape — the tokenizer produces `Value::Number` for `start`/`end`, so
/// blanket-requiring strings would break real callers.
fn validate_tokens(tokens: &[IndexMap<String, Value>]) -> Result<(), TexplitterError> {
    for (index, token) in tokens.iter().enumerate() {
        for field in ["token_class", "text", "label"] {
            match token.get(field) {
                None => {
                    return Err(TexplitterError::InvalidToken {
                        index,
                        reason: format!("missing required field `{}`", field),
                    });
                }
                Some(v) if !v.is_string() => {
                    return Err(TexplitterError::InvalidToken {
                        index,
                        reason: format!(
                            "field `{}` must be a string, got {}",
                            field,
                            match v {
                                Value::Null => "null",
                                Value::Bool(_) => "bool",
                                Value::Number(_) => "number",
                                Value::Array(_) => "array",
                                Value::Object(_) => "object",
                                Value::String(_) => unreachable!(),
                            }
                        ),
                    });
                }
                Some(_) => {}
            }
        }
    }
    Ok(())
}

pub fn replace_substring_from_end(text: &str, old: &str, new: &str, count: usize) -> String {
    let parts: Vec<&str> = text.rsplitn(count + 1, old).collect();
    parts.into_iter().rev().collect::<Vec<&str>>().join(new)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    /// Discriminated on `NodeType` — the old loose `node_type: String` +
    /// `value: String` + `tag: Vec<String>` trio collapsed into variants that
    /// only carry fields relevant to that kind.
    pub node_type: NodeType,
    /// Literal text content. Only `Translatable` and `Dummy` nodes use it for
    /// anything meaningful; for the other variants it's an empty string kept
    /// around for `from_dict` round-tripping (the operator lives in
    /// `NodeType::Binary { operator }`, not here).
    pub value: String,
    pub children: Vec<Box<TreeNode>>,
    pub attributes: Vec<(String, String)>,
}

impl TreeNode {
    /// Direct constructor for an already-typed node. Used by the SegmentTree
    /// builder and by tests.
    pub fn new(
        node_type: NodeType,
        value: &str,
        children: Vec<Box<TreeNode>>,
        attributes: Vec<(String, String)>,
    ) -> TreeNode {
        TreeNode {
            node_type,
            value: value.to_string(),
            children,
            attributes,
        }
    }

    /// Build a node from the untyped `(node_type: &str, value, tag, ...)`
    /// shape used by `from_dict`. The only in-tree caller is `from_dict`;
    /// keeping it `pub(crate)` so it doesn't leak.
    pub(crate) fn from_legacy(
        node_type: &str,
        value: &str,
        tag: Vec<String>,
        children: Vec<Box<TreeNode>>,
        attributes: Vec<(String, String)>,
    ) -> TreeNode {
        let nt = match node_type {
            "MULTIPLE" => NodeType::Multiple,
            "SINGLE" => {
                let open = tag.get(0).cloned().unwrap_or_default();
                let close = tag.get(1).cloned();
                NodeType::Single { open, close }
            }
            "BINARY" => NodeType::Binary { operator: value.to_string() },
            "BINARY_TEMPLATE" => NodeType::BinaryTemplate { operator: value.to_string() },
            "TRANSLATABLE" => NodeType::Translatable,
            "DUMMY" => NodeType::Dummy,
            _ => NodeType::Dummy,
        };
        TreeNode {
            node_type: nt,
            value: value.to_string(),
            children,
            attributes,
        }
    }

    /// Build a `TreeNode` from an `IndexMap` with the shape
    /// `{node_type, value, tag, children, attributes}`.
    ///
    /// Returns `TexplitterError::InvalidNodeDict` on any missing or
    /// wrong-type field. Used by the Python binding.
    ///
    /// Iterative (post-order) walk: the work-stack pushes an `Expand`
    /// frame for each dict that still needs to be decoded; when a dict
    /// has children it pushes a `Finalize` frame for itself plus one
    /// `Expand` per child (in reverse so the children resolve
    /// left-to-right on the parallel results stack). Iterative rather
    /// than recursive so deeply-nested input cannot overflow the
    /// thread stack before the error path runs.
    pub fn from_dict(node_dict: &IndexMap<String, Value>) -> Result<TreeNode, TexplitterError> {
        // A decoded "shell" of a node without its children — the
        // children will be popped off the parallel results stack by
        // the matching `Finalize` frame.
        struct Shell {
            node_type: String,
            value: String,
            tag: Vec<String>,
            attributes: Vec<(String, String)>,
            n_children: usize,
        }

        enum Frame {
            // An undecoded child dict, still waiting to be expanded.
            // Borrowed from the input tree so we don't have to clone
            // every nested object ahead of time.
            Expand(IndexMap<String, Value>),
            Finalize(Shell),
        }

        fn field_str<'a>(
            dict: &'a IndexMap<String, Value>,
            field: &'static str,
        ) -> Result<&'a str, TexplitterError> {
            dict.get(field)
                .ok_or_else(|| {
                    TexplitterError::InvalidNodeDict(format!("missing field `{}`", field))
                })?
                .as_str()
                .ok_or_else(|| {
                    TexplitterError::InvalidNodeDict(format!(
                        "field `{}` must be a string",
                        field
                    ))
                })
        }

        fn decode_shell_and_children(
            dict: &IndexMap<String, Value>,
        ) -> Result<(Shell, Vec<IndexMap<String, Value>>), TexplitterError> {
            let node_type = field_str(dict, "node_type")?.to_string();
            let value = field_str(dict, "value")?.to_string();

            let tag_array = dict
                .get("tag")
                .ok_or_else(|| TexplitterError::InvalidNodeDict("missing field `tag`".into()))?
                .as_array()
                .ok_or_else(|| {
                    TexplitterError::InvalidNodeDict("field `tag` must be an array".into())
                })?;
            let tag: Vec<String> = tag_array
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    v.as_str().map(|s| s.to_string()).ok_or_else(|| {
                        TexplitterError::InvalidNodeDict(format!(
                            "tag[{}] must be a string",
                            i
                        ))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;

            let attributes: Vec<(String, String)> = match dict.get("attributes") {
                None | Some(Value::Null) => Vec::new(),
                Some(v) => {
                    let arr = v.as_array().ok_or_else(|| {
                        TexplitterError::InvalidNodeDict(
                            "field `attributes` must be an array".into(),
                        )
                    })?;
                    let mut out = Vec::with_capacity(arr.len());
                    for (i, attr) in arr.iter().enumerate() {
                        let attr_arr = attr.as_array().ok_or_else(|| {
                            TexplitterError::InvalidNodeDict(format!(
                                "attributes[{}] must be a 2-element array",
                                i
                            ))
                        })?;
                        if attr_arr.len() != 2 {
                            return Err(TexplitterError::InvalidNodeDict(format!(
                                "attributes[{}] must have exactly 2 elements",
                                i
                            )));
                        }
                        let key = attr_arr[0].as_str().ok_or_else(|| {
                            TexplitterError::InvalidNodeDict(format!(
                                "attributes[{}][0] must be a string",
                                i
                            ))
                        })?;
                        let val = attr_arr[1].as_str().ok_or_else(|| {
                            TexplitterError::InvalidNodeDict(format!(
                                "attributes[{}][1] must be a string",
                                i
                            ))
                        })?;
                        out.push((key.to_string(), val.to_string()));
                    }
                    out
                }
            };

            let children_dicts: Vec<IndexMap<String, Value>> = match dict.get("children") {
                None | Some(Value::Null) => Vec::new(),
                Some(v) => {
                    let arr = v.as_array().ok_or_else(|| {
                        TexplitterError::InvalidNodeDict(
                            "field `children` must be an array".into(),
                        )
                    })?;
                    let mut out = Vec::with_capacity(arr.len());
                    for (i, child) in arr.iter().enumerate() {
                        let child_obj = child.as_object().ok_or_else(|| {
                            TexplitterError::InvalidNodeDict(format!(
                                "children[{}] must be an object",
                                i
                            ))
                        })?;
                        let child_map: IndexMap<String, Value> = child_obj
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        out.push(child_map);
                    }
                    out
                }
            };

            let shell = Shell {
                node_type,
                value,
                tag,
                attributes,
                n_children: children_dicts.len(),
            };
            Ok((shell, children_dicts))
        }

        let mut work: Vec<Frame> = vec![Frame::Expand(node_dict.clone())];
        let mut results: Vec<TreeNode> = Vec::new();

        while let Some(frame) = work.pop() {
            match frame {
                Frame::Expand(dict) => {
                    let (shell, children_dicts) = decode_shell_and_children(&dict)?;
                    // Push Finalize first so it fires after every child
                    // has been fully decoded. Children go on the stack
                    // in reverse so popping yields them in source order.
                    work.push(Frame::Finalize(shell));
                    for child_dict in children_dicts.into_iter().rev() {
                        work.push(Frame::Expand(child_dict));
                    }
                }
                Frame::Finalize(shell) => {
                    let start = results.len() - shell.n_children;
                    let children: Vec<Box<TreeNode>> =
                        results.drain(start..).map(Box::new).collect();
                    results.push(TreeNode::from_legacy(
                        &shell.node_type,
                        &shell.value,
                        shell.tag,
                        children,
                        shell.attributes,
                    ));
                }
            }
        }

        debug_assert_eq!(results.len(), 1, "from_dict finished with {} results", results.len());
        // Structural invariant: every Expand frame pushes a matching
        // Finalize that produces exactly one result, and we seeded
        // with exactly one Expand. So results has exactly one element
        // when the loop terminates.
        Ok(results.pop().expect("from_dict produced no result"))
    }

    /// Convenience: string tag for callers that need to branch on the
    /// MULTIPLE/SINGLE/BINARY/... names. New Rust code should `match` on
    /// `self.node_type` directly.
    pub fn node_type_tag(&self) -> &'static str {
        self.node_type.legacy_tag()
    }

    /// Collect this node's attributes plus every descendant's, in pre-order.
    /// Iterative walk using an explicit work stack over `&TreeNode` so that
    /// deeply nested trees don't overflow the thread stack.
    pub fn get_attributes(&self) -> Vec<(String, String)> {
        let mut attributes: Vec<(String, String)> = Vec::new();
        let mut stack: Vec<&TreeNode> = vec![self];
        while let Some(node) = stack.pop() {
            attributes.extend(node.attributes.iter().cloned());
            // Reverse so leftmost child is visited first; this yields the
            // pre-order sequence `self.attributes` then each child in turn.
            for child in node.children.iter().rev() {
                stack.push(child.as_ref());
            }
        }
        attributes
    }

    /// Build the placeholder pattern (e.g., "<p>TRANSLATABLE</p>") for this
    /// subtree. Post-order walk with a parallel results stack — see
    /// `build_full_value` for the walk-frame shape; this one reuses it.
    pub fn build_pattern(&self) -> String {
        // For leaves `build_pattern` returns a fixed string; for inner nodes
        // it concatenates child strings with per-variant fencing. So we just
        // need a post-order walk that emits one `String` per visited node
        // and uses a small "how many children do I consume" frame.
        post_order_string_fold(self, |node, child_strings| match &node.node_type {
            NodeType::Translatable | NodeType::Dummy => "TRANSLATABLE".to_string(),
            NodeType::Single { open, close } => {
                let mut out = open.clone();
                for s in child_strings {
                    out.push_str(s);
                }
                if let Some(close) = close {
                    out.push_str(close);
                }
                out
            }
            NodeType::Binary { operator } | NodeType::BinaryTemplate { operator } => {
                debug_assert_eq!(child_strings.len(), 2);
                format!("{}{}{}", child_strings[0], operator, child_strings[1])
            }
            NodeType::Multiple => child_strings.concat(),
        })
    }

    /// Reconstruct the original text that this subtree represents. Post-order
    /// walk with a parallel `Vec<String>` results stack — this is the
    /// tightest inner loop of `generate_placeholders_for_segments`, called
    /// once per frame, so it is iterative rather than recursive.
    pub fn build_full_value(&self) -> String {
        post_order_string_fold(self, |node, child_strings| match &node.node_type {
            NodeType::Translatable | NodeType::Dummy => node.value.clone(),
            NodeType::Single { open, close } => {
                let mut out = open.clone();
                for s in child_strings {
                    out.push_str(s);
                }
                if let Some(close) = close {
                    out.push_str(close);
                }
                out
            }
            NodeType::Binary { operator } | NodeType::BinaryTemplate { operator } => {
                debug_assert_eq!(child_strings.len(), 2);
                format!("{}{}{}", child_strings[0], operator, child_strings[1])
            }
            NodeType::Multiple => child_strings.concat(),
        })
    }

    pub fn strip_spaces(&self, text: &str) -> (String, String, String) {
        // Count leading ASCII spaces (matches Python `text.lstrip(' ')`).
        let leading_spaces = text.len() - text.trim_start_matches(' ').len();
        let leading_space_placeholder = "{sp}".repeat(leading_spaces);

        // We need to strip trailing spaces that precede one-or-more trailing
        // newlines, replacing them with `{sp}` placeholders. The pattern is
        // ` *\n+$`: any run of trailing spaces followed by one or more
        // newlines, anchored to end-of-string. Matching that directly by
        // byte-scanning is faster than spinning up a regex engine on the
        // hot path of `generate_placeholders_for_segments`.
        let bytes = text.as_bytes();
        // Find trailing run of `\n` then the optional run of ` ` before it.
        let nl_end = bytes.len();
        let mut nl_start = nl_end;
        while nl_start > 0 && bytes[nl_start - 1] == b'\n' {
            nl_start -= 1;
        }
        if nl_start < nl_end {
            // Found one or more trailing newlines.
            let mut sp_start = nl_start;
            while sp_start > 0 && bytes[sp_start - 1] == b' ' {
                sp_start -= 1;
            }
            // Mirror the Python branch: strip leading+trailing spaces from
            // `text`, then slice off the matched ` *\n+` tail.
            let stripped_text = text.trim_matches(' ');
            let match_len = nl_end - sp_start;
            // The matched tail is at the end of `stripped_text` too — the
            // only difference between `text` and `stripped_text` is leading
            // and trailing ASCII spaces, and the tail is spaces+newlines at
            // the very end, which `trim_matches(' ')` does not touch (it
            // only strips spaces, not newlines).
            // Python slicing `stripped_text[:len - match_len]` silently
            // clamps to 0 when `match_len > len`. Mirror that with
            // `saturating_sub` so `" \n"` → segment `""`.
            let cut = stripped_text.len().saturating_sub(match_len);
            let segment = stripped_text[..cut].to_string();
            let matched_tail = &text[sp_start..nl_end];
            let trailing_space_placeholder = matched_tail.replace(' ', "{sp}");
            (segment, leading_space_placeholder, trailing_space_placeholder)
        } else {
            // No trailing newline — count trailing ASCII spaces and emit
            // them as placeholders.
            let trailing_spaces = text.len() - text.trim_end_matches(' ').len();
            let segment = text.trim_matches(' ').to_string();
            let trailing_space_placeholder = "{sp}".repeat(trailing_spaces);
            (segment, leading_space_placeholder, trailing_space_placeholder)
        }
    }

    /// Walk the tree in-order, emitting placeholders (`%%N%%`) for each
    /// translatable run and pushing the run text onto `segments`. The old
    /// recursive version threaded `(index, result_string)` through every
    /// call site; this trampoline threads them through the parallel results
    /// stack instead. The `index` counter is just `segment_index +
    /// segments_emitted_so_far` at any point, so it's recovered from
    /// `segments.len()` rather than pushed on the work stack.
    pub fn generate_placeholders_for_segments(
        &self,
        end: &str,
        segment_index: usize,
        segments: &mut Vec<(usize, String)>,
    ) -> (usize, String) {
        enum Frame<'a> {
            Visit {
                node: &'a TreeNode,
                end: String,
            },
            FinalizeSingle {
                open: String,
                close: Option<String>,
                n: usize,
            },
            FinalizeBinary {
                operator: String,
            },
            FinalizeBinaryTemplate {
                operator: String,
            },
            FinalizeMultipleConcat {
                n: usize,
            },
        }

        let start_segments_len = segments.len();
        let index_of = |segments: &Vec<(usize, String)>| segment_index + (segments.len() - start_segments_len);

        let mut work: Vec<Frame> = vec![Frame::Visit { node: self, end: end.to_string() }];
        let mut results: Vec<String> = Vec::new();

        while let Some(frame) = work.pop() {
            match frame {
                Frame::Visit { node, end } => match &node.node_type {
                    NodeType::Multiple => {
                        let is_translatable = node
                            .children
                            .iter()
                            .any(|child| matches!(child.node_type, NodeType::Translatable));
                        if is_translatable {
                            // Emit a single placeholder for the whole
                            // MULTIPLE subtree. `build_full_value` is itself
                            // iterative now, so this is safe at any depth.
                            let full_value = node.build_full_value();
                            if !full_value.trim().is_empty() {
                                let new_index = index_of(segments) + 1;
                                let (segment, leading, trailing) =
                                    node.strip_spaces(&(full_value + &end));
                                segments.push((new_index, segment));
                                let mut s = leading;
                                s.push_str(&format!("%%{}%%", new_index));
                                s.push_str(&trailing);
                                results.push(s);
                            } else {
                                results.push(String::new());
                            }
                        } else {
                            // Same `end` is passed to every child in the old
                            // recursive code. Children's results are
                            // concatenated in source order.
                            let n = node.children.len();
                            work.push(Frame::FinalizeMultipleConcat { n });
                            for child in node.children.iter().rev() {
                                work.push(Frame::Visit {
                                    node: child.as_ref(),
                                    end: end.clone(),
                                });
                            }
                        }
                    }
                    NodeType::Binary { operator } => {
                        work.push(Frame::FinalizeBinary { operator: operator.clone() });
                        // Right pushed first so Left pops first — left's
                        // `end` is the operator, right's is "".
                        work.push(Frame::Visit {
                            node: node.children[1].as_ref(),
                            end: String::new(),
                        });
                        work.push(Frame::Visit {
                            node: node.children[0].as_ref(),
                            end: operator.clone(),
                        });
                    }
                    NodeType::BinaryTemplate { operator } => {
                        work.push(Frame::FinalizeBinaryTemplate { operator: operator.clone() });
                        work.push(Frame::Visit {
                            node: node.children[1].as_ref(),
                            end: String::new(),
                        });
                        work.push(Frame::Visit {
                            node: node.children[0].as_ref(),
                            end: String::new(),
                        });
                    }
                    NodeType::Single { open, close } => {
                        let n = node.children.len();
                        work.push(Frame::FinalizeSingle {
                            open: open.clone(),
                            close: close.clone(),
                            n,
                        });
                        // `end` comes after the node's content, so it is
                        // passed to the LAST child only. In practice Single
                        // nodes always have exactly one child in the
                        // SegmentTree, so this is a no-op — but the
                        // multi-child shape keeps the walk total.
                        for (i, child) in node.children.iter().enumerate().rev() {
                            let child_end = if i == n - 1 { end.clone() } else { String::new() };
                            work.push(Frame::Visit {
                                node: child.as_ref(),
                                end: child_end,
                            });
                        }
                    }
                    NodeType::Translatable | NodeType::Dummy => {
                        let full_value = node.build_full_value();
                        if !full_value.trim().is_empty() {
                            let new_index = index_of(segments) + 1;
                            let (segment, leading, trailing) =
                                node.strip_spaces(&(full_value + &end));
                            segments.push((new_index, segment));
                            let mut s = leading;
                            s.push_str(&format!("%%{}%%", new_index));
                            s.push_str(&trailing);
                            results.push(s);
                        } else {
                            results.push(String::new());
                        }
                    }
                },
                Frame::FinalizeSingle { open, close, n } => {
                    let start = results.len() - n;
                    let mut s = open;
                    for child_str in results.drain(start..) {
                        s.push_str(&child_str);
                    }
                    if let Some(close) = close {
                        s.push_str(&close);
                    }
                    results.push(s);
                }
                Frame::FinalizeBinary { operator } => {
                    let right = results.pop().expect("binary right missing");
                    let left = results.pop().expect("binary left missing");
                    results.push(format!("{}{}{}", left, operator, right));
                }
                Frame::FinalizeBinaryTemplate { operator } => {
                    let right = results.pop().expect("binary_template right missing");
                    let left = results.pop().expect("binary_template left missing");
                    results.push(format!("{}{}{}", left, operator, right));
                }
                Frame::FinalizeMultipleConcat { n } => {
                    let start = results.len() - n;
                    let mut s = String::new();
                    for child_str in results.drain(start..) {
                        s.push_str(&child_str);
                    }
                    results.push(s);
                }
            }
        }

        debug_assert_eq!(results.len(), 1);
        let final_result = results.pop().unwrap();
        (index_of(segments), final_result)
    }

    /// Walk the tree and produce a flat `{tag -> segment text}` map.
    /// Iterative post-order walk: each `Visit` frame carries the tag string
    /// its caller computed, and a `Finalize` frame pops the per-child result
    /// maps from a parallel stack and merges them, with variant-specific
    /// fencing (Binary inserts a `{tag}_operator` key; Single allocates an
    /// index from the shared `tag_list` at Visit-time so the numbering order
    /// matches the original pre-order recursive walk).
    pub fn extract_segments(&self) -> Vec<IndexMap<String, String>> {
        enum Frame<'a> {
            Visit { node: &'a TreeNode, tag: String },
            FinalizeSingle { n: usize },
            FinalizeBinary { tag: String, operator: String },
            FinalizeMultiple { n: usize },
        }

        let mut work: Vec<Frame> = vec![Frame::Visit { node: self, tag: "TRANSLATABLE".to_string() }];
        let mut results: Vec<IndexMap<String, String>> = Vec::new();
        let mut tag_list: Vec<String> = Vec::new();

        while let Some(frame) = work.pop() {
            match frame {
                Frame::Visit { node, tag } => match &node.node_type {
                    NodeType::Translatable | NodeType::Dummy => {
                        let mut m = IndexMap::new();
                        m.insert(tag, node.value.clone());
                        results.push(m);
                    }
                    NodeType::Single { .. } => {
                        // Allocate an index from the shared `tag_list` AT
                        // VISIT TIME (pre-order), before walking children —
                        // this way each descendant sees the parent's index
                        // when computing its own.
                        let trimmed = node.value.trim().to_string();
                        let mut index = 0usize;
                        while tag_list.contains(&format!("{}-{}", trimmed, index)) {
                            index += 1;
                        }
                        let new_tag = format!("{}/{}-{}", tag, trimmed, index);
                        tag_list.push(format!("{}-{}", trimmed, index));

                        let n = node.children.len();
                        work.push(Frame::FinalizeSingle { n });
                        for child in node.children.iter().rev() {
                            work.push(Frame::Visit { node: child.as_ref(), tag: new_tag.clone() });
                        }
                    }
                    NodeType::Binary { operator } | NodeType::BinaryTemplate { operator } => {
                        let left_tag = format!("{}_L", tag);
                        let right_tag = format!("{}_R", tag);
                        work.push(Frame::FinalizeBinary { tag: tag.clone(), operator: operator.clone() });
                        // Right-first push so left pops first and its result
                        // lands at the bottom of this frame's slice.
                        work.push(Frame::Visit { node: node.children[1].as_ref(), tag: right_tag });
                        work.push(Frame::Visit { node: node.children[0].as_ref(), tag: left_tag });
                    }
                    NodeType::Multiple => {
                        let n = node.children.len();
                        work.push(Frame::FinalizeMultiple { n });
                        for child in node.children.iter().rev() {
                            work.push(Frame::Visit { node: child.as_ref(), tag: tag.clone() });
                        }
                    }
                },
                Frame::FinalizeSingle { n } => {
                    let start = results.len() - n;
                    let mut segments: IndexMap<String, String> = IndexMap::new();
                    for child in results.drain(start..) {
                        segments.extend(child);
                    }
                    results.push(segments);
                }
                Frame::FinalizeBinary { tag, operator } => {
                    // Results on stack (in push order): [left, right]
                    let right = results.pop().expect("binary: right missing");
                    let left = results.pop().expect("binary: left missing");
                    let mut segments: IndexMap<String, String> = IndexMap::new();
                    segments.insert(format!("{}_operator", tag), operator);
                    segments.extend(left);
                    segments.extend(right);
                    results.push(segments);
                }
                Frame::FinalizeMultiple { n } => {
                    let start = results.len() - n;
                    let mut segments: IndexMap<String, String> = IndexMap::new();
                    for child in results.drain(start..) {
                        segments.extend(child);
                    }
                    results.push(segments);
                }
            }
        }

        debug_assert_eq!(results.len(), 1);
        vec![results.pop().unwrap()]
    }

    /// Same shape as `extract_segments` but emits a rich JSON-backed map
    /// instead of plain strings, and wraps Single/Binary nodes in their own
    /// sub-object rather than flat-merging children. Iterative post-order
    /// trampoline, same frame pattern.
    /// Walk the tree and produce an extended-segments map keyed by tag path.
    ///
    /// The return type is `IndexMap<String, ExtendedSegment>` rather than
    /// `IndexMap<String, serde_json::Value>` — see `ExtendedSegment` for why.
    /// The shape is otherwise a 1:1 match with the Python
    /// `_extract_extended_segments`:
    ///
    ///   * Leaf (`Translatable`/`Dummy`): `{segment, attributes}`.
    ///   * Single: `{segment, attributes, children: [ <child map>, ... ]}`.
    ///   * Binary / BinaryTemplate: `{left: <left map>, right: <right map>,
    ///     operator}`. Note: the `segment`/`attributes` fields on the
    ///     `ExtendedSegment` wrapper for a Binary node are empty; the Python
    ///     implementation doesn't emit them either.
    ///   * Multiple: its own key doesn't appear in the output — the child
    ///     maps are merged into the parent's map (just like Python's
    ///     `extended_segments.update(child_segments)` branch).
    pub fn extract_extended_segments(&self) -> IndexMap<String, ExtendedSegment> {
        enum Frame<'a> {
            Visit { node: &'a TreeNode, tag: String },
            FinalizeSingle {
                node: &'a TreeNode,
                new_tag: String,
                n: usize,
            },
            FinalizeBinary {
                tag: String,
                operator: String,
            },
            FinalizeMultiple { n: usize },
        }

        let mut work: Vec<Frame> = vec![Frame::Visit {
            node: self,
            tag: "TRANSLATABLE".to_string(),
        }];
        let mut results: Vec<IndexMap<String, ExtendedSegment>> = Vec::new();
        let mut tag_list: Vec<String> = Vec::new();

        while let Some(frame) = work.pop() {
            match frame {
                Frame::Visit { node, tag } => match &node.node_type {
                    NodeType::Translatable | NodeType::Dummy => {
                        let mut m = IndexMap::new();
                        m.insert(
                            tag,
                            ExtendedSegment::new_leaf(
                                node.value.clone(),
                                node.get_attributes(),
                            ),
                        );
                        results.push(m);
                    }
                    NodeType::Single { .. } => {
                        let trimmed = node.value.trim().to_string();
                        let mut index = 0usize;
                        while tag_list.contains(&format!("{}-{}", trimmed, index)) {
                            index += 1;
                        }
                        let new_tag = format!("{}/{}-{}", tag, trimmed, index);
                        tag_list.push(format!("{}-{}", trimmed, index));

                        let n = node.children.len();
                        work.push(Frame::FinalizeSingle {
                            node,
                            new_tag: new_tag.clone(),
                            n,
                        });
                        for child in node.children.iter().rev() {
                            work.push(Frame::Visit {
                                node: child.as_ref(),
                                tag: new_tag.clone(),
                            });
                        }
                    }
                    NodeType::Binary { operator } | NodeType::BinaryTemplate { operator } => {
                        let left_tag = format!("{}_L", tag);
                        let right_tag = format!("{}_R", tag);
                        work.push(Frame::FinalizeBinary {
                            tag: tag.clone(),
                            operator: operator.clone(),
                        });
                        work.push(Frame::Visit {
                            node: node.children[1].as_ref(),
                            tag: right_tag,
                        });
                        work.push(Frame::Visit {
                            node: node.children[0].as_ref(),
                            tag: left_tag,
                        });
                    }
                    NodeType::Multiple => {
                        let n = node.children.len();
                        work.push(Frame::FinalizeMultiple { n });
                        for child in node.children.iter().rev() {
                            work.push(Frame::Visit {
                                node: child.as_ref(),
                                tag: tag.clone(),
                            });
                        }
                    }
                },
                Frame::FinalizeSingle { node, new_tag, n } => {
                    let start = results.len() - n;
                    // Children were drained in source order into `results`.
                    // Move them out into a fresh Vec; no clone, no JSON
                    // round-trip — this is what makes construction O(N)
                    // instead of O(N²).
                    let child_segments: Vec<IndexMap<String, ExtendedSegment>> =
                        results.drain(start..).collect();
                    let mut extended = IndexMap::new();
                    extended.insert(
                        new_tag,
                        ExtendedSegment {
                            segment: node.value.clone(),
                            attributes: node.get_attributes(),
                            inner: ExtendedSegmentInner::WithChildren(child_segments),
                        },
                    );
                    results.push(extended);
                }
                Frame::FinalizeBinary { tag, operator } => {
                    let right = results.pop().expect("binary: right missing");
                    let left = results.pop().expect("binary: left missing");
                    let mut extended = IndexMap::new();
                    extended.insert(
                        tag,
                        ExtendedSegment {
                            segment: String::new(),
                            attributes: Vec::new(),
                            inner: ExtendedSegmentInner::Binary {
                                left: Box::new(left),
                                right: Box::new(right),
                                operator,
                            },
                        },
                    );
                    results.push(extended);
                }
                Frame::FinalizeMultiple { n } => {
                    let start = results.len() - n;
                    let mut merged: IndexMap<String, ExtendedSegment> = IndexMap::new();
                    for child in results.drain(start..) {
                        merged.extend(child);
                    }
                    results.push(merged);
                }
            }
        }

        debug_assert_eq!(results.len(), 1);
        results.pop().unwrap()
    }

    /// Inverse of `extract_extended_segments`: walk the tree and rebuild the
    /// original text by plugging segments looked up from `extended_segments`.
    /// Iterative trampoline matching the recursive shape — each frame carries
    /// its `tag` and `level`, Single nodes emit a `(result, key)` pair, all
    /// others emit `(result, "")`.
    pub fn rebuild_from_segments(
        &self,
        level: usize,
        tag: &str,
        tag_list: &mut Vec<String>,
        extended_segments: &IndexMap<String, ExtendedSegment>,
    ) -> (String, String) {
        enum Frame<'a> {
            Visit {
                node: &'a TreeNode,
                tag: String,
                level: usize,
            },
            FinalizeSingle {
                open: String,
                close: Option<String>,
                key: String,
                n: usize,
            },
            FinalizeBinary,
            FinalizeMultiple { n: usize },
        }

        let mut work: Vec<Frame> = vec![Frame::Visit {
            node: self,
            tag: tag.to_string(),
            level,
        }];
        let mut results: Vec<(String, String)> = Vec::new();

        while let Some(frame) = work.pop() {
            match frame {
                Frame::Visit { node, tag, level } => match &node.node_type {
                    NodeType::Translatable | NodeType::Dummy => {
                        if tag_list.contains(&tag) {
                            results.push((String::new(), String::new()));
                        } else {
                            tag_list.push(tag.clone());
                            // `extended_segments` is an `ExtendedSegment` map
                            // now — read the field directly instead of
                            // poking through a `serde_json::Value`.
                            let segment = extended_segments
                                .get(&tag)
                                .map(|es| es.segment.clone())
                                .unwrap_or_default();
                            results.push((segment, String::new()));
                        }
                    }
                    NodeType::Single { open, close } => {
                        let trimmed = node.value.trim().to_string();
                        let mut index = 0usize;
                        while tag_list.contains(&format!("{}-{}", trimmed, index)) {
                            index += 1;
                        }
                        tag_list.push(format!("{}-{}", trimmed, index));
                        let new_tag = format!("{}/{}-{}", tag, trimmed, index);
                        let key = format!("{}-{}", trimmed, index);

                        let n = node.children.len();
                        work.push(Frame::FinalizeSingle {
                            open: open.clone(),
                            close: close.clone(),
                            key,
                            n,
                        });
                        for child in node.children.iter().rev() {
                            work.push(Frame::Visit {
                                node: child.as_ref(),
                                tag: new_tag.clone(),
                                level,
                            });
                        }
                    }
                    NodeType::Binary { .. } | NodeType::BinaryTemplate { .. } => {
                        work.push(Frame::FinalizeBinary);
                        let left_tag = format!("{}_L", tag);
                        let right_tag = format!("{}_R", tag);
                        work.push(Frame::Visit {
                            node: node.children[1].as_ref(),
                            tag: right_tag,
                            level: level + 1,
                        });
                        work.push(Frame::Visit {
                            node: node.children[0].as_ref(),
                            tag: left_tag,
                            level: level + 1,
                        });
                    }
                    NodeType::Multiple => {
                        let n = node.children.len();
                        work.push(Frame::FinalizeMultiple { n });
                        for child in node.children.iter().rev() {
                            work.push(Frame::Visit {
                                node: child.as_ref(),
                                tag: tag.clone(),
                                level,
                            });
                        }
                    }
                },
                Frame::FinalizeSingle { open, close, key, n } => {
                    let start = results.len() - n;
                    let mut result = open;
                    // Children drained in source order.
                    let child_results: Vec<(String, String)> =
                        results.drain(start..).collect();
                    for (rebuilt_result, rebuilt_key) in child_results {
                        if !rebuilt_key.is_empty() {
                            // Child was a Single: substitute its parmname placeholder.
                            let placeholder = format!(
                                "<parmname parm=\"{}\"/>",
                                rebuilt_key.replace('-', "\" n=\"")
                            );
                            result = result.replace(&placeholder, &rebuilt_result);
                        } else {
                            result.push_str(&rebuilt_result);
                        }
                    }
                    if let Some(close) = close {
                        result.push_str(&close);
                    }
                    results.push((result, key));
                }
                Frame::FinalizeBinary => {
                    let right = results.pop().expect("binary right missing");
                    let left = results.pop().expect("binary left missing");
                    let mut result = left.0;
                    result.push_str(&right.0);
                    results.push((result, String::new()));
                }
                Frame::FinalizeMultiple { n } => {
                    let start = results.len() - n;
                    let mut result = String::new();
                    for (child_result, _) in results.drain(start..) {
                        result.push_str(&child_result);
                    }
                    results.push((result, String::new()));
                }
            }
        }

        debug_assert_eq!(results.len(), 1);
        results.pop().unwrap()
    }

    pub fn generate_dot_representation(&self) -> String {
        let mut dot_content = String::new();
        dot_content.push_str("digraph G {\n");
        dot_content.push_str("    rankdir = TB\n");
        dot_content.push_str("    fontname = \"DejaVu\"\n");
        dot_content.push_str("    fontsize = 10\n\n");
        dot_content.push_str("    node [\n");
        dot_content.push_str("        fontname = \"DejaVu\"\n");
        dot_content.push_str("        fontsize = 10\n");
        dot_content.push_str("        shape = \"record\"\n");
        dot_content.push_str("    ]\n\n");
        dot_content.push_str("    edge [\n");
        dot_content.push_str("        fontname = \"DejaVu\"\n");
        dot_content.push_str("        fontsize = 10\n");
        dot_content.push_str("    ]\n\n");

        // Iterative post-order walk. We carry `&TreeNode` directly (the
        // whole walk lives under the outer `&self` borrow, so children
        // accessed via `child.as_ref()` are safely reborrowed from the
        // same lifetime — no unsafe needed). Identity for the
        // node_counter map is via the node's address as `*const TreeNode`
        // used only as a map key; we never dereference it.
        let mut stack: Vec<(&TreeNode, Option<String>)> = vec![(self, None)];
        let mut node_details = vec![];
        let mut edges = vec![];
        let mut node_counter: usize = 0;
        let mut node_id_map: std::collections::HashMap<*const TreeNode, String> =
            std::collections::HashMap::new();

        while let Some((current_node, parent_node)) = stack.pop() {
            let current_node_ptr = current_node as *const TreeNode;

            if !node_id_map.contains_key(&current_node_ptr) {
                node_id_map.insert(current_node_ptr, format!("node{}", node_counter));
                node_counter += 1;
            }

            let node_id = node_id_map.get(&current_node_ptr).unwrap();
            let mut node_label = format!("    \"{}\" [label=\"{{<f0> node_type= {}|", node_id, current_node.node_type_tag());

            if current_node.value.is_empty() {
                node_label += "**EMPTY**";
            } else {
                node_label += &current_node.value.replace('"', "\\\"")
                                                .replace('[', "\\[")
                                                .replace('<', "\\<")
                                                .replace('>', "\\>")
                                                .replace(']', "\\]")
                                                .replace('/', "\\/")
                                                .replace("'", "\\'")
                                                .replace('\n', " ");
            }

            if !current_node.children.is_empty() {
                node_label += "|";
                if matches!(current_node.node_type, NodeType::Single { .. }) && !current_node.attributes.is_empty() {
                    for attribute in &current_node.attributes {
                        // Attribute keys normally arrive as `"tag/attrname"`
                        // (e.g., `"p/class"`). Fall back to the full key if a
                        // caller constructed a TreeNode with an un-prefixed
                        // attribute name — otherwise DOT rendering would
                        // panic on that input.
                        let attr_name = attribute.0.split('/').nth(1).unwrap_or(&attribute.0);
                        node_label += &format!(
                            "{}=\\\"{}\\\" |",
                            attr_name,
                            attribute.1.replace('"', "\\\"")
                                       .replace('[', "\\[")
                                       .replace('<', "\\<")
                                       .replace('>', "\\>")
                                       .replace(']', "\\]")
                                       .replace('/', "\\/")
                                       .replace("'", "\\'")
                                       .replace('\n', " ")
                        );
                    }
                }
                let placeholders: Vec<_> = (1..=current_node.children.len()).map(|i| format!("<f{}>", i)).collect();
                node_label += &format!(" {{ {} }}}}\",", placeholders.join(" | "));
            } else {
                node_label += "}\",";
            }

            if matches!(current_node.node_type, NodeType::Translatable | NodeType::Dummy) {
                node_label += " fontcolor=blue, shape=Mrecord";
            } else {
                node_label += " fontcolor=red";
            }
            node_label += "];\n";
            node_details.push(node_label);

            if let Some(parent_id) = parent_node {
                edges.push(format!("    {} -> \"{}\":f0 ;\n", parent_id, node_id));
            }

            // Need to clone `node_id` because it borrows from `node_id_map`
            // and we're about to iterate + push into `stack` which
            // borrows it transitively. Each push gets its own owned
            // parent-id string.
            let parent_ref = format!("\"{}\"", node_id);
            for (i, child) in current_node.children.iter().enumerate().rev() {
                stack.push((
                    child.as_ref(),
                    Some(format!("{}:f{}", parent_ref, i + 1)),
                ));
            }
        }

        dot_content += &node_details.concat();
        dot_content += &edges.concat();
        dot_content += "}\n";
        dot_content
    }

    pub fn generate_svg(&self) -> String {
        use xmlwriter::XmlWriter;
        // Use double-quoted attributes; this is the conventional SVG
        // serialization and what the test suite greps for.
        let mut writer = XmlWriter::new(xmlwriter::Options {
            use_single_quote: false,
            ..Default::default()
        });

        writer.start_element("svg");
        writer.write_attribute("xmlns", "http://www.w3.org/2000/svg");
        writer.write_attribute("version", "1.1");

        let (max_width, max_height) = self.calculate_node_dimensions();
        let width = max_width + 2 * PADDING;
        let height = max_height + 2 * PADDING;

        writer.write_attribute("width", &width.to_string());
        writer.write_attribute("height", &height.to_string());
        writer.write_attribute("viewBox", &format!("0 0 {} {}", width, height));

        // Node IDs restart at 0 per `generate_svg` call. The counter is
        // local to this call so back-to-back renderings of the same tree
        // produce identical output instead of drifting with a global
        // counter.
        let mut node_id_counter: usize = 0;
        self.generate_svg_detail(&mut writer, width / 2, PADDING, &mut node_id_counter);

        writer.end_element();
        writer.end_document()
    }

    /// Emit the SVG body for this subtree. Iterative walk: each node
    /// emits its own `<g id="nodeN"><rect/><text/>...</g>` group as a
    /// direct child of the root `<svg>`, then enqueues for each child a
    /// `<line>` connector (also at the svg level) and a recursive
    /// `Visit`.
    ///
    /// Child groups are siblings of their parent group, not descendants.
    /// Nesting them would mean any `transform` on a parent would cascade
    /// to its children — which is not what this visualization wants.
    /// So every `<g>` / `<line>` is appended to the single root `<svg>`
    /// element regardless of tree depth.
    ///
    /// `node_id_counter` is borrowed from the caller so every new
    /// `generate_svg()` call owns its own counter and the IDs always
    /// start at 0.
    fn generate_svg_detail(
        &self,
        writer: &mut XmlWriter,
        x: usize,
        y: usize,
        node_id_counter: &mut usize,
    ) -> (usize, usize) {
        struct Frame<'a> {
            node: &'a TreeNode,
            x: usize,
            y: usize,
        }

        // DFS walk, but everything is emitted flat into the root <svg>.
        // Use a plain stack so depth is bounded by heap, not the test
        // thread stack.
        let mut work: Vec<Frame> = vec![Frame { node: self, x, y }];

        while let Some(Frame { node, x, y }) = work.pop() {
            let node_id = *node_id_counter;
            *node_id_counter += 1;
            let fill_color = node.get_fill_color();

            let content = if node.value.is_empty() {
                "EMPTY".to_string()
            } else {
                node.value.clone()
            };
            let attributes: Vec<_> = node
                .attributes
                .iter()
                .map(|attr| format!("{}: {}", attr.0, attr.1))
                .collect();
            let (box_width, box_height, lines) =
                node.calculate_text_box_dimensions(&content, &attributes);

            // Emit this node's own group. Because we close `<g>` before
            // handling any children (below), child groups become
            // siblings, not descendants.
            writer.start_element("g");
            writer.write_attribute("id", &format!("node{}", node_id));

            writer.start_element("rect");
            writer.write_attribute("x", &(x.saturating_sub(box_width / 2)).to_string());
            writer.write_attribute("y", &y.to_string());
            writer.write_attribute("width", &box_width.to_string());
            writer.write_attribute("height", &box_height.to_string());
            writer.write_attribute("fill", fill_color);
            writer.write_attribute("stroke", "black");
            writer.end_element();

            let node_type_tag = node.node_type_tag();
            let centered_node_type_x =
                x.saturating_sub(node_type_tag.len() * CHAR_WIDTH / 2);
            let type_text_y = y + PADDING + CHAR_HEIGHT;

            // `set_preserve_whitespaces(true)` keeps the text node
            // inline with its element (`<text ...>SINGLE</text>`)
            // instead of re-indenting onto its own line. xmlwriter
            // auto-flips the flag back when the element closes.
            writer.set_preserve_whitespaces(true);
            writer.start_element("text");
            writer.write_attribute("x", &centered_node_type_x.to_string());
            writer.write_attribute("y", &type_text_y.to_string());
            writer.write_attribute("fill", "black");
            writer.write_text(node_type_tag);
            writer.end_element();
            writer.set_preserve_whitespaces(false);

            for (i, line) in lines.iter().enumerate() {
                let text_y = y + PADDING + CHAR_HEIGHT * 2 + (i * CHAR_HEIGHT);
                writer.set_preserve_whitespaces(true);
                writer.start_element("text");
                writer.write_attribute(
                    "x",
                    &(x.saturating_sub(box_width / 2) + PADDING).to_string(),
                );
                writer.write_attribute("y", &text_y.to_string());
                writer.write_attribute("fill", "blue");
                writer.write_text(line);
                writer.end_element();
                writer.set_preserve_whitespaces(false);
            }

            for (j, attr) in attributes.iter().enumerate() {
                let attr_y = y
                    + PADDING
                    + CHAR_HEIGHT * 2
                    + (lines.len() + j) * CHAR_HEIGHT;
                writer.set_preserve_whitespaces(true);
                writer.start_element("text");
                writer.write_attribute(
                    "x",
                    &(x.saturating_sub(box_width / 2) + PADDING).to_string(),
                );
                writer.write_attribute("y", &attr_y.to_string());
                writer.write_attribute("fill", "red");
                writer.write_text(attr);
                writer.end_element();
                writer.set_preserve_whitespaces(false);
            }

            // Close the node's own <g> BEFORE emitting connector lines
            // and child groups — that's what makes children siblings.
            writer.end_element();

            // Now lay out children, emit each connector line at the
            // svg level, and push child frames so the recursive walk
            // keeps going.
            let child_y = y + box_height + CHAR_HEIGHT * 2;
            let subtree_widths: Vec<_> = node
                .children
                .iter()
                .map(|child| child.calculate_node_dimensions().0)
                .collect();
            let total_width = subtree_widths.iter().sum::<usize>()
                + (subtree_widths.len().saturating_sub(1)) * 20;
            let mut start_x = x.saturating_sub(total_width / 2);

            let mut child_frames: Vec<(usize, usize)> =
                Vec::with_capacity(node.children.len());
            for i in 0..node.children.len() {
                let child_x = start_x + subtree_widths[i] / 2;
                child_frames.push((child_x, child_y));
                start_x += subtree_widths[i] + 20;
            }

            // Emit all connector lines in source order first. They're
            // siblings of every other <g>/<line>, so emission order
            // only matters for deterministic rendering.
            for (cx, cy) in &child_frames {
                writer.start_element("line");
                writer.write_attribute("x1", &x.to_string());
                writer.write_attribute("y1", &(y + box_height).to_string());
                writer.write_attribute("x2", &cx.to_string());
                writer.write_attribute("y2", &cy.to_string());
                writer.write_attribute("stroke", "black");
                writer.end_element();
            }

            // Push child frames in reverse so the leftmost child runs
            // first after popping, preserving left-to-right id order.
            for (i, (cx, cy)) in child_frames.into_iter().enumerate().rev() {
                work.push(Frame {
                    node: node.children[i].as_ref(),
                    x: cx,
                    y: cy,
                });
            }
        }

        (x, y)
    }

    fn calculate_text_box_dimensions(&self, text: &str, attributes: &[String]) -> (usize, usize, Vec<String>) {
        let wrapped_text = textwrap::wrap(text, 30).join("\n");
        let lines: Vec<_> = wrapped_text.split('\n').map(String::from).collect();
        let num_lines = lines.len() + 1 + attributes.len();
        let max_line_length = lines.iter().map(|line| line.len()).max().unwrap_or(0).max(self.node_type_tag().len()).max(attributes.iter().map(|attr| attr.len()).max().unwrap_or(0));
        let box_width = max_line_length * CHAR_WIDTH + 2 * PADDING;
        let box_height = num_lines * CHAR_HEIGHT + 2 * PADDING;
        (box_width, box_height, lines)
    }

    /// Compute the SVG bounding box for this subtree. Iterative post-order
    /// walk with a parallel `Vec<(usize, usize)>` of child dimensions.
    fn calculate_node_dimensions(&self) -> (usize, usize) {
        enum Frame<'a> {
            Visit(&'a TreeNode),
            Finalize(&'a TreeNode, usize),
        }
        let vertical_spacing = CHAR_HEIGHT * 2;

        let mut work: Vec<Frame> = vec![Frame::Visit(self)];
        let mut results: Vec<(usize, usize)> = Vec::new();

        while let Some(frame) = work.pop() {
            match frame {
                Frame::Visit(node) => {
                    let n = node.children.len();
                    work.push(Frame::Finalize(node, n));
                    for child in node.children.iter().rev() {
                        work.push(Frame::Visit(child.as_ref()));
                    }
                }
                Frame::Finalize(node, n) => {
                    let content = if node.value.is_empty() {
                        "EMPTY".to_string()
                    } else {
                        node.value.clone()
                    };
                    let attributes: Vec<_> = node
                        .attributes
                        .iter()
                        .map(|attr| format!("{}: {}", attr.0, attr.1))
                        .collect();
                    let (box_width, box_height, _) =
                        node.calculate_text_box_dimensions(&content, &attributes);

                    let mut current_height = box_height;
                    let mut total_child_width = 0usize;
                    let child_y = current_height + vertical_spacing;

                    let start = results.len() - n;
                    let child_dims: Vec<(usize, usize)> = results.drain(start..).collect();
                    for (cw, ch) in &child_dims {
                        total_child_width += cw;
                        current_height = current_height.max(child_y + ch);
                    }
                    if n > 0 {
                        total_child_width += (n - 1) * 20;
                    }

                    let max_width = box_width.max(total_child_width);
                    results.push((max_width, current_height));
                }
            }
        }

        debug_assert_eq!(results.len(), 1);
        results.pop().unwrap()
    }

    fn get_fill_color(&self) -> &str {
        match &self.node_type {
            NodeType::Translatable => "white",
            NodeType::Single { .. } => "lightblue",
            NodeType::Binary { .. } => "lightgreen",
            NodeType::Multiple => "lightyellow",
            NodeType::BinaryTemplate { .. } => "lightcoral",
            NodeType::Dummy => "white",
        }
    }
}

/// Outcome of `SegmentTree::split_token_list`. Mirrors the Python
/// `_split_token_list` return shape (position, symbol, sublists)
/// collapsed into a single typed enum:
///
///   * `Binary`   — a top-level `BREAK` / `BREAK_TEMPLATE` was found at
///                  tag-stack depth 0. The two halves go to child
///                  `build_node` expansions and the parent becomes a
///                  `Binary` / `BinaryTemplate` node carrying the
///                  operator text.
///   * `Multiple` — no top-level break. Each sublist is paired with an
///                  `is_translatable` flag — `true` when the sublist
///                  was emitted with an empty tag stack (Python's
///                  `'TRANSLATABLE'` label), `false` when it was
///                  emitted inside a wrapping tag (`'SINGLE'`). The
///                  flag is the ONLY thing that lets the splitter
///                  recognise a standalone-content sublist (e.g.
///                  `&#169; 2024 Footer` or `¶` inside `<a>¶</a>`)
///                  as translatable — the tokens themselves carry
///                  classes like SYMBOL / PUNCTUATION / BREAK, none of
///                  which are structural.
///   * `Dummy`    — input was empty (no tokens to split).
#[derive(Debug)]
enum SplitResult {
    Binary {
        operator: String,
        is_template: bool,
        left: Vec<IndexMap<String, Value>>,
        right: Vec<IndexMap<String, Value>>,
    },
    Multiple(Vec<(bool, Vec<IndexMap<String, Value>>)>),
    Dummy,
}

#[derive(Debug)]
pub struct SegmentTree {
    pub tree: TreeNode,
    pub segments: IndexMap<String, String>,
}

impl SegmentTree {
    pub fn new_from_tree(tree: TreeNode) -> SegmentTree {
        let segments = tree.extract_segments().into_iter().map(|seg| (seg.keys().cloned().next().unwrap(), seg.values().cloned().next().unwrap())).collect();
        SegmentTree {
            tree,
            segments,
        }
    }

    /// Build a `SegmentTree` from a token list. Validates every token
    /// up-front (see `validate_tokens`) so that internal walks in
    /// `build_node` can treat field accesses as structural invariants.
    ///
    /// On malformed input returns
    /// `TexplitterError::InvalidToken { index, reason }` rather than
    /// panicking. Callers coming from `Tokenizer::tokenize` never hit
    /// the error path — the tokenizer already produces well-shaped
    /// tokens — but external consumers (Python binding, test harnesses)
    /// that hand-build token maps get a catchable error instead of a
    /// panic.
    pub fn new(tokens: Vec<IndexMap<String, Value>>) -> Result<SegmentTree, TexplitterError> {
        validate_tokens(&tokens)?;
        let tree = SegmentTree::build_node(&tokens);
        Ok(SegmentTree {
            tree,
            segments: IndexMap::new(), // Assuming you'll populate this later
        })
    }

    /// Iterative construction of a TreeNode from a token slice. The old
    /// recursive version walked once per nesting level and the 1000-level
    /// stress test blew the test-thread stack. This version uses an explicit
    /// work stack so depth is bounded only by heap.
    ///
    /// Two frame kinds go on the work stack:
    ///   * `Expand(tokens)`     — still-to-be-analyzed input. Pops to produce
    ///                            either a finished leaf or a `Pending*` frame.
    ///   * `PendingBinary`      — binary/binary-template parent waiting for
    ///                            its 2 children. Left and right are pushed
    ///                            back onto the Expand queue before the
    ///                            Pending frame so they resolve first.
    ///   * `PendingMultiple`    — MULTIPLE parent: for each sublist it
    ///                            precomputes a child "slot" that is either an
    ///                            already-built leaf or a `Single` wrapper
    ///                            whose one child is still to be expanded.
    ///
    /// Finished subtrees accumulate on a parallel `results` stack. When a
    /// Pending* frame fires it pops the exact number of results its kind
    /// expects and wraps them into its parent TreeNode.
    fn build_node(initial_tokens: &[IndexMap<String, Value>]) -> TreeNode {
        // A slot that a MULTIPLE parent expects. Either already built
        // (Dummy/Translatable leaves) or a Single wrapper whose body is still
        // waiting on the work stack.
        enum MultipleSlot {
            Done(TreeNode),
            SingleWrapping {
                open: String,
                close: String,
                text_tag: String,
                attributes: Vec<(String, String)>,
            },
        }

        enum Frame {
            Expand(Vec<IndexMap<String, Value>>),
            PendingBinary {
                operator: String,
                is_template: bool,
            },
            PendingMultiple {
                slots: Vec<MultipleSlot>,
            },
        }


        let mut work: Vec<Frame> = vec![Frame::Expand(initial_tokens.to_vec())];
        let mut results: Vec<TreeNode> = Vec::new();

        // Pre-scan the raw token list to assign attribute keys in
        // source order (left-to-right, which for a well-formed document
        // is the same as a DFS walk over the tree-to-be).
        //
        // The iterative trampoline below processes all same-level
        // sublists before descending, so allocating registry keys
        // inline during the build walk would interleave them in a
        // different order. Instead: one linear scan over the flat
        // token list up front, indexed by each BEGINNING_TAG token's
        // `start` byte offset. The `analyze_token_segments` path then
        // just looks up the prebuilt `(open_tag, text_tag, attributes)`
        // tuple by `start` instead of recomputing anything.
        let mut attribute_registry: IndexMap<String, String> = IndexMap::new();
        let mut attribute_data: IndexMap<u64, (String, String, Vec<(String, String)>)> =
            IndexMap::new();
        SegmentTree::prescan_attribute_data(
            initial_tokens,
            &mut attribute_registry,
            &mut attribute_data,
        );

        while let Some(frame) = work.pop() {
            match frame {
                Frame::Expand(tokens) => {
                    if tokens.is_empty() {
                        results.push(TreeNode::new(NodeType::Dummy, "", vec![], vec![]));
                        continue;
                    }

                    let split = SegmentTree::split_token_list(&tokens);

                    match split {
                        SplitResult::Binary {
                            operator,
                            is_template,
                            left,
                            right,
                        } => {
                            // Children resolve in reverse-push order: we want
                            // left-result then right-result on top of `results`
                            // when the PendingBinary fires, which means push the
                            // Pending frame first, then right, then left.
                            work.push(Frame::PendingBinary {
                                operator,
                                is_template,
                            });
                            work.push(Frame::Expand(right));
                            work.push(Frame::Expand(left));
                        }
                        SplitResult::Dummy => {
                            results.push(TreeNode::new(NodeType::Dummy, "", vec![], vec![]));
                        }
                        SplitResult::Multiple(sublists) => {
                            // MULTIPLE branch — precompute the slot list and the
                            // list of in_segments that need recursive build_node
                            // expansion, then push PendingMultiple followed by
                            // the Expand frames in reverse so the first in_segment
                            // resolves first.
                            //
                            // Sublists come paired with an `is_translatable`
                            // flag computed by `split_token_list`. It is
                            // `true` when the sublist was emitted while the
                            // tag stack was empty (i.e. the content sits
                            // between structural tags, not inside a single
                            // wrapping tag). Python calls these
                            // 'TRANSLATABLE' vs 'SINGLE' sublists; we keep
                            // the same distinction because leaf-only content
                            // like `<a>¶</a>` or `&#169; 2024 Footer`
                            // otherwise falls through the cracks — the first
                            // token isn't a structural OPEN and there's no
                            // CLOSE to match against.
                            let mut slots: Vec<MultipleSlot> = Vec::new();
                            let mut pending_expands: Vec<Vec<IndexMap<String, Value>>> = Vec::new();

                            if !sublists.is_empty() {
                                if !sublists[0].0 {
                                    // Leading 'SINGLE' sublist: Python prepends a
                                    // DUMMY sibling so the multiple-child node
                                    // doesn't start with a wrapper.
                                    slots.push(MultipleSlot::Done(TreeNode::new(
                                        NodeType::Dummy,
                                        "",
                                        vec![],
                                        vec![],
                                    )));
                                }
                                for (is_translatable, sublist) in &sublists {
                                    if *is_translatable {
                                        // Pull the `text` field directly from each
                                        // token. Only `text` is needed here, and
                                        // reading it with a specific accessor
                                        // avoids choking on `start`/`end` fields
                                        // which are `Value::Number`, not strings.
                                        let text: String = sublist
                                            .iter()
                                            .map(|token| {
                                                token
                                                    .get("text")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("")
                                                    .to_string()
                                            })
                                            .collect::<Vec<String>>()
                                            .join("");
                                        slots.push(MultipleSlot::Done(TreeNode::new(
                                            NodeType::Translatable,
                                            &text,
                                            vec![],
                                            vec![],
                                        )));
                                    } else if sublist.last().map_or(false, |token| {
                                        matches!(
                                            TokenClass::of(token),
                                            TokenClass::CloseTag
                                                | TokenClass::CloseBracket
                                                | TokenClass::EndCloseTag
                                        )
                                    }) {
                                        let (open_tag, close_tag, text_tag, in_segment, attributes) =
                                            SegmentTree::analyze_token_segments(sublist, &attribute_data);
                                        slots.push(MultipleSlot::SingleWrapping {
                                            open: open_tag,
                                            close: close_tag,
                                            text_tag,
                                            attributes,
                                        });
                                        pending_expands.push(in_segment);
                                    }
                                }
                                if !sublists.last().map(|s| s.0).unwrap_or(true) {
                                    // Trailing 'SINGLE' sublist: same
                                    // treatment — append a DUMMY so the
                                    // MULTIPLE node doesn't end on a wrapper.
                                    slots.push(MultipleSlot::Done(TreeNode::new(
                                        NodeType::Dummy,
                                        "",
                                        vec![],
                                        vec![],
                                    )));
                                }
                            } else {
                                slots.push(MultipleSlot::Done(TreeNode::new(
                                    NodeType::Dummy,
                                    "",
                                    vec![],
                                    vec![],
                                )));
                            }

                            // Push Pending frame first (resolves last), then
                            // Expand frames in REVERSE of source order so the
                            // first in_segment sits on top and resolves first.
                            work.push(Frame::PendingMultiple { slots });
                            for segment in pending_expands.into_iter().rev() {
                                work.push(Frame::Expand(segment));
                            }
                        }
                    }
                }
                Frame::PendingBinary { operator, is_template } => {
                    // Push order was: Pending, Right, Left -> Left pops and
                    // resolves first, Right second. So results-top is the
                    // right child, next is the left child.
                    let right = results.pop().expect("pending binary: right missing");
                    let left = results.pop().expect("pending binary: left missing");
                    let node_type = if is_template {
                        NodeType::BinaryTemplate { operator: operator.clone() }
                    } else {
                        NodeType::Binary { operator: operator.clone() }
                    };
                    results.push(TreeNode::new(
                        node_type,
                        &operator,
                        vec![Box::new(left), Box::new(right)],
                        vec![],
                    ));
                }
                Frame::PendingMultiple { slots } => {
                    // Count how many results belong to this frame. They come
                    // from the Single wrappers only; Done slots carry their
                    // child directly.
                    let n_results = slots
                        .iter()
                        .filter(|s| matches!(s, MultipleSlot::SingleWrapping { .. }))
                        .count();
                    // Expand frames for Single-slot bodies were pushed in
                    // source order after PendingMultiple; the first one
                    // (leftmost slot) pops first and deposits result_1 at
                    // the bottom of this frame's slice. So results[top]
                    // is the LAST Single slot's body — pop N times, then
                    // reverse to get [result_1, result_2, ..., result_N].
                    let mut inner_results: Vec<TreeNode> = Vec::with_capacity(n_results);
                    for _ in 0..n_results {
                        inner_results.push(results.pop().expect("pending multiple: inner missing"));
                    }
                    inner_results.reverse();
                    let mut inner_iter = inner_results.into_iter();

                    let mut children: Vec<Box<TreeNode>> = Vec::with_capacity(slots.len());
                    for slot in slots {
                        match slot {
                            MultipleSlot::Done(node) => children.push(Box::new(node)),
                            MultipleSlot::SingleWrapping {
                                open,
                                close,
                                text_tag,
                                attributes,
                            } => {
                                let body = inner_iter.next().expect("inner body missing");
                                children.push(Box::new(TreeNode::new(
                                    NodeType::Single {
                                        open,
                                        close: Some(close),
                                    },
                                    &text_tag,
                                    vec![Box::new(body)],
                                    attributes,
                                )));
                            }
                        }
                    }
                    results.push(TreeNode::new(NodeType::Multiple, "", children, vec![]));
                }
            }
        }

        debug_assert_eq!(results.len(), 1, "build_node finished with {} results", results.len());
        results.pop().expect("build_node produced no result")
    }

    /// Port of Python `_split_token_list`. Makes two passes:
    ///
    ///   1. Scan for a top-level `BREAK` / `BREAK_TEMPLATE` token (one
    ///      whose tag stack is empty at the time we see it). If found,
    ///      return `SplitResult::Binary` with the tokens before and
    ///      after the break — the parent becomes a `Binary` /
    ///      `BinaryTemplate` TreeNode.
    ///   2. Otherwise walk the tokens grouping into sublists. Each
    ///      sublist carries an `is_translatable` flag: `true` if the
    ///      tag stack was empty at the moment the sublist started
    ///      (Python's `'TRANSLATABLE'`), `false` otherwise
    ///      (`'SINGLE'`). The `balanced` flag starts `true` and flips
    ///      whenever a push/pop would change its parity, at which
    ///      point the in-progress sublist is emitted with the label it
    ///      inherited when it began.
    fn split_token_list(tokens: &[IndexMap<String, Value>]) -> SplitResult {
        // Pass 1: look for a top-level BREAK / BREAK_TEMPLATE.
        // `TokenClass` lumps BREAK into `Other` on purpose (the splitter
        // normally cares about structure, not lexical class), so we
        // inspect the raw `token_class` string here instead of going
        // through the enum.
        let mut tag_stack: Vec<String> = Vec::new();
        for (pos, token) in tokens.iter().enumerate() {
            let raw_class = token["token_class"].as_str().unwrap();
            let is_break = raw_class == "BREAK";
            let is_break_template = raw_class == "BREAK_TEMPLATE";
            if (is_break || is_break_template) && tag_stack.is_empty() {
                let operator = token["text"].as_str().unwrap().to_string();
                let left = tokens[..pos].to_vec();
                let right = tokens[pos + 1..].to_vec();
                return SplitResult::Binary {
                    operator,
                    is_template: is_break_template,
                    left,
                    right,
                };
            }
            let class = TokenClass::of(token);
            match class {
                TokenClass::OpenBracket
                | TokenClass::OpenTag
                | TokenClass::BeginningTag => {
                    tag_stack.push(token["label"].as_str().unwrap().to_string());
                }
                TokenClass::CloseBracket | TokenClass::CloseTag => {
                    tag_stack.pop();
                }
                TokenClass::EndCloseTag => {
                    tag_stack.pop();
                }
                TokenClass::SingleTag => {
                    // Push and immediately pop — self-closing structural element
                    tag_stack.push(token["label"].as_str().unwrap().to_string());
                    tag_stack.pop();
                }
                _ => {}
            }
        }

        // Pass 2: group into sublists, each tagged 'TRANSLATABLE' or
        // 'SINGLE' based on the `balanced` state at emission time.
        let mut tag_stack: Vec<String> = Vec::new();
        let mut sublists: Vec<(bool, Vec<IndexMap<String, Value>>)> = Vec::new();
        let mut current_sublist: Vec<IndexMap<String, Value>> = Vec::new();
        let mut balanced = true;
        let mut last_action = "";

        for token in tokens {
            let class = TokenClass::of(token);
            if matches!(
                class,
                TokenClass::OpenBracket | TokenClass::OpenTag | TokenClass::BeginningTag
            ) {
                // Structural invariant: `validate_tokens` (called at
                // `SegmentTree::new`) has proven every token has
                // `label` as Value::String.
                tag_stack.push(token["label"].as_str().unwrap().to_string());
                last_action = "push";
            }
            if matches!(class, TokenClass::CloseBracket | TokenClass::CloseTag) {
                tag_stack.pop();
                last_action = "pop";
            }
            if class == TokenClass::EndCloseTag {
                tag_stack.pop();
                last_action = "popRC";
            }
            if class == TokenClass::SingleTag {
                // Self-closing: push + pop in one step.
                tag_stack.push(token["label"].as_str().unwrap().to_string());
                tag_stack.pop();
                last_action = "pop";
            }

            if balanced == tag_stack.is_empty() {
                current_sublist.push(token.clone());
            } else {
                if !current_sublist.is_empty() {
                    let label = balanced; // TRANSLATABLE if balanced at start
                    if last_action == "pop" || last_action == "popRC" {
                        current_sublist.push(token.clone());
                    }
                    sublists.push((label, std::mem::take(&mut current_sublist)));
                    if last_action == "push" {
                        current_sublist.push(token.clone());
                    }
                    balanced = tag_stack.is_empty();
                } else {
                    current_sublist.push(token.clone());
                    balanced = tag_stack.is_empty();
                }
            }
        }

        // Tail: Python's rule is `translatable = 'TRANSLATABLE' if
        // (balanced and last_action != 'popRC') else 'SINGLE'`, but it
        // only emits the sublist if `current_sublist` is non-empty.
        if !current_sublist.is_empty() {
            let label = balanced && last_action != "popRC";
            sublists.push((label, current_sublist));
        }

        if sublists.is_empty() {
            SplitResult::Dummy
        } else {
            SplitResult::Multiple(sublists)
        }
    }

    /// Analyze the token slice for a single segment. Returns the 5-tuple
    /// the `build_node::Expand` branch consumes:
    ///
    ///   (open_tag, close_tag, text_tag, in_segment_tokens, attributes)
    ///
    /// * For a plain `OPEN_TAG`/`OPEN_BRACKET` wrapper the open/close are
    ///   the first/last tokens' raw text, `text_tag` strips the angle
    ///   brackets, and `in_segment` is everything in between.
    /// * For a `BEGINNING_TAG` wrapper we look up the prebuilt
    ///   `(open_tag_with_placeholders, text_tag, attributes)` entry the
    ///   `prescan_attribute_data` pre-pass computed for this BEGINNING_TAG,
    ///   then determine the body range from the `END_CLOSE_TAG`
    ///   (self-closing) / `END_OPEN_TAG` (body follows, closing tag is
    ///   the final token) terminator.
    fn analyze_token_segments(
        tokens: &[IndexMap<String, Value>],
        attribute_data: &IndexMap<u64, (String, String, Vec<(String, String)>)>,
    ) -> (String, String, String, Vec<IndexMap<String, Value>>, Vec<(String, String)>) {
        let first_class = TokenClass::of(&tokens[0]);
        if matches!(first_class, TokenClass::OpenBracket | TokenClass::OpenTag) {
            // Structural invariants in this branch:
            //   - `tokens` is non-empty (caller never passes an empty
            //     slice; `split_token_list` only yields non-empty
            //     sublists, and the entry point returns early in
            //     `build_node::Expand` when the slice is empty).
            //   - Every token has `text` as `Value::String` (proven
            //     by `validate_tokens` at `SegmentTree::new`).
            let open_tag = tokens[0]["text"].as_str().unwrap().to_string();
            let close_tag = tokens.last().unwrap()["text"].as_str().unwrap().to_string();
            let text_tag = tokens[0]["text"].as_str().unwrap().replace("<", "").replace(">", "");
            let in_segment = tokens[1..tokens.len() - 1].to_vec();
            let attributes = vec![];
            (open_tag, close_tag, text_tag, in_segment, attributes)
        } else if first_class == TokenClass::BeginningTag {
            // Find the index of the terminating END_OPEN_TAG / END_CLOSE_TAG.
            // Structurally, the tokenizer always emits one of these after
            // a BEGINNING_TAG (otherwise the attribute list is unterminated
            // and the tokenizer wouldn't have matched BEGINNING_TAG in the
            // first place). We still guard against runaway walks by falling
            // back to the last token if we never find one.
            let end_index = tokens
                .iter()
                .position(|t| {
                    let c = TokenClass::of(t);
                    c == TokenClass::EndCloseTag || c == TokenClass::EndOpenTag
                })
                .unwrap_or(tokens.len() - 1);

            // Pull the prebuilt placeholder-annotated open tag,
            // text_tag, and attributes list out of the registry
            // keyed by the BEGINNING_TAG token's `start` byte offset.
            let start_key = tokens[0]["start"].as_u64().unwrap_or(0);
            let (open_tag, text_tag, attributes) = attribute_data
                .get(&start_key)
                .cloned()
                .unwrap_or_else(|| {
                    // Should never happen: `prescan_attribute_data`
                    // walks every BEGINNING_TAG in the token slice.
                    // Fall back to raw text so a missed entry degrades
                    // gracefully instead of panicking.
                    let raw = tokens[0]["text"].as_str().unwrap_or("").to_string();
                    (raw.clone(), raw, vec![])
                });

            let end_class = TokenClass::of(&tokens[end_index]);
            if end_class == TokenClass::EndCloseTag {
                // Self-closing: no body, no close tag.
                (open_tag, String::new(), text_tag, vec![], attributes)
            } else {
                // END_OPEN_TAG: body follows through the final token,
                // which is the matching CLOSE_TAG/CLOSE_BRACKET.
                let close_tag = tokens.last().unwrap()["text"].as_str().unwrap().to_string();
                let in_segment = tokens[end_index + 1..tokens.len() - 1].to_vec();
                (open_tag, close_tag, text_tag, in_segment, attributes)
            }
        } else {
            let open_tag = "".to_string();
            let close_tag = "".to_string();
            let text_tag = "".to_string();
            let in_segment = vec![];
            let attributes = vec![];
            (open_tag, close_tag, text_tag, in_segment, attributes)
        }
    }

    /// Pre-scan the raw token list and assign attribute keys for every
    /// BEGINNING_TAG in left-to-right source order, populating
    /// `attribute_data` with the placeholder-annotated opening tag,
    /// text_tag, and attributes list keyed by the BEGINNING_TAG token's
    /// `start` byte offset.
    ///
    /// Keys are allocated in source order so that the template the
    /// splitter emits is stable for a given input. Running this as a
    /// pre-pass keeps the iterative `build_node` trampoline simple
    /// instead of interleaving analysis with body expansion.
    fn prescan_attribute_data(
        tokens: &[IndexMap<String, Value>],
        attribute_registry: &mut IndexMap<String, String>,
        attribute_data: &mut IndexMap<u64, (String, String, Vec<(String, String)>)>,
    ) {
        let mut i = 0;
        while i < tokens.len() {
            let class = TokenClass::of(&tokens[i]);
            if class == TokenClass::BeginningTag {
                // Find the END_*_TAG terminator for this attribute window.
                let mut j = i;
                while j < tokens.len() {
                    let c = TokenClass::of(&tokens[j]);
                    if c == TokenClass::EndCloseTag || c == TokenClass::EndOpenTag {
                        break;
                    }
                    j += 1;
                }
                let tag_slice = &tokens[i..=j.min(tokens.len() - 1)];
                let start_key = tokens[i]["start"].as_u64().unwrap_or(0);
                let (open_tag, text_tag, attrs) =
                    SegmentTree::extract_token_attributes(tag_slice, attribute_registry);
                attribute_data.insert(start_key, (open_tag, text_tag, attrs));
                i = j + 1;
            } else {
                i += 1;
            }
        }
    }

    /// Walk a BEGINNING_TAG + MIDDLE_TAG* + END_*_TAG slice and build
    /// the placeholder-annotated opening tag string plus an ordered
    /// attributes list.
    ///
    /// Returns `(open_tag, text_tag, attributes)` where:
    ///   * `open_tag` is the full opening tag with `@key@` placeholders
    ///     substituted for each attribute value, e.g.
    ///     `<html lang="@html/lang~0@">`.
    ///   * `text_tag` is the element name (e.g. `html`), used as the
    ///     namespace prefix for the attribute keys.
    ///   * `attributes` is the `Vec<(key, value)>` list in source order,
    ///     matching the tuple shape Python expects on `TreeNode.attributes`.
    fn extract_token_attributes(
        tokens: &[IndexMap<String, Value>],
        attribute_registry: &mut IndexMap<String, String>,
    ) -> (String, String, Vec<(String, String)>) {
        let mut tag = String::new();
        let mut text_tag = String::new();
        let mut attributes: Vec<(String, String)> = Vec::new();
        let mut value: Vec<String> = Vec::new();
        let mut inside_tag = false;
        let mut current_key = String::new();

        for token in tokens {
            let class = TokenClass::of(token);
            let text = token["text"].as_str().unwrap();
            let label = token["label"].as_str().unwrap();
            match class {
                TokenClass::BeginningTag | TokenClass::OpenBracket => {
                    text_tag = label.to_string();
                    tag.push_str(text);
                }
                TokenClass::MiddleTag => {
                    // Flush the previous attribute's accumulated value,
                    // if any, before starting the next one.
                    if inside_tag {
                        attributes.push((current_key.clone(), value.concat()));
                    }
                    value.clear();
                    let text_attribute = label;
                    tag.push_str(text);
                    current_key =
                        SegmentTree::generate_unique_key(&text_tag, text_attribute, attribute_registry);
                    attribute_registry.insert(current_key.clone(), String::new());
                    tag.push_str(&format!("@{}@", current_key));
                    inside_tag = true;
                }
                TokenClass::EndCloseTag | TokenClass::EndOpenTag | TokenClass::CloseBracket => {
                    tag.push_str(text);
                    break;
                }
                _ => {
                    value.push(text.to_string());
                }
            }
        }
        // Flush the final attribute's accumulated value if the loop ended
        // without a subsequent MIDDLE_TAG to flush it.
        if inside_tag {
            attributes.push((current_key.clone(), value.concat()));
        }
        (tag, text_tag, attributes)
    }

    /// Allocate a collision-free `{text_tag}/{text_attribute}~{index}`
    /// key against the shared registry. The registry is threaded down
    /// from `build_node` so its lifetime is exactly one tree construction
    /// — keys never leak across independent splits.
    fn generate_unique_key(
        text_tag: &str,
        text_attribute: &str,
        attribute_registry: &IndexMap<String, String>,
    ) -> String {
        let mut index = 0usize;
        loop {
            let candidate = format!("{}/{}~{}", text_tag, text_attribute, index);
            if !attribute_registry.contains_key(&candidate) {
                return candidate;
            }
            index += 1;
        }
    }
}

/// Iterative Drop impl. The default derived drop recurses through
/// `Vec<Box<TreeNode>>` one frame per level, so a deeply nested
/// `SegmentTree` (e.g. 2000 levels) would overflow the stack when it
/// goes out of scope. Dismantling children with an explicit work stack
/// keeps scope-exit stack-safe to match every other walk in this file.
impl Drop for TreeNode {
    fn drop(&mut self) {
        // Take our children out first so the derived recursive drop
        // doesn't run on them, then dismantle iteratively using a work
        // stack of owned subtrees.
        let mut stack: Vec<Box<TreeNode>> = std::mem::take(&mut self.children);
        while let Some(mut node) = stack.pop() {
            // Move this node's children onto the stack, emptying its vec so
            // the Box<TreeNode> we drop below has no descendants to recurse
            // through.
            stack.extend(std::mem::take(&mut node.children));
            // `node` goes out of scope here and runs `drop(TreeNode)`
            // again — but with `children` already empty, that drop is a
            // no-op at the recursion level.
        }
    }
}

impl fmt::Display for TreeNode {
    /// Pretty-print the subtree. Iterative post-order fold so deeply-nested
    /// trees don't blow the stack when `println!("{}", tree)` hits them.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rendered = post_order_string_fold(self, |node, child_strings| {
            let mut out = format!("\"{}\"\n", node.value);
            for (i, child_str) in child_strings.iter().enumerate() {
                // Substitute the empty-set glyph for DUMMY children
                // so the rendering is compact and unambiguous.
                if matches!(node.children[i].node_type, NodeType::Dummy) {
                    out.push_str(&format!("\t'{}'\n", '\u{2205}'));
                } else {
                    out.push_str(child_str);
                }
            }
            out
        });
        f.write_str(&rendered)
    }
}


const CHAR_WIDTH: usize = 6;
const CHAR_HEIGHT: usize = 15;
const PADDING: usize = 10;
