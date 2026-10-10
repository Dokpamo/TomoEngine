# 실행 가능한 AI 조작 인터페이스

이 명령은 `tools/tomo-cli`의 Rust binary `tomo`에 구현되어 있다. `render` feature로
GPU 이미지 캡처를 추가할 수 있다. LLM, Tripo는 아직 구현하지 않았다. 외부 AI가 이 명령을 사용할 수 있다는 뜻이며
이 CLI가 혼자 자연어를 이해하고 게임을 만드는 자율 에이전트라는 뜻은 아니다.

대화형 게임은 별도 `tomo-play` 바이너리다. `capabilities`의 `interactive_window: false`는
`tomo` 자체의 기능이며 `companion_game`에 실행 명령을 제공한다. 게임 조작·입력 재현·
macOS 로컬 앱 패키징은 [GAME.md](GAME.md)를 따른다.

## 빌드와 실행

저장소 root에서 Rustup/pinned Rust를 사용한다.

```sh
cargo fetch --locked
cargo build -p tomo-cli --locked --offline
cargo run -q -p tomo-cli --locked --offline -- capabilities
cargo run -q -p tomo-cli --locked --offline -- scene validate examples/scenes/falling_box.scene.json
cargo run -q -p tomo-cli --locked --offline -- inspect examples/scenes/falling_box.scene.json
cargo run -q -p tomo-cli --locked --offline -- run examples/scenes/falling_box.scene.json --ticks 600
```

빌드 후에는 `target/debug/tomo` (Windows는 `tomo.exe`)로 동일 명령을 실행한다.
JSON 라이브러리는 최초 fetch가 필요하지만 Physics 자체에는 추가 의존성이 없다.

## 명령과 출력

| 명령 | 실제 동작 | 성공 출력 |
| --- | --- | --- |
| capabilities | 현재 기능/미지원 기능/한도 조회 | versioned JSON |
| scene validate FILE | 파싱, 규칙 검사, 실제 physics world 구성 | ok, name, revision, body_count |
| inspect FILE | 새 world의 tick 0 상태 조회 | snapshot: position/rotation/velocities/sleeping |
| run FILE --ticks N | 고정 dt로 실제 physics 실행 후 최종 검사 | report: effective_scene/provenance/snapshot/checks |
| capture FILE --ticks N --output NEW.png [--view VIEW.json] [--colliders] | 동일 tick의 GPU PNG 저장 (`render` feature 필요) | image, capture metadata, snapshot, effective_scene, report |
| scene patch FILE PATCH | revision 검사, clone 변경, 전체 후보 검증 | **재입력 가능한 scene JSON 자체** |

표준출력은 한 개 JSON과 개행이다. patch 성공만 scene 문서 자체를 출력하며, 나머지
성공/실패는 `schema_version`, `ok`를 포함하는 envelope다. 실패 시 `error`에는
`code`, `path`, `message`가 있다. JSON 파싱 오류의 상세 line/column은 message에 있다.

Exit 0: 성공. run에 checks가 없으면 모든 검사 통과는 공집합에 대한 성공일 뿐 품질 보장이 아니다.
Exit 1: 시뮬레이션은 수행됐으나 최종 check 실패.
Exit 2: 사용법·입력·파일·스키마·수치·revision·자원 한도 오류.
Exit 3: 실행 중 수치 실패·내부 오류·출력 실패. 실패 world를 계속 실행하지 않는다.

capture는 N=0을 허용한다. 이때 초기 상태를 표시하고 최종 검사는 수행하지 않아 report=null이다.
N>0이면 run과 같은 검사를 수행한다. 검사에 실패해도 이미지와 보고를 남기고 exit 1을 반환한다.
GPU가 없으면 GPU_UNAVAILABLE, 렌더링 실패는 RENDER, 이미지 쓰기 실패는 OUTPUT/exit 3이다.
기본 빌드에는 capture가 없으며 UNSUPPORTED/exit 2를 반환한다. capabilities의 rendering은
기능 포함 여부이며 사용 가능한 GPU를 탐지했다는 의미가 아니다.

```sh
cargo run -q -p tomo-cli --features render --locked --offline -- capture examples/scenes/falling_box.scene.json --ticks 600 --output settled.png --view examples/views/default.view.json --colliders
```

캡처는 기존 파일을 덮어쓰지 않는다. 가로·세로는 각각 1..2048이다. View v1은 별도 카메라/해상도
문서이며 Scene v1 형식을 바꾸지 않는다. GPU readback/PNG 인코딩 후 새 파일을 생성한다.
파일과 stdout 보고 사이의 crash transaction은 제공하지 않는다. 상세: [RENDERING.md](RENDERING.md).

```sh
# 의도적으로 실패하는 검사: 종료 코드 1이어야 한다.
cargo run -q -p tomo-cli --locked --offline -- run examples/scenes/expected_failure.scene.json --ticks 120

# 새 파일에만 출력한다. 입력 파일 자체를 변경하는 명령은 제공하지 않는다.
cargo run -q -p tomo-cli --locked --offline -- scene patch examples/scenes/falling_box.scene.json examples/patches/raise_box.patch.json > raised.scene.json
```

**입력과 동일한 파일로 `>` 리다이렉트하지 않는다. 셸이 CLI 실행 전에 원본을 잘라 버릴 수 있다.**
PowerShell 등에서는 결과가 UTF-8로 저장되는지 확인한다. 장기적으로 원자적 저장은 별도
file transaction 기능으로 추가한다. 현재는 문서 clone 검증과 원본 불변성만 보장한다.

## v1 scene 계약

우수 좌표계, +Y 위, 미터·초·킬로그램. 회전은 단위 quaternion `[x,y,z,w]`.
`position`은 질량 중심, `half_extents`는 상자의 절반 크기다. 선형 속도는 m/s,
각속도는 rad/s. raw body IDs는 저장하지 않으며 stable 문자열 ID를 사용한다.

필수 root 필드: schema_version=1, revision, name, gravity, fixed_dt, bodies.
checks는 생략 시 빈 배열이다. body의 필수 필드는 id, motion, shape, position.
rotation=identity, mass=1, velocities=0, friction=0.5, restitution=0,
is_sensor=false, allow_sleep=true가 기본값이다. 비동적 body의 mass는 v1에서 기본 표식 1만 허용한다.

현재 scene은 primitive bodies만 노출한다. Physics 자체에 있는 거리 관절·force/ray API가
모두 이 scene schema에 노출된 것은 아니다. 프리팹·게임 로직·에셋 참조·컨트롤러는 후속 버전 대상이다.

checks는 **마지막 tick**에서 position의 포함 범위와 선형 speed 상한을 검사한다.
중간 프레임의 관통, 충돌 유무나 전체 동작 궤적을 검사했다고 해석하지 않는다.

## Patch 계약

op은 add/remove/set_transform/set_material. 순서대로 적용하고 전체 후보를 검사한다.
expected_revision은 입력 문서와 같아야 하고 성공 시 1 증가한다. 실패하면 후보는 버리고
원본은 유지한다. 존재하지 않는 ID, 중복 추가, 남은 check가 참조하는 body 삭제를 거부한다.
재시도 시 같은 원본+patch는 같은 후보를 만들지만 이미 새 revision인 문서에 재적용하면
충돌한다. 서버 측 요청 journal이나 exactly-once 처리는 아직 없다.

## 한도와 보안 경계

입력당 1 MiB, 256 bodies, 512 checks, patch당 1..256 operations.
fixed_dt는 2e-6..0.25초이고 기본 물리 substeps는 2다. CLI run은 1..3600 ticks,
누적 `max(body_count, 1)^2 * ticks <= 5,000,000`을 요구한다.
이는 작업량 guard이며 실행 시간/메모리 상한을 완전히 보장하는 OS sandbox가 아니다.
실제 적대적 입력 서비스에는 프로세스 CPU/메모리/시간 제한을 별도로 적용해야 한다.

파일 경로는 로컬 호출자가 지정한다. 네트워크 URL·provider·임의 소스 빌드를 실행하지 않는다.
경로 접근 제어, 취소, 장기 작업 복구, secure code execution은 향후 도구 서비스 단계다.

## 스키마와 재현 한계

`schemas/scene-v1.schema.json`, `schemas/patch-v1.schema.json`은 구조 명세다.
참조·수치·중복·physics 검증까지 포함하는 권위 있는 실행 검사는 Rust API다.
run 보고는 기본값이 적용되고 ID로 정렬된 scene, runtime version, Jolt SHA,
OS/architecture, runtime fma feature와 snapshot을 포함한다. 정확한 바이너리 해시,
compiler flags, lockfile hash와 장기 trace는 아직 포함하지 않는다.
동일 프로세스 재현 테스트를 플랫폼 간 bit-identical 결정론 증명으로 취급하지 않는다.

## 프로젝트 생성과 검사

```sh
cargo run -q -p tomo-cli --locked --offline -- project new ./MyGame --name "My Game"
cargo run -q -p tomo-cli --locked --offline -- project validate ./MyGame
```

new는 존재하지 않는 폴더에 project.tomo.json을 만든다. validate는 프로젝트/Scene/게임
참조·수치·실제 world 구축을 검사하며 파일을 바꾸지 않는다. 두 명령은 기본 CLI에 포함되고
GPU를 초기화하지 않는다. 에디터와 독립 플레이어가 동일 파일을 사용한다. 원자성·저장 충돌과
한도는 [EDITOR.md](EDITOR.md), 구조 명세는 schemas/project-v2.schema.json과 logic-v1.schema.json을 따른다.
기존 v1은 메모리에서 이행하고 validate는 원본을 덮어쓰지 않는다. capabilities는 프로젝트
v2/이전 버전 import/선언적 로직 지원을 표시한다. 임의 스크립트 실행은 지원하지 않는다.
