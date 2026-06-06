"use strict";

// Unit tests for lib/hook-client/transcript.js — JSONL tail-parse.
// Oracle semantics: crates/unimatrix-server/src/uds/transcript_block.rs.
// Corpus-backed goldens live in the Layer 1 parity suite; these tests pin
// the oracle behaviors locally (window mechanics, budget loop, pairing,
// byte-safe truncation). Test plan: product/features/vnc-026/test-plan/transcript.md

const { describe, it } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");

const {
  MAX_PRECOMPACT_BYTES,
  TAIL_MULTIPLIER,
  extractTranscriptBlock,
  truncateUtf8,
} = require("../../lib/hook-client/transcript");

const WINDOW = MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER; // 12,000

// ── Fixture helpers ─────────────────────────────────────────────────

function tempFile(content) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-transcript-"));
  const fp = path.join(dir, "transcript.jsonl");
  fs.writeFileSync(fp, content);
  return fp;
}

/** Claude Code UX-format user record line (newline-terminated). */
function userLine(text) {
  return (
    JSON.stringify({ type: "user", message: { content: [{ type: "text", text }] } }) + "\n"
  );
}

/** Assistant record line with arbitrary content blocks. */
function asstLine(blocks) {
  return JSON.stringify({ type: "assistant", message: { content: blocks } }) + "\n";
}

/** User record carrying tool_result blocks (no text blocks). */
function toolResultLine(blocks) {
  return JSON.stringify({ type: "user", message: { content: blocks } }) + "\n";
}

/**
 * Unknown-type filler line of EXACTLY totalBytes (ASCII), parsed then
 * skipped by buildExchangePairs — pads files to precise byte counts
 * without consuming the 3000-byte block budget.
 */
function fillerLine(totalBytes) {
  const base = '{"type":"zz","p":""}\n';
  assert.ok(totalBytes >= base.length, "filler too small: " + totalBytes);
  return '{"type":"zz","p":"' + "p".repeat(totalBytes - base.length) + '"}\n';
}

// ── truncateUtf8 (byte-boundary-safe) ───────────────────────────────

describe("truncateUtf8", function () {
  it("test_truncate_at_exact_boundary", function () {
    assert.strictEqual(truncateUtf8("abc", 3), "abc"); // identity (<=)
    assert.strictEqual(truncateUtf8("abcd", 3), "abc"); // limit between chars
    assert.strictEqual(truncateUtf8("é€", 2), "é"); // boundary after 2-byte char
  });

  it("test_truncate_mid_2byte", function () {
    // "aé" = 0x61 0xC3 0xA9; limit 2 lands inside é → back off to "a"
    assert.strictEqual(truncateUtf8("aé", 2), "a");
  });

  it("test_truncate_mid_3byte", function () {
    // "a€" = 0x61 + 3 bytes; limits 2 and 3 land inside € → "a"
    assert.strictEqual(truncateUtf8("a€", 2), "a");
    assert.strictEqual(truncateUtf8("a€", 3), "a");
  });

  it("test_truncate_mid_4byte", function () {
    // "a😀" = 0x61 + 4 bytes; limits 2..4 land inside 😀 → "a"
    assert.strictEqual(truncateUtf8("a😀", 2), "a");
    assert.strictEqual(truncateUtf8("a😀", 3), "a");
    assert.strictEqual(truncateUtf8("a😀", 4), "a");
    // no split surrogate pair leaks out
    assert.ok(!truncateUtf8("a😀", 4).includes("�"));
  });

  it("test_truncate_limit_zero_and_tiny", function () {
    for (const limit of [0, 1, 2, 3]) {
      const out = truncateUtf8("😀😀", limit); // 4-byte chars only
      assert.strictEqual(out, "", "limit " + limit);
    }
    assert.strictEqual(truncateUtf8("", 0), "");
    // result is always valid UTF-8 within budget
    for (const limit of [0, 1, 2, 3, 4, 5]) {
      const out = truncateUtf8("é€😀", limit);
      const buf = Buffer.from(out, "utf8");
      assert.ok(buf.length <= limit, "limit " + limit);
      assert.strictEqual(buf.toString("utf8"), out);
    }
  });
});

// ── Tail parse: nominal + degradation paths ─────────────────────────

describe("extractTranscriptBlock", function () {
  it("tail_nominal_small_file", function () {
    // file smaller than window — whole file read, no negative seek
    const fp = tempFile(userLine("hello there") + asstLine([{ type: "text", text: "hi back" }]));
    const block = extractTranscriptBlock(fp);
    assert.strictEqual(
      block,
      "=== Recent conversation (last 1 exchanges) ===\n" +
        "[Assistant] hi back\n" + // reverse-chronological
        "[User] hello there\n" +
        "=== End recent conversation ===",
    );
  });

  it("tail_raw_api_content_shape", function () {
    // { "type": "user", "content": [...] } (no message wrapper)
    const fp = tempFile(
      JSON.stringify({ type: "user", content: [{ type: "text", text: "RAW" }] }) + "\n",
    );
    assert.ok(extractTranscriptBlock(fp).includes("[User] RAW"));
  });

  it("tail_malformed_lines", function () {
    const fp = tempFile(
      userLine("before") + "not json at all\n" + "{broken\n" + "\n" + userLine("after"),
    );
    const block = extractTranscriptBlock(fp);
    assert.ok(block.includes("[User] before"));
    assert.ok(block.includes("[User] after"));
    assert.ok(block.includes("last 2 exchanges"));
  });

  it("tail_invalid_utf8_line_dropped_neighbors_parsed", function () {
    // Rust BufRead::lines() parity: invalid-UTF-8 line dropped, not lossy-kept
    const content = Buffer.concat([
      Buffer.from(userLine("first")),
      Buffer.from([0xff, 0xfe, 0x0a]), // invalid UTF-8 line
      Buffer.from(userLine("second")),
    ]);
    const block = extractTranscriptBlock(tempFile(content));
    assert.ok(block.includes("[User] first"));
    assert.ok(block.includes("[User] second"));
    assert.ok(!block.includes("�"));
  });

  it("tail_crlf_lines_parsed", function () {
    const fp = tempFile(userLine("dos").replace("\n", "\r\n"));
    assert.ok(extractTranscriptBlock(fp).includes("[User] dos"));
  });

  it("tail_missing_file", function () {
    assert.strictEqual(extractTranscriptBlock("/nonexistent/path/t.jsonl"), null);
  });

  it("tail_empty_transcript_path", function (t) {
    const openSpy = t.mock.method(fs, "openSync");
    assert.strictEqual(extractTranscriptBlock(""), null);
    assert.strictEqual(openSpy.mock.callCount(), 0); // no read attempted
  });

  it("tail_non_string_path", function () {
    assert.strictEqual(extractTranscriptBlock(null), null);
    assert.strictEqual(extractTranscriptBlock(undefined), null);
  });

  it("tail_zero_length_file", function () {
    assert.strictEqual(extractTranscriptBlock(tempFile("")), null);
  });

  it("tail_directory_path", function () {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-transcript-dir-"));
    assert.strictEqual(extractTranscriptBlock(dir), null);
  });

  it("tail_whitespace_only_file", function () {
    assert.strictEqual(extractTranscriptBlock(tempFile("   \n\n  \n")), null);
  });
});

// ── Thinking suppression + tool pairing ─────────────────────────────

describe("exchange pairing", function () {
  it("tail_thinking_only_turns_suppressed", function () {
    const thinking = asstLine([{ type: "thinking", thinking: "hmm" }]);
    // thinking-only transcript → no turns → null
    assert.strictEqual(extractTranscriptBlock(tempFile(thinking)), null);
    // mixed: thinking turn suppressed, user turn survives
    const block = extractTranscriptBlock(tempFile(userLine("real") + thinking));
    assert.strictEqual(
      block,
      "=== Recent conversation (last 1 exchanges) ===\n" +
        "[User] real\n" +
        "=== End recent conversation ===",
    );
  });

  it("tail_tool_use_result_pairing", function () {
    const fp = tempFile(
      asstLine([
        { type: "text", text: "running" },
        { type: "tool_use", id: "t1", name: "Bash", input: { command: "ls -la" } },
      ]) + toolResultLine([{ type: "tool_result", tool_use_id: "t1", content: "file1\nfile2" }]),
    );
    const block = extractTranscriptBlock(fp);
    // reverse-chrono: tool turn (after asst text) comes first
    assert.strictEqual(
      block,
      "=== Recent conversation (last 0 exchanges) ===\n" +
        "[tool: Bash(ls -la) → file1\nfile2]\n" +
        "[Assistant] running\n" +
        "=== End recent conversation ===",
    );
  });

  it("tail_tool_result_array_content_shape", function () {
    const fp = tempFile(
      asstLine([{ type: "tool_use", id: "t1", name: "Read", input: { file_path: "/a/b" } }]) +
        toolResultLine([
          { type: "tool_result", tool_use_id: "t1", content: [{ type: "text", text: "body" }] },
        ]),
    );
    assert.ok(extractTranscriptBlock(fp).includes("[tool: Read(/a/b) → body]"));
  });

  it("tail_tool_result_missing_tool_use_id", function () {
    const fp = tempFile(
      asstLine([{ type: "tool_use", id: "t1", name: "Bash", input: { command: "x" } }]) +
        toolResultLine([{ type: "tool_result", content: "orphan" }]),
    );
    // result not paired → empty snippet
    assert.ok(extractTranscriptBlock(fp).includes("[tool: Bash(x) → ]"));
  });

  it("tail_tool_result_not_adjacent_unpaired", function () {
    const fp = tempFile(
      asstLine([{ type: "tool_use", id: "t1", name: "Bash", input: { command: "x" } }]) +
        userLine("interleaved") +
        toolResultLine([{ type: "tool_result", tool_use_id: "t1", content: "late" }]),
    );
    const block = extractTranscriptBlock(fp);
    // adjacent-record look-ahead ONLY: non-adjacent result not paired
    assert.ok(block.includes("[tool: Bash(x) → ]"));
    assert.ok(!block.includes("late"));
  });

  it("tail_orphan_tool_result_no_turn", function () {
    const fp = tempFile(
      toolResultLine([{ type: "tool_result", tool_use_id: "t9", content: "stray" }]),
    );
    assert.strictEqual(extractTranscriptBlock(fp), null);
  });

  it("tail_key_param_fallback", function () {
    // unknown tool → first string-valued input field (insertion order)
    const fp1 = tempFile(
      asstLine([{ type: "tool_use", id: "t1", name: "Custom", input: { n: 7, s: "picked" } }]),
    );
    assert.ok(extractTranscriptBlock(fp1).includes("[tool: Custom(picked) → ]"));
    // non-object input → ""
    const fp2 = tempFile(asstLine([{ type: "tool_use", id: "t1", name: "Custom", input: 42 }]));
    assert.ok(extractTranscriptBlock(fp2).includes("[tool: Custom() → ]"));
    // missing input → ""
    const fp3 = tempFile(asstLine([{ type: "tool_use", id: "t1", name: "Custom" }]));
    assert.ok(extractTranscriptBlock(fp3).includes("[tool: Custom() → ]"));
    // known tool, named field non-string → fallback to first string field
    const fp4 = tempFile(
      asstLine([{ type: "tool_use", id: "t1", name: "Bash", input: { command: 1, d: "fb" } }]),
    );
    assert.ok(extractTranscriptBlock(fp4).includes("[tool: Bash(fb) → ]"));
  });

  it("tail_tool_use_missing_id_or_name_skipped", function () {
    const fp = tempFile(
      asstLine([
        { type: "text", text: "t" },
        { type: "tool_use", name: "Bash", input: { command: "x" } }, // no id
        { type: "tool_use", id: "t2", input: { command: "y" } }, // no name
      ]),
    );
    const block = extractTranscriptBlock(fp);
    assert.ok(block.includes("[Assistant] t"));
    assert.ok(!block.includes("[tool:"));
  });
});

// ── Budget loop (MAX_PRECOMPACT_BYTES = 3000) ───────────────────────

describe("budget loop", function () {
  it("test_block_capped_at_3000_bytes", function () {
    // 20 user turns of ~400 bytes each → body capped at <=3000 bytes
    let content = "";
    for (let i = 0; i < 20; i++) {
      content += userLine("turn" + i + " " + "x".repeat(390));
    }
    const block = extractTranscriptBlock(tempFile(content));
    const lines = block.split("\n");
    const body = lines.slice(1, lines.length - 1).join("\n");
    assert.ok(Buffer.byteLength(body, "utf8") <= MAX_PRECOMPACT_BYTES);
    assert.ok(body.includes("turn19")); // most recent included
    assert.ok(!body.includes("turn0 ")); // oldest excluded
  });

  it("test_budget_break_not_continue", function () {
    // [NEW, huge, OLD] reverse-chrono: huge breaks the loop; OLD excluded
    // even though it would fit (Rust `break`, not `continue`)
    const content = userLine("OLD") + userLine("H".repeat(3500)) + userLine("NEW");
    const block = extractTranscriptBlock(tempFile(content));
    assert.strictEqual(
      block,
      "=== Recent conversation (last 1 exchanges) ===\n" +
        "[User] NEW\n" +
        "=== End recent conversation ===",
    );
  });

  it("test_zero_fitting_turns_returns_null", function () {
    // most recent turn alone exceeds 3000 bytes → empty parts → null
    assert.strictEqual(extractTranscriptBlock(tempFile(userLine("H".repeat(3500)))), null);
  });

  it("test_exchange_count_counts_user_turns_only", function () {
    const content =
      userLine("u1") + asstLine([{ type: "text", text: "a1" }]) + userLine("u2");
    const block = extractTranscriptBlock(tempFile(content));
    assert.ok(block.startsWith("=== Recent conversation (last 2 exchanges) ==="));
  });

  it("test_multibyte_turn_budget_uses_bytes_not_chars", function () {
    // 1600 chars of 2-byte é = 3200 bytes formatted > 3000 → excluded;
    // String.length (1600) would wrongly include it (UTF-16 trap)
    const content = userLine("é".repeat(1600)) + userLine("small");
    const block = extractTranscriptBlock(tempFile(content));
    assert.strictEqual(
      block,
      "=== Recent conversation (last 1 exchanges) ===\n" +
        "[User] small\n" +
        "=== End recent conversation ===",
    );
  });
});

// ── Window mechanics (12,000-byte tail) ─────────────────────────────

describe("window mechanics", function () {
  /** Build a file: ALPHA user line + unknown-type filler + RECENT user
   *  line, padded to exactly totalBytes. Filler parses but yields no turns,
   *  so the budget loop sees only the two user turns. */
  function windowFixture(totalBytes) {
    const first = userLine("ALPHA");
    const last = userLine("RECENT");
    return first + fillerLine(totalBytes - first.length - last.length) + last;
  }

  it("test_window_exactly_12000", function () {
    const block = extractTranscriptBlock(tempFile(windowFixture(WINDOW)));
    // window covers the whole file — first line intact
    assert.ok(block.includes("[User] ALPHA"));
    assert.ok(block.includes("[User] RECENT"));
    assert.ok(block.includes("last 2 exchanges"));
  });

  it("test_window_12001", function () {
    // one byte over: window starts mid-first-line → partial line fails
    // JSON.parse and is discarded; trailing "x" is a malformed line
    const block = extractTranscriptBlock(tempFile(windowFixture(WINDOW) + "x"));
    assert.ok(!block.includes("ALPHA"));
    assert.ok(block.includes("[User] RECENT"));
    assert.ok(block.includes("last 1 exchanges"));
  });

  it("tail_multibyte_split_at_window_edge", function () {
    // file = "é\n" + body, sized so the window boundary lands on é's
    // continuation byte → first windowed line is invalid UTF-8 → dropped
    // (no replacement-char divergence), neighbors still parsed
    const head = "é\n"; // 3 bytes
    const last = userLine("RECENT");
    const bodyBytes = WINDOW + 1 - Buffer.byteLength(head); // fileLen = 12,001
    const content = head + fillerLine(bodyBytes - last.length) + last;
    assert.strictEqual(Buffer.byteLength(content), WINDOW + 1);
    const block = extractTranscriptBlock(tempFile(content));
    assert.ok(block.includes("[User] RECENT"));
    assert.ok(!block.includes("�"));
  });

  it("test_read_is_single_tail_read", function (t) {
    // C-03 sync-path budget: exactly one open + one positioned read
    const fp = tempFile(windowFixture(WINDOW * 2)); // larger than window
    const openSpy = t.mock.method(fs, "openSync");
    const readSpy = t.mock.method(fs, "readSync");
    const block = extractTranscriptBlock(fp);
    assert.strictEqual(openSpy.mock.callCount(), 1);
    assert.strictEqual(readSpy.mock.callCount(), 1);
    // tail read: ALPHA (outside window) absent, RECENT present
    assert.ok(!block.includes("ALPHA"));
    assert.ok(block.includes("[User] RECENT"));
    // read was the tail region: offset = fileLen - WINDOW, length = WINDOW
    const call = readSpy.mock.calls[0];
    assert.strictEqual(call.arguments[3], WINDOW);
    assert.strictEqual(call.arguments[4], WINDOW * 2 - WINDOW);
  });
});
