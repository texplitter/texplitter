use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegexPattern {
    pub class: String,
    pub r#type: String,
    pub regex: String,
    pub callback: String,
    pub description: Option<String>, // Include other fields if necessary
    pub sample: Option<String>,
}

impl RegexPattern {
    /// The composite identifier used for named capture groups and callback dispatch:
    /// `"{CLASS}__{type}"`. This is the honest key — keying only on `type` silently
    /// collapses rules like `BREAK/dash` and `PUNCTUATION/dash` into the same slot.
    pub fn class_type(&self) -> String {
        format!("{}__{}", self.class, self.r#type)
    }
}

/// Load the rule file as an ordered `Vec<RegexPattern>`.
///
/// Returning a `Vec` (rather than a map keyed on `type`) preserves the
/// file order — which is load-bearing for first-match-wins
/// tokenization — and allows duplicate-`type` rules like `BREAK/dash`
/// and `PUNCTUATION/dash` to coexist without clobbering each other.
///
/// Rule regexes are returned verbatim. They are expected to be compiled
/// under `fancy-regex`'s native extended mode (`(?x)`), which ignores
/// whitespace and `# ...` comments outside character classes.
pub fn load_regex_patterns<P: AsRef<Path>>(path: P) -> Result<Vec<RegexPattern>, Box<dyn std::error::Error>> {
    let file_content = fs::read_to_string(path)?;
    let patterns: Vec<RegexPattern> = serde_json::from_str(&file_content)?;
    Ok(patterns)
}
