"""Build/verify local release archives. Never uploads or publishes anything."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tarfile
import zipfile

TARGETS = (
    "x86_64-unknown-linux-gnu",
    "x86_64-pc-windows-msvc",
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
)
NAME = "opensource-maintainer-mcp"


def metadata():
    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
        check=True, capture_output=True, text=True,
    )
    data = json.loads(result.stdout)
    package = next(p for p in data["packages"] if p["name"] == NAME)
    return package["version"], Path(data["target_directory"])


def check_tag(tag, version):
    if not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", tag) or tag != f"v{version}":
        raise ValueError("Release tag must exactly match the stable Cargo version (vX.Y.Z)")


def archive_name(version, target):
    if target not in TARGETS:
        raise ValueError("Unsupported release target")
    suffix = ".zip" if "windows" in target else ".tar.gz"
    return f"{NAME}-v{version}-{target}{suffix}"


def checksum(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return f"{digest.hexdigest()}  {path.name}\n"


def package(root, target_dir, output, version, target):
    archive = output / archive_name(version, target)
    binary_name = NAME + (".exe" if "windows" in target else "")
    files = [
        (target_dir / target / "release" / binary_name, binary_name),
        (root / "LICENSE", "LICENSE"),
        (root / "README.md", "README.md"),
        (root / "examples/config.toml", "config.example.toml"),
        (root / "docs/RELEASING.md", "docs/RELEASING.md"),
    ]
    if any(not path.is_file() or path.is_symlink() for path, _ in files):
        raise ValueError("Missing or non-regular release input")
    output.mkdir(parents=True, exist_ok=True)
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as bundle:
            for path, name in files:
                bundle.write(path, name)
    else:
        with tarfile.open(archive, "w:gz") as bundle:
            for path, name in files:
                bundle.add(path, arcname=name, recursive=False)
    archive.with_name(archive.name + ".sha256").write_bytes(checksum(archive).encode("utf-8"))
    return archive


def verify(output, version):
    expected = {archive_name(version, target) for target in TARGETS}
    expected |= {name + ".sha256" for name in tuple(expected)}
    if {path.name for path in output.iterdir()} != expected:
        raise ValueError("Release asset set is incomplete or contains unexpected files")
    for target in TARGETS:
        archive = output / archive_name(version, target)
        recorded = archive.with_name(archive.name + ".sha256").read_text(encoding="utf-8")
        if recorded != checksum(archive):
            raise ValueError(f"Checksum mismatch: {archive.name}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--target", choices=TARGETS)
    mode.add_argument("--check-tag")
    mode.add_argument("--verify", action="store_true")
    args = parser.parse_args()
    version, target_dir = metadata()
    if args.check_tag:
        check_tag(args.check_tag, version)
    elif args.verify:
        verify(Path("dist"), version)
    else:
        package(Path("."), target_dir, Path("dist"), version, args.target)


if __name__ == "__main__":
    main()
