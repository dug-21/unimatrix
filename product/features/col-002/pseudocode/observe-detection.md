# Pseudocode: observe-detection

## Purpose

Hotspot detection framework with extensible rule trait. Ships 3 rules: PermissionRetries, SessionTimeout, SleepWorkarounds.

## File: `crates/unimatrix-observe/src/detection.rs`

### DetectionRule Trait

```
pub trait DetectionRule {
    fn name(&self) -> &str;
    fn category(&self) -> HotspotCategory;
    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>;
}
```

### detect_hotspots (Engine)

```
pub fn detect_hotspots(
    records: &[ObservationRecord],
    rules: &[Box<dyn DetectionRule>],
) -> Vec<HotspotFinding> {
    let mut findings = Vec::new();
    for rule in rules {
        findings.extend(rule.detect(records));
    }
    findings
}
```

### default_rules

```
pub fn default_rules() -> Vec<Box<dyn DetectionRule>> {
    vec![
        Box::new(PermissionRetriesRule),
        Box::new(SessionTimeoutRule),
        Box::new(SleepWorkaroundsRule),
    ]
}
```

### Rule 1: PermissionRetriesRule (FR-06.1)

```
struct PermissionRetriesRule;

impl DetectionRule for PermissionRetriesRule {
    fn name(&self) -> &str { "permission_retries" }
    fn category(&self) -> HotspotCategory { HotspotCategory::Friction }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        // Count PreToolUse and PostToolUse per tool name
        let mut pre_counts: HashMap<String, u64> = HashMap::new();
        let mut post_counts: HashMap<String, u64> = HashMap::new();
        let mut evidence_records: HashMap<String, Vec<EvidenceRecord>> = HashMap::new();

        for record in records {
            if let Some(tool) = &record.tool {
                match record.hook {
                    HookType::PreToolUse => {
                        *pre_counts.entry(tool.clone()).or_default() += 1;
                        evidence_records.entry(tool.clone()).or_default().push(
                            EvidenceRecord {
                                description: format!("PreToolUse for {}", tool),
                                ts: record.ts,
                                tool: Some(tool.clone()),
                                detail: format!("Pre-use event at ts={}", record.ts),
                            }
                        );
                    },
                    HookType::PostToolUse => {
                        *post_counts.entry(tool.clone()).or_default() += 1;
                    },
                    _ => {},
                }
            }
        }

        let threshold = 2.0;
        let mut findings = Vec::new();

        for (tool, pre_count) in &pre_counts {
            let post_count = post_counts.get(tool).copied().unwrap_or(0);
            let retries = pre_count.saturating_sub(post_count);
            if retries > threshold as u64 {
                findings.push(HotspotFinding {
                    category: HotspotCategory::Friction,
                    severity: Severity::Warning,
                    rule_name: "permission_retries".to_string(),
                    claim: format!("Tool '{}' had {} permission retries (Pre-Post differential)", tool, retries),
                    measured: retries as f64,
                    threshold,
                    evidence: evidence_records.remove(tool).unwrap_or_default(),
                });
            }
        }

        findings
    }
}
```

### Rule 2: SessionTimeoutRule (FR-06.2)

```
struct SessionTimeoutRule;

const TIMEOUT_GAP_MS: u64 = 2 * 60 * 60 * 1000;  // 2 hours in milliseconds

impl DetectionRule for SessionTimeoutRule {
    fn name(&self) -> &str { "session_timeout" }
    fn category(&self) -> HotspotCategory { HotspotCategory::Session }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        // Group records by session_id
        let mut by_session: HashMap<&str, Vec<&ObservationRecord>> = HashMap::new();
        for record in records {
            by_session.entry(&record.session_id).or_default().push(record);
        }

        let mut findings = Vec::new();

        for (session_id, session_records) in &by_session {
            // Records should already be sorted by ts
            let mut sorted = session_records.clone();
            sorted.sort_by_key(|r| r.ts);

            for window in sorted.windows(2) {
                let gap = window[1].ts.saturating_sub(window[0].ts);
                if gap > TIMEOUT_GAP_MS {
                    let gap_hours = gap as f64 / (1000.0 * 60.0 * 60.0);
                    findings.push(HotspotFinding {
                        category: HotspotCategory::Session,
                        severity: Severity::Warning,
                        rule_name: "session_timeout".to_string(),
                        claim: format!("Session '{}' had a {:.1}h gap", session_id, gap_hours),
                        measured: gap_hours,
                        threshold: 2.0,
                        evidence: vec![
                            EvidenceRecord {
                                description: "Gap start".to_string(),
                                ts: window[0].ts,
                                tool: window[0].tool.clone(),
                                detail: format!("Last event before gap"),
                            },
                            EvidenceRecord {
                                description: "Gap end".to_string(),
                                ts: window[1].ts,
                                tool: window[1].tool.clone(),
                                detail: format!("First event after gap"),
                            },
                        ],
                    });
                }
            }
        }

        findings
    }
}
```

### Rule 3: SleepWorkaroundsRule (FR-06.3)

```
struct SleepWorkaroundsRule;

impl DetectionRule for SleepWorkaroundsRule {
    fn name(&self) -> &str { "sleep_workarounds" }
    fn category(&self) -> HotspotCategory { HotspotCategory::Friction }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let mut evidence = Vec::new();

        for record in records {
            // Match Bash tool records with sleep in input
            if record.tool.as_deref() == Some("Bash") {
                if let Some(input) = &record.input {
                    let input_str = match input {
                        Value::String(s) => s.clone(),
                        Value::Object(map) => {
                            // Check "command" field in Bash tool input
                            map.get("command")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string()
                        },
                        _ => String::new(),
                    };

                    // Match sleep command pattern
                    if contains_sleep_command(&input_str) {
                        evidence.push(EvidenceRecord {
                            description: "Sleep command in Bash input".to_string(),
                            ts: record.ts,
                            tool: Some("Bash".to_string()),
                            detail: truncate(&input_str, 200),
                        });
                    }
                }
            }
        }

        if !evidence.is_empty() {
            let count = evidence.len();
            vec![HotspotFinding {
                category: HotspotCategory::Friction,
                severity: Severity::Info,
                rule_name: "sleep_workarounds".to_string(),
                claim: format!("Found {} sleep workaround(s) in Bash commands", count),
                measured: count as f64,
                threshold: 1.0,
                evidence,
            }]
        } else {
            vec![]
        }
    }
}
```

### Helper: contains_sleep_command

```
fn contains_sleep_command(s: &str) -> bool {
    // Match "sleep" as a command (not part of another word)
    // Patterns: "sleep N", "sleep $VAR", at start or after ; | && ||
    s.split(|c: char| c == ';' || c == '|' || c == '&' || c == '\n')
        .any(|segment| {
            let trimmed = segment.trim();
            trimmed.starts_with("sleep ") || trimmed == "sleep"
        })
}
```

### Helper: truncate

```
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len { s.to_string() }
    else { format!("{}...", &s[..max_len]) }
}
```

## Error Handling

- Detection rules never fail -- they return empty findings if no matches
- The engine collects from all rules without short-circuiting

## Key Test Scenarios

- Permission retries: 5 Pre + 2 Post for tool X -> finding with retries=3 (AC-11)
- Permission retries: 3 Pre + 3 Post -> no finding (R-10 scenario 1)
- Permission retries: multiple tools, only one exceeds -> one finding (R-10 scenario 3)
- Session timeout: 3-hour gap -> finding (AC-12)
- Session timeout: 1-hour gap -> no finding
- Sleep workarounds: Bash with "sleep 5" -> finding (AC-13)
- Sleep workarounds: no Bash records -> no finding
- Custom rule implementing trait -> engine runs it (AC-18, R-05)
- Empty records -> no findings from any rule
- detect_hotspots collects from all rules (FR-05.4)
