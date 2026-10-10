#!/usr/bin/env python3
"""Tests for scripts/check-scene-boundary.py against the real repository and
synthetic copies that violate one boundary rule each."""

from __future__ import annotations

import importlib.util
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check-scene-boundary.py")
SPEC = importlib.util.spec_from_file_location("check_scene_boundary", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
boundary = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = boundary
SPEC.loader.exec_module(boundary)

ROOT = boundary.ROOT


class SceneBoundaryTests(unittest.TestCase):
    def setUp(self) -> None:
        self._temp = tempfile.TemporaryDirectory()
        self.root = Path(self._temp.name) / "scenedetect-rs"
        self.root.mkdir()
        for relative in ("Cargo.toml", "package.json", "docs/ownership", "docs/adr"):
            source = ROOT / relative
            target = self.root / relative
            if source.is_dir():
                shutil.copytree(source, target)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, target)
        for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml")):
            crate = self.root / "crates" / manifest.parent.name
            crate.mkdir(parents=True)
            shutil.copy2(manifest, crate / "Cargo.toml")
        shutil.copytree(ROOT / "crates/scenedetect-core/src", self.root / "crates/scenedetect-core/src")

    def tearDown(self) -> None:
        self._temp.cleanup()

    def manifest(self, crate: str) -> Path:
        return self.root / "crates" / crate / "Cargo.toml"

    def append_dependency(self, crate: str, line: str) -> None:
        path = self.manifest(crate)
        text = path.read_text()
        path.write_text(text.replace("[dependencies]\n", f"[dependencies]\n{line}\n", 1))

    def contract(self) -> dict:
        return json.loads((self.root / boundary.CONTRACT_RELATIVE).read_text())

    def write_contract(self, document: dict) -> None:
        (self.root / boundary.CONTRACT_RELATIVE).write_text(json.dumps(document, indent=2))

    def test_repository_satisfies_its_boundary(self) -> None:
        self.assertEqual(boundary.validate(ROOT), [])
        self.assertEqual(boundary.validate(self.root), [])

    def test_core_rejects_generic_visual_analysis_dependency(self) -> None:
        self.append_dependency(
            "scenedetect-core",
            'histograms = { package = "moenarch-image-analysis-processing", version = "0.1.0" }',
        )
        errors = boundary.validate(self.root)
        self.assertTrue(
            any("scenedetect-core depends on moenarch-image-analysis-processing" in e for e in errors),
            errors,
        )

    def test_corpus_and_product_repositories_are_rejected_everywhere(self) -> None:
        self.append_dependency("scenedetect-cli", 'youtube-corpus = "0.1"')
        self.append_dependency(
            "scenedetect-wasm", 'mi = { package = "media-intelligence-core", version = "0.1" }'
        )
        errors = boundary.validate(self.root)
        self.assertTrue(any("scenedetect-cli depends on youtube-corpus" in e for e in errors), errors)
        self.assertTrue(
            any("scenedetect-wasm depends on media-intelligence-core" in e for e in errors), errors
        )

    def test_exception_only_covers_its_declared_packages(self) -> None:
        self.append_dependency(
            "scenedetect-wasm",
            'ocr = { package = "moenarch-image-analysis-ocr", version = "0.1.0" }',
        )
        errors = boundary.validate(self.root)
        self.assertTrue(
            any("scenedetect-wasm depends on moenarch-image-analysis-ocr" in e for e in errors), errors
        )

    def test_exception_crate_must_stay_unpublished(self) -> None:
        path = self.manifest("scenedetect-wasm")
        path.write_text(path.read_text().replace("publish = false\n", "", 1))
        errors = boundary.validate(self.root)
        self.assertIn("transitional exception crate must stay unpublished: scenedetect-wasm", errors)

    def test_sibling_path_dependencies_break_fresh_clones(self) -> None:
        self.append_dependency(
            "scenedetect-core",
            'media-core = { package = "moenarch-media-core", path = "../../../moenarch-foundation/crates/media/media-core" }',
        )
        errors = boundary.validate(self.root)
        self.assertTrue(any("reaches outside the repository" in e for e in errors), errors)

    def test_local_bun_specifiers_break_fresh_clones(self) -> None:
        path = self.root / "package.json"
        document = json.loads(path.read_text())
        document.setdefault("devDependencies", {})["@moritzbrantner/moonlight"] = "file:../moonlight"
        path.write_text(json.dumps(document))
        errors = boundary.validate(self.root)
        self.assertTrue(any("local source specifier" in e for e in errors), errors)

    def test_consumer_seam_entry_points_must_stay_public(self) -> None:
        document = self.contract()
        document["consumerSeam"]["entryPoints"].append("detect_everything")
        self.write_contract(document)
        errors = boundary.validate(self.root)
        self.assertIn(
            "consumer seam entry point is no longer public in scenedetect-core: detect_everything",
            errors,
        )

    def test_inherited_renamed_workspace_dependency_is_resolved(self) -> None:
        cargo = self.root / "Cargo.toml"
        cargo.write_text(
            cargo.read_text().replace(
                "[workspace.dependencies]\n",
                '[workspace.dependencies]\nscene-corpus = { package = "youtube-corpus", version = "0.1" }\n',
                1,
            )
        )
        self.append_dependency("scenedetect-core", "scene-corpus = { workspace = true }")
        errors = boundary.validate(self.root)
        self.assertTrue(any("scenedetect-core depends on youtube-corpus" in e for e in errors), errors)

    def test_target_specific_dependency_is_checked(self) -> None:
        path = self.manifest("scenedetect-cli")
        path.write_text(
            path.read_text()
            + '\n[target.\'cfg(unix)\'.dependencies]\n'
            + 'proc = { package = "moenarch-image-analysis-processing", version = "0.1.0" }\n'
        )
        errors = boundary.validate(self.root)
        self.assertTrue(
            any("scenedetect-cli depends on moenarch-image-analysis-processing" in e for e in errors),
            errors,
        )

    def test_workspace_members_outside_crates_are_checked(self) -> None:
        helper = self.root / "tools" / "helper"
        helper.mkdir(parents=True)
        (helper / "Cargo.toml").write_text(
            '[package]\nname = "scene-helper"\nversion = "0.1.0"\nedition = "2021"\n\n'
            '[dependencies]\nyoutube-corpus = "0.1"\n'
        )
        cargo = self.root / "Cargo.toml"
        cargo.write_text(cargo.read_text().replace("members = [\n", 'members = [\n    "tools/*",\n', 1))
        errors = boundary.validate(self.root)
        self.assertTrue(any("scene-helper depends on youtube-corpus" in e for e in errors), errors)

    def test_contract_edits_cannot_widen_the_exception(self) -> None:
        widenings = [
            lambda exception: exception["allowedPackages"].append("moenarch-image-analysis-ocr"),
            lambda exception: exception.update(requiresPublishFalse=False),
        ]
        for widen in widenings:
            document = self.contract()
            widen(document["transitionalExceptions"][0])
            self.write_contract(document)
            errors = boundary.validate(self.root)
            self.assertTrue(any("must be exactly the pinned" in e for e in errors), errors)
        document = self.contract()
        document["transitionalExceptions"].append(
            {
                "crate": "scenedetect-cli",
                "requiresPublishFalse": True,
                "allowedPackages": ["youtube-corpus"],
            }
        )
        self.write_contract(document)
        self.append_dependency("scenedetect-cli", 'youtube-corpus = "0.1"')
        errors = boundary.validate(self.root)
        self.assertTrue(any("must be exactly the pinned" in e for e in errors), errors)
        self.assertTrue(any("scenedetect-cli depends on youtube-corpus" in e for e in errors), errors)

    def test_canonical_scene_ownership_cannot_be_dropped(self) -> None:
        document = self.contract()
        document["ownedCapabilities"].remove("scene-boundary-detection-algorithms")
        self.write_contract(document)
        errors = boundary.validate(self.root)
        self.assertTrue(any("must own canonical scene capabilities" in e for e in errors), errors)


if __name__ == "__main__":
    unittest.main()
