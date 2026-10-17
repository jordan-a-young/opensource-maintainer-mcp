"""Offline regression tests for native npm manifests and archive extraction."""
import copy
import json
from pathlib import Path
import tarfile
import tempfile
import subprocess
import unittest
from unittest.mock import patch

import package_npm
from package_release import archive_name


class NpmPackagingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = json.loads((package_npm.ROOT / "npm/package.json").read_text())
        for name in ["LICENSE", "npm/README.md", "npm/bin/launcher.cjs", "examples/config.toml"]:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fixture")
        self.write_source(self.source)
        self.patcher = patch.object(package_npm, "ROOT", self.root)
        self.patcher.start()
        self.addCleanup(self.patcher.stop)

    def write_source(self, data):
        (self.root / "npm/package.json").write_text(json.dumps(data))

    def test_generates_five_allowlisted_packages_without_install_scripts(self):
        package_npm.prepare("0.1.0", package_npm.TARGETS, lambda *_: b"binary")
        root = self.root / "npm-dist/packages"
        self.assertEqual(len(list(root.iterdir())), 5)
        for directory in root.iterdir():
            data = json.loads((directory / "package.json").read_text())
            self.assertNotIn("private", data)
            self.assertNotIn("scripts", data)
            self.assertEqual(data["publishConfig"]["access"], "public")
        linux = json.loads((root / "linux-x64/package.json").read_text())
        self.assertEqual(linux["libc"], ["glibc"])
        self.assertEqual(linux["os"], ["linux"])
        self.assertEqual(linux["cpu"], ["x64"])
        self.assertTrue((root / "win32-x64/bin/opensource-maintainer-mcp.exe").is_file())

    def test_rejects_version_drift_and_nonprivate_source_manifest(self):
        with self.assertRaises(ValueError):
            package_npm.manifest("0.2.0")
        for change in [{"private": False}, {"optionalDependencies": {}}]:
            source = copy.deepcopy(self.source)
            source.update(change)
            self.write_source(source)
            with self.assertRaises(ValueError):
                package_npm.manifest("0.1.0")

    def test_never_follows_symlinks_in_archives(self):
        target = "x86_64-unknown-linux-gnu"
        archive = self.root / archive_name("0.1.0", target)
        with tarfile.open(archive, "w:gz") as bundle:
            entry = tarfile.TarInfo("opensource-maintainer-mcp")
            entry.type = tarfile.SYMTYPE
            entry.linkname = "/etc/passwd"
            bundle.addfile(entry)
        with self.assertRaises(ValueError):
            package_npm.archive_binary(self.root, "0.1.0", target, "opensource-maintainer-mcp")


class CargoPackageBoundaryTests(unittest.TestCase):
    def test_generated_npm_readmes_and_licenses_stay_out_of_cargo_package(self):
        root = Path(__file__).resolve().parent.parent
        generated = root / "npm-dist"
        generated.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=generated) as directory:
            for name in ["README.md", "LICENSE"]:
                (Path(directory) / name).write_text("generated package fixture")
            result = subprocess.run(
                ["cargo", "package", "--list", "--locked", "--allow-dirty"],
                cwd=root, capture_output=True, text=True, check=True,
            )
            paths = result.stdout.splitlines()
            self.assertIn("README.md", paths)
            self.assertFalse(any(path.startswith("npm-dist/") for path in paths))
