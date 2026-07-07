# FINDINGS (INTERNAL TRACK): Authorization & anti-poison controls for `context_tag`

**Spike**: ass-094 (internal track)
**Date**: 2026-07-06
**Approach**: investigation + evaluation
**Confidence**: directional (code-anchored, file:line evidence; no PoC)

Scope for this track: **A1, A2, A3, A4, B3, B4, B5**, anchored in the codebase. External
poison-pattern / vocabulary-governance / labeling-authz research is a separate track. Prior art
`ass-093/FINDINGS.md` is taken as given (tags outside the hash; `context_tag` is the chosen op).

---

## Model as it exists today (shared basis for all answers)

- **Capability enum** (`crates/unimatrix-store/src/schema.rs:262-276`): exactly five flat variants —
  `Read=0, Write=1, Search=2, Admin=3, SessionWrite=4`. Exhaustive `as_audit_str` (schema.rs:283-292)
  and `TryFrom<u8>` (schema.rs:294-307); adding a variant is a compile-checked extension (ADR-006/SR-05).
- **Assignment** is a flat `capabilities: Vec<Capability>` per agent (`AgentRecord`, schema.rs:317).
  Auto-enroll permissive -> `[Read, Write, Search]`; strict -> `[Read, Search]`; `session_caps` config
  can override (dsn-001) (`infra/registry.rs:39-52, 68-78`, tests 223/436/486). `system` -> all four incl.
  Admin (registry.rs:187-194); `human` -> `Privileged`.
- **Trust hierarchy** exists (`TrustLevel`: System=0, Privileged=1, Internal=2, Restricted=3,
  schema.rs:234-246) but the capability gate **does not consult it**: `require_capability` checks only
  `record.capabilities.contains(&cap)` (registry.rs:81-100). Trust level is recorded, never gated on.
- **Gating** (`require_cap`, server.rs:631-644 -> registry.rs:92): every content-mutating MCP op gates on
  the same `Capability::Write` (tools.rs:884 store, 1245 correct, 1447, 3632, 3872); Admin gates
  enrollment/status-admin (tools.rs:2015, 2139).
- **Two independent write-rate mechanisms** (do not conflate):
  1. **Live** per-caller sliding-window limiter — `SecurityGateway::check_write_rate`
     (`services/gateway.rs:35-101, 166-169`), default `write_limit=60`/`window_secs=3600`
     (gateway.rs:110-131), keyed by `CallerId`, in-memory, resets on restart, `UdsSession` exempt
     (gateway.rs:60). Enforced at `store_ops.rs:114` and `store_correct.rs:29`.
  2. **Persistent** counter — `audit_write_count_since` (`audit.rs:79-92`), SQL `COUNT` over `audit_log`
     for `operation IN ('context_store','context_correct')`. **No live non-test consumer exists in the
     current tree** (only wrapper `infra/audit.rs:55` + crt-001 tests); it is a latent SLN1 budget signal.
- **Provenance fields are largely placeholders today**: `credential_type` is hardcoded `"none"` at every
  MCP write site (tools.rs:826, 1185, 1492, 1628, ...) — reserved for future credentialed/HTTP transport;
  `trust_source` is hardcoded `"agent"` for store/correct (tools.rs:986, 1319), `"system"/"auto"/"neural"`
  for server-generated (background.rs:1866, 4114) — it records write **origin**, not credential, and is
  never read as an auth input; `agent_id` is agent-declared; `agent_attribution` = `client_type`
  (tools.rs:828).

---

## Findings

### A1 — Capability granularity
**Q:** Is `Capability::Write` the correct gate for tag mutation, or is a finer/separate cap (or add/remove
split) warranted? What does the enum/registry support and how are caps assigned?

**Answer:** The `Capability` enum is deliberately coarse (5 flat variants, schema.rs:262-276) and
assignment is a flat per-agent `Vec<Capability>` (schema.rs:317). Today **all** content mutation — store,
correct, and the only existing tag-write path (`update()`'s DELETE-all/re-INSERT) — is gated by a single
global `Capability::Write` (tools.rs:884/1245/...). There is no finer axis: no `Tag` cap, no add/remove
split, no per-namespace or per-trust-tier discrimination, and `require_capability` ignores `trust_level`
entirely (registry.rs:85). So for tags, "Write alone" is literally the current state.

**Evidence:** schema.rs:262-276, 317; registry.rs:81-100; tools.rs:884, 1245.

**Recommendation:** Reuse `Capability::Write` as the **baseline** gate for the `context_tag` op (consistent
with the enum's coarse design and ass-093) — but do **not** stop there. Adding a variant is cheap and
compile-safe (exhaustive `as_audit_str`/`TryFrom`), yet a proliferation of coarse capabilities does not
express the real requirement, which is *namespace x trust-tier*, not a new global verb. Keep Write for
free-form tags; gate **reserved/trust-bearing namespaces** on an elevated bar (see A3) rather than minting
`Capability::Tag`. Do **not** split add vs remove at the capability layer — handle that asymmetry in
namespace policy + audit (see A4).

---

### A2 — Mutation scope
**Q:** Can an agent tag any entry, or only ones it authored / within its project? What ownership/project
scoping exists? Any per-entry authorship check today?

**Answer:** Two boundaries, only one enforced:
- **Cross-project** is enforced structurally by **1-client:1-project / per-slug database** (vnc-034,
  `projects.rs:109` — each slug owns its own `{base}/{slug}/` DB tree). An agent bound to project X's DB
  physically cannot reach project Y's entries; no per-entry check is needed for that boundary.
- **Within a project** there is **no authorship/ownership scoping whatsoever.** `entries` carries
  `created_by` and `modified_by` columns (`db.rs:558-559`) and they are populated, but **no code path
  compares caller to `created_by` before a mutation** (grep for such a check returns none). The gate is
  global `Write` (registry.rs:85). Any Write-capable agent can correct/deprecate/quarantine — and would be
  able to tag — **any** entry in the project, regardless of who authored it.

**Evidence:** projects.rs:109; db.rs:558-559; registry.rs:81-100; absence of any `created_by ==`/ownership
comparison in the mutation paths.

**Recommendation:** Do **not** add authorship scoping for tags. A shared, cross-annotated corpus is the
point of the retrieval model; agents legitimately tag each other's entries. Treat `created_by`/`modified_by`
as the **provenance substrate** (accountability via audit), not an auth gate. The meaningful in-project
scoping boundary is the **reserved namespace** (A3), not authorship. Record `modified_by` on every tag
mutation so accountability survives even though gating stays global.

---

### A3 — Protected namespaces
**Q:** What could enforce a reserved-namespace boundary (e.g. `status:*`) so a trust-bearing tag like
`proven` isn't writable by any writer? Existing vocabulary/validation to build on?

**Answer:** The substrate already exists. `infra/outcome_tags.rs` is a working **controlled-vocabulary /
reserved-namespace validator**: for `category == "outcome"` it parses structured `key:value` tags, rejects
keys outside a `RECOGNIZED_KEYS` allow-list, requires `type`, forbids duplicates, and validates per-key
value sets (`VALID_TYPES/RESULTS/PHASES`). It is already invoked in the write path (tools.rs:895-900). The
category registry (`self.categories.validate`, tools.rs:891-893) is a second allow-list precedent. So the
model already has (1) a write-time interception point, (2) a `key:value` structured-tag convention, and
(3) allow-list value validation. A `status:*` reserved namespace slots directly onto this.

**Who may write `proven` today:** nobody is specially gated — any `Write` agent could. That is the gap.

**Evidence:** infra/outcome_tags.rs:9-80 (RECOGNIZED_KEYS, VALID_* sets, structured parse); tools.rs:891-900.

**Recommendation:** Introduce a **reserved-prefix policy table** (domain-agnostic: a set of prefixes such as
`status:` plus a min-authorization per prefix), validated at the `context_tag` write site exactly where
`validate_outcome_tags` already runs. Two rules: (1) **value allow-list** — a `status:` tag's value must be
in `{missing,partial,proven,claimed}` (reuse the outcome_tags pattern); (2) **elevated authorization** — a
reserved-namespace write requires more than bare `Write`. Bind that bar to the **existing but currently
unused `TrustLevel`** (require System/Privileged/Internal for reserved prefixes) — this activates the trust
hierarchy that already exists (schema.rs:234-246) without minting a bespoke status permission, satisfying
the domain-agnostic constraint. Free-form (non-prefixed) tags stay on plain `Write`.

---

### A4 — Add vs remove asymmetry
**Q:** In the write path, is removal (burying trusted data / erasing status) more dangerous than adding?
Should removal carry distinct controls?

**Answer:** Yes, removal is more dangerous for the trust-bearing case. Adding a bad tag is **additive and
visible** — an extra retrieval-steering label; the entry still surfaces and the noise is detectable.
Removal is a **silent erasure with immediate effect**: because tag-filtered reads load tags live from
`entry_tags` on every query (ass-093 Q2; graph_read_filter.rs:186-209), removing a discriminating tag
**buries** an entry out of the next tag-filtered result set instantly, and removing `status:proven` erases
a delivery claim. Worse, there is **no tag version history** — the audit log is the *only* record a removed
tag ever existed (ass-093 Q5; append-only, not a hash chain). So removal's blast radius (burial + status
erasure + audit-as-sole-recovery) exceeds add's.

**Evidence:** graph_read_filter.rs:186-209 (live tag reads); audit.rs:46-64 (append-only log, no tag
history); precedent `emit_edge_cleanup_audit` (server.rs:659-690) already preserves removed-edge tuples in
a distinct audit record precisely because an eager delete is irreversible.

**Recommendation:** Asymmetric controls at the op/audit layer, **not** at the capability layer:
1. Audit **both** add and remove; for remove/replace, recording the **prior value** in audit metadata is a
   hard requirement (mirror `emit_edge_cleanup_audit` — a reconstructable record for an irreversible delete).
2. For reserved namespaces, **removal/overwrite requires the same elevated bar as setting** (A3) — you
   cannot erase a `proven` claim with less authority than it took to assert it.
3. Model a status change as an atomic **replace** (old value + new value logged together), not a bare
   remove followed by an add, so oscillation and demotion are legible in one audit record.
No add/remove capability split is warranted.

---

### B3 — Attribution / provenance
**Q:** What does the audit record capture, and what must a tag mutation record to stay forensically
trustworthy?

**Answer:** The audit record (`AuditEvent`, schema.rs:358-374; INSERT audit.rs:46-67) captures: `event_id`
(monotonic), `timestamp`, `session_id`, `agent_id`, `operation`, `target_ids`, `outcome`,
`detail`, `credential_type`, `capability_used`, `agent_attribution`, `metadata` (JSON; empty coerced to
`"{}"`, audit.rs:33-44). It is an append-only monotonic-id log, **not** a hash chain (ass-093). Caveat on
trust: `credential_type` is hardcoded `"none"` at all MCP write sites (tools.rs:826...), `trust_source`
records origin not credential and is hardcoded `"agent"` (tools.rs:986/1319), and `agent_id` is
self-declared — so today's attribution is **declarative, not authenticated**.

**Evidence:** schema.rs:358-374; audit.rs:19-67; tools.rs:826, 986, 1319.

**Recommendation:** A `context_tag` mutation must record, as a **hard requirement**: `operation="context_tag"`
(dedicated label so it is filterable and rate-countable — ass-093), `target_ids=[entry_id]`, `agent_id`,
`session_id`, `capability_used`, and a `metadata` JSON `{action: add|remove|replace, namespace/prefix, tag,
prior_value, new_value}` — `prior_value` mandatory for remove/replace (ass-093 Q5 + A4). For
reserved-namespace mutations, additionally stamp the writer's **`trust_source`/`TrustLevel`** into metadata
so a `proven` flip records *who (by trust tier)* asserted it. Name the residual risk explicitly: until
credentialed transport lands (`credential_type` real, not `"none"`), the trail is only as trustworthy as
the declared identity — this bounds forensic value and should be documented, not silently assumed away.

---

### B4 — Rate / budget & abuse
**Q:** How should tag mutation fold into the SLN1 write-budget (`audit_write_count_since`, audit.rs:79-92)?
What per-agent / per-entry controls exist or are needed against tag flooding?

**Answer:** There are two mechanisms and a tag op must engage both, plus a gap neither closes:
- **Live throttle** — `SecurityGateway::check_write_rate` (gateway.rs:166-169, default 60 writes/hr/caller)
  is the only *enforced* write limit today, wired at `store_ops.rs:114` / `store_correct.rs:29`. If
  `context_tag` does not call it, tag mutation is an **unthrottled** write vector — and tags are a *cheaper*
  poison vector than store (no re-embed, immediate retrieval effect, per SCOPE).
- **Persistent budget signal** — `audit_write_count_since` counts only `context_store`/`context_correct`
  (audit.rs:83). ass-093 already flags that any new op is invisible to it unless the `operation IN (...)`
  list is extended. (Note: this counter currently has no live enforcement consumer — it is latent.)
- **Per-entry control does not exist.** The sliding window is keyed by `CallerId` only (gateway.rs:56-101),
  never by entry. A status-oscillation or single-entry burial/dilution attack needs very few calls and
  slips under a per-caller cap.

**Evidence:** gateway.rs:35-101, 110-131, 166-169; store_ops.rs:114; store_correct.rs:29; audit.rs:79-92.

**Recommendation:**
1. **MUST** call `gateway.check_write_rate` in the `context_tag` service path — this is the load-bearing
   control; without it the op bypasses the only live write throttle.
2. **MUST** add `'context_tag'` to `audit_write_count_since`'s `operation IN (...)` list (audit.rs:83) so
   the persistent SLN1/curation signal counts it (ass-093 gap, closed deliberately not by omission).
3. **NEW control needed:** a per-`(entry, reserved-namespace)` mutation cadence guard — reject rapid status
   flips / more than N reserved-namespace mutations per entry per window — because oscillation/forgery on a
   single high-value entry is cheap and the per-caller window will not catch it.

---

### B5 — Trust tiers / quarantine
**Q:** Does `trust_source` gate authoritative tags? Can quarantined/deprecated entries be tagged? Does
tagging interact with trust/lifecycle state?

**Answer:**
- **Trust does not gate anything today.** `trust_source` is a descriptive origin string set at the write
  site (tools.rs:986/1319), never read as an auth input. `TrustLevel` is stored but `require_capability`
  consults only the flat capability list (registry.rs:85). So **no trust tier currently gates who may set
  which tag** — a `Restricted` worker with `Write` has the same tagging authority as `human`/Privileged.
- **Lifecycle state currently blocks the indirect tag path — but only incidentally.** The sole existing
  tag-write route is `context_correct`, which **refuses** deprecated ("cannot correct a deprecated entry")
  and quarantined ("cannot correct quarantined entry; restore first") entries (write_ext.rs:471-482);
  `context_store` edge validation likewise rejects quarantined targets (tools.rs:938-940). A **new direct
  `context_tag` op would bypass all of these** unless it re-implements the lifecycle guard.

**Evidence:** tools.rs:986, 1319 (trust_source hardcoded); registry.rs:85 (trust_level unused by gate);
write_ext.rs:471-482 (correct refuses deprecated/quarantined); tools.rs:938-940 (quarantined edge target
rejection).

**Recommendation:** Tagging **must** interact with lifecycle and trust state, enforced *in the new op* (not
inherited):
1. **Forbid tagging quarantined entries** — quarantine means isolated/untrusted; allowing re-tag could
   re-steer retrieval toward isolated content or forge `proven` on quarantined data.
2. **Deprecated entries:** forbid reserved/trust-bearing tags (cannot assert `proven` on superseded
   knowledge); the safe default is to mirror `context_correct` and refuse tag mutation outright.
3. **Activate `TrustLevel`** for reserved-namespace writes (see A3): authoritative tags (`status:proven`)
   require an elevated tier. This is the intended home for "who may set authoritative tags," and the
   hierarchy already exists unused.

---

## Verdict on the challengeable hypothesis

**"`Capability::Write` alone is sufficient" — NO.** `Write` is the correct *baseline* gate (reuse it), but
it is necessary, not sufficient. Concretely, from the code:

1. **No discrimination.** `Write` is a global flat cap; `require_capability` ignores `trust_level`
   (registry.rs:85) and no ownership check exists (A2). Any Write agent can forge or erase a `proven` claim
   on **any** entry in the project.
2. **Guard bypass.** A direct `context_tag` op sidesteps the lifecycle guards that currently protect
   quarantined/deprecated entries via the correct-path (write_ext.rs:471-482) (B5).
3. **Budget bypass.** `Write` does not wire the op into the live rate limiter (gateway.rs) or the persistent
   counter (audit.rs:83); both need explicit extension or tag mutation is an unbudgeted poison vector (B4).
4. **No per-entry control** exists, so single-entry oscillation/burial is uncaught (B4).

**Minimum sufficient set:** `Write` (baseline) **+** reserved-namespace validation with an elevated bar
bound to the existing `TrustLevel` (A3) **+** lifecycle-state guard in the op (B5) **+** `check_write_rate`
wiring and `audit_write_count_since` inclusion (B4) **+** mandatory prior/new value in audit metadata (B3/A4)
**+** a per-`(entry, reserved-namespace)` cadence guard (B4). All of these reuse existing primitives (the
outcome_tags validator, the trust hierarchy, the gateway limiter, the audit metadata lane); none requires a
schema change.

---

## Unanswered Questions

None for the assigned internal-track questions (A1, A2, A3, A4, B3, B4, B5). Two items are implementation
design choices, not research blockers: (1) exact min-`TrustLevel` per reserved prefix and whether the
prefix set is config-driven; (2) the numeric per-entry cadence threshold. Both depend on operational tuning.

---

## Out-of-Scope Discoveries

- **`audit_write_count_since` has no live enforcement consumer** in the current tree (only wrapper +
  crt-001 tests). The persistent SLN1 write-budget is effectively latent; the live throttle is the
  in-memory gateway limiter, which resets on restart and exempts UdsSession (gateway.rs:60). Worth a
  separate look at whether SLN1's persistent budget is actually wired anywhere. Rationale: a control assumed
  active in planning may be dormant.
- **`credential_type` is hardcoded `"none"` and `agent_id` is self-declared** across all MCP write sites —
  audit attribution is declarative, not authenticated, until credentialed transport lands. Bounds the
  forensic value of every audited op, not just tags. Rationale: affects any trust argument that leans on the
  audit trail.
- **`TrustLevel` is stored on every agent but never consulted by the capability gate** (registry.rs:85).
  A whole trust dimension is carried and ignored; several controls above (A3, B5) could activate it.
  Rationale: latent authorization capacity already in the schema.

---

## Recommendations Summary

- **A1 (granularity):** Reuse `Capability::Write` as baseline; do not mint `Capability::Tag` or split
  add/remove at the cap layer. The real axis is namespace x trust-tier, handled in A3.
- **A2 (scope):** Cross-project isolation is structural (per-slug DB, vnc-034); in-project there is zero
  authorship scoping and no per-entry check. Keep tags collaborative — use `created_by`/`modified_by` +
  audit for accountability, not gating.
- **A3 (namespaces):** Build on the existing `outcome_tags` controlled-vocabulary validator; add a
  reserved-prefix policy (`status:*`) with a value allow-list and an elevated authorization bar bound to the
  existing (currently unused) `TrustLevel`.
- **A4 (add vs remove):** Removal is more dangerous (silent burial + status erasure, audit is sole recovery).
  Require prior-value in audit, gate reserved-namespace removal at the same bar as setting, model status
  changes as atomic replace. No capability split.
- **B3 (provenance):** Mandate `operation="context_tag"`, `capability_used`, and metadata
  `{action, namespace, tag, prior_value, new_value}` (+ writer trust tier for reserved namespaces); prior
  value mandatory on remove/replace. Flag that attribution is declarative until credentialed transport.
- **B4 (rate/budget):** MUST wire `context_tag` into `gateway.check_write_rate` and MUST add it to
  `audit_write_count_since`'s op list; add a NEW per-`(entry, reserved-namespace)` cadence guard against
  oscillation/burial (no per-entry control exists today).
- **B5 (trust/lifecycle):** Neither `trust_source` nor `TrustLevel` gates tagging today. The new op must
  refuse quarantined entries, refuse reserved tags on deprecated entries, and require an elevated `TrustLevel`
  for authoritative tags — enforced in the op, not inherited from the correct-path.
- **Verdict:** `Capability::Write` alone is **NOT sufficient**. It is the right baseline plus a required
  control set (namespace validation + trust bar + lifecycle guard + rate wiring + audit prior/new value +
  per-entry cadence), all reusing existing primitives with no schema change.
