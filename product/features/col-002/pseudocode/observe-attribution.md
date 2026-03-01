# Pseudocode: observe-attribution

## Purpose

Feature attribution logic: given parsed sessions, determine which ObservationRecords belong to a target feature. Content-based sequential scanning, not git branch (SCOPE.md Resolved Decision 3, 9).

## File: `crates/unimatrix-observe/src/attribution.rs`

### Signal Extraction

```
fn extract_feature_signal(record: &ObservationRecord) -> Option<String> {
    // Check tool input for feature signals (priority order per FR-04.3):

    // (a) File paths matching product/features/{id}/
    // (b) Task subjects containing feature IDs
    // (c) Git checkout commands with feature/{id}

    if let Some(input) = &record.input {
        let input_str = match input {
            Value::String(s) => s.clone(),
            Value::Object(_) => serde_json::to_string(input).unwrap_or_default(),
            _ => String::new(),
        };

        // (a) File path pattern: product/features/{phase}-{NNN}/
        // Regex-like: look for "product/features/" followed by {phase}-{NNN}
        if let Some(feature_id) = extract_from_path(&input_str) {
            return Some(feature_id);
        }

        // (b) Task subject with feature ID pattern: {phase}-{NNN}
        // Check for known phase prefixes: ass, nxs, col, vnc, alc, crt, mtx, dsn, nan
        if let Some(feature_id) = extract_feature_id_pattern(&input_str) {
            return Some(feature_id);
        }

        // (c) Git checkout feature/{id}
        if let Some(feature_id) = extract_from_git_checkout(&input_str) {
            return Some(feature_id);
        }
    }

    None
}
```

### Helper: extract_from_path

```
fn extract_from_path(s: &str) -> Option<String> {
    // Find "product/features/" and capture the next segment
    // Pattern: product/features/{phase}-{NNN}/
    // where phase is alphabetic, NNN is numeric

    for part in s.split("product/features/") {
        // Skip the first part before the match
        if let Some(feature_dir) = part.split('/').next() {
            if is_valid_feature_id(feature_dir) {
                return Some(feature_dir.to_string());
            }
        }
    }
    None
}
```

### Helper: is_valid_feature_id

```
fn is_valid_feature_id(s: &str) -> bool {
    // Pattern: {alpha}-{digits}
    // e.g., col-002, nxs-001, alc-002
    let parts: Vec<&str> = s.splitn(2, '-').collect();
    if parts.len() != 2 { return false; }
    parts[0].chars().all(|c| c.is_ascii_alphabetic())
        && parts[1].chars().all(|c| c.is_ascii_digit())
        && !parts[1].is_empty()
}
```

### Helper: extract_feature_id_pattern

```
fn extract_feature_id_pattern(s: &str) -> Option<String> {
    // Scan for known phase prefixes followed by -NNN pattern
    // Use word boundary detection (not in middle of longer identifier)
    let known_prefixes = ["ass", "nxs", "col", "vnc", "alc", "crt", "mtx", "dsn", "nan"];

    for word in s.split_whitespace() {
        let candidate = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
        if is_valid_feature_id(candidate) {
            let prefix = candidate.split('-').next().unwrap();
            if known_prefixes.contains(&prefix) {
                return Some(candidate.to_string());
            }
        }
    }
    None
}
```

### Helper: extract_from_git_checkout

```
fn extract_from_git_checkout(s: &str) -> Option<String> {
    // Look for "feature/" pattern (from git checkout -b feature/col-002)
    if let Some(idx) = s.find("feature/") {
        let rest = &s[idx + 8..];
        let candidate: String = rest.chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-')
            .collect();
        if is_valid_feature_id(&candidate) {
            return Some(candidate);
        }
    }
    None
}
```

### attribute_sessions

```
pub fn attribute_sessions(
    sessions: &[ParsedSession],
    target_feature: &str,
) -> Vec<ObservationRecord> {
    let mut attributed = Vec::new();

    for session in sessions {
        // Walk records in timestamp order (already sorted by parser)
        let mut current_feature: Option<String> = None;
        let mut session_records: Vec<&ObservationRecord> = Vec::new();
        let mut partitions: Vec<(Option<String>, Vec<&ObservationRecord>)> = Vec::new();

        for record in &session.records {
            if let Some(signal) = extract_feature_signal(record) {
                if current_feature.as_deref() != Some(&signal) {
                    // Feature switch point -- save current partition
                    if !session_records.is_empty() {
                        partitions.push((current_feature.clone(), session_records.clone()));
                        session_records.clear();
                    }
                    current_feature = Some(signal);
                }
            }
            session_records.push(record);
        }
        // Push final partition
        if !session_records.is_empty() {
            partitions.push((current_feature.clone(), session_records));
        }

        // FR-04.4: Records before any feature ID -> attributed to first feature found
        let first_feature = partitions.iter()
            .find_map(|(f, _)| f.clone());

        for (feature, records) in &partitions {
            let effective_feature = feature.as_ref().or(first_feature.as_ref());
            if effective_feature.is_some_and(|f| f == target_feature) {
                for record in records {
                    attributed.push((*record).clone());
                }
            }
        }
    }

    attributed
}
```

## Error Handling

- No errors from attribution itself -- it filters, never fails
- Empty sessions produce empty output
- Sessions with no feature signals are excluded (FR-04.6)

## Key Test Scenarios

- Single-feature session: all records attributed (R-02 scenario 1)
- Two-feature session: records partitioned at switch point (R-02 scenario 2)
- Session with no feature signals: excluded (R-02 scenario 3)
- Feature ID in file path signal works (R-02 scenario 4)
- Feature ID in task subject signal works
- Git checkout signal works
- Records before first feature ID: attributed to first feature (FR-04.4)
- Three sessions, two with target feature: both included (R-02 scenario 6)
- Multi-feature session with 3+ features: correct partitioning
