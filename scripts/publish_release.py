#!/usr/bin/env python3
"""Publish the exact issue #72 release with fail-closed, idempotent effects."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

from check_release_plan import (
    ISSUE,
    PACKAGE,
    RELEASE_PATH,
    REPOSITORY,
    ROOT,
    TAG,
    VERSION,
    ReleaseError,
    local_tag_target,
    manifest_digest,
    registry_version,
    remote_tag_target,
    validate_manifest,
)


AUTHORIZATION_LABEL = "release:approved"


def run(
    *args: str, allow_failure: bool = False, capture: bool = True
) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        list(args),
        cwd=ROOT,
        check=False,
        text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None,
    )
    if completed.returncode and not allow_failure:
        detail = (completed.stderr or "").strip()
        raise ReleaseError(f"command failed ({' '.join(args)}): {detail}")
    return completed


def repository() -> str:
    remote = run("git", "config", "--get", "remote.origin.url").stdout.strip()
    if remote.startswith("git@github.com:"):
        remote = remote.removeprefix("git@github.com:")
    elif "github.com/" in remote:
        remote = remote.split("github.com/", 1)[1]
    return remote.removesuffix(".git").strip("/")


def exact_head() -> str:
    return run("git", "rev-parse", "HEAD").stdout.strip()


def clean() -> bool:
    return not run("git", "status", "--porcelain").stdout.strip()


def issue() -> dict[str, Any]:
    output = run(
        "gh",
        "issue",
        "view",
        str(ISSUE),
        "--repo",
        REPOSITORY,
        "--json",
        "number,state,labels,body,url",
    ).stdout
    try:
        value = json.loads(output)
    except json.JSONDecodeError as error:
        raise ReleaseError("GitHub issue response was not valid JSON") from error
    if not isinstance(value, dict):
        raise ReleaseError("GitHub issue response was not an object")
    return value


def validate_authority(head: str, digest: str) -> None:
    if repository() != REPOSITORY:
        raise ReleaseError("origin does not match the authorized repository")
    if exact_head() != head:
        raise ReleaseError("HEAD changed during publication")
    if not clean():
        raise ReleaseError("publication requires a clean worktree")
    record = issue()
    if record.get("number") != ISSUE or record.get("state") != "OPEN":
        raise ReleaseError("the destination release issue must remain open")
    labels = {
        label.get("name")
        for label in record.get("labels", [])
        if isinstance(label, dict)
    }
    if AUTHORIZATION_LABEL not in labels:
        raise ReleaseError(f"issue #{ISSUE} lacks {AUTHORIZATION_LABEL}")
    body = record.get("body") or ""
    head_line = f"Release control head: `{head}`"
    digest_line = f"Release manifest SHA-256: `{digest}`"
    if body.count(head_line) != 1 or body.count(digest_line) != 1:
        raise ReleaseError("issue body does not bind exactly one current head and manifest digest")
    recorded_heads = re.findall(r"Release control head: `([0-9a-f]{40})`", body)
    recorded_digests = re.findall(r"Release manifest SHA-256: `([0-9a-f]{64})`", body)
    if recorded_heads != [head] or recorded_digests != [digest]:
        raise ReleaseError("issue body contains stale or additional release authority")


def package_checksum() -> str:
    run(
        "cargo",
        "package",
        "-p",
        PACKAGE,
        "--locked",
        "--registry",
        "crates-io",
        capture=False,
    )
    configured = Path(os.environ.get("CARGO_TARGET_DIR", "target"))
    target = configured if configured.is_absolute() else ROOT / configured
    archive = target / "package" / f"{PACKAGE}-{VERSION}.crate"
    try:
        return hashlib.sha256(archive.read_bytes()).hexdigest()
    except OSError as error:
        raise ReleaseError(f"cannot checksum packaged archive {archive}: {error}") from error


def require_exact_registry(checksum: str, record: dict[str, Any] | None) -> bool:
    if record is None:
        return False
    if record.get("yanked") is True:
        raise ReleaseError("the existing crates.io version is yanked")
    if record.get("checksum") != checksum:
        raise ReleaseError("the existing crates.io artifact checksum differs from the candidate")
    print(f"SKIP exact non-yanked registry artifact {PACKAGE} {VERSION} {checksum}")
    return True


def wait_for_registry(checksum: str) -> None:
    for attempt in range(12):
        record = registry_version(PACKAGE, VERSION)
        if record is not None:
            require_exact_registry(checksum, record)
            return
        if attempt != 11:
            time.sleep(5)
    raise ReleaseError("published crate did not become visible with its exact checksum")


def ensure_tag(source: str, title: str) -> None:
    local = local_tag_target(TAG)
    remote = remote_tag_target(TAG)
    for location, target in (("local", local), ("remote", remote)):
        if target is not None and target != source:
            raise ReleaseError(f"{location} tag {TAG} resolves to {target}, not source_sha")
    if remote == source:
        print(f"SKIP exact remote tag {TAG} at {source}")
        return
    if local is None:
        run("git", "tag", "-a", TAG, source, "-m", title)
    run("git", "push", "origin", f"refs/tags/{TAG}", capture=False)
    if remote_tag_target(TAG) != source:
        raise ReleaseError("remote tag was not created at source_sha")


def ensure_github_release(title: str, notes: str) -> None:
    completed = run(
        "gh",
        "release",
        "view",
        TAG,
        "--repo",
        REPOSITORY,
        "--json",
        "tagName,isDraft,isPrerelease,name,body,url",
        allow_failure=True,
    )
    if completed.returncode == 0:
        try:
            record = json.loads(completed.stdout)
        except json.JSONDecodeError as error:
            raise ReleaseError("GitHub Release response was not valid JSON") from error
        if (
            record.get("tagName") != TAG
            or record.get("isDraft") is True
            or record.get("isPrerelease") is True
            or record.get("name") != title
            or record.get("body") != notes
        ):
            raise ReleaseError("existing GitHub Release does not match the final release contract")
        print(f"SKIP exact GitHub Release {record.get('url', TAG)}")
        return
    run(
        "gh",
        "release",
        "create",
        TAG,
        "--repo",
        REPOSITORY,
        "--verify-tag",
        "--title",
        title,
        "--notes",
        notes,
        capture=False,
    )


def publish(*, safeguards_only: bool) -> None:
    manifest_path = ROOT / RELEASE_PATH
    manifest = validate_manifest(manifest_path)
    head = exact_head()
    digest = manifest_digest(manifest_path)
    source = manifest["source_sha"]
    package = manifest["packages"][0]
    release = manifest["github_releases"][0]
    if safeguards_only:
        print(f"SAFEGUARDS VALID head={head} manifest_sha256={digest} source={source}")
        return

    validate_authority(head, digest)
    checksum = package_checksum()
    pinned = package.get("published_checksum")
    if pinned is not None and pinned != checksum:
        raise ReleaseError("packaged archive checksum differs from published_checksum")
    existing = registry_version(PACKAGE, VERSION)
    if not require_exact_registry(checksum, existing):
        # Re-check every mutable authority immediately before the upload.
        validate_authority(head, digest)
        if manifest_digest(manifest_path) != digest:
            raise ReleaseError("release manifest changed during publication")
        run(
            "cargo",
            "publish",
            "-p",
            PACKAGE,
            "--locked",
            "--registry",
            "crates-io",
            capture=False,
        )
        wait_for_registry(checksum)

    # Re-check authority before the independently irreversible tag/release effects.
    validate_authority(head, digest)
    ensure_tag(source, release["title"])
    if remote_tag_target(TAG) != source:
        raise ReleaseError("remote tag does not resolve to source_sha")
    ensure_github_release(release["title"], release["notes"])
    print(f"RELEASE COMPLETE {PACKAGE} {VERSION} checksum={checksum} source={source}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--check-safeguards",
        action="store_true",
        help="validate the local structural contract without requiring approval or effects",
    )
    args = parser.parse_args()
    try:
        publish(safeguards_only=args.check_safeguards)
    except (ReleaseError, ValueError) as error:
        print(f"release blocked: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
