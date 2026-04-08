# Changelog

All notable changes to Unimatrix are documented here.
Format based on [Keep a Changelog](https://keepachangelog.com/).

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
