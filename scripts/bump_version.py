#!/usr/bin/env python3
"""Set the taskbar-stats package version in Cargo.toml and Cargo.lock."""

from __future__ import annotations

import argparse
import re
from pathlib import Path

PACKAGE = "taskbar-stats"
SEMVER = re.compile(r"^\d+\.\d+\.\d+$")
TOML_VERSION = re.compile(r'(?m)^version\s*=\s*"([^"]+)"')
LOCK_VERSION = re.compile(
    r'(?m)^(name = "' + re.escape(PACKAGE) + r'"\r?\nversion = ")[^"]+(")'
)


def read(path: Path) -> str:
    return path.read_bytes().decode("utf-8")


def write(path: Path, text: str) -> None:
    path.write_bytes(text.encode("utf-8"))


def package_version(text: str) -> str:
    section = text.find("[package]")
    if section < 0:
        raise SystemExit("Cargo.toml has no [package] section.")
    match = TOML_VERSION.search(text, section)
    if not match:
        raise SystemExit("Cargo.toml has no package version.")
    return match.group(1)


def bumped(version: str, kind: str) -> str:
    major, minor, patch = (int(part) for part in version.split("."))
    if kind == "major":
        return f"{major + 1}.0.0"
    if kind == "minor":
        return f"{major}.{minor + 1}.0"
    return f"{major}.{minor}.{patch + 1}"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "version", nargs="?", default="", help="exact version, for example 0.2.0"
    )
    parser.add_argument(
        "--bump",
        choices=("patch", "minor", "major"),
        default="",
        help="bump the current version instead of passing one",
    )
    args = parser.parse_args()

    toml_path = Path("Cargo.toml")
    lock_path = Path("Cargo.lock")
    toml_text = read(toml_path)
    current = package_version(toml_text)

    if args.version:
        target = args.version.lstrip("vV")
        if not SEMVER.match(target):
            raise SystemExit(f"{target!r} is not MAJOR.MINOR.PATCH.")
    elif args.bump:
        target = bumped(current, args.bump)
    else:
        raise SystemExit("Pass a version or --bump patch|minor|major.")

    if tuple(map(int, target.split("."))) < tuple(map(int, current.split("."))):
        raise SystemExit(f"Cannot move the version backwards: {current} -> {target}.")

    section = toml_text.find("[package]")
    match = TOML_VERSION.search(toml_text, section)
    if not match:
        raise SystemExit("Cargo.toml has no package version.")
    toml_text = toml_text[: match.start(1)] + target + toml_text[match.end(1) :]
    write(toml_path, toml_text)

    lock_text = read(lock_path)
    lock_text, count = LOCK_VERSION.subn(
        lambda match: match.group(1) + target + match.group(2), lock_text
    )
    if count != 1:
        raise SystemExit(
            f"Expected one {PACKAGE} entry in Cargo.lock, found {count}."
        )
    write(lock_path, lock_text)

    print(target)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
