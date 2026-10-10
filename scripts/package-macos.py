#!/usr/bin/env python3
"""Build a local, unsigned Box Run or Tomo Editor .app. No installer, signing keys or network calls."""
import argparse
import json
import pathlib
import plistlib
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--debug", action="store_true")
    parser.add_argument("--editor", action="store_true")
    args = parser.parse_args()
    binary = "tomo-editor" if args.editor else "tomo-play"
    name = "Tomo Editor" if args.editor else "Box Run"
    if sys.platform != "darwin":
        parser.error("local macOS packaging requires macOS")
    command = ["cargo", "build", "-p", binary, "--locked", "--offline"]
    if not args.debug:
        command.append("--release")
    subprocess.run(command, cwd=ROOT, check=True)
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked", "--offline"],
        cwd=ROOT, text=True,
    ))
    target = pathlib.Path(metadata["target_directory"])
    bundle = target / f"{name}.app" / "Contents"
    (bundle / "MacOS").mkdir(parents=True, exist_ok=True)
    (bundle / "Resources").mkdir(exist_ok=True)
    # Replace the executable inode so rebuilding an open app does not alter its mapped code.
    with tempfile.NamedTemporaryFile(prefix=f".{binary}-", dir=bundle / "MacOS", delete=False) as file:
        temporary = pathlib.Path(file.name)
    try:
        shutil.copy2(target / ("debug" if args.debug else "release") / binary, temporary)
        temporary.replace(bundle / "MacOS" / binary)
    finally:
        temporary.unlink(missing_ok=True)
    shutil.copy2(ROOT / "LICENSE", bundle / "Resources" / "LICENSE")
    if args.editor:
        shutil.copytree(ROOT / "LICENSES/editor-fonts", bundle / "Resources/editor-fonts", dirs_exist_ok=True)
    with (bundle / "Info.plist").open("wb") as file:
        plistlib.dump({
            "CFBundleExecutable": binary,
            "CFBundleIdentifier": "com.dokpamo.tomoengine.editor" if args.editor else "com.dokpamo.tomoengine.boxrun",
            "CFBundleName": name,
            "CFBundleDisplayName": name,
            "CFBundlePackageType": "APPL",
            "CFBundleVersion": "1",
            "CFBundleShortVersionString": "0.1.0",
            "NSHighResolutionCapable": True,
            "LSMinimumSystemVersion": "11.0",
            "LSApplicationCategoryType": "public.app-category.games",
        }, file)
    print(json.dumps({"schema_version": 1, "app": str(bundle.parent), "signed": False}))


if __name__ == "__main__":
    main()
