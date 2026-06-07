"use strict";

/**
 * cycle-validation.js — port of `validation.rs::validate_cycle_params`
 * (:411-509) and its `validate_phase_field` helper. Split out of
 * build-request-tools.js to keep each file under the 500-line gate.
 *
 * Pure; never throws. All length checks mirror the Rust oracle: topic uses
 * BYTE length (`Buffer.byteLength`), while phase/outcome use CODE-POINT count
 * (`Array.from(...).length`) to match Rust `chars().count()`.
 */

const MAX_CYCLE_TOPIC_LEN = 128;
const MAX_PHASE_LEN = 64;
const MAX_OUTCOME_LEN = 512;
const CYCLE_START_EVENT = "cycle_start";
const CYCLE_PHASE_END_EVENT = "cycle_phase_end";
const CYCLE_STOP_EVENT = "cycle_stop";

/**
 * Validate and normalize a phase field — port of `validate_phase_field`.
 * `undefined -> null`; trim; empty -> Err; lowercase; >64 code points -> Err;
 * contains ' ' -> Err; non `[a-z0-9-_]` -> Err.
 *
 * @returns {{ok: true, value: string|null} | {ok: false}}
 */
function validatePhaseField(value) {
  if (value === undefined) {
    return { ok: true, value: null };
  }
  const trimmed = value.trim();
  if (trimmed === "") {
    return { ok: false };
  }
  const normalized = trimmed.toLowerCase();
  if (Array.from(normalized).length > MAX_PHASE_LEN) {
    return { ok: false };
  }
  if (normalized.includes(" ")) {
    return { ok: false };
  }
  if (!/^[a-z0-9\-_]+$/.test(normalized)) {
    return { ok: false };
  }
  return { ok: true, value: normalized };
}

/** Cycle-topic feature-id check — BYTE length <= 128, ASCII body (validation.rs:397). */
function isValidCycleTopic(s) {
  return (
    s !== "" &&
    Buffer.byteLength(s, "utf8") <= MAX_CYCLE_TOPIC_LEN &&
    s.includes("-") &&
    !s.startsWith("-") &&
    !s.endsWith("-") &&
    /^[A-Za-z0-9\-_.]+$/.test(s)
  );
}

/**
 * Port of `validation.rs::validate_cycle_params`.
 *
 * @returns {{ok: true, cycleType, topic, phase, outcome, nextPhase}
 *          | {ok: false}}
 */
function validateCycleParams(typeStr, topic, phase, outcome, nextPhase) {
  if (typeStr !== "start" && typeStr !== "phase-end" && typeStr !== "stop") {
    return { ok: false };
  }
  if (topic === "") {
    return { ok: false };
  }
  // Strip non-ASCII and ASCII control chars, take first 128 code points.
  const clean = Array.from(topic)
    .filter((ch) => {
      const cc = ch.codePointAt(0);
      return cc <= 0x7f && !(cc <= 0x1f || cc === 0x7f);
    })
    .slice(0, MAX_CYCLE_TOPIC_LEN)
    .join("");
  if (clean === "" || !isValidCycleTopic(clean)) {
    return { ok: false };
  }

  const phaseRes = validatePhaseField(phase);
  if (!phaseRes.ok) {
    return { ok: false };
  }
  const nextPhaseRes = validatePhaseField(nextPhase);
  if (!nextPhaseRes.ok) {
    return { ok: false };
  }

  let validatedOutcome = null;
  if (outcome !== undefined) {
    if (Array.from(outcome).length > MAX_OUTCOME_LEN) {
      return { ok: false };
    }
    for (const ch of outcome) {
      if (ch.codePointAt(0) <= 0x1f) {
        return { ok: false };
      }
    }
    validatedOutcome = outcome;
  }

  return {
    ok: true,
    cycleType: typeStr,
    topic: clean,
    phase: phaseRes.value,
    outcome: validatedOutcome,
    nextPhase: nextPhaseRes.value,
  };
}

module.exports = {
  validateCycleParams,
  validatePhaseField,
  isValidCycleTopic,
  CYCLE_START_EVENT,
  CYCLE_PHASE_END_EVENT,
  CYCLE_STOP_EVENT,
  MAX_PHASE_LEN,
  MAX_OUTCOME_LEN,
  MAX_CYCLE_TOPIC_LEN,
};
