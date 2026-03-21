//! Per-rule isolation tests and backward compatibility snapshot tests (col-023 GAP-01, GAP-02).
//!
//! GAP-01: For each of the 21 built-in Rust DetectionRule implementations, one
//!         test constructs records that WOULD trigger the rule if source_domain were
//!         "claude-code", but uses source_domain = "sre" instead.  All 21 must
//!         produce zero findings.
//!
//! GAP-02: `test_retrospective_report_backward_compat_claude_code_fixture` runs
//!         the full detection + metrics pipeline against a fixed hardcoded
//!         representative claude-code session and asserts the pipeline completes
//!         without panicking and produces the expected finding categories.

use unimatrix_observe::types::ObservationRecord;
use unimatrix_observe::{HotspotCategory, compute_metric_vector, default_rules, detect_hotspots};

// ── Shared helpers ────────────────────────────────────────────────────────────

fn make_sre(ts: u64, event_type: &str) -> ObservationRecord {
    ObservationRecord {
        ts,
        event_type: event_type.to_string(),
        source_domain: "sre".to_string(),
        session_id: "sre-sess-1".to_string(),
        tool: None,
        input: None,
        response_size: None,
        response_snippet: None,
    }
}

fn make_sre_tool(ts: u64, event_type: &str, tool: &str) -> ObservationRecord {
    ObservationRecord {
        ts,
        event_type: event_type.to_string(),
        source_domain: "sre".to_string(),
        session_id: "sre-sess-1".to_string(),
        tool: Some(tool.to_string()),
        input: None,
        response_size: None,
        response_snippet: None,
    }
}

fn make_sre_tool_input(
    ts: u64,
    event_type: &str,
    tool: &str,
    input: serde_json::Value,
) -> ObservationRecord {
    ObservationRecord {
        ts,
        event_type: event_type.to_string(),
        source_domain: "sre".to_string(),
        session_id: "sre-sess-1".to_string(),
        tool: Some(tool.to_string()),
        input: Some(input),
        response_size: None,
        response_snippet: None,
    }
}

fn make_sre_tool_response(ts: u64, event_type: &str, tool: &str, size: u64) -> ObservationRecord {
    ObservationRecord {
        ts,
        event_type: event_type.to_string(),
        source_domain: "sre".to_string(),
        session_id: "sre-sess-1".to_string(),
        tool: Some(tool.to_string()),
        input: None,
        response_size: Some(size),
        response_snippet: None,
    }
}

// Helper to run a single named rule from default_rules() against records.
fn run_rule(rule_name: &str, records: &[ObservationRecord]) -> Vec<unimatrix_observe::HotspotFinding> {
    let rules = default_rules(None);
    let rule = rules
        .into_iter()
        .find(|r| r.name() == rule_name)
        .unwrap_or_else(|| panic!("rule '{}' not found in default_rules()", rule_name));
    rule.detect(records)
}

// ── GAP-01: Per-rule isolation tests (21 rules, naming: test_{rule_name}_ignores_non_claude_code_domain) ──

/// Rule 1 (friction): permission_retries — source_domain guard isolates sre records.
#[test]
fn test_permission_retries_ignores_non_claude_code_domain() {
    // 10 PreToolUse + 2 PostToolUse for "Read" with sre domain → retries look like 8,
    // but domain guard must block all.
    let mut records: Vec<ObservationRecord> = (0..10)
        .map(|i| make_sre_tool(i * 1000, "PreToolUse", "Read"))
        .collect();
    records.extend((10..12).map(|i| make_sre_tool(i * 1000, "PostToolUse", "Read")));
    let findings = run_rule("permission_retries", &records);
    assert!(
        findings.is_empty(),
        "permission_retries must produce no findings for sre domain; got: {:?}",
        findings
    );
}

/// Rule 2 (friction): sleep_workarounds — source_domain guard isolates sre records.
#[test]
fn test_sleep_workarounds_ignores_non_claude_code_domain() {
    let records: Vec<ObservationRecord> = (0..5)
        .map(|i| {
            make_sre_tool_input(
                i * 1000,
                "PreToolUse",
                "Bash",
                serde_json::json!({"command": "sleep 5"}),
            )
        })
        .collect();
    let findings = run_rule("sleep_workarounds", &records);
    assert!(
        findings.is_empty(),
        "sleep_workarounds must produce no findings for sre domain; got: {:?}",
        findings
    );
}

/// Rule 3 (friction): search_via_bash — source_domain guard isolates sre records.
#[test]
fn test_search_via_bash_ignores_non_claude_code_domain() {
    // 2 find commands out of 2 total = 100%, far above the 5% threshold.
    let records: Vec<ObservationRecord> = (0..2)
        .map(|i| {
            make_sre_tool_input(
                i * 1000,
                "PreToolUse",
                "Bash",
                serde_json::json!({"command": "find . -name '*.log'"}),
            )
        })
        .collect();
    let findings = run_rule("search_via_bash", &records);
    assert!(
        findings.is_empty(),
        "search_via_bash must produce no findings for sre domain; got: {:?}",
        findings
    );
}

/// Rule 4 (friction): output_parsing_struggle — source_domain guard isolates sre records.
#[test]
fn test_output_parsing_struggle_ignores_non_claude_code_domain() {
    // 3 different pipe variants in under 3 minutes for the same base command.
    let records = vec![
        make_sre_tool_input(
            1000,
            "PreToolUse",
            "Bash",
            serde_json::json!({"command": "cargo test | grep FAIL"}),
        ),
        make_sre_tool_input(
            2000,
            "PreToolUse",
            "Bash",
            serde_json::json!({"command": "cargo test | tail -20"}),
        ),
        make_sre_tool_input(
            3000,
            "PreToolUse",
            "Bash",
            serde_json::json!({"command": "cargo test | head -5"}),
        ),
    ];
    let findings = run_rule("output_parsing_struggle", &records);
    assert!(
        findings.is_empty(),
        "output_parsing_struggle must produce no findings for sre domain; got: {:?}",
        findings
    );
}

/// Rule 5 (session): session_timeout — source_domain guard isolates sre records.
#[test]
fn test_session_timeout_ignores_non_claude_code_domain() {
    let three_hours_ms: u64 = 3 * 60 * 60 * 1000;
    let records = vec![
        make_sre(1000, "PreToolUse"),
        make_sre(1000 + three_hours_ms, "PreToolUse"),
    ];
    let findings = run_rule("session_timeout", &records);
    assert!(
        findings.is_empty(),
        "session_timeout must produce no findings for sre domain; got: {:?}",
        findings
    );
}

/// Rule 6 (session): cold_restart — source_domain guard isolates sre records.
#[test]
fn test_cold_restart_ignores_non_claude_code_domain() {
    let gap_ms: u64 = 35 * 60 * 1000;
    let records = vec![
        ObservationRecord {
            ts: 1000,
            event_type: "PreToolUse".to_string(),
            source_domain: "sre".to_string(),
            session_id: "sre-sess-1".to_string(),
            tool: Some("Read".to_string()),
            input: Some(serde_json::json!({"file_path": "/tmp/a.log"})),
            response_size: None,
            response_snippet: None,
        },
        ObservationRecord {
            ts: 2000,
            event_type: "PreToolUse".to_string(),
            source_domain: "sre".to_string(),
            session_id: "sre-sess-1".to_string(),
            tool: Some("Read".to_string()),
            input: Some(serde_json::json!({"file_path": "/tmp/b.log"})),
            response_size: None,
            response_snippet: None,
        },
        ObservationRecord {
            ts: 1000 + gap_ms,
            event_type: "PreToolUse".to_string(),
            source_domain: "sre".to_string(),
            session_id: "sre-sess-1".to_string(),
            tool: Some("Read".to_string()),
            input: Some(serde_json::json!({"file_path": "/tmp/a.log"})),
            response_size: None,
            response_snippet: None,
        },
        ObservationRecord {
            ts: 2000 + gap_ms,
            event_type: "PreToolUse".to_string(),
            source_domain: "sre".to_string(),
            session_id: "sre-sess-1".to_string(),
            tool: Some("Read".to_string()),
            input: Some(serde_json::json!({"file_path": "/tmp/b.log"})),
            response_size: None,
            response_snippet: None,
        },
    ];
    let findings = run_rule("cold_restart", &records);
    assert!(
        findings.is_empty(),
        "cold_restart must produce no findings for sre domain; got: {:?}",
        findings
    );
}

/// Rule 7 (session): coordinator_respawns — source_domain guard isolates sre records.
#[test]
fn test_coordinator_respawns_ignores_non_claude_code_domain() {
    let records: Vec<ObservationRecord> = (0..5)
        .map(|i| make_sre_tool(i * 1000, "SubagentStart", "uni-scrum-master"))
        .collect();
    let findings = run_rule("coordinator_respawns", &records);
    assert!(
        findings.is_empty(),
        "coordinator_respawns must produce no findings for sre domain; got: {:?}",
        findings
    );
}

/// Rule 8 (session): post_completion_work — source_domain guard isolates sre records.
#[test]
fn test_post_completion_work_ignores_non_claude_code_domain() {
    // 80 records + completed task update + 20 post-completion records, all sre.
    let mut records: Vec<ObservationRecord> = (0u64..80)
        .map(|i| make_sre_tool(i * 100, "PreToolUse", "Read"))
        .collect();
    records.push(ObservationRecord {
        ts: 8000,
        event_type: "PreToolUse".to_string(),
        source_domain: "sre".to_string(),
        session_id: "sre-sess-1".to_string(),
        tool: Some("TaskUpdate".to_string()),
        input: Some(serde_json::json!({"taskId": "1", "status": "completed"})),
        response_size: None,
        response_snippet: None,
    });
    records.extend(
        (0u64..20).map(|i| make_sre_tool(8100 + i * 100, "PreToolUse", "Read")),
    );
    let findings = run_rule("post_completion_work", &records);
    assert!(
        findings.is_empty(),
        "post_completion_work must produce no findings for sre domain; got: {:?}",
        findings
    );
}

/// Rule 9 (session): rework_events — source_domain guard isolates sre records.
#[test]
fn test_rework_events_ignores_non_claude_code_domain() {
    let records = vec![
        ObservationRecord {
            ts: 1000,
            event_type: "PreToolUse".to_string(),
            source_domain: "sre".to_string(),
            session_id: "sre-sess-1".to_string(),
            tool: Some("TaskUpdate".to_string()),
            input: Some(serde_json::json!({"taskId": "1", "status": "in_progress"})),
            response_size: None,
            response_snippet: None,
        },
        ObservationRecord {
            ts: 2000,
            event_type: "PreToolUse".to_string(),
            source_domain: "sre".to_string(),
            session_id: "sre-sess-1".to_string(),
            tool: Some("TaskUpdate".to_string()),
            input: Some(serde_json::json!({"taskId": "1", "status": "completed"})),
            response_size: None,
            response_snippet: None,
        },
        ObservationRecord {
            ts: 3000,
            event_type: "PreToolUse".to_string(),
            source_domain: "sre".to_string(),
            session_id: "sre-sess-1".to_string(),
            tool: Some("TaskUpdate".to_string()),
            input: Some(serde_json::json!({"taskId": "1", "status": "in_progress"})),
            response_size: None,
            response_snippet: None,
        },
    ];
    let findings = run_rule("rework_events", &records);
    assert!(
        findings.is_empty(),
        "rework_events must produce no findings for sre domain; got: {:?}",
        findings
    );
}

/// Rule 10 (agent): context_load — source_domain guard isolates sre records.
#[test]
fn test_context_load_ignores_non_claude_code_domain() {
    // 200 KB of Read PostToolUse before a Write, all sre domain.
    let records = vec![
        ObservationRecord {
            ts: 1000,
            event_type: "PostToolUse".to_string(),
            source_domain: "sre".to_string(),
            session_id: "sre-sess-1".to_string(),
            tool: Some("Read".to_string()),
            input: Some(serde_json::json!({"file_path": "/tmp/a.log"})),
            response_size: Some(102_400),
            response_snippet: None,
        },
        ObservationRecord {
            ts: 2000,
            event_type: "PostToolUse".to_string(),
            source_domain: "sre".to_string(),
            session_id: "sre-sess-1".to_string(),
            tool: Some("Read".to_string()),
            input: Some(serde_json::json!({"file_path": "/tmp/b.log"})),
            response_size: Some(102_400),
            response_snippet: None,
        },
        make_sre_tool(3000, "PostToolUse", "Write"),
    ];
    let findings = run_rule("context_load", &records);
    assert!(
        findings.is_empty(),
        "context_load must produce no findings for sre domain; got: {:?}",
        findings
    );
}

/// Rule 11 (agent): lifespan — source_domain guard isolates sre records.
#[test]
fn test_lifespan_ignores_non_claude_code_domain() {
    let sixty_mins_ms: u64 = 60 * 60 * 1000;
    let records = vec![
        make_sre_tool(1000, "SubagentStart", "agent-worker"),
        make_sre_tool(1000 + sixty_mins_ms, "SubagentStop", "agent-worker"),
    ];
    let findings = run_rule("lifespan", &records);
    assert!(
        findings.is_empty(),
        "lifespan must produce no findings for sre domain; got: {:?}",
        findings
    );
}

/// Rule 12 (agent): file_breadth — source_domain guard isolates sre records.
#[test]
fn test_file_breadth_ignores_non_claude_code_domain() {
    let records: Vec<ObservationRecord> = (0u64..25)
        .map(|i| {
            ObservationRecord {
                ts: i * 1000,
                event_type: "PreToolUse".to_string(),
                source_domain: "sre".to_string(),
                session_id: "sre-sess-1".to_string(),
                tool: Some("Read".to_string()),
                input: Some(serde_json::json!({"file_path": format!("/tmp/file_{i}.log")})),
                response_size: None,
                response_snippet: None,
            }
        })
        .collect();
    let findings = run_rule("file_breadth", &records);
    assert!(
        findings.is_empty(),
        "file_breadth must produce no findings for sre domain; got: {:?}",
        findings
    );
}

/// Rule 13 (agent): reread_rate — source_domain guard isolates sre records.
#[test]
fn test_reread_rate_ignores_non_claude_code_domain() {
    // 4 files each read twice = 4 re-reads > threshold of 3
    let paths = ["/tmp/a.log", "/tmp/b.log", "/tmp/c.log", "/tmp/d.log"];
    let mut records: Vec<ObservationRecord> = Vec::new();
    for (i, path) in paths.iter().enumerate() {
        for rep in 0..2u64 {
            records.push(ObservationRecord {
                ts: (i as u64 * 2 + rep) * 1000,
                event_type: "PreToolUse".to_string(),
                source_domain: "sre".to_string(),
                session_id: "sre-sess-1".to_string(),
                tool: Some("Read".to_string()),
                input: Some(serde_json::json!({"file_path": path})),
                response_size: None,
                response_snippet: None,
            });
        }
    }
    let findings = run_rule("reread_rate", &records);
    assert!(
        findings.is_empty(),
        "reread_rate must produce no findings for sre domain; got: {:?}",
        findings
    );
}

/// Rule 14 (agent): mutation_spread — source_domain guard isolates sre records.
#[test]
fn test_mutation_spread_ignores_non_claude_code_domain() {
    let records: Vec<ObservationRecord> = (0u64..12)
        .map(|i| ObservationRecord {
            ts: i * 1000,
            event_type: "PreToolUse".to_string(),
            source_domain: "sre".to_string(),
            session_id: "sre-sess-1".to_string(),
            tool: Some("Write".to_string()),
            input: Some(serde_json::json!({"file_path": format!("/tmp/file_{i}.log")})),
            response_size: None,
            response_snippet: None,
        })
        .collect();
    let findings = run_rule("mutation_spread", &records);
    assert!(
        findings.is_empty(),
        "mutation_spread must produce no findings for sre domain; got: {:?}",
        findings
    );
}

/// Rule 15 (agent): compile_cycles — source_domain guard isolates sre records.
#[test]
fn test_compile_cycles_ignores_non_claude_code_domain() {
    // 8 cargo test commands (above threshold of 6), all sre domain
    let records: Vec<ObservationRecord> = (0u64..8)
        .map(|i| {
            make_sre_tool_input(
                i * 1000,
                "PreToolUse",
                "Bash",
                serde_json::json!({"command": "cargo test --workspace"}),
            )
        })
        .collect();
    let findings = run_rule("compile_cycles", &records);
    assert!(
        findings.is_empty(),
        "compile_cycles must produce no findings for sre domain; got: {:?}",
        findings
    );
}

/// Rule 16 (agent): edit_bloat — source_domain guard isolates sre records.
#[test]
fn test_edit_bloat_ignores_non_claude_code_domain() {
    // Two Edit PostToolUse records with 60+ KB responses (above 50 KB threshold)
    let records = vec![
        ObservationRecord {
            ts: 1000,
            event_type: "PostToolUse".to_string(),
            source_domain: "sre".to_string(),
            session_id: "sre-sess-1".to_string(),
            tool: Some("Edit".to_string()),
            input: Some(serde_json::json!({"file_path": "/tmp/edit.log"})),
            response_size: Some(60_000),
            response_snippet: None,
        },
        ObservationRecord {
            ts: 2000,
            event_type: "PostToolUse".to_string(),
            source_domain: "sre".to_string(),
            session_id: "sre-sess-1".to_string(),
            tool: Some("Edit".to_string()),
            input: Some(serde_json::json!({"file_path": "/tmp/edit2.log"})),
            response_size: Some(70_000),
            response_snippet: None,
        },
    ];
    let findings = run_rule("edit_bloat", &records);
    assert!(
        findings.is_empty(),
        "edit_bloat must produce no findings for sre domain; got: {:?}",
        findings
    );
}

/// Rule 17 (scope): source_file_count — source_domain guard isolates sre records.
#[test]
fn test_source_file_count_ignores_non_claude_code_domain() {
    let records: Vec<ObservationRecord> = (0u64..8)
        .map(|i| ObservationRecord {
            ts: i * 1000,
            event_type: "PostToolUse".to_string(),
            source_domain: "sre".to_string(),
            session_id: "sre-sess-1".to_string(),
            tool: Some("Write".to_string()),
            input: Some(serde_json::json!({"file_path": format!("/tmp/file_{i}.rs")})),
            response_size: None,
            response_snippet: None,
        })
        .collect();
    let findings = run_rule("source_file_count", &records);
    assert!(
        findings.is_empty(),
        "source_file_count must produce no findings for sre domain; got: {:?}",
        findings
    );
}

/// Rule 18 (scope): design_artifact_count — source_domain guard isolates sre records.
#[test]
fn test_design_artifact_count_ignores_non_claude_code_domain() {
    let records: Vec<ObservationRecord> = (0u64..26)
        .map(|i| ObservationRecord {
            ts: i * 1000,
            event_type: "PreToolUse".to_string(),
            source_domain: "sre".to_string(),
            session_id: "sre-sess-1".to_string(),
            tool: Some("Edit".to_string()),
            input: Some(
                serde_json::json!({"file_path": format!("product/features/col-002b/doc_{i}.md")}),
            ),
            response_size: None,
            response_snippet: None,
        })
        .collect();
    let findings = run_rule("design_artifact_count", &records);
    assert!(
        findings.is_empty(),
        "design_artifact_count must produce no findings for sre domain; got: {:?}",
        findings
    );
}

/// Rule 19 (scope): adr_count — source_domain guard isolates sre records.
#[test]
fn test_adr_count_ignores_non_claude_code_domain() {
    let records: Vec<ObservationRecord> = (1u64..=5)
        .map(|i| ObservationRecord {
            ts: i * 1000,
            event_type: "PreToolUse".to_string(),
            source_domain: "sre".to_string(),
            session_id: "sre-sess-1".to_string(),
            tool: Some("Write".to_string()),
            input: Some(
                serde_json::json!({"file_path": format!("product/features/col-002b/ADR-00{i}.md")}),
            ),
            response_size: None,
            response_snippet: None,
        })
        .collect();
    let findings = run_rule("adr_count", &records);
    assert!(
        findings.is_empty(),
        "adr_count must produce no findings for sre domain; got: {:?}",
        findings
    );
}

/// Rule 20 (scope): post_delivery_issues — source_domain guard isolates sre records.
#[test]
fn test_post_delivery_issues_ignores_non_claude_code_domain() {
    let records = vec![
        ObservationRecord {
            ts: 1000,
            event_type: "PreToolUse".to_string(),
            source_domain: "sre".to_string(),
            session_id: "sre-sess-1".to_string(),
            tool: Some("TaskUpdate".to_string()),
            input: Some(serde_json::json!({"taskId": "1", "status": "completed"})),
            response_size: None,
            response_snippet: None,
        },
        make_sre_tool_input(
            2000,
            "PreToolUse",
            "Bash",
            serde_json::json!({"command": "gh issue create --title 'Bug'"}),
        ),
    ];
    let findings = run_rule("post_delivery_issues", &records);
    assert!(
        findings.is_empty(),
        "post_delivery_issues must produce no findings for sre domain; got: {:?}",
        findings
    );
}

/// Rule 21 (scope): phase_duration_outlier — source_domain guard isolates sre records.
/// This rule always returns empty from detect() by design (baseline comparison handles it),
/// but the source_domain guard is still present per ADR-005.
#[test]
fn test_phase_duration_outlier_ignores_non_claude_code_domain() {
    let records: Vec<ObservationRecord> = (0u64..10)
        .map(|i| make_sre(i * 1000, "PostToolUse"))
        .collect();
    let findings = run_rule("phase_duration_outlier", &records);
    assert!(
        findings.is_empty(),
        "phase_duration_outlier must produce no findings for sre domain; got: {:?}",
        findings
    );
}

// ── GAP-02: Backward compatibility snapshot test ──────────────────────────────

/// T-DET-COMPAT-02: End-to-end retrospective pipeline smoke test with a fixed
/// claude-code session fixture.
///
/// Purpose: regression detection. If any rule is accidentally switched off, the
/// finding category disappears from the output — this test catches that.
///
/// Fixture: a representative claude-code session with 2 agent spawns, ~50 tool
/// calls, some Bash compile commands, and a task completion.
#[test]
fn test_retrospective_report_backward_compat_claude_code_fixture() {
    let fixture = build_representative_claude_code_fixture();

    // Run full detection pipeline
    let rules = default_rules(None);
    let findings = detect_hotspots(&fixture, &rules);

    // No panic: pipeline completes (assertion above via reaching this line)

    // All claude-code rules are evaluated: call chain completed, no panic.
    // Verify compute_metric_vector also completes without panic.
    let mv = compute_metric_vector(&fixture, &findings, 1_000_000);

    // RetrospectiveReport fields: computed_at must be set.
    assert_eq!(mv.computed_at, 1_000_000, "MetricVector.computed_at must be preserved");

    // The fixture contains claude-code records of categories: Agent, Friction, Session, Scope.
    // Verify that at minimum the Agent and Friction categories appear in findings —
    // these are reliably triggered by the fixture (compile cycles + permission retries).
    let categories: Vec<HotspotCategory> =
        findings.iter().map(|f| f.category.clone()).collect();

    // Compile cycles (Agent category) must fire: fixture has 8 cargo test commands.
    assert!(
        categories.contains(&HotspotCategory::Agent),
        "expected Agent category findings from compile_cycles; got categories: {:?}",
        categories
    );

    // Permission retries (Friction category) must fire: fixture has 8 PreToolUse / 2 PostToolUse for Read.
    assert!(
        categories.contains(&HotspotCategory::Friction),
        "expected Friction category findings from permission_retries; got categories: {:?}",
        categories
    );

    // Session category must fire: fixture has a 3-hour gap.
    assert!(
        categories.contains(&HotspotCategory::Session),
        "expected Session category findings from session_timeout; got categories: {:?}",
        categories
    );

    // Verify metric vector universal fields are produced (not default-zero from missing data).
    // total_tool_calls must reflect the fixture records.
    assert!(
        mv.universal.total_tool_calls > 0,
        "expected total_tool_calls > 0 for fixture with 50+ tool-use records; got {}",
        mv.universal.total_tool_calls
    );
}

/// Build a representative hardcoded claude-code session fixture.
///
/// The fixture is designed to trigger multiple rule categories deterministically:
/// - compile_cycles (Agent): 8 cargo test commands
/// - permission_retries (Friction): 8 PreToolUse + 2 PostToolUse for Read tool
/// - session_timeout (Session): 3-hour gap between two events
/// - sleep_workarounds (Friction): one sleep command
fn build_representative_claude_code_fixture() -> Vec<ObservationRecord> {
    let mut records: Vec<ObservationRecord> = Vec::new();

    let base_ts: u64 = 1_700_000_000_000;

    // 2 agent spawns (SubagentStart)
    records.push(ObservationRecord {
        ts: base_ts,
        event_type: "SubagentStart".to_string(),
        source_domain: "claude-code".to_string(),
        session_id: "sess-fixture-1".to_string(),
        tool: Some("uni-rust-dev".to_string()),
        input: None,
        response_size: None,
        response_snippet: None,
    });
    records.push(ObservationRecord {
        ts: base_ts + 5_000,
        event_type: "SubagentStart".to_string(),
        source_domain: "claude-code".to_string(),
        session_id: "sess-fixture-1".to_string(),
        tool: Some("uni-test-specialist".to_string()),
        input: None,
        response_size: None,
        response_snippet: None,
    });

    // ~20 Read PreToolUse calls (contributes to file_breadth, reread_rate)
    for i in 0u64..20 {
        records.push(ObservationRecord {
            ts: base_ts + 10_000 + i * 500,
            event_type: "PreToolUse".to_string(),
            source_domain: "claude-code".to_string(),
            session_id: "sess-fixture-1".to_string(),
            tool: Some("Read".to_string()),
            input: Some(serde_json::json!({"file_path": format!("/workspace/crates/file_{i}.rs")})),
            response_size: None,
            response_snippet: None,
        });
    }

    // 8 Read PreToolUse without matching PostToolUse (triggers permission_retries)
    for i in 0u64..8 {
        records.push(ObservationRecord {
            ts: base_ts + 20_000 + i * 500,
            event_type: "PreToolUse".to_string(),
            source_domain: "claude-code".to_string(),
            session_id: "sess-fixture-1".to_string(),
            tool: Some("Read".to_string()),
            input: None,
            response_size: None,
            response_snippet: None,
        });
    }
    // Only 2 PostToolUse for Read (leaves 6 retries = > 2 threshold)
    for i in 0u64..2 {
        records.push(ObservationRecord {
            ts: base_ts + 24_000 + i * 500,
            event_type: "PostToolUse".to_string(),
            source_domain: "claude-code".to_string(),
            session_id: "sess-fixture-1".to_string(),
            tool: Some("Read".to_string()),
            input: None,
            response_size: None,
            response_snippet: None,
        });
    }

    // 8 cargo test commands (triggers compile_cycles > threshold of 6)
    for i in 0u64..8 {
        records.push(ObservationRecord {
            ts: base_ts + 30_000 + i * 1_000,
            event_type: "PreToolUse".to_string(),
            source_domain: "claude-code".to_string(),
            session_id: "sess-fixture-1".to_string(),
            tool: Some("Bash".to_string()),
            input: Some(serde_json::json!({"command": "cargo test --workspace 2>&1 | tail -30"})),
            response_size: None,
            response_snippet: None,
        });
    }

    // 1 sleep command (triggers sleep_workarounds)
    records.push(ObservationRecord {
        ts: base_ts + 40_000,
        event_type: "PreToolUse".to_string(),
        source_domain: "claude-code".to_string(),
        session_id: "sess-fixture-1".to_string(),
        tool: Some("Bash".to_string()),
        input: Some(serde_json::json!({"command": "sleep 2"})),
        response_size: None,
        response_snippet: None,
    });

    // A 3-hour gap (triggers session_timeout)
    let three_hours_ms: u64 = 3 * 60 * 60 * 1000;
    records.push(ObservationRecord {
        ts: base_ts + 50_000 + three_hours_ms,
        event_type: "PreToolUse".to_string(),
        source_domain: "claude-code".to_string(),
        session_id: "sess-fixture-1".to_string(),
        tool: Some("Read".to_string()),
        input: None,
        response_size: None,
        response_snippet: None,
    });

    // Task completion
    records.push(ObservationRecord {
        ts: base_ts + 60_000 + three_hours_ms,
        event_type: "PreToolUse".to_string(),
        source_domain: "claude-code".to_string(),
        session_id: "sess-fixture-1".to_string(),
        tool: Some("TaskUpdate".to_string()),
        input: Some(serde_json::json!({"taskId": "fixture-task-1", "status": "completed"})),
        response_size: None,
        response_snippet: None,
    });

    records
}
