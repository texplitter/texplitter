use texplitter_rs::parserrules::{load_regex_patterns, RegexPattern};
use fancy_regex::Regex;

fn build_regex_pattern(regex_patterns: &[RegexPattern]) -> Result<String, Box<dyn std::error::Error>> {
    let mut parts = vec![];

    for pattern in regex_patterns {
        // Handle unsupported Unicode property
        let regex_str = pattern.regex.replace(
            r"\p{InCJK_Symbols_and_Punctuation}",
            r"[\u3000-\u303F]",
        );

        let class_type = pattern.class_type();
        let part = format!("(?P<{}>{})", class_type, regex_str);
        parts.push(part);
    }

    let regex_pattern = format!("(?sx){}", parts.join("|"));
    Ok(regex_pattern)
}

#[test]
fn test_generated_regex() {
    let patterns = load_regex_patterns("../src/texplitter/config/rules/default.json").unwrap();

    let regex_pattern = build_regex_pattern(&patterns).expect("Failed to build regex pattern");
    let regex = Regex::new(&regex_pattern).expect("Failed to create regex");

    let sample = "<div class=\"container\">";
    match regex.captures(sample) {
        Ok(Some(captures)) => {
            println!("Captures:");
            for i in 0..captures.len() {
                println!("Capture {}: {:?}", i, captures.get(i).map(|m| m.as_str()));
            }
            println!("Named captures:");
            for name in regex.capture_names().flatten() {
                println!("{}: {:?}", name, captures.name(name).map(|m| m.as_str()));
            }
        }
        Ok(None) => println!("No match found"),
        Err(e) => println!("Error: {:?}", e),
    }

    assert!(regex.is_match(sample).unwrap(), "The regex did not match the sample text");
}

// #[test]
// fn test_preprocessed_regex_patterns() {
//     let patterns = load_regex_patterns("../src/texplitter/config/rules/default.json").unwrap();

//     // Check that the patterns are preprocessed correctly
//     let expected_patterns: IndexMap<String, Value> = [
//         (
//             "japanese_dot".to_string(),
//             serde_json::json!({
//                 "type": "japanese_dot",
//                 "class": "BREAK",
//                 "description": "This regular expression matches a Japanese full stop (。) or dot (．) followed by one or more whitespace characters. ",
//                 "regex": "[。．]\\s+",
//                 "callback": "default_callback",
//                 "sample": "。 "
//             })
//         ),
//         (
//             "japanese_word".to_string(),
//             serde_json::json!({
//                 "type": "japanese_word",
//                 "class": "TRANSLATABLE",
//                 "description": "This regular expression matches one or more Japanese characters, including Hiragana, Katakana, and Kanji (Han). ",
//                 "regex": "[\\p{IsHiragana}\\p{IsKatakana}\\p{IsHan}]+",
//                 "callback": "default_callback",
//                 "sample": "こんにちは世界"
//             })
//         ),
//         (
//             "japanese_pt".to_string(),
//             serde_json::json!({
//                 "type": "japanese_pt",
//                 "class": "PUNCTUATION",
//                 "description": "This regular expression matches one or more characters that belong to the Unicode block 'CJK Symbols and Punctuation'.  ",
//                 "regex": "[\\u3000-\\u303F]+",
//                 "callback": "default_callback",
//                 "sample": "〆々々"
//             })
//         )
//     ].iter().cloned().collect();

//     for (key, expected_pattern) in expected_patterns {
//         let pattern = patterns.get(&key).unwrap();
//         assert_eq!(pattern, &expected_pattern);
//     }
// }

