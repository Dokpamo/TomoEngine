// SPDX-License-Identifier: MIT
#![deny(unsafe_code)]

// The owner-authorized exception exports the Android entry symbol only.
// No unsafe blocks or foreign ABI are used in authored code.
#[cfg(target_os = "android")]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub fn android_main(app: winit::platform::android::activity::AndroidApp) {
    tomo_play::desktop::run_android(app);
}
