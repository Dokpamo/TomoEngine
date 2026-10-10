#!/usr/bin/env python3
"""Build a local, unsigned Box Run .app. No installer, signing keys or network calls."""
import argparse
import json
import pathlib
import plistlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--debug", action="store_true")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("local macOS packaging requires macOS")
    command = ["cargo", "build", "-p", "tomo-play", "--locked", "--offline"]
    if not args.debug:
        command.append("--release")
    subprocess.run(command, cwd=ROOT, check=True)
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked", "--offline"],
        cwd=ROOT, text=True,
    ))
    target = pathlib.Path(metadata["target_directory"])
    bundle = target / "Box Run.app" / "Contents"
    (bundle / "MacOS").mkdir(parents=True, exist_ok=True)
    (bundle / "Resources").mkdir(exist_ok=True)
    shutil.copy2(target / ("debug" if args.debug else "release") / "tomo-play", bundle / "MacOS" / "tomo-play")
    shutil.copy2(ROOT / "LICENSE", bundle / "Resources" / "LICENSE")
    with (bundle / "Info.plist").open("wb") as file:
        plistlib.dump({
            "CFBundleExecutable": "tomo-play",
            "CFBundleIdentifier": "com.dokpamo.tomoengine.boxrun",
            "CFBundleName": "Box Run",
            "CFBundleDisplayName": "Box Run",
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
