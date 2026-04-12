# Custom Rulesets

texplitter ships with two rulesets:

- **default** — 55 rules for HTML/XML, including tags, entities,
  CJK characters, URLs, emails, abbreviations, and word boundaries.
- **asciidoc** — extends the default ruleset with AsciiDoc-specific
  rules for section titles, inline formatting, attribute references,
  cross-references, and block delimiters.

You can create additional rulesets for any markup format.

## Ruleset file format

A ruleset is a JSON array of rule objects. Each rule has these keys:

| Key | Required | Description |
|-----|----------|-------------|
| `type` | yes | Unique identifier for the rule (e.g., `"open_tag"`) |
| `class` | yes | Token class to assign (e.g., `"OPEN_TAG"`, `"TRANSLATABLE"`) |
| `regex` | yes | Pattern in `(?x)` verbose mode (comments with `#`, free whitespace) |
| `callback` | yes | Callback name (see below) |
| `description` | no | Human-readable explanation of the rule |
| `sample` | no | Example text that the rule should match |

### Example rule

```json
{
    "type": "asciidoc_attribute_ref",
    "class": "SINGLE_TAG",
    "description": "Matches AsciiDoc attribute references ({name}).",
    "regex": "\\s*               # Optional leading whitespace\n\\{                # Opening curly brace\n[a-zA-Z0-9_-]+   # Attribute name\n\\}                # Closing curly brace\n\\s*               # Optional trailing whitespace\n",
    "callback": "symbol_callback",
    "sample": "{product-name}"
}
```

### Callbacks

| Callback | Behavior |
|----------|----------|
| `default_callback` | Assigns the token class directly |
| `symbol_callback` | Generates a unique label (`@SYM{N}`); use with `SYMBOL` or `SINGLE_TAG` class |
| `open_tag_callback` | Parses the tag name for tag tracking |
| `close_tag_callback` | Validates tag nesting |
| `left_tag_callback` | Handles opening tags with attributes |
| `mid_tag_callback` | Handles attribute assignments |
| `punctuation_callback` | Assigns `PUNCTUATION` |
| `question_mark_callback` | Handles inverted question/exclamation marks |
| `whitespace_callback` | Assigns `SEVERAL_SPACES` |

!!! warning "Tag callbacks expect HTML-style tags"
    The `open_tag_callback`, `close_tag_callback`, and `left_tag_callback`
    expect the matched text to be an HTML/XML-style tag (`<name>` or
    `</name>`). If your format uses different markers (like AsciiDoc's
    `*bold*`), use `default_callback` instead.

## Rule ordering

Rules are evaluated in the order they appear in the JSON file.
**First match wins**: the first rule whose regex matches at the earliest
position in the input text is used. This means:

1. **Specific rules before generic rules.** An AsciiDoc attribute
   reference `{name}` must come before the generic curly-bracket
   punctuation rule, or the brace will be tokenized as punctuation.

2. **Longer matches before shorter ones.** If two rules match at the
   same position, the one listed first wins — even if the other would
   match more text.

3. **AsciiDoc/format-specific rules at the top, generic word/punctuation
   rules at the bottom.** The generic rules from the default ruleset
   handle words, spaces, numbers, and punctuation in any language.

## Creating a ruleset

### Option 1: Config Editor (recommended)

```bash
pip install texplitter[config-editor]
texplitter config
```

The editor lets you:

- Duplicate an existing ruleset as a starting point
- Add, edit, and reorder rules with drag-and-drop
- Test patterns against sample text
- Use the AI assistant to generate regex from descriptions

See [Config Editor](config-editor.md) for details.

### Option 2: Manual JSON

1. Create a new file in `src/texplitter/config/rules/`:

    ```
    src/texplitter/config/rules/my_format.json
    ```

2. Start with the default ruleset as a template:

    ```python
    import json
    from texplitter import load_regex_patterns
    rules = load_regex_patterns("default")
    with open("my_format.json", "w") as f:
        json.dump(rules, f, indent=4)
    ```

3. Add your format-specific rules at the **beginning** of the array.

4. Load and test:

    ```python
    from texplitter import Tokenizer
    tok = Tokenizer("my_format")
    tokens = tok.tokenize("your test input")
    ```

## The AsciiDoc ruleset as an example

The `asciidoc` ruleset demonstrates how to extend the default rules
for a non-HTML format. It is provided as a sample to illustrate
custom ruleset creation and has not been fully tested in production
scenarios. Key design decisions:

- **Inline formatting** (`*bold*`, `_italic_`, `` `mono` ``) uses
  paired `OPEN_BRACKET` / `CLOSE_BRACKET` tokens for the opening and
  closing markers, so the content between them remains `TRANSLATABLE`.
  The word rules' lookahead character classes include `*_`#` so they
  can match words immediately before a closing marker, and the symbol
  catch-all excludes those characters so it doesn't swallow them.

- **Attribute references** (`{name}`), **cross-references**
  (`<<anchor>>`), **inline macros** (`link:url[text]`), **footnotes**,
  **callouts**, and **role blocks** use `SINGLE_TAG` class. This
  pushes and immediately pops the tag stack so the element is treated
  as a protected inline node — like a self-closing HTML tag. The
  attribute reference regex consumes optional surrounding whitespace
  so it wins over the generic curly-bracket punctuation rule.

- **Superscript** (`^text^`) and **subscript** (`~text~`) also use
  `SINGLE_TAG`. The entire construct (including content) is one
  protected token because the content is typically notation or
  annotation references (`^2^`, `^[1]^`, `~2~`), not translatable
  prose.

- **Block delimiters** (`----`, `====`, `****`) use `BREAK_TEMPLATE`
  class so they act as non-translatable structural boundaries.

- The **word, number, punctuation, and space rules** are inherited
  verbatim from the default ruleset at the bottom of the file.
