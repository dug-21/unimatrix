# Agent Report — nan-021-agent-3-c4-workload-comparator (Stage 3b)

## Component
C4 — workload driver + comparator (`harness/parity_workload.py`), the sole substantial
net-new module of nan-021. Pure test-infrastructure; ZERO production-code diff.

## 1. Files modified / created
- **CREATED** `/workspaces/unimatrix/product/test/infra-001/harness/parity_workload.py` (452 lines)
  — manifest (`ParityWorkload`/`ToolCall`/`default_workload`), symmetric durability barrier
  (`observe_count`/`durability_barrier`/`DurabilityTimeout`), stale-token loader
  (`load_https_vector`), no-seed static guard (`assert_no_seed_reachable`), and the shell-facing
  CLI (`observe-count` / `emit-manifest` / `expected-observe-count`).
- **CREATED** `/workspaces/unimatrix/product/test/infra-001/harness/metric_comparator.py` (225 lines)
  — the MetricVector comparator split out for the ≤500-line single-responsibility rule:
  `EXCLUDED` (closed 3-field D-5 set), `UNIVERSAL_FIELDS` (21 classified), `AT_RISK_FIELDS`,
  `ParityMismatch`, `assert_non_empty`, `compare_metric_vectors`, `field_by_field_record`,
  `write_field_record`. Re-exported from `parity_workload` so C2/C3 have ONE import surface.
- **CREATED** `/workspaces/unimatrix/product/test/infra-001/suites/test_parity_workload.py` (484 lines)
  — 29 pure-function unit tests, 1:1 with `test-plan/c4-workload-comparator.md`.

NOT MINE (flagged, untouched): `git status` shows `M product/test/infra-001/scripts/docker-http-posture-smoke.sh`
— that is C2's file (parallel-wave edit), outside my scope; I did not modify it.

## 2. Tests (FOREGROUND)
`python -m pytest suites/test_parity_workload.py --timeout=60` → **29 passed, 0 failed** (~0.1s, off-Docker).
Covers every R-01/R-02/R-03/R-06/R-09 + AC-03/AC-04/AC-07 scenario in the C4 test plan:
comparator teeth (drop-observe mutation fails loud), wall-clock exclusion (1s delta still passes),
ratio-exact compare, schema-drift fail, barrier stable-release + DIR-granularity (incl `-wal`/`-shm`)
+ symmetric-single-helper + hard-timeout, manifest round-trip + token + exactly-one-Bash, stale-token
rejection, first-live-run field-by-field record emission, at-risk-field flagging, no-seed audit (with
runtime-assembled teeth check).

Release-binary preflight (`harness/conftest.py`, session-wide autouse) aborted the suite on a stale
0.8.6 release binary; resolved by the documented remedy `cargo build --release` (now 0.8.7). No
production code changed.

## 3. Open-question resolutions — the contract C2 (shell) and C3 (Python) MUST follow
- **OQ-A — barrier predicate single-sourcing:** ONE mechanism. `observe_count(store_dir)` is the
  single predicate; the shell C2 leg calls it via this module's CLI
  `python -m harness.parity_workload observe-count <store_dir>` (prints the identical integer).
  No hand-written parallel `du`. The `durability_barrier(leg, ...)` helper is ONE function both legs
  invoke (parameterized by `leg`) — asymmetry is structurally impossible.
- **OQ-B — observe_count durability read:** per-slug store **DIR byte-size** (option b). Sums all
  files in `~/.unimatrix/<digest>/` INCLUDING `unimatrix.db-wal`/`-shm`, NEVER `unimatrix.db` alone
  (#5265 takeaway 3). Durability = size STABLE across two consecutive polls (WAL stopped growing).
  The review's own non-zero count is the AFTER-barrier non-empty check, not the predicate.
- **OQ-C — manifest on-disk format:** **JSON**. `ParityWorkload.to_json()` / `write_manifest(path)`
  emit `parity_workload.json`; the shell C2 leg reads the SAME bytes via
  `python -m harness.parity_workload emit-manifest <path>` (cross-language single source of truth).
  Schema: `{schema_version, session_id, feature_cycle, expected_observe_count, tool_calls[]}`.

Additional locked seams for C2/C3: HTTPS out-file payload `{"run_token", "metric_vector"}` consumed by
`load_https_vector(out_path, expected_run_token)` (rejects stale token, errors on missing file — R-03);
`session_id` IS the run-correlation token (one stable CC identity, both legs); `default_workload()`
pins `feature_cycle="nan-021"` with exactly one load-bearing Bash call whose snippet carries the token.

## 4. Issues / notes for the leader & tester
- **No seed site reachable (AC-03):** confirmed by construction. The audit matches CALL/IMPORT shapes
  (`site(` / `import site`), not the bare name, so the module can legitimately NAME the forbidden sites
  in its `FORBIDDEN_SEED_SITES` literal. (Known trap — Unimatrix pattern #4907.)
- **First-live-run gate (ADR-003 #5293) is a DELIVERY obligation, not codeable here.** The comparator
  EMITS both raw vectors + a per-field table (`field_by_field_record`); the 3-field `EXCLUDED` set
  remains an UNPROVEN ASSUMPTION until the tester examines the first dual-transport run field-by-field
  (prime suspects: `cold_restart_events`, `coordinator_respawn_count`,
  `context_load_before_first_write_kb`, `total_context_loaded_kb`, `permission_friction_events`). Any
  divergence = PRODUCT/HUMAN disposition (GH bug OR product-signed ADR-003 amendment), NEVER a silent
  widen. The tester records the disposition in RISK-COVERAGE-REPORT.md.
- No git state-changing commands run (leader commits the wave). No integration tests / other-component
  files modified.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + `context_search` — surfaced ADR-001/003/006 (#5286/#5293/#5291)
  and the metrics.rs field source; applied directly. Pattern search for the static-audit self-reference
  trap surfaced existing entry **#4907** (source-grep enumerated-site guard trips on comment/string
  mentions of the banned token — fix: match a call-shaped pattern).
- Stored: nothing novel — the one gotcha I hit (self-referential no-seed audit false-positive) is
  already captured by **#4907**, which my call-shaped detection implements. No new fixture/harness
  pattern beyond established infra-001 conventions warranted a new entry.
