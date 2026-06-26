"use strict";

// sse-parse.js (C2 unit, R-04) — text/event-stream parser. rmcp wraps each
// JSON-RPC response in one SSE data: event (tower.rs:1140) + optional priming
// event. Multi-line data: joins with "\n"; blank line = record boundary; ":"
// lines = keep-alive. Chunk-split-invariant (records emit only on terminator).

// Local copy (avoids a dispatch<->sse-parse require cycle). Generous default;
// the caller threads its configured idle budget.
const DEFAULT_READ_MS = 120000;

class SseParser {
  // Collect every JSON-RPC object carried by data: events. Bounded at `limit`
  // bytes AND `readMs` idle-stall (#839 F6): a connect-then-stall-mid-stream
  // bounds bytes but not stall time. On idle expiry the stream is destroyed with
  // an ETIMEDOUT-coded error; the for-await rethrows it so _send maps it to the
  // SAME TRANSPORT_TIMEOUT sentinel as the connect/request path (N1 -> self-heal).
  static async collect(res, limit, readMs) {
    const messages = [];
    let buffer = "";
    let total = 0;
    let t;
    // Kept REF'd (#847): load-bearing idle deadline; cleared in the finally below.
    const arm = () => { if (t) clearTimeout(t); t = setTimeout(() => { try { res.destroy(Object.assign(new Error("MCP endpoint timed out"), { code: "ETIMEDOUT" })); } catch (_e) {} }, readMs || DEFAULT_READ_MS); };
    arm();
    try {
      for await (const chunk of res) {
        arm();
        const buf = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
        total += buf.length;
        buffer += buf.toString("utf8");
        buffer = buffer.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
        let sep;
        while ((sep = buffer.indexOf("\n\n")) !== -1) {
          const record = buffer.slice(0, sep);
          buffer = buffer.slice(sep + 2);
          SseParser._emit(record, messages);
        }
        if (total > limit) {
          try { res.destroy(); } catch (_e) {}
          break;
        }
      }
    } finally {
      if (t) clearTimeout(t);
    }
    // Flush a trailing record with no terminating blank line.
    if (buffer.trim() !== "") SseParser._emit(buffer, messages);
    return messages;
  }

  static _emit(record, out) {
    const ev = SseParser.parseRecord(record);
    if (ev.dataPayload !== "") {
      try {
        out.push(JSON.parse(ev.dataPayload));
      } catch (_e) {
        // priming / non-JSON event (retry-only) — skip.
      }
    }
  }

  static parseRecord(record) {
    const dataLines = [];
    let event;
    let id;
    for (const line of record.split("\n")) {
      if (line === "" || line.charAt(0) === ":") continue; // comment/keep-alive
      if (line.startsWith("data:")) dataLines.push(line.slice(5).replace(/^ /, ""));
      else if (line.startsWith("event:")) event = line.slice(6).trim();
      else if (line.startsWith("id:")) id = line.slice(3).trim();
      // other fields ignored
    }
    return { event, id, dataPayload: dataLines.join("\n") };
  }
}

module.exports = { SseParser };
