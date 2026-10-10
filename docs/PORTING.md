# Jolt → Rust 이식 상태와 설계 계약

## 목표와 현재 상태

최종 목표는 고정된 Jolt v5.6.0의 전체 범위를 C++ 바인딩·FFI 없이 Rust로 직접
이식하는 것입니다. 단순한 독립 엔진 제작, Rapier 연결, 또는 낙하 데모 성공을
이 목표의 달성으로 취급하지 않습니다.

현재는 foundation 수학·기하에 **단일 스레드 primitive 강체 런타임**이 연결된 단계입니다.
구와 회전 상자의 충돌·마찰·반발, 수면, 거리 관절, 힘·토크 등을 실제로 계산합니다.
렌더러·에디터는 없으며 예제는 headless입니다. 전체 이식 완료율을 임의로 추정하지 않습니다.

다음 네 기준은 독립적입니다.

1. 실제 Rust 구현이 존재하고 C++ 실행에 의존하지 않는가.
2. 해당 기능의 정상·퇴화·경계 동작이 테스트되었는가.
3. 고정된 C++ reference와 differential test로 동등성을 확인했는가.
4. 같은 하드웨어·설정에서 성능 및 필요한 결정론 수준을 측정했는가.

현재 테스트 성공은 3·4번의 달성을 뜻하지 않습니다.

## 구현 범위 및 출처 구분

| 모듈 | 포함 | 미포함 / 원본과의 차이 |
| --- | --- | --- |
| math, aabb, ray, body_id | 기존 scalar foundation 및 packed ID | 전체 SIMD/API, f64 경로 |
| rotation | quaternion 연산, 회전 갱신, Transform | 전체 Jolt math, SIMD 레이아웃 |
| shape | sphere / sharp box, 질량·관성·경계·ray | convex radius 및 나머지 형상 |
| body | 힘·토크 적분, 회전, gyroscopic 옵션, checked 설정·슬롯 저장소 | Jolt 동시성 BodyManager, 전체 MotionProperties |
| broad_phase | 단일 축 sweep-and-prune | 자체 실행용 구현. Jolt broad-phase tree 이식이 아님 |
| collision | sphere/sphere, sphere/box, OBB SAT, 접촉점 구성 | 자체 primitive 통합. GJK/EPA·전체 dispatch가 아님 |
| solver | source-derived scalar 축 제약·스프링 수식, 접촉 조립 | 자체 cache·friction projection. 전체 ContactConstraintManager가 아님 |
| joint | 거리 제약·거리 스프링 | 전체 DistanceConstraint API, 나머지 관절·모터 |
| world | substeps, 단일 스레드 솔버 순서, islands, sleeping, events, ray cast | 자체 조립부. Jolt PhysicsSystem 전체 이식이 아님 |

`upstream/jolt.toml`은 기존 foundation 파일의 blob 추적 manifest입니다. 런타임 파일의
헤더에 기록된 Body/MotionProperties/AxisConstraintPart/SpringPart 등의 참조에 대해
**같은 수준의 파일·함수·blob 대응표와 원본 대조를 추가하는 작업이 남아 있습니다.**
헤더의 출처 표기만으로 전체 함수의 정확한 원본 동등성을 검증했다고 주장하지 않습니다.

전체 이식 목표에서는 자체 broad phase와 조립부를 임시 기반으로 구분합니다.
해당 기능이 존재한다는 이유로 대응하는 Jolt 원본 클래스의 이식 체크박스를 닫지 않습니다.

## 공개 API와 객체 수명

`PhysicsSystem`이 Body를 소유하고 `body(id)`는 읽기 전용 참조를 반환합니다.
변경은 world API를 통해 캐시 무효화·wake-up 규칙을 거칩니다. BodyId는 하나의 world에
속하는 ID이며 서로 다른 world의 ID를 혼용해서는 안 됩니다.

BodyId 자체는 23-bit index, 8-bit sequence, reserved bit 규약을 보존합니다.
BodyArena는 sequence가 wrap되어 오래된 ID가 재사용 슬롯과 충돌하기 전에 슬롯을
퇴역시킵니다. 생성·제거를 매우 오래 반복하면 슬롯 저장 공간이 늘 수 있습니다.
이 정책은 Jolt의 동시성 객체 수명 구현과 동일하지 않습니다.

## 시간 및 시뮬레이션 계약

- `update(dt)`는 유한한 `1e-6 <= dt <= 0.25`를 받습니다. 나눈 substep 역시 최소
  1e-6이어야 합니다. 설정상 substeps는 1~64입니다. 고정 dt 사용을 권장합니다.
- 잘못된 dt는 상태·시각·누적된 힘을 변경하기 전에 거부합니다.
- 누적 힘과 토크는 모든 substep에 적용한 후 update 끝에서 한 번 비웁니다.
- 입력 검증 실패와 실행 중 `NumericalFailure`는 다릅니다. 후자는 이전 상태로
  되돌리는 rollback이 아니라 fail-stop입니다. 실패한 world는 폐기해야 합니다.
- 현재 speculative contact는 제한된 거리의 선행 접촉 처리입니다. **CCD가 아닙니다.**
  빠른 물체·얇은 벽·큰 dt에서 tunneling을 방지한다고 보장하지 않습니다.
- Static/Kinematic/Dynamic을 구분합니다. 관절·접촉 후보의 지원 범위는 코드와 테스트를
  기준으로 하며 모든 Jolt collision-pair 정책이 재현된 것은 아닙니다.

## 수치 계약과 차이

Foundation 기본 경로는 scalar f32 및 non-FMA difference-of-products입니다.
`fma` feature는 보정된 difference-of-products에만 적용되며 엔진 전체의 SIMD나
cross-platform determinism 모드가 아닙니다.

Vec3는 3개의 f32이며 Jolt SIMD Vec3의 16-byte layout과 다릅니다. component min/max는
Rust f32 규칙을 사용하므로 NaN·signed-zero tie 처리가 일부 Jolt SIMD 경로와 다를 수
있습니다. 저수준 foundation kernel은 유한하고 중간 연산이 overflow하지 않는 입력을
전제로 합니다. checked world API와 raw math API의 보장 수준을 혼동하지 않습니다.

normalized-or의 fallback 기준은 squared length <= f32::MIN_POSITIVE입니다.
FindRoot는 cancellation-resistant 공식을 유지하며 det == 0에서도 q != 0이면
같은 근을 두 개 반환할 수 있고 Sign(-0.0)은 +1입니다.

RaySphere/RayTriangle의 miss는 f32::MAX입니다. 방향은 정규화하지 않습니다.
raw RayAABox는 내부 시작 시 음수 entry를 유지하며 정지 ray가 내부에 있으면
-f32::MAX entry를 반환할 수 있습니다. **world ray cast는 별도 segment API**이며
기본 max_fraction=1, 내부 시작은 fraction=0으로 처리합니다.

Aabb의 invalid/empty 처리와 BodyId 생성자의 release 검사 등 foundation의 강화된
검사는 원본의 일부 debug-only/invalid-input 동작과 의도적으로 다릅니다.
부동소수점 환경 변경, 무제한 world scale, 극단적 질량비, 전체 플랫폼 간 비트 동등성은
검증되지 않았습니다. 각 공개 설정의 실제 허용 범위는 checked constructor를 따릅니다.

## 테스트·CI

현재 검증은 분석적 기대값, 고정 seed 기하 invariant, 독립적인 꼭짓점 투영 OBB 검사,
낙하·쌓기·마찰·반발·관절·수면·센서 장면 및 API 경계 회귀 테스트입니다.
독립 투영 검사는 C++ Jolt oracle 비교가 아닙니다.

일반 CI에서 Linux/Windows/macOS의 Rust 1.85.0과 Linux stable을 검사합니다.
각 환경에서 default/FMA의 debug 및 release 테스트와 raycast/falling_box/stack 예제를
실행합니다. 별도 quality job은 formatting, Clippy, rustdoc, 의존성 정책을 확인합니다.

CI는 읽기 전용이며 테스트 중 코드를 자동 수정·커밋하지 않습니다. formatter bot이
별도 커밋을 만든 뒤 새 검증이 action_required로 남는 혼동을 줄이기 위해 임시
format-runtime workflow를 일반 CI로 통합했습니다. 승인·브랜치 보호 정책은 변경하지
않습니다. CI가 존재하는 것과 해당 커밋에서 성공한 것은 별개입니다.

## 남은 전체 이식 작업

1. 런타임의 원본 파일·함수 대응표 확대와 고정 C++ reference fixtures / differential tests.
2. Mat44/DVec3, closest point, support mappings, convex hull, GJK/EPA, penetration depth.
3. Capsule/convex/mesh/heightfield/compound 및 전체 shape dispatch와 contact generation.
4. Jolt의 broad-phase tree, BodyManager/MotionProperties, contact cache·island·solver
   전체 동작을 직접 이식하고 현재 자체 통합부와의 차이를 줄이는 작업.
5. CCD, 전체 constraints/joints/motors, Job System, 병렬화, SIMD, 메모리·캐시 최적화.
6. Character/CharacterVirtual, vehicles, ragdolls, soft bodies, serialization 및 기준
   버전의 나머지 기능. GPU compute/hair 관련 범위도 누락하지 않고 별도로 추적.
7. upstream UnitTests/PerformanceTest 대응표, fuzzing, 장시간 장면 회귀, 모바일·Web
   검증, 동일 조건 벤치마크와 결정론 보고서.

C++ reference는 검증용으로만 사용할 수 있으며 엔진의 Cargo build/runtime 의존성이
되어서는 안 됩니다. 기능 누락을 숨기기 위해 전체 이식 목표를 축소하지 않습니다.
