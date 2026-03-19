# Call-Site Migration (7 Sites) — Verification Plan

**Component**: 7 modified files in `crates/unimatrix-server/src/`
**Risks addressed**: R-04, R-08, R-09, R-10
**AC addressed**: AC-06, AC-08

This component has no self-contained unit tests (the sites are integration points in service
functions that require ONNX model setup). Verification is via grep/static analysis and
end-to-end integration tests.

---

## §per-site-audit — Per-Site Migration Verification (AC-06, R-04)

For each of the 7 sites, the following must be true after migration:

| Site # | File | Old call | New call | Method required |
|--------|------|----------|----------|-----------------|
| 1 | `services/search.rs` ~228 | `spawn_blocking_with_timeout` | `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` | `spawn_with_timeout` |
| 2 | `services/store_ops.rs` ~113 | `spawn_blocking_with_timeout` | `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` | `spawn_with_timeout` |
| 3 | `services/store_correct.rs` ~50 | `spawn_blocking_with_timeout` | `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` | `spawn_with_timeout` |
| 4 | `background.rs` ~543 | `spawn_blocking` | `spawn(...)` | `spawn` (no timeout) |
| 5 | `background.rs` ~1162 | `spawn_blocking` | `spawn(...)` | `spawn` (no timeout) |
| 6 | `uds/listener.rs` ~1383 | `spawn_blocking` | `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` | `spawn_with_timeout` |
| 7 | `services/status.rs` ~542 | `spawn_blocking_with_timeout` | `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` | `spawn_with_timeout` |

### Verification Commands

```bash
# 1. All 7 inference sites must have NO spawn_blocking or spawn_blocking_with_timeout
grep -n "spawn_blocking" \
  crates/unimatrix-server/src/services/search.rs \
  crates/unimatrix-server/src/services/store_ops.rs \
  crates/unimatrix-server/src/services/store_correct.rs \
  crates/unimatrix-server/src/services/status.rs \
  crates/unimatrix-server/src/uds/listener.rs
# Expected: zero results at the embedding call sites
# (Non-embedding spawn_blocking in listener.rs is permitted — see §non-inference-sites)

# 2. MCP-path sites use spawn_with_timeout
grep -n "spawn_with_timeout" \
  crates/unimatrix-server/src/services/search.rs \
  crates/unimatrix-server/src/services/store_ops.rs \
  crates/unimatrix-server/src/services/store_correct.rs \
  crates/unimatrix-server/src/services/status.rs \
  crates/unimatrix-server/src/uds/listener.rs
# Expected: at least 1 result per file at the embedding call site

# 3. Background sites use spawn (no timeout)
grep -n "rayon_pool\.spawn\b" crates/unimatrix-server/src/background.rs
# Expected: 2 results (contradiction scan ~543, quality-gate ~1162)
# Must NOT find: rayon_pool.spawn_with_timeout in these background paths

# 4. MCP_HANDLER_TIMEOUT used by name at all spawn_with_timeout call sites
grep -n "spawn_with_timeout.*MCP_HANDLER_TIMEOUT" \
  crates/unimatrix-server/src/services/search.rs \
  crates/unimatrix-server/src/services/store_ops.rs \
  crates/unimatrix-server/src/services/store_correct.rs \
  crates/unimatrix-server/src/services/status.rs \
  crates/unimatrix-server/src/uds/listener.rs
# Expected: 1 result per file — no hard-coded duration literals
```

### Error Mapping Verification (R-04 integration risk)

At each of the 5 MCP sites and 1 warmup site (spawn_with_timeout), verify the
double `.map_err` pattern is preserved:

```rust
// Expected pattern (from ARCHITECTURE.md §Call-Site Migration Pattern):
self.rayon_pool
    .spawn_with_timeout(MCP_HANDLER_TIMEOUT, { ... })
    .await
    .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?  // outer: RayonError
    .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?  // inner: CoreError
```

Grep to detect missing outer map_err (would silently swallow RayonError):
```bash
# Check that spawn_with_timeout calls are always followed by .await (not stored in let without await)
grep -A3 "spawn_with_timeout" \
  crates/unimatrix-server/src/services/search.rs \
  crates/unimatrix-server/src/services/store_ops.rs \
  crates/unimatrix-server/src/services/store_correct.rs \
  crates/unimatrix-server/src/services/status.rs
# Visual inspection: confirm .await.map_err pattern is present at each site
```

### Background Task Error Handling Verification

Background sites (contradiction scan, quality-gate loop) use `spawn(...)` with no timeout.
Verify that `RayonError::Cancelled` from these sites emits a tracing `error!` log event:

```bash
grep -n "error!" crates/unimatrix-server/src/background.rs
# Expected: at least 2 results near the spawn call sites for contradiction scan and quality-gate
# The Cancelled error must not be silently discarded (not `.ok()` or `let _ =`)
```

---

## §method-audit — Correct Method per Path (R-04, C-11)

The convention (ADR-002, C-11):
- **MCP handler paths** → `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)`
- **Background task paths** → `spawn(...)` (no timeout)

Sites 1, 2, 3, 6, 7 are MCP handler paths → must use `spawn_with_timeout`.
Sites 4 and 5 are background task paths → must use `spawn` (no timeout).

Verify no cross-contamination:
```bash
# Background paths must NOT use spawn_with_timeout for the embedding closure
grep -n "spawn_with_timeout" crates/unimatrix-server/src/background.rs
# Expected: zero results at the contradiction scan (~543) and quality-gate (~1162) embedding sites
# (spawn_with_timeout may appear elsewhere in background.rs for non-embedding operations —
#  review each result to confirm none are at the inference sites)
```

---

## §module-rustdoc — Convention Documentation (R-04)

The `RayonPool` module-level rustdoc must contain the convention:

```bash
grep -A5 "//!" crates/unimatrix-server/src/infra/rayon_pool.rs | head -30
# Expected: doc comment explaining MCP paths use spawn_with_timeout,
#           background paths use spawn (no timeout)
```

Assertion: the word "background" and either "MCP" or "handler" appear in the module rustdoc.

---

## §single-instantiation — Single RayonPool Construction (R-09, ADR-004)

```bash
# Exactly one RayonPool::new call in the startup wiring
grep -rn "RayonPool::new" crates/unimatrix-server/src/
# Expected: exactly 1 result, in main.rs or equivalent startup entry point

# AppState/ServiceLayer has exactly one ml_inference_pool field
grep -n "ml_inference_pool" crates/unimatrix-server/src/
# Expected: 1 definition site + N reference sites
# Any second definition site is a R-09 violation
```

---

## §embed-handle-guard — OnnxProvider::new remains on spawn_blocking (R-10, AC-08)

```bash
grep -n "spawn_blocking" crates/unimatrix-server/src/infra/embed_handle.rs
# Expected: exactly 1 result — the OnnxProvider::new call
# Any rayon_pool call in this file is a violation of C-03
grep -n "rayon_pool\|rayon::spawn\|RayonPool" crates/unimatrix-server/src/infra/embed_handle.rs
# Expected: zero results
```

---

## §non-inference-sites — Permitted spawn_blocking Locations

These sites retain `spawn_blocking` after migration (from IMPLEMENTATION-BRIEF.md §call-site-inventory):

| File | Description | Permitted? |
|------|-------------|------------|
| `infra/embed_handle.rs` | `OnnxProvider::new` — model I/O | YES (C-03) |
| `background.rs` ~1088 | `run_extraction_rules` — pure rule eval | YES |
| `background.rs` ~1144 | `persist_shadow_evaluations` — DB write | YES |
| `server.rs`, `gateway.rs`, `usage.rs` | Registry reads, audit writes | YES |
| `uds/listener.rs` (non-warmup) | Session lifecycle DB writes | YES |

The CI grep step must distinguish inference sites from these permitted sites. The step's
grep patterns must be precise enough not to flag permitted sites as violations. See
`ci_enforcement.md` for the exact step logic.

---

## §cargo-test-coverage — Integration via Existing Tests

The call-site migration does not require new unit tests at the service layer. The service
functions remain async and require the full ONNX model to produce embeddings. Coverage
comes from:

1. **Compilation**: `cargo check --workspace` — type-checks all 7 sites.
   If any site has a type mismatch (e.g., wrong argument to `spawn_with_timeout`), compilation fails.
2. **Integration**: infra-001 `tools` + `lifecycle` suites — exercises the migrated paths
   through the MCP protocol end-to-end.
3. **Static grep**: verifies the correct method is used at each site.
