# Alignment Report: nxs-011

> Reviewed: 2026-03-17
> Artifacts reviewed:
>   - product/features/nxs-011/architecture/ARCHITECTURE.md
>   - product/features/nxs-011/specification/SPECIFICATION.md
>   - product/features/nxs-011/RISK-TEST-STRATEGY.md
>   - product/features/nxs-011/architecture/ADR-001-pool-acquire-timeout.md
>   - product/features/nxs-011/architecture/ADR-002-write-transaction-retirement.md
>   - product/features/nxs-011/architecture/ADR-003-migration-connection-sequencing.md
>   - product/features/nxs-011/architecture/ADR-004-sqlx-data-json-placement.md
>   - product/features/nxs-011/architecture/ADR-005-native-async-trait.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/nxs-011/SCOPE.md
> Scope risk source: product/features/nxs-011/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | All five W0-1 pillars (dual-pool, analytics queue, async-native, backend abstraction, sqlx compile-time checking) are fully addressed |
| Milestone Fit | PASS | Wave 0 prerequisite role confirmed; feature explicitly targets unblocking W1 and W2 |
| Scope Gaps | WARN | One open question (ExtractionRule async boundary) is unresolved and blocks delivery; documented as R-08 in risk strategy |
| Scope Additions | WARN | FR-16 / NF-05 shed-counter in context_status extends beyond SCOPE.md ACs; SR-08 recommendation was adopted as a spec requirement — this is beneficial but constitutes a scope addition |
| Architecture Consistency | WARN | Single-file / dual-pool topology is internally consistent, but nxs-011 does not introduce the `analytics.db` file split that W1 features reference; the path from single-file to split is unplanned |
| Risk Completeness | PASS | All 10 scope risks (SR-01 through SR-10) are traced to architecture risks and test scenarios; 15 risks, 44 minimum scenarios, coverage summary present |

**Overall gate recommendation**: PASS WITH CONDITIONS — three items below require human acknowledgement before delivery begins.

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | ExtractionRule async resolution (R-08) | SCOPE.md Q4 / architecture open question 1: the ExtractionRule trait async boundary is unresolved. The architecture recommends either full async trait conversion (21 rules) or a `block_on` bridge (known panic risk). Neither path is decided. Risk strategy marks R-08 as a delivery blocker ("must be resolved before delivery begins"). The gap is documented but the decision is deferred. |
| Addition | FR-16 / NF-05 / AC-18: shed counter in context_status | SCOPE.md AC-15 covers WARN log observability of shed events. The spec adds FR-16 (shed counter exposed in context_status MCP output), NF-05 (cumulative counter since store open), and AC-18 (context_status integration test). This extends the scope of AC-15 into the MCP tool layer. The addition is directly traceable to SR-08's recommendation and is architecturally sound; it is not in the original SCOPE.md acceptance criteria. |
| Addition | AC-17: migration regression harness covering all 12 version transitions | SCOPE.md AC-11 requires all 16 migration integration tests to pass. The spec adds AC-17, a new regression harness covering all 12 version transitions starting from schema-less state. This is additive (SR-04 recommendation adopted). It strengthens the gate rather than changing the contract. |
| Addition | AC-19: Store::close() drain task lifecycle test | SCOPE.md AC-06 covers AnalyticsQueue semantics. The spec adds AC-19 (integration test that close() awaits task exit; pool connection count returns to 0). Traceable to SR-09. Additive and correct. |
| Addition | AC-20: impl-completeness tests replacing dyn object-safety tests | Not in SCOPE.md. Required as a consequence of ADR-005 (RPITIT makes dyn EntryStore invalid). Not a scope expansion in substance — it replaces an existing test category that the migration invalidates. |
| Simplification | SCOPE.md Q5: AsyncVectorStore / AsyncEmbedService disposition | SCOPE.md left Q5 open; the spec resolves it as C-06 (untouched, out of scope). Documented with rationale. Acceptable. |
| Simplification | SCOPE.md Q4: SqliteWriteTransaction replacement API shape | SCOPE.md left Q4 open; ADR-002 resolves it as direct `pool.begin().await?` at call sites, no typed wrapper. Documented with rationale. Acceptable. |
| Simplification | SCOPE.md Q1: EntryStore trait object-safety | SCOPE.md left Q1 open; ADR-005 resolves it as RPITIT / non-object-safe. Documented with rationale. Acceptable. |

---

## Variances Requiring Approval

### VARIANCE-01 (WARN): analytics.db / knowledge.db File Split Is Unaddressed by nxs-011

**What**: The product vision's W1+ sections (W1-1 Typed Graph, W3-1 Confidence Weights, W2-1 Container Packaging, W2-3 Multi-Project Routing, and the "What's Preserved Throughout" in-memory hot path rule) consistently refer to `analytics.db` and `knowledge.db` as distinct database files. For example:
- W1-1: "Persist edges to `GRAPH_EDGES` in `analytics.db`"
- W2-1 container packaging: separate `unimatrix-knowledge` and `unimatrix-analytics` volumes
- W3-1: "Learned confidence weights live in `analytics.db` (`confidence_weights` table)"
- Vision non-negotiable #7: "`analytics.db` is eventually consistent."

nxs-011 explicitly rejects the file split (SCOPE.md Non-Goals: "Database file split: A two-file architecture was considered and rejected (product vision Decision 4). This feature uses a single file with two pool handles."). The product vision's Decision 4 section (p. 1125) confirms: "W0-1 write pool topology — two concrete choices before implementation begins: pool sizing and analytics queue capacity and shed policy." This refers to configuring the single-file dual-pool topology, not approving a file split.

The tension is that W0-1 intentionally defers the file split, but the W1-1, W2-1, W2-3, and W3-1 sections use `analytics.db` language as if the split has already occurred. There is no Wave or feature in the roadmap that introduces the file split between nxs-011 and W1-1.

**Why it matters**: If W1-1 (Typed Graph) proceeds assuming `GRAPH_EDGES` lives in a separate `analytics.db`, the implementation will either:
(a) Place `GRAPH_EDGES` in the same single SQLite file (violating W1-1's vision language and the W2-1 separate-volumes architecture), or
(b) Introduce the file split at W1-1 time without a scoped feature to do it (unplanned architectural work embedded in W1-1).

Neither is explicitly sanctioned. The nxs-011 architecture does not describe a transition path from single-file to split-file. Downstream W1+ architects will encounter this gap.

**Recommendation**: Human acknowledgement needed. One of:
1. Accept that `analytics.db` in W1+ vision text is shorthand for "analytics tables in the current single file" — document this explicitly in the product vision or as a standing decision.
2. Plan a discrete file-split feature (between nxs-011 and W1-1) that introduces the `analytics.db` / `knowledge.db` separation.

This is a WARN, not a BLOCK — nxs-011 is internally consistent with Decision 4. The gap lives in the W1+ planning horizon, not in nxs-011 itself. Flag it before W1-1 scope is written.

---

### VARIANCE-02 (WARN): ExtractionRule Async Boundary Is Unresolved and Blocks Delivery

**What**: ARCHITECTURE.md open question 1 and RISK-TEST-STRATEGY.md R-08 both identify that `ExtractionRule::evaluate()` in `unimatrix-observe` is called from the server's async background task. After nxs-011, the implementation will need to either:
(a) Convert all 21 `ExtractionRule` implementations to `async fn` — scope expansion touching all detection rules.
(b) Use `Handle::current().block_on()` as an intermediate bridge — which panics when called from within a tokio worker thread ("cannot start a runtime from within a runtime"), classified as High severity / High likelihood in R-08.

The risk strategy states: "This risk must be resolved before delivery begins." The architecture states: "Flag for delivery agent wave 1."

The open question is present in both SCOPE.md (Q4, though labelled as SqliteWriteTransaction — the actual observe crate boundary is open question 1 in ARCHITECTURE.md) and is unresolved in the spec.

**Why it matters**: If delivery proceeds without resolving this, the server will either fail to compile (if block_on bridge is attempted in a spawn context) or crash at runtime (if bridge is attempted from within a tokio runtime). This affects all 21 detection rules in `unimatrix-observe` — the observation pipeline would be non-functional.

**Recommendation**: Block delivery start until the ExtractionRule path is decided. The decision is binary:
- Full async conversion of `ExtractionRule` trait: correct, in-scope for nxs-011 (C-09 already mandates observe crate migration in the same wave), adds implementation volume for 21 rules.
- A spawned blocking task bridge: wrap the sync `evaluate()` call in `tokio::task::spawn_blocking` at the call site in `background.rs` rather than `block_on` inside evaluate — this is safe from a tokio runtime perspective and avoids touching all 21 rule implementations.

The spec should record this decision explicitly. The delivery agent cannot proceed with the observe migration (FR-14) without it.

---

### VARIANCE-03 (WARN): Scope Addition — shed_events_total in context_status Extends MCP Tool Contract

**What**: SCOPE.md AC-15 specifies that shed events be "logged at WARN level with a count and the queue capacity." The spec (FR-16, NF-05, AC-18) extends this to expose `shed_events_total` in the `context_status` MCP tool output — a change to a live MCP tool's response schema.

This is not in SCOPE.md. The SR-08 risk assessment recommended it, and the spec writer adopted it. The addition is architecturally sound and operationally valuable. However:
- It modifies the `context_status` response structure (adds a field to the storage health section).
- It requires changes to the `unimatrix-server` context_status handler in addition to the store crate changes.
- MCP clients that parse `context_status` output (including documentation, test fixtures, and any external tooling) will need to handle the new field.

**Why it matters**: Scope additions to MCP tool response schemas should be explicitly approved, even when beneficial. The vision's non-negotiable for audit completeness and observability supports this addition, but the SCOPE.md process requires human sign-off on additions.

**Recommendation**: Accept with acknowledgement. The addition directly addresses vision non-negotiable #7 (analytics-derived data observability) and the product vision's W0-1 security requirement for shed event visibility. Record acceptance as a standing decision before the spec is handed to delivery.

---

## Detailed Findings

### Vision Alignment

**W0-1 specification check** (five pillars):

1. **Dual-pool architecture** (read_pool 6-8, write_pool ≤ 2): Fully addressed. ARCHITECTURE.md §1 (SqlxStore), SPECIFICATION.md FR-01, NF-01, AC-02, AC-09. Write pool cap enforced at startup with `StoreError::InvalidPoolConfig`. ADR-001 records pool sizing rationale.

2. **Analytics write queue** (bounded channel capacity 1000, drain ≤50 or 500ms, shed-under-load, integrity bypass): Fully addressed. ARCHITECTURE.md §3 (AnalyticsWrite enum, drain task), SPECIFICATION.md FR-03, FR-04, FR-07, NF-03, NF-04. Integrity tables (entries, entry_tags, audit_log, agent_registry, vector_map, counters) bypass the queue entirely per FR-06 and C-04. The drain task loop pseudocode is present in both architecture and spec and is consistent.

3. **Async-native storage** (spawn_blocking removal, AsyncEntryStore retirement): Fully addressed. ARCHITECTURE.md §4 (EntryStore trait migration), §9 (AsyncEntryStore retirement), SPECIFICATION.md FR-09, FR-10, FR-11, AC-03, AC-04, AC-05. The spec mandates zero `spawn_blocking` in the store crate (C-05) and zero `AsyncEntryStore` import sites (AC-04).

4. **Backend abstraction** (identical application code for SQLite and PostgreSQL): Addressed at the transport layer. SCOPE.md Goal 7, SPECIFICATION.md NOT IN SCOPE §1. The spec states this feature "positions the code for" PostgreSQL without performing the migration — consistent with vision language ("When centralized deployment demands PostgreSQL: change the connection string..."). The concern about SQLite-specific SQL dialect (SCOPE.md §Assumptions) is acknowledged but not mitigated with a specific test; this is a known limitation.

5. **sqlx compile-time query checking** (SQLX_OFFLINE, sqlx-data.json, CI enforcement): Fully addressed. ARCHITECTURE.md §8, SPECIFICATION.md FR-15, NF-07, AC-12, CI-01, CI-02, CI-03. ADR-004 records the workspace-level single-file decision. The spec mandates a human-readable error on stale cache (CI-02) and a pre-build `cargo sqlx check` step.

**Vision non-negotiables check** ("What's Preserved Throughout"):

- **Hash chain integrity**: Preserved. nxs-011 is a transport layer migration only (C-07, SCOPE.md Non-Goal: "No schema changes"). The `content_hash`/`previous_hash` columns are untouched. No write path is eliminated — only the underlying mechanism changes from rusqlite to sqlx. PASS.

- **Immutable audit log**: Preserved. `audit_log` is classified as an integrity table (FR-06, AC-08). Integrity writes bypass the analytics queue and are never subject to shed. Vision non-negotiable #2 explicitly calls out: "The analytics write queue (W0-1) must not become an audit bypass for analytics-side writes." The spec enforces this via C-04 ("Integrity writes never shed"). PASS.

- **ACID guarantees**: Preserved. SPECIFICATION.md C-07 and the "What's Preserved Throughout" vision note: "ACID storage: SQLite transactional guarantees — W0-1 migrates the driver but doesn't weaken the guarantees." The sqlx transaction API preserves atomicity. sqlx::Transaction rolls back on Drop. ADR-002 documents rollback semantics at the 5 rewritten call sites. PASS.

- **Single binary**: Preserved. No new services introduced. PASS.

- **In-memory hot path**: The nxs-011 architecture does not introduce any analytics-read on the search hot path. The analytics write queue is write-only from the MCP tool layer. Vision non-negotiable #7 ("Analytics-derived data is never read directly on the search hot path") is not violated by this feature. However — see VARIANCE-01 above — the future in-memory hot path architecture (graph, confidence weights) assumes a `analytics.db` separation that is not established by nxs-011. PASS for this feature; gap exists at W1 boundary.

**Security requirements check** (five items from W0-1):

1. **[High] Write pool max_connections capped ≤ 2**: Addressed. AC-09, NF-01, C-01. Startup-time hard cap with structured error. PASS.

2. **[High] Analytics shed policy applies only to analytics writes; integrity writes never dropped**: Addressed. AC-08, C-04, FR-06. Integrity tables bypass the queue. Test scenario R-06 verifies this under queue saturation. PASS.

3. **[Medium] sqlx-data.json regenerated and committed after every schema change; stale cache disables compile-time validation**: Addressed. AC-12, FR-15, CI-01, CI-02, ADR-004. PASS.

4. **[Medium] SQLX_OFFLINE=true enforced in CI**: Addressed. AC-12, NF-07, CI-01, ADR-004. PASS.

5. **[Low] acquire_timeout configured for structured error under write saturation**: Addressed. AC-10, FR-02, NF-02, ADR-001. Timeout values (read: 2s, write: 5s) are filed as named constants in ADR-001. PASS.

---

### Milestone Fit

nxs-011 is explicitly positioned as Wave 0 ("do first, unblock everything"). The architecture states: "This feature is a prerequisite for all Wave 1 features (NLI, graph edges, confidence weight updates) because each of those adds analytics write patterns that would compound the existing spawn_blocking debt if built on top of the current layer." This directly echoes the vision's rationale: "Every W1 and W2 feature built before this migration adds another spawn_blocking site that must later be unwound."

The estimated effort (1.5–2 weeks per vision; SCOPE.md consistent) is appropriate for the Wave 0 slot.

No Wave 1 or Wave 2 capabilities are introduced by this feature. The `AnalyticsWrite` enum's `#[non_exhaustive]` attribute (FR-17, C-08) is forward-compatible preparation — it does not add W1 functionality, only prevents W1 additions from breaking the drain task match exhaustiveness. This is correct milestone discipline.

**Migration system preservation**: SCOPE.md Non-Goal and SPECIFICATION.md NOT IN SCOPE §2 both explicitly exclude replacing `migration.rs` with sqlx's built-in migration runner. This is consistent with vision language: "existing migration.rs logic is preserved and executed through sqlx connections for W0-1. Migration to sqlx's built-in migration runner is a follow-on concern." PASS.

---

### Architecture Review

**Dual-pool construction and PRAGMAs**: The architecture defines `build_connect_options()` applying all 6 PRAGMAs via `SqliteConnectOptions::pragma()` per-connection. This ensures lazily-opened pool connections receive the same configuration as the initial ones (addressing R-11 in the risk strategy). The `read_only(true)` defense-in-depth flag on the read pool is noted as a potential WAL checkpoint concern (open question 3 in ARCHITECTURE.md); the risk strategy captures this as R-12 (Low severity). The architecture offers a safe fallback: remove `read_only` if it causes checkpoint issues, since routing already prevents accidental writes through the read pool.

**Migration connection sequencing (ADR-003)**: The explicit `drop(migration_conn)` before pool construction addresses SR-04 completely. The architecture's sequence in `Store::open()` matches the spec's FR-08 exactly. The `apply_pragmas_connection()` helper ensures consistent SQLite behavior across migration and pool connections.

**Analytics queue and drain task**: The architecture's drain task pseudocode is consistent with the spec's FR-04. The `biased` selector in the `tokio::select!` macro prioritizes the shutdown signal over incoming events — this is correct for clean teardown. The `drain_remaining()` call in the shutdown path ensures no events are silently discarded on close. The shed counter uses `Arc<AtomicU64>` shared between `SqlxStore` and the drain task — this is the correct pattern for `Ordering::Relaxed` cumulative counters.

**One internal inconsistency noted**: ARCHITECTURE.md §6 (SqliteWriteTransaction retirement) lists the 5 call sites as "server.rs ×3, store_correct.rs, store_ops.rs, audit.rs" — that is 6 entries. SCOPE.md and SPECIFICATION.md FR-12 also list 6 call sites. ARCHITECTURE.md §Background Research counts 5 ("5 call sites in the server crate"). This is a minor documentation ambiguity (the `audit.rs` call site appears in the list but not in the original count). Not a blocking concern but the delivery agent should audit all 6 sites.

**EntryStore trait migration (ADR-005)**: The non-object-safe RPITIT decision is correctly motivated — `SqlxStore` is the sole production implementor, `AsyncEntryStore` was the only consumer of `dyn EntryStore`, and zero-cost dispatch is directly valuable on the MCP hot path. The impl-completeness test replacement (AC-20) is the correct substitute.

**SR-02 documentation requirement**: ADR-005 mandates a doc comment on the `EntryStore` trait documenting the non-object-safe design. This is an implementation requirement for the delivery agent but is correctly captured as a constraint.

---

### Specification Review

The specification is complete and well-structured. All 16 SCOPE.md acceptance criteria (AC-01 through AC-16) are present. Four additional criteria (AC-17 through AC-20) are added and are individually justified by scope risk recommendations (SR-04→AC-17, SR-09→AC-19) or ADR consequences (ADR-005→AC-20) or the shed counter extension (SR-08→AC-18).

The domain model section precisely reflects the architecture. Ubiquitous language terms are defined and used consistently across FR, NF, AC, and constraint sections. The "NOT In Scope" section directly mirrors SCOPE.md Non-Goals with no gaps.

**One spec gap**: The spec's FR-12 call site list includes "audit.rs" as a 6th call site, which is consistent with the architecture but inconsistent with the 5-count in SCOPE.md's background research. This is a documentation inconsistency, not a functional gap.

**Shed counter in context_status (VARIANCE-03)**: FR-16, NF-05, and AC-18 add `shed_events_total` to the `context_status` output. This is beyond SCOPE.md. It is flagged as VARIANCE-03 above. The spec's decision to adopt SR-08's recommendation is architecturally sound, but the scope extension requires human acknowledgement.

---

### Risk Strategy Review

The risk strategy is comprehensive. All 15 risks map to: (a) the architecture or spec section that addresses them, or (b) an explicit open question requiring resolution. The scope risk traceability table at the end cross-references all 10 scope risks (SR-01 through SR-10) against architecture risks and resolution mechanisms — this is well-executed.

**R-08 (ExtractionRule bridge panic)**: Correctly classified as High/High and as a delivery blocker. The risk strategy states "This risk must be resolved before delivery begins" and "Coverage Requirement: This risk must be resolved before delivery begins." It is not resolved. See VARIANCE-02.

**Coverage summary**: 15 risks, 44 minimum scenarios across unit, integration, compile-time, CI, and load categories. The breakdown by priority (Critical: 3, High: 8, Medium: 4, Low: 1) is realistic. The coverage requirement for R-01 (pool starvation) appropriately requires both-pool simultaneous saturation testing, not just individual pool tests.

**R-14 appears in two priority tiers**: The risk register lists R-14 under both High and Medium in the coverage summary table (the table shows "R-10, R-11, R-13, R-14" under Medium and "R-04, R-05, R-06, R-07, R-08, R-09, R-14, R-15" under High). This is a documentation error — R-14 is listed in both rows. The risk narrative for R-14 describes it as Med/High, which places it in High. The Medium row count (4) should read 3 items (R-10, R-11, R-13). This is a documentation inconsistency, not a coverage gap.

**Security risks section**: Appropriately scoped. The SQL injection note (parameterised queries via sqlx::query!() macros) is correct. The analytics queue DoS-against-observability characterization is accurate and consistent with the shed policy design. The recommendation to add a CI grep check against `format!("SELECT...{}")` patterns in store SQL is a reasonable addition that the spec's CI-03 does not cover — this could be added as an enhancement but is not a SCOPE.md requirement.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns scope additions milestone discipline — no results (pattern category is empty in active entries).
- Stored: entry via /uni-store-pattern — "Wave-level feature designs use split-file terminology (analytics.db / knowledge.db) before the file split is introduced; architects must confirm whether the split is implied or deferred when reviewing features that explicitly use single-file topology (nxs-011 Decision 4)." Topic: vision. Category: pattern. See finding VARIANCE-01.

---

## Self-Check

- [x] ALIGNMENT-REPORT.md follows the template format
- [x] All checks are evaluated (none skipped without N/A justification)
- [x] Every VARIANCE and FAIL includes: what, why it matters, recommendation
- [x] Scope gaps and scope additions are both checked
- [x] Evidence is quoted from specific document sections, not vague references
- [x] Report path is correct: product/features/nxs-011/ALIGNMENT-REPORT.md
- [x] Knowledge Stewardship report block included
