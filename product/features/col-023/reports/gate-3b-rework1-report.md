# Gate 3b Rework-1 Report: col-023

> Gate: 3b (Code Review — rework iteration 1)
> Date: 2026-03-21
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| DomainPackRegistry wired to ALL SqlObservationSource call sites | PASS | `_observation_registry` renamed; all 3 production call sites use `SqlObservationSource::new()` with the injected Arc |
| Startup wiring — daemon path | PASS | `main.rs:550–566` builds `observation_registry`; line 649 passes it to `ServiceLayer::new()`; line 687 sets `server.observation_registry` |
| Startup wiring — stdio path | PASS | `main.rs:933–950` mirrors daemon path; line 1031 ServiceLayer; line 1069 server field |
| No remaining `new_default()` calls | PASS | Grep across all crates: zero matches |
| mcp/tools.rs call sites | PASS | Line 1116 uses `registry_for_obs = Arc::clone(&self.observation_registry)`; line 1369–1371 uses `registry_for_discover = Arc::clone(&self.observation_registry)` |
| services/status.rs call site | PASS | Line 722–725 uses `Arc::clone(&self.observation_registry)` |
| source_domain guard still first in agent.rs | PASS | `detect()` at line 28–31: `.filter(|r| r.source_domain == "claude-code")` is first operation |
| source_domain guard still first in friction.rs | PASS | Lines 27–31 first operation |
| source_domain guard still first in session.rs | PASS | Lines 31–31 first operation |
| source_domain guard still first in scope.rs | PASS | Lines 32–35 first operation |
| No stubs / todo!() / unimplemented!() | PASS | Grep over unimatrix-server/src: no matches |
| 500-line cap (new files only) | PASS (WARN carried from 3b) | No new files added in rework; pre-existing over-limit files unchanged |
| Compilation — cargo check --workspace | PASS | Clean: zero errors, 9 pre-existing warnings in unimatrix-server lib |
| Test suite — cargo test --workspace | PASS | All tests pass; no failures |

## Detailed Findings

### 1. DomainPackRegistry Wired to SqlObservationSource Call Sites

**Status**: PASS

**Evidence**:

The original failure was that `_observation_registry` was built at startup but its `_` prefix suppressed the unused-variable warning, and all `SqlObservationSource` instantiations used the `new_default()` convenience constructor instead of the registry Arc.

After rework:

- `_observation_registry` renamed to `observation_registry` in both startup paths (daemon: line 550, stdio: line 933).
- `new_default()` has been removed entirely: `grep -r "new_default()" crates/` returns zero matches.

**mcp/tools.rs** — two call sites:
- Line 1110: `let registry_for_obs = Arc::clone(&self.observation_registry);` then line 1116: `SqlObservationSource::new(store_for_obs, registry_for_obs)`.
- Line 1363: `let registry_for_discover = Arc::clone(&self.observation_registry);` then lines 1369–1371: `SqlObservationSource::new(store_for_discover, registry_for_discover)`.

**services/status.rs** — one call site:
- Line 722: `SqlObservationSource::new(Arc::clone(&self.store), Arc::clone(&self.observation_registry))`.

**StatusService struct**: `observation_registry: Arc<DomainPackRegistry>` declared as a field (line 183), initialised in constructor (line 220/230), and propagated through `ServiceLayer::new()` (services/mod.rs line 435).

**UnimatrixServer struct**: `server.observation_registry = Arc::clone(&observation_registry)` set at both startup path exit points (lines 687 and 1069), confirming the MCP tool handlers receive the startup-configured registry on every request.

---

### 2. source_domain Guards Intact in All Four Detection Files

**Status**: PASS

All four detection rule files retain the mandatory `source_domain == "claude-code"` filter as the first statement inside each `detect()` method:

| File | First filter line |
|------|-------------------|
| `detection/agent.rs` | 28–31: `.filter(|r| r.source_domain == "claude-code")` |
| `detection/friction.rs` | 27–31 |
| `detection/session.rs` | 31–31 |
| `detection/scope.rs` | 32–35 |

No regression to this ADR-005 guard was introduced during the rework.

---

### 3. Compilation and Test Suite

**Status**: PASS

`cargo check --workspace` completes with zero errors and 9 pre-existing warnings (unchanged from original gate-3b).

`cargo test --workspace` passes with no failures. All 21 detection rule tests and the 17 ingest-security tests remain intact.

---

### 4. No New Stubs

**Status**: PASS

No `todo!()`, `unimplemented!()`, or `FIXME` markers appear in `crates/unimatrix-server/src`. The two `// TODO(W2-4)` markers in `main.rs` are pre-existing scope markers for a separate future feature (gguf_rayon_pool), unchanged by this rework.

---

### 5. 500-Line Cap

**Status**: WARN (carried forward — not new)

No new source files were added or created by this rework. The eight pre-existing over-limit files identified in gate-3b were not grown further. The WARN status is unchanged from gate-3b-report.md and is not a new violation.

---

## Rework Required

None.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the registry injection fix is feature-specific. The pattern "startup-built Arc not threaded into request handlers" was documented in gate-3b-report.md as a candidate for a recurring lesson if it appears in a second feature. It has now appeared once; will warrant a lesson-learned entry on second occurrence.
