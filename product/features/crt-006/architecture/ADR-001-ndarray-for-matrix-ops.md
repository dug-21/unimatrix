## ADR-001: ndarray for Matrix Operations

### Context

crt-006 requires matrix multiplication, transposition, element-wise operations, and gradient computation for the MicroLoRA forward and backward passes. The matrices are small (384 x rank where rank is 2-16) but are on the hot path -- every embedding operation performs at least two matrix multiplications.

Options considered:
1. **ndarray**: Heap-allocated, BLAS-transparent for larger matrices, rich API for broadcasting and views. Transitive dependency graph includes optional BLAS backends.
2. **nalgebra**: Stack-allocated small matrices with compile-time dimensions. Good for truly small matrices, but 384x16 is too large for comfortable stack allocation.
3. **Hand-written loops**: Zero allocation overhead with pre-allocated buffers. But error-prone for gradient computation, loses readability, and forgoes SIMD optimization.

### Decision

Use **ndarray** with pre-allocated buffers for the MicroLoRA engine. Specifically:
- `Array2<f32>` for weight matrices A (d x r) and B (r x d)
- `Array1<f32>` for intermediate results and gradient accumulators
- Pre-allocate all buffers at `MicroLoRA::new()` time; the forward pass reuses buffers without allocation
- No explicit BLAS backend required (ndarray's built-in routines are sufficient for these dimensions)

### Consequences

- **Easier**: Gradient computation is readable and correct (broadcasting, element-wise ops, dot products). Testing gradient correctness against finite-difference approximations is straightforward.
- **Easier**: SIMD acceleration is transparent if ndarray's feature flags enable it.
- **Harder**: ndarray adds a transitive dependency (though it is a well-maintained, widely-used crate). Must verify edition 2024 compatibility.
- **Harder**: Pre-allocation discipline required -- developers must not allocate fresh arrays in the forward pass.
