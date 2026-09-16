// @ts-check
/**
 * model.js — backend model resolution + validation (AC-06, R-15).
 *
 * OpenCode carries the in-process model as `{ providerID, modelID }` (e.g.
 * `{ providerID: "ollama", modelID: "qwen3-coder" }`). The wire carrier is the
 * string `"<providerID>/<modelID>"` passed via `--model`, matching the Rust
 * `is_valid_model_id` charset (wire.rs): `^[a-z0-9._/-]{1,128}$` — distinct from
 * the narrower `source_domain` contract because it contains `/`.
 *
 * R-15 (untrusted input): the model value is validated at this shim boundary
 * before it is ever placed on the command line. An invalid value is REJECTED
 * (resolveModel returns undefined → `--model` omitted → model_id NULL), never
 * passed raw. Authoritative validation is also enforced server-side (C4/C5); this
 * is defense in depth.
 */

/** Mirrors Rust `is_valid_model_id` (wire.rs, vnc-049 C4). */
export const MODEL_ID_RE = /^[a-z0-9._/-]{1,128}$/;

/**
 * @param {unknown} value
 * @returns {boolean}
 */
export function isValidModelId(value) {
  return typeof value === "string" && MODEL_ID_RE.test(value);
}

/**
 * Resolve an OpenCode `{ providerID, modelID }` to the validated wire string.
 * Returns undefined when the model is absent (cloud/unspecified) or fails the
 * carrier charset — the caller then omits `--model`.
 *
 * @param {unknown} model - typically `{ providerID: string, modelID: string }`
 * @returns {(string|undefined)}
 */
export function resolveModel(model) {
  if (!model || typeof model !== "object") {
    return undefined;
  }
  const m = /** @type {any} */ (model);
  if (typeof m.providerID !== "string" || typeof m.modelID !== "string") {
    return undefined;
  }
  if (!m.providerID || !m.modelID) {
    return undefined;
  }
  const composed = m.providerID + "/" + m.modelID;
  return isValidModelId(composed) ? composed : undefined;
}
