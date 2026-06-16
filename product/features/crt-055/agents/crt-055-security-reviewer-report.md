# Security Review: crt-055-security-reviewer

## Risk Level: low

## Summary
Fresh-context review of the full `main...HEAD` diff for PR #761 (crt-055 consumer-half redesign of `context_cycle_review`). No injection, deserialization, access-control, or secret-leak surfaces introduced. Zero new dependencies. All Critical blast-radius items from the RISK-TEST-STRATEGY (single-writer/no-clobber, read-before-purge, millis-vs-seconds gate, width conversion, migration idempotency) are implemented correctly and covered by load-bearing tests. No blocking findings.

## Findings

### F1 — auto_close write fires on the empty-attributed return path
- **Severity**: low (informational)
- **Location**: crates/unimatrix-server/src/mcp/tools.rs:2298 (`maybe_auto_close`) relative to the `attributed.is_empty()` branch at :2323
- **Description**: `maybe_auto_close` runs at the top of the full-pipeline block, before the data-availability check. On a cycle with no attributed observations that returns a cached MetricVector / no-data result, a `cycle_stop` may still be written.
- **Recommendation**: No change required. The write is idempotent (existence-checked, no duplicate stop) and informs-only — it writes a `cycle_events` record and never controls execution (RQ-8). Behaviourally consistent with ADR-010 ("close the cycle as part of the review"). Noted for awareness only.
- **Blocking**: no

No other findings.

## OWASP Assessment (per changed file)
- **Injection (SQL/command/path)**: none. `compaction_read.rs` IN-clause uses positional placeholders, binds session_ids as data (explicit `test_compaction_read_sql_injection_guard`). `store_cycle_review` INSERT/UPDATE fully parameterized; bind order matches the SELECT. The v29→v30 migration's sole identifier interpolation comes from a fixed compile-time allowlist (`V5_INT_COLUMNS`), never user input. No shell/format-string/path operations on untrusted input.
- **Deserialization**: `signal_class_counts_json` constructed via `serde_json::Map` (never string concat); operator-supplied `class_name` safely escaped. Reads back as an object of integers — content-free. No untrusted deserialization.
- **Broken access control / privilege escalation**: none. `auto_close` is informs-not-controls; no trust-boundary or capability change.
- **Input validation**: `auto_close: bool` (`#[serde(default)]`) — boolean, no validation gap. Basis-points `context_reload_pct` clamped `0..=10000` before bind. Width conversions `u64/u32→i64` via saturating arithmetic + `u64_to_i64_saturating` (warns, never wraps/panics). 4MB `summary_json` ceiling returns `Err`, not panic.
- **Security misconfiguration / secrets**: no hardcoded credentials, tokens, or keys in the diff.
- **Vulnerable components**: `cargo audit` shows only the pre-existing RUSTSEC-2023-0071 (rsa, unmaintained) via the unused `sqlx-mysql` transitive dep — unchanged at Gate 3b; crt-055 added zero dependencies (confirmed empty Cargo.toml/Cargo.lock diff).

## Blast Radius Assessment
Worst-case if a subtle bug existed in this change would be silent corruption of the cross-cycle self-learning baseline (a believable-wrong-number, not a crash). Each such surface is closed:
- **Single-writer/no-clobber** — exactly ONE non-test `store_cycle_review()` call site (tools.rs:3032) inside the full-pipeline block; the three other returns serve a stored record without writing. `auto_close` writes to `cycle_events`, not a second `cycle_review_index` writer. Worst case (empty-clobber of a purged cycle, the #750/#5022 class) is structurally impossible.
- **Read-before-purge** — `land_fold` (tools.rs:2320) strictly precedes every `purge_cycle_transcripts` call on all paths; inversion test asserts the ordering is load-bearing. Worst case (zeroed transcript columns) is prevented and regression-guarded.
- **Millis-vs-seconds gate** — read `ts` floored `÷1000` on the read side only; boundary stays seconds; strict `>`. The `-500ms → floor T-1 → not counted` floor-guard test prevents the all-or-nothing unit-mismatch failure.
- **Width conversion** — saturating; a near-`u64::MAX` fold saturates-and-warns rather than wrapping to a negative count.
- **Migration v29→v30** — pragma pre-checks before any ALTER, table-existence guard, idempotent re-run, in-transaction stamp. Data integrity on upgrade is preserved; fresh-create / ALTER / struct are byte-aligned (three-path consistency).

## Regression Risk
- **#758 guarded-recompute coexistence** — preserved: stale pre-v5 rows recompute through the same single writer via clear-memo-fall-through; purged-retain vs data-present distinction intact. SUMMARY_SCHEMA_VERSION 4→5 documented.
- **crt-054 producer-contract reads** — `compaction_events` is read-only (no schema mutation); `activity_snapshot()` consumed as scalar counters by fixed index (0=error, 1=refusal). Disjoint-table migration handshake (29 then 30) consistent with lesson #4095.
- **Build** — affected crates compile clean (warnings are pre-existing dead-code notes).

## PR Comments
- Posted 1 comment on PR #761 (review state COMMENTED, non-blocking).
- Blocking findings: no.

## Knowledge Stewardship
- Stored: nothing novel to store — the patterns this review exercised (single-writer-past-presence-guards, schema-bump three-path consistency, declaration-chain attribution, content-opacity persist gate, parameterized-bind injection safety, basis-points-INTEGER float-footgun elimination) are already captured as #5022 / #4153 / #4140 / #4178 / #4529. No 2+-feature security anti-pattern emerged that is not already in Unimatrix.
