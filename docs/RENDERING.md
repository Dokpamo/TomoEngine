# Primitive GPU 캡처

`tomo-render`는 Scene v1과 해당 world의 Snapshot을 읽어 구·회전 상자를 렌더링한다.
물리 world를 수정하거나 다른 tick을 계산하지 않는다. 카메라의 원근 투영, 깊이 버퍼,
기본 방향광, collider 선 표시, GPU readback과 RGBA8 sRGB PNG 인코딩을 제공한다.
현재 API는 단일 이미지용이며 매번 장치를 생성한다. 실시간 게임 루프나 성능 최적화는 후속이다.

## 사용과 결과

```sh
cargo fetch --locked
cargo run -q -p tomo-cli --features render --locked --offline -- capture examples/scenes/primitives.scene.json --ticks 0 --output primitives.png --colliders
```

PNG는 새로운 경로에만 저장한다. stdout에는 동일 tick의 snapshot, 적용 scene, 카메라/해상도,
adapter/backend/device type과 body ID/위치/회전/형상/화면 중심 좌표가 포함된다.
`center_pixel`은 카메라 frustum 내부의 중심 투영이며 다른 물체에 가려졌는지를 판정하지 않는다.
clip 밖의 중심은 null이다. mesh의 실제 가시성은 GPU depth test가 처리한다.
tick 0은 초기 상태만 표시하므로 최종 검사 report가 없다. tick>0은 headless run과 동일한 검사를
유지하며 실패한 장면도 진단용 PNG를 남긴다. GPU 부재나 실패를 성공 이미지로 대체하지 않는다.

View v1 예제는 `examples/views/default.view.json`, 구조 명세는 `schemas/view-v1.schema.json`이다.
카메라 eye/target/up은 world의 오른손 좌표계와 +Y up, 단위 metre를 사용한다.
수평 pixel은 오른쪽, 수직 pixel은 아래쪽으로 증가한다. 가로/세로 각각 1..2048이며 scene은
기존 256 bodies 제한을 유지한다. 객체 정점은 snapshot의 quaternion/위치로 CPU에서 변환하고
WGSL vertex/fragment shader와 실제 GPU rasterization으로 화면을 만든다.

## 의존성과 native 경계 결정

Rust 1.85.0을 유지하기 위해 MSRV 1.84인 **wgpu 26.0.1**을 고정했다.
기본 feature를 끄고 std/parking_lot/metal/vulkan/dx12/wgsl만 사용한다.
Metal은 Apple GPU, Vulkan은 Linux/Android 후보, DX12는 Windows 후보이다.
API가 제공하는 backend와 실제 검증한 플랫폼은 구분한다. GLES/WebGPU/window surface는 현재 사용하지 않는다.
wgpu 30.0.1은 Rust 1.87 이상을 요구하므로 이번 고정 toolchain과 맞지 않는다.
버전 근거: https://docs.rs/crate/wgpu/26.0.1 및 각 crate의 배포 Cargo.toml.

- pollster 0.4.0: CLI에서 adapter/device/error-scope future를 기다리는 작은 executor.
- png 0.17.16: 외부 명령 없이 RGBA8 PNG를 인코딩하고 테스트에서 디코딩.
- serde: 기존 versioned view/metadata 형식. 별도의 벡터 수학/unsafe byte-cast 의존성은 추가하지 않았다.

Metal/Objective-C, Vulkan loader 및 Windows API 연동은 wgpu의 외부 계층에만 존재한다.
프로젝트가 작성한 Rust에는 forbid(unsafe_code)를 유지하고 physics에는 외부 dependency를 추가하지 않았다.
wgpu 및 전이 의존성 내부의 unsafe/FFI/build script를 없다고 주장하지 않는다.
`scripts/reviewed-dependencies.json`은 Cargo가 실제 해석한 127개 외부 패키지의 버전/source/license/
links/build-script 유무를 기록한다. 라이선스 선언은 MIT/Apache/기타 permissive 계열이며,
이를 전체 공급망 코드 감사 완료로 해석하지 않는다. 전체 metadata의 wasm_bindgen links는
target별 그래프에 포함된 선언이며 native 캡처 backend로 WebGPU를 활성화한 것은 아니다.
Cargo.lock의 checksum으로 배포본을 고정하고 Rust 호환 버전 resolver를 사용한다.
정책 검사는 manifest와 실제 graph를 비교하며 physics/scene/runtime의 transitive graph에
렌더러·GPU·native 의존성이 유입되는 것을 별도로 차단한다.

## 검증과 남은 범위

```sh
cargo test -p tomo-render --locked --offline --test capture -- --include-ignored
cargo test -p tomo-cli --features render --locked --offline --test cli -- --include-ignored
```

GPU 테스트는 adapter가 필요한 사실을 `ignore`로 표시하며, 위 명령 및 별도 CI job에서는
명시적으로 실행한다. adapter 실패를 skip/success로 바꾸지 않는다.
실제 픽셀에서 이동 방향/회전 종횡비, 구와 상자의 가림, collider overlay를 검사한다.
257px 비정렬 행의 GPU readback과 PNG roundtrip, CLI/headless snapshot 일치,
기존 파일 보존과 check 실패의 exit 1도 검사한다.

2026-10-10 로컬 Apple M4 / macOS Metal에서 위 검사를 통과하고 예제 PNG를 직접 확인했다.
`images/primitives.png`와 `validation/m2-metal.json`은 해당 실행의 실제 출력이다.
Linux 소프트웨어 Vulkan 검증은 CI의 gpu-capture job으로 별도 실행한다. 커밋별 성공 여부는
Actions 결과가 기준이다. Windows DX12 실기기, iOS/Android 실기기, 대화형 창/입력,
GLB/PBR/스키닝/게임 SDK는 아직 검증·구현하지 않았다.

다음 제품 목표는 primitive로 움직임/목적지/재시작/입력 재현이 되는 작은 desktop 게임이다.
일반 장면 객체와 물리 body의 분리, 실시간 수명과 CLI 자원 예산 분리도 이때 진행한다.
Jolt 원본 inventory와 differential test는 계속 독립 목표로 남는다.
