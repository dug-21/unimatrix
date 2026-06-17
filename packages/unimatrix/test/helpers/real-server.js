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
// vnc-038: the project-routing HTTP listener is HTTPS-only — `serve` always
// self-provisions a self-signed cert (http_provision::provision_tls hard-codes
// enabled=true) once `[http] enabled = true`, regardless of `[tls] enabled`. The
// harness therefore speaks HTTPS and verifies the leaf by FINGERPRINT (the OSS
// trust model — cert-pin.js), exactly as the shipped client does: complete the
// self-signed handshake with rejectUnauthorized:false, then match sha256(leaf DER)
// against the value read from {dataDir}/tls/cert.pem.
const https = require("https");
const fs = require("fs");
const os = require("os");
const path = require("path");
const crypto = require("crypto");

// vnc-027: the live UDS hook transport (the SAME module the client ships). The
// daemon starts the hook UDS listener unconditionally on `serve` at
// {dataDir}/unimatrix.sock (main.rs::start_uds_listener → paths.socket_path).
// Layer 2 UDS round-trip / FNF / delta-merge / cross-transport tests drive it
// through this real transport — cumulative infra, never a parallel framer.
const transportUds = require("../../lib/hook-client/transport-uds");

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

// Register a project slug via the `unimatrix project register <slug>` CLI so the
// HTTP listener binds on the subsequent `serve` boot (ADR-007: register writes the
// `[[projects]]` routing intent atomically; restart/boot applies it). Runs the
// SAME binary, `--project-dir`, and env (HOME / UNIMATRIX_CONFIG) the daemon uses,
// so register and serve agree on the data dir + config.toml. Async spawn (never
// spawnSync; pattern #4774). Resolves when register exits 0; rejects (hard-fail —
// #4452) on non-zero, surfacing register's stderr so a routing/genesis error is
// loud, never a silent skip.
function registerSlug(bin, projectDir, env, slug) {
  return new Promise((resolve, reject) => {
    const child = spawn(bin, ["--project-dir", projectDir, "project", "register", slug], {
      env,
      stdio: ["ignore", "pipe", "pipe"],
    });
    const out = [];
    child.stdout.on("data", (c) => out.push(c));
    child.stderr.on("data", (c) => out.push(c));
    child.on("error", reject);
    child.on("exit", (code) => {
      if (code === 0) {
        resolve();
      } else {
        reject(
          new Error(
            "`project register " +
              slug +
              "` exited " +
              code +
              "\n--- register output ---\n" +
              Buffer.concat(out).toString("utf8")
          )
        );
      }
    });
  });
}

// Compute the C2 leaf fingerprint ("sha256:" + lowercase hex of the leaf DER)
// from a provisioned PEM cert, MIRRORING cert-pin.js::computeFingerprint so a pin
// produced here matches what the client verifies on `secureConnect`. Returns null
// if the cert is unreadable/unparseable (the HTTPS in-process paths then connect
// with rejectUnauthorized:false only — sufficient for the localhost self-call).
function readCertFingerprint(certPath) {
  try {
    const pem = fs.readFileSync(certPath, "utf8");
    const x509 = new crypto.X509Certificate(pem);
    return "sha256:" + crypto.createHash("sha256").update(x509.raw).digest("hex");
  } catch (_e) {
    return null;
  }
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

// vnc-038 (ADR-003/ADR-004): observe is no longer a top-level route; it is the
// per-slug route `POST /v1/{slug}/observe`, and the HTTP listener only binds when
// at least one `[[projects]]` slug is registered (ADR-007). The Layer 2 harness
// therefore registers ONE slug into its temp data dir before `serve` and drives
// observe through `/v1/{slug}/observe`. `OBSERVE_SLUG` is that harness slug.
const OBSERVE_SLUG = "layer2";

// The per-slug observe path the client posts to verbatim (transport-http composes
// NO path — ADR-001 dumb-client). The harness's own HTTP helpers reuse it so the
// readiness probe, raw POSTs, and the spawned client all hit the SAME route.
function observePath(slug) {
  return "/v1/" + (slug || OBSERVE_SLUG) + "/observe";
}

// Poll the bound server until `POST /v1/{slug}/observe` with a Ping accepts
// (server up and HTTP listener bound — the listener only binds once a project is
// registered, ADR-007). Resolves on first non-connection response (any HTTP
// status proves the listener answered); rejects after `timeoutMs`.
function waitForServer(url, token, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  const u = new URL(url);
  return new Promise((resolve, reject) => {
    const attempt = () => {
      const body = Buffer.from(JSON.stringify({ type: "Ping" }), "utf8");
      const req = https.request(
        {
          hostname: u.hostname,
          port: u.port,
          path: observePath(),
          method: "POST",
          // Self-signed cloud cert: complete the handshake, then trust by
          // fingerprint (the harness connects to its own localhost server).
          rejectUnauthorized: false,
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

// One HTTPS POST to the per-slug observe route `{base}/v1/{slug}/observe`.
// Returns { status, contentType, body:Buffer }. `base` is the https host:port base.
function postObserve(base, token, frameObj, accept) {
  const u = new URL(base);
  const body = Buffer.from(JSON.stringify(frameObj), "utf8");
  const headers = {
    "Content-Type": "application/json",
    "Content-Length": body.length,
    Authorization: "Bearer " + token,
  };
  if (accept) headers["Accept"] = accept;
  return new Promise((resolve, reject) => {
    const req = https.request(
      {
        hostname: u.hostname,
        port: u.port,
        path: observePath(),
        method: "POST",
        rejectUnauthorized: false, // self-signed cloud cert; localhost self-call
        headers,
      },
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
 *   url            full per-slug observe URL ({base}/v1/{slug}/observe, https) —
 *                  the client posts it verbatim (ADR-001); pass as UNIMATRIX_REMOTE_URL
 *   baseUrl        https host:port base (https://127.0.0.1:{port}), no path
 *   pinnedFp       sha256:<hex> pin of the served self-signed leaf — pass as
 *                  config.pinnedFp for HTTPS in-process client transport
 *   slug           the registered harness slug the HTTP listener is bound for
 *   token          64-hex bearer token (the value the client uses)
 *   home           temp HOME (data dir lives under {home}/.unimatrix/{hash}/)
 *   projectDir     the project root passed to the server (--project-dir)
 *   post(frame, accept)  raw POST to /v1/{slug}/observe (test helper)
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

  // vnc-038 (ADR-003/ADR-004/ADR-007): the HTTP/cloud listener only binds when a
  // project slug is registered in `[[projects]]`; with none, `serve` boots as the
  // LOCAL path-hash UDS daemon and binds no HTTP TCP at all. Register the harness
  // slug FIRST so `serve` reads it on boot (register writes the stanza atomically,
  // preserving the [http] block the harness wrote). `register` is a synchronous,
  // pre-tokio subcommand; run it to completion before spawning the daemon.
  await registerSlug(bin, projectDir, env, OBSERVE_SLUG);

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

  // Base URL (https host:port — the cloud listener is TLS-only, vnc-038). Internal
  // harness helpers (waitForServer, postObserve) append the per-slug observe path
  // themselves. The EXPOSED `url` (below) is the full verbatim observe URL a client
  // posts to (ADR-001: transport-http composes no path), i.e. `{base}/v1/{slug}/observe`.
  const baseUrl = "https://" + bind + ":" + port;
  const observeUrl = baseUrl + observePath();

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

  await waitForServer(baseUrl, token, Math.max(1000, tokenDeadline - Date.now()));

  // vnc-027: the hook UDS listener binds {dataDir}/unimatrix.sock during serve
  // startup (paths.socket_path). Poll the socket file into existence so Layer 2
  // UDS suites never race the bind. Hard-fail (never skip — #4452) if it never
  // appears: that is a real regression, not a reason to silently pass.
  const socketPath = path.join(dataDir, "unimatrix.sock");
  const sockDeadline = Date.now() + Math.max(5000, startTimeoutMs);
  for (;;) {
    if (fs.existsSync(socketPath)) break;
    if (Date.now() > sockDeadline) {
      throw new Error(
        "UDS hook socket not bound within " +
          startTimeoutMs +
          "ms at " +
          socketPath +
          "\n--- server log ---\n" +
          Buffer.concat(serverLog).toString("utf8")
      );
    }
    await sleep(50);
  }

  // C2 fingerprint of the served leaf (vnc-038 HTTPS-only cloud listener). Read
  // from the provisioned {dataDir}/tls/cert.pem (present once HTTP bound) and
  // hashed exactly as cert-pin.js::computeFingerprint does (sha256 of the leaf
  // DER). In-process client paths (delta.maybeSendDelta) pass this as
  // `config.pinnedFp` so the shipped transport-http completes the self-signed
  // handshake and trusts the cert by pin — the OSS trust model, no CA.
  const pinnedFp = readCertFingerprint(path.join(dataDir, "tls", "cert.pem"));

  return {
    // The full per-slug observe URL the client posts to verbatim (ADR-001).
    // Tests pass this as UNIMATRIX_REMOTE_URL / config.url for the spawned client.
    url: observeUrl,
    // The host:port base (no path) — for tests that need to compose other routes.
    baseUrl,
    // sha256:<hex> pin of the served leaf — pass as config.pinnedFp for HTTPS
    // in-process client transport (null only if the cert could not be read).
    pinnedFp,
    // The harness slug the HTTP listener is registered/bound for (ADR-007).
    slug: OBSERVE_SLUG,
    token,
    home,
    projectDir,
    socketPath,
    serverLog: () => Buffer.concat(serverLog).toString("utf8"),
    /**
     * vnc-027 UDS connect helper — post a HookRequest frame to the live hook
     * listener over the daemon's Unix socket via the SHIPPED transport-uds
     * module (cumulative infra; identical framing/lifecycle the client uses).
     *
     * @param {object} frame  HookRequest object (serde-tagged, as for HTTP post)
     * @param {object} [opts] { sync: boolean } — sync half-closes + reads a reply
     *                        (Text/Ack/Pong/Error); FNF write-then-FIN, status 0
     * @returns {Promise<object>} SendResult { ok, status, contentType, body, failureClass }
     */
    udsPost(frame, opts) {
      return transportUds.post({ socketPath }, frame, opts || {});
    },
    /**
     * vnc-027 raw UDS connection — for adversarial framing tests (e.g. declared
     * length > bytes actually sent, then destroy: the server-side truncation
     * contract, ADR-003 §6 / R-01 s2). Resolves a net.Socket already connected.
     *
     * @returns {Promise<import("net").Socket>}
     */
    udsConnectRaw() {
      const net = require("net");
      return new Promise((resolve, reject) => {
        const sock = net.connect(socketPath, () => resolve(sock));
        sock.once("error", reject);
      });
    },
    post(frame, accept) {
      return postObserve(baseUrl, token, frame, accept);
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
      const res = await postObserve(baseUrl, token, frame, "text/plain");
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
        baseUrl,
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
        const r = await postObserve(baseUrl, token, frame, null);
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
