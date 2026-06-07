"use strict";

// Real merged-F2 server harness for Layer 2 integration suites (vnc-026,
// test-plan OVERVIEW "Integration Harness Plan" + parity-corpus.md Layer 2).
//
// Spawns the cargo-built `unimatrix serve --foreground` binary (the MERGED
// vnc-025 / PR #692 server — C-08 satisfied) with HTTP `/observe` enabled,
// isolated under a temp HOME so the data dir, db, token, and config.toml all
// land in the temp tree (project.rs::ensure_data_directory bases on
// dirs::home_dir() → $HOME/.unimatrix/{hash}/). Returns the bound URL, the
// bearer token, and a `precompact()` probe that drives the ONLY wire-observable
// buffer surface: the server-side PreCompact restoration block built from
// `TranscriptBuffer::contiguous_tail` (listener.rs::handle_compact_payload).
//
// C-07: NO server-side production changes. The TranscriptBuffer internals
// (holes / high_water / base_offset / elided_bytes) are private and #[cfg(test)]
// only — they are NOT reachable over the wire. The four pinned ADR-008
// assertions are therefore verified through their OBSERVABLE CONSEQUENCES in the
// PreCompact text/plain body (what `contiguous_tail` serves), not by reading
// struct fields. See real-server.js `precompact()` and parity-layer2.test.js.
//
// Pattern #4768 (abort-safe stub) and #4774 (async spawn, never spawnSync) are
// honoured: the server is a detached child; all I/O is async; the harness never
// blocks the libuv loop.

const { spawn } = require("child_process");
const http = require("http");
const fs = require("fs");
const os = require("os");
const path = require("path");
const crypto = require("crypto");

// Resolve the cargo-built binary. Prefer release (Layer 2 plan: `cargo build
// --release`); fall back to debug so a developer who only ran `cargo build`
// still gets a green Layer 2 run. A hard error (never a skip — vacuous-pass
// guard #4452) if neither exists.
function resolveServerBinary() {
  const root = path.resolve(__dirname, "../../../..");
  const candidates = [
    path.join(root, "target", "release", "unimatrix"),
    path.join(root, "target", "debug", "unimatrix"),
  ];
  for (const c of candidates) {
    if (fs.existsSync(c)) return c;
  }
  throw new Error(
    "Layer 2 requires a cargo-built server binary. Run `cargo build --release` " +
      "(or `cargo build`) first. Looked in:\n  " +
      candidates.join("\n  ")
  );
}

// Reserve an ephemeral port, then release it so the server can bind it. A small
// race window is acceptable for tests (same trick as stub-server.refusedPort,
// but we WANT to use the port — content_port: 0 would self-assign but the bound
// port only surfaces in the server log, which is racier to parse than this).
const net = require("net");
function reservePort() {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.once("error", reject);
    srv.listen(0, "127.0.0.1", () => {
      const { port } = srv.address();
      srv.close(() => resolve(port));
    });
  });
}

// Poll the bound server until `POST /observe` with a Ping accepts (server up and
// HTTP listener bound). Resolves on first non-connection response (any HTTP
// status proves the listener answered); rejects after `timeoutMs`.
function waitForServer(url, token, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  const u = new URL(url);
  return new Promise((resolve, reject) => {
    const attempt = () => {
      const body = Buffer.from(JSON.stringify({ type: "Ping" }), "utf8");
      const req = http.request(
        {
          hostname: u.hostname,
          port: u.port,
          path: "/observe",
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Content-Length": body.length,
            Authorization: "Bearer " + token,
          },
        },
        (res) => {
          res.resume();
          resolve(res.statusCode);
        }
      );
      req.on("error", () => {
        if (Date.now() > deadline) {
          reject(new Error("server did not become ready within " + timeoutMs + "ms"));
        } else {
          setTimeout(attempt, 50);
        }
      });
      req.end(body);
    };
    attempt();
  });
}

// One HTTP POST to `{url}/observe`. Returns { status, contentType, body:Buffer }.
function postObserve(url, token, frameObj, accept) {
  const u = new URL(url);
  const body = Buffer.from(JSON.stringify(frameObj), "utf8");
  const headers = {
    "Content-Type": "application/json",
    "Content-Length": body.length,
    Authorization: "Bearer " + token,
  };
  if (accept) headers["Accept"] = accept;
  return new Promise((resolve, reject) => {
    const req = http.request(
      { hostname: u.hostname, port: u.port, path: "/observe", method: "POST", headers },
      (res) => {
        const chunks = [];
        res.on("data", (c) => chunks.push(c));
        res.on("end", () =>
          resolve({
            status: res.statusCode,
            contentType: res.headers["content-type"] || null,
            body: Buffer.concat(chunks),
          })
        );
      }
    );
    req.on("error", reject);
    req.end(body);
  });
}

/**
 * Start the merged-F2 server with HTTP enabled, isolated under a temp HOME.
 *
 * @param {object} [opts]
 * @param {number} [opts.startTimeoutMs=30000]
 * @returns {Promise<object>} harness:
 *   url            base URL (http://127.0.0.1:{port}) — transport appends /observe
 *   token          64-hex bearer token (the value the client uses)
 *   home           temp HOME (data dir lives under {home}/.unimatrix/{hash}/)
 *   projectDir     the project root passed to the server (--project-dir)
 *   post(frame, accept)  raw POST to /observe (test helper)
 *   precompact(sessionId, opts)  drive the wire PreCompact restoration block
 *   close()        SIGTERM the child, await exit, rm the temp tree
 */
async function startRealServer(opts) {
  const options = opts || {};
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-l2-home-"));
  const projectDir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-l2-proj-"));
  // Make projectDir a git root so the client and server agree on identity if a
  // suite ever spawns the client with cwd=projectDir (not required for the raw
  // POST path, but keeps the harness reusable).
  fs.mkdirSync(path.join(projectDir, ".git"), { recursive: true });

  const port = await reservePort();
  const bind = "127.0.0.1";

  // Data dir mirrors project.rs::ensure_data_directory(projectDir, base=$HOME/.unimatrix).
  const projectHash = crypto
    .createHash("sha256")
    .update(String(path.resolve(projectDir)), "utf8")
    .digest("hex")
    .slice(0, 16);
  const dataDir = path.join(home, ".unimatrix", projectHash);
  fs.mkdirSync(dataDir, { recursive: true });

  // Minimal config.toml enabling HTTP on the reserved port, TLS off (plain HTTP
  // for the in-process test client; production terminates TLS at a proxy).
  const configToml =
    "[http]\n" +
    "enabled = true\n" +
    'bind_address = "' +
    bind +
    '"\n' +
    "content_port = " +
    port +
    "\n\n" +
    "[tls]\n" +
    "enabled = false\n";
  fs.writeFileSync(path.join(dataDir, "config.toml"), configToml);

  const bin = resolveServerBinary();
  const env = Object.assign({}, process.env, {
    HOME: home,
    USERPROFILE: home,
    // Force the config the harness wrote; do not consult the developer's global.
    UNIMATRIX_CONFIG: path.join(dataDir, "config.toml"),
  });

  const child = spawn(bin, ["--project-dir", projectDir, "serve", "--foreground"], {
    env,
    stdio: ["ignore", "pipe", "pipe"],
  });
  const serverLog = [];
  child.stdout.on("data", (c) => serverLog.push(c));
  child.stderr.on("data", (c) => serverLog.push(c));
  let exited = null;
  child.on("exit", (code, signal) => {
    exited = { code, signal };
  });

  const url = "http://" + bind + ":" + port;

  // Token is generated lazily by load_or_generate_token on HTTP startup; poll
  // the file, then poll the listener.
  const tokenPath = path.join(dataDir, "token");
  const startTimeoutMs = options.startTimeoutMs || 30000;
  const tokenDeadline = Date.now() + startTimeoutMs;
  let token = null;
  for (;;) {
    if (exited) {
      throw new Error(
        "server exited before HTTP startup (code=" +
          exited.code +
          " signal=" +
          exited.signal +
          ")\n--- server log ---\n" +
          Buffer.concat(serverLog).toString("utf8")
      );
    }
    try {
      const raw = fs.readFileSync(tokenPath, "utf8").trim();
      if (/^[0-9a-f]{64}$/.test(raw)) {
        token = raw;
        break;
      }
    } catch (_e) {
      /* not written yet */
    }
    if (Date.now() > tokenDeadline) {
      throw new Error(
        "token file not produced within " +
          startTimeoutMs +
          "ms\n--- server log ---\n" +
          Buffer.concat(serverLog).toString("utf8")
      );
    }
    await sleep(50);
  }

  await waitForServer(url, token, Math.max(1000, tokenDeadline - Date.now()));

  return {
    url,
    token,
    home,
    projectDir,
    serverLog: () => Buffer.concat(serverLog).toString("utf8"),
    post(frame, accept) {
      return postObserve(url, token, frame, accept);
    },
    /**
     * Drive the wire-observable PreCompact restoration block for a session.
     * Sends a CompactPayload with Accept: text/plain; the server reads
     * `contiguous_tail` under the per-session buffer lock, runs it through the
     * JSONL block builder, and returns it as a text/plain body
     * (handle_compact_payload → BriefingContent → observe_response_to_http).
     *
     * @returns {Promise<{status:number, contentType:string|null, text:string,
     *   raw:Buffer}>}
     */
    async precompact(sessionId, pcOpts) {
      const o = pcOpts || {};
      const frame = {
        type: "CompactPayload",
        session_id: sessionId,
        injected_entry_ids: o.injected_entry_ids || [],
        role: o.role !== undefined ? o.role : null,
        feature: o.feature !== undefined ? o.feature : null,
        token_limit: o.token_limit !== undefined ? o.token_limit : null,
      };
      if (o.transcript_excerpt !== undefined) {
        frame.transcript_excerpt = o.transcript_excerpt;
      }
      const res = await postObserve(url, token, frame, "text/plain");
      return {
        status: res.status,
        contentType: res.contentType,
        text: res.body.toString("utf8"),
        raw: res.body,
      };
    },
    /**
     * SR-11 — the ONE deterministic buffer pre-population helper (test-plan
     * OVERVIEW + parity-corpus.md). Registers `sessionId` (raw id on the wire,
     * server mints http-) and ships the given transcript `bytes` as one or more
     * `transcript_delta` frames so the merged-F2 buffer holds exactly that
     * content. Offsets advance contiguously; oversized inputs are chunked at
     * DELTA_CHUNK so a single call can pre-populate any size. Used by Layer 1
     * PreCompact restoration parity AND any Layer 2 run needing pre-population —
     * isolating ALL buffer pre-population behind this single function (SR-11 /
     * R-17: never reach into vnc-025 internals).
     *
     * @param {string} sessionId  raw session id
     * @param {Buffer|string} bytes  transcript bytes to load into the buffer
     * @returns {Promise<number>}  the final logical offset (== bytes length)
     */
    async prepopulateBuffer(sessionId, bytes) {
      const buf = Buffer.isBuffer(bytes) ? bytes : Buffer.from(String(bytes), "utf8");
      const reg = await postObserve(
        url,
        token,
        {
          type: "SessionRegister",
          session_id: sessionId,
          cwd: "/x",
          agent_role: null,
          feature: null,
        },
        null
      );
      if (reg.status !== 204) {
        throw new Error("prepopulateBuffer: SessionRegister failed (" + reg.status + ")");
      }
      const DELTA_CHUNK = 49152; // stay well under the 1 MiB frame ceiling
      let offset = 0;
      while (offset < buf.length) {
        const chunk = buf.subarray(offset, Math.min(offset + DELTA_CHUNK, buf.length));
        const frame = {
          type: "RecordEvent",
          event_type: "transcript_delta",
          session_id: sessionId,
          timestamp: Math.floor(Date.now() / 1000),
          payload: { offset, bytes: chunk.toString("utf8") },
        };
        const r = await postObserve(url, token, frame, null);
        if (r.status !== 204) {
          throw new Error("prepopulateBuffer: delta POST failed (" + r.status + ")");
        }
        offset += chunk.length;
      }
      return offset;
    },
    async close() {
      if (!exited) {
        child.kill("SIGTERM");
        const killDeadline = Date.now() + 10000;
        while (!exited && Date.now() < killDeadline) {
          await sleep(25);
        }
        if (!exited) child.kill("SIGKILL");
      }
      for (const dir of [home, projectDir]) {
        try {
          fs.rmSync(dir, { recursive: true, force: true });
        } catch (_e) {
          /* best-effort */
        }
      }
    },
  };
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

module.exports = { startRealServer, resolveServerBinary };
