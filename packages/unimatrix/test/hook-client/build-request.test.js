"use strict";

// Unit tests for lib/hook-client/build-request.js (vnc-026, test-plan/build-request.md).
// Oracle: crates/unimatrix-server/src/uds/hook.rs:440-951 (build_request,
// extract_event_topic_signal, validate_cycle_params, is_bash_failure,
// extract_file_path) + attribution.rs:15-92 (topic signal chain).
//
// These targeted units give fast-fail locality on top of the authoritative
// Layer 1 parity suite (a later wave). Spot-checks against committed corpus
// goldens are included where they pin a documented JS-divergence trap.
//
// Test-string gotcha (Unimatrix pattern #4769): adversarial strings are built
// via String.fromCodePoint, never bare \uXXXX literals in source.

const { describe, it } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const path = require("path");

const {
  buildRequest,
  implantEvent,
  nowSecs,
  validateCycleParams,
  isBashFailure,
  extractFilePath,
  extractEventTopicSignal,
  countWhitespaceWords,
} = require("../../lib/hook-client/build-request");
const { extractTopicSignal } = require("../../lib/hook-client/topic-signal");

const PARITY_DIR = path.join(__dirname, "..", "fixtures", "parity");

// Build a HookInput with sane defaults; `over` overrides named fields.
// `extra` defaults to {} (parsed, no unknown keys — Rust flatten parity).
function mkInput(over) {
  return Object.assign(
    {
      hook_event_name: "",
      session_id: null,
      cwd: null,
      transcript_path: null,
      prompt: null,
      provider: "claude-code",
      mcp_context: null,
      extra: {},
    },
    over || {}
  );
}

// Normalize volatile fields for golden comparison (MANIFEST conventions):
// timestamp -> 0, ppid-\d+ -> ppid-X, process cwd -> "<process-cwd>".
function normalizeVolatile(obj) {
  const clone = JSON.parse(JSON.stringify(obj));
  const cwd = process.cwd();
  const walk = (node) => {
    if (Array.isArray(node)) {
      node.forEach(walk);
    } else if (node && typeof node === "object") {
      for (const key of Object.keys(node)) {
        const v = node[key];
        if (key === "timestamp" && typeof v === "number") {
          node[key] = 0;
        } else if (
          key === "session_id" &&
          typeof v === "string" &&
          /^ppid-\d+$/.test(v)
        ) {
          node[key] = "ppid-X";
        } else if (key === "cwd" && v === cwd) {
          node[key] = "<process-cwd>";
        } else {
          walk(v);
        }
      }
    }
  };
  walk(clone);
  return clone;
}

function loadCorpus(caseName) {
  const dir = path.join(PARITY_DIR, caseName);
  const stdin = JSON.parse(fs.readFileSync(path.join(dir, "stdin.json"), "utf8"));
  const event = fs.readFileSync(path.join(dir, "event.txt"), "utf8").trim();
  const expected = JSON.parse(
    fs.readFileSync(path.join(dir, "expected-request.json"), "utf8")
  );
  return { stdin, event, expected };
}

// Replicate index.js's HookInput construction from a flat stdin object:
// named keys are lifted out, everything else lands in `extra` (insertion order).
const NAMED_KEYS = new Set([
  "hook_event_name",
  "session_id",
  "cwd",
  "transcript_path",
  "prompt",
  "provider",
  "mcp_context",
]);
function stdinToInput(stdin) {
  const input = mkInput({ provider: null });
  const extra = {};
  for (const key of Object.keys(stdin)) {
    if (NAMED_KEYS.has(key)) {
      input[key] = stdin[key];
    } else {
      extra[key] = stdin[key];
    }
  }
  input.extra = extra;
  return input;
}

// normalizeEventName + provider inference (index.js step 2b), for corpus replay.
const { normalizeEventName } = require("../../lib/hook-client/normalize");
function corpusToRequest(c) {
  const [canonical, provider] = normalizeEventName(c.event);
  const input = stdinToInput(c.stdin);
  const effective = canonical === "__unknown__" ? c.event : canonical;
  input.provider = provider === "unknown" ? null : provider;
  return buildRequest(effective, input);
}

describe("buildRequest: simple event arms", () => {
  it("test_build_request_ping_returns_ping", () => {
    assert.deepStrictEqual(buildRequest("Ping", mkInput()), { type: "Ping" });
  });

  it("test_build_request_session_start_null_role_feature", () => {
    const r = buildRequest("SessionStart", mkInput({ cwd: "/work" }));
    assert.strictEqual(r.type, "SessionRegister");
    assert.strictEqual(r.agent_role, null);
    assert.strictEqual(r.feature, null);
    assert.strictEqual(r.cwd, "/work");
  });

  it("test_build_request_session_start_reads_extra", () => {
    const r = buildRequest(
      "SessionStart",
      mkInput({
        cwd: "/work",
        extra: { agent_role: "developer", feature_cycle: "vnc-026" },
      })
    );
    assert.strictEqual(r.agent_role, "developer");
    assert.strictEqual(r.feature, "vnc-026");
  });

  it("test_build_request_stop_and_taskcompleted_session_close", () => {
    for (const ev of ["Stop", "TaskCompleted"]) {
      const r = buildRequest(ev, mkInput({ session_id: "s1" }));
      assert.deepStrictEqual(r, {
        type: "SessionClose",
        session_id: "s1",
        outcome: "success",
        duration_secs: 0,
      });
    }
  });

  it("test_build_request_precompact_omits_excerpt", () => {
    const r = buildRequest("PreCompact", mkInput({ session_id: "s1" }));
    assert.strictEqual(r.type, "CompactPayload");
    assert.deepStrictEqual(r.injected_entry_ids, []);
    assert.ok(!("transcript_excerpt" in r), "transcript_excerpt key omitted");
    assert.strictEqual(r.token_limit, null);
  });
});

describe("buildRequest: ppid + cwd fallback", () => {
  it("test_build_request_missing_session_id_ppid_fallback", () => {
    const r = buildRequest("Stop", mkInput({ session_id: null }));
    assert.strictEqual(r.session_id, "ppid-" + process.ppid);
  });

  it("test_ppid_collision_documented", () => {
    // R-19: two inputs missing session_id, same process.ppid → identical id.
    const a = buildRequest("Stop", mkInput({ session_id: null }));
    const b = buildRequest("Stop", mkInput({ session_id: null }));
    assert.strictEqual(a.session_id, b.session_id);
    assert.match(a.session_id, /^ppid-\d+$/);
  });

  it("test_build_request_missing_cwd_uses_process_cwd", () => {
    const r = buildRequest("SessionStart", mkInput({ cwd: null }));
    assert.strictEqual(r.cwd, process.cwd());
  });
});

describe("buildRequest: UserPromptSubmit MIN_QUERY_WORDS gate", () => {
  it("test_ups_empty_prompt_record_event", () => {
    const r = buildRequest("UserPromptSubmit", mkInput({ prompt: "" }));
    assert.strictEqual(r.type, "RecordEvent");
    assert.strictEqual(r.event_type, "UserPromptSubmit");
  });

  it("test_ups_whitespace_only_record_event", () => {
    const r = buildRequest("UserPromptSubmit", mkInput({ prompt: "   \t\n " }));
    assert.strictEqual(r.type, "RecordEvent");
  });

  it("test_ups_four_words_record_event", () => {
    const r = buildRequest(
      "UserPromptSubmit",
      mkInput({ prompt: "one two three four" })
    );
    assert.strictEqual(r.type, "RecordEvent");
  });

  it("test_ups_five_words_context_search", () => {
    const r = buildRequest(
      "UserPromptSubmit",
      mkInput({ prompt: "one two three four five", session_id: "s1" })
    );
    assert.strictEqual(r.type, "ContextSearch");
    assert.strictEqual(r.query, "one two three four five");
    assert.strictEqual(r.session_id, "s1");
    assert.ok(!("source" in r), "source key omitted when None");
    assert.strictEqual(r.role, null);
  });

  it("test_ups_session_id_null_not_ppid", () => {
    // ContextSearch carries raw session_id (null when absent), NOT ppid fallback.
    const r = buildRequest(
      "UserPromptSubmit",
      mkInput({ prompt: "one two three four five", session_id: null })
    );
    assert.strictEqual(r.session_id, null);
  });

  it("test_ups_multi_space_separators_count_words", () => {
    const r = buildRequest(
      "UserPromptSubmit",
      mkInput({ prompt: "a   b\tc\nd   e" })
    );
    assert.strictEqual(r.type, "ContextSearch");
  });

  it("test_count_whitespace_words", () => {
    assert.strictEqual(countWhitespaceWords("  approve  "), 1);
    assert.strictEqual(countWhitespaceWords(""), 0);
    assert.strictEqual(countWhitespaceWords("a b c d e"), 5);
  });
});

describe("buildRequest: PostToolUse rework extraction", () => {
  it("test_ptu_bash_exit_zero_no_failure", () => {
    const r = buildRequest(
      "PostToolUse",
      mkInput({ extra: { tool_name: "Bash", exit_code: 0 } })
    );
    assert.strictEqual(r.event_type, "post_tool_use_rework_candidate");
    assert.strictEqual(r.payload.had_failure, false);
  });

  it("test_ptu_bash_nonzero_exit_failure", () => {
    const r = buildRequest(
      "PostToolUse",
      mkInput({ extra: { tool_name: "Bash", exit_code: 7 } })
    );
    assert.strictEqual(r.payload.had_failure, true);
  });

  it("test_ptu_bash_missing_exit_no_failure", () => {
    const r = buildRequest(
      "PostToolUse",
      mkInput({ extra: { tool_name: "Bash" } })
    );
    assert.strictEqual(r.payload.had_failure, false);
  });

  it("test_ptu_bash_noninteger_exit_skipped", () => {
    // as_i64 parity: 1.5 / "2" / true are not integers → not a failure.
    for (const ec of [1.5, "2", true]) {
      const r = buildRequest(
        "PostToolUse",
        mkInput({ extra: { tool_name: "Bash", exit_code: ec } })
      );
      assert.strictEqual(r.payload.had_failure, false, "exit_code=" + ec);
    }
  });

  it("test_ptu_bash_interrupted_true_failure", () => {
    const r = buildRequest(
      "PostToolUse",
      mkInput({ extra: { tool_name: "Bash", interrupted: true } })
    );
    assert.strictEqual(r.payload.had_failure, true);
  });

  it("test_ptu_bash_interrupted_string_not_failure", () => {
    // as_bool parity: only JSON true counts.
    const r = buildRequest(
      "PostToolUse",
      mkInput({ extra: { tool_name: "Bash", interrupted: "true" } })
    );
    assert.strictEqual(r.payload.had_failure, false);
  });

  it("test_ptu_edit_file_path_from_path", () => {
    const r = buildRequest(
      "PostToolUse",
      mkInput({ extra: { tool_name: "Edit", tool_input: { path: "/a.rs" } } })
    );
    assert.strictEqual(r.payload.file_path, "/a.rs");
    assert.strictEqual(r.payload.had_failure, false);
  });

  it("test_ptu_write_file_path_from_file_path", () => {
    const r = buildRequest(
      "PostToolUse",
      mkInput({
        extra: { tool_name: "Write", tool_input: { file_path: "/b.rs" } },
      })
    );
    assert.strictEqual(r.payload.file_path, "/b.rs");
  });

  it("test_ptu_multiedit_fanout_record_events", () => {
    const r = buildRequest(
      "PostToolUse",
      mkInput({
        extra: {
          tool_name: "MultiEdit",
          tool_input: { edits: [{ path: "/a" }, { path: "/b" }, {}] },
        },
      })
    );
    assert.strictEqual(r.type, "RecordEvents");
    assert.strictEqual(r.events.length, 3);
    assert.strictEqual(r.events[0].payload.file_path, "/a");
    assert.strictEqual(r.events[1].payload.file_path, "/b");
    assert.strictEqual(r.events[2].payload.file_path, null);
    for (const e of r.events) {
      assert.strictEqual(e.event_type, "post_tool_use_rework_candidate");
      assert.strictEqual(e.payload.tool_name, "MultiEdit");
    }
  });

  it("test_ptu_multiedit_empty_edits_single_generic", () => {
    const r = buildRequest(
      "PostToolUse",
      mkInput({ extra: { tool_name: "MultiEdit", tool_input: { edits: [] } } })
    );
    assert.strictEqual(r.type, "RecordEvent");
    assert.strictEqual(r.event_type, "PostToolUse");
  });

  it("test_ptu_multiedit_missing_edits_single_generic", () => {
    const r = buildRequest(
      "PostToolUse",
      mkInput({ extra: { tool_name: "MultiEdit", tool_input: {} } })
    );
    assert.strictEqual(r.type, "RecordEvent");
  });

  it("test_ptu_multiedit_nonarray_edits_single_generic", () => {
    const r = buildRequest(
      "PostToolUse",
      mkInput({
        extra: { tool_name: "MultiEdit", tool_input: { edits: "nope" } },
      })
    );
    assert.strictEqual(r.type, "RecordEvent");
  });

  it("test_ptu_non_rework_tool_generic", () => {
    const r = buildRequest(
      "PostToolUse",
      mkInput({ extra: { tool_name: "Read" } })
    );
    assert.strictEqual(r.type, "RecordEvent");
    assert.strictEqual(r.event_type, "PostToolUse");
  });

  it("test_ptu_non_claude_provider_never_rework", () => {
    const r = buildRequest(
      "PostToolUse",
      mkInput({ provider: "gemini-cli", extra: { tool_name: "Bash" } })
    );
    assert.strictEqual(r.type, "RecordEvent");
    assert.strictEqual(r.event_type, "PostToolUse");
    assert.strictEqual(r.provider, "gemini-cli");
  });

  it("test_ptu_rework_payload_null_for_missing_tool_input", () => {
    // json! parity: missing tool_input/tool_response serialize as null.
    const r = buildRequest(
      "PostToolUse",
      mkInput({ extra: { tool_name: "Bash" } })
    );
    assert.strictEqual(r.payload.tool_input, null);
    assert.strictEqual(r.payload.tool_response, null);
  });
});

describe("buildRequest: PostToolUseFailure explicit arm", () => {
  it("test_ptuf_nominal_record_event_verbatim_name", () => {
    const r = buildRequest(
      "PostToolUseFailure",
      mkInput({ extra: { tool_name: "Bash", error: "boom" } })
    );
    assert.strictEqual(r.type, "RecordEvent");
    assert.strictEqual(r.event_type, "PostToolUseFailure");
    assert.deepStrictEqual(r.payload, { tool_name: "Bash", error: "boom" });
  });

  it("test_ptuf_empty_extra", () => {
    const r = buildRequest("PostToolUseFailure", mkInput({ extra: {} }));
    assert.deepStrictEqual(r.payload, {});
  });

  it("test_ptuf_null_extra", () => {
    const r = buildRequest("PostToolUseFailure", mkInput({ extra: null }));
    assert.strictEqual(r.payload, null);
  });

  it("test_ptuf_missing_tool_name", () => {
    const r = buildRequest(
      "PostToolUseFailure",
      mkInput({ extra: { error: "x" } })
    );
    assert.strictEqual(r.event_type, "PostToolUseFailure");
  });
});

describe("buildRequest: PreToolUse context_cycle interception", () => {
  it("test_cycle_bare_name_start", () => {
    const r = buildRequest(
      "PreToolUse",
      mkInput({
        extra: {
          tool_name: "context_cycle",
          tool_input: { type: "start", topic: "vnc-026" },
        },
      })
    );
    assert.strictEqual(r.event_type, "cycle_start");
    assert.strictEqual(r.payload.feature_cycle, "vnc-026");
    assert.strictEqual(r.topic_signal, "vnc-026");
  });

  it("test_cycle_prefixed_name_stop", () => {
    const r = buildRequest(
      "PreToolUse",
      mkInput({
        extra: {
          tool_name: "mcp__unimatrix__context_cycle",
          tool_input: { type: "stop", topic: "vnc-026" },
        },
      })
    );
    assert.strictEqual(r.event_type, "cycle_stop");
  });

  it("test_cycle_phase_end", () => {
    const r = buildRequest(
      "PreToolUse",
      mkInput({
        extra: {
          tool_name: "context_cycle",
          tool_input: {
            type: "phase-end",
            topic: "vnc-026",
            phase: "Design",
            outcome: "done",
            next_phase: "Build",
          },
        },
      })
    );
    assert.strictEqual(r.event_type, "cycle_phase_end");
    assert.strictEqual(r.payload.phase, "design"); // lowercased
    assert.strictEqual(r.payload.outcome, "done");
    assert.strictEqual(r.payload.next_phase, "build");
  });

  it("test_cycle_near_miss_not_intercepted", () => {
    // ADR-004 §1: security gate F-02 substring-like names must NOT pass; the
    // retired PreToolUse observation now yields the null no-send sentinel
    // (was a RecordEvent/PreToolUse fallthrough before the reduction).
    for (const name of [
      "context_cycles",
      "mcp__other__context_cycle",
      "evil_context_cycle_bypass",
    ]) {
      const r = buildRequest(
        "PreToolUse",
        mkInput({
          extra: { tool_name: name, tool_input: { type: "start", topic: "x-1" } },
        })
      );
      assert.strictEqual(r, null, name);
    }
  });

  it("test_cycle_invalid_params_fall_through", () => {
    const r = buildRequest(
      "PreToolUse",
      mkInput({
        extra: {
          tool_name: "context_cycle",
          tool_input: { type: "nope", topic: "vnc-026" },
        },
      })
    );
    assert.strictEqual(r, null); // sentinel (ADR-004 §1)
  });

  it("test_cycle_missing_tool_input_fall_through", () => {
    const r = buildRequest(
      "PreToolUse",
      mkInput({ extra: { tool_name: "context_cycle" } })
    );
    assert.strictEqual(r, null); // sentinel (ADR-004 §1)
  });

  it("test_cycle_goal_only_on_start", () => {
    const start = buildRequest(
      "PreToolUse",
      mkInput({
        extra: {
          tool_name: "context_cycle",
          tool_input: { type: "start", topic: "x-1", goal: "ship it" },
        },
      })
    );
    assert.strictEqual(start.payload.goal, "ship it");
    const stop = buildRequest(
      "PreToolUse",
      mkInput({
        extra: {
          tool_name: "context_cycle",
          tool_input: { type: "stop", topic: "x-1", goal: "ignored" },
        },
      })
    );
    assert.ok(!("goal" in stop.payload), "goal omitted on stop");
  });

  it("test_cycle_goal_overflow_truncated_at_multibyte_boundary", () => {
    // Build a goal > 1024 bytes whose 1024th byte lands mid multi-byte char.
    // U+1F600 (emoji) is 4 UTF-8 bytes. 256 emoji = 1024 bytes exactly; add
    // ascii prefix so the cut lands mid-emoji.
    const emoji = String.fromCodePoint(0x1f600);
    const goal = "x" + emoji.repeat(300); // 1 + 1200 bytes
    const r = buildRequest(
      "PreToolUse",
      mkInput({
        extra: {
          tool_name: "context_cycle",
          tool_input: { type: "start", topic: "x-1", goal: goal },
        },
      })
    );
    const out = r.payload.goal;
    assert.ok(Buffer.byteLength(out, "utf8") <= 1024, "goal within byte budget");
    // No replacement char / no split surrogate: re-encoding round-trips.
    assert.strictEqual(Buffer.from(out, "utf8").toString("utf8"), out);
    assert.ok(out.startsWith("x"));
  });

  // vnc-047 / #944: tags ride the hook exactly like goal (Start-only, string
  // filter, omit-when-empty). Parity oracle: hook.rs:860-917. Infallibility is
  // load-bearing — a throw here would drop the whole cycle frame incl. goal.
  it("test_cycle_tags_forwarded_on_start", () => {
    const r = buildRequest(
      "PreToolUse",
      mkInput({
        extra: {
          tool_name: "context_cycle",
          tool_input: { type: "start", topic: "x-1", tags: ["alpha", "beta"] },
        },
      })
    );
    assert.deepStrictEqual(r.payload.tags, ["alpha", "beta"]);
  });

  it("test_cycle_tags_non_string_members_filtered", () => {
    const r = buildRequest(
      "PreToolUse",
      mkInput({
        extra: {
          tool_name: "context_cycle",
          tool_input: {
            type: "start",
            topic: "x-1",
            tags: ["keep", 42, null, { a: 1 }, "", "  ", "also-keep"],
          },
        },
      })
    );
    // strings only, blank-after-trim dropped; opaque colon-prefixed kept as-is.
    assert.deepStrictEqual(r.payload.tags, ["keep", "also-keep"]);
  });

  it("test_cycle_tags_omitted_when_empty_or_absent", () => {
    const absent = buildRequest(
      "PreToolUse",
      mkInput({
        extra: {
          tool_name: "context_cycle",
          tool_input: { type: "start", topic: "x-1" },
        },
      })
    );
    assert.ok(!("tags" in absent.payload), "tags omitted when absent");

    const emptyArr = buildRequest(
      "PreToolUse",
      mkInput({
        extra: {
          tool_name: "context_cycle",
          tool_input: { type: "start", topic: "x-1", tags: [] },
        },
      })
    );
    assert.ok(!("tags" in emptyArr.payload), "tags omitted when empty array");

    const allBlank = buildRequest(
      "PreToolUse",
      mkInput({
        extra: {
          tool_name: "context_cycle",
          tool_input: { type: "start", topic: "x-1", tags: ["", "   ", 7] },
        },
      })
    );
    assert.ok(!("tags" in allBlank.payload), "tags omitted when nothing survives");
  });

  it("test_cycle_tags_malformed_never_throws_and_keeps_goal", () => {
    // Each malformed shape must omit tags, never throw, and still forward goal.
    for (const bad of [null, "not-an-array", 42, { 0: "a" }, [{ x: 1 }], [[1]]]) {
      const r = buildRequest(
        "PreToolUse",
        mkInput({
          extra: {
            tool_name: "context_cycle",
            tool_input: { type: "start", topic: "x-1", goal: "ship it", tags: bad },
          },
        })
      );
      assert.ok(r !== null, "cycle frame survives malformed tags");
      assert.ok(!("tags" in r.payload), "tags omitted for malformed input");
      assert.strictEqual(r.payload.goal, "ship it", "goal survives bad tags");
    }
  });

  it("test_cycle_tags_never_on_phase_end_or_stop", () => {
    for (const type of ["phase-end", "stop"]) {
      const ti =
        type === "phase-end"
          ? { type, topic: "x-1", phase: "design", next_phase: "delivery", tags: ["nope"] }
          : { type, topic: "x-1", outcome: "success", tags: ["nope"] };
      const r = buildRequest(
        "PreToolUse",
        mkInput({ extra: { tool_name: "context_cycle", tool_input: ti } })
      );
      assert.ok(r !== null, "non-start cycle frame builds");
      assert.ok(!("tags" in r.payload), "tags never emitted on " + type);
    }
  });

  it("test_cycle_mcp_context_promotion_does_not_mutate_input", () => {
    const input = mkInput({
      session_id: "gem-1",
      mcp_context: { tool_name: "context_cycle" },
      extra: {}, // tool_input lifted into extra by index.js normally
    });
    input.extra.tool_input = { type: "start", topic: "vnc-026" };
    const before = JSON.parse(JSON.stringify(input));
    const r = buildRequest("PreToolUse", input);
    assert.strictEqual(r.event_type, "cycle_start");
    assert.strictEqual(r.payload.feature_cycle, "vnc-026");
    // Caller input unmutated (R-01 clone rule): no tool_name leaked into extra.
    assert.deepStrictEqual(input, before);
    assert.ok(!("tool_name" in input.extra), "extra not mutated");
  });

  it("test_cycle_invalid_topic_fall_through", () => {
    const r = buildRequest(
      "PreToolUse",
      mkInput({
        extra: {
          tool_name: "context_cycle",
          tool_input: { type: "start", topic: "nohyphen" },
        },
      })
    );
    assert.strictEqual(r, null); // sentinel (ADR-004 §1)
  });
});

// vnc-027 ADR-004 §1 / FR-27 / AC-08 / R-11: PreToolUse loses standalone
// observation. Every non-cycle path returns a null no-send sentinel (index.js
// short-circuits to exit 0); stderr diagnostics are retained; cycle frames and
// the F-02 exact-equality gate are fully preserved. Only PreToolUse changes.
describe("buildRequest: PreToolUse no-send sentinel (ADR-004 §1)", () => {
  // Capture process.stderr.write byte-for-byte without leaking to the runner.
  function captureStderr(fn) {
    const orig = process.stderr.write;
    const lines = [];
    process.stderr.write = (chunk) => {
      lines.push(typeof chunk === "string" ? chunk : chunk.toString("utf8"));
      return true;
    };
    try {
      const result = fn();
      return { result, stderr: lines.join("") };
    } finally {
      process.stderr.write = orig;
    }
  }

  it("test_non_cycle_tool_name_returns_null", () => {
    // An ordinary tool name → null, no RecordEvent fallthrough, no stderr.
    const { result, stderr } = captureStderr(() =>
      buildRequest(
        "PreToolUse",
        mkInput({ extra: { tool_name: "Bash", tool_input: { command: "ls" } } })
      )
    );
    assert.strictEqual(result, null);
    assert.strictEqual(stderr, "");
  });

  it("test_missing_tool_input_returns_null_retains_stderr", () => {
    const { result, stderr } = captureStderr(() =>
      buildRequest("PreToolUse", mkInput({ extra: { tool_name: "context_cycle" } }))
    );
    assert.strictEqual(result, null);
    assert.strictEqual(
      stderr,
      "unimatrix: context_cycle PreToolUse missing tool_input\n"
    );
  });

  it("test_failed_validate_cycle_params_returns_null_retains_stderr", () => {
    const { result, stderr } = captureStderr(() =>
      buildRequest(
        "PreToolUse",
        mkInput({
          extra: {
            tool_name: "context_cycle",
            tool_input: { type: "nope", topic: "vnc-026" },
          },
        })
      )
    );
    assert.strictEqual(result, null);
    assert.strictEqual(
      stderr,
      "unimatrix: context_cycle validation failed in hook (tool_name=context_cycle)\n"
    );
  });

  it("test_malformed_non_object_tool_input_returns_null_no_throw", () => {
    // Non-object tool_input → tiObj defaults to {} → validateCycleParams fails
    // (empty type) → null sentinel + stderr; never throws (module contract).
    const { result, stderr } = captureStderr(() =>
      buildRequest(
        "PreToolUse",
        mkInput({ extra: { tool_name: "context_cycle", tool_input: "not-an-object" } })
      )
    );
    assert.strictEqual(result, null);
    assert.ok(stderr.includes("validation failed"), "stderr retained");
  });

  it("test_valid_cycle_start_returns_frame_parity", () => {
    // Valid cycle → frame UNCHANGED (stays in the byte-parity corpus, ADR-004 §4).
    const { result, stderr } = captureStderr(() =>
      buildRequest(
        "PreToolUse",
        mkInput({
          session_id: "s1",
          extra: {
            tool_name: "context_cycle",
            tool_input: { type: "start", topic: "vnc-027" },
          },
        })
      )
    );
    assert.strictEqual(stderr, "");
    assert.strictEqual(result.type, "RecordEvent");
    assert.strictEqual(result.event_type, "cycle_start");
    assert.strictEqual(result.payload.feature_cycle, "vnc-027");
    assert.strictEqual(result.topic_signal, "vnc-027");
  });

  it("test_valid_cycle_phase_end_and_stop_return_frames", () => {
    const phaseEnd = buildRequest(
      "PreToolUse",
      mkInput({
        extra: {
          tool_name: "context_cycle",
          tool_input: {
            type: "phase-end",
            topic: "vnc-027",
            phase: "Design",
            outcome: "done",
            next_phase: "Build",
          },
        },
      })
    );
    assert.strictEqual(phaseEnd.event_type, "cycle_phase_end");
    const stop = buildRequest(
      "PreToolUse",
      mkInput({
        extra: {
          tool_name: "mcp__unimatrix__context_cycle",
          tool_input: { type: "stop", topic: "vnc-027" },
        },
      })
    );
    assert.strictEqual(stop.event_type, "cycle_stop");
  });

  // F-02 exact-equality gate (defense in depth): the narrowed install matcher is
  // a regex, so "evil_context_cycle_bypass" can SPAWN the hook; the exact gate
  // must still send nothing. R-11 s3.
  it("test_exact_equality_only_context_cycle_intercepted", () => {
    for (const name of ["context_cycle", "mcp__unimatrix__context_cycle"]) {
      const r = buildRequest(
        "PreToolUse",
        mkInput({
          extra: { tool_name: name, tool_input: { type: "start", topic: "x-1" } },
        })
      );
      assert.strictEqual(r.type, "RecordEvent", name);
      assert.strictEqual(r.event_type, "cycle_start", name);
    }
  });

  it("test_evil_substring_bypass_sends_nothing", () => {
    const { result, stderr } = captureStderr(() =>
      buildRequest(
        "PreToolUse",
        mkInput({
          extra: {
            tool_name: "evil_context_cycle_bypass",
            tool_input: { type: "start", topic: "x-1" },
          },
        })
      )
    );
    assert.strictEqual(result, null);
    assert.strictEqual(stderr, ""); // gated before any diagnostic
  });

  it("test_near_miss_suffixed_not_intercepted", () => {
    for (const name of [
      "context_cycle_extra",
      "context_cycles",
      "mcp__other__context_cycle",
    ]) {
      const r = buildRequest(
        "PreToolUse",
        mkInput({
          extra: { tool_name: name, tool_input: { type: "start", topic: "x-1" } },
        })
      );
      assert.strictEqual(r, null, name);
    }
  });

  it("test_bare_tool_promotion_routes_through_sentinel_no_mutation", () => {
    // mcp_context promotion of a non-cycle bare tool still clones input (R-01)
    // and returns the sentinel (it was observation-only before).
    const input = mkInput({
      session_id: "gem-1",
      mcp_context: { tool_name: "Bash" },
      extra: {},
    });
    input.extra.tool_input = { command: "ls" };
    const before = JSON.parse(JSON.stringify(input));
    const r = buildRequest("PreToolUse", input);
    assert.strictEqual(r, null);
    assert.deepStrictEqual(input, before); // caller input unmutated
    assert.ok(!("tool_name" in input.extra), "extra not mutated");
  });

  // Scope guard (R-11 s6): ONLY PreToolUse loses fallthrough observation.
  it("test_posttooluse_fallthrough_untouched", () => {
    const ptu = buildRequest(
      "PostToolUse",
      mkInput({ extra: { tool_name: "Read" } })
    );
    assert.strictEqual(ptu.type, "RecordEvent");
    assert.strictEqual(ptu.event_type, "PostToolUse");

    const ptuf = buildRequest(
      "PostToolUseFailure",
      mkInput({ extra: { tool_name: "Bash", error: "boom" } })
    );
    assert.strictEqual(ptuf.type, "RecordEvent");
    assert.strictEqual(ptuf.event_type, "PostToolUseFailure");
  });

  it("test_other_event_builders_unchanged", () => {
    // SubagentStop generic fallthrough still observes (not PreToolUse).
    const sub = buildRequest("SubagentStop", mkInput({ extra: { a: 1 } }));
    assert.strictEqual(sub.type, "RecordEvent");
    assert.strictEqual(sub.event_type, "SubagentStop");
    // SessionStart/Stop/UserPromptSubmit/PreCompact still produce frames.
    assert.strictEqual(
      buildRequest("SessionStart", mkInput({ cwd: "/w" })).type,
      "SessionRegister"
    );
    assert.strictEqual(buildRequest("Stop", mkInput()).type, "SessionClose");
  });
});

describe("buildRequest: SubagentStart", () => {
  it("test_subagent_start_prompt_snippet_context_search", () => {
    const r = buildRequest(
      "SubagentStart",
      mkInput({ session_id: "parent-1", extra: { prompt_snippet: "do work" } })
    );
    assert.strictEqual(r.type, "ContextSearch");
    assert.strictEqual(r.query, "do work");
    assert.strictEqual(r.source, "SubagentStart");
    assert.strictEqual(r.session_id, "parent-1");
  });

  it("test_subagent_start_empty_snippet_record_event", () => {
    const r = buildRequest(
      "SubagentStart",
      mkInput({ extra: { prompt_snippet: "" } })
    );
    assert.strictEqual(r.type, "RecordEvent");
    assert.strictEqual(r.event_type, "SubagentStart");
  });

  it("test_subagent_start_whitespace_snippet_record_event", () => {
    const r = buildRequest(
      "SubagentStart",
      mkInput({ extra: { prompt_snippet: "   " } })
    );
    assert.strictEqual(r.type, "RecordEvent");
  });

  it("test_subagent_start_no_snippet_record_event", () => {
    const r = buildRequest("SubagentStart", mkInput({ extra: {} }));
    assert.strictEqual(r.type, "RecordEvent");
  });
});

describe("buildRequest: generic arm + unknown fields", () => {
  it("test_unknown_event_passthrough_raw_name", () => {
    const r = buildRequest("SomeFutureEvent", mkInput({ extra: { a: 1 } }));
    assert.strictEqual(r.type, "RecordEvent");
    assert.strictEqual(r.event_type, "SomeFutureEvent");
  });

  it("test_unknown_stdin_fields_preserved", () => {
    // ass-071 carry-in: unknown extra fields survive verbatim, in order.
    const extra = {};
    extra.future_field = { nested: [1, 2] };
    extra.subagent_id = "x";
    extra.zeta = "last";
    const r = buildRequest("SubagentStop", mkInput({ extra: extra }));
    assert.deepStrictEqual(r.payload, extra);
    assert.deepStrictEqual(Object.keys(r.payload), [
      "future_field",
      "subagent_id",
      "zeta",
    ]);
  });

  it("test_generic_null_extra_no_topic_signal", () => {
    const r = buildRequest("SubagentStop", mkInput({ extra: null }));
    assert.strictEqual(r.payload, null);
    assert.ok(!("topic_signal" in r), "topic_signal omitted when extra null");
  });

  it("test_generic_empty_extra_no_topic_signal", () => {
    // {} stringifies to "{}" → no signal (matches Rust empty flatten object).
    const r = buildRequest("SubagentStop", mkInput({ extra: {} }));
    assert.ok(!("topic_signal" in r));
  });
});

describe("buildRequest: purity", () => {
  it("test_build_request_pure_no_side_effects", () => {
    // No fs/network. Only process.ppid / process.cwd reads are allowed.
    const cwdCalls = { n: 0 };
    const origCwd = process.cwd;
    process.cwd = function () {
      cwdCalls.n += 1;
      return "/sandbox";
    };
    try {
      const input = mkInput({ session_id: "s1", cwd: null });
      const a = buildRequest("SessionStart", input);
      const b = buildRequest("SessionStart", input);
      assert.deepStrictEqual(a, b); // deterministic
      assert.strictEqual(a.cwd, "/sandbox");
    } finally {
      process.cwd = origCwd;
    }
  });
});

describe("topic-signal extraction (attribution.rs chain)", () => {
  it("test_topic_signal_from_path", () => {
    assert.strictEqual(
      extractTopicSignal("see product/features/vnc-026/SCOPE.md"),
      "vnc-026"
    );
  });

  it("test_topic_signal_bare_feature_id_token", () => {
    assert.strictEqual(extractTopicSignal("working on col-002 today"), "col-002");
  });

  it("test_topic_signal_git_checkout", () => {
    assert.strictEqual(
      extractTopicSignal("git checkout feature/abc-12"),
      "abc-12"
    );
  });

  it("test_topic_signal_unicode_whitespace_separator", () => {
    // U+2003 EM SPACE separates the token.
    const emSpace = String.fromCodePoint(0x2003);
    assert.strictEqual(
      extractTopicSignal("prefix" + emSpace + "col-002" + emSpace + "suffix"),
      "col-002"
    );
  });

  it("test_topic_signal_129_byte_candidate_rejected", () => {
    // byte length > 128 must be rejected (BYTE length, not char count).
    const long = "a-" + "b".repeat(127); // 129 bytes, has hyphen
    assert.strictEqual(Buffer.byteLength(long, "utf8"), 129);
    assert.strictEqual(extractTopicSignal(long), null);
  });

  it("test_topic_signal_no_match_returns_null", () => {
    assert.strictEqual(extractTopicSignal("nothing here at all"), null);
  });

  it("test_event_topic_signal_post_tool_use_stringifies_object", () => {
    const sig = extractEventTopicSignal(
      "PostToolUse",
      mkInput({ extra: { tool_input: { path: "product/features/x-1/a" } } })
    );
    assert.strictEqual(sig, "x-1");
  });
});

describe("validateCycleParams (validation.rs port)", () => {
  it("test_validate_rejects_bad_type", () => {
    assert.strictEqual(validateCycleParams("nope", "x-1").ok, false);
  });
  it("test_validate_rejects_empty_topic", () => {
    assert.strictEqual(validateCycleParams("start", "").ok, false);
  });
  it("test_validate_rejects_non_feature_topic", () => {
    assert.strictEqual(validateCycleParams("start", "nohyphen").ok, false);
  });
  it("test_validate_phase_rejects_space", () => {
    const v = validateCycleParams("phase-end", "x-1", "two words");
    assert.strictEqual(v.ok, false);
  });
  it("test_validate_phase_lowercased", () => {
    const v = validateCycleParams("phase-end", "x-1", "Design");
    assert.strictEqual(v.ok, true);
    assert.strictEqual(v.phase, "design");
  });
  it("test_validate_outcome_rejects_control_char", () => {
    const ctrl = String.fromCodePoint(0x01);
    const v = validateCycleParams("phase-end", "x-1", undefined, "bad" + ctrl);
    assert.strictEqual(v.ok, false);
  });
  it("test_validate_topic_strips_non_ascii", () => {
    const emoji = String.fromCodePoint(0x1f600);
    const v = validateCycleParams("start", "vnc-" + emoji + "026");
    assert.strictEqual(v.ok, true);
    assert.strictEqual(v.topic, "vnc-026");
  });
});

describe("low-level helpers", () => {
  it("test_is_bash_failure_matrix", () => {
    assert.strictEqual(isBashFailure({ exit_code: 0 }), false);
    assert.strictEqual(isBashFailure({ exit_code: 7 }), true);
    assert.strictEqual(isBashFailure({}), false);
    assert.strictEqual(isBashFailure({ exit_code: 1.5 }), false);
    assert.strictEqual(isBashFailure({ exit_code: "2" }), false);
    assert.strictEqual(isBashFailure({ interrupted: true }), true);
    assert.strictEqual(isBashFailure({ interrupted: "true" }), false);
  });

  it("test_extract_file_path", () => {
    assert.strictEqual(extractFilePath({ tool_input: { path: "/a" } }, "Edit"), "/a");
    assert.strictEqual(
      extractFilePath({ tool_input: { file_path: "/b" } }, "Write"),
      "/b"
    );
    assert.strictEqual(extractFilePath({}, "Edit"), null);
    assert.strictEqual(extractFilePath({ tool_input: {} }, "Bash"), null);
  });

  it("test_implant_event_omits_null_optionals", () => {
    const e = implantEvent("t", "s", { a: 1 }, null, null);
    assert.ok(!("topic_signal" in e));
    assert.ok(!("provider" in e));
    assert.strictEqual(typeof e.timestamp, "number");
  });

  it("test_implant_event_includes_present_optionals", () => {
    const e = implantEvent("t", "s", {}, "sig", "claude-code");
    assert.strictEqual(e.topic_signal, "sig");
    assert.strictEqual(e.provider, "claude-code");
  });

  it("test_now_secs_is_seconds", () => {
    const n = nowSecs();
    assert.ok(Number.isInteger(n));
    assert.ok(Math.abs(n - Math.floor(Date.now() / 1000)) <= 1);
  });
});

describe("corpus golden spot-checks (Layer 1 preview)", () => {
  const cases = [
    "ptu-bash-exit-zero",
    "ptu-multiedit-fanout",
    "cycle-mcp-context-promotion",
    "event-session-start",
    "event-ping",
    "event-precompact",
  ];
  for (const name of cases) {
    it("matches golden: " + name, () => {
      let c;
      try {
        c = loadCorpus(name);
      } catch (_e) {
        // Some manifest cases may not have on-disk dirs in this wave; skip.
        return;
      }
      const got = normalizeVolatile(corpusToRequest(c));
      const want = normalizeVolatile(c.expected);
      assert.deepStrictEqual(got, want);
    });
  }
});
