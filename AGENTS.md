# TomoEngine contributor instructions

## Non-negotiable project intent

Implement the pinned Jolt algorithms in Rust. Do not substitute Jolt bindings, Rapier,
a C++ subprocess, a native library, generated C/C++, or a renamed toy simulation.
The current implementation has a primitive rigid-body runtime, NOT full Jolt parity.
The local broad phase and runtime integration are intermediate scaffolding; their
existence does not complete the corresponding upstream source-port milestones.

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
- Keep documentation synchronized with actual code. Distinguish source-derived
  kernels, independently written integration, tested behavior and unverified parity.

## Numerical contracts

- Raw kernels follow the finite, non-overflowing input preconditions in PORTING.md.
- Ray directions are not unit vectors; preserve fraction parameterization.
- Raw kernels use f32::MAX for no hit; RayAABox may return a negative entry.
- World ray queries are segment queries with checked options and inside fraction 0.
- Keep FindRoot root-count and signed-zero conventions; do not casually rewrite
  it to the textbook quadratic formula.
- Default non-FMA and optional FMA paths must both be tested in debug and release.
- Do not silently enable fast-math, change reduction order, introduce epsilon
  changes, or change the floating-point environment.
- BodyId validates the packed representation, not object liveness or world identity.
- Invalid world API inputs must not silently mutate valid state. NumericalFailure
  is explicitly fail-stop, not transactional rollback.
- Speculative contacts and additional substeps are not a replacement for CCD.
- Record deliberate semantic, API, layout and performance differences.

## Required checks

```sh
cargo fmt --all -- --check
cargo test --workspace --locked --offline
cargo test --workspace --locked --offline --all-features
cargo test --workspace --locked --offline --release
cargo test --workspace --locked --offline --release --all-features
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo doc --workspace --no-deps --all-features --locked --offline
cargo run -p tomo-physics --example falling_box --release --locked --offline
cargo run -p tomo-physics --example stack --release --locked --offline
```

Add edge-case and invariant tests for each ported operation. Keep fixed RNG seeds
and include failing input values in assertions. Jolt reference comparisons must
pin compiler flags, upstream SHA, CPU/FP mode and precision; an oracle is a test
reference only and must never become an engine dependency.

CI is read-only: do not automatically rewrite and push code from validation jobs.
Check the actual target commit's logs, not only an older successful run.
Update PORTING.md with exact implemented scope and remaining work in each change.
Avoid unrelated renderer/editor development while this physics port is incomplete.
