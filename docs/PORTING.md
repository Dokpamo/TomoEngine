# Jolt → Rust 이식 상태와 설계 계약

## 목표와 완료 기준

TomoEngine의 목표는 Jolt Physics를 Rust로 직접 구현하는 것입니다. 바인딩을
추가하거나 Rapier로 대체하는 것은 이 프로젝트의 목표 달성으로 계산하지 않습니다.
기준은 `upstream/jolt.toml`에 고정된 Jolt v5.6.0 commit입니다.

다음 네 기준을 구분합니다.

1. **소스 구현**: 실제 Rust 알고리즘이 존재하며 FFI/C++ 실행에 의존하지 않음.
2. **동작 검증**: 해당 모듈의 정상·퇴화·경계 사례를 테스트함.
3. **원본 동등성**: 고정된 C++ reference와 differential test로 비교함.
4. **성능/결정론**: 같은 장면·하드웨어·설정의 측정 및 별도의 재현성 검사.

현재는 아래의 작은 foundation 범위만 구현되어 있습니다. 테스트가 있다고 해서
3번과 4번까지 달성한 것으로 취급하지 않습니다. 이 문서는 퍼센트 완료율을 추정하지
않으며, 물리엔진 전체가 동작한다고 주장하지 않습니다.

## 실제 구현 범위

| 모듈 | 포함 | 미포함 |
| --- | --- | --- |
| Vec3 | scalar f32 연산, 내적, 외적, 길이, normalized-or | SIMD 저장/명령, double precision, 전체 Jolt API |
| FindRoot | f32, 선형/상수/이차 방정식, FMA 선택 | f64 specialization, 임의 정밀도 |
| Aabb | empty, union, intersection, containment, overlap, support, scale, translate, distance | matrix transforms, plane overlap, supporting faces, double rounding |
| Ray | 단일 sphere/triangle/AABB 교차 | shape casts, 4-wide SIMD, CCD, shape dispatch |
| BodyId | 32-bit representation, 비교/해시, 검증된 생성 | BodyManager, allocation, locks, object lifetime tracking |

## 원본과 의도적으로 다른 부분

- Rust식 모듈명/메서드명과 `Option`/`Roots` 반환 타입을 사용합니다. C++ ABI 및
  소스 수준의 drop-in 호환성을 제공하지 않습니다.
- Vec3는 세 개의 f32만 저장합니다. Jolt의 16-byte SIMD layout과 다릅니다.
- 초기 scalar 계산의 이해·검증을 우선합니다. 성능을 원본 수준이라고 주장하지 않습니다.
- Vec3 component min/max는 Rust의 f32 min/max입니다. NaN 및 signed-zero tie 처리가
  일부 Jolt SIMD 경로와 다를 수 있습니다. NaN/무한대/overflow 입력은 지원 계약 밖입니다.
- normalized-or는 원본처럼 length-squared <= f32::MIN_POSITIVE일 때 fallback을
  반환합니다. NaN이나 overflow를 고치는 일반-purpose normalization 함수가 아닙니다.
- Aabb의 overlaps/scale/ray query는 invalid/empty bounds를 명시적으로 거부하거나
  empty 상태로 유지합니다. 원본 scalar AABox/RayAABox의 invalid-input 처리를 강화합니다.
- BodyId 생성자의 원본 debug assertion은 Rust의 Option으로 바꿔 release에서도
  reserved bit와 index 범위를 검사합니다. INVALID sentinel은 raw 생성으로 허용합니다.
- BodyId sequence는 원본처럼 8 bit입니다. 무한한 stale-handle 방지나 thread safety를
  보장하지 않습니다. 미래 BodyManager는 slot reuse, sequence wrap, access ownership을
  별도로 설계해야 합니다.
- 이식 대상은 현재 저수준 kernel입니다. 외부 파일/네트워크 입력을 직접 넣지 말고
  미래의 checked boundary에서 유한성, 크기, shape 유효성을 검사해야 합니다.

## 부동소수점 계약

기본 경로: scalar f32, Jolt의 non-FMA `DifferenceOfProducts`, `(x + y) + z` reduction.
`fma` feature: Jolt의 보정된 difference-of-products에 `f32::mul_add`를 사용합니다.
이 feature는 SIMD 구현이나 전체 cross-platform determinism 모드가 아닙니다.

테스트 계약은 유한한 입력이며 중간값이 f32 범위를 초과하지 않는 경우입니다.
반지름은 0 이상이어야 합니다. 극단적인 world scale, overflow, NaN/Inf 입력,
FP flush-to-zero/rounding-mode 변경은 현재 지원하지 않습니다.

FindRoot는 단순한 근의 공식으로 치환하지 않습니다. `det == 0`이어도 `q != 0`이면
원본은 동일한 근을 두 개 반환할 수 있습니다. `Sign(-0.0)`도 원본처럼 +1입니다.

RaySphere/RayTriangle의 miss는 f32::MAX입니다. RaySphere는 내부 시작을 0으로
처리하지만 RayAABox는 음수 entry를 유지합니다. 정지 ray가 유효한 AABB 안에 있으면
entry가 -f32::MAX일 수 있습니다. 이는 저수준 slab query의 원본 규약입니다.

## 테스트와 자동화

현재 회귀 테스트는 직접 작성한 분석적 expected values와 고정 seed invariant
검사입니다. upstream UnitTests 전체를 가져오거나 C++ 원본 전체와 비교한 것이 아닙니다.

CI는 Linux/Windows/macOS, Rust 1.85.0 및 Linux stable에서 debug/default,
FMA, release/FMA, example을 실행하도록 구성합니다. 별도 quality job은 rustfmt,
Clippy, rustdoc, dependency/native-source policy를 검사합니다.

CI의 존재와 실행 성공은 다릅니다. 실제 run 로그로 상태를 확인해야 합니다.

## 다음 구현 순서

### 1. 수학·기하 기반 확장

Quat/Mat44/DVec3, plane/sphere/capsule, closest point(line/triangle/tetrahedron),
support mappings와 convex hull. 각 함수는 원본 위치, 테스트, 차이 기록과 함께 추가합니다.

### 2. Narrow phase

GJK/EPA, penetration depth, contact generation/manifolds, sphere/box/capsule/triangle,
mesh/heightfield/compound shape dispatch. 퇴화 도형과 매우 가까운 접촉부터 검증합니다.

### 3. Body와 공간 구조

Body/MotionProperties, shape ownership, mass/inertia, BodyManager, broad-phase tree,
layers/filters, collectors. Rust ownership 및 세대 재사용 정책을 실제 접근 API에 반영합니다.

### 4. 단일 스레드 강체 시뮬레이션

Contact cache, island building, velocity/position solver, restitution/friction,
sleeping/activation, fixed-step integration, CCD 및 constraint 계열.
먼저 떨어지는 상자/쌓기/관절 장면의 reference 비교를 만들고 병렬화를 추가합니다.

### 5. 병렬화와 성능

Job dependencies, deterministic ordering, scheduling, SIMD, allocation/cache layout.
검증된 scalar 경로를 reference로 유지합니다. Rust라고 더 빠르다는 가정은 하지 않습니다.

### 6. 상위 기능 및 v5.6 전체 범위

Character/CharacterVirtual, vehicles, ragdolls, soft bodies, serialization,
GPU compute/hair 등 기준 버전의 나머지 기능을 별도 추적합니다.
이 목록에 있다고 구현된 것이 아닙니다. 누락을 숨기기 위해 완료 기준을 축소하지 않습니다.

### 7. 검증 확대

upstream UnitTests/PerformanceTest 대응표, 고정 reference fixtures,
differential testing, fuzzing, 장시간 장면 회귀, 플랫폼별 성능·결정론 보고서.
Reference 실행은 검증 도구에만 한정하고 Cargo build/runtime dependency로 넣지 않습니다.
