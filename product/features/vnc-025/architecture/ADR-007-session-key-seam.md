## ADR-007: `session_key()` Constructor Seam — Document the `(tenant, project, session)` Dimension, Defer the Re-Key

### Context

Goal 6 requires the enterprise composite-key dimension to be kept warm without building it.
Resolved decision 3 settled the approach: re-keying every registry call site across
`session.rs`/`listener.rs`/`tools.rs` hot paths for a dimension OSS never populates
(`tenant = "default"`) is churn with zero OSS behavior change and a real regression surface;
a documented constructor gives enterprise exactly one place to change. The registry key is
already transport-namespaced (`http-{id}` via `prefix_session_id`), which is precedent for
encoding dimensions into the string key rather than the key type.

### Decision

One function in `infra/session_transcript.rs` (keeps `session.rs` from growing further):

```rust
/// Enterprise seam: collapses the (tenant, project, session) composite dimension
/// to the registry's string key. OSS: tenant is always "default" and the function
/// returns `session_id` unchanged (transport namespacing via the existing
/// `http-` prefix is orthogonal and applied earlier, in `prefix_session_id`).
/// Enterprise re-key changes THIS function (and only this function) to emit a
/// composite encoding; no call-site re-key.
pub fn session_key(_tenant: &str, _project: &str, session_id: &str) -> String {
    session_id.to_string()
}
```

vnc-025's new code paths (`apply_transcript_delta`, `clear_transcripts_for_feature`, purge
audit emission) route their key construction through `session_key("default", "", id)`.
Existing call sites are **not** touched — the seam is documented and exercised by the new
surface only; migrating the legacy surface is part of the enterprise re-key itself.

### Consequences

- Easier: enterprise lands by editing one function plus the legacy call sites it explicitly
  chooses to migrate; the dimension is documented in code, not tribal knowledge; zero OSS
  behavior change and zero hot-path cost (the OSS path is a single `to_string` the call sites
  were paying anyway).
- Harder: until enterprise lands, the seam is exercised but degenerate — a reviewer could
  "simplify" it away; the doc comment marks it load-bearing.
- Cross-references: resolved decision 3 (scope), principle 6 (zero-required-infrastructure),
  vnc-024 ADR-005 (sibling enterprise seam on the same policy surface).
