"use strict";

// transcript.js — JSONL tail-parse (SubagentStart query derivation, RQ-6).
//
// Exact port of crates/unimatrix-server/src/uds/transcript_block.rs (the
// path-variant front-end). The Rust module is the read-only parity oracle
// (vnc-026 ADR-001); behavior here must match it byte-for-byte on the
// derived block. Used ONLY by the SubagentStart fallback in index.js — the
// single permitted transcript read on a sync spawn (FR-09 exception).
//
// Also exports truncateUtf8, shared by build-request.js (goal truncation)
// and delta.js (boundary trims). All byte budgets are measured on the UTF-8
// byte image via Buffer — never String.prototype.length (UTF-16 trap).

const fs = require("fs");

// Constants PINNED — transcript_block.rs:18-29 (R-14.2).
const MAX_PRECOMPACT_BYTES = 3000;
const TAIL_MULTIPLIER = 4; // window = MAX_PRECOMPACT_BYTES * 4 = 12,000 bytes
const TOOL_RESULT_SNIPPET_BYTES = 300;
const TOOL_KEY_PARAM_BYTES = 120;

// Hardcoded key-param map for 10 known Claude Code tools (OQ-3 settled).
const KEY_PARAM_FIELDS = {
  Bash: "command",
  Read: "file_path",
  Edit: "file_path",
  Write: "file_path",
  Glob: "pattern",
  Grep: "pattern",
  MultiEdit: "file_path",
  Task: "description",
  WebFetch: "url",
  WebSearch: "query",
};

/**
 * Truncate a string to at most `maxBytes` UTF-8 bytes, never splitting a
 * multi-byte character (transcript_block.rs::truncate_utf8). Backing off to
 * a UTF-8 char boundary keeps whole code points, so no surrogate pair is
 * ever split in the resulting JS string.
 */
function truncateUtf8(s, maxBytes) {
  const buf = Buffer.from(s, "utf8");
  if (buf.length <= maxBytes) {
    return s;
  }
  let end = maxBytes;
  // 0b10xxxxxx bytes are UTF-8 continuations — not char boundaries.
  while (end > 0 && (buf[end] & 0xc0) === 0x80) {
    end -= 1;
  }
  return buf.subarray(0, end).toString("utf8");
}

/** True iff `v` is a plain JSON object (Rust Value::as_object semantics). */
function isJsonObject(v) {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/**
 * Extract the content array from a JSONL record
 * (transcript_block.rs::get_content_array). Handles two shapes:
 *   { "type": "...", "message": { "content": [...] } }  (Claude Code UX)
 *   { "type": "...", "content": [...] }                 (raw API)
 */
function getContentArray(record) {
  const msg = record === null || record === undefined ? undefined : record.message;
  if (isJsonObject(msg) && Array.isArray(msg.content)) {
    return msg.content;
  }
  if (isJsonObject(record) && Array.isArray(record.content)) {
    return record.content;
  }
  return [];
}

/** Collect text from type:"text" blocks with string .text (oracle filter_map). */
function textBlocks(contentArr) {
  const out = [];
  for (const block of contentArr) {
    if (isJsonObject(block) && block.type === "text" && typeof block.text === "string") {
      out.push(block.text);
    }
  }
  return out;
}

/**
 * Most-identifying input field for a tool call
 * (transcript_block.rs::extract_key_param). Named field first; fallback to
 * the first string-valued field in insertion order (≙ serde preserve_order).
 */
function extractKeyParam(toolName, input) {
  const fieldName = Object.prototype.hasOwnProperty.call(KEY_PARAM_FIELDS, toolName)
    ? KEY_PARAM_FIELDS[toolName]
    : "";

  if (fieldName !== "" && isJsonObject(input) && typeof input[fieldName] === "string") {
    return truncateUtf8(input[fieldName], TOOL_KEY_PARAM_BYTES);
  }

  if (isJsonObject(input)) {
    for (const key of Object.keys(input)) {
      if (typeof input[key] === "string") {
        return truncateUtf8(input[key], TOOL_KEY_PARAM_BYTES);
      }
    }
  }
  return "";
}

/**
 * Snippet text from a tool_result content block
 * (transcript_block.rs::extract_tool_result_snippet): string content, or
 * first type:"text" block in an array.
 */
function extractToolResultSnippet(toolResultBlock) {
  const content = toolResultBlock.content;
  if (typeof content === "string") {
    return truncateUtf8(content, TOOL_RESULT_SNIPPET_BYTES);
  }
  if (Array.isArray(content)) {
    for (const block of content) {
      if (isJsonObject(block) && block.type === "text" && typeof block.text === "string") {
        return truncateUtf8(block.text, TOOL_RESULT_SNIPPET_BYTES);
      }
    }
  }
  return "";
}

/**
 * Parse JSONL lines into typed exchange turns
 * (transcript_block.rs::build_exchange_pairs).
 *
 * Fail-open: malformed lines and unknown type values skipped silently.
 * Tool-use/result pairing: adjacent-record look-ahead ONLY (ADR-002).
 * Returns turns in reverse-chronological order.
 *
 * Turn shapes: {kind:"user",text} | {kind:"assistant",text} |
 * {kind:"tool",name,keyParam,resultSnippet}.
 */
function buildExchangePairs(lines) {
  const turns = [];

  let i = 0;
  while (i < lines.length) {
    const line = lines[i];

    if (line.trim() === "") {
      i += 1;
      continue;
    }

    let record;
    try {
      record = JSON.parse(line);
    } catch (_e) {
      i += 1;
      continue;
    }

    const recordType = isJsonObject(record) ? record.type : undefined;
    if (typeof recordType !== "string") {
      i += 1;
      continue;
    }

    if (recordType === "user") {
      const userTexts = textBlocks(getContentArray(record));
      if (userTexts.length > 0) {
        turns.push({ kind: "user", text: userTexts.join("\n") });
      }
      i += 1;
    } else if (recordType === "assistant") {
      const contentArr = getContentArray(record);
      const asstTexts = textBlocks(contentArr);

      const toolUses = [];
      for (const block of contentArr) {
        if (!isJsonObject(block) || block.type !== "tool_use") {
          continue;
        }
        if (typeof block.id !== "string" || typeof block.name !== "string") {
          continue;
        }
        const input = block.input === undefined ? null : block.input;
        toolUses.push({
          id: block.id,
          name: block.name,
          keyParam: extractKeyParam(block.name, input),
        });
      }

      const hasText = asstTexts.length > 0;
      const hasToolUse = toolUses.length > 0;

      // Pure thinking turn (no text, no tool_use): suppress entirely.
      if (!hasText && !hasToolUse) {
        i += 1;
        continue;
      }

      // Emit assistant text only when present (OQ-SPEC-1).
      if (hasText) {
        turns.push({ kind: "assistant", text: asstTexts.join("\n") });
      }

      // Adjacent-record look-ahead for tool_result pairing (ADR-002).
      const resultMap = Object.create(null);
      if (hasToolUse && i + 1 < lines.length) {
        const nextLine = lines[i + 1];
        if (nextLine.trim() !== "") {
          let nextRecord;
          try {
            nextRecord = JSON.parse(nextLine);
          } catch (_e) {
            nextRecord = undefined;
          }
          if (isJsonObject(nextRecord) && nextRecord.type === "user") {
            for (const block of getContentArray(nextRecord)) {
              if (!isJsonObject(block) || block.type !== "tool_result") {
                continue;
              }
              if (typeof block.tool_use_id !== "string") {
                continue;
              }
              resultMap[block.tool_use_id] = extractToolResultSnippet(block);
            }
          }
        }
      }

      for (const tu of toolUses) {
        const resultSnippet =
          resultMap[tu.id] === undefined ? "" : resultMap[tu.id];
        turns.push({
          kind: "tool",
          name: tu.name,
          keyParam: tu.keyParam,
          resultSnippet,
        });
      }

      i += 1;
    } else {
      // Unknown type: skip.
      i += 1;
    }
  }

  turns.reverse();
  return turns;
}

/** Format a single exchange turn (transcript_block.rs::format_turn). */
function formatTurn(turn) {
  if (turn.kind === "user") {
    return "[User] " + turn.text;
  }
  if (turn.kind === "assistant") {
    return "[Assistant] " + turn.text;
  }
  return "[tool: " + turn.name + "(" + turn.keyParam + ") → " + turn.resultSnippet + "]";
}

/**
 * Shared extraction core (transcript_block.rs::block_from_lines): JSONL
 * lines → exchange turns → byte-budget loop → header/body/footer block.
 * Returns null when no complete turn fits the budget (ADR-003 degradation).
 */
function blockFromLines(lines) {
  const turns = buildExchangePairs(lines);

  const outputParts = [];
  let bytesUsed = 0;
  let exchangeCount = 0;

  for (const turn of turns) {
    const turnText = formatTurn(turn);
    const turnBytes = Buffer.byteLength(turnText, "utf8");
    if (bytesUsed + turnBytes > MAX_PRECOMPACT_BYTES) {
      break;
    }
    bytesUsed += turnBytes;
    if (turn.kind === "user") {
      exchangeCount += 1;
    }
    outputParts.push(turnText);
  }

  if (outputParts.length === 0) {
    return null;
  }

  return (
    "=== Recent conversation (last " + exchangeCount + " exchanges) ===\n" +
    outputParts.join("\n") +
    "\n=== End recent conversation ==="
  );
}

/**
 * Split a tail-window buffer into lines with Rust BufRead::lines() parity:
 * - split on 0x0A at the BYTE level (terminator dropped);
 * - strip one trailing 0x0D only from \n-terminated lines (read_line pops
 *   \r only after popping \n);
 * - drop lines that are not valid UTF-8 (String::from_utf8 errs per line and
 *   the oracle's filter_map skips it). A lossy decode would keep U+FFFD
 *   lines; the round-trip check below preserves oracle parity instead.
 */
function splitLinesLikeBufRead(buf) {
  const out = [];
  if (buf.length === 0) {
    return out;
  }

  let start = 0;
  while (start <= buf.length) {
    const nl = buf.indexOf(0x0a, start);
    const terminated = nl !== -1;
    let end = terminated ? nl : buf.length;
    if (!terminated && end === start) {
      break; // buffer ended exactly on a terminator — no final line
    }
    if (terminated && end > start && buf[end - 1] === 0x0d) {
      end -= 1; // strip one trailing \r on \n-terminated lines only
    }
    const seg = buf.subarray(start, end);
    const s = seg.toString("utf8");
    if (Buffer.from(s, "utf8").equals(seg)) {
      out.push(s); // valid-UTF-8 round trip — keep
    }
    // else: drop the line (invalid UTF-8), neighbors still parsed.
    if (!terminated) {
      break;
    }
    start = nl + 1;
  }
  return out;
}

/**
 * Read the tail of the transcript file at `path`, parse as JSONL, and format
 * a restoration block within MAX_PRECOMPACT_BYTES
 * (transcript_block.rs::extract_transcript_block).
 *
 * Returns null on ANY failure — missing file, empty path, directory path,
 * permissions, read error (ADR-003 degradation contract). Never throws.
 */
function extractTranscriptBlock(path) {
  if (typeof path !== "string" || path === "") {
    return null; // no read attempted on empty/non-string path
  }
  try {
    let buf;
    const fd = fs.openSync(path, "r");
    try {
      const fileLen = fs.fstatSync(fd).size;
      const window = MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER; // 12,000
      const seekBack = Math.min(window, fileLen);
      buf = Buffer.alloc(seekBack);
      const bytesRead = fs.readSync(fd, buf, 0, seekBack, fileLen - seekBack);
      buf = buf.subarray(0, bytesRead);
    } finally {
      fs.closeSync(fd);
    }
    return blockFromLines(splitLinesLikeBufRead(buf));
  } catch (_e) {
    return null;
  }
}

module.exports = {
  MAX_PRECOMPACT_BYTES,
  TAIL_MULTIPLIER,
  extractTranscriptBlock,
  truncateUtf8,
};
