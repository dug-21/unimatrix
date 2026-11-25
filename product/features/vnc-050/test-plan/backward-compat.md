# Backward Compatibility Matrix (FR-34..39)

Risks: R-21, R-11. ACs: AC-20..AC-24, FR-39. Old client = `@dug-21/unimatrix@0.11.3` npm tarball (integrity hash pinned, CI cache); fallback `git archive v0.11.3 packages/unimatrix`, flagged in output. All cells timeout-bounded (30 s).

## Matrix
| Client | Credential | Legacy enabled | Legacy disabled |
|---|---|---|---|
| old | v:2 pinned bundle via `init --bundle` | init+read+write OK (AC-20) | 401 disabled; existing auth error exit, no hang/loop (AC-23) |
| old | existing `remote.json` | OK; file byte-identical (AC-21) | 401, file unchanged |
| old | env observe URL+TOKEN ± fp | observe rows land (AC-22) | non-fatal, no rows |
| old | `UNIMATRIX_BUNDLE` | N/A — var unknown to 0.11.3 | N/A |
| new | v:2 pinned bundle (`UNIMATRIX_BUNDLE` and `init --bundle`) | pinned branch, no CA path (AC-24) | 401 disabled |
| new | existing `remote.json` | OK; byte-identical | 401 |
| new | env observe ± fp (CA when unset) | rows land | non-fatal |
| new | `UNIMATRIX_BUNDLE` scoped | covered in bridge-transports.md | scoped unaffected |

## Tests
- `compat_matrix_legacy_enabled`, `compat_matrix_legacy_disabled`: iterate above cells; assert status/exit/rows/file sha256.
- `old_client_rejects_fp_null_loudly`: 0.11.3 `init --bundle <fp:null bundle>` → non-zero, no file written.
- `fr39_legacy_response_diff`: fixed request set (initialize, search, store, observe, bad token, `/health`) with legacy token against main-built server vs new server; normalize ids/timestamps; diff of status, header names, body shape empty. Disabled body is the only allowed diff and only with legacy disabled.
- `pinned_fixture_bundle_frozen`: pre-vnc-050 bundle bytes checked in; decoder and attach unchanged.
