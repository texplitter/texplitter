import os
import logging
from openai import OpenAI

# Set up logging
logging.basicConfig(level=logging.INFO)

# ---------------------------------------------------------------------------
# LLM configuration
#
# Any OpenAI-compatible API works (OpenAI, Ollama, Together, vLLM,
# LiteLLM, etc.).
#
#   TEXPLITTER_LLM_API_KEY   — API key (required to enable the assistant)
#   TEXPLITTER_LLM_BASE_URL  — base URL (default: OpenAI's public endpoint)
#   TEXPLITTER_LLM_MODEL     — model name (default: gpt-4o)
#
# Legacy: TEXPLITTER_OPENAI_CREDENTIAL is accepted as a fallback for
# TEXPLITTER_LLM_API_KEY.
# ---------------------------------------------------------------------------

api_key = (
    os.environ.get("TEXPLITTER_LLM_API_KEY")
    or os.environ.get("TEXPLITTER_OPENAI_CREDENTIAL")
)
base_url = os.environ.get("TEXPLITTER_LLM_BASE_URL")  # None → OpenAI default
model = os.environ.get("TEXPLITTER_LLM_MODEL", "gpt-4o")

client = None
if api_key:
    try:
        kwargs = {"api_key": api_key}
        if base_url:
            kwargs["base_url"] = base_url
        client = OpenAI(**kwargs)
        logging.info(
            "LLM client initialized (model=%s, base_url=%s).",
            model,
            base_url or "default",
        )
    except Exception as e:
        logging.error(f"Failed to initialize LLM client: {e}")
else:
    logging.warning(
        "No LLM API key found (set TEXPLITTER_LLM_API_KEY). "
        "AI Assistant feature will be disabled."
    )

EXAMPLE_DESCRIPTION = (
    "This regular expression matches a sequence of lowercase letters, "
    "potentially followed by apostrophes or right single quotation marks "
    "and more lowercase letters. It ensures that the match is followed by "
    "specific characters or sequences, such as whitespace, punctuation, "
    "or the end of the string."
)

EXAMPLE_REGEX = r"""[\p{Ll}]+                # One or more lowercase letters
(                        # Start of group for apostrophe and following lowercase letters
    [\'']                # Apostrophe or right single quotation mark
    \p{Ll}+              # One or more lowercase letters
)*                       # Zero or more occurrences of the group
(?=                      # Positive lookahead to ensure the match is followed by specific characters
    [\s</&\(\)\[\]\{\}\p{Pf}\p{Pi}\"\'-]  # Whitespace, punctuation, or special characters
    |                    # OR
    [\.。．,:;\"\'\?!][\"\'"\s<&>]  # Punctuation followed by specific characters
    |                    # OR
    [\.。．:?\)]$         # Punctuation at the end of the string
    | $                  # End of the string
)"""

SYSTEM_PROMPT = (
    "You are a regex expert. You always output verbose, well-commented "
    "regular expressions. Every line has an inline comment."
)


def generate_regex(description):
    prompt = f"""Generate a regular expression in Python verbose ((?x)) format with inline comments explaining each part.

Requirements:
- Use the `regex` module (not `re`), so Unicode properties like \\p{{Ll}} are available.
- Each line must have an inline comment (# ...) explaining what it does.
- Use indentation to show grouping structure.
- Output ONLY the pattern, no explanation or code fences.

Example input:
Description: {EXAMPLE_DESCRIPTION}

Example output:
{EXAMPLE_REGEX}

Now generate a pattern for this description:
{description}
"""

    response = client.chat.completions.create(
        model=model,
        messages=[
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": prompt},
        ],
        max_tokens=500,
        temperature=0.5,
    )

    regex_pattern = response.choices[0].message.content.strip()
    regex_pattern = regex_pattern.strip("```regex").strip("```").strip()
    return regex_pattern


def generate_description(regex):
    prompt = f"""Based on the following regular expression, generate a clear and concise description that summarizes what it matches. The description should just describe what the regular expression does, nothing else.

Example:
Regex:
{EXAMPLE_REGEX}

Description:
{EXAMPLE_DESCRIPTION}

Now describe this regex:
Regex:
{regex}

Description:
"""

    response = client.chat.completions.create(
        model=model,
        messages=[
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": prompt},
        ],
        max_tokens=500,
        temperature=0.5,
    )

    description = response.choices[0].message.content.strip()
    return description
