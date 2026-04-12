//! End-to-end demo of the texplitter pipeline: load the rule file, tokenize a
//! sample segment, build a segment tree, and round-trip it through
//! `convert_tokens_to_string`.
//!
//! Run from the `texplitter_rs/` directory:
//!
//! ```sh
//! cargo run --example demo
//! ```
//!
//! This example exists purely as human-readable documentation of how to
//! drive the library end-to-end — nothing in the library crate links
//! against it.

use indexmap::IndexMap;
use texplitter_rs::parserrules::load_regex_patterns;
use texplitter_rs::splitter::{
    convert_tokens_to_string, replace_substring_from_end, SegmentTree, TreeNode,
};
use texplitter_rs::tokenizer::Tokenizer;

fn main() {
    // Rule file lives inside the Python package at
    // src/texplitter/config/rules/default.json (one level up from this
    // crate root). Adjust if you invoke the example from elsewhere.
    let regex_patterns =
        load_regex_patterns("../src/texplitter/config/rules/default.json").expect("load regex patterns");
    let mut tokenizer = Tokenizer::new(regex_patterns).expect("tokenizer init");

    let sample_segment = r#"
        <div>
            <p>This is a paragraph with an <strong>important</strong> part.
            <p>This paragraph is missing a closing tag.
        </div>
        <div>
            <span>Another span without closing.
        </div>
    "#;

    let tokens = tokenizer.tokenize(sample_segment).expect("tokenize");
    let unmatched_tags = tokenizer.get_unmatched_tags();
    let nesting_errors = tokenizer.get_nesting_errors();

    println!("Tokens: {:?}", tokens);
    println!("Unmatched Tags: {:?}", unmatched_tags);
    println!("Nesting Errors: {:?}", nesting_errors);

    // TreeNode round-trip from a hand-authored dict (exercise of `from_dict`).
    let node_dict = serde_json::from_str::<IndexMap<String, serde_json::Value>>(
        r#"
    {
        "node_type": "SINGLE",
        "value": "root",
        "tag": ["root_tag"],
        "children": [],
        "attributes": [["key", "value"]]
    }"#,
    )
    .expect("sample node_dict parses");

    let tree_node = TreeNode::from_dict(&node_dict).expect("TreeNode::from_dict");
    println!("TreeNode: {:?}", tree_node);

    // SegmentTree over the same tokens.
    let segment_tree = SegmentTree::new(tokens.clone()).expect("SegmentTree::new");
    println!("SegmentTree: {:?}", segment_tree.tree);

    // replace_substring_from_end round-trip.
    let replaced_text = replace_substring_from_end("hello world world", "world", "everyone", 1);
    println!("Replaced Text: {}", replaced_text);

    // convert_tokens_to_string takes a flattened `IndexMap<String, String>` —
    // convert the `Value::String` map to that shape first.
    let tokens_str_vec: Vec<IndexMap<String, String>> = tokens
        .iter()
        .map(|map| {
            map.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                .collect()
        })
        .collect();
    let concatenated_text = convert_tokens_to_string(&tokens_str_vec);
    println!("Concatenated Text: {}", concatenated_text);
}
