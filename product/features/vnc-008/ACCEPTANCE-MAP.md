# vnc-008 Acceptance Criteria Map

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | `mcp/` directory contains tools.rs, context.rs, identity.rs, response/ | file-check | `ls crates/unimatrix-server/src/mcp/` must list tools.rs, context.rs, identity.rs, response/ | PENDING |
| AC-02 | `uds/` directory contains listener.rs, hook.rs | file-check | `ls crates/unimatrix-server/src/uds/` must list listener.rs, hook.rs | PENDING |
| AC-03 | `infra/` directory contains all 13 infrastructure modules | file-check | `ls crates/unimatrix-server/src/infra/` must list audit.rs, registry.rs, session.rs, scanning.rs, validation.rs, categories.rs, contradiction.rs, coherence.rs, pidfile.rs, shutdown.rs, embed_handle.rs, usage_dedup.rs, outcome_tags.rs, mod.rs | PENDING |
| AC-04 | `services/status.rs` exists with StatusService | file-check + grep | File exists and contains `pub(crate) struct StatusService` | PENDING |
| AC-05 | Root contains only main.rs, lib.rs, error.rs, server.rs | shell | `ls crates/unimatrix-server/src/*.rs` returns exactly main.rs, lib.rs, error.rs, server.rs | PENDING |
| AC-06 | No flat-root transport or infrastructure modules remain | shell | `ls crates/unimatrix-server/src/*.rs` has no audit.rs, registry.rs, tools.rs, etc. | PENDING |
| AC-07 | `mcp/response/mod.rs` has shared helpers and re-exports | grep | Contains `parse_format`, `format_timestamp`, `ResponseFormat` | PENDING |
| AC-08 | `mcp/response/entries.rs` has entry formatting functions | grep | Contains `format_single_entry`, `format_search_results`, `format_lookup_results` | PENDING |
| AC-09 | `mcp/response/mutations.rs` has generic `format_status_change` | test | Unit test: `format_status_change("Deprecated"...)` == `format_deprecate_success(...)` for all formats | PENDING |
| AC-10 | `mcp/response/status.rs` has `format_status_report` | grep | Contains `pub fn format_status_report` or `pub(crate) fn format_status_report` | PENDING |
| AC-11 | `mcp/response/briefing.rs` has format_briefing, format_retrospective_report | grep | Contains both function names | PENDING |
| AC-12 | No standalone response.rs at crate root | file-check | `test ! -f crates/unimatrix-server/src/response.rs` | PENDING |
| AC-13 | ToolContext struct exists in mcp/context.rs | grep | Contains `pub(crate) struct ToolContext` | PENDING |
| AC-14 | All MCP tool handlers use build_context() + require_cap() | grep | `grep -c 'build_context' crates/unimatrix-server/src/mcp/tools.rs` >= 11 (one per handler minus context_status which also uses it) | PENDING |
| AC-15 | .map_err(rmcp::ErrorData::from) count reduced by 50%+ | shell | `grep -c 'map_err(rmcp::ErrorData::from)' src/mcp/tools.rs` < 40 (baseline ~79) | PENDING |
| AC-16 | StatusService in services/status.rs with compute_report() + run_maintenance() | grep | Contains `async fn compute_report` and `async fn run_maintenance` | PENDING |
| AC-17 | context_status handler delegates to StatusService | grep | `mcp/tools.rs` context_status handler < 30 lines; contains `self.services.status` | PENDING |
| AC-18 | StatusService produces identical StatusReport | test | Snapshot test: known data produces identical StatusReport fields pre/post extraction | PENDING |
| AC-19 | Capability::SessionWrite variant exists | grep | `infra/registry.rs` contains `SessionWrite` in Capability enum | PENDING |
| AC-20 | UDS connections assigned {Read, Search, SessionWrite} | test | Unit test: UDS_CAPABILITIES contains exactly Read, Search, SessionWrite | PENDING |
| AC-21 | SessionWrite permits operational writes | test | Integration test: UDS SessionRegister, RecordEvent succeed with SessionWrite | PENDING |
| AC-22 | SessionWrite does NOT permit knowledge writes, mutations, admin ops | test | Unit test: capability check rejects Write/Admin operations with only SessionWrite | PENDING |
| AC-23 | F-26 closed: UDS has formal capability boundary | test | Test: attempt Admin op via UDS, expect capability error | PENDING |
| AC-24 | MCP responses byte-identical for same inputs | test | Existing MCP test suite passes without modification | PENDING |
| AC-25 | UDS responses byte-identical for same inputs | test | Existing UDS test suite passes without modification | PENDING |
| AC-26 | No net reduction in test count | shell | `cargo test 2>&1 \| grep 'test result'` count >= pre-vnc-008 baseline | PENDING |
| AC-27 | All changes in crates/unimatrix-server/ only | shell | `git diff --name-only` shows only paths under `crates/unimatrix-server/` | PENDING |
| AC-28 | mcp/ has no storage-access imports of foundation crates | grep | `grep -r 'use unimatrix_store::' src/mcp/` returns only type imports (EntryRecord, Status), no table/Store imports (exception: tracked context_status imports until StatusService absorbs them) | PENDING |
| AC-29 | services/ is the only group with foundation crate storage access | grep | `grep -r 'use unimatrix_store::Store' src/` returns only hits in services/ and server.rs | PENDING |
| AC-30 | No circular dependencies between module groups | test | `cargo check` succeeds; no `use crate::infra::` in services/, no `use crate::services::` in infra/ | PENDING |
