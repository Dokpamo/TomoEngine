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
    "tomo-render": {"serde", "serde_json", "tomo-scene", "tomo-runtime", "wgpu", "pollster", "png"},
    "tomo-cli": {"serde_json", "tomo-scene", "tomo-runtime", "tomo-render"},
    "tomo-play": {"serde", "serde_json", "tomo-scene", "tomo-runtime", "tomo-render", "winit"},
}
REVIEWED = {
    "serde", "serde_core", "serde_derive", "serde_json", "itoa", "memchr", "ryu",
    "proc-macro2", "quote", "syn", "unicode-ident",
}
NATIVE_SUFFIXES = {".c", ".cc", ".cpp", ".cxx", ".h", ".hpp", ".a", ".lib", ".so", ".dll", ".dylib"}


def main():
    data = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--all-features", "--locked", "--offline"],
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
        for target in package["targets"]:
            if any(kind in ("lib", "bin") for kind in target["kind"]):
                if "#![forbid(unsafe_code)]" not in pathlib.Path(target["src_path"]).read_text(encoding="utf-8"):
                    raise ValueError(f"{name}: missing unsafe policy")
        for path in directory.rglob("*"):
            if path.is_file() and (path.name == "build.rs" or path.suffix in NATIVE_SUFFIXES):
                raise ValueError(f"unreviewed native/build source: {path}")
    external = [p for p in data["packages"] if p["id"] not in members]
    approved = json.loads((ROOT / "scripts/reviewed-dependencies.json").read_text())
    observed = sorted([
        {"name": p["name"], "version": p["version"], "source": p["source"],
         "license": p.get("license"), "links": p.get("links"),
         "build_script": any(t["kind"] == ["custom-build"] for t in p["targets"])}
        for p in external
    ], key=lambda p: (p["name"], p["version"]))
    if observed != approved:
        raise ValueError("dependency manifest changed: review versions, licenses and native/build boundaries")
    for package in external:
        if not package.get("source", "").startswith("registry+"):
            raise ValueError(f"unexpected dependency source: {package['name']}")
    # GPU/OS dependencies are allowed only above the scene/runtime/physics boundary.
    nodes = {node["id"]: node for node in data["resolve"]["nodes"]}
    packages = {package["id"]: package for package in data["packages"]}
    visited = set()
    pending = [local[name]["id"] for name in ("tomo-physics", "tomo-scene", "tomo-runtime")]
    while pending:
        identity = pending.pop()
        if identity in visited:
            continue
        visited.add(identity)
        package = packages[identity]
        if identity not in members and (package["name"] not in REVIEWED or package.get("links")):
            raise ValueError(f"GPU/native dependency leaked into core: {package['name']}")
        pending.extend(nodes[identity]["dependencies"])
    for path in sorted((ROOT / "schemas").glob("*.json")):
        json.loads(path.read_text(encoding="utf-8"))
    print(f"PASS: {len(local)} crate boundaries; zero physics dependencies; {len(external)} pinned external packages; GPU isolated from core")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"POLICY FAILURE: {error}", file=sys.stderr)
        sys.exit(1)
