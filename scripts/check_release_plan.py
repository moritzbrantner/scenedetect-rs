#!/usr/bin/env python3
"""Validate the exact scenedetect-core 0.1.0 release contract."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
import tomllib
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
RELEASE_PATH = Path("releases/scenedetect-core-0.1.0.toml")
REPOSITORY = "moritzbrantner/scenedetect-rs"
ISSUE = 72
REGISTRY = "crates.io"
PACKAGE = "scenedetect-core"
VERSION = "0.1.0"
PACKAGE_MANIFEST = "crates/scenedetect-core/Cargo.toml"
TAG = "scenedetect-core-v0.1.0"
REQUIRED_CHECKS = [
    "cargo metadata --format-version 1 --no-deps --locked",
    "python3 scripts/check_release_plan.py --check releases/scenedetect-core-0.1.0.toml",
    "cargo package -p scenedetect-core --locked --registry crates-io",
]
ROOT_FIELDS = {
    "schema_version",
    "repository",
    "issue",
    "source_sha",
    "registry",
    "dependency_order",
    "expected_tags",
    "required_checks",
    "required_consumer_checks",
    "packages",
    "github_releases",
}
PACKAGE_FIELDS = {
    "name",
    "version",
    "owner",
    "manifest_path",
    "dependencies",
    "tag",
    "published_checksum",
}
RELEASE_FIELDS = {"tag", "title", "notes"}


class ReleaseError(RuntimeError):
    """The checked release contract is not exact."""


def run(*args: str, allow_failure: bool = False) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        list(args),
        cwd=ROOT,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if completed.returncode and not allow_failure:
        detail = completed.stderr.strip()
        raise ReleaseError(f"command failed ({' '.join(args)}): {detail}")
    return completed


def manifest_digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_manifest(path: Path) -> dict[str, Any]:
    try:
        value = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        raise ReleaseError(f"cannot read release manifest: {error}") from error
    if not isinstance(value, dict):
        raise ReleaseError("release manifest must be a TOML table")
    return value


def cargo_metadata() -> dict[str, Any]:
    output = run(
        "cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"
    ).stdout
    try:
        return json.loads(output)
    except json.JSONDecodeError as error:
        raise ReleaseError("Cargo metadata was not valid JSON") from error


def registry_version(name: str, version: str) -> dict[str, Any] | None:
    encoded_name = urllib.parse.quote(name, safe="")
    encoded_version = urllib.parse.quote(version, safe="")
    request = urllib.request.Request(
        f"https://crates.io/api/v1/crates/{encoded_name}/{encoded_version}",
        headers={"User-Agent": "scenedetect-rs-release-control/1"},
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            payload = json.load(response)
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise ReleaseError(f"crates.io query failed with HTTP {error.code}") from error
    except (OSError, ValueError) as error:
        raise ReleaseError(f"crates.io query failed: {error}") from error
    record = payload.get("version")
    if not isinstance(record, dict):
        raise ReleaseError("crates.io returned an invalid version record")
    return record


def local_tag_target(tag: str) -> str | None:
    completed = run(
        "git", "rev-parse", "--verify", f"refs/tags/{tag}^{{}}", allow_failure=True
    )
    return completed.stdout.strip() if completed.returncode == 0 else None


def remote_tag_target(tag: str) -> str | None:
    completed = run(
        "git",
        "ls-remote",
        "--tags",
        "origin",
        f"refs/tags/{tag}",
        f"refs/tags/{tag}^{{}}",
    )
    rows = [line.split() for line in completed.stdout.splitlines() if line.strip()]
    peeled = [sha for sha, ref in rows if ref.endswith("^{}")]
    direct = [sha for sha, ref in rows if not ref.endswith("^{}")]
    return (peeled or direct or [None])[0]


def require_exact_fields(value: dict[str, Any], allowed: set[str], subject: str) -> None:
    unknown = sorted(set(value) - allowed)
    if unknown:
        raise ReleaseError(f"{subject} has unknown fields: {', '.join(unknown)}")


def validate_manifest(path: Path, *, check_remote_state: bool = True) -> dict[str, Any]:
    path = path.resolve()
    expected_path = (ROOT / RELEASE_PATH).resolve()
    if path != expected_path:
        raise ReleaseError(f"only {RELEASE_PATH.as_posix()} may authorize this release")
    manifest = load_manifest(path)
    require_exact_fields(manifest, ROOT_FIELDS, "release manifest")

    expected_root = {
        "schema_version": 1,
        "repository": REPOSITORY,
        "issue": ISSUE,
        "registry": REGISTRY,
        "dependency_order": [PACKAGE],
        "expected_tags": [TAG],
        "required_checks": REQUIRED_CHECKS,
        "required_consumer_checks": [],
    }
    for field, expected in expected_root.items():
        if manifest.get(field) != expected:
            raise ReleaseError(f"{field} must be exactly {expected!r}")

    source = manifest.get("source_sha")
    if not isinstance(source, str) or re.fullmatch(r"[0-9a-f]{40}", source) is None:
        raise ReleaseError("source_sha must be a full lowercase commit SHA")
    if run("git", "cat-file", "-e", f"{source}^{{commit}}", allow_failure=True).returncode:
        raise ReleaseError("source_sha is not a local commit")
    head = run("git", "rev-parse", "HEAD").stdout.strip()
    parent = run("git", "rev-parse", "HEAD^").stdout.strip()
    if parent != source:
        raise ReleaseError("control commit must have source_sha as its direct parent")
    changed = sorted(
        line for line in run("git", "diff", "--name-only", source, head).stdout.splitlines() if line
    )
    if changed != [RELEASE_PATH.as_posix()]:
        raise ReleaseError("only the release manifest may differ from source_sha")

    packages = manifest.get("packages")
    if not isinstance(packages, list) or len(packages) != 1 or not isinstance(packages[0], dict):
        raise ReleaseError("packages must contain exactly one package table")
    package = packages[0]
    require_exact_fields(package, PACKAGE_FIELDS, "package")
    expected_package = {
        "name": PACKAGE,
        "version": VERSION,
        "owner": REPOSITORY,
        "manifest_path": PACKAGE_MANIFEST,
        "dependencies": [],
        "tag": TAG,
    }
    for field, expected in expected_package.items():
        if package.get(field) != expected:
            raise ReleaseError(f"package {field} must be exactly {expected!r}")
    pinned_checksum = package.get("published_checksum")
    if pinned_checksum is not None and (
        not isinstance(pinned_checksum, str)
        or re.fullmatch(r"[0-9a-f]{64}", pinned_checksum) is None
    ):
        raise ReleaseError("published_checksum must be a lowercase SHA-256")

    releases = manifest.get("github_releases")
    if not isinstance(releases, list) or len(releases) != 1 or not isinstance(releases[0], dict):
        raise ReleaseError("github_releases must contain exactly one release table")
    release = releases[0]
    require_exact_fields(release, RELEASE_FIELDS, "GitHub release")
    if release.get("tag") != TAG:
        raise ReleaseError("GitHub release tag must match the package tag")
    if not all(isinstance(release.get(field), str) and release[field].strip() for field in RELEASE_FIELDS):
        raise ReleaseError("GitHub release tag, title, and notes must be non-empty strings")

    metadata = cargo_metadata()
    workspace_packages = {item["name"]: item for item in metadata.get("packages", [])}
    selected = workspace_packages.get(PACKAGE)
    if selected is None:
        raise ReleaseError("scenedetect-core is absent from Cargo metadata")
    if selected.get("version") != VERSION:
        raise ReleaseError("scenedetect-core Cargo version must be 0.1.0")
    relative_manifest = Path(selected["manifest_path"]).resolve().relative_to(ROOT).as_posix()
    if relative_manifest != PACKAGE_MANIFEST:
        raise ReleaseError("scenedetect-core manifest path is not exact")
    publish = selected.get("publish")
    if publish is not None and REGISTRY not in publish:
        raise ReleaseError("scenedetect-core is not publishable to crates.io")
    internal = sorted(
        dependency["name"]
        for dependency in selected.get("dependencies", [])
        if dependency.get("name") in workspace_packages
    )
    if internal:
        raise ReleaseError("scenedetect-core must not have workspace dependencies")
    non_registry = sorted(
        dependency["name"]
        for dependency in selected.get("dependencies", [])
        if not str(dependency.get("source") or "").startswith("registry+")
    )
    if non_registry:
        raise ReleaseError(
            "scenedetect-core dependencies must all come from a registry: "
            + ", ".join(non_registry)
        )

    if check_remote_state:
        record = registry_version(PACKAGE, VERSION)
        if record is None:
            print(f"REGISTRY ABSENT {PACKAGE} {VERSION}")
        else:
            checksum = record.get("checksum")
            if record.get("yanked") is True:
                raise ReleaseError("the crates.io version exists but is yanked")
            if not isinstance(checksum, str) or re.fullmatch(r"[0-9a-f]{64}", checksum) is None:
                raise ReleaseError("the crates.io version has an invalid checksum")
            if pinned_checksum is not None and checksum != pinned_checksum:
                raise ReleaseError("the crates.io checksum differs from published_checksum")
            print(f"REGISTRY PRESENT NON-YANKED {PACKAGE} {VERSION} {checksum}")
        for location, target in (
            ("local", local_tag_target(TAG)),
            ("remote", remote_tag_target(TAG)),
        ):
            if target is None:
                print(f"TAG ABSENT {location} {TAG}")
            elif target != source:
                raise ReleaseError(f"{location} tag {TAG} resolves to {target}, not source_sha")
            else:
                print(f"TAG EXACT {location} {TAG} {target}")
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", type=Path, required=True)
    args = parser.parse_args()
    try:
        manifest = validate_manifest(args.check)
    except (ReleaseError, ValueError) as error:
        print(f"release plan invalid: {error}", file=sys.stderr)
        return 1
    print(
        f"release plan valid: {PACKAGE} {VERSION}; "
        f"manifest sha256 {manifest_digest(args.check.resolve())}; "
        f"source {manifest['source_sha']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
