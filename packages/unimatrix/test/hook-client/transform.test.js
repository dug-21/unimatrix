"use strict";

const { describe, it } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const path = require("path");
const {
  renderEnvelope,
  writeSyncOutput,
  INJECTION_HEADER,
} = require("../../lib/hook-client/transform");

const TRANSFORM_SRC_PATH = path.join(__dirname, "..", "..", "lib", "hook-client", "transform.js");
const PARITY_DIR = path.join(__dirname, "..", "fixtures", "parity");

const ENVELOPE_PREFIX =
  '{"hookSpecificOutput":{"hookEventName":"SubagentStart","additionalContext":';
const ENVELOPE_SUFFIX = "}}\n";

// Character builders — keep this source file pure printable ASCII.
const C = String.fromCharCode;
const CP = String.fromCodePoint;

/**
 * Independent JSON string escaper (oracle approximation of serde_json /
 * RFC 8259 minimal escaping). Deliberately NOT the production code path:
 * escapes quote and backslash, shortforms for backspace/tab/LF/FF/CR,
 * lowercase-hex \u00xx for remaining control chars, raw pass-through for
 * everything else (including non-BMP and U+2028/U+2029). The true byte
 * authority is the Layer 1 golden suite (ADR-001/ADR-002).
 */
function independentJsonEscape(text) {
  const SHORT = { 8: "\\b", 9: "\\t", 10: "\\n", 12: "\\f", 13: "\\r" };
  let out = '"';
  for (const ch of text) {
    const cp = ch.codePointAt(0);
    if (ch === '"') out += '\\"';
    else if (ch === "\\") out += "\\\\";
    else if (cp < 0x20) out += SHORT[cp] || "\\u00" + cp.toString(16).padStart(2, "0");
    else out += ch;
  }
  return out + '"';
}

function expectedSubagentBytes(text) {
  return Buffer.from(ENVELOPE_PREFIX + independentJsonEscape(text) + ENVELOPE_SUFFIX, "utf8");
}

/** Capture process.stdout.write calls while fn runs; restore afterwards. */
function captureStdout(fn) {
  const writes = [];
  const original = process.stdout.write;
  process.stdout.write = function (chunk) {
    writes.push(Buffer.isBuffer(chunk) ? Buffer.from(chunk) : Buffer.from(String(chunk), "utf8"));
    return true;
  };
  try {
    fn();
  } finally {
    process.stdout.write = original;
  }
  return writes;
}

function okTextResult(body, contentType) {
  return {
    ok: true,
    status: 200,
    contentType: contentType === undefined ? "text/plain" : contentType,
    body: body === null ? null : Buffer.from(body, "utf8"),
    failureClass: null,
  };
}

// --- AC-04: SubagentStart envelope byte parity -----------------------

describe("transform.renderEnvelope - SubagentStart envelope (AC-04)", function () {
  it("test_subagent_envelope_golden_bytes", function (t) {
    // Golden-driven byte comparison. Until the parity corpus is generated
    // (ADR-001, parity-corpus component), pin against the independent
    // escaper; the Layer 1 suite owns golden iteration.
    if (!fs.existsSync(PARITY_DIR)) {
      t.diagnostic("parity goldens not yet generated - using independent escaper oracle");
    }
    // A real Entries wire body always carries the format_injection header
    // (the wrap-dispatch key); the rest exercises the inner-scalar escaping.
    const text = INJECTION_HEADER + "## Relevant Context\n\n- entry one\n- entry two";
    const out = renderEnvelope("SubagentStart", text);
    assert.ok(Buffer.isBuffer(out));
    assert.ok(out.equals(expectedSubagentBytes(text)), "envelope bytes diverge from oracle form");
    // Structural pins: compact separators, key order, trailing newline.
    const s = out.toString("utf8");
    assert.ok(s.startsWith(ENVELOPE_PREFIX));
    assert.ok(s.endsWith(ENVELOPE_SUFFIX));
    assert.strictEqual(
      s.indexOf(" "),
      ENVELOPE_PREFIX.length + 1 + text.indexOf(" "),
      "no whitespace outside the inner scalar (compact separators)"
    );
    // Round-trips to the exact envelope structure.
    const parsed = JSON.parse(s);
    assert.deepStrictEqual(parsed, {
      hookSpecificOutput: { hookEventName: "SubagentStart", additionalContext: text },
    });
  });

  it("test_subagent_envelope_trailing_newline_exactly_one", function () {
    const out = renderEnvelope("SubagentStart", INJECTION_HEADER + "x");
    const s = out.toString("utf8");
    assert.ok(s.startsWith(ENVELOPE_PREFIX), "header-prefixed body must wrap");
    assert.ok(s.endsWith("\n"));
    assert.ok(!s.endsWith("\n\n"));
  });

  it("test_subagent_header_dispatch_mirrors_oracle_enum_match", function () {
    // WIRE-DISCRIMINATOR DISPATCH (vnc-026 agent-24; resolves the former
    // stdout-subagent-non-entries-fallback todo).
    //
    // The UDS oracle (hook.rs write_stdout_subagent_inject_response) matches
    // on the in-process HookResponse ENUM: Entries -> envelope, everything
    // else (incl. BriefingContent, production-reachable via the col-025
    // goal-present branch, listener.rs:1174) -> write_stdout PLAIN. The
    // ADR-003 text/plain wire sends both as 200 text/plain -- but the Entries
    // variant is structurally marked: format_injection (the single formatting
    // truth, AC-07) unconditionally prepends INJECTION_HEADER to every Entries
    // body, and an unrenderable Entries yields 204, never a headerless 200.
    // BriefingContent always starts with the CONTEXT_GET_INSTRUCTION constant
    // (index_briefing.rs:41). The header IS the wire discriminator -- this is
    // contract-keyed dispatch, not content sniffing; ADR-002's letter governs
    // envelope SERIALIZATION (literal templates only), not the wrap decision.
    //
    // Misclassification is only possible for a BriefingContent that begins
    // with the exact header line -- impossible in production (fixed
    // CONTEXT_GET_INSTRUCTION preamble) and fail-safe if contrived (the body
    // is wrapped, so injection still occurs). A server-side header change
    // breaks the Layer 1 goldens loudly (ADR-001 drift check).
    const briefingBody = "unexpected briefing on the SubagentStart path";
    const out = renderEnvelope("SubagentStart", briefingBody);
    // Non-Entries body -> oracle plain bytes (println! parity), NOT an envelope.
    assert.ok(
      out.equals(Buffer.from(briefingBody + "\n", "utf8")),
      "non-header SubagentStart body must take the oracle plain path"
    );
    // Entries-shaped body -> envelope.
    const entriesBody = INJECTION_HEADER + "[x] (decision, 90% confidence)\n";
    const out2 = renderEnvelope("SubagentStart", entriesBody);
    assert.ok(out2.equals(expectedSubagentBytes(entriesBody)), "header body must wrap");
    // Dispatch keys on the FULL header line incl. its newline: a body that is
    // only the bare header text without the line break is not Entries-shaped.
    const bareHeader = "--- Unimatrix Context ---";
    const out3 = renderEnvelope("SubagentStart", bareHeader);
    assert.ok(out3.equals(Buffer.from(bareHeader + "\n", "utf8")), "no partial-prefix wrap");
    // The header only matters on the SubagentStart path -- plain events never wrap.
    const out4 = renderEnvelope("UserPromptSubmit", entriesBody);
    assert.ok(out4.equals(Buffer.from(entriesBody + "\n", "utf8")), "plain events stay plain");
  });
});

// --- R-03: adversarial inner-scalar escaping -------------------------

// Adversarial bodies ride behind the INJECTION_HEADER (the Entries wire
// invariant that selects the envelope path); the inner-scalar escaping under
// test is unaffected by the ASCII header prefix.
const HEADER_ESCAPED = '--- Unimatrix Context ---\\n'; // header as JSON-escaped text

describe("transform - adversarial inner-scalar escaping (R-03)", function () {
  it("test_quotes_and_backslashes", function () {
    const text = INJECTION_HEADER + 'he said "hi" \\ done';
    const out = renderEnvelope("SubagentStart", text);
    const expected = Buffer.from(
      ENVELOPE_PREFIX + '"' + HEADER_ESCAPED + 'he said \\"hi\\" \\\\ done"' + ENVELOPE_SUFFIX,
      "utf8"
    );
    assert.ok(out.equals(expected));
  });

  it("test_control_chars_shortforms", function () {
    // backspace/tab/LF/FF/CR get two-char shortforms; others lowercase
    // hex \u00xx. Hardcoded expectation pins JS escaping byte-for-byte.
    const text = INJECTION_HEADER + C(0x00, 0x01, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x1f);
    const out = renderEnvelope("SubagentStart", text);
    const expected = Buffer.from(
      ENVELOPE_PREFIX +
        '"' +
        HEADER_ESCAPED +
        '\\u0000\\u0001\\b\\t\\n\\u000b\\f\\r\\u001f"' +
        ENVELOPE_SUFFIX,
      "utf8"
    );
    assert.ok(out.equals(expected));
  });

  it("test_control_chars_dense_full_range", function () {
    let text = INJECTION_HEADER;
    for (let i = 0; i < 0x20; i++) text += C(i);
    const out = renderEnvelope("SubagentStart", text);
    assert.ok(out.equals(expectedSubagentBytes(text)));
  });

  it("test_emoji_non_bmp_raw_passthrough", function () {
    // 4-byte UTF-8, surrogate pair in JS; neither serializer u-escapes it.
    const text = INJECTION_HEADER + "crab " + CP(0x1f980) + " done " + CP(0x1f600);
    const out = renderEnvelope("SubagentStart", text);
    assert.ok(out.equals(expectedSubagentBytes(text)));
    assert.ok(out.includes(Buffer.from([0xf0, 0x9f, 0xa6, 0x80])), "raw 4-byte emoji expected");
    assert.ok(!out.includes(Buffer.from("\\ud83e", "utf8")), "must not u-escape surrogates");
  });

  it("test_u2028_u2029_raw_passthrough", function () {
    // Classic divergence point: JSON-legal raw, JS-source-illegal pre-ES2019.
    // Neither serializer escapes them.
    const text = INJECTION_HEADER + "a" + C(0x2028) + "b" + C(0x2029) + "c";
    const out = renderEnvelope("SubagentStart", text);
    assert.ok(out.equals(expectedSubagentBytes(text)));
    assert.ok(out.includes(Buffer.from([0xe2, 0x80, 0xa8])), "raw U+2028 bytes expected");
    assert.ok(out.includes(Buffer.from([0xe2, 0x80, 0xa9])), "raw U+2029 bytes expected");
    assert.ok(!out.includes(Buffer.from("\\u2028", "utf8")), "must not escape U+2028");
  });

  it("test_mixed_crlf", function () {
    const text = INJECTION_HEADER + "line1\r\nline2\nline3\r";
    const out = renderEnvelope("SubagentStart", text);
    const expected = Buffer.from(
      ENVELOPE_PREFIX + '"' + HEADER_ESCAPED + 'line1\\r\\nline2\\nline3\\r"' + ENVELOPE_SUFFIX,
      "utf8"
    );
    assert.ok(out.equals(expected));
  });

  it("test_surrogate_pair_adjacent_to_boundary_chars", function () {
    // quote + non-BMP pair + backslash + control char, packed together.
    const text = INJECTION_HEADER + '"' + CP(0x1f600) + "\\" + C(0x01);
    const out = renderEnvelope("SubagentStart", text);
    assert.ok(out.equals(expectedSubagentBytes(text)));
  });
});

// --- AC-03: plain path ------------------------------------------------

describe("transform - plain path (AC-03)", function () {
  it("test_plain_envelope_golden_bytes", function () {
    const body = "## Relevant Context\n\n- [decision] use redb";
    const out = renderEnvelope("UserPromptSubmit", body);
    assert.ok(out.equals(Buffer.from(body + "\n", "utf8")), "body verbatim + ONE newline");
  });

  it("test_plain_precompact_verbatim", function () {
    // Body printed verbatim - no escaping, no envelope, no budget.
    const body = '{"not":"an envelope"} with "quotes" and \\ slashes';
    const out = renderEnvelope("PreCompact", body);
    assert.ok(out.equals(Buffer.from(body + "\n", "utf8")));
  });

  it("test_plain_always_appends_one_newline", function () {
    // println! parity: append exactly one newline even if body ends with one.
    const out = renderEnvelope("UserPromptSubmit", "x\n");
    assert.ok(out.equals(Buffer.from("x\n\n", "utf8")));
  });

  it("test_empty_text_returns_null", function () {
    assert.strictEqual(renderEnvelope("UserPromptSubmit", ""), null);
    assert.strictEqual(renderEnvelope("SubagentStart", ""), null);
    assert.strictEqual(renderEnvelope("PreCompact", null), null);
    assert.strictEqual(renderEnvelope("PreCompact", undefined), null);
  });
});

// --- writeSyncOutput: defensive output rules (R-15, C-05) -------------

describe("transform.writeSyncOutput - defensive rules", function () {
  it("test_200_text_plain_writes_exact_body_newline", function () {
    const writes = captureStdout(function () {
      writeSyncOutput("UserPromptSubmit", okTextResult("hello"));
    });
    assert.strictEqual(writes.length, 1, "exactly one stdout write per spawn");
    assert.ok(writes[0].equals(Buffer.from("hello\n", "utf8")));
  });

  it("test_subagent_200_text_plain_writes_envelope", function () {
    const body = INJECTION_HEADER + "ctx";
    const writes = captureStdout(function () {
      writeSyncOutput("SubagentStart", okTextResult(body));
    });
    assert.strictEqual(writes.length, 1);
    assert.ok(writes[0].equals(expectedSubagentBytes(body)));
  });

  it("test_subagent_200_briefing_body_writes_plain", function () {
    // Non-Entries (BriefingContent) body on the SubagentStart path: oracle
    // plain bytes, no envelope (write_stdout fallthrough parity).
    const writes = captureStdout(function () {
      writeSyncOutput("SubagentStart", okTextResult("briefing index table"));
    });
    assert.strictEqual(writes.length, 1);
    assert.ok(writes[0].equals(Buffer.from("briefing index table\n", "utf8")));
  });

  it("test_text_plain_charset_variant_accepted", function () {
    const writes = captureStdout(function () {
      writeSyncOutput("UserPromptSubmit", okTextResult("hello", "text/plain; charset=utf-8"));
    });
    assert.strictEqual(writes.length, 1);
    assert.ok(writes[0].equals(Buffer.from("hello\n", "utf8")));
  });

  it("test_text_plain_case_insensitive", function () {
    const writes = captureStdout(function () {
      writeSyncOutput("UserPromptSubmit", okTextResult("hello", "Text/Plain; Charset=UTF-8"));
    });
    assert.strictEqual(writes.length, 1);
  });

  it("test_empty_body_no_stdout", function () {
    // 204 and 200-with-empty-body produce zero bytes (silent skip).
    const noContent = { ok: true, status: 204, contentType: null, body: null, failureClass: null };
    const empty200 = okTextResult("");
    for (const res of [noContent, empty200]) {
      const writes = captureStdout(function () {
        writeSyncOutput("SubagentStart", res);
        writeSyncOutput("UserPromptSubmit", res);
      });
      assert.strictEqual(writes.length, 0, "status " + res.status + " must produce no stdout");
    }
  });

  it("test_nontext_200_produces_no_stdout", function () {
    // R-15: Pong/Error JSON and absent Content-Type never reach the host.
    const variants = [
      okTextResult('{"type":"Pong"}', "application/json"),
      okTextResult("body", null),
      okTextResult("body", ""),
      okTextResult("body", "text/html"),
      okTextResult("body", "application/text-plain"),
    ];
    for (const res of variants) {
      const writes = captureStdout(function () {
        writeSyncOutput("UserPromptSubmit", res);
        writeSyncOutput("SubagentStart", res);
      });
      assert.strictEqual(writes.length, 0, "content-type " + res.contentType + " must be dropped");
    }
  });

  it("test_failure_paths_produce_no_stdout", function () {
    // Every transport failure class produces zero stdout bytes (C-05).
    const classes = ["auth", "connect", "timeout", "http_4xx", "http_5xx"];
    const statusFor = { auth: 401, connect: 0, timeout: 0, http_4xx: 422, http_5xx: 500 };
    for (const failureClass of classes) {
      const res = {
        ok: false,
        status: statusFor[failureClass],
        contentType: "text/plain",
        body: Buffer.from("error body", "utf8"),
        failureClass,
      };
      const writes = captureStdout(function () {
        writeSyncOutput("SubagentStart", res);
        writeSyncOutput("UserPromptSubmit", res);
      });
      assert.strictEqual(writes.length, 0, failureClass + " must produce no stdout");
    }
  });

  it("test_null_result_no_stdout", function () {
    const writes = captureStdout(function () {
      writeSyncOutput("UserPromptSubmit", null);
      writeSyncOutput("UserPromptSubmit", undefined);
    });
    assert.strictEqual(writes.length, 0);
  });
});

// --- ADR-002 literal-template enforcement (grep-gate) -----------------

describe("transform - literal-template enforcement (ADR-002)", function () {
  it("test_no_object_serialization_in_transform", function () {
    const src = fs.readFileSync(TRANSFORM_SRC_PATH, "utf8");

    // Exactly ONE serializer call site in the whole module...
    const callSites = src.match(/JSON\.stringify\s*\(/g) || [];
    assert.strictEqual(
      callSites.length,
      1,
      "transform.js must contain exactly one JSON.stringify call site"
    );

    // ...and its argument is the inner text scalar, never an object/envelope.
    const argMatch = src.match(/JSON\.stringify\s*\(\s*([^)]*)\)/);
    assert.ok(argMatch, "serializer call site not found");
    const arg = argMatch[1].trim();
    assert.strictEqual(arg, "text", "serializer argument must be the inner text scalar");
    assert.ok(!arg.startsWith("{"), "must never serialize an object literal");

    // The envelope key appears exactly once - inside the literal template.
    const envelopeKeys = src.match(/hookSpecificOutput/g) || [];
    assert.strictEqual(
      envelopeKeys.length,
      1,
      "hookSpecificOutput must appear exactly once (the literal template)"
    );
    assert.ok(
      src.includes(
        "'{\"hookSpecificOutput\":{\"hookEventName\":\"SubagentStart\",\"additionalContext\":'"
      ),
      "the literal template prefix must be present verbatim"
    );

    // Single stdout surface, no stray logging.
    const stdoutWrites = src.match(/process\.stdout\.write/g) || [];
    assert.strictEqual(stdoutWrites.length, 1, "exactly one stdout write site");
    assert.ok(!src.includes("console.log"), "no console.log in transform.js");
  });

  it("test_transform_source_is_ascii_safe", function () {
    // Guard against raw control bytes sneaking into the byte-pinned template.
    // CR is tolerated ONLY as part of a CRLF line ending: windows-latest
    // checkouts run Git with core.autocrlf=true, so working-tree newlines may
    // be CRLF. That is a checkout artifact, not a source byte -- JS string
    // literals cannot contain raw line breaks (syntax error), so a CRLF can
    // never sit inside the byte-pinned template itself. A bare CR (not
    // followed by LF) is still rejected.
    const buf = fs.readFileSync(TRANSFORM_SRC_PATH);
    for (let i = 0; i < buf.length; i++) {
      const b = buf[i];
      if (b === 0x0d && buf[i + 1] === 0x0a) continue; // CRLF line ending
      assert.ok(
        b === 0x0a || b >= 0x20,
        "raw control byte 0x" + b.toString(16) + " at offset " + i
      );
    }
  });
});
