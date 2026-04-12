"""Pytest configuration for the config editor tests.

The editor lives at `texplitter_config_editor/api/main.py`. Tests import
it as `texplitter_config_editor.api.main` — the repo root is already on
`sys.path` because pytest is invoked from there, and the `__init__.py`
files in `texplitter_config_editor/`, `texplitter_config_editor/api/`,
and `texplitter_config_editor/tests/` make the tree an importable
package chain. No `sys.path` manipulation is needed here.
"""
