"""Suite-level conftest: makes harness fixtures available to suites/*.

Adds the harness parent directory to sys.path so that imports from
harness/ work correctly when pytest discovers tests in suites/.
"""

import sys
from pathlib import Path

# Add infra-001 root to path so 'harness' package is importable
harness_parent = str(Path(__file__).resolve().parent.parent)
if harness_parent not in sys.path:
    sys.path.insert(0, harness_parent)

# Re-export fixtures so pytest discovers them for suites/
from harness.conftest import server, shared_server, populated_server, admin_server, fast_tick_server  # noqa: F401
from harness.conftest import get_binary_path  # noqa: F401
