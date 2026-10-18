# Development guide

[Back to README](../README.md) · [Contributor guidance](../AGENTS.md)

Use stable Rust and the committed `Cargo.lock`. See [setup](SETUP.md) to build and
configure the server. This guide covers local checks, architecture, and learning notes;
release and distribution checks live in [RELEASING.md](RELEASING.md) and [NPM.md](NPM.md).

- [Tests and checks](#tests-and-checks)
- [Manual live smoke test](#manual-live-smoke-test)
- [Layout and Rust learning guide](#layout-and-rust-learning-guide)
- [Roadmap](#roadmap)

## Tests and checks

Run from the repository root:

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

## Manual live smoke test

These commands explicitly call the real public GitHub API using the example configuration.
They are manual only, never part of CI:

```sh
cargo build --locked
cargo run --locked --example smoke -- target/debug/opensource-maintainer-mcp examples/config.toml
```

The Rust SDK client launches the executable over stdio, initializes MCP, discovers
`list_projects`, invokes it, prints structured results, and shuts down the session.
Expect public Availity repositories tagged `availity-opensource`, with archived entries
excluded. GitHub metadata and topic membership can change. A successful empty response
is valid protocol behavior but does not prove a known fixture was found; inspect the
returned full names when validating discovery.

To smoke-test the overview of a discovered repository, add its name:

```sh
cargo run --locked --example smoke -- target/debug/opensource-maintainer-mcp examples/config.toml element
```

Expect `project.full_name` to be `Availity/element`, a nonempty `tree_sha`, and a parsed
root `package.json` with scripts. On Windows, use the executable's `.exe` suffix.

For a live audit through the same stdio client:

```sh
cargo run --locked --example smoke -- target/debug/opensource-maintainer-mcp examples/config.toml element audit
```

Expect an audit object with stable check IDs and the tree SHA. Findings may change with
repository contents.

For a live comparison, pass comma-separated names followed by `compare`:

```sh
cargo run --locked --example smoke -- target/debug/opensource-maintainer-mcp examples/config.toml element,availity-react compare
```

Expect two project snapshots and rows containing conventions and declared dependency versions.

For a live workflow analysis:

```sh
cargo run --locked --example smoke -- target/debug/opensource-maintainer-mcp examples/config.toml element ci
```

Expect workflow summaries and explicit per-file/check statuses. Workflow counts and
findings can change over time.

For a live release configuration review:

```sh
cargo run --locked --example smoke -- target/debug/opensource-maintainer-mcp examples/config.toml element release
```

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
