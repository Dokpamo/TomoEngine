# Tomo Editor와 게임 프로젝트

첫 제작 흐름은 **프로젝트 생성 → 장면 편집 → Play → Stop → 저장 → 독립 게임 실행**이다.
현재 게임 템플릿은 Box Run의 ‘플레이어가 목적지에 도착하기’ 규칙 하나다. 새로운 프로젝트와
장애물 배치, 형상·물리 속성, 플레이어/목적지 참조, 이동 속도는 편집할 수 있다.
범용 Rust 게임 SDK, 스크립트 편집기, GLB 에셋 브라우저는 아직 없다.

## 실행

```sh
cargo fetch --locked
cargo run -p tomo-editor --release --locked --offline
```

macOS 로컬 앱:

```sh
python3 scripts/package-macos.py --editor
open "target/Tomo Editor.app"
```

첫 창은 저장 전 My Game 템플릿이다. **Save**에서 새 폴더의 전체 경로를 입력하면 프로젝트가
생성된다. **New**는 새 템플릿, **Open**은 기존 폴더 또는 project.tomo.json, **Save as**는
현재 장면을 새 폴더로 복사한다. 새 폴더의 부모 디렉터리는 미리 존재해야 한다.
기존 폴더를 암묵적으로 덮어쓰지 않는다. 이름 옆 `*`는 저장되지 않은 문서다.
닫기·새로 만들기·열기는 변경 폐기 여부를 확인하며, 열기 실패 시 현재 장면을 유지한다.
UI는 현재 영어이며 기본 폰트의 문자 지원은 제한적이다. 프로젝트 파일은 UTF-8이다.

## 편집과 플레이

| 조작 | 결과 |
| --- | --- |
| 왼쪽 목록 / 화면의 물체 클릭 | 안정적인 body ID 선택, 민트색 강조 |
| + Box / + Sphere | 고유 ID를 가진 정적 물체 생성 |
| Duplicate / Delete | 복제 / 삭제. player·goal 및 check 참조가 끊기는 삭제는 거부 |
| 오른쪽 속성 | 위치, XYZ 회전, 크기/반지름, motion, 질량, 마찰, 반발, sensor, 초기 속도 |
| 물체 왼쪽 드래그 | 현재 높이를 유지하며 X/Z 평면 이동. 선택적으로 0.25m 스냅 |
| 오른쪽 드래그 / 가운데 드래그 / 휠 | 카메라 회전 / 이동 / 줌 |
| Focus 또는 F / Reset view | 선택한 물체 보기 / 시작 카메라 |
| Undo / Redo | 최대 64개 문서 이력. 한 번의 드래그는 한 번에 되돌림 |
| Cmd/Ctrl+Z, Cmd/Ctrl+Shift+Z | 문서 Undo/Redo. 텍스트 입력 중에는 입력기의 편집 동작 사용 |
| Cmd/Ctrl+S | 저장 |
| Game settings | 프로젝트 이름, player ID, goal ID, 이동 속도 |
| Play / Pause / Stop | 독립 world 생성 / 중지 / world 폐기 후 편집 장면 복원 |
| 플레이 중 WASD / 방향키 | 기존 물리 기반 속도 컨트롤러로 이동 |

플레이 모드에서는 문서 속성을 수정하지 않는다. Stop은 게임 진행 결과를 문서에 기록하지
않으며, 재실행은 처음부터다. 에디터 카메라는 세션의 관찰 설정으로 파일에 저장하지 않는다.
이 첫 템플릿의 독립 플레이어는 기존 Box Run 카메라를 사용한다. 카메라 컴포넌트·일반 엔티티/
계층·개별 재질·prefab·여러 scene·스크립트·빌드/배포 버튼은 후속이다.

## 파일과 CLI

```sh
cargo run -q -p tomo-cli --locked --offline -- project new ./MyGame --name "My Game"
cargo run -q -p tomo-cli --locked --offline -- project validate ./MyGame
cargo run -p tomo-editor --release --locked --offline -- --project ./MyGame
cargo run -p tomo-play --release --locked --offline -- --project ./MyGame
cargo run -q -p tomo-play --locked --offline -- --project ./MyGame --replay examples/replays/box_run_win.replay.json
```

마지막 명령의 승리 기대는 기본 배치를 바꾸기 전의 템플릿용이다. 장면이 달라지면 입력도
수정해야 한다. `--project`는 다른 player 옵션보다 앞에 둔다. `--capture NEW.png`도 함께 쓸 수 있다.

```text
MyGame/
  project.tomo.json     프로젝트 v1: 이름, revision, Scene v1, game 설정
```

작동하지 않는 assets/scripts 폴더나 빈 코드를 만들지 않는다. 장면과 규칙을 한 문서에 저장해
두 파일 사이의 불일치를 피한다. `schemas/project-v1.schema.json`은 구조 설명이고, 의미 검사는
CLI/Rust 검증기가 담당한다. 파일 최대 1 MiB, 최대 256 bodies, unknown 필드/버전 거부,
유한 수치·고유 ID·참조·단위 quaternion과 실제 physics world 구축을 확인한다.
게임은 dynamic solid player와 axis-aligned static sensor box goal을 요구한다. 목표 크기는
플레이어 bounding sphere보다 커야 하며, 이동 속도는 0.1..30m/s다. 외부 코드나 provider는 실행하지 않는다.
replay는 36000 tick/1024 segment 및 body 수 제곱 × 요청 tick <= 5,000,000을 실행 전에 검사한다.

## 변경·저장·복구 계약

문서 편집은 완전한 후보와 초기 world를 검증한 뒤 한 번에 교체한다. revision이 다르면
거부한다. 정상 변경과 Undo/Redo 모두 project/scene revision을 증가시키며 overflow도 거부한다.
새로 열기/새 프로젝트는 이력을 초기화한다. 이력은 메모리에만 있고 프로그램 종료 후 복구되지 않는다.

저장은 create_new 방식의 `project.tomo.json.lock`으로 Tomo writer끼리 협력하고, 마지막에
읽거나 저장한 원문 바이트와 현재 디스크를 비교한다. 외부 변경이 있으면 CONFLICT로 실패한다.
새 문서를 같은 폴더의 `project.tomo.json.tmp`에 쓰고 sync_all 후 rename으로 교체한다.
검증/쓰기/rename 실패 시 정상 원본을 유지한다. 기존 lock/tmp를 지우거나 채택하지 않는다.
프로세스가 강제 종료되면 lock/tmp가 남을 수 있다. 다른 writer가 종료됐는지와 tmp의 복구
필요성을 확인한 뒤 사용자가 보존/제거하고 다시 저장한다. 원본이 사라지면 Save as로 복구할 수 있다.

이 계약은 임의의 외부 프로그램이 검사 직후 원본을 덮어쓰는 경합, 디렉터리 메타데이터의
전원 장애 내구성, 원격 exactly-once 작업까지 보장하지 않는다. 물리 수치 실패도 문서 rollback과
다르다. 실패한 플레이 world는 재개하지 않고 Stop 후 새 world를 만들어야 한다.

## 검증 범위

```sh
cargo test -p tomo-project -p tomo-editor --locked --offline
cargo test -p tomo-render --locked --offline --test picking
cargo run -p tomo-editor --locked --offline -- --smoke-test
```

문서/저장 검사는 strict JSON, 잘못된 후보의 원본 유지, revision 충돌·overflow, 드래그 이력,
ID·참조 보호, 디스크 충돌·lock·tmp 보존, 저장 왕복, 변경한 역할/목적지/속도의 실행을 다룬다.
headless egui 입력 검사는 실제 버튼과 pointer event로 추가·복제·Undo·Play/Pause/Stop,
닫기 확인, 드래그를 수행한다. picking은 회전 box/sphere, 깊이, 편집 평면을 검사한다.

`--smoke-test`는 실제 egui atlas/mesh와 3D viewport를 같은 wgpu surface에 표시하고,
물체 추가·플레이 이동·정지 후 초기 장면 보존·resize와 120회 present를 확인한다. 30초 내
완료하지 못하면 오류다. 자동 smoke는 OS 키보드를 생성하는 검사가 아니다.
2026-10-10 로컬 Apple M4/macOS Metal 창에서 선택·드래그·Undo·키보드 이동·정지·저장·재열기를
직접 확인했다. Linux Vulkan 창은 최신 CI 결과로 구분하고, Windows DX12 실기기/모바일은
아직 검증하지 않았다. 물리 원본 동등성 범위는 이 기능으로 확대되지 않는다.
