# nan-015 Acceptance Criteria Map

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | Docker image built without model bake-in is at least 150 MB smaller (uncompressed) than current image | shell | `docker images --format '{{.Size}}' unimatrix` before and after; compute difference >= 150 MB | PENDING |
| AC-02 | `docker-compose.yml` defines both `unimatrix-data` and `unimatrix-shared` named volumes, with `unimatrix-shared` mounted at `/shared` | shell | `docker compose config` and verify `unimatrix-shared` volume exists and is mounted at `/shared` | PENDING |
| AC-03 | On first start with empty `unimatrix-shared` volume, daemon auto-downloads both ONNX models to shared volume and enters Ready state | shell | `docker compose up -d` with fresh volumes; poll health check until healthy; `docker exec unimatrix ls /shared/models/` confirms model files present | PENDING |
| AC-04 | On subsequent starts, no model download occurs -- models loaded from shared volume | shell | `docker compose restart unimatrix`; `docker logs unimatrix` grepped for absence of download activity; startup time faster than first-run | PENDING |
| AC-05 | `unimatrix model-download` inside container writes to shared volume, not data volume | shell | `docker exec unimatrix unimatrix model-download`; verify files under `/shared/models/` and not under `/data/.cache/` | PENDING |
| AC-06 | Two containers mounting same `unimatrix-shared` volume both load models successfully | shell | Start two services with same `unimatrix-shared` volume (separate `unimatrix-data`); both pass health check | PENDING |
| AC-07 | NLI SHA-256 hash verification works with models on shared volume | test | Set `nli_model_sha256` to correct hash -- NLI Ready; set incorrect hash -- NLI degrades to cosine fallback | PENDING |
| AC-08 | Air-gap: container starts with pre-populated shared volume and `--network none` | shell | Pre-populate volume with models; `docker run --network none` with shared volume mounted; health check passes | PENDING |
| AC-09 | HEALTHCHECK continues to pass (no regression) | shell | `docker inspect --format='{{.State.Health.Status}}' unimatrix` returns `healthy` after startup | PENDING |
| AC-10 | PRODUCT-VISION.md and WAVE2-ROADMAP.md W2-1 updated to describe two-volume model | grep | `grep -c "unimatrix-shared" product/PRODUCT-VISION.md product/WAVE2-ROADMAP.md` returns non-zero for both; `grep -c "baked into" product/PRODUCT-VISION.md product/WAVE2-ROADMAP.md` returns zero for model-related context | PENDING |
| AC-11 | Documentation includes security guidance: hash pinning, `:ro` hardening, #651 gap acknowledgment | grep | `grep -l "nli_model_sha256" docker-compose.yml` and `grep -l ":ro" docker-compose.yml` and `grep -l "651" docker-compose.yml` all return matches | PENDING |
