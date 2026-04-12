//! Python bindings for texplitter_rs.
//!
//! Built with **pyo3** and **maturin** in mixed-layout mode: the native
//! extension installs as `texplitter._native` and the pure-Python shims in
//! `src/texplitter/*.py` re-export the names users actually import.
//!
//! ## Design notes
//!
//! 1. **Direct storage**, no `Arc<Mutex<_>>` wrappers around the Rust types.
//!    The `Bound<'py, T>` API and the default `Unsendable` behavior for
//!    non-`Send` types let us store the Rust struct directly and let pyo3
//!    handle GIL-protected mutability.
//!
//! 2. **`TreeNode.__init__`** takes a `node_type` / `value` / `tag` triple,
//!    which is the most natural Python constructor shape and matches what
//!    the existing test suite expects.
//!
//! 3. **`UnknownTokenError`** is exposed as a Python exception class so
//!    `except UnknownTokenError:` blocks in user code work as expected.
//!
//! 4. **`load_regex_patterns`** is exposed as a free function that returns a
//!    `list[dict]` matching the rule file JSON structure. The Python shim
//!    re-exports it from `texplitter.parserrules`.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple, PyAny};
use pyo3::exceptions::{PyValueError, PyTypeError};
use pyo3::create_exception;
use serde_json::Value;
use indexmap::IndexMap;

use crate::tokenizer::{Tokenizer as RsTokenizer, UnknownTokenError as RsUnknownTokenError};
use crate::splitter::{
    TreeNode as RsTreeNode, SegmentTree as RsSegmentTree, NodeType, ExtendedSegment,
    convert_tokens_to_string as rs_convert_tokens_to_string,
    replace_substring_from_end as rs_replace_substring_from_end,
};
use crate::parserrules::{load_regex_patterns as rs_load_regex_patterns, RegexPattern};
use crate::error::TexplitterError;

// --------------------------------------------------------------------------- //
// Exception types                                                             //
// --------------------------------------------------------------------------- //

create_exception!(
    _native,
    UnknownTokenError,
    pyo3::exceptions::PyException,
    "Raised when the tokenizer encounters text it cannot match against any rule."
);

create_exception!(
    _native,
    InvalidTokenError,
    PyValueError,
    "Raised when a user-supplied token dict is missing a required field \
     or has a field of the wrong type."
);

create_exception!(
    _native,
    InvalidNodeDictError,
    PyValueError,
    "Raised when a user-supplied node dict passed to TreeNode.from_dict \
     is malformed."
);

/// Convert a `TexplitterError` into a Python exception of the right class.
fn map_texplitter_error(err: TexplitterError) -> PyErr {
    match err {
        TexplitterError::InvalidToken { index, reason } => InvalidTokenError::new_err(format!(
            "invalid token at index {}: {}",
            index, reason
        )),
        TexplitterError::InvalidNodeDict(msg) => {
            InvalidNodeDictError::new_err(format!("invalid node dict: {}", msg))
        }
        TexplitterError::InvalidPattern(err) => {
            PyValueError::new_err(format!("invalid regex pattern: {}", err))
        }
        TexplitterError::RuleFile(msg) => PyValueError::new_err(format!("rule file error: {}", msg)),
    }
}

// --------------------------------------------------------------------------- //
// Python <-> serde_json::Value conversions                                    //
// --------------------------------------------------------------------------- //

/// Convert a Python object (str / int / float / bool / list / dict / None) into
/// a `serde_json::Value`. Used when ingesting `token` dicts the caller builds
/// in Python for `SegmentTree(...)` and `TreeNode.from_dict(...)`.
fn pyobj_to_value(obj: &Bound<'_, PyAny>) -> PyResult<Value> {
    if obj.is_none() {
        return Ok(Value::Null);
    }
    if let Ok(b) = obj.extract::<bool>() {
        return Ok(Value::Bool(b));
    }
    if let Ok(i) = obj.extract::<i64>() {
        return Ok(Value::Number(i.into()));
    }
    if let Ok(u) = obj.extract::<u64>() {
        return Ok(Value::Number(u.into()));
    }
    if let Ok(f) = obj.extract::<f64>() {
        return Ok(serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null));
    }
    if let Ok(s) = obj.extract::<String>() {
        return Ok(Value::String(s));
    }
    if let Ok(list) = obj.downcast::<PyList>() {
        let mut out = Vec::with_capacity(list.len());
        for item in list.iter() {
            out.push(pyobj_to_value(&item)?);
        }
        return Ok(Value::Array(out));
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = serde_json::Map::with_capacity(dict.len());
        for (k, v) in dict.iter() {
            let key: String = k.extract()?;
            map.insert(key, pyobj_to_value(&v)?);
        }
        return Ok(Value::Object(map));
    }
    Err(PyTypeError::new_err(format!(
        "cannot convert Python object of type {} to a JSON value",
        obj.get_type().name()?
    )))
}

/// Convert a `serde_json::Value` into a fresh Python object.
fn value_to_pyobj<'py>(py: Python<'py>, value: &Value) -> PyResult<Bound<'py, PyAny>> {
    match value {
        Value::Null => Ok(py.None().into_bound(py)),
        Value::Bool(b) => Ok(b.into_pyobject(py)?.to_owned().into_any()),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_pyobject(py)?.into_any())
            } else if let Some(u) = n.as_u64() {
                Ok(u.into_pyobject(py)?.into_any())
            } else if let Some(f) = n.as_f64() {
                Ok(f.into_pyobject(py)?.into_any())
            } else {
                Ok(py.None().into_bound(py))
            }
        }
        Value::String(s) => Ok(s.into_pyobject(py)?.into_any()),
        Value::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                list.append(value_to_pyobj(py, item)?)?;
            }
            Ok(list.into_any())
        }
        Value::Object(obj) => {
            let dict = PyDict::new(py);
            for (k, v) in obj {
                dict.set_item(k, value_to_pyobj(py, v)?)?;
            }
            Ok(dict.into_any())
        }
    }
}

/// Convert a Python dict to an `IndexMap<String, Value>` while preserving key
/// order — the splitter is sensitive to token-dict iteration order in a few
/// edge cases, and dict preservation has been standard since Python 3.7.
fn pydict_to_indexmap(dict: &Bound<'_, PyDict>) -> PyResult<IndexMap<String, Value>> {
    let mut map = IndexMap::with_capacity(dict.len());
    for (k, v) in dict.iter() {
        let key: String = k.extract()?;
        map.insert(key, pyobj_to_value(&v)?);
    }
    Ok(map)
}

fn indexmap_to_pydict<'py>(
    py: Python<'py>,
    map: &IndexMap<String, Value>,
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    for (k, v) in map {
        dict.set_item(k, value_to_pyobj(py, v)?)?;
    }
    Ok(dict)
}

// --------------------------------------------------------------------------- //
// RegexPattern loading                                                        //
// --------------------------------------------------------------------------- //

/// Load the packaged rule file and return a list of pattern dicts in the same
/// shape the old `texplitter.parserrules.regex_patterns` global had.
///
/// Arguments:
///   `path` — absolute path to a JSON rule file. Callers who want the
///   packaged default ruleset should use `texplitter.parserrules.regex_patterns`
///   instead, which is loaded from `texplitter/config/rules/default.json`
///   via `importlib.resources`.
#[pyfunction]
#[pyo3(signature = (path))]
fn load_regex_patterns_from_path<'py>(
    py: Python<'py>,
    path: &str,
) -> PyResult<Bound<'py, PyList>> {
    let patterns = rs_load_regex_patterns(path)
        .map_err(|e| PyValueError::new_err(format!("rule file error: {}", e)))?;
    regex_patterns_to_pylist(py, &patterns)
}

fn regex_patterns_to_pylist<'py>(
    py: Python<'py>,
    patterns: &[RegexPattern],
) -> PyResult<Bound<'py, PyList>> {
    let list = PyList::empty(py);
    for p in patterns {
        let d = PyDict::new(py);
        d.set_item("class", &p.class)?;
        d.set_item("type", &p.r#type)?;
        d.set_item("regex", &p.regex)?;
        d.set_item("callback", &p.callback)?;
        if let Some(desc) = &p.description {
            d.set_item("description", desc)?;
        } else {
            d.set_item("description", py.None())?;
        }
        if let Some(sample) = &p.sample {
            d.set_item("sample", sample)?;
        } else {
            d.set_item("sample", py.None())?;
        }
        list.append(d)?;
    }
    Ok(list)
}

/// Convert a Python list of pattern dicts into a `Vec<RegexPattern>`.
fn pylist_to_regex_patterns(list: &Bound<'_, PyList>) -> PyResult<Vec<RegexPattern>> {
    let mut out = Vec::with_capacity(list.len());
    for item in list.iter() {
        let d = item.downcast::<PyDict>().map_err(|_| {
            PyTypeError::new_err("each regex pattern entry must be a dict")
        })?;
        let class: String = d
            .get_item("class")?
            .ok_or_else(|| PyValueError::new_err("regex pattern missing `class`"))?
            .extract()?;
        let r#type: String = d
            .get_item("type")?
            .ok_or_else(|| PyValueError::new_err("regex pattern missing `type`"))?
            .extract()?;
        let regex: String = d
            .get_item("regex")?
            .ok_or_else(|| PyValueError::new_err("regex pattern missing `regex`"))?
            .extract()?;
        let callback: String = d
            .get_item("callback")?
            .ok_or_else(|| PyValueError::new_err("regex pattern missing `callback`"))?
            .extract()?;
        let description: Option<String> = match d.get_item("description")? {
            Some(v) if !v.is_none() => Some(v.extract()?),
            _ => None,
        };
        let sample: Option<String> = match d.get_item("sample")? {
            Some(v) if !v.is_none() => Some(v.extract()?),
            _ => None,
        };
        out.push(RegexPattern {
            class,
            r#type,
            regex,
            callback,
            description,
            sample,
        });
    }
    Ok(out)
}

// --------------------------------------------------------------------------- //
// Tokenizer                                                                   //
// --------------------------------------------------------------------------- //

#[pyclass(name = "Tokenizer", module = "texplitter._native", unsendable)]
struct PyTokenizer {
    inner: RsTokenizer,
}

#[pymethods]
impl PyTokenizer {
    /// Build a Tokenizer from a list of regex-pattern dicts (same shape as
    /// `texplitter.parserrules.regex_patterns`). The pure-Python
    /// `Tokenizer()` constructor in the shim loads the packaged rule file and
    /// forwards it here, so Python callers don't need to pass anything.
    #[new]
    fn new(patterns: &Bound<'_, PyList>) -> PyResult<Self> {
        let rules = pylist_to_regex_patterns(patterns)?;
        let inner = RsTokenizer::new(rules)
            .map_err(|e| PyValueError::new_err(format!("tokenizer init failed: {}", e)))?;
        Ok(PyTokenizer { inner })
    }

    /// Tokenize `intext` and return a `list[dict]` where each dict has keys
    /// `token_class`, `token_type`, `text`, `label`, `start`, `end`.
    fn tokenize<'py>(
        &mut self,
        py: Python<'py>,
        intext: &str,
    ) -> PyResult<Bound<'py, PyList>> {
        let tokens = self.inner.tokenize(intext).map_err(|e| {
            // Downcast the `Box<dyn Error>` to RsUnknownTokenError so the
            // Python side can `except UnknownTokenError:` regardless of the
            // message format.
            if e.downcast_ref::<RsUnknownTokenError>().is_some() {
                UnknownTokenError::new_err(e.to_string())
            } else {
                PyValueError::new_err(e.to_string())
            }
        })?;
        let list = PyList::empty(py);
        for token in &tokens {
            let d = indexmap_to_pydict(py, token)?;
            list.append(d)?;
        }
        Ok(list)
    }

    /// Return the list of unmatched-tag records collected during the most
    /// recent `tokenize()` call (empty after a well-formed input).
    fn get_unmatched_tags<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let list = PyList::empty(py);
        for tag in self.inner.get_unmatched_tags() {
            list.append(indexmap_to_pydict(py, tag)?)?;
        }
        Ok(list)
    }

    /// Return the list of nesting-error records collected during the most
    /// recent `tokenize()` call.
    fn get_nesting_errors<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let list = PyList::empty(py);
        for err in self.inner.get_nesting_errors() {
            list.append(indexmap_to_pydict(py, err)?)?;
        }
        Ok(list)
    }
}

// --------------------------------------------------------------------------- //
// TreeNode                                                                    //
// --------------------------------------------------------------------------- //

#[pyclass(name = "TreeNode", module = "texplitter._native", unsendable, from_py_object)]
#[derive(Clone)]
struct PyTreeNode {
    inner: RsTreeNode,
}

/// Normalize the `tag` argument from the Python constructor into the
/// two-element `[open, close]` vector the internal builder expects.
/// Accepts either a Python list of strings or a single string (an unclosed
/// tag, represented by a one-element vec).
fn normalize_tag_arg(tag: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    if let Ok(s) = tag.extract::<String>() {
        if s.is_empty() {
            return Ok(Vec::new());
        }
        return Ok(vec![s]);
    }
    if let Ok(list) = tag.downcast::<PyList>() {
        let mut out = Vec::with_capacity(list.len());
        for item in list.iter() {
            out.push(item.extract::<String>()?);
        }
        return Ok(out);
    }
    Err(PyTypeError::new_err(
        "`tag` must be a string or a list of strings",
    ))
}

fn normalize_children_arg(children: Option<&Bound<'_, PyList>>) -> PyResult<Vec<Box<RsTreeNode>>> {
    let Some(list) = children else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(list.len());
    for item in list.iter() {
        let child: PyRef<PyTreeNode> = item.extract().map_err(|_| {
            PyTypeError::new_err("`children` must be a list of TreeNode instances")
        })?;
        out.push(Box::new(child.inner.clone()));
    }
    Ok(out)
}

fn normalize_attributes_arg(
    attrs: Option<&Bound<'_, PyList>>,
) -> PyResult<Vec<(String, String)>> {
    let Some(list) = attrs else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(list.len());
    for item in list.iter() {
        // Accept either a 2-tuple or a 2-list.
        if let Ok(tup) = item.downcast::<PyTuple>() {
            if tup.len() != 2 {
                return Err(PyValueError::new_err(
                    "each attribute must be a 2-element (key, value) tuple",
                ));
            }
            let k: String = tup.get_item(0)?.extract()?;
            let v: String = tup.get_item(1)?.extract()?;
            out.push((k, v));
        } else if let Ok(lst) = item.downcast::<PyList>() {
            if lst.len() != 2 {
                return Err(PyValueError::new_err(
                    "each attribute must be a 2-element [key, value] list",
                ));
            }
            let k: String = lst.get_item(0)?.extract()?;
            let v: String = lst.get_item(1)?.extract()?;
            out.push((k, v));
        } else {
            return Err(PyTypeError::new_err(
                "attributes must be a list of (key, value) tuples",
            ));
        }
    }
    Ok(out)
}

#[pymethods]
impl PyTreeNode {
    /// Construct a TreeNode from a `(node_type, value, tag)` triple.
    ///
    /// `node_type` is one of `"MULTIPLE"`, `"SINGLE"`, `"BINARY"`,
    /// `"BINARY_TEMPLATE"`, `"TRANSLATABLE"`, `"DUMMY"`.
    /// `tag` is a list `[open, close]` for SINGLE nodes, a string (empty or
    /// single-element) otherwise.
    #[new]
    #[pyo3(signature = (node_type, value, tag, children=None, attributes=None))]
    fn new(
        node_type: &str,
        value: &str,
        tag: &Bound<'_, PyAny>,
        children: Option<&Bound<'_, PyList>>,
        attributes: Option<&Bound<'_, PyList>>,
    ) -> PyResult<Self> {
        let tag_vec = normalize_tag_arg(tag)?;
        let kids = normalize_children_arg(children)?;
        let attrs = normalize_attributes_arg(attributes)?;
        let inner = RsTreeNode::from_legacy(node_type, value, tag_vec, kids, attrs);
        Ok(PyTreeNode { inner })
    }

    /// Build a TreeNode from a dict with keys
    /// `{node_type, value, tag, children, attributes}`.
    #[staticmethod]
    fn from_dict(node_dict: &Bound<'_, PyDict>) -> PyResult<Self> {
        let map = pydict_to_indexmap(node_dict)?;
        let inner = RsTreeNode::from_dict(&map).map_err(map_texplitter_error)?;
        Ok(PyTreeNode { inner })
    }

    /// String tag of this node's `node_type`
    /// (`"SINGLE"`, `"BINARY"`, etc.).
    #[getter]
    fn node_type(&self) -> &'static str {
        self.inner.node_type_tag()
    }

    /// `value` field: the literal text for Translatable/Dummy nodes,
    /// the operator for Binary nodes, an empty string otherwise.
    #[getter]
    fn value(&self) -> String {
        match &self.inner.node_type {
            NodeType::Binary { operator } | NodeType::BinaryTemplate { operator } => {
                operator.clone()
            }
            _ => self.inner.value.clone(),
        }
    }

    /// `tag` field: `[open, close]` for a closed Single node,
    /// `[open]` for an unclosed Single node, `[]` for all other variants.
    #[getter]
    fn tag<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let list = PyList::empty(py);
        if let NodeType::Single { open, close } = &self.inner.node_type {
            list.append(open.as_str())?;
            if let Some(c) = close {
                list.append(c.as_str())?;
            }
        }
        Ok(list)
    }

    /// Child nodes as a fresh list of TreeNode instances.
    #[getter]
    fn children<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let list = PyList::empty(py);
        for c in &self.inner.children {
            let child = PyTreeNode {
                inner: (**c).clone(),
            };
            list.append(Py::new(py, child)?)?;
        }
        Ok(list)
    }

    /// Return this node's own attributes plus every descendant's, in
    /// pre-order. Matches the Rust `get_attributes` contract.
    fn get_attributes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let attrs = self.inner.get_attributes();
        let list = PyList::empty(py);
        for (k, v) in attrs {
            let t = PyTuple::new(py, &[k, v])?;
            list.append(t)?;
        }
        Ok(list)
    }

    fn build_pattern(&self) -> String {
        self.inner.build_pattern()
    }

    fn build_full_value(&self) -> String {
        self.inner.build_full_value()
    }

    fn strip_spaces(&self, text: &str) -> (String, String, String) {
        self.inner.strip_spaces(text)
    }

    /// Generate template + post-editing segments for this subtree.
    ///
    /// Returns `(end_index, template, post_editing_segments)` where
    /// `post_editing_segments` is a list of `(index, text)` tuples.
    #[pyo3(signature = (end=""))]
    fn generate_placeholders_for_segments<'py>(
        &self,
        py: Python<'py>,
        end: &str,
    ) -> PyResult<(usize, String, Bound<'py, PyList>)> {
        let mut segments: Vec<(usize, String)> = Vec::new();
        let (end_index, template) =
            self.inner.generate_placeholders_for_segments(end, 0, &mut segments);
        let seg_list = PyList::empty(py);
        for (idx, text) in segments {
            let t = PyTuple::new(py, &[idx.into_pyobject(py)?.into_any(), text.into_pyobject(py)?.into_any()])?;
            seg_list.append(t)?;
        }
        Ok((end_index, template, seg_list))
    }

    /// Return the flat-merged segments map for this subtree as a single-element
    /// list (matching the Rust `extract_segments` signature).
    fn extract_segments<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let segs = self.inner.extract_segments();
        let list = PyList::empty(py);
        for seg in segs {
            let d = PyDict::new(py);
            for (k, v) in seg {
                d.set_item(k, v)?;
            }
            list.append(d)?;
        }
        Ok(list)
    }

    /// Extract the rich extended-segments map (keyed by tag path).
    fn extract_extended_segments<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let map = self.inner.extract_extended_segments();
        extended_segments_to_pydict(py, &map)
    }

    /// Rebuild the document from (possibly translated) extended segments.
    ///
    /// Returns `(rebuilt_text, key)` where `key` is an internal tag identifier
    /// (empty string for the root call).
    ///
    /// # Arguments
    /// * `level` — recursion depth (pass `0` for the root call)
    /// * `tag` — root tag prefix (pass `"TRANSLATABLE"` for the root call)
    /// * `tag_list` — mutable list tracking visited tags (pass `[]` for fresh call)
    /// * `extended_segments` — dict from `extract_extended_segments()`, with
    ///   `"segment"` values optionally replaced with translated text
    fn rebuild_from_segments(
        &self,
        level: usize,
        tag: &str,
        tag_list: Vec<String>,
        extended_segments: &Bound<'_, PyDict>,
    ) -> PyResult<(String, String)> {
        let es_map = pydict_to_extended_segments(extended_segments)?;
        let mut tl = tag_list;
        Ok(self.inner.rebuild_from_segments(level, tag, &mut tl, &es_map))
    }

    fn generate_dot_representation(&self) -> String {
        self.inner.generate_dot_representation()
    }

    fn generate_svg(&self) -> String {
        self.inner.generate_svg()
    }
}

/// Convert `IndexMap<String, ExtendedSegment>` to a Python dict keyed on
/// segment key, with each value a dict of segment fields.
fn extended_segments_to_pydict<'py>(
    py: Python<'py>,
    map: &IndexMap<String, ExtendedSegment>,
) -> PyResult<Bound<'py, PyDict>> {
    let out = PyDict::new(py);
    for (key, seg) in map {
        let d = extended_segment_to_pydict(py, seg)?;
        out.set_item(key, d)?;
    }
    Ok(out)
}

fn extended_segment_to_pydict<'py>(
    py: Python<'py>,
    seg: &ExtendedSegment,
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("segment", &seg.segment)?;
    // Attributes as a list of 2-tuples (matches the Rust/Python shape).
    let attrs_list = PyList::empty(py);
    for (k, v) in &seg.attributes {
        let t = PyTuple::new(py, &[k.as_str(), v.as_str()])?;
        attrs_list.append(t)?;
    }
    d.set_item("attributes", attrs_list)?;

    use crate::splitter::ExtendedSegmentInner;
    match &seg.inner {
        ExtendedSegmentInner::Leaf => {}
        ExtendedSegmentInner::WithChildren(children) => {
            // Python shape: `children` is a list of dicts, one per child
            // map. `WithChildren` carries a `Vec<IndexMap>` because the
            // Python side flattens per-sibling maps at this level.
            let children_list = PyList::empty(py);
            for child_map in children {
                let child_dict = extended_segments_to_pydict(py, child_map)?;
                children_list.append(child_dict)?;
            }
            d.set_item("children", children_list)?;
        }
        ExtendedSegmentInner::Binary { left, right, operator } => {
            let left_dict = extended_segments_to_pydict(py, left)?;
            let right_dict = extended_segments_to_pydict(py, right)?;
            d.set_item("left", left_dict)?;
            d.set_item("right", right_dict)?;
            d.set_item("operator", operator)?;
        }
    }
    Ok(d)
}

/// Convert a Python dict (as returned by `extract_extended_segments`) back into
/// the Rust `IndexMap<String, ExtendedSegment>` that `rebuild_from_segments`
/// expects. Only the `segment` field is read by rebuild, but we reconstruct
/// the full structure for correctness.
fn pydict_to_extended_segments(
    d: &Bound<'_, PyDict>,
) -> PyResult<IndexMap<String, ExtendedSegment>> {
    use crate::splitter::ExtendedSegmentInner;
    let mut map = IndexMap::new();
    for (key, val) in d.iter() {
        let k: String = key.extract()?;
        let v = val.downcast::<PyDict>().map_err(|_| {
            PyTypeError::new_err(format!(
                "extended_segments value for key '{}' must be a dict", k
            ))
        })?;
        let seg = pydict_to_extended_segment(v)?;
        map.insert(k, seg);
    }
    Ok(map)
}

fn pydict_to_extended_segment(
    d: &Bound<'_, PyDict>,
) -> PyResult<ExtendedSegment> {
    use crate::splitter::ExtendedSegmentInner;

    let segment: String = d.get_item("segment")?
        .map(|v| v.extract::<String>())
        .transpose()?
        .unwrap_or_default();

    let mut attributes: Vec<(String, String)> = Vec::new();
    if let Some(attrs) = d.get_item("attributes")? {
        let attrs_list = attrs.downcast::<PyList>().map_err(|_| {
            PyTypeError::new_err("attributes must be a list")
        })?;
        for item in attrs_list.iter() {
            let tuple = item.downcast::<PyTuple>().map_err(|_| {
                PyTypeError::new_err("each attribute must be a tuple")
            })?;
            let k: String = tuple.get_item(0)?.extract()?;
            let v: String = tuple.get_item(1)?.extract()?;
            attributes.push((k, v));
        }
    }

    let inner = if let Some(children_obj) = d.get_item("children")? {
        let children_list = children_obj.downcast::<PyList>().map_err(|_| {
            PyTypeError::new_err("children must be a list")
        })?;
        let mut children = Vec::new();
        for child in children_list.iter() {
            let child_dict = child.downcast::<PyDict>().map_err(|_| {
                PyTypeError::new_err("each child must be a dict")
            })?;
            children.push(pydict_to_extended_segments(child_dict)?);
        }
        ExtendedSegmentInner::WithChildren(children)
    } else if let Some(left_obj) = d.get_item("left")? {
        let left = left_obj.downcast::<PyDict>().map_err(|_| {
            PyTypeError::new_err("left must be a dict")
        })?;
        let right_obj = d.get_item("right")?.ok_or_else(|| {
            PyValueError::new_err("Binary segment missing 'right'")
        })?;
        let right = right_obj.downcast::<PyDict>().map_err(|_| {
            PyTypeError::new_err("right must be a dict")
        })?;
        let operator: String = d.get_item("operator")?
            .map(|v| v.extract::<String>())
            .transpose()?
            .unwrap_or_default();
        ExtendedSegmentInner::Binary {
            left: Box::new(pydict_to_extended_segments(left)?),
            right: Box::new(pydict_to_extended_segments(right)?),
            operator,
        }
    } else {
        ExtendedSegmentInner::Leaf
    };

    Ok(ExtendedSegment { segment, attributes, inner })
}

// --------------------------------------------------------------------------- //
// SegmentTree                                                                 //
// --------------------------------------------------------------------------- //

#[pyclass(name = "SegmentTree", module = "texplitter._native", unsendable)]
struct PySegmentTree {
    inner: RsSegmentTree,
}

#[pymethods]
impl PySegmentTree {
    /// Build a SegmentTree from a list of token dicts.
    #[new]
    fn new(tokens: &Bound<'_, PyList>) -> PyResult<Self> {
        let mut rs_tokens: Vec<IndexMap<String, Value>> = Vec::with_capacity(tokens.len());
        for (i, item) in tokens.iter().enumerate() {
            let d = item.downcast::<PyDict>().map_err(|_| {
                PyTypeError::new_err(format!(
                    "token at index {} must be a dict", i
                ))
            })?;
            rs_tokens.push(pydict_to_indexmap(d)?);
        }
        let inner = RsSegmentTree::new(rs_tokens).map_err(map_texplitter_error)?;
        Ok(PySegmentTree { inner })
    }

    /// Construct a SegmentTree whose tree field is the given TreeNode.
    #[staticmethod]
    fn from_tree(tree: &PyTreeNode) -> Self {
        let inner = RsSegmentTree::new_from_tree(tree.inner.clone());
        PySegmentTree { inner }
    }

    /// The tree as a fresh `TreeNode` Python object. Cloned so the caller
    /// can freely inspect it without holding a borrow on the SegmentTree.
    #[getter]
    fn tree(&self) -> PyTreeNode {
        PyTreeNode {
            inner: self.inner.tree.clone(),
        }
    }
}

// --------------------------------------------------------------------------- //
// Free functions                                                              //
// --------------------------------------------------------------------------- //

/// Concatenate the `text` field of every token in order.
#[pyfunction]
fn convert_tokens_to_string(tokens: &Bound<'_, PyList>) -> PyResult<String> {
    let mut rs_tokens: Vec<IndexMap<String, String>> = Vec::with_capacity(tokens.len());
    for item in tokens.iter() {
        let d = item.downcast::<PyDict>().map_err(|_| {
            PyTypeError::new_err("each token must be a dict")
        })?;
        let mut map = IndexMap::with_capacity(d.len());
        for (k, v) in d.iter() {
            let key: String = k.extract()?;
            // Tolerate non-string values — stringify them the way Python
            // would. This matches the old binding's behavior where
            // `value.to_string()` was called regardless of type.
            let val: String = v.extract().unwrap_or_else(|_| v.to_string());
            map.insert(key, val);
        }
        rs_tokens.push(map);
    }
    Ok(rs_convert_tokens_to_string(&rs_tokens))
}

/// Replace the last `count` occurrences of `old` in `text` with `new`.
#[pyfunction]
fn replace_substring_from_end(text: &str, old: &str, new: &str, count: usize) -> String {
    rs_replace_substring_from_end(text, old, new, count)
}

// --------------------------------------------------------------------------- //
// Module registration                                                         //
// --------------------------------------------------------------------------- //

/// `texplitter._native` module definition.
#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyTokenizer>()?;
    m.add_class::<PyTreeNode>()?;
    m.add_class::<PySegmentTree>()?;
    m.add("UnknownTokenError", m.py().get_type::<UnknownTokenError>())?;
    m.add("InvalidTokenError", m.py().get_type::<InvalidTokenError>())?;
    m.add("InvalidNodeDictError", m.py().get_type::<InvalidNodeDictError>())?;
    m.add_function(wrap_pyfunction!(convert_tokens_to_string, m)?)?;
    m.add_function(wrap_pyfunction!(replace_substring_from_end, m)?)?;
    m.add_function(wrap_pyfunction!(load_regex_patterns_from_path, m)?)?;
    Ok(())
}
