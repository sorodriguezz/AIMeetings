#!/usr/bin/env python3
"""Set the app version everywhere it is declared.

Usage: python3 scripts/bump_version.py 0.2.0
Then: git commit -am "Release v0.2.0" && git tag v0.2.0 && git push --follow-tags
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main() -> None:
    if len(sys.argv) != 2 or not re.fullmatch(r"\d+\.\d+\.\d+", sys.argv[1]):
        raise SystemExit("usage: bump_version.py X.Y.Z")
    v = sys.argv[1]

    # tauri.conf.json reads its version from package.json.
    pkg = ROOT / "package.json"
    data = json.loads(pkg.read_text())
    data["version"] = v
    pkg.write_text(json.dumps(data, indent=2) + "\n")

    cargo = ROOT / "src-tauri/Cargo.toml"
    cargo.write_text(re.sub(r'(?m)^version = "[^"]+"', f'version = "{v}"', cargo.read_text(), count=1))

    print(f"version set to {v}")
    print(f'next: git commit -am "Release v{v}" && git tag v{v} && git push --follow-tags')


if __name__ == "__main__":
    main()
