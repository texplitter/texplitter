"""Tests for the texplitter config editor FastAPI app.

Every test that mutates a ruleset file redirects `main.RULES_DIR` to a
per-test tmp_path via monkeypatch. That way the real
`src/texplitter/config/rules/default.json` on disk is never touched and
the pytest run cannot corrupt the baseline grammar.
"""

import json
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from texplitter_config_editor.api import main


SAMPLE_PATTERN = {
    "type": "example",
    "class": "test",
    "description": "This is a test pattern",
    "regex": r"^test$",
    "callback": "callback_function",
    "sample": "test sample",
}


@pytest.fixture
def rules_dir(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    """Point `main.RULES_DIR` at a throwaway tmp directory. Every test
    that touches rulesets must use this fixture so we never write into
    the real packaged ruleset directory."""
    d = tmp_path / "rules"
    d.mkdir()
    monkeypatch.setattr(main, "RULES_DIR", d)
    return d


@pytest.fixture
def client(rules_dir: Path) -> TestClient:
    return TestClient(main.app)


def _write_ruleset(rules_dir: Path, name: str, patterns: list) -> Path:
    path = rules_dir / f"{name}.json"
    path.write_text(json.dumps(patterns), encoding="utf-8")
    return path


# ---------------------------------------------------------------------------
# GET /api/rulesets
# ---------------------------------------------------------------------------


def test_list_rulesets_empty(client: TestClient):
    response = client.get("/api/rulesets")
    assert response.status_code == 200
    assert response.json() == []


def test_list_rulesets_sorted_stems(client: TestClient, rules_dir: Path):
    _write_ruleset(rules_dir, "default", [SAMPLE_PATTERN])
    _write_ruleset(rules_dir, "html", [SAMPLE_PATTERN])
    _write_ruleset(rules_dir, "custom", [SAMPLE_PATTERN])
    # Drop a non-json peer to confirm it's filtered out.
    (rules_dir / "README.md").write_text("ignore me", encoding="utf-8")

    response = client.get("/api/rulesets")
    assert response.status_code == 200
    assert response.json() == ["custom", "default", "html"]


# ---------------------------------------------------------------------------
# GET /api/rulesets/{name}/patterns (+ 404 path)
# ---------------------------------------------------------------------------


def test_get_patterns_round_trip(client: TestClient, rules_dir: Path):
    _write_ruleset(rules_dir, "default", [SAMPLE_PATTERN])
    response = client.get("/api/rulesets/default/patterns")
    assert response.status_code == 200
    body = response.json()
    assert isinstance(body, list)
    assert len(body) == 1
    assert body[0]["type"] == "example"
    assert body[0]["class"] == "test"


def test_get_patterns_unknown_ruleset_404(client: TestClient):
    response = client.get("/api/rulesets/nonexistent/patterns")
    assert response.status_code == 404
    assert "not found" in response.json()["detail"]


# ---------------------------------------------------------------------------
# Path-traversal rejection: every endpoint that takes `{name}` must
# refuse names containing path separators, parent refs, leading dots,
# or anything outside `[a-zA-Z0-9_-]`. The FastAPI router normalizes
# some shapes into 404s before our handler runs, which is also fine —
# the point is the filesystem never sees the traversal attempt.
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    "bad_name",
    [
        "name with spaces",
        "name;rm",
        "name.dot",
        ".hidden",
    ],
)
def test_get_patterns_rejects_invalid_name(client: TestClient, bad_name: str):
    response = client.get(f"/api/rulesets/{bad_name}/patterns")
    # The _ruleset_path guard returns 400. FastAPI URL validation may also
    # reject the shape earlier with a 404/422 — all three are acceptable
    # as long as the request doesn't succeed.
    assert response.status_code in (400, 404, 422)


def test_get_patterns_rejects_path_separator(client: TestClient):
    # Explicit path-traversal attempt — FastAPI will route this as
    # /api/rulesets/..%2Fetc%2Fpasswd/patterns once URL-encoded, which
    # the router decodes and passes through to our handler.
    response = client.get("/api/rulesets/..%2Fetc%2Fpasswd/patterns")
    assert response.status_code in (400, 404, 422)


# ---------------------------------------------------------------------------
# POST / PUT / DELETE happy paths
# ---------------------------------------------------------------------------


def test_create_pattern_appends(client: TestClient, rules_dir: Path):
    _write_ruleset(rules_dir, "default", [SAMPLE_PATTERN])
    new_pattern = {**SAMPLE_PATTERN, "type": "second"}
    response = client.post("/api/rulesets/default/patterns", json=new_pattern)
    assert response.status_code == 200
    assert response.json() == {"message": "Pattern added successfully"}

    # The file on disk now has two patterns in insertion order.
    saved = json.loads((rules_dir / "default.json").read_text(encoding="utf-8"))
    assert len(saved) == 2
    assert saved[0]["type"] == "example"
    assert saved[1]["type"] == "second"


def test_create_pattern_unknown_ruleset_404(client: TestClient):
    response = client.post(
        "/api/rulesets/nonexistent/patterns", json=SAMPLE_PATTERN
    )
    assert response.status_code == 404


def test_update_pattern(client: TestClient, rules_dir: Path):
    _write_ruleset(rules_dir, "default", [SAMPLE_PATTERN])
    updated = {**SAMPLE_PATTERN, "type": "updated"}
    response = client.put("/api/rulesets/default/patterns/0", json=updated)
    assert response.status_code == 200
    assert response.json() == {"message": "Pattern updated successfully"}

    saved = json.loads((rules_dir / "default.json").read_text(encoding="utf-8"))
    assert saved[0]["type"] == "updated"


def test_update_pattern_out_of_range_404(client: TestClient, rules_dir: Path):
    _write_ruleset(rules_dir, "default", [SAMPLE_PATTERN])
    response = client.put("/api/rulesets/default/patterns/99", json=SAMPLE_PATTERN)
    assert response.status_code == 404


def test_delete_pattern(client: TestClient, rules_dir: Path):
    _write_ruleset(
        rules_dir,
        "default",
        [SAMPLE_PATTERN, {**SAMPLE_PATTERN, "type": "second"}],
    )
    response = client.delete("/api/rulesets/default/patterns/0")
    assert response.status_code == 200
    assert response.json() == {"message": "Pattern deleted successfully"}

    saved = json.loads((rules_dir / "default.json").read_text(encoding="utf-8"))
    assert len(saved) == 1
    assert saved[0]["type"] == "second"


def test_delete_pattern_out_of_range_404(client: TestClient, rules_dir: Path):
    _write_ruleset(rules_dir, "default", [SAMPLE_PATTERN])
    response = client.delete("/api/rulesets/default/patterns/99")
    assert response.status_code == 404


def test_save_patterns_order(client: TestClient, rules_dir: Path):
    _write_ruleset(
        rules_dir,
        "default",
        [SAMPLE_PATTERN, {**SAMPLE_PATTERN, "type": "second"}],
    )
    reordered = [
        {**SAMPLE_PATTERN, "type": "second"},
        SAMPLE_PATTERN,
    ]
    response = client.put(
        "/api/rulesets/default/patterns-order",
        json={"patterns": reordered},
    )
    assert response.status_code == 200
    assert response.json() == {"message": "Patterns order updated successfully"}

    saved = json.loads((rules_dir / "default.json").read_text(encoding="utf-8"))
    assert [p["type"] for p in saved] == ["second", "example"]


def test_save_patterns_order_unknown_ruleset_404(client: TestClient):
    response = client.put(
        "/api/rulesets/nonexistent/patterns-order",
        json={"patterns": []},
    )
    assert response.status_code == 404


# ---------------------------------------------------------------------------
# Ruleset lifecycle: create / delete / rename
# ---------------------------------------------------------------------------


def test_create_ruleset_empty(client: TestClient, rules_dir: Path):
    response = client.post("/api/rulesets", json={"name": "blank"})
    assert response.status_code == 200
    assert response.json()["name"] == "blank"
    saved = json.loads((rules_dir / "blank.json").read_text(encoding="utf-8"))
    assert saved == []


def test_create_ruleset_from_source_copies_patterns(
    client: TestClient, rules_dir: Path
):
    _write_ruleset(
        rules_dir,
        "default",
        [SAMPLE_PATTERN, {**SAMPLE_PATTERN, "type": "second"}],
    )
    response = client.post(
        "/api/rulesets", json={"name": "copy", "source": "default"}
    )
    assert response.status_code == 200
    saved = json.loads((rules_dir / "copy.json").read_text(encoding="utf-8"))
    # The copy preserves order exactly — order is load-bearing for
    # first-match-wins tokenization.
    assert [p["type"] for p in saved] == ["example", "second"]


def test_create_ruleset_conflict(client: TestClient, rules_dir: Path):
    _write_ruleset(rules_dir, "default", [SAMPLE_PATTERN])
    response = client.post("/api/rulesets", json={"name": "default"})
    assert response.status_code == 409


def test_create_ruleset_missing_source(client: TestClient, rules_dir: Path):
    response = client.post(
        "/api/rulesets", json={"name": "new", "source": "nonexistent"}
    )
    assert response.status_code == 404


def test_create_ruleset_invalid_name(client: TestClient):
    response = client.post("/api/rulesets", json={"name": "bad name"})
    assert response.status_code == 400


def test_create_ruleset_missing_name(client: TestClient):
    response = client.post("/api/rulesets", json={})
    assert response.status_code == 400


def test_delete_ruleset(client: TestClient, rules_dir: Path):
    _write_ruleset(rules_dir, "custom", [SAMPLE_PATTERN])
    response = client.delete("/api/rulesets/custom")
    assert response.status_code == 200
    assert not (rules_dir / "custom.json").exists()


def test_delete_ruleset_refuses_default(client: TestClient, rules_dir: Path):
    _write_ruleset(rules_dir, "default", [SAMPLE_PATTERN])
    response = client.delete("/api/rulesets/default")
    assert response.status_code == 400
    # The file must still be on disk — protection is not advisory.
    assert (rules_dir / "default.json").exists()


def test_delete_ruleset_unknown_404(client: TestClient):
    response = client.delete("/api/rulesets/nonexistent")
    assert response.status_code == 404


def test_rename_ruleset(client: TestClient, rules_dir: Path):
    _write_ruleset(rules_dir, "old", [SAMPLE_PATTERN])
    response = client.put(
        "/api/rulesets/old/rename", json={"new_name": "new"}
    )
    assert response.status_code == 200
    assert not (rules_dir / "old.json").exists()
    assert (rules_dir / "new.json").exists()


def test_rename_ruleset_refuses_default(client: TestClient, rules_dir: Path):
    _write_ruleset(rules_dir, "default", [SAMPLE_PATTERN])
    response = client.put(
        "/api/rulesets/default/rename", json={"new_name": "notdefault"}
    )
    assert response.status_code == 400
    assert (rules_dir / "default.json").exists()


def test_rename_ruleset_conflict(client: TestClient, rules_dir: Path):
    _write_ruleset(rules_dir, "one", [SAMPLE_PATTERN])
    _write_ruleset(rules_dir, "two", [SAMPLE_PATTERN])
    response = client.put(
        "/api/rulesets/one/rename", json={"new_name": "two"}
    )
    assert response.status_code == 409
    # Both files must still exist on disk.
    assert (rules_dir / "one.json").exists()
    assert (rules_dir / "two.json").exists()


def test_rename_ruleset_unknown_404(client: TestClient):
    response = client.put(
        "/api/rulesets/nonexistent/rename", json={"new_name": "anything"}
    )
    assert response.status_code == 404


# ---------------------------------------------------------------------------
# unique-classes / unique-callbacks
# ---------------------------------------------------------------------------


def test_unique_classes_and_callbacks(client: TestClient, rules_dir: Path):
    patterns = [
        {**SAMPLE_PATTERN, "type": "a", "class": "BREAK", "callback": "cb1"},
        {**SAMPLE_PATTERN, "type": "b", "class": "TRANSLATABLE", "callback": "cb2"},
        {**SAMPLE_PATTERN, "type": "c", "class": "BREAK", "callback": "cb1"},
    ]
    _write_ruleset(rules_dir, "default", patterns)

    classes = client.get("/api/rulesets/default/unique-classes").json()
    callbacks = client.get("/api/rulesets/default/unique-callbacks").json()
    assert classes == ["BREAK", "TRANSLATABLE"]
    assert callbacks == ["cb1", "cb2"]
