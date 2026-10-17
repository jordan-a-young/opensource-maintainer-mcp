# Open Source Maintainer MCP

A Rust MCP server that discovers **public** GitHub repositories by owner and topic.
Milestones 1–3 provide read-only `list_projects`, `get_project_overview`, and
`audit_repository` tools over stdio. Owners can be
GitHub users or organizations; Availity is only the example configuration.

## Setup and run

Install stable Rust with [rustup](https://rustup.rs/) if it is not already installed.
Git is needed for development; GitHub CLI is optional for repository contribution.

```sh
cargo build --locked
cp examples/config.toml config.toml
cargo run --locked -- config.toml
```

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

## MCP client configuration

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

GitHub failures return MCP tool results with `isError: true` and actionable text.
Authentication, rate limits, HTTP failures, malformed JSON, timeouts, and connection
failures are distinguished. Raw upstream bodies and credentials are not exposed.

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

## Roadmap

Future proposals only: standards comparison, CI
analysis, release configuration review, and eventual agent-factory integration. No
repository writes, database, or plugin framework are included in Milestones 1–3.

MIT licensed.
