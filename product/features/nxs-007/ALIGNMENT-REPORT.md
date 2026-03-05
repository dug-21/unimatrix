# nxs-007: Vision Alignment Report

## Alignment Summary

| Dimension | Status | Notes |
|-----------|--------|-------|
| Vision fit | PASS | Directly described in product vision roadmap |
| Scope boundaries | PASS with VARIANCE | AC-03 (compat layer deletion) revised to relocation per ADR-001 |
| Architecture direction | PASS | Follows nxs-005 -> nxs-006 -> nxs-007 -> nxs-008 progression |
| Behavioral parity | PASS | No functional changes, all tools produce identical results |
| Prerequisite chain | PASS | Correctly sequenced after nxs-006, before nxs-008 |

**Overall: PASS (1 VARIANCE)**

---

## Detailed Analysis

### 1. Product Vision Alignment

The product vision explicitly defines nxs-007:
> "Mechanical cleanup after nxs-006 cutover confirms SQLite stability. Remove redb backend implementation (~7,590 lines), Remove transitional compat layer (~742 lines), Remove all cfg gates, Flatten sqlite/ module, Remove redb from workspace dependencies, Remove export subcommand."

The SCOPE.md maps directly to this vision statement. Goals 1-10 and ACs 1-15 cover every item in the vision description.

**Status: PASS**

### 2. Scope Boundary Alignment

**VARIANCE V-01: Compat layer retained, not deleted**

The product vision says "Remove transitional compat layer (~742 lines: compat.rs, compat_handles.rs, compat_txn.rs)." The architecture (ADR-001) instead retains these types and relocates them from `sqlite/` to crate root with new names (tables.rs, handles.rs, dispatch.rs).

**Rationale**: The compat layer is the server's primary database API (90+ call sites). Deleting it would require rewriting the server's database access layer, which is explicitly nxs-008's scope ("Server decoupling: Refactor unimatrix-server to use only the EntryStore trait API"). Doing both in nxs-007 conflates mechanical cleanup with architectural refactoring.

**Impact**: ~742 lines are renamed rather than deleted. The "transitional" label is removed (the types are permanent until nxs-008). Net line deletion is ~742 fewer than the vision states but the code is no longer transitional -- it is the legitimate SQLite table API.

**Risk**: The vision description may mislead future readers into thinking nxs-007 eliminated the compat layer. The vision should be updated after nxs-007 to reflect that nxs-008 handles compat layer removal.

**Recommendation**: Accept variance. Update the nxs-007 vision entry after implementation to say "Relocate and rename compat layer" instead of "Remove transitional compat layer."

### 3. Architecture Direction Alignment

The nxs-005 -> nxs-006 -> nxs-007 -> nxs-008 progression is explicit in the product vision:

```
nxs-005: SQLite Storage Engine  (dual-backend, feature-flagged)
  └─ nxs-006: SQLite Cutover    (migrate prod DB, flip default)
       └─ nxs-007: redb Removal (delete ~8K lines, remove cfg gates)
            └─ nxs-008: Schema Normalization + Server Decoupling
```

nxs-007 correctly:
- Depends on nxs-006 (prerequisite)
- Does NOT do nxs-008 work (no schema normalization, no server decoupling)
- Produces the "clean SQLite-only codebase" that nxs-008 requires

The 7-wave architecture (ADR-003) ensures each step is independently verifiable.

**Status: PASS**

### 4. Behavioral Parity

AC-13 requires "All 10 MCP tools produce identical results." The architecture achieves this by:
- Making zero changes to the server's database access patterns (compat types retained)
- Making zero changes to the Store API
- Making zero changes to any MCP tool handler logic
- Only deleting code that was already unreachable (redb backend code behind `#[cfg(not(feature = "backend-sqlite"))]` was dead code since nxs-006 flipped the default)

The test strategy relies on existing tests as the regression suite.

**Status: PASS**

### 5. Prerequisite Chain

The vision states "Prerequisite: nxs-006 (SQLite is default, migration complete)." The architecture enforces this:
- R-08 (nxs-006 merge conflict) explicitly requires nxs-006 to be merged before implementation
- The wave plan assumes the codebase state after nxs-006

The vision also states nxs-008 depends on nxs-007. The architecture ensures nxs-008 gets what it needs:
- Clean, single-backend codebase (no cfg gates)
- Compat types available for nxs-008 to migrate away from
- No redb dependencies blocking schema changes

**Status: PASS**

---

## Variance Register

| ID | Description | Severity | Recommendation |
|----|-------------|----------|----------------|
| V-01 | Compat layer retained and relocated rather than deleted | LOW | Accept. Update vision entry after implementation. Compat deletion is nxs-008 scope. |

---

## Variances Requiring Approval

**V-01**: The compat layer is relocated and renamed rather than deleted as stated in the product vision. This is a scope reduction (less work, not more) that correctly defers server-layer changes to nxs-008. Recommend approval without vision update until nxs-007 implementation is complete.
