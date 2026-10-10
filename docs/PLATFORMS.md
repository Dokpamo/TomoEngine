# 플랫폼 패키징과 검증

구현 상태와 실제 장치 확인을 구분한다. 의존 라이브러리가 OS를 지원한다고 엔진의 기기 검증이
끝난 것은 아니다. 이 문서의 로컬 실행 기록은 2026-10-10 Apple M4/macOS에서 생성했다.
현재 Git head의 CI 결과는 GitHub Actions에서 별도로 확인한다.

## 프로젝트 패키징

최초 `cargo fetch --locked` 후 실행한다. 저장된 프로젝트를 전체 검증하고 v3로 정규화하여
바이너리와 함께 복사한다. 출력 폴더는 새 경로만 허용한다. 네트워크·배포·스토어 업로드는 하지 않는다.

```sh
# 현재 호스트의 macOS .app 또는 Windows/Linux 폴더
python3 scripts/package-game.py --project examples/projects/creator_lab --output target/CreatorLab.app
# 에디터 자체의 macOS 앱
python3 scripts/package-macos.py --editor

# Apple SDK/Xcode가 설치된 macOS
rustup target add aarch64-apple-ios-sim aarch64-apple-ios --toolchain 1.85.0
python3 scripts/package-game.py --project examples/projects/creator_lab --platform ios-simulator --output target/CreatorLab-iOS.app --debug
python3 scripts/package-game.py --project examples/projects/creator_lab --platform ios-device --output target/CreatorLab-device.app

# Android SDK build-tools/platforms, JDK와 NDK 필요
rustup target add aarch64-linux-android --toolchain 1.85.0
python3 scripts/package-game.py --project examples/projects/creator_lab --platform android --sdk ANDROID_SDK_PATH --ndk ANDROID_NDK_PATH --output target/CreatorLab-Android --debug
```

Android는 arm64-v8a NativeActivity, 최소 API 28, Vulkan backend다. APK 안의 project asset을
읽으며 저장은 앱 private data에 기록한다. Java 앱 코드/Gradle/네트워크 권한은 없다.
CPAL/Oboe가 사용하는 NDK libc++_shared와 NOTICE를 포함하고 16 KiB ELF/page 정렬을 적용한다.
`--debug`는 새 임시 테스트 인증서로 서명하고 그 키는 버린다. 동일 앱 ID의 이전 설치와 서명이
다를 수 있으므로 지속 배포/업그레이드용 서명이 아니다. release APK는 unsigned다.
Android 시작점의 export 속성 한 줄만 소유자 승인 예외이며 나머지 authored Rust는 unsafe 금지다.

현재 bundle ID는 `dev.tomo.creatorlab`이다. 프로젝트별 배포 ID·출시용 키 관리·앱 아이콘·스토어
메타데이터·공증/프로비저닝은 출시 준비 단계다. iOS device .app은 별도 개발자 서명·프로비저닝
없이는 설치되지 않는다. Windows/Linux 패키지는 해당 OS에서 빌드한다. 설치 프로그램은 아니다.

build-manifest.json에는 target, 고정 Rust 버전, Git SHA/dirty 여부, lockfile SHA-256,
패키지 파일별 SHA-256, 서명/게시 상태가 들어간다. 자체 manifest 파일의 재귀 hash는 포함하지 않는다.
외부 Cargo/SDK/toolchain/드라이버를 전부 재현하는 hermetic build 증명은 아니다.

## 확인한 범위

| 대상 | 구현/검증 증거 | 미확인 |
| --- | --- | --- |
| macOS Apple M4 | Metal 픽셀 검사, 게임/에디터 창·resize·입력, 실제 CPAL callback, 프로젝트/진행 저장 자동 검사 | 공증·설치 프로그램·여러 GPU 실기기 |
| Linux | 공통 Rust 호스트, native 폴더 패키저, CI의 software Vulkan + Xvfb 테스트 | 실제 데스크톱 GPU/오디오/장시간 성능 |
| Windows | 공통 winit/DX12/CPAL 호스트, native 폴더 패키저, CI Rust 테스트 | DX12/오디오 실기기와 설치 프로그램 |
| iOS | aarch64 simulator .app 빌드/설치, iPhone 17 Pro iOS 26.5 simulator의 Metal 화면·터치 이동·Pause·Save·Restart·Load·Next, Home/복귀 후 동일 프로세스 장면 유지 | 서명된 실제 iPhone/iPad, 발열/배터리·장시간 성능 |
| Android | Rust 1.85 + NDK 28.2 arm64 cdylib, APK 생성·16 KiB 정렬·테스트 서명 검증 | 로컬 emulator가 7.2 GB 사용자 디스크 요구/공간 부족으로 boot 불가. Vulkan·입력·오디오·복귀 실기기 실행 미검증 |
| 콘솔 | SDK 경계 설계만 존재 | 개발자 자격·허가된 SDK/기기 필요, 공개 구현 없음 |

모바일 게임은 이동/Action 터치 패드와 프로젝트 HUD 버튼을 표시한다. iOS의 winit inner_size는
safe area이므로 Metal과 터치는 outer_size를 함께 사용한다. 중단 시 입력/오디오/surface를 비우고
복귀 시 다시 구성한다. 프로세스 종료 후 자동 복원은 하지 않으며 Load로 명시적 슬롯을 연다.
게임 HUD는 현재 네이티브 접근성 요소를 생성하지 않는다.

## 재검증 명령

```sh
cargo +1.85.0 fmt --all -- --check
cargo +1.85.0 test --workspace --locked --offline
cargo +1.85.0 test --workspace --all-features --locked --offline
cargo +1.85.0 test --workspace --release --locked --offline
cargo +1.85.0 test --workspace --release --all-features --locked --offline
cargo +1.85.0 clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo +1.85.0 doc --workspace --no-deps --all-features --locked --offline
cargo +stable test --workspace --locked --offline
python3 scripts/check-policy.py
python3 scripts/test-code-edit.py
cargo test -p tomo-render --test capture --test content --locked --offline -- --include-ignored
cargo test -p tomo-cli --features render --test cli --locked --offline -- --include-ignored
cargo test -p tomo-play --locked --offline -- --include-ignored
cargo test -p tomo-render --features audio --locked --offline audio::tests::real_output_stream_processes_frames -- --ignored
cargo run -p tomo-play --locked --offline -- --smoke-test
cargo run -p tomo-editor --locked --offline -- --smoke-test
cargo run -p tomo-sdk-demo --locked --offline -- --headless
```

Linux CPAL 빌드에는 `libasound2-dev`가 필요하다. 실제 오디오 장치 검사는 GUI와 별도의 명시적
ignored test이며 GPU만 있는 CI에서 오디오 장치 성공으로 바꾸지 않는다.
GPU 검사·플랫폼 실패를 조용히 skip/success로 처리하지 않는다. Jolt 원본 동등성은 별도다.
