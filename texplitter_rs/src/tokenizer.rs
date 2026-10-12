use fancy_regex::Regex;
use serde_json::Value;
use std::error::Error;
use std::fmt;
use log::info;
use crate::callbacks::Callback;
use crate::counter::Counter;
use crate::parserrules::RegexPattern;
use indexmap::IndexMap;

#[derive(Debug)]
pub struct UnknownTokenError {
    token: String,
}

impl UnknownTokenError {
    pub fn new(token: &str) -> UnknownTokenError {
        UnknownTokenError {
            token: token.to_string(),
        }
    }
}

impl fmt::Display for UnknownTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Found token: {}", self.token)
    }
}

impl Error for UnknownTokenError {}

#[derive(Debug)]
pub struct Tokenizer {
    regexc: Regex,
    /// Callback table keyed on `CLASS__type` (see `RegexPattern::class_type`).
    /// `Callback` is a plain copy-type enum — dispatch is a `match` on a fixed set of
    /// named callbacks, so there is no heap allocation per rule and no runtime mutex
    /// on the dispatch path. Stored as a Vec of `(key, callback)` pairs instead of an
    /// `IndexMap` because lookup is O(n) against ~55 entries either way; a linear
    /// scan over a slice is faster for these sizes and avoids hashing a fresh
    /// `String` per token.
    rules: Vec<(String, Callback)>,
    tokens: Vec<IndexMap<String, Value>>,
    unmatched_tags: Vec<IndexMap<String, Value>>,
    nesting_errors: Vec<IndexMap<String, Value>>,
    error: bool,
    symbol_counter: Counter,
    /// Move a period or comma written inside closing quotes after them, as
    /// American English writes it and most other languages do not. On by
    /// default; off for a source already in logical order.
    move_punctuation_after_quotes: bool,
}

/// Moves a period or comma that sits just before a closing quote (`"`, `'`,
/// `”`, `’`) after it: American English `"the end."` becomes `"the end".`,
/// the order of Spanish (RAE) and most other languages. Text only: nothing
/// between `<` and `>` is touched, quoted attribute values included, so a
/// tag such as `<p title="Title.">` keeps its closing `">`.
pub fn move_punctuation_after_quotes(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    let mut value_quote: Option<char> = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_tag {
            out.push(c);
            match value_quote {
                Some(q) if c == q => value_quote = None,
                Some(_) => {}
                None if c == '"' || c == '\'' => value_quote = Some(c),
                None if c == '>' => in_tag = false,
                None => {}
            }
            i += 1;
            continue;
        }
        let opens_tag = c == '<'
            && chars
                .get(i + 1)
                .map_or(false, |n| n.is_ascii_alphabetic() || matches!(n, '/' | '!' | '?'));
        if opens_tag {
            in_tag = true;
            out.push(c);
            i += 1;
            continue;
        }
        if (c == '.' || c == ',') && chars.get(i + 1).map_or(false, |n| matches!(n, '"' | '\'' | '\u{201D}' | '\u{2019}')) {
            out.push(chars[i + 1]);
            out.push(c);
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

impl Tokenizer {
    /// Build a Tokenizer from an ordered rule list.
    ///
    /// The callback table is keyed on `"{CLASS}__{type}"`, not on `type` alone.
    /// Keying on `type` silently collapses duplicate-`type` rules (e.g., `BREAK/dash`
    /// and `PUNCTUATION/dash` share the same `type` but have different classes and
    /// must dispatch to different callbacks) — that bug is the reason this function
    /// takes a `Vec<RegexPattern>` now instead of an `IndexMap<String, Value>` keyed
    /// on `type`.
    pub fn new(regex_patterns: Vec<RegexPattern>) -> Result<Tokenizer, Box<dyn Error>> {
        let mut rules: Vec<(String, Callback)> = Vec::with_capacity(regex_patterns.len());
        let mut parts = Vec::with_capacity(regex_patterns.len());

        for pattern in &regex_patterns {
            // Handle unsupported Unicode property. fancy-regex doesn't recognize the
            // `InCJK_Symbols_and_Punctuation` block name; rewrite it to the explicit
            // codepoint range. Keep this rewrite here (not in the rule loader) so the
            // on-disk rule file stays readable.
            let regex_str = pattern.regex.replace(
                r"\p{InCJK_Symbols_and_Punctuation}",
                r"[\u3000-\u303F]",
            );

            let class_type = pattern.class_type();
            parts.push(format!("(?P<{}>{})", class_type, regex_str));

            // Key the callback table on CLASS__type, not on type alone. Two rules that
            // share a `type` but have different classes must dispatch to different
            // callbacks; keying on `type` would silently clobber the first variant.
            rules.push((class_type, Callback::from_name(&pattern.callback)));
        }

        // `(?s)` = dot matches newlines; `(?x)` = extended mode, whitespace and
        // `# ...` comments ignored outside character classes. Combined mode replaces
        // the old hand-rolled `preprocess_regex` pass.
        let regex_pattern = format!("(?sx){}", parts.join("|"));
        let regexc = Regex::new(&regex_pattern)?;

        Ok(Tokenizer {
            regexc,
            rules,
            tokens: vec![],
            unmatched_tags: vec![],
            nesting_errors: vec![],
            error: false,
            symbol_counter: Counter::new(),
            move_punctuation_after_quotes: true,
        })
    }

    /// Look up the callback for a given `CLASS__type` key. Linear scan — see the
    /// note on `Tokenizer::rules`. Returns `Callback::Default` for unknown keys.
    fn lookup_callback(&self, class_type_key: &str) -> Callback {
        for (key, cb) in &self.rules {
            if key == class_type_key {
                return *cb;
            }
        }
        Callback::Default
    }

    /// Whether a period or comma inside closing quotes moves after them.
    pub fn set_move_punctuation_after_quotes(&mut self, value: bool) {
        self.move_punctuation_after_quotes = value;
    }

    pub fn tokenize(&mut self, intext: &str) -> Result<Vec<IndexMap<String, Value>>, Box<dyn Error>> {
        info!("Starting tokenization");
        let intext = if self.move_punctuation_after_quotes {
            move_punctuation_after_quotes(intext)
        } else {
            intext.to_string()
        };
        let mut _position = 0;
        self.tokens.clear();
        self.unmatched_tags.clear();
        self.nesting_errors.clear();
        self.error = false;
        let mut tag_stack: Vec<IndexMap<String, Value>> = vec![];

        for result in self.regexc.captures_iter(&intext) {
            let captures = result?; // Handle potential errors here
            // Structural invariant: `captures_iter` only yields `Ok(captures)`
            // where at least group 0 (the full match) exists, so `get(0)`
            // cannot be None here.
            let whole = captures.get(0).unwrap();
            let value = whole.as_str();
            let start = whole.start();
            let end = whole.end();
            _position = end;

            let mut token_class = "";
            let mut token_type = "";
            let mut class_type_key = String::new();

            for name in self.regexc.capture_names().flatten() {
                if let Some(m) = captures.name(name) {
                    if m.as_str() == value {
                        let parts: Vec<&str> = name.splitn(2, "__").collect();
                        token_class = parts[0];
                        token_type = parts.get(1).copied().unwrap_or("");
                        class_type_key = name.to_string();
                        break;
                    }
                }
            }

            // Callback dispatch is keyed on `CLASS__type`, not `type` alone, so that
            // `BREAK/dash` and `PUNCTUATION/dash` look up different callbacks. Keying
            // on `type` alone was the second half of the duplicate-type rule collapse
            // bug — even if the loader preserved both rules, the old dispatch here
            // would still have collapsed them.
            //
            // `symbol` is the one callback that can't live in the `Callback` enum
            // because it needs `&mut self.symbol_counter`. Inlined here rather than
            // giving the enum a `&mut Counter` parameter the other 9 variants would
            // never touch.
            let (text, label) = if token_type == "symbol" {
                let label = format!("@SYM{}", self.symbol_counter.next());
                (value.to_string(), label)
            } else {
                self.lookup_callback(&class_type_key).apply(value)
            };

            let mut token = IndexMap::new();
            token.insert("token_type".to_string(), Value::String(token_type.to_string()));
            token.insert("token_class".to_string(), Value::String(token_class.to_string()));
            token.insert("text".to_string(), Value::String(text));
            token.insert("label".to_string(), Value::String(label));
            token.insert("start".to_string(), Value::Number(start.into()));
            token.insert("end".to_string(), Value::Number(end.into()));

            self.tokens.push(token.clone());

            match token_class {
                "OPEN_TAG" | "BEGINNING_TAG" => {
                    tag_stack.push(token.clone());
                }
                "END_CLOSE_TAG" => {
                    if let Some(last) = tag_stack.last() {
                        if last["token_class"] == "BEGINNING_TAG" {
                            tag_stack.pop();
                        } else {
                            self.unmatched_tags.push(token.clone());
                        }
                    }
                }
                "CLOSE_TAG" => {
                    if let Some(last) = tag_stack.pop() {
                        if last["label"] != token["label"] {
                            self.unmatched_tags.push(token.clone());
                            self.nesting_errors.push(token.clone());
                            tag_stack.push(last); // Put the unmatched opening tag back on the stack
                        }
                    } else {
                        self.unmatched_tags.push(token.clone());
                        self.nesting_errors.push(token.clone());
                    }
                }
                _ => {}
            }
        }

        while let Some(tag) = tag_stack.pop() {
            self.unmatched_tags.push(tag.clone());
            self.nesting_errors.push(tag);
        }

        self.error = !self.unmatched_tags.is_empty();
        info!("Error state: {}", self.error);

        // Filter out matched tags
        let mut open_tags = IndexMap::new();
        let mut close_tags = IndexMap::new();
        let mut matched_tags = vec![];

        for tag in &self.unmatched_tags {
            // Structural invariant: `unmatched_tags` is populated only from
            // tokens this function builds above, where `label` is always
            // inserted as `Value::String(label)` — so `as_str()` cannot be
            // None.
            let label = tag["label"].as_str().unwrap();
            if tag["token_class"] == "OPEN_TAG" || tag["token_class"] == "BEGINNING_TAG" {
                *open_tags.entry(label).or_insert(0) += 1;
            } else if tag["token_class"] == "CLOSE_TAG" {
                *close_tags.entry(label).or_insert(0) += 1;
            }
        }

        for (label, count) in open_tags.iter() {
            if let Some(close_count) = close_tags.get(label) {
                let matched_count = count.min(close_count);
                for _ in 0..*matched_count {
                    for tag in &self.unmatched_tags {
                        if tag["label"] == *label {
                            matched_tags.push(tag.clone());
                            break;
                        }
                    }
                }
            }
        }

        self.unmatched_tags.retain(|tag| !matched_tags.contains(tag));
        self.nesting_errors.retain(|tag| !matched_tags.contains(tag));

        info!("Finished tokenization with {} unmatched tags and {} nesting errors", self.unmatched_tags.len(), self.nesting_errors.len());

        Ok(self.tokens.clone())
    }

    pub fn get_unmatched_tags(&self) -> &Vec<IndexMap<String, Value>> {
        &self.unmatched_tags
    }

    pub fn get_nesting_errors(&self) -> &Vec<IndexMap<String, Value>> {
        &self.nesting_errors
    }
}
