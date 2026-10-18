# Open Source Maintainer MCP

A read-only [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) server
that helps maintainers inspect public GitHub projects from an MCP-compatible assistant.
Discover related repositories, summarize their setup, compare conventions, and review
CI and release settings—all without modifying repositories or running their scripts.

The server runs locally over stdio and reads the public GitHub API. No database or
hosted service is required. **Build from source today; npm packages and downloadable
releases are prepared but not published yet.**

## Setup and run

Install [stable Rust](https://rustup.rs/) and Git, then:

```sh
git clone https://github.com/jordan-a-young/opensource-maintainer-mcp.git
cd opensource-maintainer-mcp
cargo build --release --locked
cp examples/config.toml config.toml
```

In PowerShell, use `Copy-Item examples/config.toml config.toml` for the copy step.
Edit `config.toml` to select a GitHub user or organization and topic:

```toml
[github]
owner = "availity"
topic = "availity-opensource"
include_archived = false
```

Follow the [client setup guide](docs/CLIENTS.md) for OpenCode, Pi, Codex, or Kiro.
Point your client at the absolute paths to `target/release/opensource-maintainer-mcp`
(`.exe` on Windows) and `config.toml`. The client launches the server; you do not need
to run it separately.

`GITHUB_TOKEN` is optional; start without one. See the
[setup guide](docs/SETUP.md) for configuration, authentication, and installation details.

## What can it do?

| Tool | Purpose |
| --- | --- |
| `list_projects` | Find public projects matching your owner, topic, and archive settings. |
| `get_project_overview` | Summarize metadata, root npm/Cargo manifests, scripts, and config filenames. |
| `audit_repository` | Identify observed documentation and root manifest gaps. |
| `compare_standards` | Compare conventions and declared dependencies across 2–5 projects. |
| `analyze_ci` | Inspect GitHub Actions triggers, permissions, timeouts, and action references. |
| `review_release_config` | Review root npm publication metadata and workflow release/OIDC indicators. |

Once connected, try:

- “List the projects available through the maintainer server.”
- “Summarize a discovered project and audit its root maintenance conventions.”
- “Compare two discovered projects, showing evidence and unknown values.”
- “Analyze a project's CI and review its release configuration.”

Start with `list_projects`, then use returned simple repository names—not URLs or
`owner/name`. Reports are bounded static observations, **not security certifications
or proof of release readiness**. Private repositories are never inspected.
See the [tool reference](docs/TOOLS.md) for arguments, output formats, rules, and limits.

## Local companion UI

The private [companion app](companion/README.md) provides a repository dashboard
and an MCP testing panel for all six tools. It runs locally with React, TypeScript,
and the project-local **Vite+** toolchain. Its offline fixture mode lets you explore
reports without GitHub access. See its guide for setup, limits, and checks.

## Documentation

- [Setup and installation](docs/SETUP.md) — configuration, optional tokens, and future binary/npm installs.
- [MCP client setup](docs/CLIENTS.md) — client-specific instructions, Windows/WSL, and connection verification.
- [Tool reference](docs/TOOLS.md) — detailed behavior, evidence, statuses, and inspection boundaries.
- [Troubleshooting](docs/TROUBLESHOOTING.md) — startup, discovery, rate limits, and skipped files.
- [Development guide](docs/DEVELOPMENT.md) — tests, manual smoke checks, architecture, and Rust learning notes.
- [Contributor guidance](AGENTS.md) — project conventions and safety constraints.
- [Release process](docs/RELEASING.md) and [npm publishing](docs/NPM.md) — maintainer workflows and approval gates.

MIT licensed. See [LICENSE](LICENSE).
