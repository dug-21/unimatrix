#!/usr/bin/env node
"use strict";

/**
 * index.js — hook-client entry / dispatch (vnc-026, F3).
 *
 * Per-spawn entry: `node /abs/path/lib/hook-client/index.js <EVENT>`.
 * Mirrors crates/unimatrix-server/src/uds/hook.rs::run() step-for-step with the
 * OVERVIEW-documented deviations (HTTP transport, ADR-003 queue, client-side
 * deltas, no client PreCompact transcript prepend). Guarantees exit code 0 and
 * zero stdout on every failure path (C-05). The ONLY stdout writer in the whole
 * client is transform.js; index.js never calls console.log.
 *
 * Oracles (read-only): hook.rs::run / read_stdin / parse_hook_input /
 * resolve_cwd, sync/FNF split at hook.rs:244-251.
 *
 * Every state/queue/breadcrumb/delta/transport call is internally non-throwing;
 * the top-level try/catch is a last-resort guard, never the primary mechanism.
 */

const fs = require("fs");

const normalize = require("./normalize");
const configMod = require("./config");
const buildRequestMod = require("./build-request");
const transcript = require("./transcript");
const transport = require("./transport-http");
const transform = require("./transform");
const delta = require("./delta");
const queue = require("./queue");
const state = require("./state");

/** stdin hard cap — parity with hook.rs read_stdin take(1 MiB). */
const STDIN_CAP = 1048576;

/** Named HookInput fields (port of wire.rs HookInput); all else → extra. */
const NAMED = [
  "hook_event_name",
  "session_id",
  "cwd",
  "transcript_path",
  "prompt",
  "provider",
  "mcp_context",
];

/** All-empty HookInput with extra=null (serde parse-failure fallback). */
function emptyInput() {
  return {
    hook_event_name: "",
    session_id: null,
    cwd: null,
    transcript_path: null,
    prompt: null,
    provider: null,
    mcp_context: null,
    extra: null,
  };
}

/**
 * Read all of stdin via fd 0 (FR-01, R-14). The numeric fd is required: the
 * device-path form throws on Windows, so it is never used. Caps at 1 MiB.
 * EOF/EAGAIN on a console fd 0 (Windows) → "".
 * @returns {string}
 */
function readStdin() {
  let buf;
  try {
    buf = fs.readFileSync(0);
  } catch (_err) {
    return "";
  }
  if (buf.length > STDIN_CAP) buf = buf.subarray(0, STDIN_CAP);
  return buf.toString("utf8");
}

/** True iff v is a non-null, non-array plain object. */
function isPlainObject(v) {
  return v !== null && typeof v === "object" && !Array.isArray(v);
}

/** True iff v is a string or null/undefined (serde Option<String> shape). */
function isStringOrNull(v) {
  return v === null || v === undefined || typeof v === "string";
}

/**
 * Defensive parse — port of hook.rs::parse_hook_input + wire.rs HookInput serde
 * semantics. serde fails the WHOLE parse if any named field has a wrong type;
 * the fallback is all-empty HookInput with extra=null. Unknown stdin keys
 * survive verbatim in `extra` (insertion order — ass-071 / wire.rs flatten
 * parity); a clean parse with no unknown keys yields extra={}.
 * @returns {object} HookInput
 */
function parseHookInput(raw) {
  let obj;
  try {
    obj = JSON.parse(raw);
  } catch (_err) {
    if (raw !== "") stderrLine("parse", "stdin parse error");
    return emptyInput();
  }
  if (!isPlainObject(obj)) {
    if (raw !== "") stderrLine("parse", "stdin parse error");
    return emptyInput();
  }
  // serde type-check of named fields (whole-parse failure on any violation):
  if (
    !(obj.hook_event_name === undefined || typeof obj.hook_event_name === "string") ||
    !isStringOrNull(obj.session_id) ||
    !isStringOrNull(obj.cwd) ||
    !isStringOrNull(obj.transcript_path) ||
    !isStringOrNull(obj.prompt) ||
    !isStringOrNull(obj.provider)
    // mcp_context: Option<Value> — any JSON value is valid, no check.
  ) {
    stderrLine("parse", "stdin field type error");
    return emptyInput();
  }

  const out = {
    hook_event_name: obj.hook_event_name !== undefined ? obj.hook_event_name : "",
    session_id: obj.session_id !== undefined ? obj.session_id : null,
    cwd: obj.cwd !== undefined ? obj.cwd : null,
    transcript_path: obj.transcript_path !== undefined ? obj.transcript_path : null,
    prompt: obj.prompt !== undefined ? obj.prompt : null,
    provider: obj.provider !== undefined ? obj.provider : null,
    mcp_context: obj.mcp_context !== undefined ? obj.mcp_context : null,
    extra: {},
  };
  for (const key of Object.keys(obj)) {
    if (NAMED.indexOf(key) === -1) out.extra[key] = obj[key];
  }
  return out;
}

/**
 * Resolve working directory — port of hook.rs::resolve_cwd minus --project-dir
 * (no flag in F3). stdin.cwd (non-empty) wins over process.cwd().
 * @returns {string}
 */
function resolveCwd(input) {
  if (typeof input.cwd === "string" && input.cwd.length > 0) return input.cwd;
  try {
    return process.cwd();
  } catch (_err) {
    return ".";
  }
}

/** ADR-005 stderr one-liner. Wrapped: stderr write failure must not throw. */
function stderrLine(cls, message) {
  try {
    process.stderr.write("unimatrix: " + cls + ": " + message + "\n");
  } catch (_err) {
    // swallow — never affects exit code
  }
}

/** Extract the session id from any HookRequest frame (build-request applied the
 *  ppid fallback already, so this is always present for FNF frames). */
function sessionIdOf(request) {
  switch (request.type) {
    case "SessionRegister":
    case "SessionClose":
    case "RecordEvent":
      return request.session_id;
    case "RecordEvents":
      return request.events && request.events[0] ? request.events[0].session_id : null;
    default:
      return request.session_id !== undefined ? request.session_id : null;
  }
}

/**
 * Settle a transport result from Promise.allSettled. A rejected promise (the
 * transport never rejects, but allSettled is defensive) becomes a synthetic
 * connect failure so independence holds (AC-09).
 */
function settledSendResult(settled) {
  if (settled && settled.status === "fulfilled" && settled.value) return settled.value;
  return { ok: false, status: 0, contentType: null, body: null, failureClass: "connect" };
}

/**
 * Settle a delta outcome from Promise.allSettled. delta.maybeSendDelta resolves
 * { attempted, send? } — a rejected promise degrades to a non-attempt.
 * @returns {object|null} the DeltaOutcome, or null when no delta task ran.
 */
function settledDeltaOutcome(settled) {
  if (!settled) return null;
  if (settled.status === "fulfilled" && settled.value) return settled.value;
  return { attempted: false, reason: "rejected" };
}

/**
 * Synchronous path — ContextSearch | CompactPayload | Ping. NO queue replay, NO
 * delta, NO transcript I/O (the SubagentStart tail read already happened
 * pre-dispatch; C-03 / R-13). Exactly one POST.
 */
async function runSync(request, reqSource, config) {
  const res = await transport.post(config, request, { sync: true }); // Accept: text/plain
  transform.writeSyncOutput(reqSource, res); // stdout iff 200 text/plain non-empty
  state.recordSendOutcomes(config.stateDir, config.urlHost, [res], queue.queueDepth(config.stateDir));
  if (!res.ok) stderrLine(res.failureClass, "sync request failed");
}

/**
 * Fire-and-forget path — SessionRegister | SessionClose | RecordEvent |
 * RecordEvents. Replay-before-send (best-effort, does NOT gate the carrying
 * POST — Rust parity), then carrying POST and the delta POST concurrently
 * (ADR-007, Promise.allSettled — independent outcomes, AC-09).
 */
async function runFireAndForget(request, input, config) {
  queue.prune(config.stateDir); // 24 h age prune (wrapped, best-effort)
  await queue.replay(config, transport.post); // ≤32 frames / 256 KiB, stop-at-first-failure

  const sessionId = sessionIdOf(request);

  const tasks = [transport.post(config, request, { sync: false })]; // fnfMs timeout
  const hasTranscript =
    typeof input.transcript_path === "string" && input.transcript_path.length > 0;
  if (hasTranscript) {
    tasks.push(
      delta.maybeSendDelta(input.transcript_path, sessionId, input.provider, config)
    );
  }

  const results = await Promise.allSettled(tasks);

  const carrying = settledSendResult(results[0]);
  if (!carrying.ok) {
    queue.enqueue(config.stateDir, request); // NEVER a delta frame here (ADR-004)
    stderrLine(carrying.failureClass, "send failed, event queued");
  } else if (request.type === "SessionClose") {
    state.deleteOffset(config.stateDir, sessionId); // FR-16 lifecycle (wrapped)
  }

  // Delta outcome → breadcrumb only when a POST was actually attempted.
  const deltaOutcome = results.length > 1 ? settledDeltaOutcome(results[1]) : null;
  const deltaSend =
    deltaOutcome && deltaOutcome.attempted === true ? deltaOutcome.send : null;
  if (deltaSend && !deltaSend.ok) {
    stderrLine(deltaSend.failureClass, "transcript delta send failed");
  }

  state.recordSendOutcomes(
    config.stateDir,
    config.urlHost,
    [carrying, deltaSend],
    queue.queueDepth(config.stateDir)
  );
}

/** Map a config-miss reason to a breadcrumb/stderr failure class. */
function classForReason(reason) {
  return reason === "partial_env" ? "auth" : "connect";
}

/** Human-readable description of a config-miss reason (no secrets). */
function describeReason(reason) {
  switch (reason) {
    case "partial_env":
      return "only one of UNIMATRIX_REMOTE_URL / UNIMATRIX_REMOTE_TOKEN set";
    case "malformed":
      return "settings.local.json is not valid JSON";
    case "missing":
    default:
      return "no remote config (env vars or settings.local.json unimatrix.remote)";
  }
}

/**
 * main — top-level pipeline, invoked immediately. Always resolves; never calls
 * process.exit() with a nonzero code; never writes stdout on a failure path.
 */
async function main() {
  try {
    const rawEvent = process.argv[2] || "";
    const raw = readStdin(); // never throws
    const input = parseHookInput(raw); // never throws

    const normalized = normalize.normalizeEventName(rawEvent);
    const canonical = normalized[0];
    const providerStr = normalized[1];
    // hook.rs run() step 2b: overwrite provider from inference (no --provider in F3).
    input.provider = providerStr;
    const effectiveEvent = canonical === normalize.UNKNOWN_EVENT ? rawEvent : canonical;

    const cwd = resolveCwd(input);
    const config = configMod.resolve(cwd); // yields stateDir/projectHash too

    if (!config.ok) {
      // ADR-006: breadcrumb + stderr, exit 0, NO network.
      stderrLine(classForReason(config.reason), describeReason(config.reason));
      state.writeBreadcrumb(config.stateDir, {
        failureClass: classForReason(config.reason),
      });
      return; // exit 0
    }

    let request = buildRequestMod.buildRequest(effectiveEvent, input); // pure

    // SubagentStart fallback — hook.rs run() step 5b. Claude Code does not send
    // prompt_snippet, so build_request returns RecordEvent; derive the query
    // from the transcript tail (the sole sync-path-exempt file read, RQ-6).
    if (effectiveEvent === "SubagentStart" && request.type === "RecordEvent") {
      const role =
        isPlainObject(input.extra) && typeof input.extra.agent_type === "string"
          ? input.extra.agent_type
          : null;
      const query =
        typeof input.transcript_path === "string" && input.transcript_path.length > 0
          ? transcript.extractTranscriptBlock(input.transcript_path)
          : null;
      if (query !== null && query !== undefined) {
        request = {
          type: "ContextSearch",
          query,
          session_id: input.session_id,
          role,
          task: null,
          feature: null,
          k: null,
          max_tokens: null,
          source: "SubagentStart", // source PRESENT (omit-when-null rule)
        };
      }
    }

    const reqSource =
      request.type === "ContextSearch" && request.source !== undefined
        ? request.source
        : null;

    const isFnf =
      request.type === "SessionRegister" ||
      request.type === "SessionClose" ||
      request.type === "RecordEvent" ||
      request.type === "RecordEvents";

    if (isFnf) {
      await runFireAndForget(request, input, config);
    } else {
      await runSync(request, reqSource, config);
    }
  } catch (e) {
    // Last-resort guard: NEVER stdout, NEVER nonzero exit.
    stderrLine("internal", String((e && e.message) || e));
  }
  // No process.exit() — let the event loop drain; exitCode stays 0.
}

module.exports = {
  main,
  readStdin,
  parseHookInput,
  resolveCwd,
  sessionIdOf,
  settledSendResult,
  settledDeltaOutcome,
  runSync,
  runFireAndForget,
  STDIN_CAP,
};

// Run only when invoked directly as the hook command (not when require()d by
// tests). require.main === module is the standard CommonJS entry guard.
if (require.main === module) {
  main();
}
