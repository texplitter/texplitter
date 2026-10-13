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

/// HTML's void elements, which never take content or a closing tag.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr",
];

/// Marks a void element written without a slash (`<meta charset="utf-8">`,
/// `<img src="x.png">`, `<hr>`) as self-closing, as `<img ... />` is. An
/// element with a void name whose closing tag follows before its next
/// opening (`<link>https://...</link>` in RSS and other XML) holds content
/// and is left as it is.
/// A bracket mark without its partner is text, not markup: a closing mark with
/// no open one, and an opening mark never closed, become punctuation, so the
/// text around them is never lost and a lone `*` or `_` stays as written.
fn settle_brackets(tokens: &mut [IndexMap<String, Value>]) {
    let mut open: Vec<usize> = Vec::new();
    let mut unmatched: Vec<usize> = Vec::new();
    for i in 0..tokens.len() {
        match tokens[i]["token_class"].as_str().unwrap_or("") {
            "OPEN_BRACKET" => open.push(i),
            "CLOSE_BRACKET" => {
                if open.pop().is_none() {
                    unmatched.push(i);
                }
            }
            _ => {}
        }
    }
    unmatched.extend(open);
    for i in unmatched {
        tokens[i].insert("token_class".to_string(), Value::String("PUNCTUATION".to_string()));
    }
}

fn mark_void_elements(tokens: &mut [IndexMap<String, Value>]) {
    let class_of = |t: &IndexMap<String, Value>| t["token_class"].as_str().unwrap_or("").to_string();
    let name_of = |t: &IndexMap<String, Value>| t["label"].as_str().unwrap_or("").to_ascii_lowercase();
    let mut next_close: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut next_open: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut void_openings: Vec<usize> = Vec::new();
    for i in (0..tokens.len()).rev() {
        let class = class_of(&tokens[i]);
        let name = name_of(&tokens[i]);
        if !VOID_ELEMENTS.contains(&name.as_str()) {
            continue;
        }
        match class.as_str() {
            "CLOSE_TAG" => {
                next_close.insert(name, i);
            }
            "OPEN_TAG" | "BEGINNING_TAG" => {
                let closed = match (next_close.get(&name), next_open.get(&name)) {
                    (Some(c), Some(o)) => c < o,
                    (Some(_), None) => true,
                    _ => false,
                };
                if !closed {
                    void_openings.push(i);
                }
                next_open.insert(name, i);
            }
            _ => {}
        }
    }
    for i in void_openings {
        if class_of(&tokens[i]) == "OPEN_TAG" {
            tokens[i].insert("token_class".to_string(), Value::String("SINGLE_TAG".to_string()));
        } else if let Some(end) = (i + 1..tokens.len()).find(|&j| matches!(class_of(&tokens[j]).as_str(), "END_OPEN_TAG" | "END_CLOSE_TAG")) {
            if class_of(&tokens[end]) == "END_OPEN_TAG" {
                tokens[end].insert("token_class".to_string(), Value::String("END_CLOSE_TAG".to_string()));
            }
        }
    }
}

/// Moves a period or comma that sits just before a closing quote (`"`, `'`,
/// `”`, `’`) after it: American English `"the end."` becomes `"the end".`,
/// the order of Spanish (RAE) and most other languages. Text only: nothing
/// between `<` and `>` is touched, quoted attribute values included, so a
/// tag such as `<p title="Title.">` keeps its closing `">`.
pub fn move_punctuation_after_quotes(text: &str) -> String {
    move_punctuation_after_quotes_outside(text, &[])
}

/// The move, leaving the byte ranges in `protected` (sorted, disjoint) as
/// they are. The tag state carries across a protected range, so an entity
/// inside an attribute value does not make the rest of the tag text.
pub fn move_punctuation_after_quotes_outside(text: &str, protected: &[(usize, usize)]) -> String {
    let (chars, offsets): (Vec<char>, Vec<usize>) = text.char_indices().map(|(o, c)| (c, o)).unzip();
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    let mut value_quote: Option<char> = None;
    let mut next_protected = 0;
    let mut i = 0;
    while i < chars.len() {
        if let Some(&(start, end)) = protected.get(next_protected) {
            if offsets[i] >= start {
                // The protected range, as written; the tag state is untouched.
                out.push_str(&text[start..end]);
                while i < chars.len() && offsets[i] < end {
                    i += 1;
                }
                next_protected += 1;
                continue;
            }
        }
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

    /// The tokens of text, by the rules, in order: each with its class, type, text, label and byte offsets.
    fn scan(&mut self, intext: &str) -> Result<Vec<IndexMap<String, Value>>, Box<dyn Error>> {
        let mut tokens = Vec::new();
        self.symbol_counter.reset();
        for result in self.regexc.captures_iter(intext) {
            let captures = result?; // Handle potential errors here
            // Structural invariant: `captures_iter` only yields `Ok(captures)`
            // where at least group 0 (the full match) exists, so `get(0)`
            // cannot be None here.
            let whole = captures.get(0).unwrap();
            let value = whole.as_str();
            let start = whole.start();
            let end = whole.end();

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

            tokens.push(token);
        }
        Ok(tokens)
    }

    pub fn tokenize(&mut self, intext: &str) -> Result<Vec<IndexMap<String, Value>>, Box<dyn Error>> {
        info!("Starting tokenization");
        let intext = if self.move_punctuation_after_quotes {
            // The move is for prose. A protected token (a code block, a code span,
            // an attribute line) keeps its bytes: the text is scanned once to find
            // them, the move is applied to the text between them, and the result
            // is scanned again.
            let protected: Vec<(usize, usize)> = self
                .scan(intext)?
                .iter()
                .filter(|t| matches!(t["token_class"].as_str(), Some("BREAK_TEMPLATE") | Some("SINGLE_TAG")))
                .map(|t| (t["start"].as_u64().unwrap() as usize, t["end"].as_u64().unwrap() as usize))
                .collect();
            move_punctuation_after_quotes_outside(intext, &protected)
        } else {
            intext.to_string()
        };
        self.tokens = self.scan(&intext)?;
        self.unmatched_tags.clear();
        self.nesting_errors.clear();
        self.error = false;
        let mut tag_stack: Vec<IndexMap<String, Value>> = vec![];


        // Void elements first: their end decides how the nesting is read.
        mark_void_elements(&mut self.tokens);
        // A bracket mark without its partner is text, not markup.
        settle_brackets(&mut self.tokens);

        // Nesting check over the final tokens, in source order.
        let final_tokens = self.tokens.clone();
        for token in &final_tokens {
            let token_class = token["token_class"].as_str().unwrap_or("");
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
