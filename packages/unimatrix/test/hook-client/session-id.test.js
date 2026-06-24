"use strict";

// #832 — deterministic CC session-id resolution. The cycle DECLARATION spawn and
// every per-tool OBSERVE spawn must compute the SAME id so Path A (tracker) and
// Path B (registry) key on one identity. These are behavioral-at-unit: they
// assert the OUTCOME (same id across spawns), not internal call counts.

const { describe, it } = require("node:test");
const assert = require("assert");

const sid = require("../../lib/hook-client/session-id");
const state = require("../../lib/hook-client/state");

// HookInput stub: only the fields the resolver reads.
function input(session_id, transcript_path) {
  return { session_id: session_id, transcript_path: transcript_path };
}

// Convenience: resolve to the id (the value both spawn types must agree on).
const resolveId = (inp) => sid.resolveSessionId(inp);

describe("resolveSessionId: source precedence", () => {
  it("test_input_session_id_wins", () => {
    const id = resolveId(input("http-472aace5", "/p/t.jsonl"));
    assert.strictEqual(id, "http-472aace5");
    assert.strictEqual(sid.sourceOf(id), "input.session_id");
  });

  it("test_transcript_path_fallback_when_session_id_absent", () => {
    const id = resolveId(input(null, "/p/t.jsonl"));
    assert.strictEqual(sid.sourceOf(id), "transcript_path");
    assert.ok(/^cc-[0-9a-f]{16}$/.test(id), "derived id is cc-<16 hex>");
  });

  it("test_ppid_last_resort_when_no_cc_field", () => {
    const id = resolveId(input(null, null));
    assert.strictEqual(sid.sourceOf(id), "ppid");
    assert.strictEqual(id, "ppid-" + process.ppid);
  });

  it("test_empty_strings_are_not_anchors", () => {
    // "" session_id falls through to transcript; "" transcript falls to ppid.
    assert.strictEqual(sid.sourceOf(resolveId(input("", "/p/t"))), "transcript_path");
    assert.strictEqual(sid.sourceOf(resolveId(input("", ""))), "ppid");
  });
});

describe("resolveSessionId: declaration vs observe convergence (#832 root cause)", () => {
  it("test_same_cc_session_id_converges_both_spawns", () => {
    // Both spawns carry the same CC input.session_id → identical id (UDS today).
    const decl = resolveId(input("http-472aace5", "/p/t.jsonl"));
    const obs = resolveId(input("http-472aace5", "/p/t.jsonl"));
    assert.strictEqual(decl, obs, "declaration and observe must share one id");
  });

  it("test_null_session_id_still_converges_via_transcript (B1)", () => {
    // The BUG was: declaration spawn lacking session_id fell to a per-spawn ppid,
    // diverging from observe. Correct-by-construction: when session_id is absent
    // on BOTH spawns but the transcript_path (same conversation) is present, both
    // derive the IDENTICAL id — NOT two different ppid- ids.
    const decl = resolveId(input(null, "/proj/.claude/transcript-abc.jsonl"));
    const obs = resolveId(input(null, "/proj/.claude/transcript-abc.jsonl"));
    assert.strictEqual(decl, obs, "no divergence, no ppid split when session_id is null");
    assert.ok(decl.startsWith("cc-"), "stable transcript-derived id");
  });

  it("test_divergent_session_ids_do_NOT_collapse", () => {
    // Two genuinely different CC conversations must NOT share an id (no silent
    // cross-cycle attribution — visible-NULL is better than silent-wrong).
    const a = resolveId(input(null, "/proj/t-a.jsonl"));
    const b = resolveId(input(null, "/proj/t-b.jsonl"));
    assert.notStrictEqual(a, b, "distinct transcripts → distinct ids");
  });
});

describe("resolveSessionId: path-safety (N5)", () => {
  it("test_resolved_id_survives_sanitize_unchanged", () => {
    // The derived cc-<hex> id is filesystem/registry safe by construction, so the
    // existing state.sanitizeSessionKey is a no-op on it (no traversal possible).
    const id = resolveId(input(null, "/../../etc/passwd"));
    assert.strictEqual(state.sanitizeSessionKey(id), id, "no sanitize rewrite");
    assert.ok(!id.includes("/") && !id.includes(".."), "no path separators / traversal");
  });
});

describe("session-id B1 trace (build-request, gated to debug)", () => {
  const buildRequest = require("../../lib/hook-client/build-request");
  function captureTrace(env, inp) {
    const writes = [];
    const orig = process.stderr.write;
    process.stderr.write = (s) => { writes.push(String(s)); return true; };
    const saved = process.env.UNIMATRIX_HOOK_DEBUG;
    try {
      if (env) process.env.UNIMATRIX_HOOK_DEBUG = env;
      else delete process.env.UNIMATRIX_HOOK_DEBUG;
      buildRequest.buildRequest("PreToolUse", inp);
    } finally {
      process.stderr.write = orig;
      if (saved === undefined) delete process.env.UNIMATRIX_HOOK_DEBUG;
      else process.env.UNIMATRIX_HOOK_DEBUG = saved;
    }
    return writes.filter((w) => w.indexOf("session-id:") !== -1);
  }

  function preToolInput(session_id, transcript_path) {
    return {
      session_id, transcript_path, provider: "claude-code",
      mcp_context: null,
      extra: { tool_name: "context_cycle", tool_input: { type: "stop", topic: "x" } },
    };
  }

  it("test_trace_silent_unless_debug_env_set", () => {
    assert.strictEqual(captureTrace(null, preToolInput("s1", null)).length, 0,
      "no trace off the hot path");
  });

  it("test_trace_emits_source_and_id_when_debug", () => {
    const lines = captureTrace("1", preToolInput(null, "/p/t.jsonl"));
    assert.strictEqual(lines.length, 1, "one structured line when debug-gated");
    assert.match(lines[0], /kind=declaration\? source=transcript_path id=cc-[0-9a-f]{16}/);
  });
});
