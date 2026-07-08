# Test Plan — isolation-suite (behavioral INV-T/K/C) — PRIMARY GATE

Source: `crates/unimatrix-server/tests/project_routing_integration.rs` (EXTEND — reuse
`build_server`, `wired_router(slugs)`, `drive`, `test_entry`, `entry_count`) + the #800 Python
multi-slug HTTP fixture. This is the durable, mechanism-independent guardrail (ADR-004). Risks:
R-01, R-02 (both Critical), R-09, R-10, R-15, R-16.

AC-01 vehicle: `cargo test -p unimatrix-server --test project_routing_integration`.

## Non-Negotiable Shape (every test below)

- **Bidirectional at N≥2**: A and B registered on one instance; each write driven ONLY through
  its own `/v1/{slug}/…`; assert per data class (i) present-in-own AND (ii) absent-in-every-other,
  in BOTH directions as **distinct named cases** (`_a_driver` / `_b_driver`). Neither direction
  inferred from the other (R-01/#5348).
- **Assembled production wiring only**: POST `transcript_delta` via `route_observe`
  (real `ObserveContext` → `resolver.*_for(&key)` → `dispatch_request`) → read via that slug's
  `McpAdapter` (`cycle_review` / briefing / search). No `SessionRegistry`/`ServiceLayer`
  hand-passed to `dispatch_request`; no server field seeded (R-02/#5285/#4974).
- **No `Arc::ptr_eq` / field-overwrite in the behavioral suite** (AC-06). Those live in the
  white-box section (boot-assertion.md), clearly separated.
- Isolation half keys on **synchronous observable state** (fold result / returned entries), not
  the absence of an async effect (#5427 caveat).

## Invariant → Test Cases

| Invariant | AC | Cases (bidirectional) | Assertion |
|-----------|----|-----------------------|-----------|
| **INV-T1** transcript fidelity (#930) | AC-01 | `test_transcript_fold_fidelity_a` / `_b` | delta → `/v1/{X}/observe` under cycle; `cycle_review` via X folds non-empty candidates/bytes. Both slugs prove own-fold |
| **INV-T2** transcript isolation, identical name | AC-02 | `test_transcript_isolation_identical_cycle_a_driver` / `_b_driver` | A and B both run identical `{phase}-{NNN}`; write to X; X folds X's, other's `cycle_review` for that name folds/counts/**distills** NOTHING of X's. Assert candidate **count** + **distillation-input** exclusion, both directions (R-10, R-15) |
| **INV-T3** pending-entries isolation | AC-03 | `test_pending_entries_isolation_a` / `_b` | pending analysis at X's `cycle_review` present; never observable via the other; both directions |
| **INV-K1** knowledge fidelity | AC-04 | `test_knowledge_own_read_a` / `_b` | knowledge written as X retrievable by X's observe-path briefing/search/compact |
| **INV-K2** knowledge isolation + persistence | AC-04 | `test_knowledge_cross_read_isolation_a` / `_b`, `test_knowledge_no_durable_contamination` | X's observe-path reads NEVER return the other's entries; both directions; PLUS a persistence-level check that distillation cannot durably contaminate the victim store (R-09/R-15) |
| **INV-C1/C2** config governs own | AC-05 | `test_signal_class_counts_per_slug_a` / `_b` (+ observation status, purge) | A's declared config governs only A's observable behavior; B's never governs A's; A and B declare **different** config; derive via `resolve_slug_config`→`build_project_server`, not seeded (R-08) |

## Critical Meta-Test (R-01 — proves the reverse direction is actually exercised)

**`test_negative_control_reverse_misroute_trips_red`** — in a throwaway harness variant,
deliberately mis-wire B's route to resolve A's registry; assert the bidirectional suite goes
**RED**. Confirms the reverse-direction cells actually run — a one-directional-only suite would
stay green here (the exact failure two reviews missed in #5348). Mutation/negative-control check;
kept isolated so it never affects the real gate result.

## INV-T2 Mechanism Detail (R-10)

`take_transcripts_for_feature` folds on `SessionState.feature == fc` ∪ a held-buffer scan
(`infra/session.rs:473-497`). Per-slug registries fix cross-slug commingling, but the sharp test
is the **identical** `{phase}-{NNN}` name. Assert both the candidate **count** AND the
**distillation input** exclude the other slug — not only the returned transcript bytes. Use a
realistic name (e.g. both slugs run `nxs-001`).

## Persistence Blast-Radius (R-15)

After an INV-T2 collision or an INV-K2 cross-read, assert the victim's **durable store** never
contains the other slug's transcript-derived / distilled entries — a leak that feeds persisted
distillation is permanent, not transient. Query the victim store (own `McpAdapter` search /
`entry_count`) after the fold, keyed to the foreign marker.

## AC-07 Parity (R-16)

**`test_https_equals_uds_observe_fidelity`** — drive the **same** input through HTTPS
(`route_observe` → `McpAdapter`) and local UDS; compare `cycle_review` fold / `MetricVector`
field-for-field; **exclude/normalize wall-clock fields** (`computed_at`) to avoid flake (#5285).
Do not re-seed either side; both derive from the same input. Diff-review confirms UDS/stdio
construction paths unchanged (NFR-4).

## Coverage-Enumeration Table (AC-06 — REQUIRED artifact)

The suite ships an explicit enumeration (module-level comment or a `test_coverage_enumeration`
docs test) stating, per invariant, **behavioral vs white-box** and naming the exceptions:

| Invariant / field | Coverage kind | Vehicle |
|-------------------|---------------|---------|
| INV-T1/T2/T3, INV-K1/K2, INV-C1/C2 public surfaces | **behavioral** | route_observe → McpAdapter, N≥2 bidi |
| `store_config` (byte-limit) | **white-box only (AC-06 exception)** | bidirectional wiring-pin (project-provisioner.md) + boot sentinel |
| `inference_config` (briefing blend) | **white-box only (AC-06 exception)** | bidirectional wiring-pin + boot sentinel |
| handle-identity (registry/pending) | white-box complement | `Arc::ptr_eq` pin (boot-assertion.md) — NOT in the behavioral suite |

Absence of this table, or silent omission of `store_config`/`inference_config`, is a gate
failure (SR-05).

## AC-06 Grep-Gate

- The behavioral suite contains **no** `Arc::ptr_eq`, **no** `dispatch_request(registry=…)`
  hand-pass, **no** field-overwrite of a server config field.
- Every isolation case runs at N≥2 with both-direction cases present.

## #800 HTTP Fixture Mirror

The same invariants are proven at the true MCP wire level in
`product/test/infra-001/suites/test_project_isolation.py` (see OVERVIEW.md — Integration Harness
Plan). The Rust suite is the fast in-process gate; the #800 fixture proves them over the real
HTTPS transport (the surface a rewire breaks) and is C6's path to `proven`. Both use
marker-keyed read-as-barrier + mutually-non-substring markers (#5347).

## Coverage Trace
| Risk / AC | Test |
|-----------|------|
| R-01 | every invariant bidirectional + negative-control meta-test |
| R-02 | assembled-wiring only + grep-gate |
| R-09/R-15 | INV-K2 + persistence check |
| R-10 | INV-T2 identical-name, count + distillation-input |
| R-16 | AC-07 parity, wall-clock excluded |
| AC-01…AC-07 | rows above |
| AC-06 | enumeration table + grep-gate |
