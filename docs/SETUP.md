# Setup and installation

[Back to README](../README.md)

Build from source today. Downloadable releases and npm distribution are prepared,
but not published yet.

## Build from source

Install stable Rust with [rustup](https://rustup.rs/) if needed. Git is required to
clone the repository; GitHub CLI is optional for contribution.

```sh
git clone https://github.com/jordan-a-young/opensource-maintainer-mcp.git
cd opensource-maintainer-mcp
cargo build --release --locked
cp examples/config.toml config.toml
```

In PowerShell, use `Copy-Item examples/config.toml config.toml` for the copy step.
The executable is `target/release/opensource-maintainer-mcp` (`.exe` on Windows).
For a debug build, use `cargo build --locked` and the executable in `target/debug/`.

## Configuration

Edit your copy of [the example configuration](../examples/config.toml):

```toml
[github]
owner = "availity"
topic = "availity-opensource"
include_archived = false
```

Availity is only an example; any supported GitHub user or organization and topic can
be used. Both owner and topic are required and validated. Use a lowercase topic.
Archived repositories are excluded by default. Unknown keys and invalid values fail
startup with a nonzero exit code.

The first positional argument is the TOML path (default: `config.toml`). Use absolute
binary and configuration paths when launching from an MCP client.

## Connect an MCP client

See the [client guide](CLIENTS.md) for OpenCode, Pi, Codex, and Kiro, including
Windows/WSL and connection verification. Other clients may use this common shape;
their settings locations and schemas can differ:

```json
{
  "mcpServers": {
    "opensource-maintainer": {
      "command": "/absolute/path/opensource-maintainer-mcp/target/release/opensource-maintainer-mcp",
      "args": ["/absolute/path/opensource-maintainer-mcp/config.toml"]
    }
  }
}
```

On Windows, use the `.exe` binary and escape backslashes in JSON paths (or use forward
slashes). Restart or reconnect the client after saving its configuration. It launches
the process itself; you do not need to leave a separate `cargo run` process running.
Verify that it lists all six tools, then ask it to run `list_projects`.

## Optional GitHub token

Anonymous public access works with lower rate limits. If needed, supply `GITHUB_TOKEN`
as a fine-grained token with read-only public repository metadata access through your
environment or the client's secret mechanism. No write permissions are needed.
The server does not read `gh` credentials automatically. Never put tokens in TOML or Git.
A desktop client may not inherit variables from your terminal.

The token helps discovery only: content requests intentionally remain anonymous to
prevent private-content access if a repository changes visibility. Those requests use
the anonymous quota. See [discovery behavior](TOOLS.md#discovery-behavior) for request
limits and error handling.

## Direct startup and diagnostics

To start the process directly for diagnosis:

```sh
cargo run --locked -- config.toml
```

The server waits for MCP JSON-RPC messages on stdin; it is not an interactive CLI.
Stdout contains only protocol messages. Diagnostics go to stderr. Set `RUST_LOG=debug`
in the server environment for additional diagnostics. See [troubleshooting](TROUBLESHOOTING.md)
if startup or tool calls fail.

## Downloadable releases (not published yet)

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
`config.example.toml` to `config.toml`, and edit the owner/topic. Point your MCP client
at the extracted executable and configuration using absolute paths. No Rust or Python
installation is required to run a downloaded binary. The same stdio protocol and
optional `GITHUB_TOKEN` configuration apply.

Archives cover Linux x64 (glibc, built on Ubuntu 22.04), Windows x64, and macOS Intel
and Apple Silicon. Binaries are not code-signed/notarized; checksums provide integrity
checks. See the [release guide](RELEASING.md) for platform limits, validation, the
draft-first flow, and the separate future crates.io decision.

## npm installation (not published yet)

npm is the intended primary installation route; crates.io is optional. After the first
approved npm release, users will be able to run:

```sh
npx --yes @jys/opensource-maintainer-mcp@0.1.0 /absolute/path/config.toml
```

The launcher needs Node 22.14+ and selects the appropriate prebuilt Rust package. No
Rust installation, postinstall script, or GitHub binary download is needed. The packages
are not available on npm yet. See [npm setup and release gates](NPM.md) for the account
checklist, first-package bootstrap limitation, platform support, and trusted staging
flow. See [the npm README](../npm/README.md) for the planned MCP client configuration.
