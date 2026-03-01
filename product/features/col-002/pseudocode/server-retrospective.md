# Pseudocode: server-retrospective

## Purpose

context_retrospective MCP tool handler. Orchestrates the analysis pipeline: scan -> parse -> attribute -> detect -> compute -> store -> respond.

## File: `crates/unimatrix-server/src/tools.rs` (additions)

### RetrospectiveParams

```
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RetrospectiveParams {
    /// Feature cycle to analyze (e.g., "col-002").
    pub feature_cycle: String,
    /// Agent making the request.
    pub agent_id: Option<String>,
}
```

### context_retrospective handler

```
#[tool(name = "context_retrospective")]
async fn context_retrospective(
    &self,
    #[tool(params)] params: RetrospectiveParams,
) -> Result<CallToolResult, ErrorData> {
    // 1. Identity resolution
    let identity = self.resolve_agent(&params.agent_id)
        .map_err(ErrorData::from)?;

    // 2. Validation
    validate_retrospective_params(&params)
        .map_err(ErrorData::from)?;

    // 3. Determine observation directory
    let obs_dir = unimatrix_observe::files::observation_dir();

    // 4. Discover and parse session files (spawn_blocking for sync I/O)
    let (sessions, obs_stats) = tokio::task::spawn_blocking({
        let obs_dir = obs_dir.clone();
        move || -> std::result::Result<_, ServerError> {
            let session_files = unimatrix_observe::discover_sessions(&obs_dir)
                .map_err(|e| ServerError::ObservationError(e.to_string()))?;

            let mut parsed: Vec<ParsedSession> = Vec::new();
            for sf in &session_files {
                let records = unimatrix_observe::parse_session_file(&sf.path)
                    .unwrap_or_default();  // Skip unparseable files
                if !records.is_empty() {
                    parsed.push(ParsedSession {
                        session_id: sf.session_id.clone(),
                        records,
                    });
                }
            }

            let stats = unimatrix_observe::scan_observation_stats(&obs_dir)
                .map_err(|e| ServerError::ObservationError(e.to_string()))?;

            Ok((parsed, stats))
        }
    }).await.unwrap().map_err(ErrorData::from)?;

    // 5. Attribute sessions to target feature
    let attributed = unimatrix_observe::attribute_sessions(&sessions, &params.feature_cycle);

    // 6. Check for data availability
    let store = self.store.clone();
    let feature_cycle = params.feature_cycle.clone();

    if attributed.is_empty() {
        // No new data -- check for cached MetricVector
        let cached = tokio::task::spawn_blocking({
            let store = store.clone();
            let fc = feature_cycle.clone();
            move || store.get_metrics(&fc)
        }).await.unwrap()
        .map_err(|e| ServerError::Core(CoreError::Store(e)))
        .map_err(ErrorData::from)?;

        match cached {
            Some(bytes) => {
                // Return cached result (FR-09.6)
                let mv = unimatrix_observe::deserialize_metric_vector(&bytes)
                    .map_err(|e| ServerError::ObservationError(e.to_string()))
                    .map_err(ErrorData::from)?;

                let report = RetrospectiveReport {
                    feature_cycle: feature_cycle.clone(),
                    session_count: 0,
                    total_records: 0,
                    metrics: mv,
                    hotspots: vec![],
                    is_cached: true,
                };

                return Ok(format_retrospective_report(&report));
            },
            None => {
                // No data, no cache (FR-09.7)
                return Err(ErrorData::new(
                    ERROR_NO_OBSERVATION_DATA,
                    format!("No observation data found for feature '{}'. Ensure hook scripts are installed and sessions have been run.", feature_cycle),
                    None,
                ));
            }
        }
    }

    // 7. Run analysis pipeline
    let rules = unimatrix_observe::default_rules();
    let hotspots = unimatrix_observe::detect_hotspots(&attributed, &rules);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let metrics = unimatrix_observe::compute_metric_vector(&attributed, &hotspots, now);

    // 8. Store MetricVector
    let mv_bytes = unimatrix_observe::serialize_metric_vector(&metrics)
        .map_err(|e| ServerError::ObservationError(e.to_string()))
        .map_err(ErrorData::from)?;

    tokio::task::spawn_blocking({
        let store = store.clone();
        let fc = feature_cycle.clone();
        move || store.store_metrics(&fc, &mv_bytes)
    }).await.unwrap()
    .map_err(|e| ServerError::Core(CoreError::Store(e)))
    .map_err(ErrorData::from)?;

    // 9. Cleanup expired files (FR-09.8)
    let cleanup_dir = obs_dir.clone();
    tokio::task::spawn_blocking(move || {
        let sixty_days = 60 * 24 * 60 * 60; // 60 days in seconds
        if let Ok(expired) = unimatrix_observe::identify_expired(&cleanup_dir, sixty_days) {
            for path in expired {
                let _ = std::fs::remove_file(path);  // Best-effort
            }
        }
    }).await.unwrap();

    // 10. Build and return report
    let report = unimatrix_observe::build_report(&feature_cycle, &attributed, metrics, hotspots);

    // 11. Audit
    self.audit_tool_call(&identity, "context_retrospective", &Outcome::Success);

    Ok(format_retrospective_report(&report))
}
```

## File: `crates/unimatrix-server/src/validation.rs` (additions)

### validate_retrospective_params

```
pub fn validate_retrospective_params(params: &RetrospectiveParams) -> Result<(), ServerError> {
    if params.feature_cycle.trim().is_empty() {
        return Err(ServerError::InvalidInput {
            field: "feature_cycle".to_string(),
            reason: "must not be empty".to_string(),
        });
    }
    Ok(())
}
```

## File: `crates/unimatrix-server/src/error.rs` (additions)

### New error variant

```
/// Observation analysis failed.
ObservationError(String),
```

### New error code

```
/// MCP error code: no observation data available.
pub const ERROR_NO_OBSERVATION_DATA: ErrorCode = ErrorCode(-32010);
```

### ErrorData mapping

```
ServerError::ObservationError(msg) => ErrorData::new(
    ERROR_NO_OBSERVATION_DATA,
    format!("Observation analysis error: {msg}"),
    None,
),
```

## File: `crates/unimatrix-server/src/response.rs` (additions)

### format_retrospective_report

```
pub fn format_retrospective_report(report: &RetrospectiveReport) -> CallToolResult {
    let json = serde_json::to_string_pretty(report).unwrap_or_default();
    CallToolResult::success(vec![Content::text(json)])
}
```

## File: `crates/unimatrix-server/src/server.rs` (modifications)

Register `context_retrospective` in the tool router (rmcp #[tool] macro handles this automatically when the method is on the impl block with #[tool(tool_box)]).

## File: `crates/unimatrix-server/Cargo.toml` (modifications)

Add dependency:
```toml
unimatrix-observe = { path = "../unimatrix-observe" }
```

## Error Handling

- Missing observation dir -> no-data error or cached result
- Unparseable session files -> skipped (not fatal)
- Store write failure -> ServerError returned to caller
- Empty feature_cycle -> InvalidInput validation error
- Cleanup failures -> ignored (best-effort)

## Key Test Scenarios

- Full e2e: synthetic JSONL -> call tool -> report returned (AC-20)
- Tool stores MetricVector in OBSERVATION_METRICS (AC-23)
- Cached result when no new data but stored MV exists (AC-26)
- Error when no data and no stored MV (AC-25)
- Feature_cycle validation: empty string rejected (AC-19)
- Report includes metrics and hotspot findings (AC-21)
- Files >60 days cleaned up during execution (AC-24)
