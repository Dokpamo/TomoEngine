# 아키텍처 결정: AI 조작 우선, 독립 물리 코어

## 현재 만드는 경계

```text
외부 AI 에이전트 / 사람
          |
 tools/tomo-cli               JSON 입력·출력, 명령/오류/종료 코드
          |
 crates/tomo-runtime          장면 검증·구축, 고정 tick, 상태·검사 보고
       /            \
 crates/tomo-scene   crates/tomo-physics
 버전 있는 데이터   기존 독립 순수 Rust Jolt 이식
```

`tomo-scene`에는 serde 자료형과 장면 규칙만 둔다. 물리 API나 GPU 객체를 노출하지 않는다.
`tomo-runtime`이 안정적인 장면 문자열 ID를 하나의 world의 BodyId로 매핑한다. BodyId를
파일에 저장하거나 다른 world의 영속 ID로 취급하지 않는다. 렌더러는 snapshot DTO를
소비하며 물리 world의 쓰기 참조를 보유하지 않는다.

현재 `tomo-cli --features render`는 `tomo-render`를 선택적으로 의존한다.
`tomo-render`는 scene 및 runtime의 읽기 전용 snapshot을 받아 GPU 캡처를 수행한다.
runtime/scene/physics에서 renderer로 향하는 의존성은 없다.

## 렌더러와 후속 구조

`tomo-render`는 primitive 캡처와 지속적인 GPU surface/장치/파이프라인/정점 버퍼 재사용을 제공한다.
`tools/tomo-play`는 runtime/scene/render를 사용해 Box Run 게임, winit 창과 입력, JSON 입력 재현을
연결한다. 물리 없는 색/HUD는 `FrameStyle`로 분리했으며 일반 visual entity/ECS는 후속이다.
`tomo-assets`: GLB·텍스처 가져오기/검증·버전 있는 변환 캐시.
애니메이션·캐릭터는 책임이 확인될 때 별도 crate로 추출한다. 처음부터 빈 crate를 만들지 않는다.
플랫폼 어댑터는 창·입력·오디오·저장·수명주기·패키징을 담당한다. 콘솔 코드는 SDK 허가와
공개 가능 범위에 따라 별도 비공개 어댑터가 될 수 있다.

## 데이터와 실행 계약

- v1 장면: 우수 좌표계, +Y 위, 미터·초·킬로그램, quaternion [x,y,z,w].
- 위치는 현재 primitive 물체의 질량 중심이다. box 크기는 full size가 아닌 half_extents다.
- 장면 ID와 각 body ID는 안정적인 데이터 ID다. 입력 배열 순서가 달라도 body ID 정렬 순서로 구축한다.
- JSON은 엄격한 typed deserialization을 사용한다. 알 수 없는 필드·버전·중복 body ID를 거부한다.
- 입력 크기, 객체 수, tick 수와 시뮬레이션 작업량에 상한을 둔다. 현재 CLI는 파일/표준출력을
  처리할 뿐 샌드박스가 아니다. 외부 코드나 provider를 실행하지 않는다.
- 장면 검증은 완전한 물리 품질 보장이 아니다. 실제 시뮬레이션 검사와 원본 동등성은 별개다.
- 고정 tick 입력과 최종 snapshot을 기록한다. cross-platform bit parity를 주장하지 않는다.
- 현재 patch는 원본의 clone을 수정하고 전체 검증 후 새 문서를 반환한다. expected_revision
  불일치를 거부하고 성공 시 revision을 증가시킨다. 파일을 제자리 덮어쓰지 않는다.
- patch의 원자성은 **장면 문서 변경**에 관한 것이다. 실행 중 물리 world rollback이나
  운영체제 파일 교체, 원격 exactly-once 실행까지 보장하지 않는다.

## 실행 상태와 관찰

Runtime는 유효한 장면에서 새 world를 구성한다. 수치 실패가 발생하면 failed 상태가 되어
추가 실행을 거부한다. 새 장면 로드는 새 Runtime을 만들며 simulation time을 초기화한다.
보고에는 적용된 전체 장면, 엔진 버전, Jolt 기준 SHA, FP feature, OS/CPU 아키텍처,
fixed_dt, tick 수, 최종 body 위치·회전·속도·sleep 상태와 검사 결과를 넣는다.
대화형 `step`은 한 tick씩 계속 진행하고, `set_body_velocity`는 scene ID로 실제 world의 속도를
변경한다. tool용 `run`은 누적 3600 tick/body-squared 예산을 그대로 유지한다. desktop은
한 프레임 최대 8 tick을 수행하고, 별도 replay 도구는 파일/입력 tick 예산을 검사한다.
이 보고는 정확한 빌드 바이너리 해시/컴파일러/lockfile을 포함하는 최종 provenance 형식의
전부가 아니다. 장기 작업 journal·캡처·빌드 provenance는 별도 단계로 남긴다.

최종 체크는 위치 범위 및 선형 속도 상한부터 시작한다. 이를 충돌체 관통/스킨 품질/경로
도달 등 모든 자동 품질 검사의 구현으로 간주하지 않는다.

## Rust/의존성 정책

Physics: 외부 Cargo dependency 0, FFI·C/C++·build.rs 없이 프로젝트 Rust 코드에
forbid(unsafe_code). Jolt 알고리즘은 Rust 바인딩이나 Rapier로 대체하지 않는다.

Scene/Runtime/CLI: 프로젝트 코드에는 forbid(unsafe_code). JSON 처리는 직접 파서를
만드는 대신 serde 1.0.228 / serde_json 1.0.145를 정확한 버전으로 도입한다. 이들은
physics crate로 전파되지 않는다. Cargo.lock은 Cargo의 실제 해석 결과를 커밋한다.
직접·전이 의존성과 라이선스/links/native build 여부는 정책 검사로 제한하고 추가 시 검토한다.
제3자 crate 내부의 unsafe까지 없다고 주장하지 않는다. 향후 GPU·OS 접점의 네이티브
연동은 physics 정책을 완화하지 않고 별도 결정으로 승인·기록한다.

셰이더·그래픽 드라이버·운영체제까지 Rust로 재작성한다는 목표가 아니다.
렌더러에만 wgpu 26.0.1의 Metal/Vulkan/DX12 GPU·OS 연동과 WGSL 셰이더를 허용한다.
창·입력용 winit 0.30.13은 `tools/tomo-play`에만 둔다.
직접 의존성 목적, 전이 의존성 및 native 경계 검토는 RENDERING.md에 기록한다.
physics/scene/runtime의 전이 그래프에는 GPU/native 의존성이 들어갈 수 없으며 CI가 이를 검사한다.

## 에셋·AI 서비스 경계

Tripo는 교체 가능한 제작 provider이며 런타임 필수 서비스가 아니다. 준비된 GLB 입력이
기본 흐름이다. 외부 생성 작업은 비용 승인, 요청 ID, 재조회/재시도, 결과 해시·라이선스·모델
버전·생성/변환 내역을 저장하는 도구 계층에 둔다. 이번 단계는 실제 API 호출을 하지 않는다.
생성물·파일 metadata의 텍스트는 신뢰하지 않고 실행 명령으로 취급하지 않는다.
API 키·서명 키·콘솔 SDK는 저장소 및 배포 게임에 넣지 않는다.

## 검증 계층

1. 기존 Physics 회귀 테스트를 그대로 유지한다.
2. 장면 버전·필드·수치·ID·예산 검사와 직렬화 왕복 테스트.
3. 원자적 patch와 충돌 revision 테스트.
4. CLI 프로세스의 JSON·종료 코드·낙하 장면·실패 장면 테스트.
5. 그래픽 캡처는 별도 GPU 테스트에서 실제 픽셀/깊이/이동/회전/PNG/CLI 동등성을 검사한다.
   Box Run은 headless 입력 재현/게임 규칙과 실제 GPU HUD/게임 캡처, desktop surface smoke를 검사한다.
6. 모바일 기기/외부 provider/원본 C++ differential 테스트는 여전히 별도 후속 작업이다.

기술 근거: serde의 deny_unknown_fields (https://serde.rs/container-attrs.html),
serde_json (https://docs.rs/serde_json), wgpu (https://wgpu.rs/).
플랫폼 지원 증거는 라이브러리 문서와 별개로 RENDERING.md 및 실제 GPU 검사 결과를 따른다.
