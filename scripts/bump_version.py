#!/usr/bin/env python3
"""Synchronize app, engine and lockfile versions, or validate with --check.

Usage: python3 scripts/bump_version.py X.Y.Z
Then commit and push to main; GitHub Actions creates the tag and release.
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SEMVER = r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"


def declarations(root):
    """Yield (path, value, replacement pattern); None denotes a JSON file."""
    app = root / "app" if (root / "app/package.json").exists() else root
    package = app / "package.json"
    yield package, json.loads(package.read_text())["version"], None
    conf = app / "src-tauri/tauri.conf.json"
    value = json.loads(conf.read_text())["version"]
    # Tauri can read its version directly from a package.json path.
    if value.endswith(".json"):
        value = json.loads((conf.parent / value).read_text())["version"]
    yield conf, value, None
    for path in (app / "src-tauri/Cargo.toml", root / "engine/pyproject.toml"):
        if not path.exists():
            continue
        text = path.read_text()
        pattern = r'(?m)^(version\s*=\s*")[^"]+("\s*)$'
        value = re.search(pattern, text).group(0).split('"')[1]
        yield path, value, pattern
        lock = path.with_name("Cargo.lock" if path.name == "Cargo.toml" else "uv.lock")
        if lock.exists():
            name = re.search(r'(?m)^name\s*=\s*"([^"]+)"', text).group(1)
            pattern = r'(\[\[package\]\]\s*\nname = "' + re.escape(name) + r'"\s*\nversion = ")[^"]+(")'
            match = re.search(pattern, lock.read_text())
            if not match:
                raise ValueError(f"{lock}: missing package {name}")
            yield lock, match.group(0).split('version = "')[1].split('"')[0], pattern
    init = root / "engine/src/aics_engine/__init__.py"
    if init.exists():
        pattern = r'(?m)^(__version__ = ")[^"]+(")'
        yield init, re.search(pattern, init.read_text()).group(0).split('"')[1], pattern
    lock = app / "package-lock.json"
    if lock.exists():
        data = json.loads(lock.read_text())
        yield lock, data["version"], None
        yield lock, data["packages"][""]["version"], None


def check(root=ROOT):
    entries = list(declarations(root))
    version = entries[0][1]
    if not re.fullmatch(SEMVER, version):
        raise ValueError(f"Invalid stable version: {version}")
    mismatches = [f"{p.relative_to(root)}: {v}" for p, v, _ in entries if v != version]
    if mismatches:
        raise ValueError("Versions differ: " + "; ".join(mismatches) +
                         f". Run python3 scripts/bump_version.py {version}")
    return version


def set_version(version, root=ROOT):
    if not re.fullmatch(SEMVER, version):
        raise ValueError("usage: bump_version.py X.Y.Z | --check")
    # Read everything before writing, so malformed declarations fail early.
    for path, _, pattern in list(declarations(root)):
        if pattern:
            text = re.sub(pattern, lambda m: m.group(1) + version + m.group(2),
                          path.read_text(), count=1)
        else:
            data = json.loads(path.read_text())
            if not str(data.get("version", "")).endswith(".json"):
                data["version"] = version
            if path.name == "package-lock.json":
                data["packages"][""]["version"] = version
            text = json.dumps(data, indent=2, ensure_ascii=False) + "\n"
        path.write_text(text)
    check(root)


if __name__ == "__main__":
    try:
        if sys.argv[1:] == ["--check"]:
            print(check())
        elif len(sys.argv) == 2:
            set_version(sys.argv[1])
            print(f"Version set to {sys.argv[1]}. Commit and push to main to publish.")
        else:
            raise ValueError("usage: bump_version.py X.Y.Z | --check")
    except ValueError as error:
        sys.exit(str(error))
