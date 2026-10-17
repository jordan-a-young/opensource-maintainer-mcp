# Release process

This is release infrastructure, not a release announcement. No version tag, published
GitHub release, or crates.io upload is created by merging the preparation PR.

## What runs automatically

On every PR and push to `main`, CI runs formatting and Clippy, tests the packaging
helpers, and runs `cargo publish --dry-run --locked`. The dry run verifies the Cargo
package without uploading it. A shared build workflow runs the full mocked Rust test
suite and builds release binaries on all four native targets:

| Platform | Target | Hosted build runner | Archive |
| --- | --- | --- | --- |
| Linux x64 | `x86_64-unknown-linux-gnu` | Ubuntu 22.04 | `.tar.gz` |
| Windows x64 | `x86_64-pc-windows-msvc` | Windows Server 2022 | `.zip` |
| macOS Apple Silicon | `aarch64-apple-darwin` | macOS 15 | `.tar.gz` |
| macOS Intel | `x86_64-apple-darwin` | macOS 15 Intel | `.tar.gz` |

CI stores archives and individual SHA-256 checksum files as temporary Actions artifacts
for seven days. These are test artifacts, not published GitHub releases. Public-repository
Actions artifacts may be downloadable by signed-in users. Each archive contains the
binary, MIT license, README, example configuration, and this release guide. Linux uses
glibc (built on Ubuntu 22.04); Alpine/musl and older distributions are not supported by
this binary. Windows ARM64 and Linux ARM64 are not included. macOS binaries are not
Developer ID signed/notarized, and Windows binaries are not Authenticode signed.
Checksums detect corruption; they are not code-signing certificates or attestations.

The `rust` job retains its name. Platform builds appear as separate checks; review all
of them before merging. Repository rulesets/required checks are managed separately.

## First release: deliberate maintainer steps

1. Review and merge the preparation PR, then wait for all `main` CI jobs to pass.
2. Review the version in `Cargo.toml`, the packaged files, and release notes. For the
   first release the version is `0.1.0`; subsequent releases need a version-bump PR
   that also updates `Cargo.lock` and passes CI.
3. Only when a release is authorized, create and push an annotated `vX.Y.Z` tag on the
   reviewed main commit. Pushing the tag makes that Git reference public immediately.
   This step is intentionally **not** done during release preparation.
4. The tag starts **Prepare draft release**. It requires an exact stable version match
   with `Cargo.toml` and a commit in `main` history, reruns the full CI/build workflow,
   downloads only that run's artifacts, and verifies all four archives and checksums.
5. After all checks succeed, the workflow creates a **draft** GitHub release with eight
   assets (four archives plus four checksum files) and generated release notes. Only
   this final job has `contents: write`; tests and builds have read-only repository
   permissions. It uses the job's GitHub token, not a personal access token.
6. Review the draft's notes, version, commit, platform assets, and checksums. Download
   the intended platform archive and smoke-test the executable with an MCP client.
7. Publish the draft manually in GitHub only after explicit release approval. The
   workflow never publishes the draft and never runs `cargo publish` without `--dry-run`.

Future commands for step 3 (do not run as part of setup):

```sh
git switch main
git pull --ff-only
git tag -a v0.1.0 -m 'Release v0.1.0'
git push origin v0.1.0
```

The workflow handles stable tags only; prerelease version support requires a separate
change. It rejects tags pointing outside main history. It does not enforce that the tag
is the newest main commit, so maintainers must select the reviewed commit deliberately.
A draft release does not hide its already-public source tag.

## Failure and retry behavior

A failed test, package check, build, checksum check, or tag validation prevents the draft
job. Fix failures through a reviewed PR. Do not move published version tags; use a new
version for released fixes. Failures during asset upload may leave a partial draft.
Inspect that draft before retrying. `gh release create` refuses an existing release, so
it does not overwrite an existing draft or published release. If a partial draft must be
recreated, a maintainer may delete **only the unpublished draft**, preserve the tag,
and rerun the failed workflow. No automatic cleanup deletes releases or tags.

## crates.io is a separate future decision

This setup prepares and validates the source package but does not configure registry
credentials or a crates.io publisher. Nothing uploads to crates.io. Before a first
registry release, confirm ownership/name availability, select authentication, inspect
`cargo package --list --locked`, and repeat `cargo publish --dry-run --locked` on the
reviewed version. Only then authorize an explicit upload or a separate publishing-workflow
PR. A GitHub binary release and a crates.io package are independent distribution channels.
The README must not promise `cargo install opensource-maintainer-mcp` until it is published.

## Local validation

Requires stable Rust and Python 3.9+ (Python is development tooling only).

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
python3 -m unittest discover -s scripts -p 'test_*.py'
cargo publish --dry-run --locked
cargo build --release --locked --target x86_64-unknown-linux-gnu
python3 scripts/package_release.py --target x86_64-unknown-linux-gnu
```

Use your native target from the table; the archive helper does not cross-compile.
Cargo refuses a dirty worktree for packaging by default. During local development only,
`--allow-dirty` may be appended to the dry run; CI verifies a clean checkout. `dist/` is
ignored. `--check-tag v0.1.0` validates without creating a tag; `--verify` expects the
complete four-platform asset set and verifies its checksums without network access.

Rust learning note: this change leaves the server's ownership, traits, and error model
unchanged. Cargo's explicit package file list controls what ships to a registry; Cargo.lock
records dependency resolution. Native builds exercise the same Rust code and mocked
HTTP tests on each platform. The small Python helper only packages already-built binaries
and uses an explicit file allowlist, keeping release automation outside the MCP server.

References: [Cargo publish](https://doc.rust-lang.org/cargo/commands/cargo-publish.html),
[GitHub draft release creation](https://cli.github.com/manual/gh_release_create),
[hosted runner targets](https://github.com/actions/runner-images).
