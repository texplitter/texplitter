//! Stress and regression tests.
//!
//! Test buckets:
//!
//! 1. Deep nesting: synthesize a 1000-level `<div>` tree and verify the
//!    pipeline doesn't stack overflow. SegmentTree walks are iterative,
//!    but this test guards against any future regression that
//!    reintroduces unbounded recursion.
//!
//! 2. Large file: synthesize a ~1 MB HTML blob and verify end-to-end
//!    segmentation completes in a reasonable time.
//!
//! 3. Round-trip property: verify `tokenize` produces tokens whose
//!    concatenated `text` recovers the original input (modulo the
//!    documented `." <-> ".` normalization in `tokenizer.rs`).
//!
//! 4. Duplicate-type correction fixtures: three inputs that exercise
//!    the three duplicate-`type` rule pairs in `default.json`
//!    (BREAK/dash vs PUNCTUATION/dash; BREAK/vertical_bar vs
//!    PUNCTUATION/vertical_bar; SYMBOL/xml_entity vs
//!    SINGLE_TAG/xml_entity). The loader returns `Vec<RegexPattern>`
//!    and the tokenizer callback table is keyed on `class__type`, so
//!    duplicate-type rules survive into the combined alternation regex.

use std::time::Instant;

use texplitter_rs::parserrules::load_regex_patterns;
use texplitter_rs::splitter::SegmentTree;
use texplitter_rs::tokenizer::Tokenizer;

const PATTERNS_PATH: &str = "../src/texplitter/config/rules/default.json";

fn make_tokenizer() -> Tokenizer {
    let patterns = load_regex_patterns(PATTERNS_PATH).expect("load_regex_patterns");
    Tokenizer::new(patterns).expect("Tokenizer::new")
}

// --------------------------------------------------------------------------- //
// 1. Deep-nesting stress test                                                 //
// --------------------------------------------------------------------------- //

/// A 1000-level `<div>` tree must not stack-overflow the tokenizer.
///
/// This test covers the tokenize step. See
/// `test_deep_nesting_1000_levels_build_tree` below for the matching
/// test that drives `SegmentTree::build_node` on the same input.
#[test]
fn test_deep_nesting_1000_levels_tokenize() {
    let depth = 1000usize;
    let mut html = String::with_capacity(depth * 11 + 7);
    for _ in 0..depth {
        html.push_str("<div>");
    }
    html.push_str("content");
    for _ in 0..depth {
        html.push_str("</div>");
    }

    let mut tok = make_tokenizer();
    let tokens = tok.tokenize(&html).expect("tokenize deep-nested html");
    // 1000 open tags + 1000 close tags + 1 text token
    assert_eq!(
        tokens.len(),
        2 * depth + 1,
        "expected {} tokens, got {}",
        2 * depth + 1,
        tokens.len()
    );
}

#[test]
fn test_deep_nesting_1000_levels_build_tree() {
    // 8 MB stack as belt-and-braces. The walks are iterative and the
    // 1000-level `Vec<Box<TreeNode>>` Drop chain is handled by the
    // explicit `impl Drop for TreeNode` (which moves children to a
    // heap work stack), but a generous stack is cheap insurance.
    let handle = std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let depth = 1000usize;
            let mut html = String::with_capacity(depth * 11 + 7);
            for _ in 0..depth {
                html.push_str("<div>");
            }
            html.push_str("content");
            for _ in 0..depth {
                html.push_str("</div>");
            }

            let mut tok = make_tokenizer();
            let tokens = tok.tokenize(&html).expect("tokenize deep-nested html");
            let _tree = SegmentTree::new(tokens).expect("SegmentTree::new");
        })
        .expect("spawn deep-nesting thread");

    handle.join().expect("deep-nesting thread did not panic");
}

// --------------------------------------------------------------------------- //
// 2. Large-file stress test                                                   //
// --------------------------------------------------------------------------- //

fn build_1mb_html_blob() -> String {
    // 5000 sections ~= 910 KB — enough to exercise the regex, tree builder, and
    // placeholder generation without making CI crawl.
    let mut parts: Vec<String> = Vec::with_capacity(5002);
    parts.push("<html><body>".to_string());
    for i in 0..5000 {
        parts.push(format!(
            "<div class=\"section\"><h2>Section {i}</h2>\
             <p>This is paragraph {i} with <em>emphasis</em> and \
             <code>inline code</code> and a <a href=\"#\">link</a>. \
             Some more text to pad it out.</p></div>"
        ));
    }
    parts.push("</body></html>".to_string());
    parts.concat()
}

/// Tokenization of ~1 MB HTML should complete well under 30 s on a
/// typical dev machine. The 30 s ceiling is a catastrophic-regression
/// bound, not a micro-benchmark.
#[test]
fn test_large_file_tokenize() {
    let handle = std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let html = build_1mb_html_blob();
            assert!(
                html.len() >= 900 * 1024,
                "test blob smaller than expected: {} bytes",
                html.len()
            );

            let mut tok = make_tokenizer();
            let t0 = Instant::now();
            let tokens = tok.tokenize(&html).expect("tokenize 1 MB blob");
            let elapsed = t0.elapsed();

            assert!(
                tokens.len() > 100_000,
                "expected >100k tokens from 1 MB blob, got {}",
                tokens.len()
            );
            assert!(
                elapsed.as_secs_f64() < 30.0,
                "tokenize took {:?}, expected <30s",
                elapsed
            );
            eprintln!(
                "[baseline] rust tokenize 1 MB: {:.2}s ({} tokens)",
                elapsed.as_secs_f64(),
                tokens.len()
            );
        })
        .expect("spawn large-file tokenize thread");

    handle.join().expect("large-file tokenize thread did not panic");
}

/// SegmentTree build on ~1 MB HTML should complete well under 10 s on
/// a typical dev machine. Measured ~2.3 s in release mode; the 10 s
/// ceiling is a catastrophic-regression bound with comfortable
/// headroom, not a micro-benchmark.
#[test]
fn test_large_file_segment_tree() {
    let handle = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let html = build_1mb_html_blob();
            let mut tok = make_tokenizer();
            let tokens = tok.tokenize(&html).expect("tokenize 1 MB blob");

            let t0 = Instant::now();
            let _tree = SegmentTree::new(tokens).expect("SegmentTree::new on valid tokens");
            let elapsed = t0.elapsed();

            assert!(
                elapsed.as_secs_f64() < 10.0,
                "SegmentTree build took {:?}, expected <10s",
                elapsed
            );
            eprintln!(
                "[baseline] rust SegmentTree build 1 MB: {:.2}s",
                elapsed.as_secs_f64()
            );
        })
        .expect("spawn segtree thread");

    handle.join().expect("segtree thread did not panic");
}

/// `generate_placeholders_for_segments` on ~1 MB HTML should complete
/// in under 1 s on a typical dev machine.
///
/// `build_full_value` is only called at (a) a Multiple node with at
/// least one direct Translatable child — and that branch emits a
/// single placeholder for the whole subtree and does **not** descend
/// further — or (b) a Translatable/Dummy leaf. Because
/// Multiple-with-Translatable cuts the walk off, the
/// `build_full_value` call sites operate on disjoint subtrees, so
/// total work is linear in the node count. This test exists as an
/// empirical witness: the 1 s bound fires immediately if a future
/// change reintroduces quadratic behavior.
#[test]
fn test_large_file_placeholders_under_1s() {
    let handle = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let html = build_1mb_html_blob();
            let mut tok = make_tokenizer();
            let tokens = tok.tokenize(&html).expect("tokenize 1 MB blob");
            let tree = SegmentTree::new(tokens).expect("SegmentTree::new on valid tokens");

            let t0 = Instant::now();
            let mut segments: Vec<(usize, String)> = Vec::new();
            let _ = tree
                .tree
                .generate_placeholders_for_segments("", 0, &mut segments);
            let elapsed = t0.elapsed();

            eprintln!(
                "[baseline] rust generate_placeholders_for_segments 1 MB: {:.3}s ({} segments)",
                elapsed.as_secs_f64(),
                segments.len()
            );
            assert!(
                elapsed.as_secs_f64() < 1.0,
                "generate_placeholders_for_segments took {:?}, expected <1s",
                elapsed
            );
        })
        .expect("spawn placeholders thread");

    handle.join().expect("placeholders thread did not panic");
}

// --------------------------------------------------------------------------- //
// 3. Round-trip property test                                                 //
// --------------------------------------------------------------------------- //

/// For any input that does not trigger the `." <-> ".` / `.' <-> '.` normalization in
/// `Tokenizer::tokenize` (tokenizer.rs:116), the concatenated token text should recover
/// the input byte-for-byte.
#[test]
fn test_tokenize_roundtrip() {
    let inputs = [
        "Hello World",
        "<p>Simple paragraph.</p>",
        "<div><p>Nested <em>emphasis</em> inside paragraph.</p></div>",
        "<html><body><h1>Title</h1><p>Body text with <a href='#'>link</a>.</p></body></html>",
        "Line one.\nLine two.\nLine three.",
    ];

    let mut tok = make_tokenizer();
    for input in inputs {
        let tokens = tok.tokenize(input).expect("tokenize roundtrip input");
        let recovered: String = tokens
            .iter()
            .map(|t| t["text"].as_str().unwrap_or("").to_string())
            .collect();
        assert_eq!(
            recovered, input,
            "round-trip failed:\n  in:  {:?}\n  out: {:?}",
            input, recovered
        );
    }
}

// --------------------------------------------------------------------------- //
// 4. Duplicate-type correction fixtures                                       //
// --------------------------------------------------------------------------- //
//
// These three inputs exercise the three duplicate-`type` rule pairs in
// config/rules/default.json:
//
//   | type          | variant A            | variant B                 |
//   |---------------|----------------------|---------------------------|
//   | dash          | BREAK ` - `          | PUNCTUATION `-`           |
//   | vertical_bar  | BREAK ` | `          | PUNCTUATION `|`           |
//   | xml_entity    | SYMBOL `{foo-bar}`   | SINGLE_TAG `&abc;`        |
//
// The loader returns `Vec<RegexPattern>` (preserving both entries)
// and the tokenizer callback table is keyed on `class__type`, so
// duplicate-type rules each reach the combined alternation regex and
// dispatch to their own callback. These tests pin that contract.

fn tokens_of_class_type<'a>(
    tokens: &'a [indexmap::IndexMap<String, serde_json::Value>],
    class: &str,
    ty: &str,
) -> Vec<&'a indexmap::IndexMap<String, serde_json::Value>> {
    tokens
        .iter()
        .filter(|t| {
            t.get("token_class").and_then(|v| v.as_str()) == Some(class)
                && t.get("token_type").and_then(|v| v.as_str()) == Some(ty)
        })
        .collect()
}

#[test]
fn test_duplicate_type_dash_break() {
    let mut tok = make_tokenizer();
    let tokens = tok.tokenize("sentence - another sentence").expect("tokenize");
    let breaks = tokens_of_class_type(&tokens, "BREAK", "dash");
    assert_eq!(
        breaks.len(),
        1,
        "expected one BREAK/dash token for ' - ', got {}: {:?}",
        breaks.len(),
        tokens
    );
    assert_eq!(breaks[0]["text"].as_str(), Some(" - "));
}

#[test]
fn test_duplicate_type_vertical_bar_break() {
    let mut tok = make_tokenizer();
    let tokens = tok.tokenize("item | other item").expect("tokenize");
    let breaks = tokens_of_class_type(&tokens, "BREAK", "vertical_bar");
    assert_eq!(
        breaks.len(),
        1,
        "expected one BREAK/vertical_bar token for ' | ', got {}: {:?}",
        breaks.len(),
        tokens
    );
    assert_eq!(breaks[0]["text"].as_str(), Some(" | "));
}

#[test]
fn test_duplicate_type_xml_entity_symbol() {
    // After the fix, this rule will also be renamed to `curly_placeholder` in a separate
    // rule-file commit; at that point, update the expected token_type here.
    let mut tok = make_tokenizer();
    let tokens = tok
        .tokenize("see {placeholder-name} here")
        .expect("tokenize");
    let symbols = tokens_of_class_type(&tokens, "SYMBOL", "xml_entity");
    assert_eq!(
        symbols.len(),
        1,
        "expected one SYMBOL/xml_entity token for '{{placeholder-name}}', got {}: {:?}",
        symbols.len(),
        tokens
    );
    assert!(
        symbols[0]["text"]
            .as_str()
            .unwrap_or("")
            .contains("{placeholder-name}"),
        "SYMBOL token text should contain '{{placeholder-name}}': {:?}",
        symbols[0]
    );
}

#[test]
fn test_duplicate_type_xml_entity_single_tag() {
    // `&amp;` — SINGLE_TAG/xml_entity must coexist with SYMBOL/xml_entity and dispatch
    // its own callback. This test pins the duplicate-type contract for the SINGLE_TAG
    // variant; the SYMBOL variant is covered by `test_duplicate_type_xml_entity_symbol`.
    let mut tok = make_tokenizer();
    let tokens = tok.tokenize("see &amp; here").expect("tokenize");
    let singles = tokens_of_class_type(&tokens, "SINGLE_TAG", "xml_entity");
    assert_eq!(
        singles.len(),
        1,
        "expected one SINGLE_TAG/xml_entity token for '&amp;', got {}: {:?}",
        singles.len(),
        tokens
    );
    assert_eq!(singles[0]["text"].as_str(), Some("&amp;"));
}

/// The `texplitter_config_editor/` rule-reorder UI depends on insertion order being
/// preserved all the way from the JSON file to the combined alternation regex. This
/// test proves order matters end-to-end: construct two tokenizers from inline rule
/// lists that differ only in the order of two same-`type` rules whose regexes match
/// the *same span at the same position*, and verify that the first-listed rule wins.
///
/// Note: to exercise order-based tie-breaking specifically (rather than regex
/// leftmost-longest match), both patterns must match exactly the same span at the
/// same position. The BREAK/dash vs PUNCTUATION/dash real-world case has different
/// span widths (`\s-\s` vs `-`), so leftmost-longest wins on overlap and order
/// doesn't matter there. Here we use two rules with identical regexes
/// (`placeholder`) and differing classes to hit the tie-break path explicitly.
#[test]
fn test_duplicate_type_rule_order_matters() {
    use texplitter_rs::parserrules::RegexPattern;

    fn rule(class: &str, ty: &str, regex: &str) -> RegexPattern {
        RegexPattern {
            class: class.to_string(),
            r#type: ty.to_string(),
            regex: regex.to_string(),
            callback: "default_callback".to_string(),
            description: None,
            sample: None,
        }
    }

    // Two rules that match the exact same literal `placeholder`, differing only in class.
    let first_wins_symbol = vec![
        rule("SYMBOL", "placeholder", r"placeholder"),
        rule("TRANSLATABLE", "placeholder", r"placeholder"),
    ];
    let first_wins_translatable = vec![
        rule("TRANSLATABLE", "placeholder", r"placeholder"),
        rule("SYMBOL", "placeholder", r"placeholder"),
    ];

    let mut tok_a = Tokenizer::new(first_wins_symbol).expect("Tokenizer::new SYMBOL-first");
    let tokens = tok_a.tokenize("placeholder").expect("tokenize SYMBOL-first");
    assert_eq!(tokens.len(), 1, "expected one token, got {:?}", tokens);
    assert_eq!(
        tokens[0]["token_class"].as_str(),
        Some("SYMBOL"),
        "with SYMBOL/placeholder first, tokenization must dispatch to SYMBOL: {:?}",
        tokens
    );

    let mut tok_b =
        Tokenizer::new(first_wins_translatable).expect("Tokenizer::new TRANSLATABLE-first");
    let tokens = tok_b
        .tokenize("placeholder")
        .expect("tokenize TRANSLATABLE-first");
    assert_eq!(tokens.len(), 1, "expected one token, got {:?}", tokens);
    assert_eq!(
        tokens[0]["token_class"].as_str(),
        Some("TRANSLATABLE"),
        "with TRANSLATABLE/placeholder first, tokenization must dispatch to TRANSLATABLE: {:?}",
        tokens
    );
}
