# Primitive GPU 캡처

`tomo-render`는 Scene v1과 해당 world의 Snapshot을 읽어 구·회전 상자를 렌더링한다.
물리 world를 수정하거나 다른 tick을 계산하지 않는다. 카메라의 원근 투영, 깊이 버퍼,
기본 방향광, collider 선 표시, GPU readback과 RGBA8 sRGB PNG 인코딩을 제공한다.
`capture`는 단일 이미지용이며 매번 장치를 생성한다. `WindowRenderer`는 장치/파이프라인과
grow-only 정점 버퍼를 유지하며 surface에 출력한다. resize 때 surface/깊이 버퍼를 다시 설정한다.
게임 루프와 입력은 `tools/tomo-play`에 있으며 GAME.md를 따른다.

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
API가 제공하는 backend와 실제 검증한 플랫폼은 구분한다. GLES/WebGPU는 현재 사용하지 않는다.
wgpu 30.0.1은 Rust 1.87 이상을 요구하므로 이번 고정 toolchain과 맞지 않는다.
버전 근거: https://docs.rs/crate/wgpu/26.0.1 및 각 crate의 배포 Cargo.toml.

- pollster 0.4.0: CLI에서 adapter/device/error-scope future를 기다리는 작은 executor.
- png 0.17.16: 외부 명령 없이 RGBA8 PNG를 인코딩하고 테스트에서 디코딩.
- serde: 기존 versioned view/metadata 형식. 별도의 벡터 수학/unsafe byte-cast 의존성은 추가하지 않았다.
- winit 0.30.13 (MSRV 1.70, Apache-2.0): `tomo-play`/`tomo-editor`의 창/키보드/수명주기.
  default feature를 끄고 rwh_06/x11/wayland/wayland-dlopen만 사용한다.
  근거: https://docs.rs/crate/winit/0.30.13 및 배포 Cargo.toml. client decoration/font 라이브러리는
  활성화하지 않는다. HUD는 프로젝트의 작은 ASCII bitmap alphabet으로 그리며 추가 폰트가 없다.

Metal/Objective-C, Vulkan loader 및 Windows API 연동은 wgpu의 외부 계층에만 존재한다.
프로젝트가 작성한 Rust에는 forbid(unsafe_code)를 유지하고 physics에는 외부 dependency를 추가하지 않았다.
wgpu 및 전이 의존성 내부의 unsafe/FFI/build script를 없다고 주장하지 않는다.
`scripts/reviewed-dependencies.json`은 Cargo가 실제 해석한 282개 외부 패키지의 버전/source/license/
links/build-script 유무를 기록한다. 라이선스 선언은 MIT/Apache/기타 permissive 계열이며,
이를 전체 공급망 코드 감사 완료로 해석하지 않는다. 전체 metadata의 wasm_bindgen links는
target별 그래프에 포함된 선언이며 native 캡처 backend로 WebGPU를 활성화한 것은 아니다.
Cargo.lock의 checksum으로 배포본을 고정하고 Rust 호환 버전 resolver를 사용한다.
Box Run 추가 시 winit 전이 그래프의 Objective-C/AppKit, X11/Wayland/xkbcommon/dlopen,
target별 Android JNI/NDK·Windows·Redox·Wasm 경계를 검토했다. objc-sys의 links와
windowing build script는 외부 OS 계층에만 허용한다. native-free로 표시하지 않는다.
여러 라이선스 선택지가 있는 패키지는 MIT/Apache 등 permissive 선택지가 있는지 확인했다.
이는 버전·선언·계층 검토이며 전이 코드 전체 보안 감사나 모바일 실행 검증이 아니다.
정책 검사는 manifest와 실제 graph를 비교하며 physics/scene/runtime/game/project의 transitive graph에
렌더러·GPU·native 의존성이 유입되는 것을 별도로 차단한다.

## 검증과 남은 범위

```sh
cargo test -p tomo-render --locked --offline --test capture -- --include-ignored
cargo test -p tomo-cli --features render --locked --offline --test cli -- --include-ignored
cargo test -p tomo-play --locked --offline -- --include-ignored
cargo run -p tomo-play --locked --offline -- --smoke-test
```

GPU 테스트는 adapter가 필요한 사실을 `ignore`로 표시하며, 위 명령 및 별도 CI job에서는
명시적으로 실행한다. adapter 실패를 skip/success로 바꾸지 않는다.
실제 픽셀에서 이동 방향/회전 종횡비, 구와 상자의 가림, collider overlay를 검사한다.
257px 비정렬 행의 GPU readback과 PNG roundtrip, CLI/headless snapshot 일치,
기존 파일 보존과 check 실패의 exit 1도 검사한다.

2026-10-10 로컬 Apple M4 / macOS Metal에서 위 검사를 통과하고 예제 PNG를 직접 확인했다.
`images/primitives.png`와 `validation/m2-metal.json`은 해당 실행의 실제 출력이다.
Linux 소프트웨어 Vulkan 검증은 CI의 gpu-capture job으로 별도 실행한다. 커밋별 성공 여부는
Actions 결과가 기준이다. macOS 대화형 창/입력은 Box Run에서 검증했다. Windows DX12 실기기, iOS/Android 실기기,
GLB/PBR/스키닝은 미구현이다. 제작 v3의 SDK/모바일 증거는 AUTHORING.md와 PLATFORMS.md를 따른다.

Box Run은 움직임/목적지/재시작/입력 재현과 실시간 수명/CLI 예산 분리를 구현했다.
제작 v3에서 일반 visual entity와 물리 body를 분리했다. 파일 에셋 렌더링은 후속이다.
Jolt 원본 inventory와 differential test는 계속 독립 목표로 남는다.

## Editor backend 의존성 결정

Rust 1.85.0을 유지하는 **egui / egui-winit 0.32.3**을 고정했다. egui는 widgets/텍스트 편집과
CPU tessellation, egui-winit은 창 입력·IME·텍스트 clipboard 연결을 담당한다. 배포 manifest의
MSRV 1.85와 MIT OR Apache-2.0을 확인했다. 근거: https://docs.rs/crate/egui/0.32.3 및
https://docs.rs/crate/egui-winit/0.32.3 . egui-winit default features는 끄고 clipboard/x11/wayland만 켰다.
브라우저 링크 실행, accesskit, 별도 egui-wgpu/eframe backend는 활성화하지 않는다.

`crates/tomo-render/src/gui.rs`의 선택적 painter는 기존 wgpu 26 장치에 atlas·mesh·scissor를
올리고, 같은 surface의 3D viewport 뒤에 패널을 합성한다. project Rust는 unsafe 없이
정점 바이트를 작성한다. egui managed texture/mesh용이며 외부 paint callback은 거부한다.
GUI atlas 최대 4096²/64개, 한 프레임 최대 2,000,000 정점이며, 폰트 갱신은 surface 재시도에도
보존한다. shader는 egui의 gamma RGBA를 sRGB surface용 linear RGB로 변환한다.

추가 38개 전이 패키지를 버전·source·license·links/build-script·도달 계층 기준으로 검토했다.
ab_glyph/ttf-parser 및 epaint 기본 폰트는 텍스트 표시용이다. 폰트의 OFL-1.1,
Ubuntu-font-1.0, Hack/emoji 고지문을 LICENSES/editor-fonts에 보존하고 로컬 .app에도 복사한다.
arboard/clipboard-win/Objective-C AppKit·CoreGraphics·IOSurface/dispatch2와
smithay-clipboard/Wayland/calloop는 외부 OS clipboard 계층이다. arboard의 image-data feature가
image/png 0.18/tiff/zune/fax/weezl/moxcms/pxfm 전이 코드를 포함하지만, 에디터의 자산 importer가
구현됐다는 뜻은 아니다. 이들의 라이선스는 MIT/Apache/BSL/Zlib/BSD 등 permissive 선택지를
확인했다. objc2와 smithay-client-toolkit의 외부 build script를 허용하고 core 유입은 금지한다.
기존 lockfile 버전은 유지했고 정책 allowlist는 282개 외부 패키지를 정확히 비교한다.
이는 선언·버전·계층 검토이며 전이 코드 전체 보안 감사나 unsafe-free 보장이 아니다.

에디터의 실제 창·atlas·resize 검사는 `cargo run -p tomo-editor --locked --offline -- --smoke-test`다.
자동 GUI/문서 테스트와 수동 조작 범위는 EDITOR.md를 따른다.


## Audio output boundary (authoring v3)

CPAL **0.15.3** (Apache-2.0) is pinned as an optional `tomo-render/audio` OS adapter,
selected by the editor and player. Its purpose is real default-device audio output;
CPU-only scene/game/project/physics crates remain independent of CPAL. Reference:
https://docs.rs/cpal/0.15.3/cpal/ . Tones are synthesized with attack/release envelopes,
32 concurrent voices, a 64-message queue and bounded gain. Queue/device errors are
reported; headless runs never open a device. This is synthesized effects, not a claim
of compressed audio decoding, music streaming or spatial audio support.

The 40 added lockfile packages were reviewed for version, license, build script and
native boundary declarations. All have MIT/Apache/BSD permissive license options.
CoreAudio/coreaudio-sys use Apple's framework and bindgen/clang (macOS/iOS);
ALSA/alsa-sys require Linux libasound2-dev; windows 0.54 uses WASAPI; Android's
oboe/oboe-sys/JNI/NDK include external native C++ audio code. These remain above
the core boundary. Bindgen, clang-sys, coreaudio-sys, CPAL, ALSA and oboe build
scripts and Windows target libraries are recorded in reviewed-dependencies.json.
No authored unsafe/FFI/build.rs or native source was introduced. This declaration
review is not a full dependency audit or proof of device support on every platform.

## Android entry and mobile surface review

Owner approval on 2026-10-10 permits exactly the exported `android_main` attribute in
`tools/tomo-android/src/lib.rs`. The crate uses deny(unsafe_code), with a local allowance
for unsafe(no_mangle); no authored unsafe block, raw-pointer code, C ABI or physics FFI.
Winit 0.30.13's existing android-activity 0.6 NativeActivity feature supplies the OS bridge.
The Android cdylib reads bounded APK assets and uses app-private saves; no network permission.
The NDK toolchain/linker is packaging infrastructure; its libc++ shared library/NOTICE is
bundled for CPAL/Oboe. No external Cargo versions were changed by this feature.

iOS uses the full outer window size for both Metal and UIKit touch coordinates: winit
0.30 inner_size reports only the safe area. Mobile window sizing is OS-owned.
Surface resize/loss/recreation and controls share the same coordinate system.
This is platform integration, not mobile physical-device certification.
