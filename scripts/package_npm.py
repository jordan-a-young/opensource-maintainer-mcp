"""Generate local npm package directories from tested native binaries; never publish."""

import argparse
import json
from pathlib import Path
import shutil
import tarfile
import zipfile

from package_release import NAME, TARGETS, archive_name, metadata, verify

ROOT = Path(__file__).resolve().parent.parent
PLATFORMS = {
    "x86_64-unknown-linux-gnu": ("linux", "x64"),
    "x86_64-pc-windows-msvc": ("win32", "x64"),
    "aarch64-apple-darwin": ("darwin", "arm64"),
    "x86_64-apple-darwin": ("darwin", "x64"),
}


def manifest(version):
    data = json.loads((ROOT / "npm/package.json").read_text(encoding="utf-8"))
    if data["version"] != version or not data.get("private"):
        raise ValueError(
            "Source npm manifest must be private and match Cargo's version"
        )
    expected = {f"{data['name']}-{os}-{cpu}": version for os, cpu in PLATFORMS.values()}
    if data.get("optionalDependencies") != expected:
        raise ValueError(
            "Optional native packages must exactly match the release version and platforms"
        )
    data.pop("private")
    data.pop("scripts")
    return data


def write_package(destination, data):
    destination.mkdir(parents=True)
    (destination / "package.json").write_text(
        json.dumps(data, indent=2) + "\n", encoding="utf-8"
    )
    shutil.copyfile(ROOT / "LICENSE", destination / "LICENSE")
    shutil.copyfile(ROOT / "npm/README.md", destination / "README.md")


def prepare(version, targets, binary_reader):
    data = manifest(version)
    output = ROOT / "npm-dist"
    if output.exists():
        shutil.rmtree(output)
    packages = output / "packages"
    launcher = packages / "launcher"
    write_package(launcher, data)
    (launcher / "bin").mkdir()
    shutil.copyfile(ROOT / "npm/bin/launcher.cjs", launcher / "bin/launcher.cjs")
    (launcher / "bin/launcher.cjs").chmod(0o755)
    shutil.copyfile(ROOT / "examples/config.toml", launcher / "config.example.toml")
    for target in targets:
        system, cpu = PLATFORMS[target]
        suffix = f"{system}-{cpu}"
        binary_name = NAME + (".exe" if system == "win32" else "")
        native = {
            key: data[key]
            for key in [
                "version",
                "description",
                "license",
                "repository",
                "publishConfig",
            ]
        }
        native.update(
            name=f"{data['name']}-{suffix}",
            os=[system],
            cpu=[cpu],
            files=[f"bin/{binary_name}"],
        )
        if system == "linux":
            native["libc"] = ["glibc"]
        destination = packages / suffix
        write_package(destination, native)
        (destination / "bin").mkdir()
        binary = destination / "bin" / binary_name
        binary.write_bytes(binary_reader(target, binary_name))
        binary.chmod(0o755)


def archive_binary(archives, version, target, name):
    archive = archives / archive_name(version, target)
    limit = 256 * 1024 * 1024
    if "windows" in target:
        with zipfile.ZipFile(archive) as bundle:
            info = bundle.getinfo(name)
            if (
                info.file_size > limit
                or (info.external_attr >> 16) & 0o170000 == 0o120000
            ):
                raise ValueError("Unsupported binary archive entry")
            return bundle.read(info)
    with tarfile.open(archive) as bundle:
        info = bundle.getmember(name)
        if not info.isfile() or info.size > limit:
            raise ValueError("Unsupported binary archive entry")
        return bundle.extractfile(info).read()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--target", choices=TARGETS)
    mode.add_argument("--archives", type=Path)
    args = parser.parse_args()
    version, target_dir = metadata()
    if args.archives:
        archives = args.archives.resolve()
        verify(archives, version)
        output = (ROOT / "npm-dist").resolve()
        inputs = [archives, *(path.resolve() for path in archives.iterdir())]
        if any(path == output or output in path.parents for path in inputs):
            raise ValueError("Release archive inputs must not overlap npm-dist output")
        prepare(
            version,
            TARGETS,
            lambda target, name: archive_binary(archives, version, target, name),
        )
    else:

        def read_binary(target, name):
            source = target_dir / target / "release" / name
            if not source.is_file() or source.is_symlink():
                raise ValueError("Expected a regular built binary")
            return source.read_bytes()

        prepare(version, [args.target], read_binary)


if __name__ == "__main__":
    main()
