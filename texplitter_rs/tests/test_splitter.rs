use texplitter_rs::parserrules::load_regex_patterns;
use texplitter_rs::splitter::{TreeNode, NodeType, SegmentTree, convert_tokens_to_string, replace_substring_from_end};
use texplitter_rs::tokenizer::Tokenizer;
use texplitter_rs::TexplitterError;
use serde_json::Value;
use indexmap::IndexMap;

// Convert tokens to string
#[test]
fn test_convert_tokens_to_string() {
    let tokens: Vec<IndexMap<String, String>> = vec![
        [("text".to_string(), "Hello".to_string())].iter().cloned().collect(),
        [("text".to_string(), " ".to_string())].iter().cloned().collect(),
        [("text".to_string(), "World".to_string())].iter().cloned().collect()
    ];
    let result = convert_tokens_to_string(&tokens);
    assert_eq!(result, "Hello World");
}

// Replace substring from end
#[test]
fn test_replace_substring_from_end() {
    let original = "hello world world";
    let substring = "world";
    let replacement = "everyone";
    let result = replace_substring_from_end(original, substring, replacement, 1);
    let expected = "hello world everyone";
    assert_eq!(result, expected);
}

// `strip_spaces` matches the effective pattern ` *\n+$` directly with byte
// indexing. The equivalent regex with a `(?<=.*)` lookbehind would be
// rejected by fancy-regex, so we don't use a regex at all here.
#[test]
fn test_strip_spaces_python_parity() {
    let node = TreeNode::new(NodeType::Translatable, "x", vec![], vec![]);
    let cases: &[(&str, &str, &str, &str)] = &[
        ("hello", "hello", "", ""),
        ("  hello", "hello", "{sp}{sp}", ""),
        ("hello  ", "hello", "", "{sp}{sp}"),
        ("  hello  ", "hello", "{sp}{sp}", "{sp}{sp}"),
        ("hello\n", "hello", "", "\n"),
        ("hello \n", "hello", "", "{sp}\n"),
        ("hello  \n\n", "hello", "", "{sp}{sp}\n\n"),
        ("  hello  \n", "hello", "{sp}{sp}", "{sp}{sp}\n"),
        ("   ", "", "{sp}{sp}{sp}", "{sp}{sp}{sp}"),
        ("\n", "", "", "\n"),
        ("\n\n", "", "", "\n\n"),
        (" \n", "", "{sp}", "{sp}\n"),
        ("hello world", "hello world", "", ""),
    ];
    for (input, seg, lead, trail) in cases {
        let (s, l, t) = node.strip_spaces(input);
        assert_eq!(
            (s.as_str(), l.as_str(), t.as_str()),
            (*seg, *lead, *trail),
            "strip_spaces({:?}) mismatch",
            input
        );
    }
}

// TreeNode creation
#[test]
fn test_tree_node_creation() {
    let node = TreeNode::new(
        NodeType::Single { open: "<tag>".to_string(), close: Some("</tag>".to_string()) },
        "value",
        vec![],
        vec![],
    );
    assert!(matches!(node.node_type, NodeType::Single { .. }));
    assert_eq!(node.value, "value");
    if let NodeType::Single { open, close } = &node.node_type {
        assert_eq!(open, "<tag>");
        assert_eq!(close.as_deref(), Some("</tag>"));
    }
}

// TreeNode build full value
#[test]
fn test_tree_node_build_full_value() {
    let child = Box::new(TreeNode::new(NodeType::Translatable, "child_value", vec![], vec![]));
    let node = TreeNode::new(
        NodeType::Single { open: "<tag>".to_string(), close: Some("</tag>".to_string()) },
        "value",
        vec![child],
        vec![],
    );
    let result = node.build_full_value();
    assert_eq!(result, "<tag>child_value</tag>");
}

// TreeNode extract segments
#[test]
fn test_tree_node_extract_segments() {
    let child = Box::new(TreeNode::new(NodeType::Translatable, "child_value", vec![], vec![]));
    let node = TreeNode::new(
        NodeType::Single { open: "<tag>".to_string(), close: Some("</tag>".to_string()) },
        "value",
        vec![child],
        vec![],
    );
    let result = node.extract_segments();
    let expected: Vec<IndexMap<String, String>> = vec![
        [("TRANSLATABLE/value-0".to_string(), "child_value".to_string())].iter().cloned().collect()
    ];
    assert_eq!(result, expected);
}

// Generate DOT representation
#[test]
fn test_generate_dot_representation() {
    let child = Box::new(TreeNode::new(NodeType::Translatable, "child_value", vec![], vec![]));
    let node = TreeNode::new(
        NodeType::Single { open: "<tag>".to_string(), close: Some("</tag>".to_string()) },
        "value",
        vec![child],
        vec![],
    );
    let result = node.generate_dot_representation();
    let expected = r#"digraph G {
    rankdir = TB
    fontname = "DejaVu"
    fontsize = 10

    node [
        fontname = "DejaVu"
        fontsize = 10
        shape = "record"
    ]

    edge [
        fontname = "DejaVu"
        fontsize = 10
    ]

    "node0" [label="{<f0> node_type= SINGLE|value| { <f1> }}", fontcolor=red];
    "node1" [label="{<f0> node_type= TRANSLATABLE|child_value}", fontcolor=blue, shape=Mrecord];
    "node0":f1 -> "node1":f0 ;
}
"#;
    assert_eq!(result.trim(), expected.trim());
}

// Generate SVG
//
// Structural assertions on the output, not byte-for-byte equality:
// `xmlwriter` output formatting (attribute quoting, whitespace,
// element layout) is an implementation detail, but the semantics —
// root `<svg>` element, two sibling `<g>` groups at `node0`/`node1`
// with their rects and labels, a connector `<line>` between them —
// are the actual contract. Mirrors `tests/test_splitter.py::test_generate_svg`.
#[test]
fn test_generate_svg() {
    let child = Box::new(TreeNode::new(NodeType::Translatable, "child_value", vec![], vec![]));
    let node = TreeNode::new(
        NodeType::Single { open: "".to_string(), close: None },
        "value",
        vec![child],
        vec![],
    );
    let svg = node.generate_svg();

    // Document element is <svg> with the SVG namespace.
    assert!(
        svg.contains("<svg") && svg.contains("xmlns=\"http://www.w3.org/2000/svg\""),
        "expected <svg> root with SVG namespace, got: {svg}"
    );

    // Both node groups are present, with node1 appearing after node0.
    let node0 = svg.find("id=\"node0\"").expect("node0 group missing");
    let node1 = svg.find("id=\"node1\"").expect("node1 group missing");
    assert!(node0 < node1, "node0 must appear before node1");

    // Child groups are siblings of their parent, not descendants: the
    // closing </g> of node0 must appear before node1's opening tag.
    // (If node1 were nested inside node0, a transform on node0 would
    // cascade — the bug this test originally guarded against.)
    let node0_close = svg[node0..]
        .find("</g>")
        .map(|i| node0 + i)
        .expect("node0 </g> missing");
    assert!(
        node0_close < node1,
        "node1 must be a sibling of node0, not nested inside it"
    );

    // Each group contains a <rect> and the node's type + value text.
    assert!(svg.contains("SINGLE"), "parent node type SINGLE missing");
    assert!(svg.contains("value"), "parent node value missing");
    assert!(svg.contains("TRANSLATABLE"), "child node type TRANSLATABLE missing");
    assert!(svg.contains("child_value"), "child node value missing");
    assert_eq!(
        svg.matches("<rect").count(),
        2,
        "expected one <rect> per node, got: {svg}"
    );

    // A single connector <line> joins the two nodes.
    assert_eq!(
        svg.matches("<line").count(),
        1,
        "expected one connector <line>, got: {svg}"
    );
}

// SegmentTree creation
#[test]
fn test_segment_tree_creation() {
    // Equivalent to the Python test case tokens
    let tokens: Vec<IndexMap<String, Value>> = vec![
        [
            ("token_class".to_string(), Value::String("OPEN_BRACKET".to_string())),
            ("text".to_string(), Value::String("<tag>".to_string())),
            ("label".to_string(), Value::String("tag".to_string())),
        ].iter().cloned().collect(),
        [
            ("token_class".to_string(), Value::String("TRANSLATABLE".to_string())),
            ("text".to_string(), Value::String("text".to_string())),
            ("label".to_string(), Value::String("text".to_string())),
        ].iter().cloned().collect(),
        [
            ("token_class".to_string(), Value::String("CLOSE_BRACKET".to_string())),
            ("text".to_string(), Value::String("</tag>".to_string())),
            ("label".to_string(), Value::String("tag".to_string())),
        ].iter().cloned().collect(),
    ];

    // Create the segment tree
    let segment_tree = SegmentTree::new(tokens).expect("SegmentTree::new on valid tokens");

    // Check the tree node type
    assert!(matches!(segment_tree.tree.node_type, NodeType::Multiple));
}

// Malformed-input entry-point check. `SegmentTree::new` runs
// `validate_tokens` up front so wrong-shape fields (missing `text`,
// numeric `token_class`, etc.) surface as a catchable
// `TexplitterError::InvalidToken` rather than a panic.
#[test]
fn test_segment_tree_malformed_input_returns_error() {
    // Case 1: `token_class` is a number instead of a string.
    let tokens: Vec<IndexMap<String, Value>> = vec![
        [
            ("token_class".to_string(), Value::Number(42.into())),
            ("text".to_string(), Value::String("x".to_string())),
            ("label".to_string(), Value::String("x".to_string())),
        ].iter().cloned().collect(),
    ];
    let err = SegmentTree::new(tokens).expect_err("numeric token_class should fail validation");
    match err {
        TexplitterError::InvalidToken { index, .. } => assert_eq!(index, 0),
        other => panic!("expected InvalidToken, got {:?}", other),
    }

    // Case 2: `text` key is missing entirely.
    let tokens: Vec<IndexMap<String, Value>> = vec![
        [
            ("token_class".to_string(), Value::String("TRANSLATABLE".to_string())),
            ("label".to_string(), Value::String("x".to_string())),
        ].iter().cloned().collect(),
    ];
    let err = SegmentTree::new(tokens).expect_err("missing text key should fail validation");
    match err {
        TexplitterError::InvalidToken { index, .. } => assert_eq!(index, 0),
        other => panic!("expected InvalidToken, got {:?}", other),
    }

    // Case 3: `label` is null.
    let tokens: Vec<IndexMap<String, Value>> = vec![
        [
            ("token_class".to_string(), Value::String("TRANSLATABLE".to_string())),
            ("text".to_string(), Value::String("x".to_string())),
            ("label".to_string(), Value::Null),
        ].iter().cloned().collect(),
    ];
    let err = SegmentTree::new(tokens).expect_err("null label should fail validation");
    assert!(matches!(err, TexplitterError::InvalidToken { .. }));
}

// Complex HTML segment
#[test]
fn test_complex_html_segment() {
    let segment = r#"
    <p>See if this is a sentence.</p>
    <html lang="en">
      <head>
        <meta charset="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
        <title>Sample HTML</title>
        <link rel="stylesheet" type="text/css" href="_static/sample.css" />
      </head>
      <body>
        <p>See if this is a sentence.</p>
        <div class="document">
          <div class="documentwrapper">
            <div class="bodywrapper">
              <div class="body" role="main">
                <section id="sample-section">
                  <h1>Sample Section.<a class="headerlink" href="sample-section" title="Link to this heading">¶</a></h1>
                  <div class="toctree-wrapper compound"></div>
                  <section id="sample-subsection">
                    <h2>Sample Subsection.<a class="headerlink" href="sample-subsection" title="Link to this heading">¶</a></h2>
                    <dl class="py class" id="sample-class">
                      <dt class="sig sig-object py" id="sample-id">
                        <span class="sig-name descname">Sample Class</span>
                        <a class="headerlink" href="sample-id" title="Link to this definition">¶</a>
                      </dt>
                      <dd>
                        <p>A simple sample class for testing.</p>
                      </dd>
                    </dl>
                  </section>
                </section>
              </div>
            </div>
          </div>
        </div>
        <p>See if this is another sentence.</p>
        <div class="footer">
          &#169; 2024 Sample Footer
        </div>
      </body>
    </html>
    "#;

    let patterns = load_regex_patterns("../src/texplitter/config/rules/default.json").unwrap();
    let mut tokenizer = Tokenizer::new(patterns).expect("Failed to initialize tokenizer");
    let tokens = tokenizer.tokenize(segment).expect("Failed to tokenize segment");
    let unmatched_tags = tokenizer.get_unmatched_tags();
    let nesting_errors = tokenizer.get_nesting_errors();

    println!("{:?}", tokens);
    println!("{}", "*".repeat(40));
    println!("Unmatched Tags:");
    println!("{:?}", unmatched_tags);
    println!("{}", "*".repeat(40));
    println!("Nesting Errors:");
    println!("{:?}", nesting_errors);
    println!("{}", "*".repeat(40));

    // Additional debug output for analysis
    println!("Tokens: {:?}", tokens);
    println!("Unmatched Tags: {:?}", unmatched_tags);
    println!("Nesting Errors: {:?}", nesting_errors);

    assert!(unmatched_tags.is_empty(), "Unmatched tags should be empty but found {:?}", unmatched_tags);
    assert!(nesting_errors.is_empty(), "Nesting errors should be empty but found {:?}", nesting_errors);
}

// Deep-nesting verification for the TreeNode walk methods. Constructs a
// 2000-level hand-rolled Single chain (one TRANSLATABLE leaf at the bottom)
// and runs every walk method on it. A single surviving recursive call would
// blow the default 2 MiB test-thread stack well under 2000 levels. This
// test bypasses the tokenizer so it isolates the TreeNode walk machinery
// from any upstream input-validation paths.
#[test]
fn test_tree_node_walks_2000_levels_iterative() {
    // Build the tree on the heap — 2000 levels is well past the depth at
    // which any accidental per-level stack frame (~32–200 B per walk frame)
    // would overflow the default 2 MiB test-thread stack.
    let depth = 2000usize;
    let mut node = TreeNode::new(NodeType::Translatable, "leaf", vec![], vec![]);
    for _ in 0..depth {
        let wrapped = TreeNode::new(
            NodeType::Single {
                open: "<x>".to_string(),
                close: Some("</x>".to_string()),
            },
            "x",
            vec![Box::new(node)],
            vec![],
        );
        node = wrapped;
    }

    // Exercise each walk. A single surviving recursive call here would
    // panic with a stack overflow before returning.
    let _ = node.get_attributes();
    let _ = node.build_pattern();
    let full = node.build_full_value();
    assert!(full.contains("leaf"));
    let _ = node.extract_segments();

    // `extract_extended_segments` + `rebuild_from_segments` operate on the
    // `ExtendedSegment` struct. `ExtendedSegment` has an explicit iterative
    // `Drop` impl that drains nested `IndexMap<String, ExtendedSegment>`
    // children via a heap work stack, so a 2000-level tree survives
    // construction, rebuild, and drop.
    let extended = node.extract_extended_segments();
    let mut tag_list: Vec<String> = Vec::new();
    let _ = node.rebuild_from_segments(0, "TRANSLATABLE", &mut tag_list, &extended);
    drop(extended);

    let _ = format!("{}", node);
}
