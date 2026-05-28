# Test Plan: C1 -- Dockerfile ENV Block

## Component

Remove `UNIMATRIX_CONFIG=/etc/unimatrix/config.toml` from the Dockerfile runtime ENV statement.

## Risks Covered

- **R-01** (High): Container cold start without UNIMATRIX_CONFIG ENV
- **R-06** (High): Explicit UNIMATRIX_CONFIG override breaks (partial -- C1 removes default, not the code path)

## Unit Test Expectations

No unit tests apply. The Dockerfile is not exercised by `cargo test`.

## Container Verification

### CV-01: Docker Build Succeeds
- **Arrange**: Modified Dockerfile with UNIMATRIX_CONFIG line removed from ENV block
- **Act**: `docker build -t unimatrix-nxs013-test .`
- **Assert**: Build completes with exit code 0
- **Risk**: R-01

### CV-02: docker inspect Confirms ENV Block
- **Arrange**: Built image from CV-01
- **Act**: `docker inspect --format '{{.Config.Env}}' unimatrix-nxs013-test`
- **Assert**: Output contains `HOME=/data`, `LD_LIBRARY_PATH=/usr/local/lib`, `UNIMATRIX_LOG=info`. Output does NOT contain `UNIMATRIX_CONFIG`.
- **Risk**: R-01, AC-01

### CV-03: Container Cold Start (Empty Volume)
- **Arrange**: Built image, empty `unimatrix-data` volume
- **Act**: `docker run -v unimatrix-data:/data unimatrix-nxs013-test`
- **Assert**: Daemon starts. Startup logs contain "primary config" messages (not "env override"). `write_default_config_if_absent` writes config.toml to data directory.
- **Risk**: R-01, AC-10

### CV-04: Explicit UNIMATRIX_CONFIG Override Still Works
- **Arrange**: Built image, config file at a known path
- **Act**: `docker run -e UNIMATRIX_CONFIG=/path/to/config.toml ...`
- **Assert**: Startup logs show "env override" messages, confirming the explicit env var takes precedence
- **Risk**: R-06, AC-01, NFR-04

## Code Review Checklist

- [ ] Only the `UNIMATRIX_CONFIG=...` line removed from the ENV statement
- [ ] `HOME=/data` remains in the ENV statement
- [ ] `LD_LIBRARY_PATH=/usr/local/lib` remains in the ENV statement
- [ ] `UNIMATRIX_LOG=info` remains in the ENV statement
- [ ] No other Dockerfile lines modified
- [ ] ENV statement syntax correct (continuation backslashes if multi-line)

## Edge Cases

- **HOME=/data accidentally removed**: Would break all path resolution. CV-02 catches this by asserting HOME=/data is present.
- **ENV statement syntax error after line removal**: Docker build (CV-01) would fail immediately.
