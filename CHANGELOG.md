# Changelog

All notable changes to Unimatrix are documented here.
Format based on [Keep a Changelog](https://keepachangelog.com/).

## [0.11.1] - 2026-07-08

### Fixes
- **Per-slug state isolation (cloud/HTTPS observe path)**: on a multi-project cloud instance the HTTPS `/observe` write path filled daemon-global state while each per-slug server read constructor-defaults, so per-slug transcript folds, knowledge briefings, and config silently saw wrong/empty state (transcript_delta 204-ACKed but never folded). Completed the per-request per-slug resolution funnel (registry + transcript_hold + signature scanner + pending + services + config), added a release-hard boot assertion plus a compile-time exhaustive field census that closes the whole "constructor-default never overwritten" class, and shipped a solution-independent bidirectional behavioral isolation suite. Closes #930. (vnc-046, #936)
- **hook**: replaced a tautological AC-SR02 test with a real SubagentStart injection e2e; added a `HOOK_TIMEOUT` env seam. (#918, #933)
- **infra-001**: green blocking smoke for container-cloud HTTPS deploy + operator client-works verification. (#915, #916, #932)

## [0.11.0] - 2026-07-07

### Features
- **context_tag**: new MCP tool — in-place tag mutation (`add`/`remove`/`replace`) on the non-hashed tag lane. Changes volatile tags without rewriting the record, re-pointing edges, re-embedding, or zeroing the entry's learning vector. Complete generic audit contract; value-opaque (no hard-coded vocabulary); atomic `replace`. Capability delivery status (the `delivery:` tag) is the first consumer. The `protected_tags` value-hygiene policy is deferred to a future release. (vnc-045, #929)

## [0.10.0] - 2026-07-06

### Features
- **context_graph**: split the overloaded `format` parameter into orthogonal serialization (`format`: `json`/`markdown`) and verbosity (`detail`: `summary`/`full`) axes, with a lean summary node projection; default verbosity is now `summary` (vnc-044, #920)
- **context_graph**: subgraph filter discoverability + live depth-1 read (vnc-043, #903)
- **context_correct**: cross-version hash chain wired in weak mode + on-demand verify CLI (nxs-014, #914)
- **context_cycle_review**: non-destructive scoped transcript retrieval — phase/anchor/match/window (crt-057, #894)
- **context_deprecate**: eager agent-authored edge cleanup at deprecation (crt-058, #911)

### Fixes
- pin rmcp-macros to =1.7.0 so a default `cargo install` compiles (#905)
- retire tick-starved DependencyOnDeprecatedRule and stale_dependency_edges metric (#897)

## [0.9.0] - 2026-07-03

### Fixes
- repoint recoverable inbound edges before orphaned-edge compaction (#879)
- default-resolve context_graph supersessions across read surfaces (#881) (#885)
- raise REDIRECT_CEILING 50->500 to stop redirect truncation (#882) (#884)
- cap concurrent-link RSS to stop workspace test-link OOM (#878) (#880)
- observe SessionClose 204-no-op is observable (#819) (#877)
- client-side dormancy reconnect + session-id restore + errors-only observability (#872) (#875)
- fix D1/D4 parity flip via corpus under-population (#844) (#866)
- provision sqlite3 on parity lane for D2 host-side read (#849) (#863)
- reground hook-client caps to 110000/200000 (#840) (#862)
- remove D6 isolation from nan-022 parity matrix — isolation is not a parity property (#845) (#854)

## [0.8.9] - 2026-06-26

### Fixes
- ref the F6 mid-stream idle-read deadline timers so they fire when the event loop is otherwise idle — the post-dormancy mid-stream stall guard added in 0.8.8 now reliably triggers ETIMEDOUT → self-heal instead of hanging on an unref'd timer; fixes red CI on Node 18/22 (#847) (#848)

## [0.8.8] - 2026-06-26

### Fixes
- arm transport/connect timeout + silent-eviction self-heal in the HTTPS MCP bridge: the first MCP call after long client dormancy no longer hangs forever on a half-open socket — a bounded connect/TLS/idle transport timeout fails fast and transparently re-inits the session, with the self-heal now reachable on silent (hung-socket) eviction as well as signalled (404) eviction (#839) (#842)

## [0.8.7] - 2026-06-24

### Fixes
- anchor cloud cycle attribution on the Claude Code session id so HTTP/cloud observations carry the cycle feature in topic_signal — context_cycle_review now returns metrics over HTTPS as it does over local UDS; enrich safety-net rejects command fragments and makes the unattributed branch loud (#832) (#834)

## [0.8.6] - 2026-06-23

### Fixes
- self-heal stale cloud MCP session: server keep_alive override (4h finite bound) + bridge re-init-on-404 with single-flight guard (#830) (#831)

## [0.8.5] - 2026-06-22

### Fixes
- atomic per-slug vector dump (temp+rename, meta-last) + boot load-fallback for torn/missing index — degrade & self-heal instead of aborting slug boot (#824) (#826)
- dump per-slug vector index on shutdown so the dump→load round-trip persists (#823) (#825)

## [0.8.4] - 2026-06-21

### Fixes
- release: make npm publish steps idempotent on re-run (#821) (#822)
- smoke Gate 7 fires SessionStart (persisting) + bounded poll (#818)
- set UNIMATRIX_PUBLIC_URL on smoke emit container + guard placeholder bundle (#812)
- capture init --bundle stderr in release smoke Gate 6 (#812)
- anndists: restore arch-gated simdeez_f import dropped by #798

## [0.8.3] - 2026-06-20

### Fixes
- /shared read-write default + first-boot embed-readiness release gate (#767) (#810)
- enable HTTP multi-project posture in cloud Docker image (#783) (#786)

## [0.8.2] - 2026-06-18

### Fixes
- win32-guard credstore POSIX-mode assertions for cross-platform CI (#775)
- gate-3c: size-gate header meta-assertion lockstep 160,000->180,000 (#775)
- http: wire rmcp allowed_hosts from UNIMATRIX_PUBLIC_URL (#774) (#778)

## [0.8.1] - 2026-06-17

### Features
- Mandatory project identity at the deployment entrypoint (vnc-038, #770): cloud/container deployments require `unimatrix project register <slug>` before serving — the no-slug/default route is removed (hard cutover). `client-bundle <slug>` emits a per-project `v:2` bundle carrying server-composed MCP + observe URLs; the client posts them verbatim (dumb-client invariant). Adding the Nth project is the same single command; local STDIO/UDS installs are unaffected.

### Fixes
- Personal-cloud bundle attach no longer 404s (#766): `init --bundle` init-time Ping and runtime hook telemetry now reach the per-slug observe route (`/v1/{slug}/observe`) on the same per-request funnel as MCP, with per-slug store isolation proven at N=2.
- First-boot bearer token is never written to stdout/logs — delivered only via the bundle; `router.rs` split under the 500-line guideline and a stale `dead_code` allow removed (#735).
- Layer-2 test harness reconciled to the per-slug deployment model (#771).

## [0.8.0] - 2026-06-16

### Features
- crt-054: transcript-fold producer — compaction_events table + in-memory activity fold (#759)
- uni-zero: add security posture review and codebase health assessment

### Fixes
- re-point per-session telemetry onto PostToolUse + guarded recompute (#750) (#758)
- de-flake test-integrity cluster + harden cargo-test process (#742) (#748)
- align skip_if_no_model() model dir to cache_subdir() + loud-fail guard (#723) (#740)
- INSERT OR IGNORE in insert_session to prevent compaction-resume clobber (#300) (#715)
- deadline-poll test sync for fire-and-forget write races (#691, #452, #305) (#714)
- correct three stale test assertions (#695, #576, #575) (#713)
- harden cargo test convention to stop orphaned process trees (#122) (#711)
- deps: bump openssl 0.10.75→0.10.80 and rustls-webpki 0.103.9→0.103.13 (#665) (#667)
- router: enforce body size limit on stream, not just Content-Length header (#663) (#668)
- token: eliminate TOCTOU races in token file creation (#662) (#664)
- protocol: research spike output alignment + GitHub issue lifecycle
- agent: add Report Block and self-check to uni-architect stewardship
- test: align TestLargeContent thresholds with 8KB server limit (#652) (#659)
- config: nli_model_sha256 merge uses global-wins semantics (#655) (#656)
- embed: add SHA-256 hash verification for embedding model (#651) (#654)
- store: reject databases with schema version newer than binary supports (#650) (#653)
- import: replace manual MAX(event_id)+1 with log_audit_event in record_provenance (#633) (#642)
- server: prevent integration tests from leaking dirs into ~/.unimatrix (#640) (#641)
- infra: ensure binary crate target included in RUST_LOG filter (#638)
- infra: daemon child opens log file explicitly, eliminating fd inheritance (#638) (#639)
- infra: config load observability and category authority enforcement (#635) (#636)
- infra: config categories not authoritative — compiled defaults diverged from config (#632) (#634)
- switch to Docker CE for containerd v2 compatibility
- pin docker-in-docker to v27 for containerd v2 compatibility

## [0.7.2] - 2026-05-22

### Fixes
- config: write commented default config.toml to project path on unimatrix init; add --force flag (#626)

## [0.7.1] - 2026-05-22

### Features
- uni-zero: upgrade orientation to load goal and feature graph by category

### Fixes
- graph: subgraph and neighbors depth>1 fall back to DB on cold-start — missing use_fallback check (#623)

## [0.7.0] - 2026-05-21

### Features
- context_graph — inverse, filter, path modes (vnc-020, #614)
- context_graph subgraph mode — bounded multi-hop BFS (vnc-019, #597)
- context_graph — chain, current, neighbors read modes, 14th MCP tool (vnc-018, #609)
- auto-redirect incoming graph edges on context_correct (vnc-017, #607)
- typed edge write path — context_edge tool + edges param + 10 new RelationType variants (vnc-015, #600)
- audit_log 4-column migration + MCP client attribution via clientInfo.name (vnc-014, #577)
- canonical event normalization for multi-LLM hook providers (vnc-013, #567)

### Fixes
- graph_read_subgraph: add MAX_EDGES_UPPER constant and debug_assert at fetch_edge_metadata call site (#611, #622)
- graph: reject resolve_supersessions on inverse/filter, add coverage tests (#616, #613, #620)
- filter: guard NaN/±Infinity on min/max_confidence before SQL (#615, #619)
- path_via_db() DB-backed BFS fallback for cold-start graph cache (#612, #618)
- DependencyOnDeprecated wiring defect + end-to-end integration test (vnc-016, #605)
- context_quarantine capability corrected to Admin in spec and tests (#580, #592)
- audit: rename counter next_audit_event_id → next_audit_id (#587, #590)
- audit: atomic counter allocation eliminates UNIQUE constraint race (#584, #586)
- audit: session_id linkage restored in audit_log (#582, #583)
- audit: restore audit_log writes by fixing spawn_blocking + block_in_place panic (#579, #581)
- validation: enforce byte-based content size cap on context_store and context_correct (#561, #573)
- curation_health: route snapshot queries through read_pool via store layer (#535)
- listener: remove contradictory debug_assert, ungated col-019 normalization tests (#565)

## [0.6.3] - 2026-04-09

### Fixes
- correct npm package metadata: repository URL (anthropics → dug-21), homepage field, and README for npmjs.com pages (#549)
- add agent reports for bugfix-554 gate compliance
- use new_multi_thread in sync runtime fallback (#554)

## [0.6.2] - 2026-04-09

### Fixes
- release: normalize test embeddings and bake ELF rpath=$ORIGIN into release binary for self-contained .so distribution (#552)

## [0.6.1] - 2026-04-09

### Fixes
- release: restore linux-x64 build job (ubuntu-22.04, glibc 2.35) and wire into npm publish (#550)

## [0.6.0] - 2026-04-08

### Features
- daemon mode — persistent background server via UDS MCP transport (#295)

### Fixes
- tools: normalize mcp__unimatrix__ prefix in categorize_tool_for_phase and compute_phase_stats (#536)
- contradiction_density_score: replace quarantine proxy with scan pair count (#545)
- co_access_promotion_tick: use allowlist (status = Active) to stop deprecated/proposed endpoint oscillation (#528)
- uds: re-register evicted sessions on cycle_start to restore topic_signal attribution (#519)
- entry-tags-index: add compound (tag, entry_id) index to fix S1 co-occurrence O(K) scan (#509)
- crt-046: InferenceConfig validate missing range checks + briefing cluster ID cap (#515)
- eval: harness scenario ID collision + snapshot pairing validation (#501 #502)
- co_access_promotion_tick: exclude quarantined endpoints from promotion SELECT (#476)
- compaction: use allowlist WHERE status = Active so deprecated-endpoint edges are deleted (#471)
- nli_detection_tick: give Informs independent budget MAX_INFORMS_PER_TICK=25 (#473)
- get_cycle_start_goal: first-written-goal-wins; NULL row no longer shadows original goal (#468)
- background: exclude quarantined entries from GRAPH_EDGES compaction (#458)
- security: enforce heal_pass_batch_size range and typed SQL status bindings (#444)
- maintenance: enforce index-active-set invariant (#444)
- categories: retire duties and reference from INITIAL_CATEGORIES (#436 #440)
- observe: remove RecurringFrictionRule from extraction pipeline (#437 #438)
- config: lower supports_edge_threshold 0.7 → 0.6 (#434)
- freshness: half-life 168h → 8760h (1 year), recalibrate tests (#426)
- nli: prevent tick stall — shuffle candidates, exclude no-embedding entries (#421)
- coaccess: increase CO_ACCESS_STALENESS_SECONDS from 30 to 365 days (#408)
- col-025: persist context_cycle goal through hook payload (#389)
- skills: replace pseudo-code MCP calls with proper JSON format in all uni-* skills
- retrospective: render goal as dedicated section, never silently omit (#384)
- background: remove dead-knowledge auto-deprecation pass (#369 #371)
- hook: fix SubagentStart query derivation and lower similarity floor
- briefing: make BriefingParams.role optional (#364)
- contradiction: pre-fetch entries in Tokio context before quality-gate rayon dispatch (#360)
- background,observe: replace unbounded observation scan and full-topic query (#351)
- confidence: propagate Arc<ConfidenceParams> to all serving-path call sites (#311 #347)
- validation: reject control chars in outcome and non-ASCII in phase fields (#343)
- 6 hardening fixes — merge validation, saturating counters, session sanitization, markdown escaping, u64 cast (#337 #345 #346 #378 #379 #380)
- open_readonly must not set journal_mode=WAL pragma
- context_cycle_review: pre-fetch entry categories async to avoid block_on panic (#313)
- server: replace blocking log_event() with fire-and-forget async at 5 call sites (#308)
- store: convert synchronous audit writes to fire-and-forget (#302)
- daemon: move --project-dir before subcommand in child args (#295)

## [0.5.9] - 2026-03-16

### Fixes
- server: decouple compute_report() from maintenance tick — skip O(N) ONNX phases (#280)
- server: cache contradiction scan result in background tick (#278)
- server: use is_multiple_of for contradiction scan tick gate (#278)
- server: batch extraction tick observations to 1000 rows (#279)
- server: wrap all hot-path MCP handler spawn_blocking calls with timeout (#277)
- server: wrap background tick in panic supervisor loop (#276)
- server: replace naked JoinHandle unwrap in compute_report() (#275)
- vector: iterate all HNSW layers in get_embedding (#286)

## [0.5.8] - 2026-03-13

### Fixes
- init: set LD_LIBRARY_PATH in hook commands and fix --project-dir argument order

## [0.5.7] - 2026-03-13

### Fixes
- init: set LD_LIBRARY_PATH for binary invocations during init

## [0.5.6] - 2026-03-13

### Fixes
- CI: build arm64 on ubuntu-22.04 to target glibc 2.35

## [0.5.5] - 2026-03-13

### Fixes
- CI: switch npm packages to public access

## [0.5.4] - 2026-03-13

### Fixes
- CI: disable x64 build, arm64-only release (#247)

## [0.5.3] - 2026-03-13

### Fixes
- CI: add test retry for transient CI failures (#247)

## [0.5.2] - 2026-03-13

### Fixes
- CI: download embedding model before tests in release pipeline (#245)

## [0.5.1] - 2026-03-13

### Fixes
- CI: install ORT on CI runner and add linux-arm64 build (#243)

## [0.5.0] - 2026-03-13

### Features
- Quarantine state restoration — schema v7→v8, multi-status quarantine, restore to pre-quarantine status (#142)
- col-011 knowledge architecture — specialized coordinators, skills, and feedback loop
- col-010b — evidence synthesis & lesson-learned persistence (re-delivery) (#78)
- col-010 P0 — session lifecycle persistence + injection log (#77)
- col-002 retrospective pipeline (#58)
- crt-006 adaptive embedding pipeline with integration tests (#49)
- crt-005 coherence gate — f64 scoring, lambda metric, maintenance actions
- crt-001 usage tracking implementation (#25)
- nxs-003 embedding pipeline — unimatrix-embed crate (#5)
- nxs-002 vector index — unimatrix-vector crate (#2)

### Fixes
- Server: resolve ghost process, tick contention, and handler timeouts (#236)
- Server: add agent_id to CycleParams so context_cycle resolves caller identity (#230)
- Registry: permissive auto-enroll grants Write to unknown agents (#228)
- Session: resolve feature_cycle attribution gaps (#198)
- Context_status deadlock + async blocking store calls (#176)
- Content-based attribution fallback for retrospective (#162)
- Batch spawn_blocking DB writes to prevent blocking pool saturation (#158)
- Resolve deadlock in scan_sessions_by_feature_with_status (#152)
- Server recovery — PidGuard race, db retry, transport logging (#146)
- Embed model retry on failure + abort tick handle on shutdown (#120)
- Init order — run migration before create_tables for v5 databases (#104)
- Align SQLite Store API with redb signatures (#95)
- Drop ServiceLayer in shutdown to release Arc<Store> refs (#92)
- Stdin size limit and data directory permissions hardening
- PID file mechanism and retry loop for stale DB lock (#23)
