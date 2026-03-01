# Pseudocode: observe-parser

## Purpose

Parse JSONL session files line-by-line into typed ObservationRecord structs. Handle ISO-8601 timestamp conversion and SubagentStart/Stop field normalization.

## File: `crates/unimatrix-observe/src/parser.rs`

### parse_timestamp

```
pub fn parse_timestamp(ts: &str) -> Result<u64> {
    // Expected format: YYYY-MM-DDTHH:MM:SS.mmmZ (ISO-8601 UTC with milliseconds)
    // Manual parsing, no chrono dependency (SCOPE.md Resolved Decision 13)

    // Validate basic structure: length >= 24, has T separator, ends with Z
    if ts.len() < 24 || ts.as_bytes()[10] != b'T' || !ts.ends_with('Z') {
        return Err(TimestampParse("invalid format"))
    }

    // Parse components by byte position:
    // YYYY-MM-DDTHH:MM:SS.mmmZ
    // 0123456789012345678901234
    year  = parse_u32(ts[0..4])
    month = parse_u32(ts[5..7])
    day   = parse_u32(ts[8..10])
    hour  = parse_u32(ts[11..13])
    min   = parse_u32(ts[14..16])
    sec   = parse_u32(ts[17..19])
    millis = parse_u32(ts[20..23])

    // Validate ranges
    if month < 1 || month > 12 { return Err }
    if day < 1 || day > days_in_month(year, month) { return Err }
    if hour > 23 || min > 59 || sec > 59 { return Err }
    if millis > 999 { return Err }

    // Convert to epoch millis using civil_to_days algorithm (same as response.rs)
    epoch_days = civil_to_days(year, month, day)
    epoch_secs = epoch_days * 86400 + hour * 3600 + min * 60 + sec
    epoch_millis = epoch_secs * 1000 + millis

    return Ok(epoch_millis)
}
```

Helper: `days_in_month(year, month) -> u32` handles leap years.

Helper: `civil_to_days(y, m, d) -> i64` converts civil date to days since epoch. Inverse of the algorithm in `response.rs::format_timestamp`.

### Raw JSONL Record (intermediate deserialization target)

```
// Not part of public API -- used only inside parser
struct RawRecord {
    ts: String,                          // ISO-8601 string
    hook: String,                        // "PreToolUse", "PostToolUse", etc.
    session_id: String,
    // Tool-use fields:
    tool: Option<String>,
    input: Option<serde_json::Value>,
    response_size: Option<u64>,
    response_snippet: Option<String>,
    // Subagent fields:
    agent_type: Option<String>,
    prompt_snippet: Option<String>,
}
```

Derive: Deserialize.

### parse_line

```
fn parse_line(line: &str) -> Option<ObservationRecord> {
    // Deserialize as RawRecord (serde_json::from_str)
    let raw: RawRecord = serde_json::from_str(line).ok()?;

    // Parse timestamp
    let ts = parse_timestamp(&raw.ts).ok()?;

    // Parse hook type
    let hook = match raw.hook.as_str() {
        "PreToolUse" => HookType::PreToolUse,
        "PostToolUse" => HookType::PostToolUse,
        "SubagentStart" => HookType::SubagentStart,
        "SubagentStop" => HookType::SubagentStop,
        _ => return None,
    };

    // Normalize fields based on hook type (Architecture Section 1, FR-02.5)
    let (tool, input) = match hook {
        PreToolUse | PostToolUse => (raw.tool, raw.input),
        SubagentStart => {
            // agent_type -> tool, prompt_snippet -> input (as Value::String)
            let tool = raw.agent_type.filter(|s| !s.is_empty());
            let input = raw.prompt_snippet.map(|s| Value::String(s));
            (tool, input)
        },
        SubagentStop => {
            // agent_type is empty (platform constraint), so tool=None, input=None
            (None, None)
        },
    };

    Some(ObservationRecord {
        ts,
        hook,
        session_id: raw.session_id,
        tool,
        input,
        response_size: raw.response_size,
        response_snippet: raw.response_snippet,
    })
}
```

### parse_session_file

```
pub fn parse_session_file(path: &Path) -> Result<Vec<ObservationRecord>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() { continue; }
        // Skip malformed lines (R-01: no error, just skip)
        if let Some(record) = parse_line(&line) {
            records.push(record);
        }
    }

    // Sort by timestamp for consistent ordering (Architecture edge case)
    records.sort_by_key(|r| r.ts);

    Ok(records)
}
```

## Error Handling

- Malformed lines: return None from parse_line (skip, don't fail)
- File open errors: propagate as ObserveError::Io
- Empty file: return Ok(vec![])
- All-malformed file: return Ok(vec![])

## Key Test Scenarios

- Parse valid PreToolUse line -> correct ObservationRecord
- Parse valid PostToolUse with response_size and snippet
- Parse SubagentStart -> agent_type maps to tool field
- Parse SubagentStop -> tool=None, input=None
- Malformed line skipped, valid lines still parsed (R-01)
- Empty file -> empty vec
- All malformed -> empty vec
- Timestamp parsing at epoch boundaries (R-03)
- Timestamp with leap year date
- Invalid timestamp format rejected
- File with 10K records parses correctly (R-13)
- Lines sorted by timestamp after parse
