#!/usr/bin/env python3
"""
run_eval.py — Behavioral eval runner for Unimatrix.

Evaluates the current knowledge base against the 1,585 behavioral scenarios
in scenarios.jsonl (canonical eval instrument per ASS-040 roadmap).

Workflow:
  1. unimatrix snapshot  — frozen copy of live DB (WAL-isolated, no daemon stop)
  2. unimatrix eval run  — replay scenarios through a profile via production Rust code
  3. aggregate           — compute mean MRR and P@k from per-scenario JSON files

Usage:
  python run_eval.py
  python run_eval.py --profile /path/to/profile.toml
  python run_eval.py --out /tmp/my-eval --keep
  python run_eval.py --k 5

Baseline: MRR = 0.2651  (conf-boost-c, 1761 scenarios, 2026-04-03, GH #501/#502)
Exit 0   — MRR >= baseline (AC-11 pass)
Exit 1   — MRR < baseline (AC-11 regression) or any step failed
"""

import argparse
import hashlib
import json
import subprocess
import sys
import tempfile
import time
import tomllib
from pathlib import Path

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------

SCRIPT_DIR = Path(__file__).parent
REPO_ROOT = SCRIPT_DIR.parent.parent.parent.parent  # product/research/ass-039/harness → repo root

SCENARIOS_DEFAULT = SCRIPT_DIR / "scenarios.jsonl"
PROFILE_DEFAULT = REPO_ROOT / "product/research/ass-037/harness/profiles/conf-boost-c.toml"

UNIMATRIX_BIN = "unimatrix"
BASELINE_MRR = 0.2651
DEFAULT_K = 5


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def profile_name_from_toml(path: Path) -> str:
    """Read [profile] name from a profile TOML file."""
    with open(path, "rb") as f:
        data = tomllib.load(f)
    return data.get("profile", {}).get("name", path.stem)


def run_step(label: str, cmd: list[str]) -> None:
    """Run a CLI step, printing label and command. Exits on non-zero."""
    print(f"\n[{label}] {' '.join(str(c) for c in cmd)}", flush=True)
    t0 = time.monotonic()
    result = subprocess.run(cmd)
    elapsed = time.monotonic() - t0
    if result.returncode != 0:
        print(f"  ERROR: command exited {result.returncode} after {elapsed:.1f}s")
        sys.exit(1)
    print(f"  done ({elapsed:.1f}s)", flush=True)


def _sha256_file(path: Path) -> str:
    """Compute SHA-256 of a file via streaming read."""
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def check_snapshot_pairing(
    scenarios_path: Path,
    snap_path: Path,
    allow_mismatch: bool,
) -> None:
    """
    Compare the snapshot hash against scenarios_meta.json.
    Missing sidecar: WARNING only (backward compat).
    Hash mismatch + allow_mismatch: WARNING.
    Hash mismatch (no flag): ERROR + exit(1).
    """
    sidecar = scenarios_path.parent / "scenarios_meta.json"
    if not sidecar.exists():
        print(
            f"  WARNING: scenarios_meta.json not found alongside {scenarios_path.name}"
            " — cannot verify snapshot pairing (backward compat mode)",
            flush=True,
        )
        return

    try:
        meta = json.loads(sidecar.read_text())
    except (json.JSONDecodeError, OSError) as exc:
        print(f"  WARNING: could not read scenarios_meta.json: {exc}", flush=True)
        return

    expected_hash = meta.get("source_db_hash", "")
    generated_at = meta.get("generated_at", "unknown")
    current_hash = _sha256_file(snap_path)

    if current_hash == expected_hash:
        return  # Normal path — silent

    msg_prefix = "WARNING" if allow_mismatch else "ERROR"
    print(
        f"\n{msg_prefix}: Snapshot hash mismatch — eval results would be invalid.\n"
        f"  scenarios generated from: {expected_hash[:12]}... on {generated_at}\n"
        f"  current snapshot:         {current_hash[:12]}...",
        flush=True,
    )

    if not allow_mismatch:
        print(
            "\nThese scenarios were generated from a different DB state than the current\n"
            "snapshot. MRR measured across different DB states reflects KB drift, not\n"
            "retrieval quality. Re-generate scenarios.jsonl from the same snapshot:\n"
            "\n"
            "  unimatrix snapshot --out /tmp/eval/snap.db\n"
            "  python product/research/ass-039/build_scenarios.py  (pointing at snap.db)\n"
            "  python product/research/ass-039/harness/run_eval.py --scenarios ... --profile ...\n"
            "\n"
            "To override (measure drift intentionally): --allow-snapshot-mismatch",
            flush=True,
        )
        sys.exit(1)


def aggregate(results_dir: Path, profile: str) -> tuple[float, float, int]:
    """
    Read per-scenario JSON files from results_dir and return (mean_mrr, mean_p_at_k, count).

    Each file has structure: {"profiles": {"<profile>": {"mrr": f, "p_at_k": f}}}
    profile-meta.json is skipped.
    """
    mrr_sum = 0.0
    pat_sum = 0.0
    n = 0
    bad = 0

    for path in sorted(results_dir.glob("*.json")):
        if path.name == "profile-meta.json":
            continue
        try:
            data = json.loads(path.read_text())
            pr = data["profiles"][profile]
            mrr_sum += pr["mrr"]
            pat_sum += pr["p_at_k"]
            n += 1
        except (KeyError, json.JSONDecodeError, OSError):
            bad += 1

    if bad:
        print(f"  Warning: {bad} result file(s) skipped (missing profile or malformed)")

    if n == 0:
        return 0.0, 0.0, 0

    return mrr_sum / n, pat_sum / n, n


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Behavioral eval runner: MRR/P@k against current knowledge base",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=f"Baseline: MRR = {BASELINE_MRR} (conf-boost-c, 1761 scenarios, 2026-04-03)",
    )
    parser.add_argument(
        "--scenarios",
        type=Path,
        default=SCENARIOS_DEFAULT,
        metavar="PATH",
        help=f"JSONL scenarios file (default: {SCENARIOS_DEFAULT.name})",
    )
    parser.add_argument(
        "--profile",
        type=Path,
        default=PROFILE_DEFAULT,
        metavar="PATH",
        help="Profile TOML to evaluate (default: conf-boost-c)",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=None,
        metavar="DIR",
        help="Persist result files to this directory (default: tempdir, deleted after run)",
    )
    parser.add_argument(
        "--k",
        type=int,
        default=DEFAULT_K,
        help=f"K for P@K metric (default: {DEFAULT_K})",
    )
    parser.add_argument(
        "--keep",
        action="store_true",
        help="Keep snapshot file alongside --out directory",
    )
    parser.add_argument(
        "--allow-snapshot-mismatch",
        action="store_true",
        help="Allow eval to run even when snapshot hash differs from scenarios_meta.json",
    )
    args = parser.parse_args()

    # -- Validate inputs --
    if not args.scenarios.exists():
        print(f"ERROR: scenarios file not found: {args.scenarios}")
        sys.exit(1)
    if not args.profile.exists():
        print(f"ERROR: profile not found: {args.profile}")
        sys.exit(1)
    if args.k < 1:
        print("ERROR: --k must be >= 1")
        sys.exit(1)

    profile_name = profile_name_from_toml(args.profile)

    # Count scenarios with behavioral ground truth
    with open(args.scenarios) as f:
        scenarios = [json.loads(line) for line in f if line.strip()]
    gt_count = sum(1 for s in scenarios if s.get("expected"))
    print(
        f"Scenarios : {len(scenarios)} total, {gt_count} with ground truth",
        flush=True,
    )
    print(f"Profile   : {profile_name}  ({args.profile.name})", flush=True)
    print(f"Metric    : MRR + P@{args.k}", flush=True)

    # -- Run in a temp workspace (snapshot + optional results) --
    with tempfile.TemporaryDirectory(prefix="unimatrix-eval-") as tmpdir:
        tmp = Path(tmpdir)
        snap_path = tmp / "snap.db"
        results_dir = args.out if args.out else tmp / "results"

        if args.out:
            args.out.mkdir(parents=True, exist_ok=True)

        # Step 1: Snapshot
        run_step("1/3 snapshot", [UNIMATRIX_BIN, "snapshot", "--out", str(snap_path)])

        # Snapshot pairing check — validates scenarios were generated from same DB state
        check_snapshot_pairing(args.scenarios, snap_path, args.allow_snapshot_mismatch)

        # Step 2: Eval run
        run_step(
            "2/3 eval run",
            [
                UNIMATRIX_BIN, "eval", "run",
                "--db", str(snap_path),
                "--scenarios", str(args.scenarios),
                "--configs", str(args.profile),
                "--out", str(results_dir),
                "--k", str(args.k),
            ],
        )

        # Step 3: Aggregate
        print("\n[3/3] Aggregating...", flush=True)
        mrr, pat, n = aggregate(results_dir, profile_name)

        if n == 0:
            print("ERROR: no results produced — check eval run output above")
            sys.exit(1)

        delta = mrr - BASELINE_MRR
        print(f"\n{'─' * 52}")
        print(f"Profile   : {profile_name}")
        print(f"Scenarios : {n}")
        print(f"MRR       : {mrr:.4f}  (baseline {BASELINE_MRR}, delta {delta:+.4f})")
        print(f"P@{args.k}       : {pat:.4f}")
        print(f"{'─' * 52}")

        if args.out:
            print(f"Results   : {args.out}")
        if args.keep and args.out:
            keep_snap = args.out / "snap.db"
            snap_path.rename(keep_snap)
            print(f"Snapshot  : {keep_snap}")

        if mrr < BASELINE_MRR:
            print(f"\nREGRESSION: MRR {mrr:.4f} < baseline {BASELINE_MRR} (AC-11)")
            sys.exit(1)

        print(f"\nPASS: MRR >= {BASELINE_MRR} (AC-11)")


if __name__ == "__main__":
    main()
