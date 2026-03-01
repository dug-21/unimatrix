# col-002 Acceptance Criteria Map

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | Hook scripts capture PreToolUse, PostToolUse, SubagentStart, SubagentStop | test | Pipe synthetic JSON for each event type to each script, verify JSONL output contains correct hook field | PENDING |
| AC-02 | Records route to `~/.unimatrix/observation/{session_id}.jsonl` based on session_id | test | Pipe JSON with known session_id, verify file created at expected path | PENDING |
| AC-03 | Hook scripts exit 0 always | test | Pipe invalid JSON to each script, verify exit code is 0 | PENDING |
| AC-04 | Record schema includes ts, hook, session_id, tool, input, response_size, response_snippet | test | Parse hook output JSONL line as JSON, assert all 7 fields present | PENDING |
| AC-05 | Response snippet truncated to bound per-record size | test | Pipe PostToolUse with 10KB response, verify snippet <= 500 chars | PENDING |
| AC-06 | `unimatrix-observe` parses JSONL session files into typed record structs | test | `cargo test -p unimatrix-observe parser::tests` -- sample JSONL roundtrip | PENDING |
| AC-07 | Sequential feature attribution walks records in timestamp order | test | `cargo test -p unimatrix-observe attribution::tests` -- multi-feature sequence | PENDING |
| AC-08 | Multi-session features: all attributed sessions included | test | 3 synthetic sessions, 2 with target feature, verify all 2 included | PENDING |
| AC-09 | Multi-feature sessions: records partitioned by switch points | test | Session with feat-A then feat-B paths, verify partition | PENDING |
| AC-10 | Hotspot framework supports registering rules by category | test | Register mock rule, verify detect_hotspots calls it | PENDING |
| AC-11 | Permission retries rule: Pre-Post differential, threshold >2 | test | Synthetic records: 5 Pre + 2 Post for tool X, verify finding | PENDING |
| AC-12 | Session timeout rule: gap >2 hours, any occurrence | test | Records with 3-hour gap, verify detection | PENDING |
| AC-13 | Sleep workarounds rule: regex match in Bash input | test | Bash record with `sleep 5`, verify detection | PENDING |
| AC-14 | Each hotspot includes category, severity, claim, measured, threshold, evidence | test | Verify HotspotFinding struct fields on detection output | PENDING |
| AC-15 | MetricVector contains universal metrics and phase metrics | test | Compute from synthetic records, assert both sections populated | PENDING |
| AC-16 | Phase names extracted from task subject prefix (split on first `:`) | test | Input `"3a: Pseudocode"` -> phase key `"3a"` | PENDING |
| AC-17 | `unimatrix-observe` has no dependency on store or server | grep | `grep -c 'unimatrix-store\|unimatrix-server\|unimatrix-core' crates/unimatrix-observe/Cargo.toml` returns 0 | PENDING |
| AC-18 | Hotspot framework extensible without engine modification | test | Implement DetectionRule trait on new struct, pass to detect_hotspots, verify it runs | PENDING |
| AC-19 | `context_retrospective` accepts `feature_cycle` parameter | test | Integration test: call tool with valid feature_cycle, verify acceptance | PENDING |
| AC-20 | Tool scans, attributes, analyzes, returns report | test | Write synthetic JSONL to temp dir, call tool, verify report structure | PENDING |
| AC-21 | Report includes metrics and hotspot findings with evidence | test | Verify report fields after e2e call | PENDING |
| AC-22 | Report is self-contained | test | Verify report contains metrics + hotspots + session_count + total_records | PENDING |
| AC-23 | Tool stores MetricVector in OBSERVATION_METRICS | test | Call tool, read OBSERVATION_METRICS, verify entry exists | PENDING |
| AC-24 | Tool cleans up files older than 60 days | test | Create files with old timestamps, call tool, verify deleted | PENDING |
| AC-25 | Error when no data and no stored MetricVector | test | Empty observation dir, no stored MV, verify error response | PENDING |
| AC-26 | Cached result when no new data but stored MetricVector exists | test | Store MV, call tool with empty dir, verify is_cached=true | PENDING |
| AC-27 | MetricVector includes computed_at timestamp | test | Verify field populated after compute_metric_vector call | PENDING |
| AC-28 | OBSERVATION_METRICS table exists with correct schema | test | `Store::open`, open table in read txn, verify accessible | PENDING |
| AC-29 | Table created during Store::open (14th table) | test | Extend test_open_creates_all_tables to include OBSERVATION_METRICS | PENDING |
| AC-30 | Store provides store_metrics, get_metrics, list_all_metrics | test | CRUD: store -> get -> verify, list -> verify count | PENDING |
| AC-31 | Schema version remains 3 | test | Verify no migration triggered on open | PENDING |
| AC-32 | MetricVector serializable/deserializable via bincode | test | Roundtrip: serialize -> deserialize -> assert_eq | PENDING |
| AC-33 | Files >60 days auto-removed during retrospective or maintain | test | Create aged files, trigger cleanup, verify removal | PENDING |
| AC-34 | context_status reports observation file count, size, oldest age | test | Create observation files, call status, verify fields | PENDING |
| AC-35 | context_status warns when files approach 60-day threshold | test | Create files at 50 days age, verify warning in output | PENDING |
| AC-36 | `#![forbid(unsafe_code)]` on unimatrix-observe | grep | `grep 'forbid(unsafe_code)' crates/unimatrix-observe/src/lib.rs` | PENDING |
| AC-37 | No new crate dependencies beyond workspace | grep | Check Cargo.toml for unexpected dependencies | PENDING |
| AC-38 | All existing tests pass | test | `cargo test --workspace` -- no failures | PENDING |
| AC-39 | Unit tests cover parsing, attribution, detection rules, serialization, file age | test | Verify test modules exist and pass in unimatrix-observe | PENDING |
| AC-40 | Integration tests cover e2e retrospective, status fields, OBSERVATION_METRICS CRUD | test | Integration test suite in unimatrix-server/tests/ | PENDING |
| AC-41 | Tests build on existing workspace fixtures | test | Reuse test_helpers patterns from unimatrix-store | PENDING |
