# Security Review Report: vnc-016

**Agent ID**: vnc-016-security-reviewer
**PR**: #605
**Feature**: vnc-016
**GH Issue**: #603
**Date**: 2026-05-18
**Risk Level**: LOW
**Blocking Findings**: None

---

## Summary

Three production code files were modified in this PR: a one-token SQL column-name fix in `read.rs`, a new `write_capable: bool` field added to `UsageContext` in `usage.rs` with trust-level gate replacement in both `record_mcp_usage` and `record_hook_injection`, and `write_capable` propagated to all five `UsageContext` construction sites in `tools.rs`. The Python test harness gains a backward-compatible keyword parameter on `context_store`. No new dependencies, no new untrusted input surfaces, no hardcoded secrets.

---

## Findings

### Finding 1 — Access Control Fix is Structurally Correct (Informational)

**Severity**: Informational
**Location**: `crates/unimatrix-server/src/services/usage.rs` lines 212–218 and 272–280

The old gate checked `TrustLevel::System | Privileged | Internal`, excluding Restricted-trust agents even when they held `Capability::Write`. The replacement — `if ctx.write_capable` — is derived from the result of `require_cap(Capability::Write)` at `tools.rs:655`, which runs before any `UsageContext` is constructed.

All five `UsageContext` construction sites in `tools.rs` verified: search (`false`), lookup (`false`), store (`true`), get (`false`), briefing (`false`). No `UsageContext` constructed outside `tools.rs` and `usage.rs` test code. No Default derivation — any future omission is a compile error.

**Blocking**: No.

---

### Finding 2 — SQL Fix: No Injection Risk (Informational)

**Severity**: Informational
**Location**: `crates/unimatrix-store/src/read.rs` line 1618

Change is `fe.feature_cycle = ?1` to `fe.feature_id = ?1`. Bind via sqlx parameterized API — no string interpolation. Not vulnerable to injection.

**Blocking**: No.

---

### Finding 3 — Residual Silent Failure Path (Pre-existing, Non-blocking)

**Severity**: Low
**Location**: `crates/unimatrix-server/src/mcp/tools.rs` lines 2169–2177

`query_stale_prerequisite_edges_for_cycle` errors are swallowed by `unwrap_or_else` with `tracing::warn!`. Intentionally out of scope for vnc-016. GH Issue #604 filed and referenced in PR description per IMPLEMENTATION-BRIEF.md requirement.

**Blocking**: No.

---

### Finding 4 — Hook Injection Gate: Correct but No Integration Test Coverage (Low)

**Severity**: Low
**Location**: `crates/unimatrix-server/src/services/usage.rs` lines 272–280

`record_hook_injection` gate is correctly fixed. No external caller constructs a `UsageContext` with `AccessSource::HookInjection` in current codebase. Coverage gap is known (C-12); gate logic is correct. Not a security defect.

**Blocking**: No.

---

## Blast Radius Assessment

Low. The only site that sets `write_capable: true` is unreachable without `require_cap(Write)` returning `Ok`. Rust compiler exhaustive struct construction prevents silent omissions at future call sites.

## Regression Risk

Low. `write_capable: bool` with no Default is the strongest possible API-level enforcement. All existing `UsageContext` literals in test code were updated. Python harness change is keyword-only with `None` default — zero disruption to existing callers.

## Verdict

**Risk Level: LOW. No blocking findings. Merge readiness: READY.**
