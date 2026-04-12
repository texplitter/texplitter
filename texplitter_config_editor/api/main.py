# main.py

from fastapi import FastAPI, HTTPException, Body, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse, FileResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel, Field, ValidationError, validator
from typing import List
from pathlib import Path
import json
import logging
import os
import re

# Import the AI assistant module
from . import ai_assistant

# Set up logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s',
    handlers=[
        logging.FileHandler("backend.log"),  # Log to a file named 'backend.log'
        logging.StreamHandler()  # Also log to console
    ]
)

app = FastAPI()

# Path to the built Vue.js frontend. Optional: the module must remain
# importable when the frontend hasn't been built yet (developers running
# only the API, or pytest importing `main` for unit tests). If the
# `dist/assets` directory is missing we skip the static mount and the
# index/favicon routes degrade to a small placeholder response.
dist_path = Path(__file__).resolve().parent.parent / "frontend" / "dist"
logging.info(f"Dist path: {dist_path}")

_frontend_built = (dist_path / "assets").is_dir()
if _frontend_built:
    app.mount("/assets", StaticFiles(directory=dist_path / "assets"), name="assets")
else:
    logging.warning(
        f"Frontend not built at {dist_path}; /assets mount disabled. "
        f"Run `npm run build` in frontend/ to enable the UI."
    )


# Serve the Vue.js app
@app.get("/")
async def read_index():
    index_file_path = dist_path / "index.html"
    logging.info(f"Index file path: {index_file_path}")
    if not index_file_path.is_file():
        return JSONResponse(
            status_code=503,
            content={"detail": "frontend not built; run `npm run build` in frontend/"},
        )
    return FileResponse(index_file_path)


# Serve the favicon
@app.get("/favicon.ico")
async def favicon():
    favicon_path = dist_path / "favicon.ico"
    if not favicon_path.is_file():
        raise HTTPException(status_code=404, detail="favicon not built")
    return FileResponse(favicon_path)

# Configure CORS
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],  # Adjust this to the appropriate origins in production
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# ---------------------------------------------------------------------------
# Ruleset storage
# ---------------------------------------------------------------------------
#
# Every ruleset is a standalone JSON file under
# `src/texplitter/config/rules/`. Each file is a complete, order-sensitive
# list of regex-pattern dicts — there is no concatenation or inheritance
# between rulesets (first-match-wins tokenization makes composition
# semantically broken, so the editor deliberately refuses to compose).
#
# `default.json` is the baseline ruleset that the runtime
# `Tokenizer()` constructor loads when called with no arguments; peers
# next to it are additional user-authored rulesets loadable by name via
# `texplitter.parserrules.load_regex_patterns(name)`.
#
# The config editor resolves a concrete filesystem path (rather than
# using `importlib.resources`) because it needs to *write* files in
# place — `importlib.resources` returns read-only Traversable paths
# when the package ships from a wheel.
RULES_DIR = (
    Path(__file__).resolve().parent.parent.parent
    / "src" / "texplitter" / "config" / "rules"
)
logging.info(f"Rules directory: {RULES_DIR}")

if not RULES_DIR.is_dir():
    logging.error(f"Rules directory does not exist: {RULES_DIR}")

# Ruleset names are file stems under `RULES_DIR`. We restrict the allowed
# character set so user-supplied path segments can never escape the
# directory (no slashes, no `..`, no leading dots). The library loader
# `texplitter.parserrules.load_regex_patterns` applies the same regex.
_RULESET_NAME_RE = re.compile(r"^[a-zA-Z0-9_-]+$")

# `default` is the baseline ruleset that zero-arg `Tokenizer()` loads.
# Deleting or renaming it would break every downstream caller that
# relies on the zero-arg default, so the mutation routes refuse those
# operations. Edits to the *contents* of default.json are still fine.
_PROTECTED_RULESETS = frozenset({"default"})


def _ruleset_path(name: str) -> Path:
    """Resolve a ruleset name to its on-disk `.json` path.

    Raises HTTPException(400) for any name that does not match the
    allow-list regex — this is the path-traversal guard. Does NOT check
    existence; callers that need 404 semantics (load/update/delete) must
    check `path.is_file()` themselves.
    """
    if not _RULESET_NAME_RE.match(name):
        raise HTTPException(
            status_code=400,
            detail=f"invalid ruleset name {name!r}",
        )
    return RULES_DIR / f"{name}.json"


class Pattern(BaseModel):
    type: str
    class_: str = Field(alias="class")
    description: str
    regex: str
    callback: str
    sample: str

    @validator('*')
    def check_no_lists(cls, v):
        if isinstance(v, list):
            raise ValueError('List is not allowed for this field')
        return v


def load_patterns(name: str) -> List[Pattern]:
    """Load and parse the named ruleset, or raise 404 if missing."""
    path = _ruleset_path(name)
    if not path.is_file():
        raise HTTPException(status_code=404, detail=f"ruleset {name!r} not found")
    with open(path, 'r', encoding='utf-8') as file:
        patterns_data = json.load(file)
    return [Pattern(**pattern) for pattern in patterns_data]


def save_patterns(name: str, patterns: List[Pattern]) -> None:
    """Overwrite the named ruleset with `patterns`. The parent directory
    must already exist; `_ruleset_path` guarantees the filename is safe."""
    path = _ruleset_path(name)
    patterns_data = [pattern.dict(by_alias=True) for pattern in patterns]
    with open(path, 'w', encoding='utf-8') as file:
        json.dump(patterns_data, file, ensure_ascii=False, indent=4)


@app.get("/api/rulesets")
async def list_rulesets(request: Request):
    """Enumerate available ruleset names (file stems, no `.json`)."""
    logging.info(f"Request path: {request.url.path}, method: {request.method}")
    if not RULES_DIR.is_dir():
        return []
    names = sorted(
        p.stem
        for p in RULES_DIR.iterdir()
        if p.is_file() and p.suffix == ".json"
    )
    logging.info(f"Rulesets: {names}")
    return names


@app.post("/api/rulesets")
async def create_ruleset(request: Request, payload: dict = Body(...)):
    """Create a new ruleset, either empty or seeded from an existing one.

    Body: `{"name": "<stem>", "source": "<stem>"?}`.

    - 400 if `name` is missing/invalid (same allow-list as `_ruleset_path`)
    - 409 if a ruleset with that name already exists
    - 404 if `source` is supplied but doesn't exist

    When `source` is present we copy the file's contents verbatim; this
    preserves the exact pattern order, which is load-bearing for
    first-match-wins tokenization. We deliberately don't round-trip
    through `Pattern` so any extra fields in the source file stay
    byte-compatible with whatever the library accepts.
    """
    logging.info(f"Request path: {request.url.path}, method: {request.method}")
    name = payload.get("name", "")
    source = payload.get("source")
    if not isinstance(name, str) or not name:
        raise HTTPException(status_code=400, detail="ruleset 'name' is required")
    target = _ruleset_path(name)
    if target.exists():
        raise HTTPException(
            status_code=409, detail=f"ruleset {name!r} already exists"
        )
    if source is not None:
        if not isinstance(source, str) or not source:
            raise HTTPException(
                status_code=400, detail="'source' must be a non-empty string"
            )
        source_path = _ruleset_path(source)
        if not source_path.is_file():
            raise HTTPException(
                status_code=404, detail=f"source ruleset {source!r} not found"
            )
        target.write_text(source_path.read_text(encoding="utf-8"), encoding="utf-8")
        logging.info(f"Ruleset {name!r} created from {source!r}")
    else:
        target.write_text("[]", encoding="utf-8")
        logging.info(f"Ruleset {name!r} created empty")
    return {"message": "Ruleset created successfully", "name": name}


@app.delete("/api/rulesets/{name}")
async def delete_ruleset(request: Request, name: str):
    """Delete a ruleset file. `default` is protected — see `_PROTECTED_RULESETS`."""
    logging.info(f"Request path: {request.url.path}, method: {request.method}")
    if name in _PROTECTED_RULESETS:
        raise HTTPException(
            status_code=400,
            detail=f"ruleset {name!r} is protected and cannot be deleted",
        )
    path = _ruleset_path(name)
    if not path.is_file():
        raise HTTPException(status_code=404, detail=f"ruleset {name!r} not found")
    path.unlink()
    logging.info(f"Ruleset {name!r} deleted")
    return {"message": "Ruleset deleted successfully"}


@app.put("/api/rulesets/{name}/rename")
async def rename_ruleset(request: Request, name: str, payload: dict = Body(...)):
    """Rename a ruleset. Body: `{"new_name": "<stem>"}`.

    Refuses to rename `default` (it's the baseline the runtime looks
    for). 409 if the target name already exists.
    """
    logging.info(f"Request path: {request.url.path}, method: {request.method}")
    if name in _PROTECTED_RULESETS:
        raise HTTPException(
            status_code=400,
            detail=f"ruleset {name!r} is protected and cannot be renamed",
        )
    new_name = payload.get("new_name", "")
    if not isinstance(new_name, str) or not new_name:
        raise HTTPException(status_code=400, detail="'new_name' is required")
    src = _ruleset_path(name)
    dst = _ruleset_path(new_name)
    if not src.is_file():
        raise HTTPException(status_code=404, detail=f"ruleset {name!r} not found")
    if dst.exists():
        raise HTTPException(
            status_code=409, detail=f"ruleset {new_name!r} already exists"
        )
    src.rename(dst)
    logging.info(f"Ruleset renamed: {name!r} -> {new_name!r}")
    return {"message": "Ruleset renamed successfully", "name": new_name}


@app.get("/api/rulesets/{name}/patterns")
async def get_patterns(request: Request, name: str):
    logging.info(f"Request path: {request.url.path}, method: {request.method}")
    patterns = load_patterns(name)
    logging.info(f"Fetched {len(patterns)} patterns from ruleset {name!r}")
    return patterns


@app.get("/api/rulesets/{name}/unique-classes")
async def get_unique_classes(request: Request, name: str):
    logging.info(f"Request path: {request.url.path}, method: {request.method}")
    patterns = load_patterns(name)
    unique_classes = sorted({pattern.class_ for pattern in patterns})
    logging.info(f"Unique classes for {name!r}: {unique_classes}")
    return unique_classes


@app.get("/api/rulesets/{name}/unique-callbacks")
async def get_unique_callbacks(request: Request, name: str):
    logging.info(f"Request path: {request.url.path}, method: {request.method}")
    patterns = load_patterns(name)
    unique_callbacks = sorted({pattern.callback for pattern in patterns})
    logging.info(f"Unique callbacks for {name!r}: {unique_callbacks}")
    return unique_callbacks


@app.post("/api/rulesets/{name}/patterns")
async def create_pattern(request: Request, name: str, pattern: Pattern):
    logging.info(f"Request path: {request.url.path}, method: {request.method}")
    patterns = load_patterns(name)
    patterns.append(pattern)
    save_patterns(name, patterns)
    logging.info(f"Pattern created in {name!r}: {pattern.dict(by_alias=True)}")
    return {"message": "Pattern added successfully"}


@app.put("/api/rulesets/{name}/patterns/{index}")
async def update_pattern(request: Request, name: str, index: int, pattern: Pattern):
    logging.info(f"Request path: {request.url.path}, method: {request.method}")
    patterns = load_patterns(name)
    if not (0 <= index < len(patterns)):
        raise HTTPException(status_code=404, detail="Pattern not found")
    patterns[index] = pattern
    save_patterns(name, patterns)
    logging.info(f"Pattern updated at {name!r}[{index}]: {pattern.dict(by_alias=True)}")
    return {"message": "Pattern updated successfully"}


@app.put("/api/rulesets/{name}/patterns-order")
async def save_patterns_order(request: Request, name: str, payload: dict = Body(...)):
    logging.info(f"Request path: {request.url.path}, method: {request.method}")
    # Touch the ruleset first so we 404 before parsing the body — cheaper
    # than validating the payload only to discover the file doesn't exist.
    _ = load_patterns(name)
    patterns_data = payload.get("patterns", [])
    try:
        patterns = [Pattern(**pattern) for pattern in patterns_data]
    except Exception as e:
        logging.error(f"Error validating reordered patterns for {name!r}: {e}")
        raise HTTPException(
            status_code=400,
            detail=f"invalid pattern in reorder payload: {e}",
        )
    save_patterns(name, patterns)
    logging.info(f"Patterns order saved for {name!r}")
    return {"message": "Patterns order updated successfully"}


@app.delete("/api/rulesets/{name}/patterns/{index}")
async def delete_pattern(request: Request, name: str, index: int):
    logging.info(f"Request path: {request.url.path}, method: {request.method}")
    patterns = load_patterns(name)
    if not (0 <= index < len(patterns)):
        raise HTTPException(status_code=404, detail="Pattern not found")
    patterns.pop(index)
    save_patterns(name, patterns)
    logging.info(f"Pattern deleted at {name!r}[{index}]")
    return {"message": "Pattern deleted successfully"}

# AI Assistant endpoints
@app.post("/api/ai-assistant/create")
async def ai_assistant_create(request: Request, data: dict = Body(...)):
    logging.info(f"Request path: {request.url.path}, method: {request.method}")
    description = data.get("description", "")
    regex = data.get("regex", "")
    
    try:
        generated_regex = ai_assistant.generate_regex(description)
        return {"description": description, "regex": generated_regex}
    except Exception as e:
        logging.error(f"Error generating regex: {e}")
        raise HTTPException(status_code=500, detail=f"Error generating regex: {e}")

@app.post("/api/ai-assistant/edit")
async def ai_assistant_edit(request: Request, data: dict = Body(...)):
    logging.info(f"Request path: {request.url.path}, method: {request.method}")
    description = data.get("description", "")
    regex = data.get("regex", "")
    
    try:
        generated_description = ai_assistant.generate_description(regex)
        return {"description": generated_description, "regex": regex}
    except Exception as e:
        logging.error(f"Error generating description: {e}")
        raise HTTPException(status_code=500, detail=f"Error generating description: {e}")

@app.post("/api/test-pattern")
async def test_pattern(request: Request, data: dict = Body(...)):
    logging.info(f"Request path: {request.url.path}, method: {request.method}")
    regex = data.get("regex", "")
    sample = data.get("sample", "")
    test_result = f"{regex}{sample}"
    return {"testResult": test_result}

@app.get("/api/has-openai-credential")
async def has_openai_credential(request: Request):
    logging.info(f"Request path: {request.url.path}, method: {request.method}")
    has_credential = bool(
        os.environ.get("TEXPLITTER_LLM_API_KEY")
        or os.environ.get("TEXPLITTER_OPENAI_CREDENTIAL")
    )
    logging.info(f"Has LLM credential: {has_credential}")
    return {"has_credential": has_credential}

@app.exception_handler(ValidationError)
async def validation_exception_handler(request: Request, exc: ValidationError):
    logging.error(f"Validation error at {request.url.path}: {exc.errors()}")
    return JSONResponse(
        status_code=422,
        content={"detail": exc.errors()},
    )
