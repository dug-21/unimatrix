"""conftest.py for tests/ directory.

Adds the infra-001 root to sys.path so that 'harness' package is importable
when pytest discovers tests in this directory.
"""

import sys
from pathlib import Path

# Add infra-001 root to path so 'harness' package is importable.
_infra_root = str(Path(__file__).resolve().parent.parent)
if _infra_root not in sys.path:
    sys.path.insert(0, _infra_root)
