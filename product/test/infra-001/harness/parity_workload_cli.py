"""C4 — CLI for the parity workload: the shell C2/C5 leg's single entrypoint into the
shared durability predicate + the canonical manifest.

Factored out of `parity_workload.py` (the ≤500-line / single-responsibility split — the
nan-021 `metric_comparator.py` lib-split precedent) so the manifest module stays the
contract and this module owns the thin argv shim. The `python -m harness.parity_workload`
entrypoint is preserved: `parity_workload.__main__` delegates to `main()` here, so the
shell leg's `observe-count`/`emit-manifest`/`expected-observe-count` commands are unchanged
(OQ-A/OQ-C single source — shell calls THIS module, never a parallel script).
"""

from __future__ import annotations

import sys

from harness.parity_workload import default_workload, observe_count


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        sys.stderr.write(
            "usage: python -m harness.parity_workload "
            "{observe-count <store_dir> | emit-manifest <path> | expected-observe-count}\n"
        )
        return 2
    cmd = argv[1]
    if cmd == "observe-count":
        if len(argv) != 3:
            sys.stderr.write("usage: observe-count <store_dir>\n")
            return 2
        sys.stdout.write(f"{observe_count(argv[2])}\n")
        return 0
    if cmd == "emit-manifest":
        if len(argv) != 3:
            sys.stderr.write("usage: emit-manifest <path>\n")
            return 2
        path = default_workload().write_manifest(argv[2])
        sys.stdout.write(f"{path}\n")
        return 0
    if cmd == "expected-observe-count":
        sys.stdout.write(f"{default_workload().expected_observe_count}\n")
        return 0
    sys.stderr.write(f"unknown command: {cmd}\n")
    return 2
