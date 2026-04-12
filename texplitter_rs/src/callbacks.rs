use indexmap::IndexMap;

/// Callback identifier, resolved from the rule's `callback` string field once
/// at load time. Dispatch is a plain `match` — no `Arc<Mutex<dyn Fn>>`, no
/// heap allocation per token, no atomic refcount bumps, no poisoned-mutex panics.
///
/// `Symbol` is handled out-of-band in the tokenizer because it needs `&mut Counter`
/// state that none of the other variants use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Callback {
    Default,
    Bracket,
    Punctuation,
    QuestionMark,
    Whitespace,
    MidTag,
    OpenTag,
    CloseTag,
    LeftTag,
}

impl Callback {
    /// Resolve the rule file's `callback` string to a `Callback` variant.
    /// Unknown names silently fall back to `Default`, matching the old
    /// `Arc::new(Mutex::new(default_callback))` fallback in `Tokenizer::new`.
    pub fn from_name(name: &str) -> Callback {
        match name {
            "default_callback" => Callback::Default,
            "bracket_callback" => Callback::Bracket,
            "punctuation_callback" => Callback::Punctuation,
            "question_mark_callback" => Callback::QuestionMark,
            "whitespace_callback" => Callback::Whitespace,
            "mid_tag_callback" => Callback::MidTag,
            "open_tag_callback" => Callback::OpenTag,
            "close_tag_callback" => Callback::CloseTag,
            "left_tag_callback" => Callback::LeftTag,
            _ => Callback::Default,
        }
    }

    /// Apply the callback to a matched token. Returns `(text, label)`.
    pub fn apply(self, token: &str) -> (String, String) {
        match self {
            Callback::Default => default_callback(token),
            Callback::Bracket => bracket_callback(token),
            Callback::Punctuation => punctuation_callback(token),
            Callback::QuestionMark => question_mark_callback(token),
            Callback::Whitespace => whitespace_callback(token),
            Callback::MidTag => mid_tag_callback(token),
            Callback::OpenTag => open_tag_callback(token),
            Callback::CloseTag => close_tag_callback(token),
            Callback::LeftTag => left_tag_callback(token),
        }
    }
}

pub fn default_callback(token: &str) -> (String, String) {
    (token.to_string(), token.to_string())
}

pub fn bracket_callback(token: &str) -> (String, String) {
    let bracket_map: IndexMap<&str, &str> = [
        ("(", "opnbr"), (")", "clsbr"),
        ("[", "opnsqbr"), ("]", "clssqbr"),
        ("{", "opncrlybr"), ("}", "clscrlybr")
    ].iter().cloned().collect();
    let label = bracket_map.get(token.trim()).unwrap_or(&token).to_string();
    (label, token.to_string())
}

pub fn punctuation_callback(token: &str) -> (String, String) {
    let text = format!(" {}", token);
    (text, token.to_string())
}

pub fn question_mark_callback(token: &str) -> (String, String) {
    let text = format!("{} ", token);
    (text, token.to_string())
}

pub fn whitespace_callback(token: &str) -> (String, String) {
    let text = token.to_string();
    let label = token.len().to_string();
    (text, label)
}

pub fn mid_tag_callback(token: &str) -> (String, String) {
    // Matches the Python `mid_tag_callback`:
    //   token = token.replace('\n', ' ')
    //   attribute_name = token.split(' ')[-1][:-2]
    // i.e. split on literal spaces, take the last segment, strip the
    // trailing `="` (or equivalent). `[:-2]` is a character slice in
    // Python so we drop the trailing two `char`s, not two bytes.
    let token = token.replace("\n", " ");
    let last_segment = token.rsplit(' ').next().unwrap_or("");
    let mut chars: Vec<char> = last_segment.chars().collect();
    if chars.len() >= 2 {
        chars.truncate(chars.len() - 2);
    } else {
        chars.clear();
    }
    let attribute_name: String = chars.into_iter().collect();
    (token, attribute_name)
}

pub fn open_tag_callback(token: &str) -> (String, String) {
    let token_stripped = token.trim();
    let tag_name = token_stripped[1..token_stripped.len() - 1].to_string();
    (token.to_string(), tag_name)
}

pub fn close_tag_callback(token: &str) -> (String, String) {
    let token_stripped = token.trim();
    let tag_name = token_stripped[2..token_stripped.len() - 1].to_string();
    (token.to_string(), tag_name)
}

pub fn left_tag_callback(token: &str) -> (String, String) {
    let token_stripped = token.trim();
    let tag_name = token_stripped[1..].split_whitespace().next().unwrap_or("").to_string(); // Removed the leading '<'
    (token.to_string(), tag_name)
}
