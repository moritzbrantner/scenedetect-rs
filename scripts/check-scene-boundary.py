#!/usr/bin/env python3
"""Validate scenedetect-rs's canonical scene-capability ownership boundary.

The machine-readable contract lives in docs/ownership/scenedetect-rs-boundary.json.
This check keeps the repository a focused scene capability:

- no crate depends on generic visual-analysis or downstream corpus/product
  packages, except declared, unpublished transitional adapters;
- committed manifests never reach sibling checkouts through path dependencies,
  so a fresh clone resolves on its own;
- the documented scenedetect-core consumer seam still exists.
"""

from __future__ import annotations

import json
import re
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CONTRACT_RELATIVE = Path("docs/ownership/scenedetect-rs-boundary.json")
REPOSITORY = "moritzbrantner/scenedetect-rs"
REQUIRED_OWNED = {
    "detection-stats",
    "scene-boundary-detection-algorithms",
    "scene-list-derivation",
    "scene-timeline-projection",
}
DEPENDENCY_SECTIONS = ("dependencies", "dev-dependencies", "build-dependencies")
SOURCE_SPECIFIER_PREFIXES = ("file:", "link:", "portal:")
# The only transitional exception ADR 0011 accepts. It is pinned here so that editing the
# policy file this check protects cannot widen the exception.
PINNED_EXCEPTIONS = {
    "scenedetect-wasm": {
        "allowedPackages": ["moenarch-image-analysis-processing"],
        "requiresPublishFalse": True,
    },
}


@dataclass(frozen=True)
class Dependency:
    crate: str
    key: str
    package: str
    path: str | None


def load_contract(root: Path) -> dict[str, Any]:
    return json.loads((root / CONTRACT_RELATIVE).read_text())


def contract_errors(contract: dict[str, Any], root: Path) -> list[str]:
    errors: list[str] = []
    if contract.get("schemaVersion") != 1:
        errors.append("boundary contract must use schemaVersion 1")
    if contract.get("repository") != REPOSITORY:
        errors.append(f"boundary contract must name {REPOSITORY}")
    if contract.get("layer") != "capability":
        errors.append("scenedetect-rs must stay a capability-layer repository")
    owned = set(contract.get("ownedCapabilities", []))
    missing = sorted(REQUIRED_OWNED - owned)
    if missing:
        errors.append(f"boundary contract must own canonical scene capabilities: {missing}")
    for authority in contract.get("excludedAuthorities", []):
        if authority.get("authority") in owned:
            errors.append(f"capability is both owned and excluded: {authority.get('authority')}")

    seam = contract.get("consumerSeam", {})
    if seam.get("package") != "scenedetect-core":
        errors.append("the consumer seam must be the scenedetect-core package")
    contract_path = seam.get("contract")
    if not contract_path or not (root / contract_path).is_file():
        errors.append(f"consumer seam contract document is missing: {contract_path}")
    core_source = "\n".join(
        path.read_text() for path in sorted((root / "crates/scenedetect-core/src").rglob("*.rs"))
    )
    for entry in seam.get("entryPoints", []):
        pattern = rf"\bpub\s+(?:trait|struct|enum|fn|type)\s+{re.escape(entry)}\b"
        if not re.search(pattern, core_source):
            errors.append(f"consumer seam entry point is no longer public in scenedetect-core: {entry}")
    return errors


def forbidden_rules(contract: dict[str, Any]) -> tuple[set[str], tuple[str, ...]]:
    names: set[str] = set()
    prefixes: list[str] = []
    for authority in contract.get("excludedAuthorities", []):
        names.update(authority.get("forbiddenPackages", []))
        prefixes.extend(authority.get("packagePrefixes", []))
        prefixes.extend(authority.get("forbiddenPackagePrefixes", []))
    return names, tuple(prefixes)


def is_forbidden(package: str, names: set[str], prefixes: tuple[str, ...]) -> bool:
    return package in names or package.startswith(prefixes)


def package_manifests(root: Path, workspace: dict[str, Any]) -> list[Path]:
    """Every package manifest of the workspace: the root package, if any, and every member."""
    manifests: set[Path] = set()
    if "package" in workspace:
        manifests.add(root / "Cargo.toml")
    settings = workspace.get("workspace", {})
    excluded = {(root / pattern).resolve() for pattern in settings.get("exclude", [])}
    for pattern in settings.get("members", []):
        for member in root.glob(pattern):
            manifest = member / "Cargo.toml"
            if manifest.is_file() and member.resolve() not in excluded:
                manifests.add(manifest)
    return sorted(manifests)


def crate_dependencies(root: Path) -> tuple[list[Dependency], dict[str, dict[str, Any]]]:
    workspace = tomllib.loads((root / "Cargo.toml").read_text())
    workspace_dependencies = workspace.get("workspace", {}).get("dependencies", {})
    dependencies: list[Dependency] = []
    packages: dict[str, dict[str, Any]] = {}
    for manifest_path in package_manifests(root, workspace):
        manifest = tomllib.loads(manifest_path.read_text())
        crate = manifest["package"]["name"]
        packages[crate] = manifest["package"]
        tables = [manifest.get(section, {}) for section in DEPENDENCY_SECTIONS]
        for target in manifest.get("target", {}).values():
            tables.extend(target.get(section, {}) for section in DEPENDENCY_SECTIONS)
        for table in tables:
            for key, spec in table.items():
                if isinstance(spec, dict) and spec.get("workspace"):
                    base = workspace_dependencies.get(key, {})
                    spec = {**base, **spec} if isinstance(base, dict) else {"version": base}
                    base_path = root
                else:
                    base_path = manifest_path.parent
                package = spec.get("package", key) if isinstance(spec, dict) else key
                path = None
                if isinstance(spec, dict) and "path" in spec:
                    path = str((base_path / spec["path"]).resolve())
                dependencies.append(Dependency(crate, key, package, path))
    for key, spec in workspace_dependencies.items():
        package = spec.get("package", key) if isinstance(spec, dict) else key
        path = None
        if isinstance(spec, dict) and "path" in spec:
            path = str((root / spec["path"]).resolve())
        dependencies.append(Dependency("[workspace]", key, package, path))
    return dependencies, packages


def dependency_errors(contract: dict[str, Any], root: Path) -> list[str]:
    errors: list[str] = []
    names, prefixes = forbidden_rules(contract)
    exceptions = {
        exception["crate"]: exception for exception in contract.get("transitionalExceptions", [])
    }
    declared = {
        crate: {
            "allowedPackages": sorted(exception.get("allowedPackages", [])),
            "requiresPublishFalse": exception.get("requiresPublishFalse"),
        }
        for crate, exception in exceptions.items()
    }
    if declared != PINNED_EXCEPTIONS or len(exceptions) != len(contract.get("transitionalExceptions", [])):
        errors.append(
            "transitional exceptions must be exactly the pinned scenedetect-wasm -> "
            "moenarch-image-analysis-processing exception with requiresPublishFalse; "
            "widening it needs an ADR and a change to this check"
        )
        exceptions = {crate: {**PINNED_EXCEPTIONS[crate], "crate": crate} for crate in PINNED_EXCEPTIONS}
    dependencies, packages = crate_dependencies(root)
    for crate, exception in exceptions.items():
        package = packages.get(crate)
        if package is None:
            errors.append(f"transitional exception names an unknown crate: {crate}")
        elif exception.get("requiresPublishFalse") and package.get("publish") is not False:
            errors.append(f"transitional exception crate must stay unpublished: {crate}")

    resolved_root = root.resolve()
    for dependency in dependencies:
        if dependency.path is not None:
            path = Path(dependency.path)
            if path != resolved_root and resolved_root not in path.parents:
                errors.append(
                    f"{dependency.crate} reaches outside the repository through a path "
                    f"dependency ({dependency.key} -> {dependency.path}); fresh clones must resolve on their own"
                )
        if not is_forbidden(dependency.package, names, prefixes):
            continue
        allowed = exceptions.get(dependency.crate, {}).get("allowedPackages", [])
        if dependency.package in allowed:
            continue
        errors.append(
            f"{dependency.crate} depends on {dependency.package}, which belongs to an excluded authority"
        )
    return errors


def bun_errors(contract: dict[str, Any], root: Path) -> list[str]:
    errors: list[str] = []
    names, prefixes = forbidden_rules(contract)
    package_json = root / "package.json"
    if not package_json.is_file():
        return errors
    document = json.loads(package_json.read_text())
    for section in ("dependencies", "devDependencies", "optionalDependencies", "peerDependencies"):
        for name, specifier in document.get(section, {}).items():
            bare = name.rsplit("/", 1)[-1]
            if is_forbidden(name, names, prefixes) or is_forbidden(bare, names, prefixes):
                errors.append(f"package.json {section} depends on excluded package {name}")
            if isinstance(specifier, str) and specifier.startswith(SOURCE_SPECIFIER_PREFIXES):
                errors.append(
                    f"package.json {section} uses a local source specifier for {name} ({specifier}); "
                    "fresh clones must resolve on their own"
                )
    return errors


def validate(root: Path = ROOT) -> list[str]:
    contract = load_contract(root)
    return contract_errors(contract, root) + dependency_errors(contract, root) + bun_errors(contract, root)


def main() -> int:
    errors = validate()
    for error in errors:
        print(f"error: {error}", file=sys.stderr)
    if errors:
        return 1
    print("scene capability boundary checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
