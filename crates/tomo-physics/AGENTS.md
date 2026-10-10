# Physics-specific invariants

These rules preserve the original Jolt port contract after the approved engine expansion.
Read ../../docs/PORTING.md and ../../upstream/jolt.toml before changing physics code.

- Implement pinned Jolt algorithms directly in Rust, not bindings, Rapier, a C++
  subprocess, native libraries or a renamed toy simulation.
- The existing primitive runtime is incomplete. Local broad phase and integration
  scaffolding are not completion of corresponding upstream classes.
- Read pinned upstream source, retain MIT notices, add file/function provenance.
- External Cargo dependencies: zero. No build.rs, native source/linkage or FFI.
- Keep #![forbid(unsafe_code)]. Never weaken the policy as a side effect of tooling.
- Do not alter numeric reduction order, tolerances, FMA behavior or FP environment
  without evidence and an explicit compatibility record.
- Raw kernels require finite, non-overflowing inputs. Preserve FindRoot root count
  and signed-zero rules, ray fraction parameterization and f32::MAX no-hit sentinel.
- Raw AABB rays may have negative entry. World rays are checked segment queries.
- BodyId is world-local, not proof of liveness, locks or cross-world identity.
- Invalid input must not mutate valid state. NumericalFailure remains fail-stop.
- Speculative contacts or substeps are not CCD. Port, parity and speed are distinct.
- Keep all physics regression tests and examples. Test default/FMA in Debug/Release.
- Renderer/editor changes belong outside this crate. Physics must remain reusable
  and buildable alone without scene/renderer/CLI/Tripo dependencies.
