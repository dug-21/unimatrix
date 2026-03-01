//! JSONL line parsing, ISO-8601 timestamp conversion, and SubagentStart/Stop field normalization.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde::Deserialize;

use crate::error::{ObserveError, Result};
use crate::types::{HookType, ObservationRecord};

/// Intermediate deserialization target for raw JSONL records.
#[derive(Deserialize)]
struct RawRecord {
    ts: String,
    hook: String,
    session_id: String,
    // Tool-use fields
    tool: Option<String>,
    input: Option<serde_json::Value>,
    response_size: Option<u64>,
    response_snippet: Option<String>,
    // Subagent fields
    agent_type: Option<String>,
    prompt_snippet: Option<String>,
}

/// Parse an ISO-8601 timestamp with milliseconds to Unix epoch milliseconds.
///
/// Expected format: `YYYY-MM-DDTHH:MM:SS.mmmZ` (exactly 24 chars).
pub fn parse_timestamp(ts: &str) -> Result<u64> {
    let bytes = ts.as_bytes();

    if bytes.len() < 24 || bytes[10] != b'T' || !ts.ends_with('Z') {
        return Err(ObserveError::TimestampParse(format!(
            "invalid format: expected YYYY-MM-DDTHH:MM:SS.mmmZ, got '{ts}'"
        )));
    }

    let year = parse_u32(&ts[0..4])?;
    let month = parse_u32(&ts[5..7])?;
    let day = parse_u32(&ts[8..10])?;
    let hour = parse_u32(&ts[11..13])?;
    let min = parse_u32(&ts[14..16])?;
    let sec = parse_u32(&ts[17..19])?;
    let millis = parse_u32(&ts[20..23])?;

    if month < 1 || month > 12 {
        return Err(ObserveError::TimestampParse(format!("invalid month: {month}")));
    }
    if day < 1 || day > days_in_month(year, month) {
        return Err(ObserveError::TimestampParse(format!(
            "invalid day: {day} for {year}-{month:02}"
        )));
    }
    if hour > 23 {
        return Err(ObserveError::TimestampParse(format!("invalid hour: {hour}")));
    }
    if min > 59 {
        return Err(ObserveError::TimestampParse(format!("invalid minute: {min}")));
    }
    if sec > 59 {
        return Err(ObserveError::TimestampParse(format!("invalid second: {sec}")));
    }
    if millis > 999 {
        return Err(ObserveError::TimestampParse(format!("invalid millis: {millis}")));
    }

    let epoch_days = civil_to_days(year as i64, month, day);
    let epoch_secs = epoch_days * 86400 + (hour as i64) * 3600 + (min as i64) * 60 + (sec as i64);
    let epoch_millis = (epoch_secs as u64) * 1000 + (millis as u64);

    Ok(epoch_millis)
}

fn parse_u32(s: &str) -> Result<u32> {
    s.parse::<u32>()
        .map_err(|_| ObserveError::TimestampParse(format!("invalid number: '{s}'")))
}

fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 => 31,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        3 => 31,
        4 => 30,
        5 => 31,
        6 => 30,
        7 => 31,
        8 => 31,
        9 => 30,
        10 => 31,
        11 => 30,
        12 => 31,
        _ => 0,
    }
}

/// Convert civil date to days since Unix epoch (1970-01-01).
/// Algorithm: inverse of Howard Hinnant's civil_from_days.
fn civil_to_days(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + (doe as i64) - 719_468
}

/// Parse a single JSONL line into an ObservationRecord.
///
/// Returns None for malformed lines (skip, don't fail).
fn parse_line(line: &str) -> Option<ObservationRecord> {
    let raw: RawRecord = serde_json::from_str(line).ok()?;

    let ts = parse_timestamp(&raw.ts).ok()?;

    let hook = match raw.hook.as_str() {
        "PreToolUse" => HookType::PreToolUse,
        "PostToolUse" => HookType::PostToolUse,
        "SubagentStart" => HookType::SubagentStart,
        "SubagentStop" => HookType::SubagentStop,
        _ => return None,
    };

    // Normalize fields based on hook type (FR-02.5)
    let (tool, input) = match hook {
        HookType::PreToolUse | HookType::PostToolUse => (raw.tool, raw.input),
        HookType::SubagentStart => {
            let tool = raw.agent_type.filter(|s| !s.is_empty());
            let input = raw.prompt_snippet.map(serde_json::Value::String);
            (tool, input)
        }
        HookType::SubagentStop => (None, None),
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

/// Parse a JSONL session file into a sorted vector of ObservationRecords.
///
/// Malformed lines are skipped (R-01). Records are sorted by timestamp.
pub fn parse_session_file(path: &Path) -> Result<Vec<ObservationRecord>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(record) = parse_line(&line) {
            records.push(record);
        }
    }

    records.sort_by_key(|r| r.ts);
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pre_tool_json(ts: &str, session: &str, tool: &str) -> String {
        format!(
            r#"{{"ts":"{ts}","hook":"PreToolUse","session_id":"{session}","tool":"{tool}","input":{{"file_path":"/tmp/test"}}}}"#
        )
    }

    fn make_post_tool_json(ts: &str, session: &str, tool: &str) -> String {
        format!(
            r#"{{"ts":"{ts}","hook":"PostToolUse","session_id":"{session}","tool":"{tool}","input":null,"response_size":1024,"response_snippet":"some output"}}"#
        )
    }

    #[test]
    fn test_parse_timestamp_standard() {
        let ts = parse_timestamp("2024-06-15T14:30:45.123Z").unwrap();
        // Verify it's a reasonable epoch millis value (2024 is ~1718xxx seconds)
        assert!(ts > 1_718_000_000_000);
        assert!(ts < 1_720_000_000_000);
    }

    #[test]
    fn test_parse_timestamp_epoch_zero() {
        let ts = parse_timestamp("1970-01-01T00:00:00.000Z").unwrap();
        assert_eq!(ts, 0);
    }

    #[test]
    fn test_parse_timestamp_2038_boundary() {
        let ts = parse_timestamp("2038-01-19T03:14:07.000Z").unwrap();
        assert_eq!(ts, 2_147_483_647_000);
    }

    #[test]
    fn test_parse_timestamp_leap_year() {
        let ts = parse_timestamp("2024-02-29T12:00:00.000Z");
        assert!(ts.is_ok());
    }

    #[test]
    fn test_parse_timestamp_midnight() {
        let ts = parse_timestamp("2024-01-01T00:00:00.000Z").unwrap();
        assert!(ts > 0);
    }

    #[test]
    fn test_parse_timestamp_end_of_day() {
        let ts = parse_timestamp("2024-12-31T23:59:59.999Z").unwrap();
        assert!(ts > 0);
    }

    #[test]
    fn test_parse_timestamp_invalid_format() {
        assert!(parse_timestamp("2024/01/01 12:00:00").is_err());
    }

    #[test]
    fn test_parse_timestamp_no_z_suffix() {
        assert!(parse_timestamp("2024-01-01T12:00:00.000").is_err());
    }

    #[test]
    fn test_parse_timestamp_invalid_month() {
        assert!(parse_timestamp("2024-13-01T00:00:00.000Z").is_err());
    }

    #[test]
    fn test_parse_timestamp_feb_29_non_leap() {
        assert!(parse_timestamp("2023-02-29T00:00:00.000Z").is_err());
    }

    #[test]
    fn test_parse_line_pre_tool_use() {
        let json = make_pre_tool_json("2024-06-15T14:30:45.000Z", "sess-1", "Read");
        let record = parse_line(&json).unwrap();
        assert_eq!(record.hook, HookType::PreToolUse);
        assert_eq!(record.session_id, "sess-1");
        assert_eq!(record.tool, Some("Read".to_string()));
        assert!(record.input.is_some());
    }

    #[test]
    fn test_parse_line_post_tool_use() {
        let json = make_post_tool_json("2024-06-15T14:30:45.000Z", "sess-1", "Read");
        let record = parse_line(&json).unwrap();
        assert_eq!(record.hook, HookType::PostToolUse);
        assert_eq!(record.response_size, Some(1024));
        assert_eq!(record.response_snippet, Some("some output".to_string()));
    }

    #[test]
    fn test_parse_line_subagent_start() {
        let json = r#"{"ts":"2024-06-15T14:30:45.000Z","hook":"SubagentStart","session_id":"sess-1","agent_type":"uni-pseudocode","prompt_snippet":"Design components"}"#;
        let record = parse_line(json).unwrap();
        assert_eq!(record.hook, HookType::SubagentStart);
        assert_eq!(record.tool, Some("uni-pseudocode".to_string()));
        assert_eq!(record.input, Some(serde_json::Value::String("Design components".to_string())));
    }

    #[test]
    fn test_parse_line_subagent_stop() {
        let json = r#"{"ts":"2024-06-15T14:30:45.000Z","hook":"SubagentStop","session_id":"sess-1","agent_type":""}"#;
        let record = parse_line(json).unwrap();
        assert_eq!(record.hook, HookType::SubagentStop);
        assert_eq!(record.tool, None);
        assert_eq!(record.input, None);
    }

    #[test]
    fn test_parse_line_malformed_json() {
        assert!(parse_line("{garbage}").is_none());
    }

    #[test]
    fn test_parse_line_unknown_hook() {
        let json = r#"{"ts":"2024-06-15T14:30:45.000Z","hook":"Unknown","session_id":"s1"}"#;
        assert!(parse_line(json).is_none());
    }

    #[test]
    fn test_parse_line_missing_session_id() {
        let json = r#"{"ts":"2024-06-15T14:30:45.000Z","hook":"PreToolUse"}"#;
        assert!(parse_line(json).is_none());
    }

    #[test]
    fn test_parse_session_file_valid() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");
        let lines = vec![
            make_pre_tool_json("2024-06-15T14:30:45.000Z", "s1", "Read"),
            make_pre_tool_json("2024-06-15T14:30:46.000Z", "s1", "Write"),
            make_pre_tool_json("2024-06-15T14:30:47.000Z", "s1", "Bash"),
        ];
        std::fs::write(&path, lines.join("\n")).unwrap();

        let records = parse_session_file(&path).unwrap();
        assert_eq!(records.len(), 3);
    }

    #[test]
    fn test_parse_session_file_mixed() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");
        let lines = vec![
            make_pre_tool_json("2024-06-15T14:30:45.000Z", "s1", "Read"),
            "garbage line".to_string(),
            make_pre_tool_json("2024-06-15T14:30:46.000Z", "s1", "Write"),
            "{bad json}".to_string(),
            make_pre_tool_json("2024-06-15T14:30:47.000Z", "s1", "Bash"),
        ];
        std::fs::write(&path, lines.join("\n")).unwrap();

        let records = parse_session_file(&path).unwrap();
        assert_eq!(records.len(), 3);
    }

    #[test]
    fn test_parse_session_file_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");
        std::fs::write(&path, "").unwrap();

        let records = parse_session_file(&path).unwrap();
        assert!(records.is_empty());
    }

    #[test]
    fn test_parse_session_file_all_malformed() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");
        std::fs::write(&path, "bad1\nbad2\nbad3\n").unwrap();

        let records = parse_session_file(&path).unwrap();
        assert!(records.is_empty());
    }

    #[test]
    fn test_parse_session_file_sorted_by_timestamp() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");
        let lines = vec![
            make_pre_tool_json("2024-06-15T14:30:47.000Z", "s1", "Bash"),
            make_pre_tool_json("2024-06-15T14:30:45.000Z", "s1", "Read"),
            make_pre_tool_json("2024-06-15T14:30:46.000Z", "s1", "Write"),
        ];
        std::fs::write(&path, lines.join("\n")).unwrap();

        let records = parse_session_file(&path).unwrap();
        assert!(records[0].ts < records[1].ts);
        assert!(records[1].ts < records[2].ts);
    }

    #[test]
    fn test_parse_session_file_large() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");
        let mut lines = Vec::new();
        for i in 0..10_000u64 {
            let secs = i / 1000;
            let millis = i % 1000;
            lines.push(make_pre_tool_json(
                &format!("2024-06-15T14:{:02}:{:02}.{:03}Z", secs / 60, secs % 60, millis),
                "s1",
                "Read",
            ));
        }
        std::fs::write(&path, lines.join("\n")).unwrap();

        let start = std::time::Instant::now();
        let records = parse_session_file(&path).unwrap();
        let elapsed = start.elapsed();
        assert_eq!(records.len(), 10_000);
        assert!(elapsed.as_secs() < 2, "parsing took {:?}, expected < 2s", elapsed);
    }

    #[test]
    fn test_parse_session_file_nonexistent() {
        let result = parse_session_file(Path::new("/nonexistent/file.jsonl"));
        assert!(result.is_err());
    }
}
