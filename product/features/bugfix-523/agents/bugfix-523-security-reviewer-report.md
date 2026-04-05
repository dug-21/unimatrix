# Security Review: bugfix-523-security-reviewer

## Risk Level: low

## Summary

All four items in this batch are surgical, minimal changes. The session injection gap (Item 4)
is correctly closed: `sanitize_session_id` is inserted before the first `event.session_id` use,
mirroring the established pattern in every other dispatch arm. The NaN guards (Item 3) properly
use `!v.is_finite()` for inline fields and `!value.is_finite()` in the loop bodies, covering all
19 fields. The NLI gate (Item 1) is correctly placed after `run_cosine_supports_path` and before
`get_provider()`, with the boolean condition in the correct direction. The log downgrade (Item 2)
changes only the two category_map miss sites — the non-finite cosine `warn!` is untouched.
No new dependencies, no hardcoded secrets, no schema changes. All critical tests pass.

---

## Findings

### F-01: ARCHITECTURE.md documents `nli_informs_ppr_weight` as `f64`, actual field is `f32`
- **Severity**: low
- **Location**: `product/features/bugfix-523/architecture/ARCHITECTURE.md` line 136; `infra/config.rs` line 565
- **Description**: The Group A field list in ARCHITECTURE.md marks `nli_informs_ppr_weight` as `(f64)`. The actual `InferenceConfig` struct field is `f32`. The production guard at `config.rs:1301` and the test at line 8215 both correctly use `f32` — this is a documentation-only discrepancy. Unimatrix entry #4144 already documents this class of stale-type-column risk.
- **Recommendation**: Non-blocking. The implementation is correct; the architecture doc has a stale type annotation. No code change required. Correct the architecture doc annotation if a follow-up clean-up pass is planned.
- **Blocking**: no

### F-02: Rejected session_id value is logged verbatim (established pattern, no new risk)
- **Severity**: low
- **Location**: `uds/listener.rs:669-672` (new code); existing pattern at lines 545, 745
- **Description**: When `sanitize_session_id` rejects a malformed session_id, the value is logged at `warn!` level via `session_id = %event.session_id`. A crafted session_id containing ANSI escape sequences or CRLF could corrupt structured log output. This is not a new vulnerability introduced by this PR — the identical pattern exists in all other dispatch arms (lines 545, 745, 876). The new code mirrors the established pattern exactly.
- **Recommendation**: Non-blocking for this PR. If log-injection hardening is desired, it applies to all arms equally and should be addressed in a dedicated hardening item with sanitization of logged-but-rejected values. Unimatrix entry #3902 already documents this class of risk.
- **Blocking**: no

---

## OWASP Assessment

| Check | Result | Notes |
|-------|--------|-------|
| Injection (A03) | PASS | `sanitize_session_id` allowlist `[a-zA-Z0-9\-_]` max 128 chars is applied before any registry or DB write. Guard position verified: before `event.payload.get("tool_name")` and before both `record_rework_event` and `record_topic_signal` calls. |
| Broken Access Control (A01) | PASS | Capability check (`SessionWrite`) remains before the new session_id guard — unauthorized clients are rejected before session data is processed. Guard ordering: capability gate first, then session_id validation, then data extraction. Correct. |
| Security Misconfiguration (A05) | PASS | Item 3 guards prevent server from starting with NaN/Inf config values. Blast radius reduced from silent scoring corruption to fail-fast startup error. |
| Input Validation | PASS | All 19 `InferenceConfig` float fields now have `!v.is_finite()` prefix guards. Verified by field count: 11 inline guards (Group A) + 1 loop guard for 6 fusion weight fields (Group B) + 1 loop guard for 2 phase weight fields (Group C) = 19 fields. |
| Deserialization | N/A | No new deserialization paths. Config loading is unchanged. |
| Hardcoded Secrets | PASS | No credentials, tokens, or API keys in the diff. |
| New Dependencies | PASS | No new crate dependencies. `Cargo.toml` and `Cargo.lock` unchanged. |

---

## Special Focus Areas

### Item 1 (NLI gate) — Gate condition direction and bypass

The gate `if !config.nli_enabled { return; }` at `nli_detection_tick.rs:561` is positioned after
the `candidate_pairs.is_empty()` fast-exit (line 552) and before `get_provider().await` (line 571).

- **Can the condition be inverted?** No. The condition is `!config.nli_enabled` — a plain boolean
  NOT on a struct field. When `nli_enabled=true`, the condition is `false` and falls through to
  `get_provider()`. When `nli_enabled=false`, the condition is `true` and returns early. Correct.

- **Does early return leak any state?** No. At the return point, the only writes that have
  occurred are Path A (Informs) writes and Path C (cosine Supports) writes — both intentional and
  unconditional. No NLI-specific state is written before the gate. The rayon pool is not touched.

- **Path A and Path C bypass?** Verified: `run_cosine_supports_path` is called at line 536–544,
  which completes before the PATH B entry gate comment at line 546. `background.rs` is unchanged —
  the outer call to `run_graph_inference_tick` remains unconditional. Tests
  `test_nli_gate_path_a_informs_edges_still_written_nli_disabled` and
  `test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled` both pass, confirming
  the gate does not precede Path A or Path C.

### Item 3 (NaN guards) — Full coverage of 19 fields

Guard sites confirmed by code inspection:

- **Group A (11 inline f32/f64 fields)**: Lines 1029, 1039, 1049, 1093, 1104, 1227, 1248, 1259,
  1291, 1302, 1313 — all use `let v = self.<field>; if !v.is_finite() || ...` pattern.

- **Group B (6 fusion weight f64 fields in loop)**: Line 1166 — `if !value.is_finite() || *value ...`
  where `value: &f64`. Auto-deref makes `value.is_finite()` call `f64::is_finite()`. Correct.

- **Group C (2 phase weight f64 fields in loop)**: Line 1184 — same pattern as Group B. Correct.

- **Pre-existing crt-046 guards (3 fields)**: Lines 1390, 1393, 1404, 1415 — unchanged, not part
  of this batch.

All 19 NaN tests pass (`test result: ok. 19 passed`). Field names in tests match array entry
strings exactly for loop-based fields (e.g., `"w_sim"`, `"w_coac"`, `"w_prov"`).

The sum-check bypass noted in ARCHITECTURE.md (`NaN > 1.0` is false → sum check silently passes)
is correctly mitigated: per-field guards fire before the sum-check at line 1198.

### Item 4 (sanitize_session_id) — Guard before all uses of session_id

Code-verified insertion order in the `post_tool_use_rework_candidate` arm:
1. Capability check (lines 660–665) — returns early if no `SessionWrite`
2. **`sanitize_session_id` guard (lines 668–678)** — NEW; returns `ERR_INVALID_PAYLOAD` if malformed
3. `event.payload.get("tool_name")` extraction (line 679) — first payload access
4. `session_registry.record_rework_event(&event.session_id, ...)` (line 703) — first registry write
5. `session_registry.record_topic_signal(&event.session_id, ...)` (line 707) — second registry write

No use of `event.session_id` between the capability check return and the guard. No `event.session_id`
value reaches any registry or storage path without passing the guard. SR-05 compliance confirmed.

Test AC-28 (`test_dispatch_rework_candidate_invalid_session_id_rejected`) passes with
`session_id = "../../etc/passwd"` → `HookResponse::Error { code: ERR_INVALID_PAYLOAD }`.
Test AC-29 (`test_dispatch_rework_candidate_valid_session_id_succeeds`) passes with
`session_id = "session-abc123"` → `HookResponse::Ack`, `rework_events.len() == 1`.

---

## Blast Radius Assessment

**If Item 1 gate has a subtle bug (inverted condition):**
NLI Supports edges stop accumulating when `nli_enabled=true` (production default for NLI deployments).
Failure mode: data absence (no NLI edges written), no crash, no error log. Silent degradation.
Mitigated by: `test_nli_gate_nli_enabled_path_not_regressed` and `test_nli_gate_path_b_skipped_nli_disabled`.

**If Item 3 has a missing guard on one field:**
NaN propagates into that field's scoring path until server restart. For fusion weight fields,
the sum-check also passes NaN (IEEE 754 semantics). All search results would have undefined
scores for that weight component. Failure mode: silent scoring corruption. Mitigated by the
19 individual NaN tests, all of which pass.

**If Item 4 guard has a bug (wrong position or wrong error code):**
A crafted session_id (path traversal chars, SQL metacharacters, Unicode control chars) reaches
`session_registry.record_rework_event`. Blast radius: session registry key corruption, potential
downstream consumer misbehavior. Any hook client with `SessionWrite` capability could trigger.
Mitigated by: AC-28 and AC-29, both passing.

**If Item 2 changed the wrong `warn!` site:**
HNSW vector corruption signals silently drop to `debug!` level. Operators lose the structural
anomaly signal. Failure mode: operational blind spot, not a crash. Mitigated by code review
(log level not asserted in tests per ADR-001(c)/entry #4143). Verified by code inspection:
the non-finite cosine `warn!` at `nli_detection_tick.rs:777` is unchanged; only lines 807
and 817 (category_map miss sites) were changed from `warn!` to `debug!`.

---

## Regression Risk

- **Item 1**: `test_nli_gate_nli_enabled_path_not_regressed` confirms NLI-enabled path is
  unaffected. `background.rs` unconditional call confirmed unchanged.
- **Item 2**: Log downgrade is behavior-neutral for all non-log-level properties. No test
  regression risk.
- **Item 3**: Pre-existing boundary-value tests for the 19 fields remain valid — adding
  `!v.is_finite()` prefix does not change the comparison ranges, only adds finite-check before
  them. Pre-existing crt-046 guards are unchanged.
- **Item 4**: AC-29 regression guard passes — valid session_ids are not rejected. The existing
  `sanitize_session_id` unit tests (lines 3846–3894) are unchanged and cover the guard function
  itself.

---

## PR Comments
- Posted 1 comment on PR #524
- Blocking findings: no

---

## Knowledge Stewardship

- Entry #4141 / #3921 appear to be duplicate entries for the same rule ("UDS dispatch: all arms that use session_id must carry sanitize_session_id"). These look like duplicate records — worth investigating with `context_get` in a follow-up. Not corrected here as the content may differ.
- Stored: nothing novel to store — the NaN guard pattern is already in entry #4133, the session_id consistency rule is in entries #3921/#4141, and the stale-type-column risk is in entry #4144. No new cross-feature anti-pattern visible from this batch.
