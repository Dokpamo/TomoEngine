#!/usr/bin/env python3
"""Build a validated host package; no production signing keys or publishing."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parents[1]

def run(args):
    return subprocess.check_output(args, cwd=ROOT, text=True)

def versioned_directory(root, prefix, required):
    candidates = []
    for path in root.iterdir():
        match = re.fullmatch(re.escape(prefix) + r'(\d+(?:\.\d+)*)', path.name)
        if match and path.is_dir() and all((path / name).is_file() for name in required):
            candidates.append((tuple(int(part) for part in match[1].split('.')), path))
    if not candidates:
        raise ValueError(f'no complete released SDK component found in {root}')
    return max(candidates)[1]

def android(args, normalized, build, profile):
    sdk = Path(args.sdk or os.environ.get('ANDROID_HOME', os.environ.get('ANDROID_SDK_ROOT', ''))).resolve()
    ndk_value = args.ndk or os.environ.get('ANDROID_NDK_HOME')
    if not ndk_value or not (sdk / 'build-tools').is_dir():
        raise ValueError('Android requires --sdk/ANDROID_HOME and --ndk/ANDROID_NDK_HOME')
    ndk = Path(ndk_value).resolve()
    hosts = {'Darwin': 'darwin-x86_64', 'Linux': 'linux-x86_64', 'Windows': 'windows-x86_64'}
    llvm = ndk / 'toolchains/llvm/prebuilt' / hosts[platform.system()]
    suffix = '.cmd' if os.name == 'nt' else ''
    clang = llvm / 'bin' / ('aarch64-linux-android28-clang' + suffix)
    env = dict(os.environ)
    env.update(ANDROID_NDK_HOME=str(ndk), CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=str(clang),
               CC_aarch64_linux_android=str(clang),
               CXX_aarch64_linux_android=str(llvm / 'bin' / ('aarch64-linux-android28-clang++' + suffix)),
               AR_aarch64_linux_android=str(llvm / 'bin/llvm-ar'),
               CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS='-C link-arg=-Wl,-z,max-page-size=16384')
    subprocess.run(build + ['-p', 'tomo-android', '--target', 'aarch64-linux-android'], cwd=ROOT, env=env, check=True, stdout=sys.stderr)
    build_tools = versioned_directory(sdk / 'build-tools', '', ['aapt.exe' if os.name == 'nt' else 'aapt', 'zipalign.exe' if os.name == 'nt' else 'zipalign', 'apksigner.bat' if os.name == 'nt' else 'apksigner'])
    android_jar = versioned_directory(sdk / 'platforms', 'android-', ['android.jar']) / 'android.jar'
    output = args.output.resolve()
    output.mkdir()
    with tempfile.TemporaryDirectory(prefix='android-package-', dir=ROOT / 'target') as tmp:
        stage = Path(tmp)
        assets = stage / 'assets'
        (assets / 'game').mkdir(parents=True)
        (assets / 'game/project.tomo.json').write_text(json.dumps(normalized, indent=2) + '\n', encoding='utf-8')
        shutil.copy2(ROOT / 'LICENSE', assets / 'LICENSE')
        shutil.copytree(ROOT / 'LICENSES', assets / 'LICENSES')
        manifest = ET.Element('manifest', {'xmlns:android': 'http://schemas.android.com/apk/res/android', 'package': 'dev.tomo.creatorlab', 'android:versionCode': '1', 'android:versionName': '0.1.0'})
        ET.SubElement(manifest, 'uses-sdk', {'android:minSdkVersion': '28', 'android:targetSdkVersion': '35'})
        ET.SubElement(manifest, 'uses-feature', {'android:name': 'android.hardware.vulkan.level', 'android:version': '1', 'android:required': 'true'})
        app = ET.SubElement(manifest, 'application', {'android:label': normalized['name'], 'android:hasCode': 'false', 'android:debuggable': str(args.debug).lower(), 'android:extractNativeLibs': 'true', 'android:theme': '@android:style/Theme.Material.NoActionBar.Fullscreen'})
        activity = ET.SubElement(app, 'activity', {'android:name': 'android.app.NativeActivity', 'android:exported': 'true', 'android:screenOrientation': 'landscape', 'android:configChanges': 'orientation|keyboardHidden|screenSize'})
        ET.SubElement(activity, 'meta-data', {'android:name': 'android.app.lib_name', 'android:value': 'tomo_android'})
        intent = ET.SubElement(activity, 'intent-filter')
        ET.SubElement(intent, 'action', {'android:name': 'android.intent.action.MAIN'})
        ET.SubElement(intent, 'category', {'android:name': 'android.intent.category.LAUNCHER'})
        ET.ElementTree(manifest).write(stage / 'AndroidManifest.xml', encoding='utf-8', xml_declaration=True)
        unsigned = stage / 'unsigned.apk'
        subprocess.run([str(build_tools / ('aapt.exe' if os.name == 'nt' else 'aapt')), 'package', '-f', '-M', str(stage / 'AndroidManifest.xml'), '-I', str(android_jar), '-A', str(assets), '-F', str(unsigned)], check=True, stdout=sys.stderr)
        with zipfile.ZipFile(unsigned, 'a', compression=zipfile.ZIP_DEFLATED) as archive:
            archive.write(ROOT / 'target/aarch64-linux-android' / profile / 'libtomo_android.so', 'lib/arm64-v8a/libtomo_android.so')
            archive.write(llvm / 'sysroot/usr/lib/aarch64-linux-android/libc++_shared.so', 'lib/arm64-v8a/libc++_shared.so')
            archive.writestr('assets/NDK-NOTICE', (ndk / 'NOTICE').read_text())
        apk = output / 'TomoGame.apk'
        subprocess.run([str(build_tools / ('zipalign.exe' if os.name == 'nt' else 'zipalign')), '-P', '16', '-f', '4', str(unsigned), str(apk)], check=True, stdout=sys.stderr)
        if args.debug:
            key = stage / 'test-only.keystore'
            subprocess.run(['keytool', '-genkeypair', '-keystore', str(key), '-storepass', 'android', '-keypass', 'android', '-alias', 'androiddebugkey', '-dname', 'CN=Tomo Local Test', '-keyalg', 'RSA', '-validity', '30'], check=True, stdout=sys.stderr, stderr=sys.stderr)
            subprocess.run([str(build_tools / ('apksigner.bat' if os.name == 'nt' else 'apksigner')), 'sign', '--ks', str(key), '--ks-pass', 'pass:android', '--key-pass', 'pass:android', str(apk)], check=True, stdout=sys.stderr)
            subprocess.run([str(build_tools / ('apksigner.bat' if os.name == 'nt' else 'apksigner')), 'verify', str(apk)], check=True, stdout=sys.stderr)
    return output, output, 'aarch64-linux-android', 'ephemeral-debug' if args.debug else False

class JsonParser(argparse.ArgumentParser):
    def error(self, message):
        raise ValueError(message)


def main():
    parser = JsonParser()
    parser.add_argument('--project', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--platform', choices=['desktop', 'ios-simulator', 'ios-device', 'android'], default='desktop')
    parser.add_argument('--debug', action='store_true')
    parser.add_argument('--sdk')
    parser.add_argument('--ndk')
    args = parser.parse_args()
    project = args.project.resolve()
    profile = 'debug' if args.debug else 'release'
    build = ['cargo', '+1.85.0', 'build', '--locked', '--offline']
    if profile == 'release':
        build += ['--release']
    # Validate before creating the output and copy the normalized v3 document.
    subprocess.run(build + ['-p', 'tomo-cli'], cwd=ROOT, check=True, stdout=sys.stderr)
    suffix = '.exe' if os.name == 'nt' else ''
    cli = ROOT / 'target' / profile / ('tomo' + suffix)
    normalized = json.loads(run([str(cli), 'project', 'inspect', str(project)]))
    target = None
    if args.platform.startswith('ios'):
        if platform.system() != 'Darwin':
            raise ValueError('iOS packaging requires macOS and the Apple SDK')
        target = 'aarch64-apple-ios-sim' if args.platform == 'ios-simulator' else 'aarch64-apple-ios'
    signed = False
    if args.platform == 'android':
        output, resource, target, signed = android(args, normalized, build, profile)
    else:
        subprocess.run(build + ['-p', 'tomo-play'] + (['--target', target] if target else []), cwd=ROOT, check=True, stdout=sys.stderr)
        binary = ROOT / 'target' / (target or '') / profile / ('tomo-play' + suffix)
        output = args.output.resolve()
        output.mkdir()  # Never replace a preexisting package.
        if args.platform.startswith('ios'):
            executable = output / 'TomoGame'
            resource = output
            plist_path = output / 'Info.plist'
        elif platform.system() == 'Darwin':
            executable = output / 'Contents/MacOS/TomoGame'
            resource = output / 'Contents/Resources'
            plist_path = output / 'Contents/Info.plist'
        else:
            executable = output / ('TomoGame' + suffix)
            resource = output
            plist_path = None
        executable.parent.mkdir(parents=True, exist_ok=True)
        (resource / 'game').mkdir(parents=True)
        shutil.copy2(binary, executable)
        (resource / 'game/project.tomo.json').write_text(json.dumps(normalized, indent=2) + '\n', encoding='utf-8')
        shutil.copy2(ROOT / 'LICENSE', resource / 'LICENSE')
        shutil.copytree(ROOT / 'LICENSES', resource / 'LICENSES')
        if plist_path:
            info = dict(CFBundleExecutable='TomoGame', CFBundleIdentifier='dev.tomo.creatorlab', CFBundleName=normalized['name'], CFBundlePackageType='APPL', CFBundleShortVersionString='0.1.0', CFBundleVersion='1', NSHighResolutionCapable=True)
            if args.platform.startswith('ios'):
                info.update(MinimumOSVersion='16.0', UIDeviceFamily=[1, 2], UILaunchScreen={}, UISupportedInterfaceOrientations=['UIInterfaceOrientationLandscapeLeft', 'UIInterfaceOrientationLandscapeRight'], UIRequiresFullScreen=True)
            plist_path.write_bytes(plistlib.dumps(info))
    files = {str(p.relative_to(output)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(output.rglob('*')) if p.is_file()}
    manifest = dict(schema_version=1, project=normalized['name'], platform=args.platform, machine=platform.machine(), target=target or run(['rustc', '+1.85.0', '-vV']).split('host: ')[1].splitlines()[0], git_commit=run(['git', 'rev-parse', 'HEAD']).strip(), working_tree_dirty=bool(run(['git', 'status', '--porcelain']).strip()), rustc=run(['rustc', '+1.85.0', '-vV']), cargo_lock_sha256=hashlib.sha256((ROOT / 'Cargo.lock').read_bytes()).hexdigest(), files=files, signed=signed, published=False)
    (resource / 'build-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(dict(schema_version=1, ok=True, package=str(output), manifest=str(resource / 'build-manifest.json'))))

if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError, KeyError) as error:
        print(json.dumps(dict(schema_version=1, ok=False, error=str(error))))
        sys.exit(2)
