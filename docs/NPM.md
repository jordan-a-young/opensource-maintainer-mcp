# npm distribution and trusted publishing

The intended public package is `@jys/opensource-maintainer-mcp`. `jys` is the chosen npm
organization scope, not the GitHub username. Ownership/availability of that scope has
not been verified. GitHub remains `jordan-a-young/opensource-maintainer-mcp`.

## What is prepared without publishing

The repository contains a small Node launcher and local packaging helpers. Native
packages contain the already-tested Rust binaries; there are no install scripts,
postinstall downloads, runtime downloads, or compilation on the user's computer.
Optional dependencies select the supported OS/CPU package with an exact version match.
Linux requires glibc. Missing, unsupported, or mismatched binaries fail with a diagnostic
on stderr. The launcher preserves MCP stdio and forwards arguments, exit codes, and signals
(where supported by the host OS).

The source `npm/package.json` is deliberately private. Packaging generates publishable
copies under ignored `npm-dist/`, taking binaries either from a native release build or
from the verified four-platform archive set. It excludes development scripts and files.
Only these generated packages are candidates for a future release:

| Package | Purpose |
| --- | --- |
| `@jys/opensource-maintainer-mcp` | User-facing launcher |
| `@jys/opensource-maintainer-mcp-linux-x64` | Linux x64 glibc executable |
| `@jys/opensource-maintainer-mcp-win32-x64` | Windows x64 executable |
| `@jys/opensource-maintainer-mcp-darwin-arm64` | Apple Silicon executable |
| `@jys/opensource-maintainer-mcp-darwin-x64` | Intel Mac executable |

All five versions must match Cargo.toml. Update the source npm version and all optional
dependency versions in each version-bump PR. Native binaries are not code-signed/notarized.
No crates.io publication is necessary for npm installation.

## What you can do in npm now

1. Sign in as `jordan-a-young`, verify your account email, and enable account 2FA.
2. Create or obtain access to the `jys` organization if available. Your account needs
   permission to publish public packages under that scope. Do not choose a paid/private
   package plan just for this public project. If the scope is unavailable, change the
   source npm name, dependency names, and docs together before release.
3. Stop there until a first-package bootstrap is explicitly approved. Do not publish
   placeholders or run `npm stage publish` as part of this preparation.

**npm requires an existing registry package before configuring trust.** Staging a new
package creates a publicly visible `0.0.0-stage` placeholder. Consequently, complete
account-side activation cannot be done while promising that nothing is published.
The first-package bootstrap requires a separate approval, interactive npm authentication,
and a review of the five exact packages. No bootstrap or staging is performed by CI.
See [npm trust prerequisites](https://docs.npmjs.com/cli/v11/commands/npm-trust/) and
[staged publishing behavior](https://docs.npmjs.com/staged-publishing/).

## Account settings once the packages exist

For **each of the five packages**, choose GitHub Actions in its npm trusted-publisher
settings and enter:

| Field | Value |
| --- | --- |
| GitHub organization/user | `jordan-a-young` |
| Repository | `opensource-maintainer-mcp` |
| Workflow filename | `npm-publish.yml` (no directory prefix) |
| Environment | `npm` |
| Allowed operation | Stage only; do not enable direct publishing |

Do not create an `NPM_TOKEN` GitHub secret for this workflow. It uses GitHub OIDC and
explicitly rejects static npm token variables. Configure trust near the intended first
use: npm currently expires new unvalidated trust configurations after two days. A dry
run does not validate account-side OIDC trust. After successful setup, use npm's publishing
access option requiring 2FA and disallowing traditional tokens.
[Trusted publishing documentation](https://docs.npmjs.com/trusted-publishers/).

## GitHub settings and approval gates

The manual workflow is `.github/workflows/npm-publish.yml`. Its default mode is `dry-run`;
there are no npm publishing triggers on PRs, pushes, tags, or GitHub release publication.

For this repository, preparation configured the `npm` environment with `jordan-a-young`
as required reviewer, allowed only `v*` tags, and set repository variable
`NPM_STAGING_ENABLED=false`. Self-review is permitted so the sole maintainer can explicitly
approve their own manual run. For forks, configure these settings separately. Keep the
staging variable unset or `false` throughout preparation. Enabling it is a separate,
explicit release decision after npm account setup. No npm account settings are modified
by the repository's tests or workflows.

When staging is explicitly approved:

1. Merge a reviewed version-bump PR and wait for all CI checks.
2. Create the approved `vX.Y.Z` tag on a reviewed commit in main history. This separately
   triggers the existing **draft-only GitHub release** process; the tag itself is public.
3. Manually run **Prepare or stage npm packages** on that **tag**, choose `stage`, and
   enter `stage-reviewed-packages`. The enable variable, confirmation, tag/version match,
   main ancestry, all CI checks, and environment approval must pass.
4. CI rebuilds/tests all four native targets, installs local tarballs offline on each
   platform, assembles all five npm tarballs, checks their file lists, and dry-runs them.
   It stages those same checked tarballs using OIDC. This uploads to npm but does not
   approve them for public installation. Do not run this step during preparation.
5. Review the staged packages on npm. Approve the **four native packages first**, confirm
   their exact versions are available, then approve the launcher last. Approval requires
   npm 2FA. The workflow contains no `npm stage approve` or direct `npm publish` operation.
6. Smoke-test installation of the published version through `npx`. Publishing the GitHub
   release draft is an independent decision; npm users receive bundled native packages
   and do not download binaries from GitHub at install time.

Staging/publishing multiple packages is not atomic. If some stages succeed and another
fails, inspect npm's staged list before retrying; the helper does not overwrite, remove,
or auto-approve anything. Never approve a launcher whose native versions are unavailable.
A failed/partial bootstrap needs the same deliberate review. npm authentication and the
complete staged-to-public path remain untested until an authorized real release.

## Local and CI validation (no registry writes)

Requires Python 3.9+, stable Rust, and Node 22.14+; CI uses Node 24. Future staging pins npm
11.16.0, which supports OIDC and staging. No npm dependency installation is needed to run
unit tests: they use Node's standard library only, so there is no development lockfile.
The launcher pins each native optional dependency to the exact package version.

```sh
npm --prefix npm test
python3 -m unittest discover -s scripts -p 'test_*.py'
cargo build --release --locked --target x86_64-unknown-linux-gnu
python3 scripts/package_npm.py --target x86_64-unknown-linux-gnu
npm --prefix npm run test:install
```

The integration test uses local tarballs, `--offline`, disabled lifecycle scripts, and a
loopback-only registry URL. It verifies real npm command-shim startup, configuration paths
with spaces, MCP initialization, all six tools, and shutdown, without calling GitHub.
It intentionally omits unavailable nonnative optional packages. Native selection is also
covered by npm `os`/`cpu`/`libc` metadata and launcher tests; registry resolution itself
will be smoke-tested after publication.

To validate the complete set, put the four original native release archives and checksum
files in `dist/`, then run:

```sh
python3 scripts/package_npm.py --archives dist
npm --prefix npm run pack:all
```

This creates five tarballs and a checksum/name/version manifest in `npm-dist/tarballs/`,
then runs `npm publish --dry-run --offline` for each. Nothing uploads. PR CI performs this
same check after all platform jobs and retains the npm tarballs as temporary Actions
artifacts. Like existing binary CI artifacts, these are downloadable development artifacts
in a public repository, not npm registry publications.

Rust learning: the runtime remains Rust and its API is unchanged. The Node launcher is
only process glue; Python packages the existing native outputs. This keeps npm distribution
separate from MCP handlers and read-only repository analysis.
