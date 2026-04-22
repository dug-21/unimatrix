# Test Plan: Capability::as_audit_str (infra/registry.rs)

## Component Summary

A new method `as_audit_str()` is added to the `Capability` enum in
`unimatrix-server/src/infra/registry.rs`. It provides a compile-time-exhaustive match
returning lowercase string constants for use in `AuditEvent.capability_used`.

The method must NOT contain a wildcard arm (`_`). Future `Capability` variant additions
that omit an `as_audit_str` arm will produce a compile error (desired behavior).

---

## Unit Tests

### CAP-U-01: `Capability::Read.as_audit_str()` returns `"read"`

**Risk**: R-09, AC-11
**Assert**: `Capability::Read.as_audit_str() == "read"`
Return type is `&'static str` — no allocation.

---

### CAP-U-02: `Capability::Write.as_audit_str()` returns `"write"`

**Risk**: R-09, AC-11
**Assert**: `Capability::Write.as_audit_str() == "write"`

---

### CAP-U-03: `Capability::Search.as_audit_str()` returns `"search"`

**Risk**: R-09, AC-11
**Assert**: `Capability::Search.as_audit_str() == "search"`

---

### CAP-U-04: `Capability::Admin.as_audit_str()` returns `"admin"`

**Risk**: R-09, AC-11
**Assert**: `Capability::Admin.as_audit_str() == "admin"`

---

### CAP-U-05: No wildcard arm in the match — exhaustiveness enforced

**Risk**: R-09
**Assert**: Code inspection confirms the `match self { ... }` in `as_audit_str` contains
exactly four arms (one per variant) and NO wildcard `_ => ...` arm.

This is enforced at compile time: if a new `Capability` variant is added, the compiler
will reject the incomplete match. The test plan notes this as a structural guarantee, not
a runtime assertion.

**Optionally**: Add `#[deny(unreachable_patterns)]` attribute to the `as_audit_str` function
body to make the compile-time guarantee explicit.

---

### CAP-U-06: Return type is `&'static str` — no heap allocation

**Risk**: R-09 (performance contract)
**Assert**: The return type annotation on `as_audit_str` is `&'static str`. Each arm returns
a string literal, not a `String` or `.to_string()` value.

---

### CAP-U-07: All 12 tool `capability_used` values match this table (integration with tools.rs)

**Risk**: R-09, AC-11
Cross-reference with TOOL-U-04 in `tools.md`. Each tool handler must use
`Capability::X.as_audit_str().to_string()` for `capability_used`, not an inline literal.

Expected mappings verified by TOOL-U-04:

| Tool | `Capability::as_audit_str()` |
|------|------------------------------|
| `context_search`, `context_lookup`, `context_briefing` | `"search"` |
| `context_get`, `context_status`, `context_retrospective` | `"read"` |
| `context_store`, `context_correct`, `context_deprecate`, `context_quarantine`, `context_cycle` | `"write"` |
| `context_enroll` | `"admin"` |

---

## Compile-Time Contract

The exhaustive match in `as_audit_str()` is the primary risk mitigation for R-09. No
integration test can verify the absence of a wildcard arm — only code inspection and
compilation can. The test plan records this as a code review checkpoint for the Stage 3c
reviewer.

**Reviewer checklist item**: Verify the `match` block in `as_audit_str()` has exactly 4 arms
and no `_ =>` arm before the Stage 3c gate passes.
