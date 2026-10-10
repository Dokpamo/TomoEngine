# TomoEngine

Jolt Physics를 C++ 바인딩 없이 **100% Rust로 직접 이식**하는 프로젝트입니다.

> **현재: 구·상자를 지원하는 초기 3D 강체 런타임이 동작합니다.**
> 중력, 충돌 반응, 회전, 마찰, 반발, 수면, 거리 관절을 연결했습니다.
> **Jolt 전체 기능의 완전 이식 또는 원본과의 동등성이 검증된 상태는 아닙니다.**

구현은 [작업 브랜치](https://github.com/Dokpamo/TomoEngine/tree/feat/jolt-rust-foundation)와
[PR #1](https://github.com/Dokpamo/TomoEngine/pull/1)에 있습니다. PR이 병합되기 전에는
`main`의 파일 목록만으로 작업 브랜치의 구현 여부를 판단할 수 없습니다.

## 바로 실행

```sh
git clone --branch feat/jolt-rust-foundation https://github.com/Dokpamo/TomoEngine.git
cd TomoEngine
cargo run -p tomo-physics --example falling_box --release --locked --offline
cargo run -p tomo-physics --example stack --release --locked --offline
```

두 예제는 화면을 그리지 않는 **headless 시뮬레이션**입니다. 렌더러는 포함하지 않습니다.
`falling_box`는 높이 5에서 떨어진 상자가 실제 정적 상자 형상의 바닥에 닿아 멈추는지,
`stack`은 상자 5개가 쌓인 상태에서 안정적으로 정지하는지 assertion으로 검사합니다.
예제 출력을 미리 저장해 재생하거나 바닥 높이를 하드코딩해 고정시키는 방식이 아닙니다.

Rustup 및 `rust-toolchain.toml`에 지정된 Rust 1.85.0이 필요합니다. `--offline`은
Cargo 의존성 해석에 대한 옵션이며 toolchain 최초 설치에는 다운로드가 필요합니다.

## 실제 구현 범위

| 영역 | 현재 구현 |
| --- | --- |
| 수학·기하 | scalar Vec3, FindRoot, AABB, ray queries, Quat, Transform |
| 형상 | 구, 회전 가능한 날카로운 모서리 상자, 질량·관성·경계 계산 |
| 객체 | Static / Kinematic / Dynamic, 슬롯·세대 ID, 힘·토크·충격량 |
| 충돌 후보 | 단일 축 sweep-and-prune, 양방향 충돌 필터 |
| 정밀 충돌 | 구–구, 구–상자, 상자–상자 SAT 및 접촉점 구성 |
| 솔버 | 접촉 속도·위치 보정, 마찰, 반발, warm starting |
| 실행 | PhysicsSystem, substeps, 회전·중력·감쇠, 연결된 객체의 수면·깨우기 |
| 관절 | 고정 거리 제약 및 주파수·감쇠 기반 거리 스프링 |
| 조회·이벤트 | world ray cast, 센서, 접촉 시작·유지·종료 이벤트 |

위 표는 **동작하는 부분 구현의 범위**입니다. 일부 수치 커널은 Jolt 소스 기반이고,
현재 broad phase, primitive collision, 저장소, 이벤트 및 실행 조립부는 자체 구현입니다.
이를 Jolt의 전체 클래스나 병렬 PhysicsSystem을 그대로 이식한 것으로 계산하지 않습니다.

**아직 없는 기능:** GJK/EPA, capsule/convex/mesh/heightfield/compound shape의 완전한
지원, CCD, Jolt의 broad-phase tree·병렬 Job System, 전체 관절·모터, 캐릭터, 차량,
ragdoll, soft bodies, 전체 직렬화 및 기준 버전의 나머지 기능.

세부 범위·차이·검증 한계는 [PORTING.md](docs/PORTING.md)에 기록합니다.

## 순수 Rust 정책

현재 엔진 crate의 외부 Cargo dependency는 0개입니다. C/C++ 소스, `build.rs`, FFI,
Jolt 네이티브 라이브러리, Rapier 대체 의존성 없이 `#![forbid(unsafe_code)]`를 사용합니다.
이 정책은 프로젝트 엔진 코드와 Cargo 의존성에 관한 것이며 Rust 표준 라이브러리 내부,
컴파일러나 운영체제까지 모두 Rust로 재작성했다는 뜻이 아닙니다.

## 검증

```sh
cargo fmt --all -- --check
cargo test --workspace --locked --offline
cargo test --workspace --all-features --locked --offline
cargo test --workspace --release --locked --offline
cargo test --workspace --release --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo doc --workspace --no-deps --all-features --locked --offline
```

[GitHub Actions](https://github.com/Dokpamo/TomoEngine/actions)에서 **대상 커밋의 실제
검사 결과**를 확인해야 합니다. 예전 커밋의 성공, CI 파일의 존재, 테스트 수만으로
최신 코드의 성공이나 전체 Jolt 호환성을 주장하지 않습니다.

분석적인 기대값, 고정 seed 불변조건, 물리 장면 회귀, API 경계 검사를 포함합니다.
원본 C++ 전체와의 differential testing, 성능 동등성, 플랫폼 간 bit-identical
결정론은 아직 검증하지 않았습니다. 동일 프로세스 내 재실행 테스트와는 별개입니다.

## 원본 및 라이선스

- 기준: Jolt Physics `v5.6.0`, commit `e77f175595e64cb44218cc9d9d56fc365ad0e36a`
- foundation 원본 추적: [upstream/jolt.toml](upstream/jolt.toml)
- 원본: https://github.com/jrouwe/JoltPhysics/tree/e77f175595e64cb44218cc9d9d56fc365ad0e36a
- MIT 고지: [LICENSE](LICENSE), [Jolt-MIT.txt](LICENSES/Jolt-MIT.txt)

공식 Jolt 프로젝트와 별개인 초기 이식 프로젝트입니다.
