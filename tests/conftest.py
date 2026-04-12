import os
import sys

# Prefer an installed `texplitter` (e.g. `pip install -e .`), but fall back
# to adding `src/` to sys.path so `pytest` works against a raw checkout
# without any install step.
project_root = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
src_path = os.path.join(project_root, "src")
if src_path not in sys.path:
    sys.path.insert(0, src_path)
