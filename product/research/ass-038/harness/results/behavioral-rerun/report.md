# Unimatrix Eval Report

Generated: 1775088950 (unix epoch) | Scenarios: 1443

## 1. Summary

| Profile | Scenarios | P@K | MRR | CC@k | ICD (max=ln(n)) | Avg Latency (ms) | ΔP@K | ΔMRR | ΔCC@k | ΔICD | ΔLatency (ms) |
|---------|-----------|-----|-----|------|----------------|-----------------|------|------|--------|------|---------------|
| combined-ppr-disabled | 1443 | 0.1117 | 0.2913 | 0.4524 | 0.6637 | 5.8 | — | — | — | — | — |
| combined-ppr-enabled | 1443 | 0.1117 | 0.2913 | 0.4524 | 0.6637 | 6.2 | — | — | — | — | +0.4 |

## 2. Notable Ranking Changes

### obs-02b88d42-1773938074000

**Query**: crate boundary architecture embed server core  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 71: ADR-001: Core Crate as Trait H | 71: ADR-001: Core Crate as Trait H |
| 2 | 747: Cross-Crate Test Infrastructur | 747: Cross-Crate Test Infrastructur |
| 3 | 729: Intelligence pipeline testing  | 729: Intelligence pipeline testing  |
| 4 | 69: ADR-003: hf-hub Crate for Mode | 69: ADR-003: hf-hub Crate for Mode |
| 5 | 68: ADR-002: Raw ort + tokenizers  | 68: ADR-002: Raw ort + tokenizers  |

### obs-02b88d42-1773938076000

**Query**: async wrapper service handle pattern  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 316: ServiceLayer extraction patter | 316: ServiceLayer extraction patter |
| 2 | 1560: Background-tick state cache pa | 1560: Background-tick state cache pa |
| 3 | 323: How to add a new service to Se | 323: How to add a new service to Se |
| 4 | 3213: Arc startup resource threading | 3213: Arc startup resource threading |
| 5 | 2553: Changing a service constructor | 2553: Changing a service constructor |

### obs-02b88d42-1773944977000

**Query**: rayon thread pool tokio bridge spawn_blocking inference  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 2571: Rayon-Tokio bridge pattern for | 2571: Rayon-Tokio bridge pattern for |
| 2 | 3353: Rayon worker threads have no T | 3353: Rayon worker threads have no T |
| 3 | 3354: Pre-fetch Vec&lt;EntryRecord&g | 3354: Pre-fetch Vec&lt;EntryRecord&g |
| 4 | 2742: Collect owned data before rayo | 2742: Collect owned data before rayo |

### obs-02b88d42-1773947580000

**Query**: lesson-learned failures gate rejection  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 3579: Gate 3b: implementation wave d | 3579: Gate 3b: implementation wave d |
| 2 | 1006: ADR-003: One Composite Steward | 1006: ADR-003: One Composite Steward |
| 3 | 2758: Gate 3c: always grep non-negot | 2758: Gate 3c: always grep non-negot |
| 4 | 1203: Gate Validators Must Check All | 1203: Gate Validators Must Check All |
| 5 | 2577: validate() implementation requ | 2577: validate() implementation requ |

### obs-02b88d42-1773947581000

**Query**: lesson-learned failures gate rejection  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 3579: Gate 3b: implementation wave d | 3579: Gate 3b: implementation wave d |
| 2 | 1006: ADR-003: One Composite Steward | 1006: ADR-003: One Composite Steward |
| 3 | 2758: Gate 3c: always grep non-negot | 2758: Gate 3c: always grep non-negot |
| 4 | 1203: Gate Validators Must Check All | 1203: Gate Validators Must Check All |
| 5 | 2577: validate() implementation requ | 2577: validate() implementation requ |

### obs-02b88d42-1773947582000

**Query**: risk pattern  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 1007: ADR-004: Separate /store-patte | 1007: ADR-004: Separate /store-patte |
| 2 | 1544: ADR-002 crt-018b: Hold (Not In | 1544: ADR-002 crt-018b: Hold (Not In |
| 3 | 951: ADR-002: Deterministic Example | 951: ADR-002: Deterministic Example |
| 4 | 1616: Background Tick Dedup Flags: W | 1616: Background Tick Dedup Flags: W |
| 5 | 3890: ADR-001 (crt-035): Bidirection | 3890: ADR-001 (crt-035): Bidirection |

### obs-02b88d42-1773947591000

**Query**: rayon tokio oneshot channel CPU inference thread  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 2571: Rayon-Tokio bridge pattern for | 2571: Rayon-Tokio bridge pattern for |
| 2 | 3353: Rayon worker threads have no T | 3353: Rayon worker threads have no T |
| 3 | 2728: Rayon W1-2 Compliance Test Pat | 2728: Rayon W1-2 Compliance Test Pat |
| 4 | 2543: Rayon panic_handler required t | 2543: Rayon panic_handler required t |

### obs-02b88d42-1773947592000

**Query**: ort ONNX release candidate pinned version dependency  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 68: ADR-002: Raw ort + tokenizers  | 68: ADR-002: Raw ort + tokenizers  |
| 2 | 2804: ONNX-backed components: use fe | 2804: ONNX-backed components: use fe |
| 3 | 1208: Procedure: Creating a Unimatri | 1208: Procedure: Creating a Unimatri |
| 4 | 2798: ONNX cross-encoder token_type_ | 2798: ONNX cross-encoder token_type_ |
| 5 | 2805: Lazy-loading ONNX service hand | 2805: Lazy-loading ONNX service hand |

### obs-02b88d42-1773947756000

**Query**: rayon thread pool tokio oneshot bridge ML inference spawn_blocking  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 2571: Rayon-Tokio bridge pattern for | 2571: Rayon-Tokio bridge pattern for |
| 2 | 3353: Rayon worker threads have no T | 3353: Rayon worker threads have no T |
| 3 | 2742: Collect owned data before rayo | 2742: Collect owned data before rayo |
| 4 | 3354: Pre-fetch Vec&lt;EntryRecord&g | 3354: Pre-fetch Vec&lt;EntryRecord&g |

### obs-02b88d42-1773947763000

**Query**: OrtSession EmbedAdapter thread safety Send async wrappers embedding call site migration  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 2524: AsyncEmbedService is unused in | 2524: AsyncEmbedService is unused in |
| 2 | 68: ADR-002: Raw ort + tokenizers  | 68: ADR-002: Raw ort + tokenizers  |
| 3 | 1146: ADR-004: Re-Embedding After DB | 1146: ADR-004: Re-Embedding After DB |
| 4 | 2492: NLI model integration: W1-2 ar | 2492: NLI model integration: W1-2 ar |
| 5 | 2554: Accessing ml_inference_pool fr | 2554: Accessing ml_inference_pool fr |

## 3. Latency Distribution

| ≤ ms | Count |
|------|-------|
| 50 | 2886 |
| 100 | 0 |
| 200 | 0 |
| 500 | 0 |
| 1000 | 0 |
| 2000 | 0 |
| > 2000 | 0 |

## 4. Entry-Level Analysis

_No entry rank changes recorded._

## 5. Zero-Regression Check

**No regressions detected.** All candidate profiles maintain or improve MRR and P@K across all scenarios.

## 7. Distribution Analysis

_ICD is raw Shannon entropy (natural log). Maximum value is ln(n_categories).
Values are comparable across profiles run with the same configured categories._

### CC@k Range by Profile

| Profile | Scenarios | Min | Max | Mean |
|---------|-----------|-----|-----|------|
| combined-ppr-disabled | 1443 | 0.2000 | 1.0000 | 0.4524 |
| combined-ppr-enabled | 1443 | 0.2000 | 1.0000 | 0.4524 |

### ICD Range by Profile (max=ln(n))

| Profile | Scenarios | Min | Max | Mean |
|---------|-----------|-----|-----|------|
| combined-ppr-disabled | 1443 | 0.0000 | 1.6094 | 0.6637 |
| combined-ppr-enabled | 1443 | 0.0000 | 1.6094 | 0.6637 |
