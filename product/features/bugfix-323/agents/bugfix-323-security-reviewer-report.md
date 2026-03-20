# Security Review: bugfix-323-security-reviewer

## Risk Level: low

## Summary

The fix adds two targeted changes: `copy_vector_files()` in `snapshot.rs` copies HNSW files from the live vector dir into a snapshot-adjacent `vector/` subdirectory, and `from_profile()` in `layer.rs` adds a conditional `VectorIndex::load()` branch when snapshot vector files are present. No new dependencies introduced. No hardcoded secrets. Path traversal risk exists in theory but is mitigated by the constrained operating context. One low-severity finding (basename from meta file used unsanitized in path joins) warrants hardening but is not blocking.

---

## Findings

### Finding 1: Basename from meta file is not sanitised before use in path joins

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/snapshot.rs:197-198` and `crates/unimatrix-vector/src/persistence.rs:103-104`
- **Description**: The `basename` value is parsed from `unimatrix-vector.meta` (a system-written file) and used to construct file paths via `src_vector_dir.join(format!("{basename}.hnsw.graph"))`. If `basename` contained path separators (e.g., `../../etc/passwd`), `Path::join()` would allow traversal outside `src_vector_dir`. However, the meta file is always written by `VectorIndex::dump()` which hardcodes `DUMP_BASENAME = "unimatrix"` — a constant with no path components. The risk is latent and requires an attacker to corrupt the meta file on disk, which requires local write access to `~/.unimatrix/{hash}/vector/`. No external input path reaches this code.
- **Recommendation**: Add a one-line guard rejecting basename values that contain `/` or `\` before the path joins in both `copy_vector_files()` and `VectorIndex::load()`. This closes the latent path traversal channel regardless of how the meta file was written.
- **Blocking**: no

### Finding 2: `vector_meta.exists()` TOCTOU window in layer.rs

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/eval/profile/layer.rs:162`
- **Description**: The code checks `vector_meta.exists()` and then calls `VectorIndex::load()` which re-reads and acts on the same path. In theory, the meta file could be removed or replaced between the check and the load (TOCTOU). In practice the snapshot directory is a user-controlled, offline eval artifact not exposed to concurrent writers. The failure mode is a load error returned to the caller — not a security event. Error handling is correct: `VectorIndex::load()` propagates `VectorError::Persistence` which maps to `EvalError::Store`, returned to the caller without panic.
- **Recommendation**: Accept as-is for the eval harness use case. If snapshot directories are ever exposed to multi-writer environments, the check should be removed and `VectorIndex::load()` called directly, treating `NotFound` as the backwards-compat fallback.
- **Blocking**: no

### Finding 3: `out_parent` path used for destination vector dir without further canonicalization

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/snapshot.rs:85-88`
- **Description**: `out_parent` is derived from `out` (the raw `--out` argument, not the canonicalized `out_resolved`) via `.parent()`. The live-DB path guard compares against the *canonicalized* `out_resolved`. If `out` contains symlink components in the parent portion, `out_parent` might differ from the canonical parent. However: (1) the destination is always joined with the hardcoded literal `"vector"` — it can never write to an arbitrary path; (2) `dst_vector_dir` cannot resolve to `src_vector_dir` because `src_vector_dir` is `~/.unimatrix/{hash}/vector/` while `dst_vector_dir` is `{out_parent}/vector/` — these are structurally different; (3) the live-DB guard already rejected any `out` that resolves to the active database.
- **Recommendation**: No immediate action required. If this function is ever extended to copy additional file types beyond the three hardcoded extensions, the parent path should be canonicalized before joining.
- **Blocking**: no

---

## Blast Radius Assessment

**Worst case if copy_vector_files() has a subtle bug**: writes files to an unintended directory that happens to be `{out_parent}/vector/`. Since `out_parent` comes from the user-supplied `--out` path (rejected if it resolves to the active DB), the worst case is: snapshot vector files land in a wrong scratch directory, or the copy fails and returns an error. The live database is **not** in the copy path — HNSW files are copied *into* the output, not sourced from or written to the live database file. Failure mode is safe: `Result<(), Box<dyn Error>>` propagated to the caller, no silent data loss.

**Worst case if the VectorIndex::load() conditional in from_profile() has a subtle bug**: `from_profile()` returns `EvalError::Store(...)` which the caller (eval run command) surfaces as a non-zero exit code. The live database is never opened in this code path — the existing `LiveDbPath` guard already canonicalize-compares the supplied `db_path` against the active DB and returns early if they match. An incorrect load (e.g., loading wrong dimension HNSW) is caught by the dimension mismatch check inside `VectorIndex::load()`. Silent data corruption cannot occur because the eval layer is read-only (`SqlxStore::open_readonly()`).

---

## Regression Risk

**Backward compatibility**: The `VectorIndex::load()` conditional is explicitly gated on `vector_meta.exists()`. Pre-fix snapshots (lacking a `vector/` directory) follow the `else` branch and construct a fresh empty index — identical to the previous behaviour. This is the correct and safe regression path.

**Dependency chain**: `from_profile()` is only called from the eval harness (D3 `eval run`). No production request-handling paths call it. Regression scope is limited to the offline eval pipeline.

**Test coverage**: The new `test_from_profile_loads_vector_index_from_snapshot_dir` test directly exercises the new branch. The backward-compat path is covered by all pre-existing `from_profile` tests (they create snapshots without a `vector/` dir and verify the `else` branch still initialises correctly).

---

## OWASP Checklist

| Category | Assessment |
|----------|-----------|
| Injection (path) | Latent only — basename is system-written; no external input reaches path joins. See Finding 1. |
| Injection (SQL/cmd) | Not applicable — no new SQL or shell invocations. |
| Broken access control | Not applicable — no new trust boundaries or privilege operations. |
| Security misconfiguration | Not applicable — no configuration changes. |
| Vulnerable components | No new dependencies. |
| Data integrity | The live-DB path guard (canonicalize compare) prevents overwriting the active database. |
| Deserialization | `parse_basename_from_meta` performs simple `split_once('=')` text parsing — no deserialization attack surface. |
| Input validation | `basename` from meta file not validated for path separators. Acceptable for current threat model. |
| Secrets | None found. |
| Unsafe code | None introduced (confirmed by gate bugfix report). |

---

## PR Comments

- Posted 1 comment on PR #325 (assessment comment)
- Blocking findings: no

---

## Knowledge Stewardship

- nothing novel to store -- the latent basename path-join pattern is specific to this snapshot helper; the threat model (system-written meta file, no external input) makes it low priority and not a generalizable lesson-learned worth storing. The general principle (validate external input before path joins) is already a well-known OWASP A01 practice and requires no new Unimatrix entry.
