# 게임 제작 v3

외부 모델 없이 상자·구·빈 객체로 여러 장면의 게임을 제작하는 현재 구현이다.
GLB/PBR/스켈레톤·캐릭터 애니메이션, Tripo와 Jolt 전체 이식은 이번 소유자 요청에서 제외했다.
범용 ECS, 노드 그래프, 모든 종류의 UI/음원 처리를 구현했다는 뜻은 아니다.

## 에디터

```sh
cargo fetch --locked
cargo run -p tomo-editor --release --locked --offline -- --project examples/projects/creator_lab
```

**Studio**에서 Objects / Cameras / Scenes / Prefabs / Game UI / Audio를 선택한다.
객체와 카메라 속성을 편집하고 **Apply**하면 전체 후보 검증 후 한 번의 Undo로 기록한다.
**Done**으로 닫고 **Play**한다. 기존 **Logic** 창에서 이벤트·조건·동작을 연결한다.
미적용 초안은 Discard로 버리고, 다른 문서 revision에 오래된 초안을 적용하면 충돌한다.
Studio와 Logic 창이 열려 있으면 미리보기를 멈춘다. 게임의 Play 상태는 편집 문서를 바꾸지 않는다.
프로젝트 저장과 게임 진행 저장은 별개의 작업이다.

- Objects: 독립 visual 상자/구, 빈 부모, 부모·자식 계층, 위치·회전·균일 스케일,
  회전 속도, 태그, 색·표시 여부. 루트 객체는 기존 물리 body에 연결할 수 있다.
  visual만 추가하면 물리 body는 늘어나지 않는다. body 연결 visual은 기본 body 표시를 대체한다.
- Cameras: 원근 카메라 여러 개, 활성 카메라 선택, 편집 시점 복사, entity 위치 따라가기.
- Scenes: 현재 장면 복제, 장면 전환, 다음 장면 ID. Play에서 Next로 새 world를 구성한다.
  플레이어/목적지가 없는 게임도 만들 수 있다. 이때 MovePlayer/GoalReached 규칙은 사용할 수 없다.
- Prefabs: 루트와 자식/연결 body를 템플릿으로 저장하고 새 ID 접두사와 위치로 복제한다.
  이미 배치한 복사본은 독립적이며 템플릿 수정이 자동 전파되지 않는다.
- Game UI: label, score, timer, status, score progress, restart/pause/play/save/load/next 버튼.
  위치/크기는 화면 비율이다. 현재 게임 HUD 글자는 대문자 ASCII·숫자와 ` .:/-+`다.
- Audio: 시작·점수·승리·패배·특정 규칙 실행에 주파수/길이/음량 효과음을 연결한다.
  실제 OS 오디오로 합성음을 출력한다. 음원 파일/음악 스트리밍/공간 음향은 지원하지 않는다.

`creator_lab`은 두 장면, 장식 객체/프리팹, 점프·점수·시간 제한, UI와 효과음을 함께 사용한다.
프로젝트 v1/v2는 엄격하게 읽어 메모리에서 v3로 이행하고, 저장은 v3로 한다.
`schemas/project-v3.schema.json`, `content-v1.schema.json`이 공개 데이터 구조다.
checkpoint/save/code-edit 요청도 각각 v1 스키마가 있다. JSON Schema 도구에서는 저장소 스키마를
각 `$id`로 등록해 URN 참조를 해석하며 네트워크에서 스키마를 가져오지 않는다.
Rust의 의미 검증이 참조/순환/작업 예산도 검사한다. JSON Schema만 통과해도 실행 가능한 것은 아니다.

## 진행 저장과 수명주기

게임 창의 F5/F9 또는 제작한 Save/Load 버튼을 사용한다. 에디터 미리보기에서는 Game 메뉴를 쓴다.
직접 연 프로젝트는 같은 폴더의 `progress.tomo.json`, 패키지 앱은 OS 사용자 데이터 영역에 저장한다.
Android는 앱의 private data, iOS는 Documents, macOS는 Application Support,
Windows는 LOCALAPPDATA, Linux는 HOME/.local/share 아래를 사용한다.

Save v1은 프로젝트와 버전 있는 체크포인트를 포함한다. 같은 프로젝트 revision·엔진 버전·FMA
설정인지 확인하고 최초 world에서 입력을 재실행해 contact cache까지 복원한 후보를 교체한다.
다른 프로젝트의 기존 슬롯은 덮어쓰지 않는다. scene 전환도 저장/복원한다.
최대 36,000 tick, 압축 입력 1,024 구간, 5,000,000 work units와 1 MiB 파일 예산을 적용한다.
게임은 예산 이후에도 계속하지만 저장은 SAVE_LIMIT을 반환한다. 임의 플랫폼 간 bit parity,
외부 서비스 복원, 모든 장시간 세션 저장을 보장하지 않는다.

일시정지·포커스 상실·화면 중단 시 키/터치/프레임 누적과 재생 중인 합성음을 비운다.
중단된 surface는 버리고 복귀 시 재생성하며 메모리의 게임은 유지한다.
운영체제가 프로세스를 종료하면 명시적으로 저장한 슬롯에서만 복원한다. 자동 저장은 하지 않는다.
모바일은 UP/LEFT/RIGHT/DOWN/ACTION 터치 패드를 추가로 표시한다.
이 영역 위에 사용자 버튼을 겹쳐 놓으면 사용자 버튼이 우선한다.

## 컴파일하는 Rust SDK

```sh
cargo run -p tomo-sdk-demo --locked --offline -- --headless
cargo run -p tomo-sdk-demo --release --locked --offline
```

`examples/rust_game/src/main.rs`가 실제 실행 예제다. `tomo_game::sdk::Behavior`를 구현하고
매 고정 tick 이전에 검증된 Action 목록(최대 8개)을 반환한다. 입력과 현재 snapshot/점수 등을
읽고 body 속도·점수·종료를 바꿀 수 있다. `ScriptedGame`은 headless 호스트,
`tomo_play::desktop::run_scripted`는 실제 창 호스트다. Restart는 behavior.reset도 호출한다.
반환 오류나 잘못된 명령은 게임을 fail-stop으로 만든다. 종료 동작은 그 tick의 물리 전에 적용된다.

SDK는 신뢰하는 native Rust 코드다. JSON 문자열을 eval하지 않으며 tick 함수의 CPU 사용을
런타임에서 격리하지 않는다. 일반 선언적 저장에 native behavior의 내부 상태를 거짓으로 포함하지
않는다. SDK 게임의 자체 영속 상태는 사용자 호스트에서 정의해야 하며 기본 창은 Save/Load를
거부한다. 노드 그래프, 런타임 Rust 컴파일/핫리로드는 없다.

## AI 문서 편집·재시도·rollback

```sh
cargo run -q -p tomo-cli --locked --offline -- project inspect PROJECT_DIR
cargo run -q -p tomo-cli --locked --offline -- project edit PROJECT_DIR request.json
cargo run -q -p tomo-cli --locked --offline -- project rollback PROJECT_DIR edit_1 --request-id undo_1 --expected-revision 1
```

요청은 `{ "schema_version":1, "request_id":"edit_1", "expected_revision":0, "candidate":PROJECT_V3 }`다.
완전한 후보를 검증한 뒤 revision을 증가시키며 `.tomo-journal`에 prepared/applied와 before/after를
남긴다. 같은 ID/동일 요청은 재적용하지 않고 영수증을 돌려준다. 다른 내용의 ID 재사용은 거부한다.
prepared 이후 중단된 경우 현재 문서가 before 또는 after인지 검사해 이어간다.
rollback도 새 ID와 새 revision을 가지는 편집이다. 원래 after 이후 다른 편집이 있으면 충돌한다.
최대 64개 작업 기록을 보존하며 가득 차면 완료 기록을 별도로 보관한 뒤 정리해야 한다.

lock은 이 도구를 사용하는 writer끼리의 협조적 잠금이다. 기존 복구용 임시 파일은 덮어쓰지 않는다.
문서/로컬 파일만 복원하며 실행 중 world, Git, 컴파일 결과, 네트워크 작업을 취소하지 않는다.
외부 작업의 exactly-once 실행은 보장하지 않는다. provider/API 키/자율 LLM 루프는 포함하지 않는다.

## AI Rust 소스 편집

```sh
python3 scripts/code-edit.py request.json --check
python3 scripts/code-edit.py --rollback edit_1 --request-id undo_1 --check
python3 scripts/test-code-edit.py
```

요청 v1의 필드는 `schema_version`, `request_id`, `path`, `expected_sha256`, `content`이며
다른 필드는 거부한다. 경로는 기존 `examples/rust_game/src/*.rs`만 허용하고 내용은 256 KiB 이하다.
변경 전 SHA-256을 비교하고 `.tomo-code-journal`에 before/after와 검증 결과를 기록한다.
재시도·준비 단계 복구·충돌·rollback을 지원하며 기록은 최대 32개다.

`--check`는 macOS에서 별도 소스 복사본을 만들어 고정된 Cargo check만 실행한다.
환경을 정리하고 sandbox-exec로 네트워크/허용 경로 밖 파일 접근을 금지하며 90초 프로세스 그룹
제한을 적용한다. 시스템·toolchain·검토한 Cargo cache·저장소는 읽을 수 있으므로 적대적 코드에
대한 완전한 VM 보안 경계라고 주장하지 않는다. Rust 자체는 실행하지 않아도 매크로/빌드 도구가
실행될 수 있다. 실패하면 원본을 유지한다. 다른 OS의 격리 compiler runner는 미구현이며
`--check`를 거부한다. 옵션 없는 실행은 데이터 편집만 하고 컴파일 검증 성공을 주장하지 않는다.

패키징과 실제 플랫폼 증거는 [PLATFORMS.md](PLATFORMS.md)를 따른다.
