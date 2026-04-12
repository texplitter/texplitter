# Config Editor

texplitter includes a browser-based editor for creating and managing
tokenization rulesets.

## Installation

```bash
pip install texplitter[config-editor]
```

## Launching

```bash
texplitter config
```

This starts a FastAPI server on `http://127.0.0.1:8000` and opens
your browser. Use `--host` and `--port` to customize:

```bash
texplitter config --host 0.0.0.0 --port 9000
```

## Features

### Ruleset management

The dropdown in the navigation bar lets you switch between rulesets.
You can:

- **Create** a new empty ruleset
- **Duplicate** an existing ruleset (copies all rules)
- **Rename** a ruleset
- **Delete** a ruleset (the `default` ruleset is protected)

Your selected ruleset is remembered across browser sessions.

### Pattern editing

Each rule is shown as a row in the pattern table. Click a row to
select it, then use the toolbar buttons to:

- **Add** a new pattern (at the beginning, end, or relative to
  the selected row)
- **Edit** the selected pattern
- **Delete** the selected pattern
- **Move up/down** to change rule order (critical for
  first-match-wins behavior)

The pattern editor form has fields for:

| Field | Description |
|-------|-------------|
| Type | Unique rule identifier |
| Class | Token class to assign (dropdown with existing values) |
| Description | Human-readable explanation |
| Regex | Pattern in verbose mode (syntax-highlighted with CodeMirror) |
| Callback | Callback function name (dropdown with existing values) |
| Sample | Example text for testing |

### AI Assistant

The config editor includes an AI assistant that can generate regex
patterns from plain-English descriptions and vice versa. It works
with any OpenAI-compatible API (OpenAI, Ollama, Together, vLLM,
LiteLLM, or any provider that supports the `chat.completions`
endpoint).

- **Generate regex from description**: describe what you want to
  match in plain English and the LLM generates a regex pattern.
- **Generate description from regex**: paste a regex and get a
  human-readable explanation.

To enable, set your LLM credentials:

```bash
export TEXPLITTER_LLM_API_KEY="your-api-key"
export TEXPLITTER_LLM_BASE_URL="https://api.example.com/v1"  # optional
export TEXPLITTER_LLM_MODEL="your-model-name"                # optional, default: gpt-4o
texplitter config
```

For local models via Ollama, no API key is needed — just set the
base URL and model:

```bash
export TEXPLITTER_LLM_API_KEY="unused"
export TEXPLITTER_LLM_BASE_URL="http://localhost:11434/v1"
export TEXPLITTER_LLM_MODEL="llama3"
texplitter config
```

## Architecture

The config editor is a Vue 3 + Tailwind CSS frontend served by a
FastAPI backend. The backend reads and writes ruleset JSON files
directly in `src/texplitter/config/rules/`. All changes are saved
immediately — there is no "save" button.

### API endpoints

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/rulesets` | List available rulesets |
| POST | `/api/rulesets` | Create a new ruleset |
| DELETE | `/api/rulesets/{name}` | Delete a ruleset |
| PUT | `/api/rulesets/{name}/rename` | Rename a ruleset |
| GET | `/api/rulesets/{name}/patterns` | Get all patterns |
| POST | `/api/rulesets/{name}/patterns` | Add a pattern |
| PUT | `/api/rulesets/{name}/patterns/{index}` | Update a pattern |
| DELETE | `/api/rulesets/{name}/patterns/{index}` | Delete a pattern |
| PUT | `/api/rulesets/{name}/patterns-order` | Reorder all patterns |
