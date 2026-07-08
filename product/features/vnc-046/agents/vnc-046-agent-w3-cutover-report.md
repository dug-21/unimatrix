# Agent Report — vnc-046 Wave 3 (Atomic Cutover)

**Agent:** vnc-046-agent-w3-cutover
**Scope:** Switch the observe read path onto the per-slug resolver; delete the old
global/vestigial ObserveContext state. One atomic, breaking, go-red→green change.
**Commit:** a0cf93e5 `impl(observe-context): atomic cutover to per-slug observe resolution (#930)`

## What Changed

1. **`http/router.rs` — ObserveContext reshape (AC-09).** Collapsed 8 fields → 3
   `{ resolver, embed_service, server_version }`. Deleted `session_registry`,
   `pending_entries_analysis`, `services` (P1/P2 split-brain sources) + the 2
   vestigial `vector_store`/`adapt_service`. Removed now-dead imports
   (`AsyncVectorStore`, `VectorAdapter`, `AdaptationService`, `SessionRegistry`,
   `Mutex`, `PendingEntriesAnalysis`, `ServiceLayer`). File now 451 lines.

2. **`http/router/handlers.rs` — route_observe (ADR-001, R-14).** After the
   existing `resolve_store(&key)`, added Step 0b resolving
   `registry_for`/`pending_for`/`services_for` from the SAME already-parsed `key`.
   Each `Err` → `internal_error_response()` (**500, never 404**, no panic) —
   post-store `*_for` Err is a boot-wiring contradiction. `dispatch_request` call
   now threads the resolved `&registry`/`&pending`/`&services`.

3. **`uds/listener.rs` — dispatch_request signature.** Dropped the 2 vestigial
   `_vector_store`/`_adapt_service` params. Updated **all call sites**: 1 real UDS
   path + ~86 in-file tests. The private UDS hops `accept_loop`/`handle_connection`
   no longer forward the two handles (params removed / kept as `_`-prefixed at
   `handle_connection`); the `pub` entry `start_uds_listener` keeps them
   `_`-prefixed so the daemon construction call in main.rs is untouched (NFR-4).

4. **`main.rs` — ObserveContext construction.** Rewired to the 3-field shape.
   Wave-2 per-slug scanner + config param-threading at `build_project_server` left
   untouched. No boot assertion / IsolationProbe / field census (that is Wave 4).

5. **Test double + new R-14 unit test (`http/router/tests.rs`).** `observe_ctx_over`
   helper reshaped to 3 fields. Added
   `test_post_store_star_for_err_maps_to_500_not_404` using the Wave-1
   `StubProjectRouter` (store Ok, `*_for` Err) → asserts 500, not 404.

6. **`uds/listener/tests/{transcript,stamp_read}.rs`** — dispatch call sites updated
   (dispatch_with_caps helper + the `dispatch!` macro).

## Verification

- `cargo build -p unimatrix-server` — green, 0 warnings.
- `cargo test -p unimatrix-server --lib` — **4513 passed, 0 failed, 1 ignored**
  (one transient failure on an unrelated `eval`/parallelism-flake test cleared on
  rerun; two subsequent full runs clean).
- `cargo test -p unimatrix-server --bin unimatrix` — **127 passed, 0 failed**.
- `cargo clippy -p unimatrix-server --all-targets` — clean on all changed files
  (2 pre-existing `repeat().take()` warnings remain in untouched
  `mcp/response/verbosity.rs`).
- Reverted pure fmt churn on out-of-scope `mcp/edge_write_delete_agent_tests.rs`
  (twice — `cargo fmt` keeps re-touching it; pre-existing unformatted).

## Files Modified (7, all in scope)
- crates/unimatrix-server/src/http/router.rs
- crates/unimatrix-server/src/http/router/handlers.rs
- crates/unimatrix-server/src/http/router/tests.rs
- crates/unimatrix-server/src/main.rs
- crates/unimatrix-server/src/uds/listener.rs
- crates/unimatrix-server/src/uds/listener/tests/stamp_read.rs
- crates/unimatrix-server/src/uds/listener/tests/transcript.rs

## Flags for Wave 4 / Stage 3c
- **Wave 4 (boot-assertion):** not done here by design. `assert_per_slug_isolation`
  + exhaustive `UnimatrixServer` field census + IsolationProbe still owed. The
  R-14 500 path currently relies on that boot assertion foreclosing the
  contradiction at boot — until Wave 4 lands, a mis-wired slug degrades to a
  runtime 500 rather than a loud boot abort.
- **Pre-existing 500-line violations (flag, not fixed):** `uds/listener.rs` = 9630
  lines, `main.rs` = 2236 lines. Both were already far over before this wave (this
  change *removed* ~178 lines from listener.rs). Not split — per instruction, do
  not split listener.rs gratuitously; flagged as pre-existing.
- **`internal_error_response()` body text** is `{"error":"failed to read request
  body"}` (reused per pseudocode) — semantically misleading for the R-14 case but
  status is correct 500. Stage 3c may want a dedicated message; out of scope here.
- **Stage 3c (tests/ crate):** untouched per instruction. The bidirectional N≥2
  behavioral isolation suite (INV-T/K/C) over `route_observe` is the real
  enforcement of the per-slug convergence this wave wired.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_get #5637 (multi_thread flavor — applied to the
  new test), context_search vnc-046 decisions (ADR-001/002/005 confirmed the
  reshape + 500-not-404 boundary). Findings applied.
- Stored: entry #5639 "Scope mechanical positional-arg removal to the target call —
  sibling helpers share arg identifiers" via context_store (pattern, topic
  unimatrix-server) — a real codemod gotcha: a file-global triple-line match
  over-matched `dispatch_compact`, a `Deps` struct literal, and tuple-return
  helpers that reuse `vs`/`es`/`adapt`; fix = gate the transform to within the
  `dispatch_request(` call span.
