> 제작 v3 업데이트: 현재 프로젝트는 v3로 저장하며 v1/v2를 읽습니다. 일반 객체·카메라·장면·
> 프리팹·UI·효과음·진행 저장·Rust SDK·AI 편집 사용법은 [AUTHORING.md](AUTHORING.md),
> 패키징과 플랫폼 검증은 [PLATFORMS.md](PLATFORMS.md)를 참고하세요. 아래 기존 v1/v2 설명은 해당 형식/기능의 계약입니다.

# 에디터에서 게임 규칙 만들기

에디터 상단 **Logic**에서 이벤트(WHEN), 모든 조건(IF), 순서 있는 동작(DO)을 조합한다.
기본 이동과 목적지 승리도 같은 규칙으로 표현하므로 수정·비활성화·삭제할 수 있다.
이 창은 선언적 규칙을 편집한다. 컴파일하는 Rust SDK는 AUTHORING.md를 따른다. 노드 그래프/임의 함수 eval은 없다.

## 처음 사용하기

1. **Logic → + Space jump**로 Space를 누를 때 player의 속도를 `[0, 6, 0]`m/s로 바꾼다.
2. **+ Time limit**로 시작 후 600 tick에 실패하는 규칙을 추가한다. 기본 60Hz에서는 10초다.
3. **Apply rules → Done → Play**를 누른다. WASD/방향키로 이동하고 Space로 점프한다.
4. **Logic**을 다시 열면 미리보기가 멈추고 규칙별 실행 횟수·최근 실행 규칙·점수를 볼 수 있다.
5. **Done → Stop → Save**로 규칙을 포함한 프로젝트를 저장한다. `tomo-play --project 폴더`도
   동일한 `tomo-game` 실행기를 사용한다.

Space jump는 단순한 속도 설정이다. 공중에서도 다시 사용할 수 있고 X/Z 속도도 0으로
설정한다. 다음 tick의 이동 규칙이 수평 속도를 다시 정한다. 접지 판정 캐릭터 컨트롤러는 후속이다.

**+ Rule**은 ‘시작할 때 점수 1 추가’ 규칙을 만든다. 이름·이벤트·조건·동작을 바꿔 사용한다.
**+ Contact score**는 player와 goal의 첫 접촉에 점수를 준다. Body B를 장애물이나 다른 sensor로
바꿀 수 있다. **Move up/down**은 규칙 순서, **Earlier**는 동작 순서를 바꾼다.
동작은 여러 개 넣을 수 있지만 Win/Lose는 반드시 마지막이어야 한다.

편집 중에는 초안을 유지한다. Apply는 완전한 후보·참조·예산·초기 물리 world를 검증하고
revision이 일치할 때만 문서에 반영한다. 한 번의 Apply는 하나의 Undo다. 잘못된 후보는
문서·world·이력을 바꾸지 않는다. **Reload current**는 초안을 버리고 현재 문서를 다시 읽는다.
**Discard & close**도 초안만 버린다. Apply 전에는 Save/Play/파일 전환을 막고, 창 닫기도
초안을 처리한 다음 진행한다. 적용한 문서도 Save 전에는 디스크에 기록되지 않는다.

## 이벤트·조건·동작

| 종류 | 지원 항목 | 의미 |
| --- | --- | --- |
| WHEN | Game starts | 첫 실행 tick에 한 번 검사 |
| WHEN | Every tick | 실행한 물리 tick마다 검사 |
| WHEN | Key pressed | W/위, S/아래, A/왼쪽, D/오른쪽, Space의 released → held 전이 |
| WHEN | Timer | 1..36000 tick, 선택적으로 주기 반복 |
| WHEN | Contact | 실제 physics Started/Persisted/Stopped, sensor 포함 |
| WHEN | Goal reached | 기존 목적지의 보수적인 수평 포함·높이 판정 |
| IF | Key held / Score at least / Height below | 모든 조건이 참일 때 실행; 조건이 없으면 항상 허용 |
| DO | Move player with input | game.player와 game.speed로 기존 upright 이동 컨트롤러 실행 |
| DO | Set velocity | 지정한 dynamic/kinematic body의 월드 선형 속도 교체; 각속도 보존 |
| DO | Add score | 부호 있는 정수 점수 변경 |
| DO | Win game / Lose game | 해당 라운드 종료, 다음 규칙·동작 생략 |

`Enabled`를 꺼도 구조·참조는 검증한다. `Once per run`은 이벤트와 조건이 일치해 실행된 뒤에만
소비한다. 반복 Timer도 Once를 켜 두면 한 번만 실행된다. 조건이 거짓인 Start/단발 Timer를
나중에 다시 시도하지 않는다. 매 tick 재검사가 필요하면 Every tick을 쓴다.
Start clock은 ‘첫 이동 또는 Space’와 ‘Play 즉시’ 중 선택한다. UI 프레임·wall clock 대신
실제 수행된 simulation tick을 사용하며, pause/포커스 해제 중에는 타이머가 진행되지 않는다.
키 입력은 tick 단위 상태다. 두 simulation tick 사이에서 완전히 끝난 아주 짧은 탭은 놓칠 수 있다.

## 실행 순서와 실패 계약

1. 입력/시작/Every tick/Timer 규칙을 목록 순서대로 평가한다.
2. 실제 physics를 한 tick 실행한다.
3. Contact/Goal reached 규칙을 목록 순서대로 평가한다.

각 단계의 뒤쪽 규칙은 앞쪽 동작이 만든 점수·속도를 본다. Contact/Goal 판정 위치는 완료된
tick의 snapshot이다. Contact 뒤의 속도 설정은 다음 물리 tick에 이동으로 반영된다.
두 단계 사이의 규칙 순서를 한 목록의 전역 실행 순서로 해석하지 않는다. 예를 들어 같은 tick의
pre-physics 제한시간 실패는 post-physics 목적지 승리보다 먼저 처리된다.
Win/Lose 이후에는 해당 tick의 physics를 완료하고 동결한다. terminal 상태를 다시 step해도
게임이 진행되지 않는다. Stop/재시작은 원본 문서에서 새 world를 만들고 점수·키 edge·타이머·
Once·실행 횟수·최근 기록을 초기화한다. 독립 실행기의 세션 최고 승리 시간만 유지한다.

접촉은 `tomo-runtime::contact_events`가 안정적인 scene ID로 변환한 실제 엔진 이벤트다.
physics 내부 substep에서 같은 쌍/종류가 여러 번 발생해도 규칙은 tick당 최대 한 번 실행한다.
쌍의 A/B 순서는 의미가 없고, 최소 한 body가 dynamic이어야 한다. contact는 검출 단계의
접촉 증거이며 연속 충돌 보장이나 Jolt 원본 동등성 증명이 아니다. physics 소스는 변경하지 않았다.

실행 중 점수 한도 또는 physics 오류가 나면 game은 fail-stop이 된다. 이미 실행한 동작을
rollback하지 않으며 재시작 전까지 더 진행하지 않는다. 문서 원자성과 구분한다.
오류를 정상 성공/NaN JSON으로 직렬화하지 않는다. standalone의 RULE_EXECUTION 오류는 exit 3이다.

## 형식·호환성·한도

- Project v3의 `game.logic`에 Logic v1을 필수로 저장한다. Scene은 v1 그대로다.
- 기존 Project v1/v2는 엄격한 구형 필드 검증 후 메모리에서 v3로 변환한다. player/goal/speed와
  revision을 유지하고 이동·목적지 규칙 2개를 만든다. Open/validate는 파일을 쓰지 않으며,
  Save할 때 v3로 저장한다. 구형 바이너리는 새 v3 파일을 읽을 수 없다.
- 최대 64 rules, 각 8 conditions와 1..8 actions, 실행 기록 최근 32개, 파일 전체 1 MiB.
  빈 규칙 목록도 유효하며 그 경우 물리만 진행한다. 프로그램에 루프·재귀·외부 호출은 없다.
- ID는 고유한 ASCII 1..64자, 이름은 제어 문자 없는 UTF-8 1..128 bytes다.
  알 수 없는 필드/종류/버전, 끊어진 참조, static body 속도 변경, NaN/Infinity를 거부한다.
- Set velocity는 축별 ±100m/s, height는 ±10000m, 점수 변경/조건은 ±1,000,000이다.
  실행 점수는 ±1,000,000,000이며 초과하면 중단한다.
- 기존 body 최대 256, player(dynamic solid)/goal(static axis-aligned sensor box) 역할·크기
  제약은 유지된다. 물체 삭제는 비활성 규칙의 참조까지 포함해 검사한다.
- Replay v1은 기존 방향 입력만 읽고, v2가 `action`(Space)을 추가한다. 실행 전 36000 tick,
  1024 segment, body² × 요청 ticks ≤ 5,000,000과 별도로
  `(규칙 수 + 조건 수 + 동작 수) × 요청 ticks ≤ 5,000,000`을 검사한다.

구조 스키마는 `schemas/project-v3.schema.json`, `logic-v1.schema.json`, `replay-v2.schema.json`.
참조·유한 수치·UTF-8 바이트·순서·실행 예산의 권위 있는 검사는 Rust 검증기다.
replay의 `expect_win: false`는 미승리 상태 전체를 허용한다. 정확한 Lost/점수 판정은 JSON의
`status`/`score`를 검사한다. `rule_events`는 최근 32회라 전체 영속 trace/journal이 아니다.

## 예제와 검증

`Logic Lab`은 이동, Space 점프, 첫 바닥 접촉 점수, 점수가 있어야 목적지 승리, 30초 제한으로
구성된다. **Open**으로 `examples/projects/logic_lab`을 열거나 아래 명령을 사용한다.
자신의 게임으로 만들 때 **Save as**로 저장소 밖의 새 프로젝트 폴더에 복사할 수 있다.

```sh
cargo run -p tomo-editor --release --locked --offline -- --project examples/projects/logic_lab
cargo run -q -p tomo-cli --locked --offline -- project validate examples/projects/logic_lab
cargo run -q -p tomo-play --locked --offline -- --project examples/projects/logic_lab --replay examples/replays/logic_lab_timeout.replay.json
cargo test -p tomo-game -p tomo-project -p tomo-editor -p tomo-play --locked --offline
```

timeout 예제의 결과는 Lost, score 1, tick 1800이다. 기본 Box Run 승리 replay도 변경 없이
514 tick에 도착한다. 자동 검사는 timer/edge/contact/순서/Once/조건/종료/재시작/fail-stop,
schema·예산 거부, v1 migration, 참조 보호, 실제 egui 버튼으로 규칙 작성·Apply·Undo와
저장/재열기 후 동일 입력의 preview/standalone 결과를 비교한다. 실제 GPU/OS 검증은
EDITOR.md와 해당 commit의 CI 결과를 구분해 따른다. 새 의존성은 추가하지 않았다.
