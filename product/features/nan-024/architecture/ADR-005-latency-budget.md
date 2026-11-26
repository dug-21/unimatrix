## ADR-005: Hook latency budget relative to Node baseline, incl. not-a-project path

### Context
D-006's 25 ms is unattainable (`node -e 0` p95 33.6 ms). D-017/D-025: budget = baseline p95 + 15 ms, floor 50 ms; covers the not-a-project path, which fires in every project for user-level Codex hooks; measure it and add a pre-Node check if over budget.

### Decision
- Budget: hook p95 <= max(node_baseline_p95 + 15 ms, 50 ms), `node -e 0` baseline in the same CI job and runner. Cloud adds measured RTT p95.
- Measured paths (50 iterations per event class, 5 warm-ups), literal written command through the harness shell (ADR-001):
  1. Linux local: shim -> resolver -> client -> UDS -> daemon.
  2. Linux, macOS, Windows cloud: shim -> client -> HTTPS (+RTT); Windows includes the identity-compare trust path (ADR-002).
  3. Not-a-project, all three OSes: same budget, no RTT.
- **Pre-Node check** (D-025), enabled iff path 3 breaches on any OS; must never contradict the D-045 predicate (lossless superset):
  - headless env (`UNIMATRIX_BUNDLE`, `UNIMATRIX_REMOTE_*`) set and `UNIMATRIX_PROJECT_ROOT` set and cwd equal to or inside it (D-052) -> start Node;
  - Store dirs are only direct children of `~/.unimatrix/` whose name is exactly 16 characters, all in `[0-9a-f]` (the `computeProjectHash` format). All other entries (`bin/`, `pkg/`, files) are ignored.
  - else for each store dir's `root` file (one line: the project root, written by init; no Rust change), read with shell builtins (`read` / `set /p`) and hit when cwd is equal to or inside that root; hit -> start Node;
  - any store dir (as defined above) lacking a `root` file (legacy) -> start Node;
  - otherwise exit 0 silently. Builtins only, no subprocess.
  - "Equal to or inside" is a path-component match, not a string prefix: cwd == root, or cwd starts with root + separator (sh: `case "$PWD/" in "$root"/*)`; cmd: `%CD%\` begins with `%root%\`, case-insensitive, via `if /i` on a substring of equal length). This stops a sibling that shares the prefix (`/a/proj2` vs root `/a/proj`) from matching (R-34).
  - The same check always runs on the node-not-found path (ADR-001).
- CI fails on any breach.

### Consequences
- Budget tracks hardware. The `root` marker is a speed hint only; the Node predicate is authoritative.
