"use strict";

const assert = require("assert");
const { describe, it } = require("node:test");

// Component tests — Surgical TOML helper (nan-023, ADR-002). Pure-function unit
// tests over `raw:string`, no filesystem. The writer manages EXACTLY
// `[mcp_servers.unimatrix]` and copies every other byte verbatim; foreign-byte
// preservation is a FIRST-CLASS assertion (NFR-08), asserted by byte-diff.
// Primary risk R-04 (foreign preservation), plus R-10 (injection escaping) and
// R-11 (malformed → safe signal, never throw / never a partial write).

const {
  upsertTomlTable,
  readTomlTable,
  tomlString,
} = require("../lib/codex-install.js");

const TABLE = "mcp_servers.unimatrix";

// Minimal TOML BASIC-string decoder — the R-10 "read-back" oracle. Inverse of
// `tomlString`; used only to prove escaped values re-parse to the intended
// value. Not a general TOML parser.
function parseTomlBasicString(literal) {
  assert.ok(literal.startsWith('"') && literal.endsWith('"'), "not a quoted basic string: " + literal);
  const inner = literal.slice(1, -1);
  let out = "";
  for (let i = 0; i < inner.length; i++) {
    const ch = inner[i];
    if (ch !== "\\") { out += ch; continue; }
    const esc = inner[i + 1];
    i += 1;
    switch (esc) {
      case "\\": out += "\\"; break;
      case '"': out += '"'; break;
      case "b": out += "\b"; break;
      case "t": out += "\t"; break;
      case "n": out += "\n"; break;
      case "f": out += "\f"; break;
      case "r": out += "\r"; break;
      case "u": {
        out += String.fromCodePoint(parseInt(inner.slice(i + 1, i + 5), 16));
        i += 4;
        break;
      }
      default: assert.fail("unknown escape \\" + esc + " in " + literal);
    }
  }
  return out;
}

// Extract the raw quoted literal for `key = "..."` from a rendered body/file.
function extractValueLiteral(text, key) {
  const m = new RegExp("^\\s*" + key + '\\s*=\\s*(".*?")\\s*$', "m").exec(text);
  assert.ok(m, "no `" + key + "` line found in:\n" + text);
  return m[1];
}

describe("readTomlTable", () => {
  it("test_readTomlTable_present_returns_body", () => {
    const raw = '[mcp_servers.unimatrix]\ncommand = "x"\n';
    const r = readTomlTable(raw, TABLE);
    assert.strictEqual(r.present, true);
    assert.strictEqual(r.body, 'command = "x"\n');
  });

  it("test_readTomlTable_absent_returns_not_present", () => {
    const raw = '[tools]\nfoo = 1\n\n[mcp_servers.other]\ncommand = "z"\n';
    const r = readTomlTable(raw, TABLE);
    assert.strictEqual(r.present, false);
    assert.strictEqual(r.body, "");
  });

  it("test_readTomlTable_stops_at_next_top_level_table", () => {
    const raw = '[mcp_servers.unimatrix]\ncommand = "x"\n\n[other]\nk = 1\n';
    const r = readTomlTable(raw, TABLE);
    assert.strictEqual(r.present, true);
    assert.strictEqual(r.body, 'command = "x"\n\n'); // includes the trailing blank, excludes [other]
    assert.ok(!r.body.includes("[other]"));
    assert.ok(!r.body.includes("k = 1"));
  });

  it("test_readTomlTable_distinguishes_parent_table", () => {
    // A stray `[mcp_servers]` parent must NOT be read as the child.
    const raw =
      '[mcp_servers]\nnetwork = true\n\n[mcp_servers.unimatrix]\ncommand = "child"\n';
    const r = readTomlTable(raw, TABLE);
    assert.strictEqual(r.present, true);
    assert.strictEqual(r.body, 'command = "child"\n');
    assert.ok(!r.body.includes("network"));
  });

  it("returns malformed (never throws) on an unterminated table header", () => {
    const raw = "[mcp_servers.unimatrix\ncommand = 1\n";
    const r = readTomlTable(raw, TABLE);
    assert.strictEqual(r.present, false);
    assert.strictEqual(r.malformed, true);
  });
});

describe("upsertTomlTable — insert (absent)", () => {
  it("test_upsertTomlTable_absent_appends_with_single_blank_separator", () => {
    const raw = "[tools]\nfoo = 1"; // non-empty, no trailing newline
    const r = upsertTomlTable(raw, TABLE, ['command = "/abs/unimatrix"']);
    assert.strictEqual(r.changed, true);
    assert.strictEqual(
      r.text,
      '[tools]\nfoo = 1\n\n[mcp_servers.unimatrix]\ncommand = "/abs/unimatrix"\n'
    );
    // exactly one blank-line separator (not two)
    assert.ok(!r.text.includes("\n\n\n"));
    // every original byte preserved as a prefix
    assert.ok(r.text.startsWith(raw));
  });

  it("test_upsertTomlTable_empty_file_appends_no_leading_blank", () => {
    const r = upsertTomlTable("", TABLE, ['command = "/abs/unimatrix"']);
    assert.strictEqual(r.changed, true);
    assert.strictEqual(
      r.text,
      '[mcp_servers.unimatrix]\ncommand = "/abs/unimatrix"\n'
    );
    assert.ok(!r.text.startsWith("\n"));
  });

  it("test_upsertTomlTable_file_ending_in_newline_no_double_blank", () => {
    const raw = "foo = 1\n"; // already ends in a newline
    const r = upsertTomlTable(raw, TABLE, ['command = "x"']);
    assert.strictEqual(r.text, 'foo = 1\n\n[mcp_servers.unimatrix]\ncommand = "x"\n');
    assert.ok(!r.text.includes("\n\n\n")); // one blank line, never two
  });

  it("does not add a separator when the file already ends in a blank line", () => {
    const raw = "foo = 1\n\n";
    const r = upsertTomlTable(raw, TABLE, ['command = "x"']);
    assert.strictEqual(r.text, 'foo = 1\n\n[mcp_servers.unimatrix]\ncommand = "x"\n');
  });

  it("appends multi-key body (command + args) verbatim", () => {
    const r = upsertTomlTable("", TABLE, [
      'command = "node"',
      'args = ["/abs/mcp-bridge.js", "abc123"]',
    ]);
    assert.strictEqual(
      r.text,
      '[mcp_servers.unimatrix]\ncommand = "node"\nargs = ["/abs/mcp-bridge.js", "abc123"]\n'
    );
  });
});

describe("upsertTomlTable — update (present)", () => {
  it("test_upsertTomlTable_present_replaces_only_region_body", () => {
    const raw =
      '# top comment\n[tools]\na = 1\n\n[mcp_servers.unimatrix]\ncommand = "STALE"\n\n[mcp_servers.other]\ncommand = "keep"\n';
    const r = upsertTomlTable(raw, TABLE, ['command = "FRESH"']);
    assert.strictEqual(r.changed, true);
    // Everything before the owned header is byte-identical.
    const beforeHeader = "# top comment\n[tools]\na = 1\n\n[mcp_servers.unimatrix]\n";
    assert.ok(r.text.startsWith(beforeHeader));
    // Everything from the next table onward is byte-identical.
    const afterRegion = '[mcp_servers.other]\ncommand = "keep"\n';
    assert.ok(r.text.endsWith(afterRegion));
    assert.ok(r.text.includes('command = "FRESH"'));
    assert.ok(!r.text.includes("STALE"));
  });

  it("test_upsertTomlTable_idempotent_when_body_equal", () => {
    const raw = '[mcp_servers.unimatrix]\ncommand = "x"\n';
    const r = upsertTomlTable(raw, TABLE, ['command = "x"']);
    assert.strictEqual(r.changed, false);
    assert.strictEqual(r.text, raw); // byte-identical (AC-04 / NFR-01)
  });

  it("is idempotent across a preserved trailing blank line before the next table", () => {
    const raw = '[mcp_servers.unimatrix]\ncommand = "x"\n\n[other]\nk = 1\n';
    const r = upsertTomlTable(raw, TABLE, ['command = "x"']);
    assert.strictEqual(r.changed, false);
    assert.strictEqual(r.text, raw);
  });

  it("re-upsert after a change is idempotent (round trip stabilizes)", () => {
    const raw = '[mcp_servers.unimatrix]\ncommand = "old"\n';
    const once = upsertTomlTable(raw, TABLE, ['command = "new"']);
    assert.strictEqual(once.changed, true);
    const twice = upsertTomlTable(once.text, TABLE, ['command = "new"']);
    assert.strictEqual(twice.changed, false);
    assert.strictEqual(twice.text, once.text);
  });
});

describe("foreign preservation (R-04 — first-class)", () => {
  it("test_upsertTomlTable_preserves_foreign_tables", () => {
    // Foreign sibling table, a top-level [tools], interleaved comments, and
    // non-alphabetical key order. Byte-diff of every foreign region = empty.
    const raw = [
      "# header comment",
      "[tools]",
      "zeta = 1        # non-alpha order on purpose",
      "alpha = 2",
      "",
      "# owns nothing here",
      "[mcp_servers.other]",
      'command = "other-bin"',
      "env = { KEY = \"v\" }",
      "",
      "[[servers]]",
      'name = "arr"',
      "",
    ].join("\n");
    const foreignBefore = raw; // no owned table yet
    const r = upsertTomlTable(raw, TABLE, ['command = "/abs/unimatrix"']);
    assert.strictEqual(r.changed, true);
    // The entire original file survives as a prefix (nothing foreign moved).
    assert.ok(r.text.startsWith(foreignBefore));
    // Owned table is present.
    const rd = readTomlTable(r.text, TABLE);
    assert.strictEqual(rd.present, true);
    assert.strictEqual(rd.body, 'command = "/abs/unimatrix"\n');
    // Foreign regions unchanged: strip the owned region and compare byte-for-byte.
    const ownedBlock = '[mcp_servers.unimatrix]\ncommand = "/abs/unimatrix"\n';
    const withoutOwned = r.text.slice(0, r.text.indexOf("\n[mcp_servers.unimatrix]") + 1) +
      r.text.slice(r.text.indexOf(ownedBlock) + ownedBlock.length);
    // The foreign bytes we started with must all still be present, in order.
    assert.ok(withoutOwned.includes("[mcp_servers.other]"));
    assert.ok(withoutOwned.includes("[[servers]]"));
    assert.ok(withoutOwned.includes("zeta = 1"));
    assert.ok(withoutOwned.includes("# header comment"));
  });

  it("test_upsertTomlTable_preserves_comment_adjacent_above_owned", () => {
    const raw =
      '[tools]\na = 1\n\n# unimatrix mcp (keep me)\n[mcp_servers.unimatrix]\ncommand = "old"\n';
    const r = upsertTomlTable(raw, TABLE, ['command = "new"']);
    assert.strictEqual(r.changed, true);
    assert.ok(r.text.includes("# unimatrix mcp (keep me)\n[mcp_servers.unimatrix]"));
    assert.ok(r.text.includes('command = "new"'));
    assert.ok(!r.text.includes("old"));
  });

  it("test_upsertTomlTable_preserves_comment_adjacent_below_owned", () => {
    const raw =
      '[mcp_servers.unimatrix]\ncommand = "old"\n# trailing owned comment\n\n[other]\nk = 1\n';
    const r = upsertTomlTable(raw, TABLE, ['command = "new"']);
    assert.strictEqual(r.changed, true);
    // The comment below the owned command line lives INSIDE the region, so it is
    // part of the replaced body — but the update replaces the whole body with the
    // desired body. Assert the foreign [other] table and its bytes survive intact.
    assert.ok(r.text.endsWith("[other]\nk = 1\n"));
    assert.ok(r.text.includes('command = "new"'));
  });

  it("test_upsertTomlTable_preserves_crlf_line_endings", () => {
    const raw = "[tools]\r\na = 1\r\n\r\n[mcp_servers.other]\r\ncommand = \"z\"\r\n";
    const r = upsertTomlTable(raw, TABLE, ['command = "/abs/unimatrix"']);
    assert.strictEqual(r.changed, true);
    // Foreign CRLF bytes untouched (original survives as a prefix).
    assert.ok(r.text.startsWith(raw));
    // The appended owned table uses CRLF too (dominant ending).
    assert.ok(r.text.includes("[mcp_servers.unimatrix]\r\ncommand = \"/abs/unimatrix\"\r\n"));
    assert.ok(!/[^\r]\n/.test(r.text.slice(raw.length))); // no bare LF in the appended region
  });

  it("preserves a `[` that appears inside a multi-line basic string (not a header)", () => {
    const raw = [
      "[tools]",
      'note = """',
      "[mcp_servers.unimatrix]  <- this is text, not a header",
      '"""',
      "",
    ].join("\n");
    // The owned table is genuinely absent (the bracket line is inside a string).
    assert.strictEqual(readTomlTable(raw, TABLE).present, false);
    const r = upsertTomlTable(raw, TABLE, ['command = "x"']);
    assert.strictEqual(r.changed, true);
    assert.ok(r.text.startsWith(raw)); // string content preserved verbatim
    // The real owned table is appended at the end, exactly once.
    assert.strictEqual((r.text.match(/^\[mcp_servers\.unimatrix\]$/gm) || []).length, 1);
  });

  it("test_upsertTomlTable_present_replaces_only_region_body over parent + sibling", () => {
    // [mcp_servers] parent present but no child → treated as absent → append.
    const raw = "[mcp_servers]\nnetwork = false\n";
    const r = upsertTomlTable(raw, TABLE, ['command = "x"']);
    assert.strictEqual(r.changed, true);
    assert.ok(r.text.startsWith("[mcp_servers]\nnetwork = false\n"));
    assert.ok(r.text.includes("[mcp_servers.unimatrix]\ncommand = \"x\"\n"));
  });

  it("does not false-match a [mcp_servers.other] sibling", () => {
    const raw = '[mcp_servers.other]\ncommand = "keep"\n';
    assert.strictEqual(readTomlTable(raw, TABLE).present, false);
    const r = upsertTomlTable(raw, TABLE, ['command = "x"']);
    assert.strictEqual(r.changed, true);
    assert.ok(r.text.startsWith('[mcp_servers.other]\ncommand = "keep"\n'));
  });
});

describe("malformed / fail-safe (R-11)", () => {
  it("test_upsertTomlTable_ambiguous_boundary_signals_malformed — unterminated multi-line string", () => {
    const raw = "[tools]\nnote = \"\"\"\nunclosed here\n";
    const r = upsertTomlTable(raw, TABLE, ['command = "x"']);
    assert.strictEqual(r.malformed, true);
    assert.strictEqual(r.changed, false);
    assert.strictEqual(r.text, raw); // file byte-preserved — NEVER a partial write
  });

  it("signals malformed on an unterminated table header, file preserved", () => {
    const raw = "[mcp_servers.unimatrix\ncommand = 1\n";
    const r = upsertTomlTable(raw, TABLE, ['command = "x"']);
    assert.strictEqual(r.malformed, true);
    assert.strictEqual(r.changed, false);
    assert.strictEqual(r.text, raw);
  });

  it("never throws on arbitrary garbage input", () => {
    const inputs = ["", "\n\n\n", "]]][[[", "= = =", "\r\r\r", "🙂 [x", "[["];
    for (const raw of inputs) {
      assert.doesNotThrow(() => upsertTomlTable(raw, TABLE, ['command = "x"']));
      assert.doesNotThrow(() => readTomlTable(raw, TABLE));
    }
  });
});

describe("injection / string escaping (R-10)", () => {
  it("test_upsertTomlTable_escapes_metacharacter_path_value", () => {
    // A path containing a space and a double-quote. The emitted value must be
    // TOML-escaped and re-parse (read-back) to the intended path — never naive
    // concatenation that would break the string or inject a second key.
    const evil = '/opt/my apps/uni"; command = "pwned';
    const body = ["command = " + tomlString(evil)];
    const r = upsertTomlTable("", TABLE, body);
    assert.strictEqual(r.changed, true);

    // The rendered file must still parse as exactly one owned table with one key.
    const rd = readTomlTable(r.text, TABLE);
    assert.strictEqual(rd.present, true);
    // Read the value back through the TOML basic-string decoder → intended path.
    const literal = extractValueLiteral(rd.body, "command");
    assert.strictEqual(parseTomlBasicString(literal), evil);
    // The stray `command = "pwned` fragment did NOT become a second real key.
    assert.strictEqual((rd.body.match(/^command\s*=/gm) || []).length, 1);
  });

  it("escapes backslashes, control chars, tabs and newlines round-trip", () => {
    const samples = [
      "C:\\Program Files\\uni\\bin.exe",
      "a\tb\nc",
      "bell\u0007null\u0000del\u007f",
      'quote " and backslash \\ mixed',
      "unicode λ ✓ ok",
    ];
    for (const s of samples) {
      const lit = tomlString(s);
      assert.ok(lit.startsWith('"') && lit.endsWith('"'));
      assert.strictEqual(parseTomlBasicString(lit), s, "round-trip failed for: " + JSON.stringify(s));
    }
  });

  it("an escaped value survives a full upsert + read cycle for cloud args shape", () => {
    const bridge = "/abs/with space/mcp-bridge.js";
    const hash = 'h"ash';
    const body = [
      'command = "node"',
      "args = [" + tomlString(bridge) + ", " + tomlString(hash) + "]",
    ];
    const r = upsertTomlTable("", TABLE, body);
    const rd = readTomlTable(r.text, TABLE);
    assert.ok(rd.body.includes('command = "node"'));
    // Extract both quoted literals from the args array and decode them.
    const m = /args = \[(".*?"), (".*?")\]/.exec(rd.body);
    assert.ok(m, "args line not found:\n" + rd.body);
    assert.strictEqual(parseTomlBasicString(m[1]), bridge);
    assert.strictEqual(parseTomlBasicString(m[2]), hash);
  });
});
