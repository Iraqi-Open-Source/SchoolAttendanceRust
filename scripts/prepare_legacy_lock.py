#!/usr/bin/env python3
"""Make a fresh Cargo.lock usable by Rust/Cargo 1.77.2.

Stable Cargo honors rust-version=1.77.2, but a crate can still be selected when
it publishes edition = "2024" and leaves rust-version unset. Cargo 1.77 cannot
parse those manifests. This script downgrades each such package to the newest
older release Cargo 1.77 can parse, then rewrites the lockfile header to v3.
"""
from pathlib import Path
import json
import re
import subprocess
import urllib.request

ROOT = Path(__file__).resolve().parent.parent / "src-tauri"
LOCK = ROOT / "Cargo.lock"
MSRV = (1, 77, 2)
OK_EDITIONS = {"2015", "2018", "2021"}


def run(args, quiet_failure=False):
    result = subprocess.run(args, cwd=ROOT, text=True, capture_output=True)
    if result.returncode == 0 and result.stdout:
        print(result.stdout, end="" if result.stdout.endswith("\n") else "\n")
    elif result.returncode != 0 and not quiet_failure and result.stderr:
        print(result.stderr, end="" if result.stderr.endswith("\n") else "\n")
    return result


def locked_packages(text):
    packages = []
    for block in text.split("[[package]]")[1:]:
        name = re.search(r'(?m)^name = "([^"]+)"', block)
        version = re.search(r'(?m)^version = "([^"]+)"', block)
        if name and version:
            packages.append((name.group(1), version.group(1)))
    return packages


def registry_manifests(name, version):
    folder = f"{name}-{version}"
    root = Path.home() / ".cargo" / "registry" / "src"
    if not root.is_dir():
        return []
    return [path / "Cargo.toml" for path in root.glob(f"*/{folder}") if (path / "Cargo.toml").is_file()]


def edition_of(name, version):
    for manifest in registry_manifests(name, version):
        match = re.search(r'(?m)^edition = "([^"]+)"', manifest.read_text(encoding="utf-8", errors="replace"))
        if match:
            return match.group(1)
    return None


def semver_key(version):
    numbers = []
    for part in version.split("+", 1)[0].split("."):
        digits = []
        for char in part:
            if char.isdigit():
                digits.append(char)
            else:
                break
        numbers.append(int("".join(digits) or "0"))
    return tuple(numbers)


def rust_version_key(value):
    if not value:
        return None
    return semver_key(value)


def crates_io_versions(name):
    request = urllib.request.Request(
        f"https://crates.io/api/v1/crates/{name}",
        headers={"User-Agent": "school-attendance-lockfile (github.com/Iraqi-Open-Source/SchoolAttendanceRust)"},
    )
    with urllib.request.urlopen(request, timeout=60) as response:
        payload = json.load(response)
    return payload["versions"]


def newest_parseable(name, current):
    current_key = semver_key(current)
    chosen = None
    for version in crates_io_versions(name):
        if version.get("yanked"):
            continue
        edition = version.get("edition")
        if edition not in OK_EDITIONS and edition is not None:
            continue
        rust_version = rust_version_key(version.get("rust_version"))
        if rust_version is not None and rust_version > MSRV:
            continue
        number = version["num"]
        if semver_key(number) > current_key:
            continue
        if chosen is None or semver_key(number) > semver_key(chosen):
            chosen = number
    return chosen


def unreadable_packages():
    found = []
    for name, version in locked_packages(LOCK.read_text(encoding="utf-8")):
        edition = edition_of(name, version)
        if edition and edition not in OK_EDITIONS:
            found.append((name, version, edition))
    return found


def rewrite_v3():
    raw = LOCK.read_text(encoding="utf-8")
    changed, count = re.subn(r"(?m)^version = 4$", "version = 3", raw, count=1)
    if not count and not re.search(r"(?m)^version = 3$", raw):
        raise SystemExit("Unexpected Cargo.lock version, did not modify")
    if count:
        LOCK.write_text(changed, encoding="utf-8")
    print("Cargo.lock is in v3 format (Cargo 1.77 compatible)")


def main():
    if not LOCK.is_file():
        raise SystemExit(f"Cargo.lock missing: {LOCK}")
    for _ in range(8):
        fetch = run(["cargo", "+stable", "fetch", "--locked"])
        if fetch.returncode != 0:
            raise SystemExit(fetch.returncode)
        bad = unreadable_packages()
        if not bad:
            break
        progressed = False
        for name, version, edition in bad:
            precise = newest_parseable(name, version)
            if not precise or precise == version:
                print(f"warning: {name} {version} uses edition {edition} and has no older Cargo 1.77 build")
                continue
            print(f"downgrade {name} {version} (edition {edition}) -> {precise}")
            updated = run(["cargo", "+stable", "update", "-p", name, "--precise", precise], quiet_failure=True)
            if updated.returncode == 0:
                progressed = True
            else:
                print(f"warning: left {name} {version}; a dependency requires a newer release")
        if not progressed:
            break
    rewrite_v3()


if __name__ == "__main__":
    main()
