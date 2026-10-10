#!/usr/bin/env python3
"""Verify workspace boundaries and the reviewed dependency allowlist, not all safety properties."""
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
EXPECTED = {
    "tomo-physics": set(),
    "tomo-scene": {"serde", "serde_json"},
    "tomo-runtime": {"serde", "tomo-scene", "tomo-physics"},
    "tomo-cli": {"serde_json", "tomo-scene", "tomo-runtime"},
}
REVIEWED = {
    "serde", "serde_core", "serde_derive", "serde_json", "itoa", "memchr", "ryu",
    "proc-macro2", "quote", "syn", "unicode-ident",
}
NATIVE_SUFFIXES = {".c", ".cc", ".cpp", ".cxx", ".h", ".hpp", ".a", ".lib", ".so", ".dll", ".dylib"}


def main():
    data = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--offline"],
        cwd=ROOT, text=True,
    ))
    members = set(data["workspace_members"])
    local = {p["name"]: p for p in data["packages"] if p["id"] in members}
    if set(local) != set(EXPECTED):
        raise ValueError("workspace members changed: review architectural boundaries")
    for name, expected in EXPECTED.items():
        package = local[name]
        actual = {dependency["name"] for dependency in package["dependencies"]}
        if actual != expected:
            raise ValueError(f"{name}: unreviewed dependency boundary: {actual}")
        directory = pathlib.Path(package["manifest_path"]).parent
        entry = directory / "src" / ("main.rs" if name == "tomo-cli" else "lib.rs")
        if "#![forbid(unsafe_code)]" not in entry.read_text(encoding="utf-8"):
            raise ValueError(f"{name}: missing unsafe policy")
        for path in directory.rglob("*"):
            if path.is_file() and (path.name == "build.rs" or path.suffix in NATIVE_SUFFIXES):
                raise ValueError(f"unreviewed native/build source: {path}")
    external = [p for p in data["packages"] if p["id"] not in members]
    for package in external:
        if package["name"] not in REVIEWED or package.get("links"):
            raise ValueError(f"unreviewed dependency/native links: {package['name']}")
        if not package.get("source", "").startswith("registry+"):
            raise ValueError(f"unexpected dependency source: {package['name']}")
        print(f"reviewed dependency: {package['name']} {package['version']} ({package.get('license')})")
    for path in sorted((ROOT / "schemas").glob("*.json")):
        json.loads(path.read_text(encoding="utf-8"))
    print("PASS: four crate boundaries; zero physics dependencies; reviewed serialization graph")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"POLICY FAILURE: {error}", file=sys.stderr)
        sys.exit(1)
