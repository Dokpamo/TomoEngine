# TomoEngine contributor instructions

## Non-negotiable project intent

Implement the pinned Jolt algorithms in Rust. Do not substitute Jolt bindings, Rapier,
a C++ subprocess, a native library, generated C/C++, or a renamed toy simulation.
The current implementation is a small foundation, NOT a complete physics engine.

- Read `docs/PORTING.md` and `upstream/jolt.toml` before extending a module.
- Read the pinned upstream implementation, not just its name or documentation.
- Preserve Jolt copyright and MIT permission notices for derived source/tests.
- Keep the engine crate `#![forbid(unsafe_code)]`. Propose any policy change explicitly.
- No native build dependencies or `build.rs`. No runtime FFI or C/C++ linkage.
- External Rust dependencies also require review; currently there are none.
- Do not replace source-port work with a dependency without the owner's approval.
- Do not add placeholder APIs returning success or claiming unsupported features.
- Never claim feature parity, bit parity, performance parity, or thread safety
  without tests/evidence for the particular claim.

## Numerical contracts

- Raw kernels follow the finite, non-overflowing input preconditions in PORTING.md.
- Ray directions are not unit vectors; preserve fraction parameterization.
- `f32::MAX` is the no-hit sentinel. RayAABox may return a negative entry.
- Keep `FindRoot` root-count and signed-zero conventions; do not casually rewrite
  it to the textbook quadratic formula.
- Default non-FMA and optional FMA paths must both be tested.
- Do not silently enable fast-math, change reduction order, introduce epsilon
  changes, or change the floating-point environment.
- BodyId validates the packed representation, not object liveness or ownership.
- Record deliberate semantic, API, layout and performance differences.

## Required checks

```sh
cargo fmt --all -- --check
cargo test --workspace --locked --offline
cargo test --workspace --locked --offline --all-features
cargo test --workspace --locked --offline --release --all-features
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo doc --workspace --no-deps --all-features --locked --offline
```

Add edge-case and invariant tests for each ported operation. Keep fixed RNG seeds
and include failing input values in assertions. Jolt reference comparisons must
pin compiler flags, upstream SHA, CPU/FP mode and precision; an oracle is a test
reference only and must never become an engine dependency.

Update PORTING.md with exact implemented scope and remaining work in each change.
Avoid unrelated renderer/editor development while this physics port is incomplete.
