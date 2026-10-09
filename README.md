# TomoEngine

Jolt Physics의 알고리즘을 **C++ 바인딩 없이 Rust로 직접 이식**하는 프로젝트입니다.

> **초기 이식 단계입니다. 완성된 Jolt 대체품이나 전체 게임엔진이 아닙니다.**
> 현재는 수학·기하·광선 교차 판정·BodyID 기반만 구현되어 있습니다.
> 강체 시뮬레이션, 접촉 솔버, GJK/EPA, broad phase, 관절, CCD는 아직 없습니다.

## 고정된 원본

- Jolt Physics `v5.6.0`
- Commit: `e77f175595e64cb44218cc9d9d56fc365ad0e36a`
- [원본 저장소](https://github.com/jrouwe/JoltPhysics/tree/e77f175595e64cb44218cc9d9d56fc365ad0e36a)
- 파일별 추적 정보: [`upstream/jolt.toml`](upstream/jolt.toml)
- 구현 범위와 원본과의 차이: [`docs/PORTING.md`](docs/PORTING.md)

## 현재 구현

| 영역 | 구현 범위 |
| --- | --- |
| `math::Vec3` | scalar f32 벡터, 내적·외적, 길이, 정규화, 성분 연산 |
| `math::find_root` | Jolt의 수치적으로 안정적인 이차방정식 근 계산 |
| `aabb::Aabb` | 경계 상자, 병합·교집합·포함·겹침, 크기·면적·부피, support point |
| `ray` | RaySphere, RayTriangle, RayAABox, RayInvDirection의 scalar 이식 |
| `body_id::BodyId` | Jolt의 23-bit index + 8-bit sequence + reserved-bit 규약 |

현재 crate에는 **외부 Cargo dependency, build.rs, C/C++ 소스, FFI가 없습니다.**
`#![forbid(unsafe_code)]`를 사용합니다. 이 정책은 프로젝트가 작성하는 엔진 코드에
적용되며, Rust 표준 라이브러리 내부나 운영체제까지 Rust로 재작성한다는 뜻은 아닙니다.

## 실행

Rustup을 설치한 환경에서 저장소의 `rust-toolchain.toml`에 따라 Rust 1.85.0을 사용합니다.

```sh
cargo test --workspace --locked --offline
cargo test --workspace --locked --offline --all-features
cargo test --workspace --locked --offline --release --all-features
cargo run -p tomo-physics --example raycast --locked --offline
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo doc --workspace --no-deps --all-features --locked --offline
```

`offline`은 Cargo 의존성 해석에 적용됩니다. Rust toolchain 자체의 최초 설치에는
별도로 다운로드가 필요합니다.

```rust
use tomo_physics::{Vec3, ray::ray_sphere};

let fraction = ray_sphere(
    Vec3::new(-3.0, 0.0, 0.0),
    Vec3::new(4.0, 0.0, 0.0),
    Vec3::ZERO,
    1.0,
);
assert_eq!(fraction, 0.5);
```

방향 벡터는 정규화하지 않습니다. 반환값은 `origin + fraction * direction`의
fraction입니다. 이 저수준 함수는 무한 ray를 검사하므로 선분 검사는 호출자가
`0 <= fraction <= 1` 범위를 추가로 확인해야 합니다. 미충돌은 `f32::MAX`입니다.

## 수치 모드와 검증 한계

기본 모드는 Jolt의 scalar / non-FMA 경로에 대응합니다. `fma` feature는
`DifferenceOfProducts`에 Jolt의 보정된 FMA 경로를 적용합니다.
기능별 회귀 테스트와 고정 seed 기하 property test를 포함합니다.

**전체 Jolt와의 동등성, SIMD 성능, 플랫폼 간 bit-identical 결정론은 아직 검증되지 않았습니다.**
테스트 통과와 전체 물리엔진 이식 완료는 별개입니다. 실제 CI 결과는
[Actions](https://github.com/Dokpamo/TomoEngine/actions)에서 확인할 수 있습니다.

## 라이선스

MIT. Jolt에서 파생된 구현의 원저작권 고지와 허가문을 `LICENSE` 및
`LICENSES/Jolt-MIT.txt`에 보존합니다. 공식 Jolt 프로젝트와 별개인 초기 이식본입니다.
