use serde_json::{Value, Map};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::env;

pub fn load_regex_patterns() -> Result<HashMap<String, Value>, Box<dyn std::error::Error>> {
    // The rule file lives under the Python package as package data
    // (src/texplitter/config/rules/default.json). From the texplitter_rs
    // crate root the relative path is ../src/texplitter/config/rules/default.json.
    let mut path = env::current_dir()?;
    path.push("../src/texplitter/config/rules/default.json");

    let mut file = File::open(path)?;
    let mut data = String::new();
    file.read_to_string(&mut data)?;
    let patterns: Vec<HashMap<String, Value>> = serde_json::from_str(&data)?;

    let mut combined_patterns = HashMap::new();
    for pattern in patterns {
        if let Some(pattern_type) = pattern.get("type").and_then(Value::as_str) {
            let map: Map<String, Value> = pattern.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            combined_patterns.insert(pattern_type.to_string(), Value::Object(map));
        }
    }

    Ok(combined_patterns)
}
