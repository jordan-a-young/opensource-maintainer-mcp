# Open Source Maintainer MCP

A read-only Rust MCP server distributed through npm. Discover and inspect public GitHub
repositories without installing Rust. The Node launcher selects a version-matched native
package; installation never compiles code or runs a postinstall downloader.

These packages are being prepared and are not yet published. Once published:

```sh
npx --yes @jys/opensource-maintainer-mcp@0.1.0 /absolute/path/config.toml
```

Create the configuration yourself (or copy `config.example.toml` from the installed
launcher package):

```toml
[github]
owner = "availity"
topic = "availity-opensource"
include_archived = false
```

An MCP client configuration would use:

```json
{
  "mcpServers": {
    "opensource-maintainer": {
      "command": "npx",
      "args": ["--yes", "@jys/opensource-maintainer-mcp@0.1.0", "/absolute/path/config.toml"]
    }
  }
}
```

On Windows, clients may require `npx.cmd` and an absolute Windows configuration path.
Set `GITHUB_TOKEN` through your client's environment only if needed for API rate limits.
No write permissions are required. Stdout is reserved for MCP messages; diagnostics use
stderr. The tool does not read your GitHub CLI login.

Requires Node 22.14+ and optional dependencies enabled. Supported binaries: Linux x64
glibc (built on Ubuntu 22.04), Windows x64, macOS Intel and Apple Silicon. Alpine/musl and
other architectures are unsupported. The launcher reports missing or mismatched native
packages instead of silently downloading or compiling a fallback. Do not install with
`--omit=optional`; reinstall with `--include=optional` if the native package is missing.

Start with `list_projects`, then use a returned simple repository name (not a URL or
`owner/name`). The configured owner/topic and archive policy apply to every tool.

| Tool | Arguments | Purpose |
| --- | --- | --- |
| `list_projects` | `{}` | Discover matching public projects. |
| `get_project_overview` | `{"repository":"element"}` | Summarize metadata and root npm/Cargo setup. |
| `audit_repository` | `{"repository":"element"}` | Report observed maintenance gaps with evidence. |
| `compare_standards` | `{"repositories":["element","availity-react"]}` | Compare conventions and declarations across 2–5 projects. |
| `analyze_ci` | `{"repository":"element"}` | Statically inspect GitHub Actions workflows. |
| `review_release_config` | `{"repository":"element"}` | Review npm metadata and release/OIDC workflow indicators. |

Replace the example names with your discovery results. Ask your MCP assistant to list
projects, then summarize or review a selected project. The server never modifies
repositories, runs project scripts, or publishes packages. Findings are bounded static
observations; unknown results and account-side trusted publishing remain unverified.

See the [repository README](https://github.com/jordan-a-young/opensource-maintainer-mcp#readme)
for detailed tool outputs, limits, and troubleshooting. Contributor instructions live in
[AGENTS.md](https://github.com/jordan-a-young/opensource-maintainer-mcp/blob/main/AGENTS.md).
MIT licensed.

For OpenCode, Pi, Codex, and Kiro configuration, see the
[client guide](https://github.com/jordan-a-young/opensource-maintainer-mcp/blob/main/docs/CLIENTS.md).
It uses source builds until npm publication.
