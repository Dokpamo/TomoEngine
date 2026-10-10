# 목표 계층과 단계별 완료 기준

최상위 기준: PROJECT.md. 범위는 P/R/A/T/X 다섯 축이며 퍼센트 완료율을 임의로 쓰지 않는다.
목표 상태는 설계 / 구현 / 검증으로 나눈다. 테스트 수가 증가해도 원본 이식 범위가 자동 확대되지 않는다.

| 단계 | 범위 | 종료 조건 | 상태 |
| --- | --- | --- | --- |
| M0 목표·구조 | P/R/A/T/X | 목표·의존성·품질/플랫폼 완료 기준을 문서화하고 충돌하는 AGENTS 규칙을 분리 | 이 변경에서 설정 |
| M1 AI headless 기초 | R/T | JSON 장면 → 검증 → 고정 tick 실행 → JSON 상태/검사, 안전한 문서 patch, 실패 종료 코드, CI | 구현·검증 후 main 통합 |
| M2 관찰 가능한 3D | R/T | 같은 primitive 장면을 렌더링, 카메라·디버그 collider, 지정 tick 캡처, headless 수치와 비교 | 캡처 + Box Run 대화형 창/입력/목적지/재시작 구현, Metal 실기기 검증; 모바일 후속 |
| M2E 에디터·프로젝트 | R/T | 실제 GUI로 편집·Play/Stop·저장 후 독립 실행 | 첫 목적지 게임 템플릿 구현, 문서/GUI 입력/Metal 검증; 범용 SDK 후속 |
| M2L 게임 로직 편집 | R/T | 에디터에서 이벤트·조건·동작 작성, 저장/미리보기/독립 실행에서 동일 규칙 사용 | 선언적 규칙 v1 구현, 자동 검사·실제 Metal 창 검증; 범용 코드 SDK 후속 |
| M3 에셋과 캐릭터 | R/A/T | 정적 GLB/PBR 검증 후 skeleton/skinning/blending, 실제 이동/충돌, 입력 재현 테스트 | 미구현 |
| M4 실제 게임 제작 | R/A/T | Rust 게임 SDK, 게임 UI·오디오·저장, AI 장면/코드 수정, 작업 journal·재시도·rollback, 통합 게임 테스트 | M2L 선언적 로직은 선행 구현; 범용 SDK·나머지 범위 미구현 |
| M5 생성 서비스 | A/T | Tripo 작업/비용/라이선스/provenance 및 offline fallback, 생성 결과 검증·최적화 | 미구현 |
| M6 5 OS 배포 | R/X/T | Windows/macOS/Linux/iOS/Android 실제 기기 실행·수명주기·입력·저장·패키징·성능 예산 | 미검증 |
| M7 콘솔 | X | 개발자 승인/SDK/개발기기 확보 후 개별 백엔드·저장/입력/패키징·인증 | 외부 조건 대기, 미구현 |

M6의 작은 smoke test는 M2부터 가능한 환경에 추가한다. 모든 PC 기능 완성 뒤 처음
모바일을 실행하는 순서가 아니다. 위 표는 우선순위이며 각 단계가 다른 축의 성공을 대체하지 않는다.

## P: Jolt 직접 이식의 독립 작업 흐름

기준 Jolt v5.6.0 SHA는 upstream/jolt.toml, 세부 현황은 PORTING.md 및 이슈 #2를 따른다.
P 작업은 M1/M2가 동작해도 계속 미완료 항목을 유지한다.

P1: 전체 upstream 파일/함수/테스트 inventory와 현재 runtime 출처 대응표 보강.
P2: 수학·support/closest-point·GJK/EPA, 전체 collision shapes/dispatch.
P3: Jolt body/broad-phase/contact/island/solver/constraints/CCD 직접 이식과 현재 자체 기반 교체.
P4: Job System/SIMD/병렬화, 캐릭터/차량/ragdoll/soft body/serialization 등 기준 버전 전체 범위.
P5: 전체 기능별 oracle 비교, 동일 조건 성능·메모리, 플랫폼별 결정론/장시간 안정성 검증.

샘플 성공을 원본 기능 완료로 표시하지 않는다. 원본 reference는 검증용이며 Cargo
build/runtime에 링크하지 않는다. 목표 원본 버전 변경은 별도 결정과 inventory 갱신이 필요하다.

## M1 구체적 완료 조건

- scene data는 physics/GPU를 의존하지 않으며 unknown field/version과 잘못된 수치를 거부한다.
- runtime은 scene → PhysicsSystem을 실제 연결한다. 미리 저장된 낙하 결과를 출력하지 않는다.
- validate/run/patch/capabilities가 JSON을 출력하고 문서화된 비정상 종료 코드를 사용한다.
- patch 실패 시 원본 문서/파일을 유지하고 revision 충돌을 검사한다.
- 검사 실패와 파싱/실행 오류를 구분한다. 지원하지 않는 기능을 capabilities에서 숨기지 않는다.
- 기존 physics 테스트와 새 scene/runtime/CLI 테스트를 Debug/Release 및 default/FMA로 실행한다.
- 최신 commit의 실제 CI를 확인한다. lockfile bootstrap과 개발용 임시 workflow는 제거한다.

## 다음 실행 작업

Box Run으로 desktop 입력·물리 이동·벽·목적지·승리·재시작·고정 입력 재현·GPU HUD를 연결했다.
CLI 누적 예산과 게임 수명을 분리했고 색/HUD는 물리 scene 외부의 표현 데이터로 두었다.
소유자 요청으로 GLB보다 에디터와 게임 제작 구조를 우선했다. M2E는 프로젝트 v1, 재사용
게임 규칙, 실제 편집 창, 트랜잭션/Undo, 충돌 감지 저장과 Play/Stop을 연결한다. 구체적인
구현·검증·미지원 범위는 EDITOR.md를 따른다. 소유자의 게임 로직 편집 요청은 M2E 확장과
M4의 선행 범위인 M2L로 구체화했다. Project v2/Logic v1의 이벤트·조건·동작, 점수·종료,
실제 접촉 이벤트, 규칙 편집/검증/Undo와 replay를 연결한다. 세부 계약은 LOGIC.md이며
M4 전체나 임의 코드 실행 SDK의 완료를 뜻하지 않는다. 다음 제작 확장은 카메라/일반 entity와 visual
분리, 여러 scene·prefab·게임 SDK다. 정적 GLB 가져오기·검증·렌더링은 보류된 후속 작업이다. Windows 실기기와 모바일의
창/입력/수명주기 검증도 별도로 남는다. Box Run의 구체적 계약/테스트는 GAME.md를 따른다.
P 작업의 다음 항목은 P1 원본 inventory/검증 fixture다. 렌더링 성공 때문에 이 항목을 닫지 않는다.
긴 구현은 완료된 파일·테스트·미완료 항목을 PR에 남기고, 응답 종료 뒤 자동으로 계속 수행된다고
약속하지 않는다. 작업 재개 시 브랜치 head와 변경 내용을 다시 읽는다.
