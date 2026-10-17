# Open Source Maintainer MCP

A read-only [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) server
that helps maintainers inspect public GitHub projects from an MCP-compatible assistant.
Configure a GitHub user or organization and a topic, then ask the assistant to discover
projects, summarize their setup, compare conventions, and review CI and release settings.
Availity is the example configuration; any supported owner/topic can be used.

Use it to orient yourself in an unfamiliar project or identify maintenance work across
related repositories. Its six tools return structured observations and evidence from
bounded static inspection. They do not modify repositories, run project scripts, publish
packages, inspect private repositories, or certify security or release readiness.

The server runs locally as a stdio process launched by your MCP client and reads the
public GitHub API over HTTPS. It requires no database or hosted service. Source builds
are usable now; npm packages and downloadable releases are prepared for future publication.

- [Setup and run](#setup-and-run) and [MCP client configuration](#mcp-client-configuration)
- [Available tools](#available-tools) and [example requests](#example-requests)
- [Troubleshooting](#troubleshooting)
- [Contributor guidance](AGENTS.md) and [tests and checks](#tests-and-checks)
- [Release maintenance](docs/RELEASING.md) and [npm publishing setup](docs/NPM.md)

## Setup and run

Install stable Rust with [rustup](https://rustup.rs/) if it is not already installed.
Git is needed for development; GitHub CLI is optional for repository contribution.

```sh
git clone https://github.com/jordan-a-young/opensource-maintainer-mcp.git
cd opensource-maintainer-mcp
cargo build --locked
cp examples/config.toml config.toml
```

Edit `config.toml` to select your owner and topic, then configure your MCP client below.
In PowerShell, use `Copy-Item examples/config.toml config.toml` for the copy step.
To start the process directly for diagnosis, run `cargo run --locked -- config.toml`.

The server waits for MCP JSON-RPC messages on stdin; it is not an interactive CLI.
Stdout contains only protocol messages. Diagnostics go to stderr. Set `RUST_LOG=debug`
for additional diagnostics. Use an absolute configuration path when launched by a client.

```toml
[github]
owner = "availity"
topic = "availity-opensource"
include_archived = false
```

Both owner and topic are required and validated. Archived repositories are excluded by
default. Unknown keys and invalid values fail startup with a nonzero exit code.
The first positional argument is the TOML path (default: `config.toml`).

`GITHUB_TOKEN` is optional. Anonymous public access works with lower rate limits. If
needed, supply a fine-grained token with read-only public repository metadata access
through your environment or client's secret mechanism. No write permissions are needed.
The server does not read `gh` credentials automatically. Never put tokens in TOML or Git.

## Installation and releases

Release automation is prepared; no published release is promised yet. Until a release
is published, build from source with `cargo build --release --locked` and use the binary
in `target/release/` (with `.exe` on Windows).

Once a release is published, download the archive matching your OS/CPU and its `.sha256`
file from [GitHub Releases](https://github.com/jordan-a-young/opensource-maintainer-mcp/releases).
Verify the downloaded archive before extracting:

```sh
# Linux: run in the download directory, replacing ARCHIVE with the actual filename
sha256sum -c ARCHIVE.tar.gz.sha256
# macOS
shasum -a 256 -c ARCHIVE.tar.gz.sha256
```

On Windows, run `Get-FileHash .\ARCHIVE.zip -Algorithm SHA256` in PowerShell and compare
its hash with the first field in `ARCHIVE.zip.sha256`. Extract the archive, copy
`config.example.toml` to `config.toml`, and edit the owner/topic. Point your MCP client's
`command` at the extracted executable and its `args` at the configuration using absolute
paths. No Rust or Python installation is required to run a downloaded binary. The same
stdio protocol and optional `GITHUB_TOKEN` configuration apply.

Archives cover Linux x64 (glibc, built on Ubuntu 22.04), Windows x64, and macOS Intel and
Apple Silicon. Binaries are not code-signed/notarized; checksums provide integrity checks.
See the [release guide](docs/RELEASING.md) for the draft-first flow, platform limits,
validation, and the separate future crates.io decision.

## npm installation (prepared, not published yet)

npm is the intended primary installation route; crates.io is optional. After the first
approved npm release, users will be able to run:

```sh
npx --yes @jys/opensource-maintainer-mcp@0.1.0 /absolute/path/config.toml
```

The launcher needs Node 22.14+ and selects the appropriate prebuilt Rust package. No Rust
installation, postinstall script, or GitHub binary download is needed. The packages are
not available on npm yet. See [npm setup and release gates](docs/NPM.md) for the account
checklist, first-package bootstrap limitation, platform support, and trusted staging flow.
See [the npm README](npm/README.md) for the planned MCP client configuration.

## MCP client configuration

For step-by-step OpenCode, Pi, Codex, and Kiro setup, see the
[client guide](docs/CLIENTS.md), including Windows/WSL and connection verification.

Build the executable, then adapt these absolute paths to your checkout:

```json
{
  "mcpServers": {
    "opensource-maintainer": {
      "command": "/absolute/path/opensource-maintainer-mcp/target/debug/opensource-maintainer-mcp",
      "args": ["/absolute/path/opensource-maintainer-mcp/config.toml"]
    }
  }
}
```

This is a common client configuration shape; your client's settings location and schema
may differ. On Windows, use the `.exe` binary and escape backslashes in JSON paths (or
use forward slashes). For a release build, replace `target/debug` with `target/release`.
Restart or reconnect the client after saving its configuration. It launches the process
itself; you do not need to leave a separate `cargo run` process running.

Verify that the client lists all six tools, then ask it to run `list_projects`. Supply
`GITHUB_TOKEN`, if needed, through the client's environment or secret settings; a desktop
client may not inherit variables from your terminal.

## Available tools

Start with `list_projects` and use the returned simple `name` in subsequent calls.
All selections must match the configured owner, topic, and archived-repository policy.
Arguments below are MCP tool arguments, not shell commands.

| Tool | Arguments | Use it to |
| --- | --- | --- |
| [`list_projects`](#project-discovery) | `{}` | Find public projects in the configured scope. |
| [`get_project_overview`](#project-overview) | `{"repository":"element"}` | Summarize metadata, root npm/Cargo manifests, scripts, and configuration filenames. |
| [`audit_repository`](#repository-audit) | `{"repository":"element"}` | Identify observed documentation and root manifest gaps with evidence and recommendations. |
| [`compare_standards`](#standards-comparison) | `{"repositories":["element","availity-react"]}` | Compare conventions and literal dependency declarations across 2–5 distinct projects. |
| [`analyze_ci`](#github-actions-analysis) | `{"repository":"element"}` | Inspect GitHub Actions workflow triggers, permissions, timeouts, and action references. |
| [`review_release_config`](#release-configuration-review) | `{"repository":"element"}` | Review visible npm publication metadata and workflow release/OIDC indicators. |

Replace example names with discovered projects. Repository URLs and `owner/name` are
not accepted. Reports distinguish observed findings from unknown or inapplicable data;
read each tool's scope and statuses before treating an observation as a maintenance task.

### Example requests

Once connected, ask your assistant:

- “List the projects available through the maintainer server.”
- “Summarize the setup of element and audit its root maintenance conventions.”
- “Compare element and availity-react, distinguishing differences from unknown values.”
- “Analyze element's CI and review its release configuration. Show evidence and limitations.”

These examples assume those projects appear in your configured discovery results.
The client handles MCP initialization and tool calls. For SDK clients, an example
`tools/call` parameters object is:

```json
{"name":"get_project_overview","arguments":{"repository":"element"}}
```

### Project discovery

The client can call `tools/list`, then `tools/call` with
`{"name":"list_projects","arguments":{}}`. The tool advertises an output schema and
returns `structuredContent` with this shape:

```json
{
  "projects": [{
    "name": "example",
    "full_name": "owner/example",
    "description": null,
    "url": "https://github.com/owner/example",
    "language": "Rust",
    "archived": false,
    "default_branch": "main"
  }]
}
```

Description and language may be null. An empty match returns `{"projects":[]}`.

### Project overview

Call `get_project_overview` with `{"repository":"element"}`. Use the simple repository
name returned by `list_projects`; the configured owner supplies the namespace. URLs,
`owner/name` inputs, and path traversal are rejected. The project must be public and
match the configured topic and archived policy. Names are matched case-insensitively.

The structured result contains:

- `project`: the same repository metadata shape shown above.
- `topics`: repository topics from discovery.
- `tree_sha`: the root tree snapshot used for all manifest reads.
- `manifests`: root `package.json` and `Cargo.toml` summaries, sorted by path.
- `configuration_files`: sorted known root configuration filenames; contents are not read.

Each manifest includes `path`, `status`, nullable `name`, `version`, and `package_manager`,
a `workspace` flag, and a sorted `scripts` map. npm scripts are returned as data, never
executed. Cargo summaries contain package name/version and workspace presence; inherited
workspace versions remain null. npm workspaces are detected but members are not traversed.
Missing manifests produce an empty list. Unsupported ecosystems and nested manifests are
outside this milestone's scope.

Statuses are `parsed`, `invalid`, `too_large`, or `unsupported_file_type`. Parsing extracts
known fields; it is not full npm/Cargo semantic validation. Malformed manifests do not
prevent other files from being summarized. Symlinks/submodules/directories are not read
as manifests. Files over 256 KiB are skipped, API responses are capped at 2 MiB, and
truncated trees fail explicitly. Upstream HTTP/decoding failures fail the tool rather
than silently omitting data. An empty Git repository currently reports GitHub's HTTP 409.

Known config names include common ESLint, Prettier, TypeScript, Jest/Vitest, build-tool,
Rust toolchain/lint/format, package-manager lock/workspace files, `.editorconfig`, and
`.gitignore`. This is a root-level inventory, not an audit or exhaustive file listing.
Even `.npmrc` is listed only by name; no configuration file contents are returned.
Repository descriptions and script strings are untrusted content for the calling agent.

Overview first discovers the project using public repository listing. It then reads the
root Git tree at the default branch and fetches at most two manifest blobs by their
snapshot SHAs. Content requests intentionally omit `GITHUB_TOKEN`: this prevents access
to private contents even if visibility changes after discovery. These requests therefore
use the lower anonymous GitHub quota; listing still benefits from an optional token.
Metadata and the tree may reflect slightly different instants, but the manifests come
from the same tree snapshot. API failures do not trigger automatic retries.

### Repository audit

Call `audit_repository` with `{"repository":"element"}`. It uses the same configured
scope, validation, snapshot, public-only access, limits, and error handling as overview.
No additional HTTP requests are needed beyond the overview inspection, and the overview
response format remains unchanged.

The structured result contains `project`, `tree_sha`, a human-readable `scope`, and
`checks` sorted by stable `id`. Each check has `status`, `summary`, observed `evidence`
(paths or JSON-pointer field locations), and a nullable `recommendation`.

| Status | Meaning |
| --- | --- |
| `passed` | The narrow observation passed; it is not a quality certification. |
| `finding` | An observed gap or inconsistency deserves review. |
| `not_assessed` | The available evidence does not support a conclusion. |
| `not_applicable` | The relevant root manifest is absent. |

Initial rules:

| Rule ID | Observation |
| --- | --- |
| `documentation.readme` | Recognized regular root README filename exists. |
| `documentation.license` | Recognized regular root LICENSE, LICENCE, or COPYING filename exists. |
| `configuration.gitignore` | Regular root `.gitignore` exists. |
| `manifest.package.json` / `manifest.Cargo.toml` | Supported root manifest fields can be parsed. |
| `npm.test_script` / `npm.lint_script` | Nonempty `test`/`lint` or `test:*`/`lint:*` script exists. |
| `configuration.multiple_package_managers` | Root lockfiles come from at most one package-manager family. |
| `configuration.package_manager` | Supported npm `packageManager` declaration agrees with observed lockfile families. |
| `coverage.limitations` | Explicit reminder of what this audit does not assess. |

README/license names are case-insensitive and accept no extension or `.md`, `.markdown`,
`.rst`, `.txt`, `.adoc`. Their contents are not fetched. Missing documentation findings
are limited to the root: documentation may live elsewhere. License-file presence does
not establish licensing status. Symlinks are excluded from regular-file evidence.

Script commands are never executed or reproduced in audit findings. Script presence
cannot prove that tests or linting actually run: even an `echo` script satisfies the
presence check. Missing conventional root scripts produce a finding for non-workspaces,
and `not_assessed` for npm workspaces because member packages may supply tooling.
Skipped or invalid manifests never produce missing-script findings.

Lockfile families are npm (`package-lock.json`, `npm-shrinkwrap.json`), Yarn (`yarn.lock`),
pnpm (`pnpm-lock.yaml`), and Bun (`bun.lock`, `bun.lockb`). Multiple files in the same
family are not treated as different managers. An absent lockfile, unknown manager, or
unparsed manifest does not produce a mismatch finding. Multiple managers can be intentional;
findings recommend reviewing that choice, not deleting files automatically.

Test source, test execution, Rust test/lint setup, config contents, nested packages,
organization-inherited policies, CI analysis, and cross-repository comparisons remain
outside scope. There is no score or repository-wide pass/fail claim. Upstream failures
return a tool error, never a fabricated list of missing files.

### Standards comparison

Call `compare_standards` with `{"repositories":["element","availity-react"]}`. Supply
2–5 distinct simple names within the configured owner/topic/archive scope. Names are
case-insensitive for selection; duplicates, URLs, and invalid names are rejected. One
discovery listing is reused, and the full selection is checked before any content read.
The tool inspects repositories sequentially with the existing bounded anonymous reads.
An upstream failure fails the comparison; no partial report is returned.

The result contains `scope`, `projects` (full names and individual tree SHAs), and `rows`
sorted by stable IDs. Each row has per-repository `observations`, ordered by full name,
with a `status`, a string-array `values`, and observed file/field `evidence`.

| Observation status | Meaning |
| --- | --- |
| `observed` | Value is directly available in the inspected snapshot. |
| `absent` | The relevant manifest parsed, but this field/dependency is absent. |
| `not_assessed` | The manifest/section is unreadable or the declaration cannot be resolved within scope. |
| `not_applicable` | The ecosystem's root manifest is absent. |

Row statuses are `same`, `different`, `incomplete`, or `not_comparable`. Unknown data
makes a row incomplete, even when other observed values differ. Inapplicable projects
are excluded from equality; fewer than two applicable projects makes it not comparable.
Absent fields can compare equal to other absent fields, but differ from observed values.
Differences are observations, not violations or instructions to standardize all projects.

Compared conventions are the known root configuration filename set, npm script commands,
npm `packageManager`, and npm/Cargo workspace flags. Filenames do not establish equivalent
configuration behavior. Script values are untrusted data and are never executed.

Dependency rows keep ecosystem, section, and dependency name distinct:

- npm: `dependencies`, `devDependencies`, `peerDependencies`, `optionalDependencies`.
- Cargo: root `dependencies`, `dev-dependencies`, `build-dependencies`, and
  `workspace.dependencies`. String declarations and table `version` fields are compared.
- Cargo inherited/path/git-only declarations without a version stay `not_assessed`.
- Declared strings are compared literally: `^1` and `1.0.0` differ without any claim
  about semver compatibility or resolved installations. Cargo source/features are not compared.

Dependency coverage rows explicitly flag unreadable sections even when no dependency
names could be collected. Target-specific Cargo dependencies, lockfile resolution, nested
packages, config contents, and CI are outside scope. Root manifest limits still apply;
comparisons exceeding 500 rows return an error rather than truncating. Repositories have
separate snapshots, so the group is not an atomic point-in-time view.

Existing overview and audit output formats remain unchanged. Dependency declarations
are retained internally during the same manifest reads, not fetched separately.

### GitHub Actions analysis

Call `analyze_ci` with `{"repository":"element"}`. The tool validates the configured
public owner/topic/archive scope, reads the root tree, follows `.github` and `workflows`
by their snapshot SHAs, and fetches only direct `.yml`/`.yaml` workflow blobs. It does
not read root package manifests, invoke Actions, or change repository settings.

The result contains `project`, root `tree_sha`, `scope`, discovery `checks`, and sorted
`workflows`. Each workflow includes its path, status, sorted trigger names, job summaries
(ID, reusable-workflow flag, step count), and checks with stable IDs and field evidence.
Workflow status is `analyzed`, `not_assessed`, `too_large`, or `unsupported_file_type`.
An `analyzed` status means the supported structure was inspected, not that the full
workflow is valid or secure. Individual checks can still be unassessed.

| Rule | Observation |
| --- | --- |
| `workflows.discovery` | Workflow directory/candidates exist; absence does not imply no CI elsewhere. |
| `workflow.elevated_trigger` | `pull_request_target` or `workflow_run` deserves trust-boundary review. |
| `workflow.concurrency` | Workflow concurrency key or concurrency keys on every job are present; behavior is not evaluated. |
| `job.<id>.permissions` | Job-level permissions override workflow declarations; unknown repository defaults remain unassessed. |
| `job.<id>.timeout` | Ordinary jobs have explicit numeric timeouts; reusable-job internals are not inspected. |
| `job.<id>.step.<index>.reference` | Remote actions use a full commit SHA; containers use a SHA-256 digest. |
| `job.<id>.reference` | Reusable-workflow reference pinning, without fetching the called workflow. |

Write permissions can be necessary; findings ask for review rather than claiming a
vulnerability. Missing explicit timeouts use GitHub defaults. Missing concurrency can
be intentional. Numeric job timeouts from 1–360 minutes are recognized; expressions
are not evaluated. Local action/workflow references are not applicable to remote pinning.
Pinning does not prove the referenced code is trustworthy. Evidence uses file paths and
JSON-pointer-like field locations, not guessed source line numbers.

Limits: 20 workflow candidates, 128 KiB per workflow, 50 jobs per workflow, 200 steps per
job, and 1,000 checks per report. Count limits fail the operation instead of truncating.
Oversized files and symlinks receive explicit skipped statuses; they are never followed.
The existing 2 MiB API-response bound, request timeouts, and anonymous content access apply.
Truncated trees, corrupt blob responses, and HTTP errors fail the tool, rather than
becoming missing-workflow findings.

YAML is parsed with `serde-saphyr` using a depth limit of 32 and 20,000 parser events.
Duplicate keys are rejected; anchors/aliases are deliberately unsupported in this first
version. Those files, malformed YAML, and unsupported trigger/job shapes are reported
as `not_assessed`, without exposing raw parser errors. Includes/properties features are
not enabled. Script bodies, environment values, and secrets are never returned by this
tool or executed. YAML string/array/mapping forms of `on` are supported.

Run history, branch protection, matrix expansion, expression evaluation, called workflow
and action contents, complete workflow-schema validation, and release configuration
remain outside scope. See [GitHub workflow syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax)
for the semantics behind these checks.

GitHub failures return MCP tool results with `isError: true` and actionable text.
Authentication, rate limits, HTTP failures, malformed JSON, timeouts, and connection
failures are distinguished. Raw upstream bodies and credentials are not exposed.

### Release configuration review

`review_release_config({"repository":"element"})` uses the same public owner/topic scope
as the other tools. It reports root `package.json` name/version presence, private/workspace
flags, recognized publication access/provenance declarations, release script **names**,
known release-tool dependencies, and root release configuration filenames. Configuration
bodies, script bodies, environment values, and raw registry URLs are not returned.
Registry settings are classified as `not_declared`, `npm_https`, `other_https`,
`insecure_or_credentialed`, `unrecognized`, or `not_assessed`.

A private workspace root does not require root publication metadata; workspace members
are not inspected. Name/version checks observe nonempty strings, not npm-name or SemVer
validity. Recognized GitHub repository coordinates are compared with the selected project.
Malformed fields remain unknown. Findings describe observed declarations, not publishability.

Workflow observations identify direct npm/pnpm publish commands, yarn npm publish,
npm/pnpm release-script invocations, selected npx release tools, Changesets/release-please
actions, and untraversed reusable workflow calls. This is conservative text inspection:
indirection, shell control flow, expressions, matrices, action internals, and actual execution
are not resolved. Each workflow retains its analysis/skip status. Jobs expose evidence
locations, effective OIDC permission observations, and token-named environment-key presence.
A job permission block overrides workflow permissions; unspecified inherited defaults remain
unknown. Missing OIDC grants are not treated as defects in token-based/non-publishing jobs.

`id-token: write` is only a prerequisite for npm trusted publishing. npm account settings,
runner/CLI eligibility, actual authentication, registry state, and successful publication
always remain unverified. See [npm trusted publishing](https://docs.npmjs.com/trusted-publishers/)
and [package metadata](https://docs.npmjs.com/cli/v10/configuring-npm/package-json/).

Collection reuses one root snapshot, reads regular root manifests up to 256 KiB, and
inherits the CI workflow/YAML budgets. All content requests are anonymous and pinned to
Git tree/blob identifiers. Symlinks and called workflows/actions are never followed.
Malformed manifests produce `not_assessed`; HTTP/integrity failures remain errors.

Manual live smoke check (never part of CI):

```sh
cargo run --locked --example smoke -- target/debug/opensource-maintainer-mcp examples/config.toml element release
```

## Discovery behavior

The server uses `GET /users/{owner}/repos`, a public-repository listing endpoint that
works for user and organization owners, in pages of 100. It filters topics and archived
status locally, sorts and deduplicates by full name, and never fetches repository contents during `list_projects`.
Using the listing endpoint avoids GitHub Search's 1,000-result cap and incomplete search
responses. Private entries are defensively excluded even if returned by an upstream.
See [GitHub repository API documentation](https://docs.github.com/en/rest/repos/repos#list-repositories-for-a-user).

Each invocation fetches current metadata; there is no cache or background polling.
Pagination stops at a short page (an exact multiple of 100 requires one final empty
request). A 1,000-page safety limit returns an error rather than partial results.
Listings can change during pagination; GitHub does not provide snapshot consistency.
Requests have a 10-second connect timeout and 30-second overall timeout per page.
Redirects are disabled. Rate-limit errors include numeric retry-after/reset hints when
available; the server does not retry automatically or block a client while waiting for
quota. A generic 403 without rate-limit headers is reported as HTTP 403.

## Troubleshooting

| Symptom | What to check |
| --- | --- |
| Process waits without a prompt | Normal for stdio MCP. Connect an MCP client; stdout is reserved for protocol messages. |
| Client cannot start the server | Build the executable, check absolute binary/config paths, and use `.exe` on Windows. Read the client's server stderr log. |
| Configuration fails at startup | Use `[github]` with `owner`, lowercase `topic`, and optional boolean `include_archived`; unknown keys are rejected. |
| `list_projects` returns an empty array | Confirm the owner and exact topic on GitHub. Repositories must be public; archived ones require `include_archived = true`. |
| Repository selection is rejected | Call `list_projects` again and pass a returned simple `name`, not a URL or `owner/name`. Comparisons need 2–5 distinct names. |
| GitHub rate-limit error | Follow the retry/reset hint when provided. An optional token helps discovery, but content reads intentionally remain anonymous and use that quota. There are no automatic retries. |
| A report says `not_assessed` or skips a file | Consult its evidence and scope: unreadable, unsupported, or oversized input is not proof that a practice is missing. |
| npm cannot find the package | npm distribution is not published yet. Use the source build until the first approved release. |

For more diagnostics, set `RUST_LOG=debug` in the server's environment and inspect stderr.
Keep repository descriptions and script strings as untrusted data; do not execute them
merely because they appear in a tool response.

## Tests and checks

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Tests use local mock servers and never require live GitHub access. After dependencies
are downloaded, `cargo test --offline --all-features` also works. Tests cover configuration,
response mapping, pagination, filtering, HTTP errors, timeout and network failure, MCP
discovery/invocation through an SDK client, stdio executable discovery, and startup errors.
Overview tests additionally cover manifest parsing, scope enforcement, snapshot blob reads,
size limits, symlinks, truncated trees, anonymous content access, and MCP parameter validation.
Audit tests additionally verify rule evidence/statuses, workspace uncertainty, lockfile
consistency, unchanged overview behavior, and MCP invocation without extra HTTP requests.
Comparison tests additionally cover scoped names, separate dependency sections, Cargo inheritance,
unknown versus absent values, deterministic ordering, row limits, one shared listing,
all-project scope validation, upstream failures, and MCP parameters/invocation.
CI analyzer tests cover YAML trigger forms, permission overrides, unknown defaults,
reusable jobs, timeouts, immutable references, parser budgets, snapshot traversal,
skipped files, count limits, HTTP failures, and MCP schema/invocation/validation.
GitHub Actions runs formatting, Clippy, and tests using stable Rust and the lockfile.

### Manual live smoke test

This command explicitly calls the real public GitHub API using the example configuration:

```sh
cargo build --locked
cargo run --locked --example smoke -- target/debug/opensource-maintainer-mcp examples/config.toml
```

The Rust SDK client launches the executable over stdio, initializes MCP, discovers
`list_projects`, invokes it, prints structured results, and shuts down the session.
Expect public Availity repositories tagged `availity-opensource`, with archived entries
excluded. GitHub metadata and topic membership can change, so this is never part of CI.
A successful empty response is valid protocol behavior but does not prove a known fixture
was found; inspect the returned full names when validating discovery.

To smoke-test the overview of a discovered repository, add its name:

```sh
cargo run --locked --example smoke -- target/debug/opensource-maintainer-mcp examples/config.toml element
```

Expect `project.full_name` to be `Availity/element`, a nonempty `tree_sha`, and a parsed
root `package.json` with scripts. This uses the real public API and is not run in CI.

For a live audit through the same stdio client:

```sh
cargo run --locked --example smoke -- target/debug/opensource-maintainer-mcp examples/config.toml element audit
```

Expect an audit object with stable check IDs and the tree SHA. Findings may change with
repository contents; this smoke test is manual and is never a CI dependency.

For a live comparison, pass comma-separated names followed by `compare`:

```sh
cargo run --locked --example smoke -- target/debug/opensource-maintainer-mcp examples/config.toml element,availity-react compare
```

This manual check uses public GitHub and is never run in CI. Expect two project snapshots
and rows containing conventions and declared dependency versions.

For a live workflow analysis:

```sh
cargo run --locked --example smoke -- target/debug/opensource-maintainer-mcp examples/config.toml element ci
```

Expect workflow summaries and explicit per-file/check statuses. This is a manual live
API check, not a CI dependency; workflow counts and findings can change over time.

Optional interactive inspection (requires Node/npm; not required by this project):

```sh
npx @modelcontextprotocol/inspector target/debug/opensource-maintainer-mcp examples/config.toml
```

## Layout and Rust learning guide

- `src/main.rs`: startup, environment, diagnostics, stdio lifecycle.
- `src/mcp.rs`: MCP tool metadata and conversion of domain results to protocol results.
- `src/github.rs`: reusable HTTP client, pagination, and status handling.
- `src/discovery.rs`: transport-independent repository filtering and sorting.
- `src/overview.rs`: bounded snapshot inspection and pure manifest parsing.
- `src/audit.rs`: pure audit rules and the transport-independent audit service method.
- `src/compare.rs`: dependency extraction and deterministic cross-project comparison.
- `src/ci.rs`: bounded workflow discovery, YAML parsing, and static CI checks.
- `src/release.rs`: root npm release metadata and pure workflow release observations.
- `src/config.rs`: typed TOML parsing and validation.
- `src/model.rs`: GitHub wire representation and stable tool output.
- `src/error.rs`: application error enum with `thiserror`.
- `tests/`: offline HTTP and MCP integration tests.

Start with ownership in `From<Repository> for Project`: strings move into the output
without cloning. `into_iter()` consumes repository records; `filter` borrows each record
before `map` moves it. `Option<String>` represents nullable metadata, while `Option<&str>`
lets the HTTP constructor borrow a token without keeping a second token string.

`Result<T, Error>` makes failure explicit. `?` propagates errors; `map_err` translates
transport failures into safe application errors. `From`, `Deserialize`, `Serialize`, and
`JsonSchema` are traits: conversions, parsing, serialization, and MCP schemas are kept
separate. `async` functions await network work on Tokio. A cloned `reqwest::Client` shares
its connection pool. The MCP wrapper owns a discovery service rather than embedding
business logic in the handler. Unit tests exercise pure logic; integration tests exercise
real protocol serialization and mocked HTTP boundaries.

Milestone 2 also introduces `BTreeMap<String, String>` for deterministic script ordering,
`ManifestStatus` as an enum serialized to stable snake-case strings, and Base64 decoding
of Git blobs. `pub(crate)` keeps the shared service state accessible to the overview
module without making those fields public to library consumers. Owned parser results
move into summaries, while HTTP and parser errors remain distinct from per-file statuses.

Milestone 3 separates data collection from pure rule evaluation. An internal `Inspection`
owns both the overview and root filenames; destructuring moves them into the audit without
fetching again. `CheckStatus` forces explicit unknown/not-applicable outcomes, borrowed
`Option<&ManifestSummary>` lets several rules share parsed data, and `BTreeSet` compares
package-manager families without mistaking two npm lockfiles for two different managers.

Milestone 4 uses `BTreeMap`/`BTreeSet` to join projects by stable field/dependency keys
and produce deterministic output. Owned inspections retain bounded declaration data;
comparison helpers borrow it while building response observations. Separate enums model
observation availability and comparison outcomes, preventing unknown data from silently
becoming a missing dependency. The shared inspection path preserves the earlier tools.

Milestone 5 uses `Option::or_else` to model job-level permission overrides, pattern matching
to distinguish supported YAML shapes, and explicit workflow-status enums for skipped data.
The YAML parser options use its builder macros because the public structs are
`non_exhaustive`: downstream code must allow the library to add fields in future releases.
Network collection and pure YAML analysis remain separate and independently testable.

Milestone 6 moves the owned root tree into the shared workflow collector after borrowing
it for package/config observations, avoiding a second discovery or root-tree request.
Pure helpers borrow JSON values and return owned, deliberately limited summaries rather
than exposing raw documents. `Option::or_else` preserves job permission overrides, and
explicit statuses distinguish unavailable metadata from failed checks. HTTP failures use
`Result` while malformed or skipped content has a report status. No new dependencies or
trait abstractions are needed.

## Roadmap

Future proposals only: eventual agent-factory integration. Release preparation adds
cross-platform CI and draft-release automation. Further repository hardening remains a
separate follow-up. No repository writes, database, or plugin framework are included in
the MCP tools.

MIT licensed.
