"""Offline checks for release layout, integrity, and version guards."""
from pathlib import Path
import tarfile
import tempfile
import unittest
import zipfile

from package_release import NAME, TARGETS, check_tag, package, verify


class ReleasePackagingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.target_dir = self.root / "target"
        self.output = self.root / "dist"
        for name in ["LICENSE", "README.md", "examples/config.toml", "docs/RELEASING.md"]:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fixture\n", encoding="utf-8")
        (self.root / ".env").write_text("DO_NOT_PACKAGE", encoding="utf-8")
        for target in TARGETS:
            name = NAME + (".exe" if "windows" in target else "")
            binary = self.target_dir / target / "release" / name
            binary.parent.mkdir(parents=True, exist_ok=True)
            binary.write_bytes(b"fixture binary")
            binary.chmod(0o755)

    def build(self, target):
        return package(self.root, self.target_dir, self.output, "0.1.0", target)

    def test_archive_layout_and_checksums_for_all_targets(self):
        for target in TARGETS:
            archive = self.build(target)
            expected = {"LICENSE", "README.md", "config.example.toml", "docs/RELEASING.md"}
            expected.add(NAME + (".exe" if "windows" in target else ""))
            if "windows" in target:
                with zipfile.ZipFile(archive) as bundle:
                    self.assertEqual(set(bundle.namelist()), expected)
            else:
                with tarfile.open(archive) as bundle:
                    self.assertEqual(set(bundle.getnames()), expected)
                    self.assertTrue(bundle.getmember(NAME).mode & 0o111)
        verify(self.output, "0.1.0")

    def test_missing_extra_or_corrupt_assets_are_rejected(self):
        self.build(TARGETS[0])
        with self.assertRaises(ValueError):
            verify(self.output, "0.1.0")
        for target in TARGETS[1:]:
            self.build(target)
        extra = self.output / "unexpected"
        extra.touch()
        with self.assertRaises(ValueError):
            verify(self.output, "0.1.0")
        extra.unlink()
        archive = self.build(TARGETS[0])
        archive.write_bytes(b"corrupted")
        with self.assertRaisesRegex(ValueError, "Checksum mismatch"):
            verify(self.output, "0.1.0")

    def test_missing_input_and_unsupported_target_are_rejected(self):
        (self.root / "LICENSE").unlink()
        with self.assertRaises(ValueError):
            self.build(TARGETS[0])
        self.assertFalse(self.output.exists())
        with self.assertRaises(ValueError):
            self.build("../../outside")

    def test_tag_must_match_stable_manifest_version(self):
        check_tag("v0.1.0", "0.1.0")
        for tag in ["0.1.0", "v0.2.0", "v0.1.0-beta.1", "v0.1.0/extra", "v0.1.0\n"]:
            with self.assertRaises(ValueError):
                check_tag(tag, "0.1.0")


if __name__ == "__main__":
    unittest.main()
