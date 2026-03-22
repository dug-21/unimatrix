# Security Review: crt-026-security-reviewer

## Risk Level: low

## Summary

crt-026 adds per-session category histogram tracking to the search ranking pipeline. The implementation correctly gates all new session-state reads/writes behind existing validation (category allowlist enforcement, session_id sanitization), uses in-memory-only storage with no persistence or new external inputs, and applies the histogram as an additive scoring term bounded by `[0.0, 0.02]`. No blocking security findings were identified. Two low-severity observations are noted.

---

## Findings

### Finding 1: u32 counter overflow — plain `+= 1` without saturation

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/infra/session.rs:249`
- **Description**: `*count += 1;` uses plain integer addition on a `u32`. In release builds, Rust wraps on overflow by default (no panic). A session that stores more than `u32::MAX` (~4.3 billion) entries for a single category would wrap the counter to a small value, corrupting `p(category)` for that session. In practice this is unreachable (no session lives that long; sessions are cleared on reconnection), but the code expresses no intent and diverges from the `saturating_sub` idiom used elsewhere in `session.rs` (lines 448, 1163).
- **Recommendation**: Change `*count += 1;` to `*count = count.saturating_add(1);` for explicit intent and consistency with existing counter patterns in the file.
- **Blocking**: no

### Finding 2: CompactPayload path passes session_id to histogram read without prior sanitize_session_id call

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:854-862`, `1161`
- **Description**: The `HookRequest::CompactPayload` dispatch block at line 841 does not call `sanitize_session_id` before forwarding `session_id` to `handle_compact_payload`, which then calls `session_registry.get_category_histogram(session_id)` at line 1161. The `ContextSearch` and `SessionRegister`/`SessionClose` paths all call `sanitize_session_id` before registry access. The `CompactPayload` path has been missing this call since before crt-026 (this is a pre-existing condition, not introduced by this PR). crt-026 adds a new registry call (`get_category_histogram`) on the unsanitized path. The blast radius is limited: `get_category_histogram` is a pure read returning an empty map for unknown session IDs; no SQL, no file I/O, no state mutation occurs on an unrecognized key. However, the unsanitized session_id may produce unexpected log entries or be forwarded into `AuditContext.source` strings.
- **Recommendation**: Add `sanitize_session_id` validation for the `CompactPayload` session_id at the dispatch site (line 854), consistent with `SessionRegister`, `SessionClose`, and `ContextSearch`. This is a pre-existing gap surfaced by the new registry call; the fix belongs in this PR as a companion to the new histogram read.
- **Blocking**: no

---

## Area-by-Area Assessment

### session_id as HashMap key (spawn prompt item 1)

`session_id` is the key into `self.sessions` (the `SessionRegistry` HashMap). At the MCP path, `session_id` originates from `AuditContext` which is built from validated MCP call parameters — not directly from raw tool arguments. At the UDS path, `ContextSearch` validates via `sanitize_session_id` (alphanumeric + dash/underscore, max 128 chars) before any registry access. The `CompactPayload` path is the only UDS handler that does not call `sanitize_session_id` before registry access, and this is pre-existing; the impact is read-only (see Finding 2).

### category as HashMap key in histogram (spawn prompt item 2)

`category` flowing into `record_category_store` is validated by `CategoryAllowlist::validate` (an allowlist of 7 controlled strings: lesson-learned, decision, convention, pattern, procedure, duties, reference) at `tools.rs:505` before the histogram call at `tools.rs:583`. The vocabulary is controlled; injection of arbitrary category strings is blocked at the allowlist before reaching the histogram.

### Numeric overflow in category_histogram (spawn prompt item 3)

Two u32 arithmetic sites exist:
1. `*count += 1` in `record_category_store` — plain addition, see Finding 1.
2. `h.values().copied().sum()` for `histogram_total` in `search.rs:808` — this also uses plain `u32` sum. If individual counts overflow (Finding 1 scenario), the sum could also wrap. Both are protected by the "session doesn't live long enough" argument. Neither causes a security issue; worst case is a subtly incorrect boost value during a pathological session.

The `histogram_total as f64` cast at line 856 is safe — u32 fits precisely in f64.

### format_compaction_payload injection risk (spawn prompt item 4)

Category strings in `format_compaction_payload` originate from `category_counts` keys, which are only set via `record_category_store` after category allowlist validation. The output is a plain Rust string (no shell execution, no HTML rendering, no SQL). The format string `"{} \u{00d7} {}"` takes category name and u32 count — neither is user-controlled at this point. No injection risk identified.

### Cross-session isolation (spawn prompt item 5)

Session A's histogram cannot affect session B's results. The scoring path resolves the histogram once per call via `params.category_histogram`, which is populated in the handler from `get_category_histogram(session_id_A)` — a clone of session A's state. Session B's call uses its own pre-resolved clone. The underlying `HashMap<String, SessionState>` is keyed by session_id; there is no shared mutable state between sessions in the scoring path.

### FusionWeights::effective() NLI-absent denominator (spawn prompt item 6)

Confirmed correct. At `search.rs:91`, the denominator is explicitly `self.w_sim + self.w_conf + self.w_coac + self.w_util + self.w_prov` — five terms. `w_phase_histogram` and `w_phase_explicit` are NOT in the denominator and are passed through unchanged in all three branches of `effective()` (NLI active, NLI absent normal, NLI absent all-zero degenerate). The code comment at line 92 states this intent explicitly. This correctly prevents dilution of existing weights when NLI is absent.

---

## OWASP Checks

| Check | Result |
|-------|--------|
| Injection (SQL, command, path traversal) | Not applicable. No new SQL, file I/O, or shell execution. Histogram values are in-memory only. |
| Broken access control | Not applicable. No new endpoints, capabilities, or trust-level changes. |
| Security misconfiguration | Not applicable. New config fields have default values and range validation in `InferenceConfig::validate()`. |
| Vulnerable components | No new dependencies introduced. |
| Data integrity | Duplicate-store guard correctly placed before `record_category_store` (tools.rs:569-583). Ordering confirmed by code reading. |
| Deserialization | No new deserialization boundaries. `ServiceSearchParams.category_histogram` is internal data, not deserialized from external input. |
| Input validation | Category validated via allowlist before histogram recording. session_id validated at ContextSearch UDS path. CompactPayload session_id is pre-existing gap (Finding 2). |
| Secrets | No hardcoded credentials, tokens, or API keys in the diff. |

---

## Blast Radius Assessment

**Worst case if the fix has a subtle bug:**

The highest-impact failure mode is FM-05 from the risk strategy: if a `Some(empty_map)` were to reach the scoring loop despite the `is_empty() → None` guard, `histogram_total = 0` would fire the `else { 0.0 }` branch correctly (the guard on line 851 is `histogram_total > 0`). This path does NOT produce NaN — the guard is correct. No search corruption is possible from this path.

The second worst case is the u32 wrap in `record_category_store` (Finding 1). If a counter wraps, `p(category)` becomes artificially small for that category, mildly degrading boost quality for that session. No crash, no data corruption, no cross-session effect.

The blast radius is bounded to: subtly incorrect search ranking in a single long-lived session. No data loss, no privacy leak, no privilege escalation, no denial of service.

---

## Regression Risk

**Existing functionality that could break:**

1. `FusionWeights` struct literal construction sites (8 found): all updated with `w_phase_histogram: 0.0, w_phase_explicit: 0.0`. No omission found. Confirmed all construction sites compile.
2. `ServiceSearchParams` construction sites (6 found): all updated with `session_id: None, category_histogram: None`. No omission found.
3. `FusedScoreInputs` construction sites: all updated with `phase_histogram_norm: 0.0, phase_explicit_norm: 0.0`. No omission found.
4. `format_compaction_payload` call sites: all updated with `&std::collections::HashMap::new()` as the new last argument. 8 existing test call sites updated.
5. Cold-start parity: `phase_histogram_norm = 0.0` when `histogram_total == 0` ensures `compute_fused_score` output is bit-identical to pre-crt-026 when no session histogram exists. Confirmed by code inspection.

Regression risk is low. All struct literal sites were found and updated; the Rust compiler enforces exhaustive field initialization for non-`Default` structs, though note that `FusionWeights` and `FusedScoreInputs` now derive `Default`, which could allow `..Default::default()` to silently omit new fields in future. This is acceptable given the zero-default values.

---

## PR Comments

- Posted 1 comment on PR #344 (findings summary).
- Blocking findings: no.

---

## Knowledge Stewardship

- Nothing novel to store -- the `u32` counter saturation pattern and the missing `sanitize_session_id` on `CompactPayload` are both narrow, crt-026-specific observations. The saturation pattern is a Rust code quality item, not a generalizable lesson. The CompactPayload sanitize gap was pre-existing and does not represent a new anti-pattern to record.
