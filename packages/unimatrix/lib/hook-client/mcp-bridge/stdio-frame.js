"use strict";

// stdio-frame.js (C2 unit, R-16) — newline-delimited JSON-RPC framing.
// Byte-split-invariant on read; write = JSON + "\n".

const PARSE_ERROR = -32700;

class StdioFramer {
  constructor(stdin, stdout) {
    this.stdin = stdin;
    this.stdout = stdout;
    this.buffer = "";
    this.cb = null;
  }

  onMessage(cb) {
    this.cb = cb;
    this.stdin.setEncoding("utf8");
    this.stdin.on("data", (chunk) => this._feed(chunk));
  }

  // Exposed for direct unit testing of the read invariants.
  _feed(chunk) {
    this.buffer += chunk;
    let idx;
    while ((idx = this.buffer.indexOf("\n")) !== -1) {
      const line = this.buffer.slice(0, idx);
      this.buffer = this.buffer.slice(idx + 1);
      if (line.trim() === "") continue;
      let msg;
      try {
        msg = JSON.parse(line);
      } catch (_err) {
        this.write({ jsonrpc: "2.0", id: null, error: { code: PARSE_ERROR, message: "parse error" } });
        continue;
      }
      if (this.cb) this.cb(msg);
    }
  }

  write(obj) {
    try {
      this.stdout.write(JSON.stringify(obj) + "\n");
    } catch (_err) {
      // stdout closed; nothing to do (host gone).
    }
  }
}

module.exports = { StdioFramer, PARSE_ERROR };
