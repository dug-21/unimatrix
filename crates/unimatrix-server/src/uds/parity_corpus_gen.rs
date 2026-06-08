//! Parity-corpus golden generator (vnc-026, ADR-001).
//!
//! The Rust hook is the oracle. This additive dev-test generates the committed
//! golden corpus under `packages/unimatrix/test/fixtures/parity/` by running the
//! REAL private oracle functions (`parse_hook_input`, `normalize_event_name`,
//! `build_request`, the run() step-5b SubagentStart fallback, and the
//! `write_stdout` / `write_stdout_subagent_inject` expressions pinned at
//! hook.rs:963-1006) against a fixed case table, then normalizing volatile
//! fields so regeneration is byte-stable (CI drift gate: regenerate + diff = 0).
//!
//! Run explicitly via `scripts/regen-parity.sh` (never in the default pass).
//!
//! Conventions (documented in MANIFEST.json): relative `transcript_path`
//! values in `stdin.json` resolve against the case directory; volatile
//! normalization is `timestamp` → 0 (RecordEvent flatten + RecordEvents
//! elements), `^ppid-\d+$` session ids → `ppid-X`, and a `cwd` equal to the
//! running process's working directory → `"<process-cwd>"`.
//!
//! No production code is modified (C-07): this module is `#[cfg(test)]`-wired
//! from `hook.rs` exactly like `transcript_block_tests.rs`.

use super::*;

use crate::uds::transcript_block::{MAX_PRECOMPACT_BYTES, TAIL_MULTIPLIER};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

#[path = "parity_corpus_cases.rs"]
mod cases_a;
#[path = "parity_corpus_cases_b.rs"]
mod cases_b;
#[path = "parity_corpus_cases_stdout.rs"]
mod cases_stdout;
#[path = "parity_corpus_cases_tools.rs"]
mod cases_tools;
#[path = "parity_corpus_gen_tests.rs"]
mod gen_tests;
#[path = "parity_corpus_transcripts.rs"]
mod transcripts;

/// One corpus case: raw inputs plus the manifest arm keys it covers.
pub(crate) struct Case {
    pub(crate) name: &'static str,
    pub(crate) event: &'static str,
    pub(crate) arms: &'static [&'static str],
    pub(crate) stdin: String,
    pub(crate) transcript: Option<String>,
    pub(crate) response: Option<HookResponse>,
}

impl Case {
    pub(crate) fn new(
        name: &'static str,
        event: &'static str,
        arms: &'static [&'static str],
        stdin: impl Into<String>,
    ) -> Self {
        Case {
            name,
            event,
            arms,
            stdin: stdin.into(),
            transcript: None,
            response: None,
        }
    }

    pub(crate) fn with_transcript(mut self, transcript: impl Into<String>) -> Self {
        self.transcript = Some(transcript.into());
        self
    }

    pub(crate) fn with_response(mut self, response: HookResponse) -> Self {
        self.response = Some(response);
        self
    }
}

/// The full mandatory edge-case table (ADR-001 inventory).
fn all_cases() -> Vec<Case> {
    let mut cases = cases_a::cases();
    cases.extend(cases_tools::cases());
    cases.extend(cases_b::cases());
    cases.extend(cases_stdout::cases());
    cases
}

/// Every manifest arm key. A case table that leaves any key uncovered fails
/// both `test_generator_branch_coverage` and the generator itself (R-02).
fn all_arm_keys() -> Vec<&'static str> {
    let mut keys: Vec<&'static str> = Vec::new();
    keys.extend_from_slice(cases_a::ARM_KEYS_A);
    keys.extend_from_slice(cases_tools::ARM_KEYS_TOOLS);
    keys.extend_from_slice(cases_b::ARM_KEYS_B);
    keys.extend_from_slice(cases_stdout::ARM_KEYS_STDOUT);
    keys
}

// -- Oracle pipeline (mirrors hook.rs::run() steps 1-5b) --

/// Mirror of `read_stdin()`'s 1 MiB cap (`stdin().take(1_048_576).read_to_string`).
/// A cap landing mid-UTF-8-char makes `read_to_string` fail, leaving the buffer
/// empty — mirrored here by returning "".
fn cap_stdin(s: &str) -> &str {
    const CAP: usize = 1_048_576;
    if s.len() <= CAP {
        return s;
    }
    if s.is_char_boundary(CAP) {
        &s[..CAP]
    } else {
        ""
    }
}

/// Resolve a stdin `transcript_path` against the case directory (corpus
/// convention: relative paths are case-dir-relative).
fn resolve_case_path(case_dir: &Path, p: &str) -> String {
    if Path::new(p).is_absolute() {
        p.to_string()
    } else {
        case_dir.join(p).to_string_lossy().into_owned()
    }
}

/// Replica of run() step 5b: SubagentStart RecordEvent → ContextSearch via the
/// transcript tail. Identical logic; only the path is case-dir-resolved.
fn apply_subagent_fallback(
    effective_event: &str,
    request: HookRequest,
    input: &HookInput,
    case_dir: &Path,
) -> HookRequest {
    if effective_event == "SubagentStart" && matches!(request, HookRequest::RecordEvent { .. }) {
        let role = input
            .extra
            .get("agent_type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let query = input
            .transcript_path
            .as_deref()
            .filter(|p| !p.is_empty())
            .map(|p| resolve_case_path(case_dir, p))
            .and_then(|p| extract_transcript_block(&p));
        match query {
            Some(q) => HookRequest::ContextSearch {
                query: q,
                session_id: input.session_id.clone(),
                source: Some("SubagentStart".to_string()),
                role,
                task: None,
                feature: None,
                k: None,
                max_tokens: None,
                accept: None, // vnc-027 ADR-001 §6: Rust hook never sends accept
            },
            None => request,
        }
    } else {
        request
    }
}

/// Run the oracle pipeline for one case: parse → normalize → build_request →
/// SubagentStart fallback. Returns the request plus the parsed input (needed
/// for the PreCompact transcript-block step of the stdout reconstruction).
fn oracle_request(case: &Case, case_dir: &Path) -> (HookRequest, HookInput) {
    let capped = cap_stdin(&case.stdin);
    let mut input = parse_hook_input(capped);
    let (canonical, provider) = normalize_event_name(case.event);
    input.provider = Some(provider.to_string());
    let effective_event: String = if canonical == "__unknown__" {
        case.event.to_string()
    } else {
        canonical.to_string()
    };
    let request = build_request(&effective_event, &input);
    let request = apply_subagent_fallback(&effective_event, request, &input, case_dir);
    (request, input)
}

// -- Volatile-field normalization (ADR-001 comparison rules) --

fn is_ppid_session(s: &str) -> bool {
    s.strip_prefix("ppid-")
        .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
}

fn normalize_frame(obj: &mut serde_json::Map<String, serde_json::Value>, process_cwd: &str) {
    if let Some(ts) = obj.get_mut("timestamp")
        && ts.is_number()
    {
        *ts = serde_json::Value::from(0u64);
    }
    if let Some(sid) = obj.get_mut("session_id")
        && sid.as_str().is_some_and(is_ppid_session)
    {
        *sid = serde_json::Value::String("ppid-X".to_string());
    }
    if let Some(cwd) = obj.get_mut("cwd")
        && cwd.as_str() == Some(process_cwd)
    {
        *cwd = serde_json::Value::String("<process-cwd>".to_string());
    }
}

/// Normalize volatile fields in a serialized HookRequest so regeneration is
/// byte-stable: timestamps → 0, ppid-fallback session ids → "ppid-X", and a
/// process-cwd fallback value → "<process-cwd>". Applied to the top-level
/// frame (RecordEvent flatten) and to each RecordEvents element.
fn normalize_volatile(value: &mut serde_json::Value) {
    let process_cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    if let Some(obj) = value.as_object_mut() {
        normalize_frame(obj, &process_cwd);
        if let Some(events) = obj.get_mut("events").and_then(|e| e.as_array_mut()) {
            for ev in events {
                if let Some(ev_obj) = ev.as_object_mut() {
                    normalize_frame(ev_obj, &process_cwd);
                }
            }
        }
    }
}

// -- Stdout reconstruction (pinned to hook.rs:963-1028) --
//
// write_stdout writes to the real process stdout and cannot be captured
// in-process without refactor; this reconstruction uses the SAME serde_json
// serializer + the same verbatim envelope/println expressions, so the
// cross-language drift risk preserved is zero. This is the one accepted
// oracle indirection in the design (pseudocode/parity-corpus.md).

fn reconstruct_write_stdout(response: &HookResponse) -> Vec<u8> {
    match response {
        HookResponse::Entries { items, .. } => match format_injection(items, MAX_INJECTION_BYTES) {
            // println!("{text}")
            Some(text) => format!("{text}\n").into_bytes(),
            None => Vec::new(),
        },
        HookResponse::BriefingContent { content, .. } => {
            if content.is_empty() {
                Vec::new()
            } else {
                // println!("{content}")
                format!("{content}\n").into_bytes()
            }
        }
        other => {
            // let json = serde_json::to_string(response)?; println!("{json}");
            let json = serde_json::to_string(other).expect("HookResponse serializes");
            format!("{json}\n").into_bytes()
        }
    }
}

/// Mirror run()'s response routing: SubagentStart source → hookSpecificOutput
/// envelope (hook.rs:994-1006); BriefingContent on the plain path → transcript
/// prepend (hook.rs:279-290); everything else → write_stdout (hook.rs:963-985).
fn reconstruct_stdout(
    response: &HookResponse,
    req_source: Option<&str>,
    transcript_block: Option<&str>,
) -> Vec<u8> {
    if req_source == Some("SubagentStart") {
        match response {
            HookResponse::Entries { items, .. } => {
                match format_injection(items, MAX_INJECTION_BYTES) {
                    Some(text) => {
                        // Verbatim expression from write_stdout_subagent_inject
                        // (hook.rs:996-1005): serde_json::json! envelope +
                        // writeln!("{}", envelope).
                        let envelope = serde_json::json!({
                            "hookSpecificOutput": {
                                "hookEventName": "SubagentStart",
                                "additionalContext": text
                            }
                        });
                        format!("{envelope}\n").into_bytes()
                    }
                    None => Vec::new(),
                }
            }
            other => reconstruct_write_stdout(other),
        }
    } else {
        match response {
            HookResponse::BriefingContent { content, .. } => {
                let full_output = prepend_transcript(transcript_block, content);
                if full_output.is_empty() {
                    Vec::new()
                } else {
                    format!("{full_output}\n").into_bytes()
                }
            }
            other => reconstruct_write_stdout(other),
        }
    }
}

// -- Generation --

fn write_pretty_json(path: &Path, value: &serde_json::Value) {
    let pretty = serde_json::to_string_pretty(value).expect("value serializes");
    fs::write(path, format!("{pretty}\n"))
        .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

fn generate_case(case: &Case, case_dir: &Path) {
    fs::create_dir_all(case_dir)
        .unwrap_or_else(|e| panic!("create case dir {}: {e}", case_dir.display()));

    fs::write(case_dir.join("stdin.json"), case.stdin.as_bytes())
        .unwrap_or_else(|e| panic!("write stdin.json for {}: {e}", case.name));
    fs::write(case_dir.join("event.txt"), format!("{}\n", case.event))
        .unwrap_or_else(|e| panic!("write event.txt for {}: {e}", case.name));
    if let Some(transcript) = &case.transcript {
        fs::write(case_dir.join("transcript.jsonl"), transcript.as_bytes())
            .unwrap_or_else(|e| panic!("write transcript.jsonl for {}: {e}", case.name));
    }

    let (request, input) = oracle_request(case, case_dir);

    let mut request_json = serde_json::to_value(&request).expect("HookRequest serializes");
    normalize_volatile(&mut request_json);
    write_pretty_json(&case_dir.join("expected-request.json"), &request_json);

    if let Some(response) = &case.response {
        let response_json = serde_json::to_value(response).expect("HookResponse serializes");
        write_pretty_json(&case_dir.join("response.json"), &response_json);

        // run() step 5c: source extracted from the request.
        let req_source: Option<String> = match &request {
            HookRequest::ContextSearch { source, .. } => source.clone(),
            _ => None,
        };
        // run() step 5d: PreCompact transcript block (case-dir-resolved path).
        let transcript_block: Option<String> =
            if matches!(request, HookRequest::CompactPayload { .. }) {
                input
                    .transcript_path
                    .as_deref()
                    .filter(|p| !p.is_empty())
                    .map(|p| resolve_case_path(case_dir, p))
                    .and_then(|p| extract_transcript_block(&p))
            } else {
                None
            };

        let stdout_bytes =
            reconstruct_stdout(response, req_source.as_deref(), transcript_block.as_deref());
        fs::write(case_dir.join("expected-stdout.bin"), stdout_bytes)
            .unwrap_or_else(|e| panic!("write expected-stdout.bin for {}: {e}", case.name));
    }
}

/// R-02 completeness assertions: unique well-formed case names, every used arm
/// key declared, every declared arm key covered by ≥1 case.
fn assert_coverage(cases: &[Case]) {
    assert!(!cases.is_empty(), "corpus case table is empty");

    let mut seen_names: BTreeSet<&str> = BTreeSet::new();
    for case in cases {
        assert!(
            !case.name.is_empty()
                && case
                    .name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
            "case name {:?} must be non-empty kebab-case [a-z0-9-]",
            case.name
        );
        assert!(
            seen_names.insert(case.name),
            "duplicate case name {:?}",
            case.name
        );
        assert!(
            !case.arms.is_empty(),
            "case {:?} maps no arm keys",
            case.name
        );
    }

    let known: BTreeSet<&str> = all_arm_keys().into_iter().collect();
    assert_eq!(
        known.len(),
        all_arm_keys().len(),
        "duplicate entries in ARM_KEYS"
    );

    let mut covered: BTreeSet<&str> = BTreeSet::new();
    for case in cases {
        for arm in case.arms {
            assert!(
                known.contains(arm),
                "case {:?} references unknown arm key {:?}",
                case.name,
                arm
            );
            covered.insert(arm);
        }
    }
    let uncovered: Vec<&&str> = known.iter().filter(|k| !covered.contains(**k)).collect();
    assert!(
        uncovered.is_empty(),
        "arm keys without a corpus case (R-02): {uncovered:?}"
    );
}

fn build_manifest(cases: &[Case]) -> serde_json::Value {
    let mut arms: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for key in all_arm_keys() {
        arms.insert(key, Vec::new());
    }
    for case in cases {
        for arm in case.arms {
            arms.get_mut(arm).expect("arm key known").push(case.name);
        }
    }
    serde_json::json!({
        "generated_by": "parity_corpus_gen.rs",
        "case_count": cases.len(),
        "conventions": {
            "transcript_path": "relative transcript_path values in stdin.json resolve against the case directory",
            "stdin_cap_bytes": 1_048_576,
            "volatile_normalization": [
                "timestamp -> 0 (top-level RecordEvent flatten and each RecordEvents element)",
                "session_id matching ^ppid-\\d+$ -> \"ppid-X\"",
                "cwd equal to the running process current working directory -> \"<process-cwd>\""
            ]
        },
        "arms": arms,
    })
}

/// The corpus generator. `#[ignore]`d so the default test pass never writes
/// into `packages/`; CI runs it explicitly and fails (never skips) if it does
/// not execute (R-20).
#[test]
#[ignore = "corpus generator — run explicitly via CI drift job or scripts/regen-parity.sh"]
fn generate_parity_corpus() {
    let out_dir =
        PathBuf::from(std::env::var("UNIMATRIX_PARITY_DIR").expect(
            "UNIMATRIX_PARITY_DIR not set — run via CI drift job or scripts/regen-parity.sh",
        ));

    let cases = all_cases();
    assert_coverage(&cases);

    fs::create_dir_all(&out_dir)
        .unwrap_or_else(|e| panic!("create out dir {}: {e}", out_dir.display()));

    // Never half-write a corpus: stage per-case dirs, then move into place.
    let staging = out_dir.join(".parity-staging");
    if staging.exists() {
        fs::remove_dir_all(&staging).expect("clear stale staging dir");
    }
    fs::create_dir_all(&staging).expect("create staging dir");

    for case in &cases {
        generate_case(case, &staging.join(case.name));
    }

    let case_names: BTreeSet<&str> = cases.iter().map(|c| c.name).collect();
    for case in &cases {
        let dst = out_dir.join(case.name);
        if dst.exists() {
            fs::remove_dir_all(&dst)
                .unwrap_or_else(|e| panic!("replace case dir {}: {e}", dst.display()));
        }
        fs::rename(staging.join(case.name), &dst)
            .unwrap_or_else(|e| panic!("move case dir {}: {e}", dst.display()));
    }
    fs::remove_dir_all(&staging).expect("remove staging dir");

    // Prune stale case directories from renamed/removed cases. Non-directory
    // files (e.g., other committed fixtures) are left untouched.
    for entry in fs::read_dir(&out_dir).expect("read out dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !case_names.contains(name.as_str()) {
                fs::remove_dir_all(&path)
                    .unwrap_or_else(|e| panic!("prune stale case dir {}: {e}", path.display()));
            }
        }
    }

    write_pretty_json(&out_dir.join("MANIFEST.json"), &build_manifest(&cases));

    // Non-vacuity: the corpus the job just wrote must be non-trivial.
    assert!(
        cases.len() >= 60,
        "corpus unexpectedly small: {}",
        cases.len()
    );
}
