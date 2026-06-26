"use strict";

// bridge-cycle-capture.js (nan-022 C2') — per-dimension MCP-bridge-surface CAPTURE
// helpers for the HTTPS leg, split out of bridge-cycle-driver.js (the ≤500-line /
// single-responsibility rule — mirrors the Python `parity_legs_capture.py` split of
// `parity_legs.py`). This module owns the retrieval (D1) + briefing (D4) capture
// PARSING; the driver owns the bridge spawn + JSON-RPC framing and calls into here.
//
// PARITY ORACLE (byte-for-byte): every parser here mirrors the canonical Python
// capture in harness/parity_legs_capture.py — same RANKED id ordering, same
// score-presence semantics (scores present ONLY when EVERY entry carries one, else
// null so K3 degrades to membership-only), same tolerant entries/results shapes.
// A divergence in id/score ordering here is a silent cross-leg parity defect — the
// shapes are pinned to the UDS leg, not invented (ADR-005 / parity_bundle_contract).
//
// NO transport/cert/spawn code lives here — it consumes ONLY the driver's `rpc` /
// `toolCall` / `resultText` machinery over the EXISTING bridge session (C-2). A
// net-new transport path would be a fork smell to FLAG, not add.

// The kwargs the typed UDS client's context_* methods accept (uds_client.py). The
// Python leg pops positional args (query/role/task/id) then `_clean`s the rest to
// THIS whitelist before the MCP call; we mirror it so the JS `tools/call` arguments
// dict is byte-identical to the UDS leg's MCP arguments (so the server ranks the
// IDENTICAL request and any cross-leg diff is a transport effect, not a request diff).
const ALLOWED_ARG_KEYS = new Set([
  "topic", "category", "tags", "k", "id", "limit", "status",
  "agent_id", "format", "feature", "helpful", "max_tokens",
]);

function cleanArgs(args) {
  const out = {};
  for (const k of Object.keys(args || {})) {
    if (ALLOWED_ARG_KEYS.has(k)) out[k] = args[k];
  }
  return out;
}

// Extract (ids, scores|null) in RANKED order from a list of entry dicts. Mirrors
// `_ids_scores_from_entries`: each entry's `id` is the ranked key; `score`/
// `similarity` (when present on EVERY entry) is the aligned score, else null.
function idsScoresFromEntries(entries) {
  if (!Array.isArray(entries)) return { ids: [], scores: null };
  const ids = [];
  const scores = [];
  let haveScores = true;
  for (const e of entries) {
    if (!e || typeof e !== "object") {
      haveScores = false;
      continue;
    }
    ids.push(e.id !== undefined ? e.id : null);
    const sc = e.score !== undefined ? e.score : e.similarity;
    if (sc === undefined || sc === null) {
      haveScores = false;
    } else {
      scores.push(sc);
    }
  }
  return { ids, scores: haveScores && scores.length ? scores : null };
}

// Parse a context_search/lookup result into (result_ids, scores|null) in RANKED
// order. Mirrors `_parse_ranked_result`: the leg requests format="json"; tolerant of
// top-level {entries:[...]} / {results:[...]} or a bare [...]. Unparseable JSON yields
// an empty ranking so the Python emptiness guard names it INFRA, never a vacuous pass.
function parseRankedResult(text) {
  let doc;
  try {
    doc = JSON.parse(text);
  } catch (_e) {
    return { ids: [], scores: null };
  }
  let entries;
  if (doc && typeof doc === "object" && !Array.isArray(doc)) {
    entries = doc.entries;
    if (entries === undefined && Array.isArray(doc.results)) entries = doc.results;
  } else {
    entries = doc;
  }
  return idsScoresFromEntries(entries);
}

// Parse a context_briefing result into (briefing_ids, scores|null, injection_set).
// Mirrors `_parse_briefing_result`: ranked entries (entries/results) + the injected
// set (injection_set/injected, mapped to ids). Unparseable JSON yields empties so the
// Python emptiness guard names it INFRA.
function parseBriefingResult(text) {
  let doc;
  try {
    doc = JSON.parse(text);
  } catch (_e) {
    return { ids: [], scores: null, injection_set: [] };
  }
  if (!doc || typeof doc !== "object" || Array.isArray(doc)) {
    return { ids: [], scores: null, injection_set: [] };
  }
  const entries = doc.entries || doc.results || [];
  const { ids, scores } = idsScoresFromEntries(entries);
  const injected = doc.injection_set || doc.injected || [];
  const injection_set = (Array.isArray(injected) ? injected : []).map((e) =>
    e && typeof e === "object" ? (e.id !== undefined ? e.id : null) : e
  );
  return { ids, scores, injection_set };
}

// Build the MCP `arguments` for a retrieval manifest call, byte-identical to what the
// Python UDS leg sends (capture_retrieval): default format="json", whitelist the rest.
// `query`/`id` ride through as-is (they ARE accepted MCP arguments — the Python leg
// pops them only to pass positionally, then the server receives them under the same
// keys). context_get's id is coerced to int to mirror `int(args.pop("id"...))`.
function retrievalArgs(call) {
  const a = Object.assign({}, call.args);
  if (a.format === undefined) a.format = "json";
  if (call.name === "context_get") {
    const rawId = a.id !== undefined ? a.id : a.entry_id;
    const id = parseInt(rawId, 10);
    const cleaned = cleanArgs(a);
    cleaned.id = Number.isNaN(id) ? 0 : id;
    return cleaned;
  }
  // search/lookup: query (when present) is an accepted arg; keep it + the whitelist.
  const cleaned = cleanArgs(a);
  if (a.query !== undefined) cleaned.query = a.query;
  return cleaned;
}

// Build the MCP `arguments` for a briefing manifest call, byte-identical to the Python
// UDS leg (capture_briefing): role defaults to "tester", task rides through, format
// defaults to "json", the rest is whitelisted.
function briefingArgs(call) {
  const a = Object.assign({}, call.args);
  if (a.format === undefined) a.format = "json";
  const role = a.role !== undefined ? a.role : "tester";
  const task = a.task !== undefined ? a.task : "";
  const cleaned = cleanArgs(a);
  cleaned.role = role;
  cleaned.task = task;
  return cleaned;
}

// Drive the retrieval (D1) query set over the EXISTING bridge session. `drive` is a
// closure (id => envelope) -> Promise resolving the bridge JSON-RPC response;
// `resultText` extracts the tool-result text. Returns a list of
// {tool, args, result_ids, scores} — result_ids in RANKED order, scores aligned (or
// null). A short/empty result is emitted AS-IS (never padded) so the Python degenerate-
// corpus / never-empty guard classifies it INFRA, never a vacuous pass (R-06/R-09).
async function driveRetrieval(retrievalCalls, drive, resultText) {
  const out = [];
  for (const call of retrievalCalls) {
    const args = retrievalArgs(call);
    const resp = await drive(call.name, args);
    const { ids, scores } = parseRankedResult(resultText(resp));
    out.push({ tool: call.name, args: call.args, result_ids: ids, scores });
  }
  return out;
}

// Extract the Informs edge-ID list from a parsed RetrospectiveReport (the analytics
// review document). Mirrors the Python `read_informs_edges`: tolerant of
// `informs_edges` / `edges`; dict entries -> id/edge_id, scalars pass through. Compared
// UNORDERED, ids exact (NFR-6). A non-report / absent edges yields [].
function informsEdgesFromReport(report) {
  if (!report || typeof report !== "object") return [];
  let edges = report.informs_edges;
  if (edges === undefined || edges === null) edges = report.edges || [];
  if (!Array.isArray(edges)) return [];
  return edges.map((e) =>
    e && typeof e === "object"
      ? (e.id !== undefined ? e.id : e.edge_id !== undefined ? e.edge_id : null)
      : e
  );
}

// Extract the per-phase signal from a MetricVector dict. Mirrors the Python
// `read_phase_signal`: returns the `phases` mapping (or {} if absent). Compared
// EXACTLY (NFR-6).
function phaseSignalFromMetricVector(metricVector) {
  if (!metricVector || typeof metricVector !== "object") return {};
  const phases = metricVector.phases;
  return phases && typeof phases === "object" && !Array.isArray(phases) ? phases : {};
}

// Drive the briefing (D4) query set over the EXISTING bridge session. Returns
// {ids, scores|null, injection_set} aggregated across the briefing calls — mirrors
// the Python capture_briefing aggregation (extend ids/injection; scores null if ANY
// call lacked scores).
async function driveBriefing(briefingCalls, drive, resultText) {
  const ids = [];
  const injection_set = [];
  let scores = [];
  let haveScores = true;
  for (const call of briefingCalls) {
    const args = briefingArgs(call);
    const resp = await drive("context_briefing", args);
    const parsed = parseBriefingResult(resultText(resp));
    for (const i of parsed.ids) ids.push(i);
    for (const i of parsed.injection_set) injection_set.push(i);
    if (parsed.scores === null) {
      haveScores = false;
    } else {
      scores = scores.concat(parsed.scores);
    }
  }
  return { ids, scores: haveScores && scores.length ? scores : null, injection_set };
}

module.exports = {
  ALLOWED_ARG_KEYS,
  cleanArgs,
  idsScoresFromEntries,
  parseRankedResult,
  parseBriefingResult,
  retrievalArgs,
  briefingArgs,
  informsEdgesFromReport,
  phaseSignalFromMetricVector,
  driveRetrieval,
  driveBriefing,
};
