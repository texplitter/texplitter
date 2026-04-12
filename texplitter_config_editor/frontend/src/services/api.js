// API shim for the texplitter config editor.
//
// Every ruleset-scoped call takes a `ruleset` name (a file stem under
// `src/texplitter/config/rules/`) and builds the URL from it. The editor
// supports editing multiple independent rulesets; there is no
// concatenation or inheritance between them — each file is its own
// first-match-wins grammar. The backend reuses the same
// `^[a-zA-Z0-9_-]+$` guard as the library loader, so callers should pass
// raw stems (no `.json` suffix, no path separators).

function encodeRuleset(ruleset) {
  // The backend rejects anything outside `[a-zA-Z0-9_-]`, which is
  // already URL-safe, so encodeURIComponent is a belt-and-braces guard
  // against future callers that forget and pass something exotic.
  return encodeURIComponent(ruleset)
}

export async function fetchRulesets() {
  const response = await fetch('/api/rulesets')
  if (!response.ok) {
    throw new Error(`Failed to fetch rulesets: ${response.status}`)
  }
  return await response.json()
}

export async function fetchPatterns(ruleset) {
  const response = await fetch(`/api/rulesets/${encodeRuleset(ruleset)}/patterns`)
  if (!response.ok) {
    throw new Error(`Failed to fetch patterns for ${ruleset}: ${response.status}`)
  }
  return await response.json()
}

export async function fetchUniqueClassValues(ruleset) {
  const response = await fetch(`/api/rulesets/${encodeRuleset(ruleset)}/unique-classes`)
  if (!response.ok) {
    throw new Error(`Failed to fetch unique classes for ${ruleset}: ${response.status}`)
  }
  return await response.json()
}

export async function fetchUniqueCallbackValues(ruleset) {
  const response = await fetch(`/api/rulesets/${encodeRuleset(ruleset)}/unique-callbacks`)
  if (!response.ok) {
    throw new Error(`Failed to fetch unique callbacks for ${ruleset}: ${response.status}`)
  }
  return await response.json()
}

export async function checkAIAssistant() {
  const response = await fetch('/api/has-openai-credential')
  const data = await response.json()
  return data.has_credential // Ensure it's a boolean comparison
}

export async function generate_description(regex) {
  const response = await fetch('/api/ai-assistant/edit', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({ regex })
  })
  if (!response.ok) {
    throw new Error('Failed to generate description')
  }
  return await response.json()
}

export async function generate_regex(description) {
  const response = await fetch('/api/ai-assistant/create', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({ description })
  })
  if (!response.ok) {
    throw new Error('Failed to generate regex')
  }
  return await response.json()
}

export async function test_pattern(regex, sample) {
  const response = await fetch('/api/test-pattern', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({ regex, sample })
  })
  if (!response.ok) {
    throw new Error('Failed to test pattern')
  }
  return await response.json()
}

// The URL builder is also exported so the Vue view can construct
// mutation URLs (POST / PUT / DELETE) without duplicating the template
// string or re-implementing the encoding guard. Keeping it in one place
// makes the ruleset-segment location the single source of truth.
export function rulesetPatternsUrl(ruleset) {
  return `/api/rulesets/${encodeRuleset(ruleset)}/patterns`
}

export function rulesetPatternsOrderUrl(ruleset) {
  return `/api/rulesets/${encodeRuleset(ruleset)}/patterns-order`
}

// Ruleset lifecycle. `createRuleset` accepts an optional `source`
// stem — when present, the backend copies the source file verbatim so
// the new ruleset is a deep duplicate (pattern order preserved, which
// matters for first-match-wins tokenization).
export async function createRuleset(name, source = null) {
  const body = source ? { name, source } : { name }
  const response = await fetch('/api/rulesets', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body)
  })
  if (!response.ok) {
    const detail = await response.json().catch(() => ({}))
    throw new Error(detail.detail || `Failed to create ruleset: ${response.status}`)
  }
  return await response.json()
}

export async function deleteRuleset(name) {
  const response = await fetch(`/api/rulesets/${encodeRuleset(name)}`, {
    method: 'DELETE'
  })
  if (!response.ok) {
    const detail = await response.json().catch(() => ({}))
    throw new Error(detail.detail || `Failed to delete ruleset: ${response.status}`)
  }
  return await response.json()
}

export async function renameRuleset(name, newName) {
  const response = await fetch(`/api/rulesets/${encodeRuleset(name)}/rename`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ new_name: newName })
  })
  if (!response.ok) {
    const detail = await response.json().catch(() => ({}))
    throw new Error(detail.detail || `Failed to rename ruleset: ${response.status}`)
  }
  return await response.json()
}
